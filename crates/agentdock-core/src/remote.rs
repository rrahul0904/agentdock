use serde::{Deserialize, Serialize};

pub const REMOTE_PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteCapability {
    SessionInventory,
    SessionLogs,
    AgentInput,
    ActionApproval,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteCapabilityDescriptor {
    pub capability: RemoteCapability,
    pub mutating: bool,
    pub requires_pairing: bool,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteAuthChallenge {
    pub protocol_version: u16,
    pub device_id: String,
    pub challenge_id: String,
    pub server_nonce: String,
    pub issued_at_ms: i64,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteAuthProof {
    pub protocol_version: u16,
    pub device_id: String,
    pub challenge_id: String,
    pub client_nonce: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteReconnectCursor {
    pub previous_transport_session_id: Option<String>,
    pub last_received_sequence: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteTransportEnvelope {
    pub protocol_version: u16,
    pub device_id: String,
    pub transport_session_id: String,
    pub epoch: String,
    pub sequence: u64,
    pub message_id: String,
    pub capability: RemoteCapability,
    pub target_agent_session_id: Option<String>,
    pub parameter_hash: Option<String>,
    pub expires_at_ms: i64,
}

impl RemoteTransportEnvelope {
    pub fn is_well_formed(&self) -> bool {
        if self.protocol_version != REMOTE_PROTOCOL_VERSION
            || !bounded_identifier(&self.device_id)
            || !bounded_identifier(&self.transport_session_id)
            || !bounded_identifier(&self.epoch)
            || !bounded_identifier(&self.message_id)
            || self.sequence == 0
            || self.expires_at_ms <= 0
        {
            return false;
        }

        let descriptor = remote_capability(self.capability);
        if descriptor.mutating {
            return self
                .target_agent_session_id
                .as_deref()
                .is_some_and(bounded_identifier)
                && self
                    .parameter_hash
                    .as_deref()
                    .is_some_and(valid_parameter_hash);
        }

        true
    }
}

pub fn remote_capability(capability: RemoteCapability) -> RemoteCapabilityDescriptor {
    match capability {
        RemoteCapability::SessionInventory => RemoteCapabilityDescriptor {
            capability,
            mutating: false,
            requires_pairing: true,
            requires_approval: false,
        },
        RemoteCapability::SessionLogs => RemoteCapabilityDescriptor {
            capability,
            mutating: false,
            requires_pairing: true,
            requires_approval: false,
        },
        RemoteCapability::AgentInput => RemoteCapabilityDescriptor {
            capability,
            mutating: true,
            requires_pairing: true,
            requires_approval: true,
        },
        RemoteCapability::ActionApproval => RemoteCapabilityDescriptor {
            capability,
            mutating: true,
            requires_pairing: true,
            requires_approval: true,
        },
    }
}

pub fn remote_capabilities() -> Vec<RemoteCapabilityDescriptor> {
    [
        RemoteCapability::SessionInventory,
        RemoteCapability::SessionLogs,
        RemoteCapability::AgentInput,
        RemoteCapability::ActionApproval,
    ]
    .into_iter()
    .map(remote_capability)
    .collect()
}

fn bounded_identifier(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 128
}

fn valid_parameter_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_remote_capabilities_require_pairing() {
        assert!(remote_capabilities()
            .iter()
            .all(|capability| capability.requires_pairing));
    }

    #[test]
    fn mutating_remote_capabilities_require_explicit_approval() {
        assert!(remote_capabilities()
            .iter()
            .filter(|capability| capability.mutating)
            .all(|capability| capability.requires_approval));
    }

    #[test]
    fn capability_contract_is_intentionally_narrow() {
        let capabilities = remote_capabilities();

        assert_eq!(capabilities.len(), 4);
        assert_eq!(
            capabilities
                .iter()
                .map(|descriptor| descriptor.capability)
                .collect::<Vec<_>>(),
            vec![
                RemoteCapability::SessionInventory,
                RemoteCapability::SessionLogs,
                RemoteCapability::AgentInput,
                RemoteCapability::ActionApproval,
            ]
        );
    }

    #[test]
    fn mutating_transport_envelope_requires_exact_parameter_binding() {
        let mut envelope = RemoteTransportEnvelope {
            protocol_version: REMOTE_PROTOCOL_VERSION,
            device_id: "dev_test".into(),
            transport_session_id: "rts_test".into(),
            epoch: "epoch_1".into(),
            sequence: 1,
            message_id: "msg_1".into(),
            capability: RemoteCapability::AgentInput,
            target_agent_session_id: Some("ags_test".into()),
            parameter_hash: None,
            expires_at_ms: 10_000,
        };

        assert!(!envelope.is_well_formed());

        envelope.parameter_hash = Some("a".repeat(64));
        assert!(envelope.is_well_formed());

        envelope.protocol_version = REMOTE_PROTOCOL_VERSION + 1;
        assert!(!envelope.is_well_formed());
    }
}
