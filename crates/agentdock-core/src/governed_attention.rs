use serde::{Deserialize, Serialize};

const MAX_REQUEST_TTL_SECONDS: u32 = 86_400;
const MAX_REQUEST_USES: u32 = 100;
const MAX_REASON_BYTES: usize = 4_096;
const MAX_SUMMARY_BYTES: usize = 2_048;

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord,
)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityEnvironment {
    Local,
    Preview,
    Staging,
    Production,
    Unknown,
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord,
)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityActionClass {
    Observe,
    Read,
    LocalMutation,
    RemoteWrite,
    Deploy,
    DatabaseWrite,
    Delete,
    Unknown,
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord,
)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityDecision {
    Pending,
    Approved,
    Denied,
    Expired,
    Revoked,
    Consumed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityRequest {
    pub id: String,
    pub project_id: String,
    pub task_id: Option<String>,
    pub session_id: String,
    pub provider: String,
    pub target: Option<String>,
    pub environment: CapabilityEnvironment,
    pub action: CapabilityActionClass,
    /// Lowercase SHA-256 hex of the canonical action intent.
    pub intent_digest: String,
    pub requested_ttl_seconds: u32,
    pub requested_uses: u32,
    pub reason: String,
}

impl CapabilityRequest {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !is_bounded_identifier(&self.id)
            || !is_bounded_identifier(&self.project_id)
            || !is_bounded_identifier(&self.session_id)
        {
            return Err("invalid identifier");
        }
        if self
            .task_id
            .as_deref()
            .is_some_and(|value| !is_bounded_identifier(value))
        {
            return Err("invalid task identifier");
        }
        if self.provider.trim().is_empty() || self.provider.len() > 128 {
            return Err("invalid provider");
        }
        if self
            .target
            .as_deref()
            .is_some_and(|value| value.trim().is_empty() || value.len() > 512)
        {
            return Err("invalid target");
        }
        if !is_lower_hex_sha256(&self.intent_digest) {
            return Err("invalid intent digest");
        }
        if self.requested_ttl_seconds == 0
            || self.requested_ttl_seconds > MAX_REQUEST_TTL_SECONDS
        {
            return Err("invalid requested ttl");
        }
        if self.requested_uses == 0 || self.requested_uses > MAX_REQUEST_USES {
            return Err("invalid requested uses");
        }
        if self.reason.trim().is_empty() || self.reason.len() > MAX_REASON_BYTES {
            return Err("invalid reason");
        }
        Ok(())
    }

    /// Conservative projection only. Policy remains authoritative.
    /// Unknown classifications and production are never eligible for auto-approval.
    pub fn eligible_for_auto_approval(&self) -> bool {
        self.validate().is_ok()
            && matches!(
                self.environment,
                CapabilityEnvironment::Local
                    | CapabilityEnvironment::Preview
                    | CapabilityEnvironment::Staging
            )
            && matches!(
                self.action,
                CapabilityActionClass::Observe | CapabilityActionClass::Read
            )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityGrantView {
    pub request_id: String,
    pub decision: CapabilityDecision,
    /// Human-readable scope only; this projection intentionally has no credential/token field.
    pub scope_summary: String,
    pub expires_at_unix_ms: Option<u64>,
    pub remaining_uses: u32,
    pub evidence_source: String,
}

impl CapabilityGrantView {
    pub fn is_active_at(&self, now_unix_ms: u64) -> bool {
        self.decision == CapabilityDecision::Approved
            && self.remaining_uses > 0
            && self
                .expires_at_unix_ms
                .is_some_and(|expires_at| expires_at > now_unix_ms)
    }
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord,
)]
#[serde(rename_all = "kebab-case")]
pub enum AttentionKind {
    NeedsInput,
    Review,
    AccessRequest,
    VerificationFailed,
    MergeGate,
    DeployGate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttentionItem {
    pub id: String,
    pub project_id: String,
    pub task_id: Option<String>,
    pub session_id: Option<String>,
    pub kind: AttentionKind,
    pub summary: String,
    pub source_ref: String,
    pub created_at_unix_ms: u64,
}

impl AttentionItem {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !is_bounded_identifier(&self.id) || !is_bounded_identifier(&self.project_id) {
            return Err("invalid identifier");
        }
        if self
            .task_id
            .as_deref()
            .is_some_and(|value| !is_bounded_identifier(value))
        {
            return Err("invalid task identifier");
        }
        if self
            .session_id
            .as_deref()
            .is_some_and(|value| !is_bounded_identifier(value))
        {
            return Err("invalid session identifier");
        }
        if self.summary.trim().is_empty() || self.summary.len() > MAX_SUMMARY_BYTES {
            return Err("invalid summary");
        }
        if self.source_ref.trim().is_empty() || self.source_ref.len() > 512 {
            return Err("invalid source reference");
        }
        Ok(())
    }

    pub fn ordering_key(&self) -> (u64, &str) {
        (self.created_at_unix_ms, self.id.as_str())
    }
}

fn is_bounded_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
        })
}

fn is_lower_hex_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(
        environment: CapabilityEnvironment,
        action: CapabilityActionClass,
    ) -> CapabilityRequest {
        CapabilityRequest {
            id: "req-1".into(),
            project_id: "project-1".into(),
            task_id: Some("task-1".into()),
            session_id: "session-1".into(),
            provider: "github".into(),
            target: Some("rrahul0904/agentdock".into()),
            environment,
            action,
            intent_digest: "a".repeat(64),
            requested_ttl_seconds: 300,
            requested_uses: 1,
            reason: "Read pull request state".into(),
        }
    }

    #[test]
    fn known_read_can_be_policy_candidate() {
        let value = request(CapabilityEnvironment::Preview, CapabilityActionClass::Read);
        assert!(value.validate().is_ok());
        assert!(value.eligible_for_auto_approval());
    }

    #[test]
    fn malformed_request_never_auto_approves() {
        let mut value = request(CapabilityEnvironment::Preview, CapabilityActionClass::Read);
        value.intent_digest = "not-a-digest".into();
        assert!(!value.eligible_for_auto_approval());
    }

    #[test]
    fn unknown_environment_or_action_never_auto_approves() {
        assert!(
            !request(CapabilityEnvironment::Unknown, CapabilityActionClass::Read)
                .eligible_for_auto_approval()
        );
        assert!(
            !request(CapabilityEnvironment::Local, CapabilityActionClass::Unknown)
                .eligible_for_auto_approval()
        );
    }

    #[test]
    fn production_never_auto_approves_even_reads() {
        assert!(
            !request(CapabilityEnvironment::Production, CapabilityActionClass::Read)
                .eligible_for_auto_approval()
        );
    }

    #[test]
    fn remote_mutations_never_auto_approve() {
        assert!(
            !request(
                CapabilityEnvironment::Staging,
                CapabilityActionClass::RemoteWrite,
            )
            .eligible_for_auto_approval()
        );
        assert!(
            !request(CapabilityEnvironment::Preview, CapabilityActionClass::Deploy)
                .eligible_for_auto_approval()
        );
    }

    #[test]
    fn digest_must_be_exact_lowercase_sha256_hex() {
        let mut value = request(CapabilityEnvironment::Local, CapabilityActionClass::Read);
        value.intent_digest = "A".repeat(64);
        assert_eq!(value.validate(), Err("invalid intent digest"));

        value.intent_digest = "a".repeat(63);
        assert_eq!(value.validate(), Err("invalid intent digest"));
    }

    #[test]
    fn grant_activity_requires_approved_unexpired_remaining_use() {
        let active = CapabilityGrantView {
            request_id: "req-1".into(),
            decision: CapabilityDecision::Approved,
            scope_summary: "github read on rrahul0904/agentdock".into(),
            expires_at_unix_ms: Some(2_000),
            remaining_uses: 1,
            evidence_source: "approval-receipt:abc".into(),
        };
        assert!(active.is_active_at(1_999));

        let mut consumed = active.clone();
        consumed.remaining_uses = 0;
        assert!(!consumed.is_active_at(1_999));

        let mut revoked = active.clone();
        revoked.decision = CapabilityDecision::Revoked;
        assert!(!revoked.is_active_at(1_999));

        assert!(!active.is_active_at(2_000));
    }

    #[test]
    fn attention_ordering_is_deterministic() {
        let mut items = [
            AttentionItem {
                id: "b".into(),
                project_id: "project-1".into(),
                task_id: None,
                session_id: None,
                kind: AttentionKind::Review,
                summary: "Review B".into(),
                source_ref: "receipt:b".into(),
                created_at_unix_ms: 10,
            },
            AttentionItem {
                id: "a".into(),
                project_id: "project-1".into(),
                task_id: None,
                session_id: None,
                kind: AttentionKind::AccessRequest,
                summary: "Review A".into(),
                source_ref: "receipt:a".into(),
                created_at_unix_ms: 10,
            },
        ];

        items.sort_by(|left, right| left.ordering_key().cmp(&right.ordering_key()));
        assert_eq!(items[0].id, "a");
        assert_eq!(items[1].id, "b");
    }
}
