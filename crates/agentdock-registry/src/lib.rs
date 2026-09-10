use agentdock_core::{LifecycleState, ProjectIdentity, Protocol, Service};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

const SCHEMA_VERSION: i64 = 1;
const DEFAULT_ORPHAN_AFTER_MS: i64 = 30_000;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("sqlite error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid lifecycle state in registry: {0}")]
    InvalidLifecycle(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectRecord {
    pub id: String,
    pub identity: ProjectIdentity,
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
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
        let mut registry = Self { conn, orphan_after_ms: DEFAULT_ORPHAN_AFTER_MS };
        registry.migrate()?;
        Ok(registry)
    }

    pub fn set_orphan_after_ms(&mut self, value: i64) {
        self.orphan_after_ms = value.max(0);
    }

    pub fn migrate(&mut self) -> Result<(), RegistryError> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
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
             CREATE INDEX IF NOT EXISTS idx_services_state ON services(state);
             CREATE INDEX IF NOT EXISTS idx_services_project_id ON services(project_id);
             CREATE TABLE IF NOT EXISTS events (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                kind TEXT NOT NULL,
                entity_type TEXT NOT NULL,
                entity_id TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_events_created_at ON events(created_at_ms);",
        )?;
        self.conn.execute(
            "INSERT INTO metadata(key, value) VALUES('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![SCHEMA_VERSION.to_string()],
        )?;
        Ok(())
    }

    pub fn reconcile_now(&mut self, services: &[Service]) -> Result<ReconcileSummary, RegistryError> {
        self.reconcile(services, now_ms())
    }

    pub fn reconcile(&mut self, services: &[Service], observed_at_ms: i64) -> Result<ReconcileSummary, RegistryError> {
        let orphan_after_ms = self.orphan_after_ms;
        let tx = self.conn.transaction()?;
        let mut summary = ReconcileSummary::default();
        let mut seen_service_ids = HashSet::new();

        for service in services {
            let project_id = match service.project.as_ref() {
                Some(project) => Some(upsert_project(&tx, project, observed_at_ms)?),
                None => None,
            };
            let identity_key = service_identity_key(service, project_id.as_deref());
            let service_id = stable_id("svc", &identity_key);
            let previous_state: Option<String> = tx.query_row(
                "SELECT state FROM services WHERE id = ?1",
                params![service_id],
                |row| row.get(0),
            ).optional()?;

            let snapshot_json = serde_json::to_string(service)?;
            tx.execute(
                "INSERT INTO services(id, identity_key, project_id, snapshot_json, state, first_seen_ms, last_seen_ms, missing_since_ms)
                 VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?5, NULL)
                 ON CONFLICT(id) DO UPDATE SET
                    identity_key = excluded.identity_key,
                    project_id = excluded.project_id,
                    snapshot_json = excluded.snapshot_json,
                    state = 'active',
                    last_seen_ms = excluded.last_seen_ms,
                    missing_since_ms = NULL",
                params![service_id, identity_key, project_id, snapshot_json, observed_at_ms],
            )?;

            match previous_state.as_deref() {
                None => {
                    summary.discovered += 1;
                    insert_event(&tx, "service.discovered", &service_id, serde_json::json!({"port": service.port}), observed_at_ms)?;
                }
                Some("active") => {}
                Some(_) => {
                    summary.resumed += 1;
                    insert_event(&tx, "service.resumed", &service_id, serde_json::json!({"port": service.port}), observed_at_ms)?;
                }
            }
            seen_service_ids.insert(service_id);
        }

        let existing = {
            let mut stmt = tx.prepare("SELECT id, state, missing_since_ms FROM services")?;
            let rows = stmt.query_map([], |row| Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<i64>>(2)?,
            )))?;
            let mut values = Vec::new();
            for row in rows { values.push(row?); }
            values
        };

        for (service_id, state, missing_since_ms) in existing {
            if seen_service_ids.contains(&service_id) { continue; }
            match state.as_str() {
                "active" => {
                    tx.execute(
                        "UPDATE services SET state = 'stale', missing_since_ms = ?2 WHERE id = ?1",
                        params![service_id, observed_at_ms],
                    )?;
                    summary.stale += 1;
                    insert_event(&tx, "service.stale", &service_id, serde_json::json!({}), observed_at_ms)?;
                }
                "stale" => {
                    if let Some(missing_since) = missing_since_ms {
                        if observed_at_ms.saturating_sub(missing_since) >= orphan_after_ms {
                            tx.execute("UPDATE services SET state = 'orphaned' WHERE id = ?1", params![service_id])?;
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
                other => return Err(RegistryError::InvalidLifecycle(other.to_string())),
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
            "SELECT id, name, root, git_root, git_worktree, first_seen_ms, last_seen_ms
             FROM projects ORDER BY last_seen_ms DESC, name ASC"
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
                first_seen_ms: row.get(5)?,
                last_seen_ms: row.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows { out.push(row?); }
        Ok(out)
    }

    pub fn list_services(&self, include_hidden: bool) -> Result<Vec<ServiceRecord>, RegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, snapshot_json, state, first_seen_ms, last_seen_ms, missing_since_ms
             FROM services ORDER BY last_seen_ms DESC, id ASC"
        )?;
        let rows = stmt.query_map([], |row| Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, Option<i64>>(6)?,
        )))?;
        let mut out = Vec::new();
        for row in rows {
            let (id, project_id, snapshot_json, state_text, first_seen_ms, last_seen_ms, missing_since_ms) = row?;
            let service: Service = serde_json::from_str(&snapshot_json)?;
            if !include_hidden && !service.is_default_visible() { continue; }
            let state = LifecycleState::parse(&state_text)
                .ok_or_else(|| RegistryError::InvalidLifecycle(state_text.clone()))?;
            out.push(ServiceRecord { id, project_id, service, state, first_seen_ms, last_seen_ms, missing_since_ms });
        }
        Ok(out)
    }

    pub fn list_events(&self, after_seq: i64, limit: usize) -> Result<Vec<EventRecord>, RegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT seq, kind, entity_type, entity_id, payload_json, created_at_ms
             FROM events WHERE seq > ?1 ORDER BY seq ASC LIMIT ?2"
        )?;
        let rows = stmt.query_map(params![after_seq, limit.clamp(1, 1000) as i64], |row| Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
        )))?;
        let mut out = Vec::new();
        for row in rows {
            let (seq, kind, entity_type, entity_id, payload_json, created_at_ms) = row?;
            out.push(EventRecord {
                seq, kind, entity_type, entity_id,
                payload: serde_json::from_str(&payload_json)?,
                created_at_ms,
            });
        }
        Ok(out)
    }

    pub fn status(&self) -> Result<RegistryStatus, RegistryError> {
        let projects = scalar_count(&self.conn, "SELECT COUNT(*) FROM projects")?;
        let services = scalar_count(&self.conn, "SELECT COUNT(*) FROM services")?;
        let events = scalar_count(&self.conn, "SELECT COUNT(*) FROM events")?;
        let (active, stale, orphaned) = state_counts_conn(&self.conn)?;
        Ok(RegistryStatus { schema_version: SCHEMA_VERSION, projects, services, active, stale, orphaned, events })
    }
}

fn upsert_project(tx: &Transaction<'_>, project: &ProjectIdentity, observed_at_ms: i64) -> Result<String, RegistryError> {
    let canonical_key = project.root.to_string_lossy().to_string();
    let project_id = tx.query_row(
        "SELECT id FROM projects WHERE canonical_key = ?1",
        params![canonical_key],
        |row| row.get::<_, String>(0),
    ).optional()?.unwrap_or_else(|| stable_id("prj", &canonical_key));

    tx.execute(
        "INSERT INTO projects(id, canonical_key, name, root, git_root, git_worktree, first_seen_ms, last_seen_ms)
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
            project.git_root.as_ref().map(|p| p.to_string_lossy().to_string()),
            if project.git_worktree { 1 } else { 0 },
            observed_at_ms
        ],
    )?;
    Ok(project_id)
}

fn service_identity_key(service: &Service, project_id: Option<&str>) -> String {
    let protocol = match service.protocol { Protocol::Tcp => "tcp", Protocol::Udp => "udp" };
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

fn insert_event(
    tx: &Transaction<'_>,
    kind: &str,
    service_id: &str,
    payload: serde_json::Value,
    created_at_ms: i64,
) -> Result<(), RegistryError> {
    tx.execute(
        "INSERT INTO events(kind, entity_type, entity_id, payload_json, created_at_ms)
         VALUES (?1, 'service', ?2, ?3, ?4)",
        params![kind, service_id, serde_json::to_string(&payload)?, created_at_ms],
    )?;
    Ok(())
}

fn state_counts_tx(tx: &Transaction<'_>) -> Result<(usize, usize, usize), RegistryError> {
    Ok((count_state_tx(tx, "active")?, count_state_tx(tx, "stale")?, count_state_tx(tx, "orphaned")?))
}
fn count_state_tx(tx: &Transaction<'_>, state: &str) -> Result<usize, RegistryError> {
    Ok(tx.query_row("SELECT COUNT(*) FROM services WHERE state = ?1", params![state], |r| r.get::<_, i64>(0))? as usize)
}
fn state_counts_conn(conn: &Connection) -> Result<(usize, usize, usize), RegistryError> {
    Ok((count_state_conn(conn, "active")?, count_state_conn(conn, "stale")?, count_state_conn(conn, "orphaned")?))
}
fn count_state_conn(conn: &Connection, state: &str) -> Result<usize, RegistryError> {
    Ok(conn.query_row("SELECT COUNT(*) FROM services WHERE state = ?1", params![state], |r| r.get::<_, i64>(0))? as usize)
}
fn scalar_count(conn: &Connection, sql: &str) -> Result<usize, RegistryError> {
    Ok(conn.query_row(sql, [], |r| r.get::<_, i64>(0))? as usize)
}
fn stable_id(prefix: &str, value: &str) -> String {
    format!("{prefix}_{:016x}", fnv1a64(value.as_bytes()))
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
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis().min(i64::MAX as u128) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdock_core::{Framework, ServiceClassification};
    use std::path::PathBuf;

    fn service(port: u16) -> Service {
        Service {
            pid: Some(42),
            port,
            protocol: Protocol::Tcp,
            bind_address: Some("127.0.0.1".into()),
            command: Some("node".into()),
            command_line: Some("node next dev".into()),
            working_directory: Some(PathBuf::from("/tmp/storefront")),
            project: Some(ProjectIdentity {
                name: "storefront".into(),
                root: PathBuf::from("/tmp/storefront"),
                git_root: Some(PathBuf::from("/tmp/storefront")),
                git_worktree: false,
            }),
            framework: Framework::NextJs,
            container: None,
            agent: None,
            classification: ServiceClassification::Development,
        }
    }

    #[test]
    fn service_identity_survives_port_change_for_project() {
        let mut registry = Registry::in_memory().unwrap();
        registry.reconcile(&[service(3000)], 1_000).unwrap();
        let first = registry.list_services(true).unwrap();
        registry.reconcile(&[service(3007)], 2_000).unwrap();
        let second = registry.list_services(true).unwrap();
        assert_eq!(first[0].id, second[0].id);
        assert_eq!(second[0].service.port, 3007);
    }

    #[test]
    fn lifecycle_transitions_and_resumes() {
        let mut registry = Registry::in_memory().unwrap();
        registry.set_orphan_after_ms(1_000);
        registry.reconcile(&[service(3000)], 1_000).unwrap();
        registry.reconcile(&[], 2_000).unwrap();
        assert_eq!(registry.list_services(true).unwrap()[0].state, LifecycleState::Stale);
        registry.reconcile(&[], 3_100).unwrap();
        assert_eq!(registry.list_services(true).unwrap()[0].state, LifecycleState::Orphaned);
        registry.reconcile(&[service(3010)], 4_000).unwrap();
        assert_eq!(registry.list_services(true).unwrap()[0].state, LifecycleState::Active);
    }
}
