use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

pub const SCHEMA_VERSION: &str = "agentdock-hive/v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HiveConfig {
    pub max_hops: u16,
    pub max_body_bytes: usize,
}

impl Default for HiveConfig {
    fn default() -> Self {
        Self {
            max_hops: 8,
            max_body_bytes: 64 * 1024,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentRegistration {
    pub agent_id: String,
    pub floor_id: String,
    pub session_id: String,
    pub workspace_root: String,
    pub provider: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageStatus {
    Queued,
    InFlight { lease: u64 },
    Acked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HiveMessage {
    pub message_id: String,
    pub floor_id: String,
    pub from_agent: String,
    pub to_agent: String,
    pub task_id: Option<String>,
    pub body: String,
    pub hop_count: u16,
    pub attempts: u32,
    pub status: MessageStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnqueueMessage {
    pub message_id: String,
    pub from_agent: String,
    pub to_agent: String,
    pub task_id: Option<String>,
    pub body: String,
    pub hop_count: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Delivery {
    pub lease: u64,
    pub message: HiveMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HiveState {
    pub schema_version: String,
    pub config: HiveConfig,
    pub agents: BTreeMap<String, AgentRegistration>,
    pub messages: BTreeMap<String, HiveMessage>,
    pub queue: Vec<String>,
    pub acked: BTreeSet<String>,
    next_lease: u64,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HiveError {
    #[error("max_hops must be greater than zero")]
    InvalidMaxHops,
    #[error("max_body_bytes must be greater than zero")]
    InvalidBodyLimit,
    #[error("field {0} must not be empty")]
    EmptyField(&'static str),
    #[error("agent already registered: {0}")]
    AgentAlreadyExists(String),
    #[error("agent not found: {0}")]
    AgentNotFound(String),
    #[error("session already owned on floor: {0}")]
    SessionConflict(String),
    #[error("workspace already owned on floor: {0}")]
    WorkspaceConflict(String),
    #[error("message already exists: {0}")]
    MessageAlreadyExists(String),
    #[error("message not found: {0}")]
    MessageNotFound(String),
    #[error("cross-floor routing is not allowed")]
    CrossFloorRoute,
    #[error("message hop count {actual} exceeds limit {limit}")]
    HopLimitExceeded { actual: u16, limit: u16 },
    #[error("message body exceeds configured byte limit")]
    BodyTooLarge,
    #[error("message is not in flight")]
    MessageNotInFlight,
    #[error("lease mismatch")]
    LeaseMismatch,
    #[error("acknowledger does not own this delivery")]
    AckOwnerMismatch,
    #[error("snapshot schema mismatch: {0}")]
    SchemaMismatch(String),
    #[error("snapshot invariant failed: {0}")]
    InvalidSnapshot(String),
    #[error("snapshot serialization failed: {0}")]
    SnapshotSerialization(String),
}

impl HiveState {
    pub fn new(config: HiveConfig) -> Result<Self, HiveError> {
        validate_config(&config)?;
        Ok(Self {
            schema_version: SCHEMA_VERSION.to_string(),
            config,
            agents: BTreeMap::new(),
            messages: BTreeMap::new(),
            queue: Vec::new(),
            acked: BTreeSet::new(),
            next_lease: 1,
        })
    }

    pub fn register_agent(&mut self, agent: AgentRegistration) -> Result<(), HiveError> {
        require_non_empty("agent_id", &agent.agent_id)?;
        require_non_empty("floor_id", &agent.floor_id)?;
        require_non_empty("session_id", &agent.session_id)?;
        require_non_empty("workspace_root", &agent.workspace_root)?;
        require_non_empty("provider", &agent.provider)?;

        if self.agents.contains_key(&agent.agent_id) {
            return Err(HiveError::AgentAlreadyExists(agent.agent_id));
        }

        for existing in self.agents.values().filter(|a| a.floor_id == agent.floor_id) {
            if existing.session_id == agent.session_id {
                return Err(HiveError::SessionConflict(agent.session_id));
            }
            if existing.workspace_root == agent.workspace_root {
                return Err(HiveError::WorkspaceConflict(agent.workspace_root));
            }
        }

        self.agents.insert(agent.agent_id.clone(), agent);
        Ok(())
    }

    pub fn enqueue(&mut self, input: EnqueueMessage) -> Result<(), HiveError> {
        require_non_empty("message_id", &input.message_id)?;
        require_non_empty("from_agent", &input.from_agent)?;
        require_non_empty("to_agent", &input.to_agent)?;
        require_non_empty("body", &input.body)?;

        if self.messages.contains_key(&input.message_id) {
            return Err(HiveError::MessageAlreadyExists(input.message_id));
        }
        if input.body.as_bytes().len() > self.config.max_body_bytes {
            return Err(HiveError::BodyTooLarge);
        }
        if input.hop_count > self.config.max_hops {
            return Err(HiveError::HopLimitExceeded {
                actual: input.hop_count,
                limit: self.config.max_hops,
            });
        }

        let from = self
            .agents
            .get(&input.from_agent)
            .ok_or_else(|| HiveError::AgentNotFound(input.from_agent.clone()))?;
        let to = self
            .agents
            .get(&input.to_agent)
            .ok_or_else(|| HiveError::AgentNotFound(input.to_agent.clone()))?;
        if from.floor_id != to.floor_id {
            return Err(HiveError::CrossFloorRoute);
        }

        let message = HiveMessage {
            message_id: input.message_id.clone(),
            floor_id: from.floor_id.clone(),
            from_agent: input.from_agent,
            to_agent: input.to_agent,
            task_id: input.task_id,
            body: input.body,
            hop_count: input.hop_count,
            attempts: 0,
            status: MessageStatus::Queued,
        };
        self.messages.insert(input.message_id.clone(), message);
        self.queue.push(input.message_id);
        Ok(())
    }

    pub fn reply(
        &mut self,
        original_message_id: &str,
        new_message_id: String,
        from_agent: String,
        body: String,
    ) -> Result<(), HiveError> {
        let original = self
            .messages
            .get(original_message_id)
            .ok_or_else(|| HiveError::MessageNotFound(original_message_id.to_string()))?
            .clone();

        if original.to_agent != from_agent {
            return Err(HiveError::AckOwnerMismatch);
        }

        let next_hop = original.hop_count.saturating_add(1);
        self.enqueue(EnqueueMessage {
            message_id: new_message_id,
            from_agent,
            to_agent: original.from_agent,
            task_id: original.task_id,
            body,
            hop_count: next_hop,
        })
    }

    pub fn claim_next(
        &mut self,
        floor_id: &str,
        agent_id: &str,
    ) -> Result<Option<Delivery>, HiveError> {
        let agent = self
            .agents
            .get(agent_id)
            .ok_or_else(|| HiveError::AgentNotFound(agent_id.to_string()))?;
        if agent.floor_id != floor_id {
            return Ok(None);
        }

        let message_id = self.queue.iter().find_map(|id| {
            let message = self.messages.get(id)?;
            if message.floor_id == floor_id
                && message.to_agent == agent_id
                && matches!(message.status, MessageStatus::Queued)
            {
                Some(id.clone())
            } else {
                None
            }
        });

        let Some(message_id) = message_id else {
            return Ok(None);
        };

        let lease = self.next_lease;
        self.next_lease = self.next_lease.saturating_add(1);
        let message = self
            .messages
            .get_mut(&message_id)
            .expect("queue references an existing message");
        message.attempts = message.attempts.saturating_add(1);
        message.status = MessageStatus::InFlight { lease };

        Ok(Some(Delivery {
            lease,
            message: message.clone(),
        }))
    }

    pub fn ack(
        &mut self,
        message_id: &str,
        agent_id: &str,
        lease: u64,
    ) -> Result<(), HiveError> {
        let message = self
            .messages
            .get_mut(message_id)
            .ok_or_else(|| HiveError::MessageNotFound(message_id.to_string()))?;
        if message.to_agent != agent_id {
            return Err(HiveError::AckOwnerMismatch);
        }
        match message.status {
            MessageStatus::InFlight { lease: current } if current == lease => {
                message.status = MessageStatus::Acked;
                self.acked.insert(message_id.to_string());
                Ok(())
            }
            MessageStatus::InFlight { .. } => Err(HiveError::LeaseMismatch),
            _ => Err(HiveError::MessageNotInFlight),
        }
    }

    pub fn recover_inflight(&mut self) -> usize {
        let mut recovered = 0;
        for message in self.messages.values_mut() {
            if matches!(message.status, MessageStatus::InFlight { .. }) {
                message.status = MessageStatus::Queued;
                recovered += 1;
            }
        }
        recovered
    }

    pub fn snapshot_json(&self) -> Result<String, HiveError> {
        serde_json::to_string(self).map_err(|e| HiveError::SnapshotSerialization(e.to_string()))
    }

    pub fn from_snapshot_json(snapshot: &str) -> Result<Self, HiveError> {
        let state: Self = serde_json::from_str(snapshot)
            .map_err(|e| HiveError::SnapshotSerialization(e.to_string()))?;
        state.validate()?;
        Ok(state)
    }

    pub fn validate(&self) -> Result<(), HiveError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(HiveError::SchemaMismatch(self.schema_version.clone()));
        }
        validate_config(&self.config)?;

        let mut queued_ids = BTreeSet::new();
        for id in &self.queue {
            if !queued_ids.insert(id.clone()) {
                return Err(HiveError::InvalidSnapshot(format!(
                    "duplicate queue entry: {id}"
                )));
            }
            if !self.messages.contains_key(id) {
                return Err(HiveError::InvalidSnapshot(format!(
                    "queue references missing message: {id}"
                )));
            }
        }

        for (id, message) in &self.messages {
            if !queued_ids.contains(id) {
                return Err(HiveError::InvalidSnapshot(format!(
                    "message missing from queue ledger: {id}"
                )));
            }
            if message.message_id != *id {
                return Err(HiveError::InvalidSnapshot(format!(
                    "message key/id mismatch: {id}"
                )));
            }
            if message.hop_count > self.config.max_hops {
                return Err(HiveError::InvalidSnapshot(format!(
                    "message exceeds hop limit: {id}"
                )));
            }
            let from = self.agents.get(&message.from_agent).ok_or_else(|| {
                HiveError::InvalidSnapshot(format!("missing sender for message: {id}"))
            })?;
            let to = self.agents.get(&message.to_agent).ok_or_else(|| {
                HiveError::InvalidSnapshot(format!("missing recipient for message: {id}"))
            })?;
            if from.floor_id != message.floor_id || to.floor_id != message.floor_id {
                return Err(HiveError::InvalidSnapshot(format!(
                    "cross-floor message in snapshot: {id}"
                )));
            }
            let is_acked = matches!(message.status, MessageStatus::Acked);
            if is_acked != self.acked.contains(id) {
                return Err(HiveError::InvalidSnapshot(format!(
                    "ack ledger mismatch: {id}"
                )));
            }
        }
        Ok(())
    }
}

fn validate_config(config: &HiveConfig) -> Result<(), HiveError> {
    if config.max_hops == 0 {
        return Err(HiveError::InvalidMaxHops);
    }
    if config.max_body_bytes == 0 {
        return Err(HiveError::InvalidBodyLimit);
    }
    Ok(())
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), HiveError> {
    if value.trim().is_empty() {
        Err(HiveError::EmptyField(field))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, floor: &str, session: &str, workspace: &str) -> AgentRegistration {
        AgentRegistration {
            agent_id: id.to_string(),
            floor_id: floor.to_string(),
            session_id: session.to_string(),
            workspace_root: workspace.to_string(),
            provider: "fake".to_string(),
        }
    }

    fn hive() -> HiveState {
        let mut hive = HiveState::new(HiveConfig::default()).unwrap();
        hive.register_agent(agent("michael", "floor-a", "s1", "/work/michael"))
            .unwrap();
        hive.register_agent(agent("dwight", "floor-a", "s2", "/work/dwight"))
            .unwrap();
        hive
    }

    fn enqueue_basic(hive: &mut HiveState) {
        hive.enqueue(EnqueueMessage {
            message_id: "m1".to_string(),
            from_agent: "michael".to_string(),
            to_agent: "dwight".to_string(),
            task_id: Some("t1".to_string()),
            body: "inspect repository".to_string(),
            hop_count: 0,
        })
        .unwrap();
    }

    #[test]
    fn rejects_workspace_collision_on_same_floor() {
        let mut hive = hive();
        let error = hive
            .register_agent(agent("jim", "floor-a", "s3", "/work/dwight"))
            .unwrap_err();
        assert_eq!(
            error,
            HiveError::WorkspaceConflict("/work/dwight".to_string())
        );
    }

    #[test]
    fn delivery_is_claimed_once_until_ack_or_recovery() {
        let mut hive = hive();
        enqueue_basic(&mut hive);

        let first = hive.claim_next("floor-a", "dwight").unwrap().unwrap();
        assert_eq!(first.message.message_id, "m1");
        assert_eq!(first.message.attempts, 1);
        assert!(hive.claim_next("floor-a", "dwight").unwrap().is_none());

        hive.ack("m1", "dwight", first.lease).unwrap();
        assert!(hive.claim_next("floor-a", "dwight").unwrap().is_none());
        assert!(hive.acked.contains("m1"));
    }

    #[test]
    fn restart_requeues_same_message_identity() {
        let mut hive = hive();
        enqueue_basic(&mut hive);
        let first = hive.claim_next("floor-a", "dwight").unwrap().unwrap();
        assert_eq!(first.message.attempts, 1);

        let snapshot = hive.snapshot_json().unwrap();
        let mut restored = HiveState::from_snapshot_json(&snapshot).unwrap();
        assert_eq!(restored.recover_inflight(), 1);

        let retried = restored.claim_next("floor-a", "dwight").unwrap().unwrap();
        assert_eq!(retried.message.message_id, "m1");
        assert_eq!(retried.message.attempts, 2);
        assert_ne!(retried.lease, first.lease);
        assert_eq!(restored.messages.len(), 1);
    }

    #[test]
    fn stale_lease_cannot_ack_retried_delivery() {
        let mut hive = hive();
        enqueue_basic(&mut hive);
        let first = hive.claim_next("floor-a", "dwight").unwrap().unwrap();
        hive.recover_inflight();
        let second = hive.claim_next("floor-a", "dwight").unwrap().unwrap();

        assert_eq!(
            hive.ack("m1", "dwight", first.lease).unwrap_err(),
            HiveError::LeaseMismatch
        );
        hive.ack("m1", "dwight", second.lease).unwrap();
    }

    #[test]
    fn hop_limit_breaks_message_loops() {
        let mut hive = HiveState::new(HiveConfig {
            max_hops: 1,
            max_body_bytes: 1024,
        })
        .unwrap();
        hive.register_agent(agent("a", "floor-a", "s1", "/a"))
            .unwrap();
        hive.register_agent(agent("b", "floor-a", "s2", "/b"))
            .unwrap();
        hive.enqueue(EnqueueMessage {
            message_id: "m1".into(),
            from_agent: "a".into(),
            to_agent: "b".into(),
            task_id: None,
            body: "one".into(),
            hop_count: 0,
        })
        .unwrap();
        hive.reply("m1", "m2".into(), "b".into(), "two".into())
            .unwrap();

        let error = hive
            .reply("m2", "m3".into(), "a".into(), "three".into())
            .unwrap_err();
        assert_eq!(
            error,
            HiveError::HopLimitExceeded {
                actual: 2,
                limit: 1
            }
        );
    }

    #[test]
    fn cross_floor_routing_is_refused() {
        let mut hive = hive();
        hive.register_agent(agent("pam", "floor-b", "s3", "/work/pam"))
            .unwrap();
        let error = hive
            .enqueue(EnqueueMessage {
                message_id: "m-cross".into(),
                from_agent: "michael".into(),
                to_agent: "pam".into(),
                task_id: None,
                body: "cross floor".into(),
                hop_count: 0,
            })
            .unwrap_err();
        assert_eq!(error, HiveError::CrossFloorRoute);
    }

    #[test]
    fn snapshots_are_deterministic_and_validate_on_restore() {
        let mut hive = hive();
        enqueue_basic(&mut hive);
        let first = hive.snapshot_json().unwrap();
        let restored = HiveState::from_snapshot_json(&first).unwrap();
        let second = restored.snapshot_json().unwrap();
        assert_eq!(first, second);
    }
}
