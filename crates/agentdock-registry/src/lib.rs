use agentdock_core::{
    slugify, AgentIdentity, AgentKind, LifecycleState, ProjectIdentity, Protocol, Service,
    ServiceClassification,
};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

mod remote;
pub use remote::{RemoteApprovalRecord, RemoteRegistryError, RemoteTransportSessionRecord};

const SCHEMA_VERSION: i64 = 5;
const DEFAULT_ORPHAN_AFTER_MS: i64 = 30_000;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("sqlite error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid lifecycle state in registry: {0}")]
    InvalidLifecycle(String),
    #[error("invalid localhost alias: {0}")]
    InvalidHostname(String),
    #[error("pairing challenge not found")]
    PairingChallengeNotFound,
    #[error("pairing challenge expired")]
    PairingChallengeExpired,
    #[error("pairing challenge already consumed")]
    PairingChallengeConsumed,
    #[error("pairing challenge locked after too many failed attempts")]
    PairingChallengeLocked,
    #[error("invalid pairing secret")]
    InvalidPairingSecret,
    #[error("pairing request not found")]
    PairingRequestNotFound,
    #[error("pairing request is not pending")]
    PairingRequestNotPending,
    #[error("paired device not found")]
    PairedDeviceNotFound,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectRecord {
    pub id: String,
    pub identity: ProjectIdentity,
    pub canonical_hostname: Option<String>,
    pub first_seen_ms: i64,
    pub last_seen_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceRecord {
    pub id: String,
    pub project_id: Option<String>,
    pub service: Service,
    pub state: LifecycleState,
    pub first_seen_ms: i64,
    pub last_seen_ms: i64,
    pub missing_since_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSessionRecord {
    pub id: String,
    pub service_id: String,
    pub project_id: Option<String>,
    pub agent: AgentIdentity,
    pub state: LifecycleState,
    pub first_seen_ms: i64,
    pub last_seen_ms: i64,
    pub missing_since_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSessionLogRecord {
    pub seq: i64,
    pub session_id: String,
    pub level: String,
    pub message: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PairingChallengeRecord {
    pub id: String,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
    pub consumed_at_ms: Option<i64>,
    pub failed_attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PairingRequestRecord {
    pub id: String,
    pub challenge_id: String,
    pub device_name: String,
    pub device_public_key: String,
    pub status: String,
    pub created_at_ms: i64,
    pub decided_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PairedDeviceRecord {
    pub id: String,
    pub name: String,
    pub public_key: String,
    pub created_at_ms: i64,
    pub revoked_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteRecord {
    pub hostname: String,
    pub project_id: String,
    pub canonical: bool,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteResolution {
    pub route: RouteRecord,
    pub service: Option<ServiceRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventRecord {
    pub seq: i64,
    pub kind: String,
    pub entity_type: String,
    pub entity_id: String,
    pub payload: serde_json::Value,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ReconcileSummary {
    pub discovered: usize,
    pub resumed: usize,
    pub stale: usize,
    pub orphaned: usize,
    pub active_total: usize,
    pub stale_total: usize,
    pub orphaned_total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RegistryStatus {
    pub schema_version: i64,
    pub projects: usize,
    pub services: usize,
    pub agent_sessions: usize,
    pub agent_session_logs: usize,
    pub pairing_requests: usize,
    pub paired_devices: usize,
    pub remote_transport_sessions: usize,
    pub remote_approvals: usize,
    pub routes: usize,
    pub active: usize,
    pub stale: usize,
    pub orphaned: usize,
    pub events: usize,
}

pub struct Registry {
    conn: Connection,
    orphan_after_ms: i64,
}

impl Registry {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RegistryError> {
        Self::from_connection(Connection::open(path)?)
    }

    pub fn in_memory() -> Result<Self, RegistryError> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self, RegistryError> {
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;

        let mut registry = Self {
            conn,
            orphan_after_ms: DEFAULT_ORPHAN_AFTER_MS,
        };
        registry.migrate()?;
        Ok(registry)
    }

    pub fn set_orphan_after_ms(&mut self, value: i64) {
        self.orphan_after_ms = value.max(0);
    }

    pub fn migrate(&mut self) -> Result<(), RegistryError> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS metadata (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
             );

             CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                canonical_key TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL,
                root TEXT NOT NULL,
                git_root TEXT,
                git_worktree INTEGER NOT NULL,
                first_seen_ms INTEGER NOT NULL,
                last_seen_ms INTEGER NOT NULL
             );

             CREATE TABLE IF NOT EXISTS services (
                id TEXT PRIMARY KEY,
                identity_key TEXT NOT NULL UNIQUE,
                project_id TEXT,
                snapshot_json TEXT NOT NULL,
                state TEXT NOT NULL,
                first_seen_ms INTEGER NOT NULL,
                last_seen_ms INTEGER NOT NULL,
                missing_since_ms INTEGER,
                FOREIGN KEY(project_id) REFERENCES projects(id)
             );

             CREATE INDEX IF NOT EXISTS idx_services_state
             ON services(state);

             CREATE INDEX IF NOT EXISTS idx_services_project_id
             ON services(project_id);

             CREATE TABLE IF NOT EXISTS agent_sessions (
                id TEXT PRIMARY KEY,
                identity_key TEXT NOT NULL UNIQUE,
                service_id TEXT NOT NULL,
                project_id TEXT,
                agent_json TEXT NOT NULL,
                state TEXT NOT NULL,
                first_seen_ms INTEGER NOT NULL,
                last_seen_ms INTEGER NOT NULL,
                missing_since_ms INTEGER,
                FOREIGN KEY(service_id) REFERENCES services(id),
                FOREIGN KEY(project_id) REFERENCES projects(id)
             );

             CREATE INDEX IF NOT EXISTS idx_agent_sessions_state
             ON agent_sessions(state);

             CREATE INDEX IF NOT EXISTS idx_agent_sessions_project_id
             ON agent_sessions(project_id);

             CREATE TABLE IF NOT EXISTS agent_session_logs (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                level TEXT NOT NULL,
                message TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL,
                FOREIGN KEY(session_id) REFERENCES agent_sessions(id)
             );

             CREATE INDEX IF NOT EXISTS idx_agent_session_logs_session_seq
             ON agent_session_logs(session_id, seq);

             CREATE TABLE IF NOT EXISTS pairing_challenges (
                id TEXT PRIMARY KEY,
                secret_hash TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL,
                expires_at_ms INTEGER NOT NULL,
                consumed_at_ms INTEGER,
                failed_attempts INTEGER NOT NULL DEFAULT 0
             );

             CREATE TABLE IF NOT EXISTS pairing_requests (
                id TEXT PRIMARY KEY,
                challenge_id TEXT NOT NULL UNIQUE,
                device_name TEXT NOT NULL,
                device_public_key TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL,
                decided_at_ms INTEGER,
                FOREIGN KEY(challenge_id) REFERENCES pairing_challenges(id)
             );

             CREATE INDEX IF NOT EXISTS idx_pairing_requests_status
             ON pairing_requests(status);

             CREATE TABLE IF NOT EXISTS paired_devices (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                public_key TEXT NOT NULL UNIQUE,
                created_at_ms INTEGER NOT NULL,
                revoked_at_ms INTEGER
             );

             CREATE INDEX IF NOT EXISTS idx_paired_devices_revoked_at
             ON paired_devices(revoked_at_ms);

             CREATE TABLE IF NOT EXISTS routes (
                hostname TEXT PRIMARY KEY COLLATE NOCASE,
                project_id TEXT NOT NULL,
                is_canonical INTEGER NOT NULL DEFAULT 0,
                created_at_ms INTEGER NOT NULL,
                FOREIGN KEY(project_id) REFERENCES projects(id)
             );

             CREATE UNIQUE INDEX IF NOT EXISTS idx_routes_canonical_project
             ON routes(project_id)
             WHERE is_canonical = 1;

             CREATE INDEX IF NOT EXISTS idx_routes_project
             ON routes(project_id);

             CREATE TABLE IF NOT EXISTS events (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                kind TEXT NOT NULL,
                entity_type TEXT NOT NULL,
                entity_id TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL
             );

             CREATE INDEX IF NOT EXISTS idx_events_created_at
             ON events(created_at_ms);",
        )?;

        remote::migrate(&self.conn)?;

        self.conn.execute(
            "INSERT INTO metadata(key, value)
             VALUES('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![SCHEMA_VERSION.to_string()],
        )?;

        Ok(())
    }

    pub fn reconcile_now(
        &mut self,
        services: &[Service],
    ) -> Result<ReconcileSummary, RegistryError> {
        self.reconcile(services, now_ms())
    }

    pub fn reconcile(
        &mut self,
        services: &[Service],
        observed_at_ms: i64,
    ) -> Result<ReconcileSummary, RegistryError> {
        let orphan_after_ms = self.orphan_after_ms;
        let tx = self.conn.transaction()?;
        let mut summary = ReconcileSummary::default();
        let mut seen_service_ids = HashSet::new();
        let mut seen_agent_session_ids = HashSet::new();

        for service in services {
            let project_id = match service.project.as_ref() {
                Some(project) => Some(upsert_project(&tx, project, observed_at_ms)?),
                None => None,
            };

            let identity_key = service_identity_key(service, project_id.as_deref());
            let service_id = stable_id("svc", &identity_key);
            let previous_state: Option<String> = tx
                .query_row(
                    "SELECT state FROM services WHERE id = ?1",
                    params![service_id],
                    |row| row.get(0),
                )
                .optional()?;

            let snapshot_json = serde_json::to_string(service)?;
            tx.execute(
                "INSERT INTO services(
                    id,
                    identity_key,
                    project_id,
                    snapshot_json,
                    state,
                    first_seen_ms,
                    last_seen_ms,
                    missing_since_ms
                 )
                 VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?5, NULL)
                 ON CONFLICT(id) DO UPDATE SET
                    identity_key = excluded.identity_key,
                    project_id = excluded.project_id,
                    snapshot_json = excluded.snapshot_json,
                    state = 'active',
                    last_seen_ms = excluded.last_seen_ms,
                    missing_since_ms = NULL",
                params![
                    service_id,
                    identity_key,
                    project_id,
                    snapshot_json,
                    observed_at_ms
                ],
            )?;

            match previous_state.as_deref() {
                None => {
                    summary.discovered += 1;
                    insert_event(
                        &tx,
                        "service.discovered",
                        &service_id,
                        serde_json::json!({"port": service.port}),
                        observed_at_ms,
                    )?;
                }
                Some("active") => {}
                Some(_) => {
                    summary.resumed += 1;
                    insert_event(
                        &tx,
                        "service.resumed",
                        &service_id,
                        serde_json::json!({"port": service.port}),
                        observed_at_ms,
                    )?;
                }
            }

            if let Some(agent) = service.agent.as_ref() {
                let identity_key = agent_session_identity_key(&service_id, agent);
                let session_id = stable_id("ags", &identity_key);
                let previous_state: Option<String> = tx
                    .query_row(
                        "SELECT state FROM agent_sessions WHERE id = ?1",
                        params![session_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                let agent_json = serde_json::to_string(agent)?;

                tx.execute(
                    "INSERT INTO agent_sessions(
                        id,
                        identity_key,
                        service_id,
                        project_id,
                        agent_json,
                        state,
                        first_seen_ms,
                        last_seen_ms,
                        missing_since_ms
                     )
                     VALUES (?1, ?2, ?3, ?4, ?5, 'active', ?6, ?6, NULL)
                     ON CONFLICT(id) DO UPDATE SET
                        service_id = excluded.service_id,
                        project_id = excluded.project_id,
                        agent_json = excluded.agent_json,
                        state = 'active',
                        last_seen_ms = excluded.last_seen_ms,
                        missing_since_ms = NULL",
                    params![
                        session_id,
                        identity_key,
                        service_id,
                        project_id,
                        agent_json,
                        observed_at_ms
                    ],
                )?;

                match previous_state.as_deref() {
                    None => {
                        insert_agent_session_event(
                            &tx,
                            "agent_session.discovered",
                            &session_id,
                            serde_json::json!({"service_id": service_id}),
                            observed_at_ms,
                        )?;
                        insert_agent_session_log(
                            &tx,
                            &session_id,
                            "info",
                            "Agent session discovered",
                            observed_at_ms,
                        )?;
                    }
                    Some("active") => {}
                    Some(_) => {
                        insert_agent_session_event(
                            &tx,
                            "agent_session.resumed",
                            &session_id,
                            serde_json::json!({"service_id": service_id}),
                            observed_at_ms,
                        )?;
                        insert_agent_session_log(
                            &tx,
                            &session_id,
                            "info",
                            "Agent session resumed",
                            observed_at_ms,
                        )?;
                    }
                }

                seen_agent_session_ids.insert(session_id);
            }

            seen_service_ids.insert(service_id);
        }

        let existing = {
            let mut stmt = tx.prepare(
                "SELECT id, state, missing_since_ms
                 FROM services",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            })?;

            let mut values = Vec::new();
            for row in rows {
                values.push(row?);
            }
            values
        };

        for (service_id, state, missing_since_ms) in existing {
            if seen_service_ids.contains(&service_id) {
                continue;
            }

            match state.as_str() {
                "active" => {
                    tx.execute(
                        "UPDATE services
                         SET state = 'stale', missing_since_ms = ?2
                         WHERE id = ?1",
                        params![service_id, observed_at_ms],
                    )?;
                    summary.stale += 1;
                    insert_event(
                        &tx,
                        "service.stale",
                        &service_id,
                        serde_json::json!({}),
                        observed_at_ms,
                    )?;
                }
                "stale" => {
                    if let Some(missing_since) = missing_since_ms {
                        if observed_at_ms.saturating_sub(missing_since) >= orphan_after_ms {
                            tx.execute(
                                "UPDATE services
                                 SET state = 'orphaned'
                                 WHERE id = ?1",
                                params![service_id],
                            )?;
                            summary.orphaned += 1;
                            insert_event(
                                &tx,
                                "service.orphaned",
                                &service_id,
                                serde_json::json!({"missing_since_ms": missing_since}),
                                observed_at_ms,
                            )?;
                        }
                    }
                }
                "orphaned" => {}
                other => {
                    return Err(RegistryError::InvalidLifecycle(other.to_string()));
                }
            }
        }

        let existing_agent_sessions = {
            let mut stmt = tx.prepare(
                "SELECT id, state, missing_since_ms
                 FROM agent_sessions",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            })?;

            let mut values = Vec::new();
            for row in rows {
                values.push(row?);
            }
            values
        };

        for (session_id, state, missing_since_ms) in existing_agent_sessions {
            if seen_agent_session_ids.contains(&session_id) {
                continue;
            }

            match state.as_str() {
                "active" => {
                    tx.execute(
                        "UPDATE agent_sessions
                         SET state = 'stale', missing_since_ms = ?2
                         WHERE id = ?1",
                        params![session_id, observed_at_ms],
                    )?;
                    insert_agent_session_event(
                        &tx,
                        "agent_session.stale",
                        &session_id,
                        serde_json::json!({}),
                        observed_at_ms,
                    )?;
                    insert_agent_session_log(
                        &tx,
                        &session_id,
                        "info",
                        "Agent session became stale",
                        observed_at_ms,
                    )?;
                }
                "stale" => {
                    if let Some(missing_since) = missing_since_ms {
                        if observed_at_ms.saturating_sub(missing_since) >= orphan_after_ms {
                            tx.execute(
                                "UPDATE agent_sessions
                                 SET state = 'orphaned'
                                 WHERE id = ?1",
                                params![session_id],
                            )?;
                            insert_agent_session_event(
                                &tx,
                                "agent_session.orphaned",
                                &session_id,
                                serde_json::json!({"missing_since_ms": missing_since}),
                                observed_at_ms,
                            )?;
                            insert_agent_session_log(
                                &tx,
                                &session_id,
                                "warn",
                                "Agent session became orphaned",
                                observed_at_ms,
                            )?;
                        }
                    }
                }
                "orphaned" => {}
                other => {
                    return Err(RegistryError::InvalidLifecycle(other.to_string()));
                }
            }
        }

        let (active_total, stale_total, orphaned_total) = state_counts_tx(&tx)?;
        summary.active_total = active_total;
        summary.stale_total = stale_total;
        summary.orphaned_total = orphaned_total;

        tx.commit()?;
        Ok(summary)
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectRecord>, RegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT
                p.id,
                p.name,
                p.root,
                p.git_root,
                p.git_worktree,
                p.first_seen_ms,
                p.last_seen_ms,
                r.hostname
             FROM projects p
             LEFT JOIN routes r
                ON r.project_id = p.id
               AND r.is_canonical = 1
             ORDER BY p.last_seen_ms DESC, p.name ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            let root: String = row.get(2)?;
            let git_root: Option<String> = row.get(3)?;

            Ok(ProjectRecord {
                id: row.get(0)?,
                identity: ProjectIdentity {
                    name: row.get(1)?,
                    root: root.into(),
                    git_root: git_root.map(Into::into),
                    git_worktree: row.get::<_, i64>(4)? != 0,
                },
                canonical_hostname: row.get(7)?,
                first_seen_ms: row.get(5)?,
                last_seen_ms: row.get(6)?,
            })
        })?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    pub fn list_services(&self, include_hidden: bool) -> Result<Vec<ServiceRecord>, RegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT
                id,
                project_id,
                snapshot_json,
                state,
                first_seen_ms,
                last_seen_ms,
                missing_since_ms
             FROM services
             ORDER BY last_seen_ms DESC, id ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, Option<i64>>(6)?,
            ))
        })?;

        let mut records = Vec::new();
        for row in rows {
            let (
                id,
                project_id,
                snapshot_json,
                state_text,
                first_seen_ms,
                last_seen_ms,
                missing_since_ms,
            ) = row?;

            let service: Service = serde_json::from_str(&snapshot_json)?;
            if !include_hidden && !service.is_default_visible() {
                continue;
            }

            let state = LifecycleState::parse(&state_text)
                .ok_or_else(|| RegistryError::InvalidLifecycle(state_text.clone()))?;

            records.push(ServiceRecord {
                id,
                project_id,
                service,
                state,
                first_seen_ms,
                last_seen_ms,
                missing_since_ms,
            });
        }

        Ok(records)
    }

    pub fn list_agent_sessions(&self) -> Result<Vec<AgentSessionRecord>, RegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT
                id,
                service_id,
                project_id,
                agent_json,
                state,
                first_seen_ms,
                last_seen_ms,
                missing_since_ms
             FROM agent_sessions
             ORDER BY last_seen_ms DESC, id ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, Option<i64>>(7)?,
            ))
        })?;

        let mut sessions = Vec::new();
        for row in rows {
            let (
                id,
                service_id,
                project_id,
                agent_json,
                state_text,
                first_seen_ms,
                last_seen_ms,
                missing_since_ms,
            ) = row?;
            let state = LifecycleState::parse(&state_text)
                .ok_or_else(|| RegistryError::InvalidLifecycle(state_text.clone()))?;

            sessions.push(AgentSessionRecord {
                id,
                service_id,
                project_id,
                agent: serde_json::from_str(&agent_json)?,
                state,
                first_seen_ms,
                last_seen_ms,
                missing_since_ms,
            });
        }

        Ok(sessions)
    }

    pub fn list_agent_session_logs(
        &self,
        session_id: &str,
        after_seq: i64,
        limit: usize,
    ) -> Result<Vec<AgentSessionLogRecord>, RegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT seq, session_id, level, message, created_at_ms
             FROM agent_session_logs
             WHERE session_id = ?1
               AND seq > ?2
             ORDER BY seq ASC
             LIMIT ?3",
        )?;

        let rows = stmt.query_map(
            params![session_id, after_seq, limit.clamp(1, 500) as i64],
            |row| {
                Ok(AgentSessionLogRecord {
                    seq: row.get(0)?,
                    session_id: row.get(1)?,
                    level: row.get(2)?,
                    message: row.get(3)?,
                    created_at_ms: row.get(4)?,
                })
            },
        )?;

        let mut logs = Vec::new();
        for row in rows {
            logs.push(row?);
        }
        Ok(logs)
    }

    pub fn create_pairing_challenge(
        &mut self,
        challenge_id: &str,
        secret_hash: &str,
        created_at_ms: i64,
        expires_at_ms: i64,
    ) -> Result<PairingChallengeRecord, RegistryError> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO pairing_challenges(
                id,
                secret_hash,
                created_at_ms,
                expires_at_ms,
                consumed_at_ms,
                failed_attempts
             )
             VALUES (?1, ?2, ?3, ?4, NULL, 0)",
            params![challenge_id, secret_hash, created_at_ms, expires_at_ms],
        )?;
        insert_typed_event(
            &tx,
            "pairing.challenge.created",
            "pairing_challenge",
            challenge_id,
            serde_json::json!({"expires_at_ms": expires_at_ms}),
            created_at_ms,
        )?;
        tx.commit()?;

        Ok(PairingChallengeRecord {
            id: challenge_id.to_string(),
            created_at_ms,
            expires_at_ms,
            consumed_at_ms: None,
            failed_attempts: 0,
        })
    }

    pub fn submit_pairing_request(
        &mut self,
        challenge_id: &str,
        candidate_secret_hash: &str,
        device_name: &str,
        device_public_key: &str,
        requested_at_ms: i64,
    ) -> Result<PairingRequestRecord, RegistryError> {
        let tx = self.conn.transaction()?;
        let challenge = tx
            .query_row(
                "SELECT secret_hash, expires_at_ms, consumed_at_ms, failed_attempts
                 FROM pairing_challenges
                 WHERE id = ?1",
                params![challenge_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?
            .ok_or(RegistryError::PairingChallengeNotFound)?;

        let (secret_hash, expires_at_ms, consumed_at_ms, failed_attempts) = challenge;
        if consumed_at_ms.is_some() {
            return Err(RegistryError::PairingChallengeConsumed);
        }
        if requested_at_ms > expires_at_ms {
            return Err(RegistryError::PairingChallengeExpired);
        }
        if failed_attempts >= 5 {
            return Err(RegistryError::PairingChallengeLocked);
        }

        if !constant_time_eq(secret_hash.as_bytes(), candidate_secret_hash.as_bytes()) {
            let next_attempts = failed_attempts.saturating_add(1);
            tx.execute(
                "UPDATE pairing_challenges
                 SET failed_attempts = ?2,
                     consumed_at_ms = CASE WHEN ?2 >= 5 THEN ?3 ELSE consumed_at_ms END
                 WHERE id = ?1",
                params![challenge_id, next_attempts, requested_at_ms],
            )?;
            tx.commit()?;
            return Err(if next_attempts >= 5 {
                RegistryError::PairingChallengeLocked
            } else {
                RegistryError::InvalidPairingSecret
            });
        }

        let request_id = stable_id(
            "preq",
            &format!("{challenge_id}|{device_public_key}|{requested_at_ms}"),
        );
        tx.execute(
            "UPDATE pairing_challenges
             SET consumed_at_ms = ?2
             WHERE id = ?1",
            params![challenge_id, requested_at_ms],
        )?;
        tx.execute(
            "INSERT INTO pairing_requests(
                id,
                challenge_id,
                device_name,
                device_public_key,
                status,
                created_at_ms,
                decided_at_ms
             )
             VALUES (?1, ?2, ?3, ?4, 'pending', ?5, NULL)",
            params![
                request_id,
                challenge_id,
                device_name,
                device_public_key,
                requested_at_ms
            ],
        )?;
        insert_typed_event(
            &tx,
            "pairing.requested",
            "pairing_request",
            &request_id,
            serde_json::json!({"device_name": device_name}),
            requested_at_ms,
        )?;
        tx.commit()?;

        Ok(PairingRequestRecord {
            id: request_id,
            challenge_id: challenge_id.to_string(),
            device_name: device_name.to_string(),
            device_public_key: device_public_key.to_string(),
            status: "pending".to_string(),
            created_at_ms: requested_at_ms,
            decided_at_ms: None,
        })
    }

    pub fn list_pairing_requests(
        &self,
        pending_only: bool,
    ) -> Result<Vec<PairingRequestRecord>, RegistryError> {
        let sql = if pending_only {
            "SELECT id, challenge_id, device_name, device_public_key, status, created_at_ms, decided_at_ms
             FROM pairing_requests
             WHERE status = 'pending'
             ORDER BY created_at_ms ASC, id ASC"
        } else {
            "SELECT id, challenge_id, device_name, device_public_key, status, created_at_ms, decided_at_ms
             FROM pairing_requests
             ORDER BY created_at_ms DESC, id ASC"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| {
            Ok(PairingRequestRecord {
                id: row.get(0)?,
                challenge_id: row.get(1)?,
                device_name: row.get(2)?,
                device_public_key: row.get(3)?,
                status: row.get(4)?,
                created_at_ms: row.get(5)?,
                decided_at_ms: row.get(6)?,
            })
        })?;

        let mut requests = Vec::new();
        for row in rows {
            requests.push(row?);
        }
        Ok(requests)
    }

    pub fn approve_pairing_request(
        &mut self,
        request_id: &str,
        decided_at_ms: i64,
    ) -> Result<PairedDeviceRecord, RegistryError> {
        let tx = self.conn.transaction()?;
        let request = tx
            .query_row(
                "SELECT device_name, device_public_key, status
                 FROM pairing_requests
                 WHERE id = ?1",
                params![request_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?
            .ok_or(RegistryError::PairingRequestNotFound)?;
        let (device_name, device_public_key, status) = request;
        if status != "pending" {
            return Err(RegistryError::PairingRequestNotPending);
        }

        let device_id = stable_id("dev", &device_public_key);
        tx.execute(
            "INSERT INTO paired_devices(
                id,
                name,
                public_key,
                created_at_ms,
                revoked_at_ms
             )
             VALUES (?1, ?2, ?3, ?4, NULL)
             ON CONFLICT(public_key) DO UPDATE SET
                name = excluded.name,
                created_at_ms = excluded.created_at_ms,
                revoked_at_ms = NULL",
            params![device_id, device_name, device_public_key, decided_at_ms],
        )?;
        tx.execute(
            "UPDATE pairing_requests
             SET status = 'approved', decided_at_ms = ?2
             WHERE id = ?1",
            params![request_id, decided_at_ms],
        )?;
        insert_typed_event(
            &tx,
            "pairing.approved",
            "paired_device",
            &device_id,
            serde_json::json!({"request_id": request_id}),
            decided_at_ms,
        )?;
        tx.commit()?;

        Ok(PairedDeviceRecord {
            id: device_id,
            name: device_name,
            public_key: device_public_key,
            created_at_ms: decided_at_ms,
            revoked_at_ms: None,
        })
    }

    pub fn deny_pairing_request(
        &mut self,
        request_id: &str,
        decided_at_ms: i64,
    ) -> Result<PairingRequestRecord, RegistryError> {
        let tx = self.conn.transaction()?;
        let request = tx
            .query_row(
                "SELECT challenge_id, device_name, device_public_key, status, created_at_ms
                 FROM pairing_requests
                 WHERE id = ?1",
                params![request_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .optional()?
            .ok_or(RegistryError::PairingRequestNotFound)?;
        let (challenge_id, device_name, device_public_key, status, created_at_ms) = request;
        if status != "pending" {
            return Err(RegistryError::PairingRequestNotPending);
        }

        tx.execute(
            "UPDATE pairing_requests
             SET status = 'denied', decided_at_ms = ?2
             WHERE id = ?1",
            params![request_id, decided_at_ms],
        )?;
        insert_typed_event(
            &tx,
            "pairing.denied",
            "pairing_request",
            request_id,
            serde_json::json!({}),
            decided_at_ms,
        )?;
        tx.commit()?;

        Ok(PairingRequestRecord {
            id: request_id.to_string(),
            challenge_id,
            device_name,
            device_public_key,
            status: "denied".to_string(),
            created_at_ms,
            decided_at_ms: Some(decided_at_ms),
        })
    }

    pub fn list_paired_devices(
        &self,
        include_revoked: bool,
    ) -> Result<Vec<PairedDeviceRecord>, RegistryError> {
        let sql = if include_revoked {
            "SELECT id, name, public_key, created_at_ms, revoked_at_ms
             FROM paired_devices
             ORDER BY created_at_ms DESC, id ASC"
        } else {
            "SELECT id, name, public_key, created_at_ms, revoked_at_ms
             FROM paired_devices
             WHERE revoked_at_ms IS NULL
             ORDER BY created_at_ms DESC, id ASC"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| {
            Ok(PairedDeviceRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                public_key: row.get(2)?,
                created_at_ms: row.get(3)?,
                revoked_at_ms: row.get(4)?,
            })
        })?;

        let mut devices = Vec::new();
        for row in rows {
            devices.push(row?);
        }
        Ok(devices)
    }

    pub fn revoke_paired_device(
        &mut self,
        device_id: &str,
        revoked_at_ms: i64,
    ) -> Result<PairedDeviceRecord, RegistryError> {
        let tx = self.conn.transaction()?;
        let device = tx
            .query_row(
                "SELECT name, public_key, created_at_ms, revoked_at_ms
                 FROM paired_devices
                 WHERE id = ?1",
                params![device_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                    ))
                },
            )
            .optional()?
            .ok_or(RegistryError::PairedDeviceNotFound)?;
        let (name, public_key, created_at_ms, previous_revoked_at_ms) = device;
        let effective_revoked_at_ms = previous_revoked_at_ms.unwrap_or(revoked_at_ms);

        tx.execute(
            "UPDATE paired_devices
             SET revoked_at_ms = COALESCE(revoked_at_ms, ?2)
             WHERE id = ?1",
            params![device_id, revoked_at_ms],
        )?;
        tx.execute(
            "UPDATE remote_transport_sessions
             SET closed_at_ms = COALESCE(closed_at_ms, ?2)
             WHERE device_id = ?1
               AND closed_at_ms IS NULL",
            params![device_id, revoked_at_ms],
        )?;
        tx.execute(
            "UPDATE remote_approvals
             SET status = 'revoked',
                 decided_at_ms = COALESCE(decided_at_ms, ?2)
             WHERE device_id = ?1
               AND consumed_at_ms IS NULL
               AND status IN ('pending', 'approved')",
            params![device_id, revoked_at_ms],
        )?;
        if previous_revoked_at_ms.is_none() {
            insert_typed_event(
                &tx,
                "pairing.device.revoked",
                "paired_device",
                device_id,
                serde_json::json!({}),
                revoked_at_ms,
            )?;
        }
        tx.commit()?;

        Ok(PairedDeviceRecord {
            id: device_id.to_string(),
            name,
            public_key,
            created_at_ms,
            revoked_at_ms: Some(effective_revoked_at_ms),
        })
    }

    pub fn list_routes(&self) -> Result<Vec<RouteRecord>, RegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT hostname, project_id, is_canonical, created_at_ms
             FROM routes
             ORDER BY is_canonical DESC, hostname ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(RouteRecord {
                hostname: row.get(0)?,
                project_id: row.get(1)?,
                canonical: row.get::<_, i64>(2)? != 0,
                created_at_ms: row.get(3)?,
            })
        })?;

        let mut routes = Vec::new();
        for row in rows {
            routes.push(row?);
        }
        Ok(routes)
    }

    pub fn add_alias(
        &self,
        project_id: &str,
        hostname: &str,
    ) -> Result<RouteRecord, RegistryError> {
        let hostname = normalize_localhost_name(hostname)?;
        let created_at_ms = now_ms();

        self.conn.execute(
            "INSERT INTO routes(
                hostname,
                project_id,
                is_canonical,
                created_at_ms
             )
             VALUES (?1, ?2, 0, ?3)",
            params![hostname, project_id, created_at_ms],
        )?;

        Ok(RouteRecord {
            hostname,
            project_id: project_id.to_string(),
            canonical: false,
            created_at_ms,
        })
    }

    pub fn resolve_hostname(
        &self,
        hostname: &str,
    ) -> Result<Option<RouteResolution>, RegistryError> {
        let hostname = normalize_localhost_name(hostname)?;

        let route = self
            .conn
            .query_row(
                "SELECT hostname, project_id, is_canonical, created_at_ms
                 FROM routes
                 WHERE hostname = ?1",
                params![hostname],
                |row| {
                    Ok(RouteRecord {
                        hostname: row.get(0)?,
                        project_id: row.get(1)?,
                        canonical: row.get::<_, i64>(2)? != 0,
                        created_at_ms: row.get(3)?,
                    })
                },
            )
            .optional()?;

        let Some(route) = route else {
            return Ok(None);
        };

        let mut candidates = self
            .list_services(true)?
            .into_iter()
            .filter(|record| {
                record.project_id.as_deref() == Some(route.project_id.as_str())
                    && record.state == LifecycleState::Active
                    && record.service.classification == ServiceClassification::Development
            })
            .collect::<Vec<_>>();

        candidates.sort_by(|left, right| {
            right
                .last_seen_ms
                .cmp(&left.last_seen_ms)
                .then_with(|| left.service.port.cmp(&right.service.port))
        });

        Ok(Some(RouteResolution {
            route,
            service: candidates.into_iter().next(),
        }))
    }

    pub fn list_events(
        &self,
        after_seq: i64,
        limit: usize,
    ) -> Result<Vec<EventRecord>, RegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT
                seq,
                kind,
                entity_type,
                entity_id,
                payload_json,
                created_at_ms
             FROM events
             WHERE seq > ?1
             ORDER BY seq ASC
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![after_seq, limit.clamp(1, 1000) as i64], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?;

        let mut events = Vec::new();
        for row in rows {
            let (seq, kind, entity_type, entity_id, payload_json, created_at_ms) = row?;

            events.push(EventRecord {
                seq,
                kind,
                entity_type,
                entity_id,
                payload: serde_json::from_str(&payload_json)?,
                created_at_ms,
            });
        }

        Ok(events)
    }

    pub fn status(&self) -> Result<RegistryStatus, RegistryError> {
        let projects = scalar_count(&self.conn, "SELECT COUNT(*) FROM projects")?;
        let services = scalar_count(&self.conn, "SELECT COUNT(*) FROM services")?;
        let agent_sessions = scalar_count(&self.conn, "SELECT COUNT(*) FROM agent_sessions")?;
        let agent_session_logs =
            scalar_count(&self.conn, "SELECT COUNT(*) FROM agent_session_logs")?;
        let pairing_requests = scalar_count(&self.conn, "SELECT COUNT(*) FROM pairing_requests")?;
        let paired_devices = scalar_count(&self.conn, "SELECT COUNT(*) FROM paired_devices")?;
        let remote_transport_sessions =
            scalar_count(&self.conn, "SELECT COUNT(*) FROM remote_transport_sessions")?;
        let remote_approvals = scalar_count(&self.conn, "SELECT COUNT(*) FROM remote_approvals")?;
        let routes = scalar_count(&self.conn, "SELECT COUNT(*) FROM routes")?;
        let events = scalar_count(&self.conn, "SELECT COUNT(*) FROM events")?;
        let (active, stale, orphaned) = state_counts_conn(&self.conn)?;

        Ok(RegistryStatus {
            schema_version: SCHEMA_VERSION,
            projects,
            services,
            agent_sessions,
            agent_session_logs,
            pairing_requests,
            paired_devices,
            remote_transport_sessions,
            remote_approvals,
            routes,
            active,
            stale,
            orphaned,
            events,
        })
    }
}

fn upsert_project(
    tx: &Transaction<'_>,
    project: &ProjectIdentity,
    observed_at_ms: i64,
) -> Result<String, RegistryError> {
    let canonical_key = project.root.to_string_lossy().to_string();
    let project_id = tx
        .query_row(
            "SELECT id
             FROM projects
             WHERE canonical_key = ?1",
            params![canonical_key],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .unwrap_or_else(|| stable_id("prj", &canonical_key));

    tx.execute(
        "INSERT INTO projects(
            id,
            canonical_key,
            name,
            root,
            git_root,
            git_worktree,
            first_seen_ms,
            last_seen_ms
         )
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            root = excluded.root,
            git_root = excluded.git_root,
            git_worktree = excluded.git_worktree,
            last_seen_ms = excluded.last_seen_ms",
        params![
            project_id,
            canonical_key,
            project.name,
            project.root.to_string_lossy().to_string(),
            project
                .git_root
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            if project.git_worktree { 1 } else { 0 },
            observed_at_ms
        ],
    )?;

    ensure_canonical_route(tx, &project_id, &project.name, observed_at_ms)?;
    Ok(project_id)
}

fn ensure_canonical_route(
    tx: &Transaction<'_>,
    project_id: &str,
    project_name: &str,
    created_at_ms: i64,
) -> Result<String, RegistryError> {
    if let Some(existing) = tx
        .query_row(
            "SELECT hostname
             FROM routes
             WHERE project_id = ?1
               AND is_canonical = 1",
            params![project_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    {
        return Ok(existing);
    }

    let base = {
        let slug = slugify(project_name);
        if slug.is_empty() {
            format!("project-{}", short_hash(project_id))
        } else {
            slug
        }
    };

    let mut attempt = 0_u32;
    let hostname = loop {
        let candidate = if attempt == 0 {
            format!("{base}.localhost")
        } else if attempt == 1 {
            format!("{base}-{}.localhost", short_hash(project_id))
        } else {
            format!("{base}-{}-{attempt}.localhost", short_hash(project_id))
        };

        let owner = tx
            .query_row(
                "SELECT project_id
                 FROM routes
                 WHERE hostname = ?1",
                params![candidate],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        if owner.as_deref().is_none() || owner.as_deref() == Some(project_id) {
            break candidate;
        }

        attempt = attempt.saturating_add(1);
    };

    tx.execute(
        "INSERT INTO routes(
            hostname,
            project_id,
            is_canonical,
            created_at_ms
         )
         VALUES (?1, ?2, 1, ?3)",
        params![hostname, project_id, created_at_ms],
    )?;

    Ok(hostname)
}

fn normalize_localhost_name(hostname: &str) -> Result<String, RegistryError> {
    let normalized = hostname.trim().trim_end_matches('.').to_ascii_lowercase();

    if normalized.is_empty()
        || normalized.contains('/')
        || normalized.contains(':')
        || !normalized.ends_with(".localhost")
    {
        return Err(RegistryError::InvalidHostname(hostname.to_string()));
    }

    Ok(normalized)
}

fn service_identity_key(service: &Service, project_id: Option<&str>) -> String {
    let protocol = match service.protocol {
        Protocol::Tcp => "tcp",
        Protocol::Udp => "udp",
    };

    match project_id {
        Some(project_id) => format!(
            "project={project_id}|protocol={protocol}|framework={:?}|command={}",
            service.framework,
            service.command.as_deref().unwrap_or("unknown")
        ),
        None => format!(
            "protocol={protocol}|command={}|address={}|port={}",
            service.command.as_deref().unwrap_or("unknown"),
            service.bind_address.as_deref().unwrap_or("*"),
            service.port
        ),
    }
}

fn agent_session_identity_key(service_id: &str, agent: &AgentIdentity) -> String {
    let kind = match agent.kind {
        AgentKind::Codex => "codex",
        AgentKind::ClaudeCode => "claude-code",
        AgentKind::Cursor => "cursor",
        AgentKind::Gemini => "gemini",
        AgentKind::Terminal => "terminal",
        AgentKind::Unknown => "unknown",
    };

    match agent.session_id.as_deref() {
        Some(session_id) => format!("kind={kind}|session={session_id}"),
        None => format!("kind={kind}|service={service_id}"),
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    let mut diff = 0_u8;
    for (left, right) in left.iter().zip(right.iter()) {
        diff |= left ^ right;
    }
    diff == 0
}

fn insert_typed_event(
    tx: &Transaction<'_>,
    kind: &str,
    entity_type: &str,
    entity_id: &str,
    payload: serde_json::Value,
    created_at_ms: i64,
) -> Result<(), RegistryError> {
    tx.execute(
        "INSERT INTO events(
            kind,
            entity_type,
            entity_id,
            payload_json,
            created_at_ms
         )
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            kind,
            entity_type,
            entity_id,
            serde_json::to_string(&payload)?,
            created_at_ms
        ],
    )?;

    Ok(())
}

fn insert_agent_session_event(
    tx: &Transaction<'_>,
    kind: &str,
    session_id: &str,
    payload: serde_json::Value,
    created_at_ms: i64,
) -> Result<(), RegistryError> {
    tx.execute(
        "INSERT INTO events(
            kind,
            entity_type,
            entity_id,
            payload_json,
            created_at_ms
         )
         VALUES (?1, 'agent_session', ?2, ?3, ?4)",
        params![
            kind,
            session_id,
            serde_json::to_string(&payload)?,
            created_at_ms
        ],
    )?;

    Ok(())
}

fn insert_agent_session_log(
    tx: &Transaction<'_>,
    session_id: &str,
    level: &str,
    message: &str,
    created_at_ms: i64,
) -> Result<(), RegistryError> {
    tx.execute(
        "INSERT INTO agent_session_logs(
            session_id,
            level,
            message,
            created_at_ms
         )
         VALUES (?1, ?2, ?3, ?4)",
        params![session_id, level, message, created_at_ms],
    )?;

    Ok(())
}

fn insert_event(
    tx: &Transaction<'_>,
    kind: &str,
    service_id: &str,
    payload: serde_json::Value,
    created_at_ms: i64,
) -> Result<(), RegistryError> {
    tx.execute(
        "INSERT INTO events(
            kind,
            entity_type,
            entity_id,
            payload_json,
            created_at_ms
         )
         VALUES (?1, 'service', ?2, ?3, ?4)",
        params![
            kind,
            service_id,
            serde_json::to_string(&payload)?,
            created_at_ms
        ],
    )?;

    Ok(())
}

fn state_counts_tx(tx: &Transaction<'_>) -> Result<(usize, usize, usize), RegistryError> {
    Ok((
        count_state_tx(tx, "active")?,
        count_state_tx(tx, "stale")?,
        count_state_tx(tx, "orphaned")?,
    ))
}

fn count_state_tx(tx: &Transaction<'_>, state: &str) -> Result<usize, RegistryError> {
    Ok(tx.query_row(
        "SELECT COUNT(*)
         FROM services
         WHERE state = ?1",
        params![state],
        |row| row.get::<_, i64>(0),
    )? as usize)
}

fn state_counts_conn(conn: &Connection) -> Result<(usize, usize, usize), RegistryError> {
    Ok((
        count_state_conn(conn, "active")?,
        count_state_conn(conn, "stale")?,
        count_state_conn(conn, "orphaned")?,
    ))
}

fn count_state_conn(conn: &Connection, state: &str) -> Result<usize, RegistryError> {
    Ok(conn.query_row(
        "SELECT COUNT(*)
         FROM services
         WHERE state = ?1",
        params![state],
        |row| row.get::<_, i64>(0),
    )? as usize)
}

fn scalar_count(conn: &Connection, sql: &str) -> Result<usize, RegistryError> {
    Ok(conn.query_row(sql, [], |row| row.get::<_, i64>(0))? as usize)
}

fn stable_id(prefix: &str, value: &str) -> String {
    format!("{prefix}_{:016x}", fnv1a64(value.as_bytes()))
}

fn short_hash(value: &str) -> String {
    format!("{:06x}", fnv1a64(value.as_bytes()) & 0x00ff_ffff)
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;

    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }

    hash
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdock_core::{AgentKind, Framework};
    use std::path::PathBuf;

    fn service(project_name: &str, root: &str, port: u16) -> Service {
        Service {
            pid: Some(42),
            port,
            protocol: Protocol::Tcp,
            bind_address: Some("127.0.0.1".into()),
            command: Some("node".into()),
            command_line: Some("node next dev".into()),
            working_directory: Some(PathBuf::from(root)),
            project: Some(ProjectIdentity {
                name: project_name.into(),
                root: PathBuf::from(root),
                git_root: Some(PathBuf::from(root)),
                git_worktree: false,
            }),
            framework: Framework::NextJs,
            container: None,
            agent: None,
            classification: ServiceClassification::Development,
        }
    }

    fn agent_service(project_name: &str, root: &str, port: u16) -> Service {
        let mut service = service(project_name, root, port);
        service.agent = Some(AgentIdentity {
            kind: AgentKind::Codex,
            session_id: Some("codex:test-session".into()),
        });
        service
    }

    #[test]
    fn durable_agent_session_tracks_lifecycle_and_logs() {
        let mut registry = Registry::in_memory().unwrap();
        registry.set_orphan_after_ms(1_000);

        registry
            .reconcile(
                &[agent_service("storefront", "/tmp/storefront", 3000)],
                1_000,
            )
            .unwrap();

        let first = registry.list_agent_sessions().unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].state, LifecycleState::Active);
        assert_eq!(first[0].agent.kind, AgentKind::Codex);
        let session_id = first[0].id.clone();
        assert_eq!(
            registry
                .list_agent_session_logs(&session_id, 0, 100)
                .unwrap()
                .len(),
            1
        );

        registry.reconcile(&[], 2_000).unwrap();
        assert_eq!(
            registry.list_agent_sessions().unwrap()[0].state,
            LifecycleState::Stale
        );

        registry.reconcile(&[], 3_100).unwrap();
        assert_eq!(
            registry.list_agent_sessions().unwrap()[0].state,
            LifecycleState::Orphaned
        );

        registry
            .reconcile(
                &[agent_service("storefront", "/tmp/storefront", 3010)],
                4_000,
            )
            .unwrap();

        let resumed = registry.list_agent_sessions().unwrap();
        assert_eq!(resumed[0].id, session_id);
        assert_eq!(resumed[0].state, LifecycleState::Active);
        assert_eq!(
            registry
                .list_agent_session_logs(&session_id, 0, 100)
                .unwrap()
                .len(),
            4
        );
    }

    #[test]
    fn pairing_requires_valid_one_time_secret_and_local_approval() {
        let mut registry = Registry::in_memory().unwrap();

        registry
            .create_pairing_challenge("pc_test", "hash-good", 1_000, 6_000)
            .unwrap();

        let request = registry
            .submit_pairing_request(
                "pc_test",
                "hash-good",
                "Rahul iPhone",
                "device-public-key",
                2_000,
            )
            .unwrap();
        assert_eq!(request.status, "pending");

        assert!(matches!(
            registry.submit_pairing_request(
                "pc_test",
                "hash-good",
                "Replay",
                "other-public-key",
                2_100,
            ),
            Err(RegistryError::PairingChallengeConsumed)
        ));

        let device = registry
            .approve_pairing_request(&request.id, 3_000)
            .unwrap();
        assert!(device.revoked_at_ms.is_none());
        assert_eq!(registry.list_paired_devices(false).unwrap().len(), 1);

        let revoked = registry.revoke_paired_device(&device.id, 4_000).unwrap();
        assert_eq!(revoked.revoked_at_ms, Some(4_000));
        assert!(registry.list_paired_devices(false).unwrap().is_empty());
        assert_eq!(registry.list_paired_devices(true).unwrap().len(), 1);
    }

    #[test]
    fn pairing_challenge_locks_after_repeated_invalid_secrets() {
        let mut registry = Registry::in_memory().unwrap();
        registry
            .create_pairing_challenge("pc_lock", "hash-good", 1_000, 10_000)
            .unwrap();

        for attempt in 0..4 {
            assert!(matches!(
                registry.submit_pairing_request(
                    "pc_lock",
                    "hash-bad",
                    "Untrusted",
                    &format!("key-{attempt}"),
                    2_000 + attempt,
                ),
                Err(RegistryError::InvalidPairingSecret)
            ));
        }

        assert!(matches!(
            registry
                .submit_pairing_request("pc_lock", "hash-bad", "Untrusted", "key-final", 2_100,),
            Err(RegistryError::PairingChallengeLocked)
        ));
    }

    #[test]
    fn service_identity_survives_port_change_for_project() {
        let mut registry = Registry::in_memory().unwrap();

        registry
            .reconcile(&[service("storefront", "/tmp/storefront", 3000)], 1_000)
            .unwrap();
        let first = registry.list_services(true).unwrap();

        registry
            .reconcile(&[service("storefront", "/tmp/storefront", 3007)], 2_000)
            .unwrap();
        let second = registry.list_services(true).unwrap();

        assert_eq!(first[0].id, second[0].id);
        assert_eq!(second[0].service.port, 3007);
    }

    #[test]
    fn lifecycle_transitions_and_resumes() {
        let mut registry = Registry::in_memory().unwrap();
        registry.set_orphan_after_ms(1_000);

        registry
            .reconcile(&[service("storefront", "/tmp/storefront", 3000)], 1_000)
            .unwrap();
        registry.reconcile(&[], 2_000).unwrap();

        assert_eq!(
            registry.list_services(true).unwrap()[0].state,
            LifecycleState::Stale
        );

        registry.reconcile(&[], 3_100).unwrap();

        assert_eq!(
            registry.list_services(true).unwrap()[0].state,
            LifecycleState::Orphaned
        );

        registry
            .reconcile(&[service("storefront", "/tmp/storefront", 3010)], 4_000)
            .unwrap();

        assert_eq!(
            registry.list_services(true).unwrap()[0].state,
            LifecycleState::Active
        );
    }

    #[test]
    fn canonical_route_follows_port_change() {
        let mut registry = Registry::in_memory().unwrap();

        registry
            .reconcile(&[service("storefront", "/tmp/storefront", 3000)], 1_000)
            .unwrap();

        let first = registry
            .resolve_hostname("storefront.localhost")
            .unwrap()
            .unwrap();
        assert_eq!(first.service.unwrap().service.port, 3000);

        registry
            .reconcile(&[service("storefront", "/tmp/storefront", 3011)], 2_000)
            .unwrap();

        let second = registry
            .resolve_hostname("storefront.localhost")
            .unwrap()
            .unwrap();
        assert_eq!(second.service.unwrap().service.port, 3011);
    }

    #[test]
    fn route_collision_gets_deterministic_suffix() {
        let mut registry = Registry::in_memory().unwrap();

        registry
            .reconcile(
                &[
                    service("app", "/tmp/team-a/app", 3000),
                    service("app", "/tmp/team-b/app", 4000),
                ],
                1_000,
            )
            .unwrap();

        let projects = registry.list_projects().unwrap();
        let mut hostnames = projects
            .into_iter()
            .filter_map(|project| project.canonical_hostname)
            .collect::<Vec<_>>();

        hostnames.sort();
        hostnames.dedup();

        assert_eq!(hostnames.len(), 2);
        assert!(hostnames.iter().any(|host| host == "app.localhost"));
        assert!(hostnames
            .iter()
            .any(|host| host.starts_with("app-") && host.ends_with(".localhost")));
    }
}
