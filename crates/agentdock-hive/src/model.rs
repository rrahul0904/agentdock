use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_ATTEMPTS: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Floor {
    pub schema_version: u32,
    pub id: String,
    pub owner_id: String,
    /// Canonical, existing workspace path. One floor per workspace in Phase A.
    pub workspace: String,
    pub max_hops: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Provider { Fake, Codex }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Agent {
    pub schema_version: u32,
    pub id: String,
    pub floor_id: String,
    pub role: String,
    pub provider: Provider,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionState { Active, Interrupted }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Session {
    pub schema_version: u32,
    pub id: String,
    pub floor_id: String,
    pub agent_id: String,
    pub owner_id: String,
    pub workspace: String,
    pub state: SessionState,
}

/// Internal daemon capability, NOT an unauthenticated HTTP input or a bearer token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    pub floor_id: String,
    pub agent_id: String,
    pub session_id: String,
    pub owner_id: String,
    pub workspace: String,
}

impl From<&Session> for Scope {
    fn from(s: &Session) -> Self {
        Self {
            floor_id: s.floor_id.clone(), agent_id: s.agent_id.clone(),
            session_id: s.id.clone(), owner_id: s.owner_id.clone(),
            workspace: s.workspace.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskState { Todo, Doing, Blocked, Done }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub schema_version: u32,
    pub id: String,
    pub floor_id: String,
    pub session_id: String,
    pub agent_id: String,
    pub title: String,
    pub state: TaskState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MemoryRef {
    pub schema_version: u32,
    pub id: String,
    pub floor_id: String,
    pub session_id: String,
    pub agent_id: String,
    /// Canonical in-workspace reference; the hive does not read or index its content.
    pub path: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageAct { Request, Inform, Propose, Query, Agree, Refuse, Done }

impl MessageAct {
    pub fn requires_reply(self) -> bool {
        matches!(self, Self::Request | Self::Propose | Self::Query)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub schema_version: u32,
    pub id: String,
    pub floor_id: String,
    pub from_agent_id: String,
    pub from_session_id: String,
    pub to_agent_id: String,
    pub idempotency_key: String,
    pub conversation_id: String,
    pub in_reply_to: Option<String>,
    pub act: MessageAct,
    pub body: String,
    pub hops: u32,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone)]
pub struct SendRequest {
    pub to_agent_id: String,
    pub idempotency_key: String,
    pub conversation_id: String,
    pub in_reply_to: Option<String>,
    pub act: MessageAct,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    SessionStarted, SessionInterrupted,
    MessageEnqueued, MessageClaimed, MessageAcked, MessageRetried,
    MessageDead, MessageReplayed, HopLimitReached,
    TaskChanged, MemoryLinked, TurnStarted, TurnSucceeded, TurnFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub schema_version: u32,
    pub seq: i64,
    pub floor_id: String,
    pub actor_agent_id: String,
    pub peer_agent_id: Option<String>,
    pub message_id: Option<String>,
    pub kind: EventKind,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone)]
pub struct Delivery {
    pub message: Message,
    pub token: String,
    pub attempt: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryState { Pending, Leased, Retry, Acked, Dead }
