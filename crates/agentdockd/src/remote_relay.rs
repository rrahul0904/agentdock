use crate::remote_auth;
use agentdock_core::remote::{
    RemoteAuthChallenge, RemoteAuthProof, RemoteCapability, RemoteTransportEnvelope,
    REMOTE_PROTOCOL_VERSION,
};
use agentdock_registry::{now_ms, Registry, RemoteRegistryError};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tungstenite::client::{connect_with_config, ClientRequestBuilder};
use tungstenite::http::Uri;
use tungstenite::Message;

const AUTH_CHALLENGE_TTL_MS: i64 = 60_000;
const TRANSPORT_SESSION_TTL_MS: i64 = 30 * 60 * 1_000;
const RESPONSE_TTL_MS: i64 = 60_000;
const RECONNECT_DELAY: Duration = Duration::from_secs(2);
const RELAY_SUBPROTOCOL: &str = "agentdock.v1";

#[derive(Debug, Clone)]
pub struct RelayConfig {
    pub url: String,
    pub token: String,
}

impl RelayConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !self.url.starts_with("wss://") {
            return Err("remote relay URL must use wss://".to_string());
        }
        if self.token.trim().len() < 32 || self.token.contains('\r') || self.token.contains('\n') {
            return Err(
                "remote relay token must contain at least 32 characters and no line breaks"
                    .to_string(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct AuthenticatedConnection {
    device_id: String,
    transport_session_id: String,
    epoch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RelayClientFrame {
    AuthBegin {
        device_id: String,
    },
    AuthProof {
        proof: RemoteAuthProof,
    },
    SessionInventory {
        envelope: RemoteTransportEnvelope,
    },
    SessionLogs {
        envelope: RemoteTransportEnvelope,
        session_id: String,
        after: i64,
        limit: usize,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RelayServerFrame {
    RelayReady {
        protocol_version: u16,
    },
    AuthChallenge {
        challenge: RemoteAuthChallenge,
    },
    Authenticated {
        device_id: String,
        transport_session_id: String,
        epoch: String,
        expires_at_ms: i64,
    },
    SessionInventory {
        envelope: RemoteTransportEnvelope,
        sessions: serde_json::Value,
    },
    SessionLogs {
        envelope: RemoteTransportEnvelope,
        session_id: String,
        logs: serde_json::Value,
    },
    Error {
        code: String,
        message: String,
        message_id: Option<String>,
    },
}

pub fn spawn(config: RelayConfig, registry: Arc<Mutex<Registry>>) -> Result<(), String> {
    config.validate()?;

    thread::Builder::new()
        .name("agentdock-remote-relay".to_string())
        .spawn(move || loop {
            if let Err(error) = run_connection(&config, &registry) {
                eprintln!("AgentDock remote relay disconnected: {error}");
            }
            thread::sleep(RECONNECT_DELAY);
        })
        .map_err(|error| format!("failed to start remote relay thread: {error}"))?;

    Ok(())
}

fn run_connection(config: &RelayConfig, registry: &Arc<Mutex<Registry>>) -> Result<(), String> {
    let uri: Uri = config
        .url
        .parse()
        .map_err(|error| format!("invalid remote relay URL: {error}"))?;
    let request = ClientRequestBuilder::new(uri)
        .with_header("Authorization", format!("Bearer {}", config.token))
        .with_header("X-AgentDock-Protocol", REMOTE_PROTOCOL_VERSION.to_string())
        .with_sub_protocol(RELAY_SUBPROTOCOL);

    let (mut socket, _) = connect_with_config(request, None, 0)
        .map_err(|error| format!("remote relay connect failed: {error}"))?;

    send_frame(
        &mut socket,
        &RelayServerFrame::RelayReady {
            protocol_version: REMOTE_PROTOCOL_VERSION,
        },
    )?;

    let mut authenticated = None;

    let result = loop {
        let message = match socket.read() {
            Ok(message) => message,
            Err(error) => break Err(format!("remote relay read failed: {error}")),
        };

        match message {
            Message::Text(text) => {
                let response = match serde_json::from_str::<RelayClientFrame>(&text) {
                    Ok(frame) => {
                        let mut registry = match registry.lock() {
                            Ok(registry) => registry,
                            Err(_) => break Err("registry mutex poisoned".to_string()),
                        };
                        handle_client_frame(&mut registry, &mut authenticated, frame, now_ms())
                    }
                    Err(error) => RelayServerFrame::Error {
                        code: "invalid_frame".to_string(),
                        message: error.to_string(),
                        message_id: None,
                    },
                };

                if let Err(error) = send_frame(&mut socket, &response) {
                    break Err(error);
                }
            }
            Message::Close(_) => break Ok(()),
            Message::Ping(payload) => {
                if let Err(error) = socket.send(Message::Pong(payload)) {
                    break Err(format!("remote relay pong failed: {error}"));
                }
            }
            Message::Pong(_) | Message::Binary(_) | Message::Frame(_) => {}
        }
    };

    close_authenticated_session(registry, authenticated.take());
    result
}

fn send_frame<S>(
    socket: &mut tungstenite::WebSocket<S>,
    frame: &RelayServerFrame,
) -> Result<(), String>
where
    S: std::io::Read + std::io::Write,
{
    let payload = serde_json::to_string(frame)
        .map_err(|error| format!("relay frame encode failed: {error}"))?;
    socket
        .send(Message::Text(payload.into()))
        .map_err(|error| format!("relay frame send failed: {error}"))
}

fn close_authenticated_session(
    registry: &Arc<Mutex<Registry>>,
    authenticated: Option<AuthenticatedConnection>,
) {
    let Some(authenticated) = authenticated else {
        return;
    };
    if let Ok(mut registry) = registry.lock() {
        let _ =
            registry.close_remote_transport_session(&authenticated.transport_session_id, now_ms());
    }
}

fn handle_client_frame(
    registry: &mut Registry,
    authenticated: &mut Option<AuthenticatedConnection>,
    frame: RelayClientFrame,
    observed_at_ms: i64,
) -> RelayServerFrame {
    match frame {
        RelayClientFrame::AuthBegin { device_id } => {
            if device_id.trim().is_empty() || device_id.len() > 128 {
                return relay_error("invalid_device_id", "invalid device ID", None);
            }

            let public_key = match registry.paired_device_public_key(&device_id) {
                Ok(public_key) => public_key,
                Err(error) => return registry_error_frame(error, None),
            };
            if !remote_auth::is_supported_device_public_key(&public_key) {
                return relay_error(
                    "unsupported_device_key",
                    "paired device does not have a supported Ed25519 public key",
                    None,
                );
            }

            let challenge = RemoteAuthChallenge {
                protocol_version: REMOTE_PROTOCOL_VERSION,
                device_id: device_id.clone(),
                challenge_id: format!("rac_{}", random_hex(16)),
                server_nonce: random_hex(32),
                issued_at_ms: observed_at_ms,
                expires_at_ms: observed_at_ms.saturating_add(AUTH_CHALLENGE_TTL_MS),
            };

            match registry.create_remote_auth_challenge(
                &challenge.challenge_id,
                &challenge.device_id,
                &challenge.server_nonce,
                challenge.issued_at_ms,
                challenge.expires_at_ms,
            ) {
                Ok(_) => RelayServerFrame::AuthChallenge { challenge },
                Err(error) => registry_error_frame(error, None),
            }
        }
        RelayClientFrame::AuthProof { proof } => {
            let challenge_record =
                match registry.load_remote_auth_challenge(&proof.challenge_id, observed_at_ms) {
                    Ok(challenge) => challenge,
                    Err(error) => return registry_error_frame(error, None),
                };
            let challenge = RemoteAuthChallenge {
                protocol_version: REMOTE_PROTOCOL_VERSION,
                device_id: challenge_record.device_id.clone(),
                challenge_id: challenge_record.id.clone(),
                server_nonce: challenge_record.server_nonce.clone(),
                issued_at_ms: challenge_record.issued_at_ms,
                expires_at_ms: challenge_record.expires_at_ms,
            };
            let public_key = match registry.paired_device_public_key(&challenge.device_id) {
                Ok(public_key) => public_key,
                Err(error) => return registry_error_frame(error, None),
            };

            if let Err(error) = remote_auth::verify_device_proof(&public_key, &challenge, &proof) {
                return relay_error("auth_proof_rejected", &error.to_string(), None);
            }

            if let Err(error) = registry.consume_remote_auth_challenge(
                &challenge.challenge_id,
                &challenge.device_id,
                &challenge.server_nonce,
                observed_at_ms,
            ) {
                return registry_error_frame(error, None);
            }

            if let Some(previous) = authenticated.take() {
                let _ = registry
                    .close_remote_transport_session(&previous.transport_session_id, observed_at_ms);
            }

            let transport_session_id = format!("rts_{}", random_hex(16));
            let epoch = format!("epoch_{}", random_hex(16));
            let expires_at_ms = observed_at_ms.saturating_add(TRANSPORT_SESSION_TTL_MS);
            let session = match registry.open_remote_transport_session(
                &transport_session_id,
                &challenge.device_id,
                &epoch,
                observed_at_ms,
                expires_at_ms,
            ) {
                Ok(session) => session,
                Err(error) => return registry_error_frame(error, None),
            };

            *authenticated = Some(AuthenticatedConnection {
                device_id: session.device_id.clone(),
                transport_session_id: session.id.clone(),
                epoch: session.epoch.clone(),
            });

            RelayServerFrame::Authenticated {
                device_id: session.device_id,
                transport_session_id: session.id,
                epoch: session.epoch,
                expires_at_ms: session.expires_at_ms,
            }
        }
        RelayClientFrame::SessionInventory { envelope } => {
            let state = match authorize_read_frame(
                registry,
                authenticated.as_ref(),
                &envelope,
                RemoteCapability::SessionInventory,
                observed_at_ms,
            ) {
                Ok(state) => state,
                Err(frame) => return frame,
            };

            let sessions = match registry.list_agent_sessions() {
                Ok(sessions) => sessions,
                Err(error) => {
                    return relay_error(
                        "registry_error",
                        &error.to_string(),
                        Some(envelope.message_id),
                    )
                }
            };
            let response_envelope = match response_envelope(
                registry,
                state,
                RemoteCapability::SessionInventory,
                None,
                observed_at_ms,
            ) {
                Ok(envelope) => envelope,
                Err(frame) => return frame,
            };

            RelayServerFrame::SessionInventory {
                envelope: response_envelope,
                sessions: serde_json::to_value(sessions).unwrap_or_else(|_| serde_json::json!([])),
            }
        }
        RelayClientFrame::SessionLogs {
            envelope,
            session_id,
            after,
            limit,
        } => {
            let state = match authorize_read_frame(
                registry,
                authenticated.as_ref(),
                &envelope,
                RemoteCapability::SessionLogs,
                observed_at_ms,
            ) {
                Ok(state) => state,
                Err(frame) => return frame,
            };
            if envelope.target_agent_session_id.as_deref() != Some(session_id.as_str()) {
                return relay_error(
                    "target_binding_mismatch",
                    "log request target does not match the signed transport envelope",
                    Some(envelope.message_id),
                );
            }

            let logs = match registry.list_agent_session_logs(
                &session_id,
                after.max(0),
                limit.clamp(1, 200),
            ) {
                Ok(logs) => logs,
                Err(error) => {
                    return relay_error(
                        "registry_error",
                        &error.to_string(),
                        Some(envelope.message_id),
                    )
                }
            };
            let response_envelope = match response_envelope(
                registry,
                state,
                RemoteCapability::SessionLogs,
                Some(session_id.clone()),
                observed_at_ms,
            ) {
                Ok(envelope) => envelope,
                Err(frame) => return frame,
            };

            RelayServerFrame::SessionLogs {
                envelope: response_envelope,
                session_id,
                logs: serde_json::to_value(logs).unwrap_or_else(|_| serde_json::json!([])),
            }
        }
    }
}

fn authorize_read_frame<'a>(
    registry: &mut Registry,
    authenticated: Option<&'a AuthenticatedConnection>,
    envelope: &RemoteTransportEnvelope,
    required_capability: RemoteCapability,
    observed_at_ms: i64,
) -> Result<&'a AuthenticatedConnection, RelayServerFrame> {
    let Some(state) = authenticated else {
        return Err(relay_error(
            "authentication_required",
            "remote transport authentication is required",
            Some(envelope.message_id.clone()),
        ));
    };

    if !envelope.is_well_formed()
        || envelope.expires_at_ms < observed_at_ms
        || envelope.device_id != state.device_id
        || envelope.transport_session_id != state.transport_session_id
        || envelope.epoch != state.epoch
    {
        return Err(relay_error(
            "invalid_envelope",
            "remote transport envelope binding is invalid or expired",
            Some(envelope.message_id.clone()),
        ));
    }

    if envelope.capability != required_capability {
        return Err(relay_error(
            "capability_not_allowed",
            "relay request capability does not match the read-only operation",
            Some(envelope.message_id.clone()),
        ));
    }

    if !matches!(
        envelope.capability,
        RemoteCapability::SessionInventory | RemoteCapability::SessionLogs
    ) {
        return Err(relay_error(
            "mutating_remote_capability_disabled",
            "mutating remote capabilities remain disabled",
            Some(envelope.message_id.clone()),
        ));
    }

    let sequence = match i64::try_from(envelope.sequence) {
        Ok(sequence) => sequence,
        Err(_) => {
            return Err(relay_error(
                "sequence_overflow",
                "remote transport sequence exceeds the durable counter range",
                Some(envelope.message_id.clone()),
            ));
        }
    };

    match registry.accept_remote_sequence(
        &state.transport_session_id,
        &state.epoch,
        sequence,
        observed_at_ms,
    ) {
        Ok(_) => Ok(state),
        Err(error) => Err(registry_error_frame(
            error,
            Some(envelope.message_id.clone()),
        )),
    }
}

fn response_envelope(
    registry: &mut Registry,
    state: &AuthenticatedConnection,
    capability: RemoteCapability,
    target_agent_session_id: Option<String>,
    observed_at_ms: i64,
) -> Result<RemoteTransportEnvelope, RelayServerFrame> {
    let sequence = registry
        .next_remote_tx_sequence(&state.transport_session_id, &state.epoch, observed_at_ms)
        .map_err(|error| registry_error_frame(error, None))?;

    Ok(RemoteTransportEnvelope {
        protocol_version: REMOTE_PROTOCOL_VERSION,
        device_id: state.device_id.clone(),
        transport_session_id: state.transport_session_id.clone(),
        epoch: state.epoch.clone(),
        sequence: u64::try_from(sequence).unwrap_or(u64::MAX),
        message_id: format!("msg_{}", random_hex(16)),
        capability,
        target_agent_session_id,
        parameter_hash: None,
        expires_at_ms: observed_at_ms.saturating_add(RESPONSE_TTL_MS),
    })
}

fn registry_error_frame(
    error: RemoteRegistryError,
    message_id: Option<String>,
) -> RelayServerFrame {
    let code = match error {
        RemoteRegistryError::DeviceUnavailable => "device_unavailable",
        RemoteRegistryError::AuthChallengeNotFound => "auth_challenge_not_found",
        RemoteRegistryError::AuthChallengeUnavailable => "auth_challenge_unavailable",
        RemoteRegistryError::AuthChallengeBindingMismatch => "auth_challenge_binding_mismatch",
        RemoteRegistryError::SessionNotFound => "transport_session_not_found",
        RemoteRegistryError::SessionUnavailable => "transport_session_unavailable",
        RemoteRegistryError::EpochMismatch => "epoch_mismatch",
        RemoteRegistryError::ReplayDetected => "replay_detected",
        RemoteRegistryError::SequenceOverflow => "sequence_overflow",
        RemoteRegistryError::TargetSessionNotFound => "target_session_not_found",
        RemoteRegistryError::ApprovalNotFound => "approval_not_found",
        RemoteRegistryError::ApprovalUnavailable => "approval_unavailable",
        RemoteRegistryError::ApprovalBindingMismatch => "approval_binding_mismatch",
        RemoteRegistryError::Sql(_) => "registry_error",
    };

    relay_error(code, &error.to_string(), message_id)
}

fn relay_error(code: &str, message: &str, message_id: Option<String>) -> RelayServerFrame {
    RelayServerFrame::Error {
        code: code.to_string(),
        message: message.to_string(),
        message_id,
    }
}

fn random_hex(byte_len: usize) -> String {
    let mut bytes = vec![0_u8; byte_len];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdock_core::{
        AgentIdentity, AgentKind, Framework, ProjectIdentity, Protocol, Service,
        ServiceClassification,
    };
    use ed25519_dalek::{Signer, SigningKey};
    use std::path::PathBuf;

    fn ed25519_public_key(signing_key: &SigningKey) -> String {
        format!(
            "ed25519:{}",
            signing_key
                .verifying_key()
                .to_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        )
    }

    fn pair_device(registry: &mut Registry, signing_key: &SigningKey) -> String {
        registry
            .create_pairing_challenge("pc_relay", "hash-good", 1_000, 10_000)
            .unwrap();
        let request = registry
            .submit_pairing_request(
                "pc_relay",
                "hash-good",
                "Relay device",
                &ed25519_public_key(signing_key),
                2_000,
            )
            .unwrap();
        registry
            .approve_pairing_request(&request.id, 3_000)
            .unwrap()
            .id
    }

    fn create_agent_session(registry: &mut Registry) -> String {
        let service = Service {
            pid: Some(42),
            port: 3000,
            protocol: Protocol::Tcp,
            bind_address: Some("127.0.0.1".into()),
            command: Some("codex".into()),
            command_line: Some("codex app-server".into()),
            working_directory: Some(PathBuf::from("/tmp/agentdock-relay-test")),
            project: Some(ProjectIdentity {
                name: "agentdock-relay-test".into(),
                root: PathBuf::from("/tmp/agentdock-relay-test"),
                git_root: Some(PathBuf::from("/tmp/agentdock-relay-test")),
                git_worktree: false,
            }),
            framework: Framework::Unknown,
            container: None,
            agent: Some(AgentIdentity {
                kind: AgentKind::Codex,
                session_id: Some("codex:relay-test".into()),
            }),
            classification: ServiceClassification::Development,
        };
        registry.reconcile(&[service], 3_100).unwrap();
        registry.list_agent_sessions().unwrap()[0].id.clone()
    }

    fn authenticate(
        registry: &mut Registry,
        signing_key: &SigningKey,
        device_id: &str,
    ) -> (AuthenticatedConnection, i64) {
        let mut state = None;
        let challenge = match handle_client_frame(
            registry,
            &mut state,
            RelayClientFrame::AuthBegin {
                device_id: device_id.to_string(),
            },
            4_000,
        ) {
            RelayServerFrame::AuthChallenge { challenge } => challenge,
            other => panic!("expected auth challenge, got {other:?}"),
        };

        let mut proof = RemoteAuthProof {
            protocol_version: REMOTE_PROTOCOL_VERSION,
            device_id: device_id.to_string(),
            challenge_id: challenge.challenge_id.clone(),
            client_nonce: "22".repeat(32),
            signature: String::new(),
        };
        let transcript = remote_auth::auth_transcript(&challenge, &proof).unwrap();
        proof.signature = format!(
            "ed25519:{}",
            signing_key
                .sign(transcript.as_bytes())
                .to_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );

        let expires_at_ms = match handle_client_frame(
            registry,
            &mut state,
            RelayClientFrame::AuthProof { proof },
            4_100,
        ) {
            RelayServerFrame::Authenticated { expires_at_ms, .. } => expires_at_ms,
            other => panic!("expected authenticated frame, got {other:?}"),
        };

        (state.unwrap(), expires_at_ms)
    }

    fn read_envelope(
        state: &AuthenticatedConnection,
        sequence: u64,
        capability: RemoteCapability,
        target: Option<String>,
    ) -> RemoteTransportEnvelope {
        RemoteTransportEnvelope {
            protocol_version: REMOTE_PROTOCOL_VERSION,
            device_id: state.device_id.clone(),
            transport_session_id: state.transport_session_id.clone(),
            epoch: state.epoch.clone(),
            sequence,
            message_id: format!("req_{sequence}"),
            capability,
            target_agent_session_id: target,
            parameter_hash: None,
            expires_at_ms: 20_000,
        }
    }

    #[test]
    fn relay_config_requires_tls_and_nontrivial_token() {
        assert!(RelayConfig {
            url: "ws://relay.example.test".into(),
            token: "x".repeat(64),
        }
        .validate()
        .is_err());
        assert!(RelayConfig {
            url: "wss://relay.example.test".into(),
            token: "short".into(),
        }
        .validate()
        .is_err());
        assert!(RelayConfig {
            url: "wss://relay.example.test".into(),
            token: format!("{}\n", "x".repeat(64)),
        }
        .validate()
        .is_err());
    }

    #[test]
    fn relay_rejects_sequence_values_outside_durable_counter_range() {
        let mut registry = Registry::in_memory().unwrap();
        let signing_key = SigningKey::from_bytes(&[11_u8; 32]);
        let device_id = pair_device(&mut registry, &signing_key);
        create_agent_session(&mut registry);
        let (state, _) = authenticate(&mut registry, &signing_key, &device_id);
        let mut authenticated = Some(state.clone());

        let envelope = read_envelope(&state, u64::MAX, RemoteCapability::SessionInventory, None);
        assert!(matches!(
            handle_client_frame(
                &mut registry,
                &mut authenticated,
                RelayClientFrame::SessionInventory { envelope },
                4_200,
            ),
            RelayServerFrame::Error { ref code, .. } if code == "sequence_overflow"
        ));
    }

    #[test]
    fn authenticated_relay_allows_read_only_inventory_and_rejects_replay() {
        let mut registry = Registry::in_memory().unwrap();
        let signing_key = SigningKey::from_bytes(&[12_u8; 32]);
        let device_id = pair_device(&mut registry, &signing_key);
        create_agent_session(&mut registry);
        let (state, _) = authenticate(&mut registry, &signing_key, &device_id);
        let mut authenticated = Some(state.clone());

        let envelope = read_envelope(&state, 1, RemoteCapability::SessionInventory, None);
        assert!(matches!(
            handle_client_frame(
                &mut registry,
                &mut authenticated,
                RelayClientFrame::SessionInventory {
                    envelope: envelope.clone(),
                },
                4_200,
            ),
            RelayServerFrame::SessionInventory { .. }
        ));

        assert!(matches!(
            handle_client_frame(
                &mut registry,
                &mut authenticated,
                RelayClientFrame::SessionInventory { envelope },
                4_300,
            ),
            RelayServerFrame::Error { ref code, .. } if code == "replay_detected"
        ));
    }

    #[test]
    fn relay_rejects_mutating_capability_on_read_only_surface() {
        let mut registry = Registry::in_memory().unwrap();
        let signing_key = SigningKey::from_bytes(&[13_u8; 32]);
        let device_id = pair_device(&mut registry, &signing_key);
        let agent_session_id = create_agent_session(&mut registry);
        let (state, _) = authenticate(&mut registry, &signing_key, &device_id);
        let mut authenticated = Some(state.clone());

        let mut envelope = read_envelope(
            &state,
            1,
            RemoteCapability::AgentInput,
            Some(agent_session_id),
        );
        envelope.parameter_hash = Some("a".repeat(64));

        assert!(matches!(
            handle_client_frame(
                &mut registry,
                &mut authenticated,
                RelayClientFrame::SessionInventory { envelope },
                4_200,
            ),
            RelayServerFrame::Error { ref code, .. } if code == "capability_not_allowed"
        ));
    }
}
