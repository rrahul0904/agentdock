use super::{constant_time_eq, insert_typed_event, Registry, RegistryError};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RemoteRegistryError {
    #[error("sqlite error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("paired device is missing or revoked")]
    DeviceUnavailable,
    #[error("remote authentication challenge not found")]
    AuthChallengeNotFound,
    #[error("remote authentication challenge is unavailable")]
    AuthChallengeUnavailable,
    #[error("remote authentication challenge binding mismatch")]
    AuthChallengeBindingMismatch,
    #[error("remote transport session not found")]
    SessionNotFound,
    #[error("remote transport session is closed or expired")]
    SessionUnavailable,
    #[error("remote transport epoch mismatch")]
    EpochMismatch,
    #[error("remote transport replay detected")]
    ReplayDetected,
    #[error("remote transport sequence overflow")]
    SequenceOverflow,
    #[error("remote approval target agent session not found")]
    TargetSessionNotFound,
    #[error("remote approval not found")]
    ApprovalNotFound,
    #[error("remote approval is unavailable")]
    ApprovalUnavailable,
    #[error("remote approval binding mismatch")]
    ApprovalBindingMismatch,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteAuthChallengeRecord {
    pub id: String,
    pub device_id: String,
    pub server_nonce: String,
    pub issued_at_ms: i64,
    pub expires_at_ms: i64,
    pub consumed_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteTransportSessionRecord {
    pub id: String,
    pub device_id: String,
    pub epoch: String,
    pub last_rx_seq: i64,
    pub last_tx_seq: i64,
    pub authenticated_at_ms: i64,
    pub expires_at_ms: i64,
    pub closed_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteApprovalRecord {
    pub id: String,
    pub device_id: String,
    pub capability: String,
    pub target_session_id: String,
    pub parameter_hash: String,
    pub status: String,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
    pub decided_at_ms: Option<i64>,
    pub consumed_at_ms: Option<i64>,
}

pub(super) fn migrate(conn: &Connection) -> Result<(), RegistryError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS remote_auth_challenges (
            id TEXT PRIMARY KEY,
            device_id TEXT NOT NULL,
            server_nonce TEXT NOT NULL,
            issued_at_ms INTEGER NOT NULL,
            expires_at_ms INTEGER NOT NULL,
            consumed_at_ms INTEGER,
            FOREIGN KEY(device_id) REFERENCES paired_devices(id)
         );

         CREATE INDEX IF NOT EXISTS idx_remote_auth_challenges_device
         ON remote_auth_challenges(device_id, issued_at_ms DESC);

         CREATE TABLE IF NOT EXISTS remote_transport_sessions (
            id TEXT PRIMARY KEY,
            device_id TEXT NOT NULL,
            epoch TEXT NOT NULL,
            last_rx_seq INTEGER NOT NULL DEFAULT 0,
            last_tx_seq INTEGER NOT NULL DEFAULT 0,
            authenticated_at_ms INTEGER NOT NULL,
            expires_at_ms INTEGER NOT NULL,
            closed_at_ms INTEGER,
            FOREIGN KEY(device_id) REFERENCES paired_devices(id)
         );

         CREATE INDEX IF NOT EXISTS idx_remote_transport_sessions_device
         ON remote_transport_sessions(device_id, authenticated_at_ms DESC);

         CREATE UNIQUE INDEX IF NOT EXISTS idx_remote_transport_sessions_open_device
         ON remote_transport_sessions(device_id)
         WHERE closed_at_ms IS NULL;

         CREATE TABLE IF NOT EXISTS remote_approvals (
            id TEXT PRIMARY KEY,
            device_id TEXT NOT NULL,
            capability TEXT NOT NULL,
            target_session_id TEXT NOT NULL,
            parameter_hash TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL,
            expires_at_ms INTEGER NOT NULL,
            decided_at_ms INTEGER,
            consumed_at_ms INTEGER,
            FOREIGN KEY(device_id) REFERENCES paired_devices(id),
            FOREIGN KEY(target_session_id) REFERENCES agent_sessions(id)
         );

         CREATE INDEX IF NOT EXISTS idx_remote_approvals_device_status
         ON remote_approvals(device_id, status, created_at_ms DESC);

         CREATE INDEX IF NOT EXISTS idx_remote_approvals_target
         ON remote_approvals(target_session_id, created_at_ms DESC);",
    )?;

    Ok(())
}

impl Registry {
    pub fn paired_device_public_key(
        &self,
        device_id: &str,
    ) -> Result<String, RemoteRegistryError> {
        self.conn
            .query_row(
                "SELECT public_key
                 FROM paired_devices
                 WHERE id = ?1
                   AND revoked_at_ms IS NULL",
                params![device_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or(RemoteRegistryError::DeviceUnavailable)
    }

    pub fn create_remote_auth_challenge(
        &mut self,
        challenge_id: &str,
        device_id: &str,
        server_nonce: &str,
        issued_at_ms: i64,
        expires_at_ms: i64,
    ) -> Result<RemoteAuthChallengeRecord, RemoteRegistryError> {
        if expires_at_ms <= issued_at_ms {
            return Err(RemoteRegistryError::AuthChallengeUnavailable);
        }

        let tx = self.conn.transaction()?;
        require_active_device(&tx, device_id)?;

        tx.execute(
            "INSERT INTO remote_auth_challenges(
                id,
                device_id,
                server_nonce,
                issued_at_ms,
                expires_at_ms,
                consumed_at_ms
             )
             VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
            params![
                challenge_id,
                device_id,
                server_nonce,
                issued_at_ms,
                expires_at_ms
            ],
        )?;

        insert_typed_event(
            &tx,
            "remote.auth.challenge.created",
            "remote_auth_challenge",
            challenge_id,
            serde_json::json!({"device_id": device_id}),
            issued_at_ms,
        )
        .map_err(remote_registry_event_error)?;

        tx.commit()?;

        Ok(RemoteAuthChallengeRecord {
            id: challenge_id.to_string(),
            device_id: device_id.to_string(),
            server_nonce: server_nonce.to_string(),
            issued_at_ms,
            expires_at_ms,
            consumed_at_ms: None,
        })
    }

    pub fn load_remote_auth_challenge(
        &self,
        challenge_id: &str,
        observed_at_ms: i64,
    ) -> Result<RemoteAuthChallengeRecord, RemoteRegistryError> {
        let challenge = self
            .conn
            .query_row(
                "SELECT id, device_id, server_nonce, issued_at_ms, expires_at_ms, consumed_at_ms
                 FROM remote_auth_challenges
                 WHERE id = ?1",
                params![challenge_id],
                |row| {
                    Ok(RemoteAuthChallengeRecord {
                        id: row.get(0)?,
                        device_id: row.get(1)?,
                        server_nonce: row.get(2)?,
                        issued_at_ms: row.get(3)?,
                        expires_at_ms: row.get(4)?,
                        consumed_at_ms: row.get(5)?,
                    })
                },
            )
            .optional()?
            .ok_or(RemoteRegistryError::AuthChallengeNotFound)?;

        if challenge.consumed_at_ms.is_some() || observed_at_ms > challenge.expires_at_ms {
            return Err(RemoteRegistryError::AuthChallengeUnavailable);
        }

        self.paired_device_public_key(&challenge.device_id)?;
        Ok(challenge)
    }

    pub fn consume_remote_auth_challenge(
        &mut self,
        challenge_id: &str,
        device_id: &str,
        server_nonce: &str,
        consumed_at_ms: i64,
    ) -> Result<RemoteAuthChallengeRecord, RemoteRegistryError> {
        let tx = self.conn.transaction()?;
        require_active_device(&tx, device_id)?;

        let challenge = tx
            .query_row(
                "SELECT id, device_id, server_nonce, issued_at_ms, expires_at_ms, consumed_at_ms
                 FROM remote_auth_challenges
                 WHERE id = ?1",
                params![challenge_id],
                |row| {
                    Ok(RemoteAuthChallengeRecord {
                        id: row.get(0)?,
                        device_id: row.get(1)?,
                        server_nonce: row.get(2)?,
                        issued_at_ms: row.get(3)?,
                        expires_at_ms: row.get(4)?,
                        consumed_at_ms: row.get(5)?,
                    })
                },
            )
            .optional()?
            .ok_or(RemoteRegistryError::AuthChallengeNotFound)?;

        if challenge.device_id != device_id || challenge.server_nonce != server_nonce {
            return Err(RemoteRegistryError::AuthChallengeBindingMismatch);
        }
        if challenge.consumed_at_ms.is_some() || consumed_at_ms > challenge.expires_at_ms {
            return Err(RemoteRegistryError::AuthChallengeUnavailable);
        }

        let updated = tx.execute(
            "UPDATE remote_auth_challenges
             SET consumed_at_ms = ?2
             WHERE id = ?1
               AND consumed_at_ms IS NULL
               AND expires_at_ms >= ?2",
            params![challenge_id, consumed_at_ms],
        )?;
        if updated != 1 {
            return Err(RemoteRegistryError::AuthChallengeUnavailable);
        }

        insert_typed_event(
            &tx,
            "remote.auth.challenge.consumed",
            "remote_auth_challenge",
            challenge_id,
            serde_json::json!({"device_id": device_id}),
            consumed_at_ms,
        )
        .map_err(remote_registry_event_error)?;

        tx.commit()?;

        Ok(RemoteAuthChallengeRecord {
            consumed_at_ms: Some(consumed_at_ms),
            ..challenge
        })
    }

    pub fn open_remote_transport_session(
        &mut self,
        session_id: &str,
        device_id: &str,
        epoch: &str,
        authenticated_at_ms: i64,
        expires_at_ms: i64,
    ) -> Result<RemoteTransportSessionRecord, RemoteRegistryError> {
        if expires_at_ms <= authenticated_at_ms {
            return Err(RemoteRegistryError::SessionUnavailable);
        }

        let tx = self.conn.transaction()?;
        require_active_device(&tx, device_id)?;

        tx.execute(
            "UPDATE remote_transport_sessions
             SET closed_at_ms = COALESCE(closed_at_ms, ?2)
             WHERE device_id = ?1
               AND closed_at_ms IS NULL",
            params![device_id, authenticated_at_ms],
        )?;

        tx.execute(
            "INSERT INTO remote_transport_sessions(
                id,
                device_id,
                epoch,
                last_rx_seq,
                last_tx_seq,
                authenticated_at_ms,
                expires_at_ms,
                closed_at_ms
             )
             VALUES (?1, ?2, ?3, 0, 0, ?4, ?5, NULL)",
            params![
                session_id,
                device_id,
                epoch,
                authenticated_at_ms,
                expires_at_ms
            ],
        )?;

        insert_typed_event(
            &tx,
            "remote.transport.authenticated",
            "remote_transport_session",
            session_id,
            serde_json::json!({"device_id": device_id, "epoch": epoch}),
            authenticated_at_ms,
        )
        .map_err(remote_registry_event_error)?;

        tx.commit()?;

        Ok(RemoteTransportSessionRecord {
            id: session_id.to_string(),
            device_id: device_id.to_string(),
            epoch: epoch.to_string(),
            last_rx_seq: 0,
            last_tx_seq: 0,
            authenticated_at_ms,
            expires_at_ms,
            closed_at_ms: None,
        })
    }

    pub fn accept_remote_sequence(
        &mut self,
        session_id: &str,
        epoch: &str,
        sequence: i64,
        observed_at_ms: i64,
    ) -> Result<RemoteTransportSessionRecord, RemoteRegistryError> {
        if sequence <= 0 {
            return Err(RemoteRegistryError::ReplayDetected);
        }

        let tx = self.conn.transaction()?;
        let mut session = load_transport_session(&tx, session_id)?;

        ensure_session_active(&tx, &mut session, observed_at_ms)?;
        if session.epoch != epoch {
            return Err(RemoteRegistryError::EpochMismatch);
        }
        if sequence <= session.last_rx_seq {
            return Err(RemoteRegistryError::ReplayDetected);
        }

        tx.execute(
            "UPDATE remote_transport_sessions
             SET last_rx_seq = ?2
             WHERE id = ?1",
            params![session_id, sequence],
        )?;
        session.last_rx_seq = sequence;
        tx.commit()?;

        Ok(session)
    }

    pub fn next_remote_tx_sequence(
        &mut self,
        session_id: &str,
        epoch: &str,
        observed_at_ms: i64,
    ) -> Result<i64, RemoteRegistryError> {
        let tx = self.conn.transaction()?;
        let mut session = load_transport_session(&tx, session_id)?;

        ensure_session_active(&tx, &mut session, observed_at_ms)?;
        if session.epoch != epoch {
            return Err(RemoteRegistryError::EpochMismatch);
        }

        let next = session
            .last_tx_seq
            .checked_add(1)
            .ok_or(RemoteRegistryError::SequenceOverflow)?;
        tx.execute(
            "UPDATE remote_transport_sessions
             SET last_tx_seq = ?2
             WHERE id = ?1",
            params![session_id, next],
        )?;
        tx.commit()?;

        Ok(next)
    }

    pub fn close_remote_transport_session(
        &mut self,
        session_id: &str,
        closed_at_ms: i64,
    ) -> Result<RemoteTransportSessionRecord, RemoteRegistryError> {
        let tx = self.conn.transaction()?;
        let mut session = load_transport_session(&tx, session_id)?;
        tx.execute(
            "UPDATE remote_transport_sessions
             SET closed_at_ms = COALESCE(closed_at_ms, ?2)
             WHERE id = ?1",
            params![session_id, closed_at_ms],
        )?;
        session.closed_at_ms = session.closed_at_ms.or(Some(closed_at_ms));
        tx.commit()?;
        Ok(session)
    }

    pub fn create_remote_approval(
        &mut self,
        approval_id: &str,
        device_id: &str,
        capability: &str,
        target_session_id: &str,
        parameter_hash: &str,
        created_at_ms: i64,
        expires_at_ms: i64,
    ) -> Result<RemoteApprovalRecord, RemoteRegistryError> {
        if expires_at_ms <= created_at_ms {
            return Err(RemoteRegistryError::ApprovalUnavailable);
        }

        let tx = self.conn.transaction()?;
        require_active_device(&tx, device_id)?;

        let target_exists = tx
            .query_row(
                "SELECT 1 FROM agent_sessions WHERE id = ?1",
                params![target_session_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !target_exists {
            return Err(RemoteRegistryError::TargetSessionNotFound);
        }

        tx.execute(
            "INSERT INTO remote_approvals(
                id,
                device_id,
                capability,
                target_session_id,
                parameter_hash,
                status,
                created_at_ms,
                expires_at_ms,
                decided_at_ms,
                consumed_at_ms
             )
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?7, NULL, NULL)",
            params![
                approval_id,
                device_id,
                capability,
                target_session_id,
                parameter_hash,
                created_at_ms,
                expires_at_ms
            ],
        )?;

        insert_typed_event(
            &tx,
            "remote.approval.requested",
            "remote_approval",
            approval_id,
            serde_json::json!({
                "device_id": device_id,
                "capability": capability,
                "target_session_id": target_session_id,
                "parameter_hash": parameter_hash
            }),
            created_at_ms,
        )
        .map_err(remote_registry_event_error)?;

        tx.commit()?;

        Ok(RemoteApprovalRecord {
            id: approval_id.to_string(),
            device_id: device_id.to_string(),
            capability: capability.to_string(),
            target_session_id: target_session_id.to_string(),
            parameter_hash: parameter_hash.to_string(),
            status: "pending".to_string(),
            created_at_ms,
            expires_at_ms,
            decided_at_ms: None,
            consumed_at_ms: None,
        })
    }

    pub fn decide_remote_approval(
        &mut self,
        approval_id: &str,
        approved: bool,
        decided_at_ms: i64,
    ) -> Result<RemoteApprovalRecord, RemoteRegistryError> {
        let tx = self.conn.transaction()?;
        let mut approval = load_remote_approval(&tx, approval_id)?;
        if approval.status != "pending"
            || approval.consumed_at_ms.is_some()
            || decided_at_ms > approval.expires_at_ms
        {
            return Err(RemoteRegistryError::ApprovalUnavailable);
        }

        let status = if approved { "approved" } else { "denied" };
        tx.execute(
            "UPDATE remote_approvals
             SET status = ?2,
                 decided_at_ms = ?3
             WHERE id = ?1",
            params![approval_id, status, decided_at_ms],
        )?;

        insert_typed_event(
            &tx,
            if approved {
                "remote.approval.approved"
            } else {
                "remote.approval.denied"
            },
            "remote_approval",
            approval_id,
            serde_json::json!({"device_id": approval.device_id}),
            decided_at_ms,
        )
        .map_err(remote_registry_event_error)?;

        approval.status = status.to_string();
        approval.decided_at_ms = Some(decided_at_ms);
        tx.commit()?;
        Ok(approval)
    }

    pub fn consume_remote_approval(
        &mut self,
        approval_id: &str,
        device_id: &str,
        capability: &str,
        target_session_id: &str,
        parameter_hash: &str,
        consumed_at_ms: i64,
    ) -> Result<RemoteApprovalRecord, RemoteRegistryError> {
        let tx = self.conn.transaction()?;
        require_active_device(&tx, device_id)?;
        let mut approval = load_remote_approval(&tx, approval_id)?;

        if approval.status != "approved"
            || approval.consumed_at_ms.is_some()
            || consumed_at_ms > approval.expires_at_ms
        {
            return Err(RemoteRegistryError::ApprovalUnavailable);
        }

        if approval.device_id != device_id
            || approval.capability != capability
            || approval.target_session_id != target_session_id
            || !constant_time_eq(
                approval.parameter_hash.as_bytes(),
                parameter_hash.as_bytes(),
            )
        {
            return Err(RemoteRegistryError::ApprovalBindingMismatch);
        }

        tx.execute(
            "UPDATE remote_approvals
             SET consumed_at_ms = ?2
             WHERE id = ?1",
            params![approval_id, consumed_at_ms],
        )?;

        insert_typed_event(
            &tx,
            "remote.approval.consumed",
            "remote_approval",
            approval_id,
            serde_json::json!({
                "device_id": device_id,
                "capability": capability,
                "target_session_id": target_session_id
            }),
            consumed_at_ms,
        )
        .map_err(remote_registry_event_error)?;

        approval.consumed_at_ms = Some(consumed_at_ms);
        tx.commit()?;
        Ok(approval)
    }
}

fn require_active_device(tx: &Transaction<'_>, device_id: &str) -> Result<(), RemoteRegistryError> {
    let active = tx
        .query_row(
            "SELECT 1
             FROM paired_devices
             WHERE id = ?1
               AND revoked_at_ms IS NULL",
            params![device_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();

    if active {
        Ok(())
    } else {
        Err(RemoteRegistryError::DeviceUnavailable)
    }
}

fn load_transport_session(
    tx: &Transaction<'_>,
    session_id: &str,
) -> Result<RemoteTransportSessionRecord, RemoteRegistryError> {
    tx.query_row(
        "SELECT
            id,
            device_id,
            epoch,
            last_rx_seq,
            last_tx_seq,
            authenticated_at_ms,
            expires_at_ms,
            closed_at_ms
         FROM remote_transport_sessions
         WHERE id = ?1",
        params![session_id],
        |row| {
            Ok(RemoteTransportSessionRecord {
                id: row.get(0)?,
                device_id: row.get(1)?,
                epoch: row.get(2)?,
                last_rx_seq: row.get(3)?,
                last_tx_seq: row.get(4)?,
                authenticated_at_ms: row.get(5)?,
                expires_at_ms: row.get(6)?,
                closed_at_ms: row.get(7)?,
            })
        },
    )
    .optional()?
    .ok_or(RemoteRegistryError::SessionNotFound)
}

fn ensure_session_active(
    tx: &Transaction<'_>,
    session: &mut RemoteTransportSessionRecord,
    observed_at_ms: i64,
) -> Result<(), RemoteRegistryError> {
    require_active_device(tx, &session.device_id)?;

    if session.closed_at_ms.is_some() {
        return Err(RemoteRegistryError::SessionUnavailable);
    }

    if observed_at_ms > session.expires_at_ms {
        tx.execute(
            "UPDATE remote_transport_sessions
             SET closed_at_ms = COALESCE(closed_at_ms, ?2)
             WHERE id = ?1",
            params![session.id, observed_at_ms],
        )?;
        session.closed_at_ms = Some(observed_at_ms);
        return Err(RemoteRegistryError::SessionUnavailable);
    }

    Ok(())
}

fn load_remote_approval(
    tx: &Transaction<'_>,
    approval_id: &str,
) -> Result<RemoteApprovalRecord, RemoteRegistryError> {
    tx.query_row(
        "SELECT
            id,
            device_id,
            capability,
            target_session_id,
            parameter_hash,
            status,
            created_at_ms,
            expires_at_ms,
            decided_at_ms,
            consumed_at_ms
         FROM remote_approvals
         WHERE id = ?1",
        params![approval_id],
        |row| {
            Ok(RemoteApprovalRecord {
                id: row.get(0)?,
                device_id: row.get(1)?,
                capability: row.get(2)?,
                target_session_id: row.get(3)?,
                parameter_hash: row.get(4)?,
                status: row.get(5)?,
                created_at_ms: row.get(6)?,
                expires_at_ms: row.get(7)?,
                decided_at_ms: row.get(8)?,
                consumed_at_ms: row.get(9)?,
            })
        },
    )
    .optional()?
    .ok_or(RemoteRegistryError::ApprovalNotFound)
}

fn remote_registry_event_error(error: RegistryError) -> RemoteRegistryError {
    match error {
        RegistryError::Sql(error) => RemoteRegistryError::Sql(error),
        other => panic!("remote event persistence returned unexpected registry error: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdock_core::{
        AgentIdentity, AgentKind, Framework, ProjectIdentity, Protocol, Service,
        ServiceClassification,
    };
    use std::path::PathBuf;

    fn pair_device(registry: &mut Registry, suffix: &str) -> super::super::PairedDeviceRecord {
        let challenge_id = format!("pc_{suffix}");
        let public_key = format!("device-public-key-{suffix}");
        registry
            .create_pairing_challenge(&challenge_id, "hash-good", 1_000, 10_000)
            .unwrap();
        let request = registry
            .submit_pairing_request(
                &challenge_id,
                "hash-good",
                "Test device",
                &public_key,
                2_000,
            )
            .unwrap();
        registry
            .approve_pairing_request(&request.id, 3_000)
            .unwrap()
    }

    fn create_agent_session(registry: &mut Registry) -> String {
        let service = Service {
            pid: Some(42),
            port: 3000,
            protocol: Protocol::Tcp,
            bind_address: Some("127.0.0.1".into()),
            command: Some("codex".into()),
            command_line: Some("codex app-server".into()),
            working_directory: Some(PathBuf::from("/tmp/agentdock-remote-test")),
            project: Some(ProjectIdentity {
                name: "agentdock-remote-test".into(),
                root: PathBuf::from("/tmp/agentdock-remote-test"),
                git_root: Some(PathBuf::from("/tmp/agentdock-remote-test")),
                git_worktree: false,
            }),
            framework: Framework::Unknown,
            container: None,
            agent: Some(AgentIdentity {
                kind: AgentKind::Codex,
                session_id: Some("codex:remote-test".into()),
            }),
            classification: ServiceClassification::Development,
        };

        registry.reconcile(&[service], 3_100).unwrap();
        registry.list_agent_sessions().unwrap()[0].id.clone()
    }

    #[test]
    fn transport_rejects_replay_and_old_reconnect_epoch() {
        let mut registry = Registry::in_memory().unwrap();
        let device = pair_device(&mut registry, "transport");

        registry
            .open_remote_transport_session("rts_one", &device.id, "epoch_one", 4_000, 20_000)
            .unwrap();
        registry
            .accept_remote_sequence("rts_one", "epoch_one", 1, 4_100)
            .unwrap();

        assert!(matches!(
            registry.accept_remote_sequence("rts_one", "epoch_one", 1, 4_200),
            Err(RemoteRegistryError::ReplayDetected)
        ));

        registry
            .open_remote_transport_session("rts_two", &device.id, "epoch_two", 5_000, 20_000)
            .unwrap();

        assert!(matches!(
            registry.accept_remote_sequence("rts_one", "epoch_one", 2, 5_100),
            Err(RemoteRegistryError::SessionUnavailable)
        ));
        assert!(matches!(
            registry.accept_remote_sequence("rts_two", "epoch_one", 1, 5_100),
            Err(RemoteRegistryError::EpochMismatch)
        ));

        assert_eq!(
            registry
                .next_remote_tx_sequence("rts_two", "epoch_two", 5_200)
                .unwrap(),
            1
        );
        assert_eq!(
            registry
                .next_remote_tx_sequence("rts_two", "epoch_two", 5_300)
                .unwrap(),
            2
        );
    }

    #[test]
    fn approval_is_parameter_bound_and_one_shot() {
        let mut registry = Registry::in_memory().unwrap();
        let device = pair_device(&mut registry, "approval");
        let agent_session_id = create_agent_session(&mut registry);
        let expected_hash = "a".repeat(64);

        registry
            .create_remote_approval(
                "rap_test",
                &device.id,
                "agent_input",
                &agent_session_id,
                &expected_hash,
                4_000,
                10_000,
            )
            .unwrap();
        registry
            .decide_remote_approval("rap_test", true, 4_100)
            .unwrap();

        assert!(matches!(
            registry.consume_remote_approval(
                "rap_test",
                &device.id,
                "agent_input",
                &agent_session_id,
                &"b".repeat(64),
                4_200,
            ),
            Err(RemoteRegistryError::ApprovalBindingMismatch)
        ));

        let consumed = registry
            .consume_remote_approval(
                "rap_test",
                &device.id,
                "agent_input",
                &agent_session_id,
                &expected_hash,
                4_300,
            )
            .unwrap();
        assert_eq!(consumed.consumed_at_ms, Some(4_300));

        assert!(matches!(
            registry.consume_remote_approval(
                "rap_test",
                &device.id,
                "agent_input",
                &agent_session_id,
                &expected_hash,
                4_400,
            ),
            Err(RemoteRegistryError::ApprovalUnavailable)
        ));
    }

    #[test]
    fn device_revocation_invalidates_transport_and_pending_execution() {
        let mut registry = Registry::in_memory().unwrap();
        let device = pair_device(&mut registry, "revoke");
        let agent_session_id = create_agent_session(&mut registry);
        let expected_hash = "c".repeat(64);

        registry
            .open_remote_transport_session("rts_revoke", &device.id, "epoch_revoke", 4_000, 20_000)
            .unwrap();
        registry
            .create_remote_approval(
                "rap_revoke",
                &device.id,
                "agent_input",
                &agent_session_id,
                &expected_hash,
                4_100,
                10_000,
            )
            .unwrap();
        registry
            .decide_remote_approval("rap_revoke", true, 4_200)
            .unwrap();

        registry.revoke_paired_device(&device.id, 4_300).unwrap();

        assert!(matches!(
            registry.accept_remote_sequence("rts_revoke", "epoch_revoke", 1, 4_400),
            Err(RemoteRegistryError::DeviceUnavailable)
                | Err(RemoteRegistryError::SessionUnavailable)
        ));
        assert!(matches!(
            registry.consume_remote_approval(
                "rap_revoke",
                &device.id,
                "agent_input",
                &agent_session_id,
                &expected_hash,
                4_500,
            ),
            Err(RemoteRegistryError::DeviceUnavailable)
                | Err(RemoteRegistryError::ApprovalUnavailable)
        ));
    }
}
