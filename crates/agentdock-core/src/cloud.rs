use crate::AgentKind;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum MachineState {
    Requested,
    Provisioning,
    Ready,
    Suspended,
    Stopping,
    Stopped,
    Deleting,
    Deleted,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MachineRecord {
    pub machine_id: String,
    pub owner_id: String,
    pub project_id: String,
    pub workspace_id: String,
    pub agent_kind: AgentKind,
    pub encrypted_volume_ref: Option<String>,
    pub state: MachineState,
    pub generation: u64,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateMachineRequest {
    pub machine_id: String,
    pub owner_id: String,
    pub project_id: String,
    pub workspace_id: String,
    pub agent_kind: AgentKind,
    pub encrypted_volume_ref: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum MachineOperationKind {
    Create,
    Start,
    Stop,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OperationReceipt {
    pub idempotency_key: String,
    pub machine_id: String,
    pub owner_id: String,
    pub kind: MachineOperationKind,
    pub generation: u64,
    pub final_state: MachineState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskEvent {
    pub sequence: u64,
    pub machine_id: String,
    pub owner_id: String,
    pub session_id: String,
    pub task_id: String,
    pub kind: String,
    pub payload: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CloudError {
    NotFound,
    Forbidden,
    IdempotencyConflict,
    InvalidTransition {
        from: MachineState,
        operation: MachineOperationKind,
    },
    LockPoisoned,
}

impl fmt::Display for CloudError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "machine not found"),
            Self::Forbidden => write!(f, "owner is not authorized for this machine"),
            Self::IdempotencyConflict => write!(f, "idempotency key conflicts with an existing operation"),
            Self::InvalidTransition { from, operation } => write!(
                f,
                "invalid machine state transition from {from:?} for {operation:?}"
            ),
            Self::LockPoisoned => write!(f, "provider state lock is poisoned"),
        }
    }
}

impl std::error::Error for CloudError {}

pub trait ComputeProvider: Send + Sync {
    fn create_machine(&self, request: CreateMachineRequest) -> Result<MachineRecord, CloudError>;
    fn start_machine(&self, owner_id: &str, machine_id: &str, idempotency_key: &str)
        -> Result<MachineRecord, CloudError>;
    fn stop_machine(&self, owner_id: &str, machine_id: &str, idempotency_key: &str)
        -> Result<MachineRecord, CloudError>;
    fn delete_machine(&self, owner_id: &str, machine_id: &str, idempotency_key: &str)
        -> Result<MachineRecord, CloudError>;
    fn get_machine(&self, owner_id: &str, machine_id: &str) -> Result<MachineRecord, CloudError>;
    fn list_machines(&self, owner_id: &str) -> Result<Vec<MachineRecord>, CloudError>;
    fn append_task_event(
        &self,
        owner_id: &str,
        machine_id: &str,
        session_id: &str,
        task_id: &str,
        kind: &str,
        payload: &str,
    ) -> Result<TaskEvent, CloudError>;
    fn task_events_since(
        &self,
        owner_id: &str,
        machine_id: &str,
        after_sequence: u64,
    ) -> Result<Vec<TaskEvent>, CloudError>;
}

#[derive(Debug, Default)]
struct FakeState {
    machines: BTreeMap<String, MachineRecord>,
    operations: BTreeMap<String, OperationReceipt>,
    events: BTreeMap<String, Vec<TaskEvent>>,
    next_sequence: u64,
}

#[derive(Debug, Clone, Default)]
pub struct FakeComputeProvider {
    state: Arc<Mutex<FakeState>>,
}

impl FakeComputeProvider {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, FakeState>, CloudError> {
        self.state.lock().map_err(|_| CloudError::LockPoisoned)
    }

    fn validate_owner(machine: &MachineRecord, owner_id: &str) -> Result<(), CloudError> {
        if machine.owner_id == owner_id {
            Ok(())
        } else {
            Err(CloudError::Forbidden)
        }
    }

    fn replay_operation(
        state: &FakeState,
        owner_id: &str,
        machine_id: &str,
        idempotency_key: &str,
        kind: MachineOperationKind,
    ) -> Result<Option<MachineRecord>, CloudError> {
        let Some(receipt) = state.operations.get(idempotency_key) else {
            return Ok(None);
        };
        if receipt.owner_id != owner_id || receipt.machine_id != machine_id || receipt.kind != kind {
            return Err(CloudError::IdempotencyConflict);
        }
        let machine = state.machines.get(machine_id).ok_or(CloudError::NotFound)?;
        Self::validate_owner(machine, owner_id)?;
        Ok(Some(machine.clone()))
    }

    fn record_operation(
        state: &mut FakeState,
        idempotency_key: &str,
        machine: &MachineRecord,
        kind: MachineOperationKind,
    ) {
        state.operations.insert(
            idempotency_key.to_string(),
            OperationReceipt {
                idempotency_key: idempotency_key.to_string(),
                machine_id: machine.machine_id.clone(),
                owner_id: machine.owner_id.clone(),
                kind,
                generation: machine.generation,
                final_state: machine.state,
            },
        );
    }

    fn mutate_machine(
        state: &mut FakeState,
        owner_id: &str,
        machine_id: &str,
        idempotency_key: &str,
        kind: MachineOperationKind,
    ) -> Result<MachineRecord, CloudError> {
        if let Some(existing) =
            Self::replay_operation(state, owner_id, machine_id, idempotency_key, kind)?
        {
            return Ok(existing);
        }

        let result = {
            let machine = state.machines.get_mut(machine_id).ok_or(CloudError::NotFound)?;
            Self::validate_owner(machine, owner_id)?;

            machine.state = match (kind, machine.state) {
                (MachineOperationKind::Start, MachineState::Stopped)
                | (MachineOperationKind::Start, MachineState::Suspended)
                | (MachineOperationKind::Start, MachineState::Ready) => MachineState::Ready,
                (MachineOperationKind::Stop, MachineState::Ready)
                | (MachineOperationKind::Stop, MachineState::Suspended)
                | (MachineOperationKind::Stop, MachineState::Stopped) => MachineState::Stopped,
                (MachineOperationKind::Delete, MachineState::Deleted) => MachineState::Deleted,
                (MachineOperationKind::Delete, _) => MachineState::Deleted,
                _ => {
                    return Err(CloudError::InvalidTransition {
                        from: machine.state,
                        operation: kind,
                    })
                }
            };
            machine.generation += 1;
            machine.failure_reason = None;
            machine.clone()
        };

        Self::record_operation(state, idempotency_key, &result, kind);
        Ok(result)
    }
}

impl ComputeProvider for FakeComputeProvider {
    fn create_machine(&self, request: CreateMachineRequest) -> Result<MachineRecord, CloudError> {
        let mut state = self.lock()?;
        if let Some(existing) = Self::replay_operation(
            &state,
            &request.owner_id,
            &request.machine_id,
            &request.idempotency_key,
            MachineOperationKind::Create,
        )? {
            return Ok(existing);
        }

        if let Some(existing) = state.machines.get(&request.machine_id) {
            Self::validate_owner(existing, &request.owner_id)?;
            return Err(CloudError::IdempotencyConflict);
        }

        let CreateMachineRequest {
            machine_id,
            owner_id,
            project_id,
            workspace_id,
            agent_kind,
            encrypted_volume_ref,
            idempotency_key,
        } = request;

        let machine = MachineRecord {
            machine_id,
            owner_id,
            project_id,
            workspace_id,
            agent_kind,
            encrypted_volume_ref,
            state: MachineState::Ready,
            generation: 1,
            failure_reason: None,
        };
        state.machines.insert(machine.machine_id.clone(), machine.clone());
        Self::record_operation(
            &mut state,
            &idempotency_key,
            &machine,
            MachineOperationKind::Create,
        );
        Ok(machine)
    }

    fn start_machine(
        &self,
        owner_id: &str,
        machine_id: &str,
        idempotency_key: &str,
    ) -> Result<MachineRecord, CloudError> {
        let mut state = self.lock()?;
        Self::mutate_machine(
            &mut state,
            owner_id,
            machine_id,
            idempotency_key,
            MachineOperationKind::Start,
        )
    }

    fn stop_machine(
        &self,
        owner_id: &str,
        machine_id: &str,
        idempotency_key: &str,
    ) -> Result<MachineRecord, CloudError> {
        let mut state = self.lock()?;
        Self::mutate_machine(
            &mut state,
            owner_id,
            machine_id,
            idempotency_key,
            MachineOperationKind::Stop,
        )
    }

    fn delete_machine(
        &self,
        owner_id: &str,
        machine_id: &str,
        idempotency_key: &str,
    ) -> Result<MachineRecord, CloudError> {
        let mut state = self.lock()?;
        Self::mutate_machine(
            &mut state,
            owner_id,
            machine_id,
            idempotency_key,
            MachineOperationKind::Delete,
        )
    }

    fn get_machine(&self, owner_id: &str, machine_id: &str) -> Result<MachineRecord, CloudError> {
        let state = self.lock()?;
        let machine = state.machines.get(machine_id).ok_or(CloudError::NotFound)?;
        Self::validate_owner(machine, owner_id)?;
        Ok(machine.clone())
    }

    fn list_machines(&self, owner_id: &str) -> Result<Vec<MachineRecord>, CloudError> {
        let state = self.lock()?;
        Ok(state
            .machines
            .values()
            .filter(|machine| machine.owner_id == owner_id)
            .cloned()
            .collect())
    }

    fn append_task_event(
        &self,
        owner_id: &str,
        machine_id: &str,
        session_id: &str,
        task_id: &str,
        kind: &str,
        payload: &str,
    ) -> Result<TaskEvent, CloudError> {
        let mut state = self.lock()?;
        {
            let machine = state.machines.get(machine_id).ok_or(CloudError::NotFound)?;
            Self::validate_owner(machine, owner_id)?;
            if machine.state == MachineState::Deleted {
                return Err(CloudError::InvalidTransition {
                    from: machine.state,
                    operation: MachineOperationKind::Delete,
                });
            }
        }

        state.next_sequence += 1;
        let event = TaskEvent {
            sequence: state.next_sequence,
            machine_id: machine_id.to_string(),
            owner_id: owner_id.to_string(),
            session_id: session_id.to_string(),
            task_id: task_id.to_string(),
            kind: kind.to_string(),
            payload: payload.to_string(),
        };
        state
            .events
            .entry(machine_id.to_string())
            .or_default()
            .push(event.clone());
        Ok(event)
    }

    fn task_events_since(
        &self,
        owner_id: &str,
        machine_id: &str,
        after_sequence: u64,
    ) -> Result<Vec<TaskEvent>, CloudError> {
        let state = self.lock()?;
        let machine = state.machines.get(machine_id).ok_or(CloudError::NotFound)?;
        Self::validate_owner(machine, owner_id)?;
        Ok(state
            .events
            .get(machine_id)
            .into_iter()
            .flatten()
            .filter(|event| event.sequence > after_sequence)
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> CreateMachineRequest {
        CreateMachineRequest {
            machine_id: "machine-1".into(),
            owner_id: "owner-a".into(),
            project_id: "project-a".into(),
            workspace_id: "workspace-a".into(),
            agent_kind: AgentKind::Codex,
            encrypted_volume_ref: Some("volume://machine-1".into()),
            idempotency_key: "create-1".into(),
        }
    }

    #[test]
    fn create_is_idempotent() {
        let provider = FakeComputeProvider::new();
        let first = provider.create_machine(request()).unwrap();
        let second = provider.create_machine(request()).unwrap();
        assert_eq!(first, second);
        assert_eq!(provider.list_machines("owner-a").unwrap().len(), 1);
    }

    #[test]
    fn owner_isolation_fails_closed() {
        let provider = FakeComputeProvider::new();
        provider.create_machine(request()).unwrap();
        assert_eq!(
            provider.get_machine("owner-b", "machine-1"),
            Err(CloudError::Forbidden)
        );
        assert!(provider.list_machines("owner-b").unwrap().is_empty());
    }

    #[test]
    fn lifecycle_operations_are_replay_safe() {
        let provider = FakeComputeProvider::new();
        provider.create_machine(request()).unwrap();
        let stopped = provider.stop_machine("owner-a", "machine-1", "stop-1").unwrap();
        let stopped_again = provider.stop_machine("owner-a", "machine-1", "stop-1").unwrap();
        assert_eq!(stopped, stopped_again);
        assert_eq!(stopped.state, MachineState::Stopped);

        let started = provider.start_machine("owner-a", "machine-1", "start-1").unwrap();
        assert_eq!(started.state, MachineState::Ready);

        let deleted = provider.delete_machine("owner-a", "machine-1", "delete-1").unwrap();
        assert_eq!(deleted.state, MachineState::Deleted);
    }

    #[test]
    fn reconnect_replays_only_unseen_task_events() {
        let provider = FakeComputeProvider::new();
        provider.create_machine(request()).unwrap();
        let first = provider
            .append_task_event(
                "owner-a",
                "machine-1",
                "session-1",
                "task-1",
                "accepted",
                "started",
            )
            .unwrap();
        provider
            .append_task_event(
                "owner-a",
                "machine-1",
                "session-1",
                "task-1",
                "log",
                "working",
            )
            .unwrap();

        let replay = provider
            .task_events_since("owner-a", "machine-1", first.sequence)
            .unwrap();
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].payload, "working");
    }
}
