use serde::{Deserialize, Serialize};

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

pub fn remote_capabilities() -> Vec<RemoteCapabilityDescriptor> {
    vec![
        RemoteCapabilityDescriptor {
            capability: RemoteCapability::SessionInventory,
            mutating: false,
            requires_pairing: true,
            requires_approval: false,
        },
        RemoteCapabilityDescriptor {
            capability: RemoteCapability::SessionLogs,
            mutating: false,
            requires_pairing: true,
            requires_approval: false,
        },
        RemoteCapabilityDescriptor {
            capability: RemoteCapability::AgentInput,
            mutating: true,
            requires_pairing: true,
            requires_approval: true,
        },
        RemoteCapabilityDescriptor {
            capability: RemoteCapability::ActionApproval,
            mutating: true,
            requires_pairing: true,
            requires_approval: true,
        },
    ]
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
}
