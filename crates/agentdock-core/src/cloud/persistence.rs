//! SQLite-backed, single-host fake registry.
//!
//! Every operation reloads the authoritative snapshot inside BEGIN IMMEDIATE.
//! The snapshot and idempotency receipts/event cursor commit atomically. There
//! are no cloud side effects to infer or replay: reconciliation only quarantines
//! interrupted local state; a real provider needs its own observation contract.
use super::*;
use rusqlite::{params, Connection, TransactionBehavior};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

const SCHEMA_VERSION: i64 = 1;
const INTERRUPTED: &str =
    "interrupted local transition; provider state has not been independently verified";

fn storage(error: impl fmt::Display) -> CloudError {
    CloudError::Storage(error.to_string())
}

fn corrupt(message: impl Into<String>) -> CloudError {
    CloudError::CorruptRegistry(message.into())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationReport {
    pub inspected: usize,
    pub quarantined_machine_ids: Vec<String>,
}

/// A durable *local fake* provider. A separate handle/process reads the same
/// SQLite database, not a stale in-memory cache. This is not a VM adapter.
#[derive(Debug, Clone)]
pub struct DurableComputeProvider {
    path: PathBuf,
}

impl DurableComputeProvider {
    /// The containing directory must already exist and be trusted. Never put
    /// API tokens or plaintext credentials in this registry.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, CloudError> {
        let provider = Self {
            path: path.as_ref().to_path_buf(),
        };
        let connection = provider.connect()?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
                 CREATE TABLE IF NOT EXISTS cloud_registry (
                   singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                   schema_version INTEGER NOT NULL,
                   snapshot TEXT NOT NULL
                 );",
            )
            .map_err(storage)?;
        let initial = serde_json::to_string(&FakeState::default()).map_err(storage)?;
        connection
            .execute(
                "INSERT OR IGNORE INTO cloud_registry (singleton, schema_version, snapshot)
                 VALUES (1, ?1, ?2)",
                params![SCHEMA_VERSION, initial],
            )
            .map_err(storage)?;
        drop(connection);
        provider.reconcile()?;
        Ok(provider)
    }

    fn connect(&self) -> Result<Connection, CloudError> {
        let connection = Connection::open(&self.path).map_err(storage)?;
        connection
            .busy_timeout(Duration::from_secs(10))
            .map_err(storage)?;
        connection
            .execute_batch("PRAGMA synchronous=FULL;")
            .map_err(storage)?;
        Ok(connection)
    }

    fn transact<T>(
        &self,
        write: bool,
        operation: impl FnOnce(&FakeComputeProvider) -> Result<T, CloudError>,
    ) -> Result<T, CloudError> {
        let mut connection = self.connect()?;
        // A single SQLite writer prevents lost updates between independent
        // handles/processes, including for the global task-event cursor.
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(storage)?;
        let (version, snapshot): (i64, String) = transaction
            .query_row(
                "SELECT schema_version, snapshot FROM cloud_registry WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(storage)?;
        if version != SCHEMA_VERSION {
            return Err(corrupt(format!("unsupported cloud registry version {version}")));
        }
        let state: FakeState = serde_json::from_str(&snapshot)
            .map_err(|error| corrupt(format!("invalid registry snapshot: {error}")))?;
        validate(&state)?;
        let fake = FakeComputeProvider {
            state: Arc::new(Mutex::new(state)),
        };
        let result = operation(&fake)?;
        if write {
            let state = fake.lock()?.clone();
            validate(&state)?;
            let json = serde_json::to_string(&state).map_err(storage)?;
            transaction
                .execute(
                    "UPDATE cloud_registry SET snapshot = ?1 WHERE singleton = 1",
                    params![json],
                )
                .map_err(storage)?;
        }
        transaction.commit().map_err(storage)?;
        Ok(result)
    }

    /// On restart, incomplete *local* transitions are quarantined rather than
    /// optimistically marked ready/deleted. Stable fake states are retained.
    /// This never probes, provisions, or certifies an external provider.
    pub fn reconcile(&self) -> Result<ReconciliationReport, CloudError> {
        self.transact(true, |fake| {
            let mut state = fake.lock()?;
            let mut report = ReconciliationReport {
                inspected: state.machines.len(),
                quarantined_machine_ids: Vec::new(),
            };
            for machine in state.machines.values_mut() {
                if matches!(
                    machine.state,
                    MachineState::Requested
                        | MachineState::Provisioning
                        | MachineState::Stopping
                        | MachineState::Deleting
                ) {
                    machine.state = MachineState::Failed;
                    machine.generation = machine
                        .generation
                        .checked_add(1)
                        .ok_or_else(|| corrupt("machine generation overflow"))?;
                    machine.failure_reason = Some(INTERRUPTED.to_owned());
                    report
                        .quarantined_machine_ids
                        .push(machine.machine_id.clone());
                }
            }
            Ok(report)
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn validate(state: &FakeState) -> Result<(), CloudError> {
    for (key, machine) in &state.machines {
        if key != &machine.machine_id
            || machine.machine_id.is_empty()
            || machine.owner_id.is_empty()
            || machine.project_id.is_empty()
            || machine.workspace_id.is_empty()
            || machine.generation == 0
        {
            return Err(corrupt("invalid machine identity or generation"));
        }
    }
    for (key, receipt) in &state.operations {
        let machine = state
            .machines
            .get(&receipt.machine_id)
            .ok_or_else(|| corrupt("orphan operation receipt"))?;
        if key != &receipt.idempotency_key
            || receipt.owner_id != machine.owner_id
            || receipt.generation == 0
            || receipt.generation > machine.generation
        {
            return Err(corrupt("invalid operation receipt or ownership"));
        }
    }
    let mut sequences = BTreeSet::new();
    for (machine_id, events) in &state.events {
        let machine = state
            .machines
            .get(machine_id)
            .ok_or_else(|| corrupt("orphan event stream"))?;
        let mut prior = 0;
        for event in events {
            if event.machine_id != *machine_id
                || event.owner_id != machine.owner_id
                || event.sequence <= prior
                || event.sequence > state.next_sequence
                || !sequences.insert(event.sequence)
            {
                return Err(corrupt("invalid event cursor or ownership"));
            }
            prior = event.sequence;
        }
    }
    Ok(())
}

impl ComputeProvider for DurableComputeProvider {
    fn create_machine(&self, request: CreateMachineRequest) -> Result<MachineRecord, CloudError> {
        self.transact(true, |fake| fake.create_machine(request))
    }

    fn start_machine(
        &self,
        owner_id: &str,
        machine_id: &str,
        idempotency_key: &str,
    ) -> Result<MachineRecord, CloudError> {
        self.transact(true, |fake| {
            fake.start_machine(owner_id, machine_id, idempotency_key)
        })
    }

    fn stop_machine(
        &self,
        owner_id: &str,
        machine_id: &str,
        idempotency_key: &str,
    ) -> Result<MachineRecord, CloudError> {
        self.transact(true, |fake| {
            fake.stop_machine(owner_id, machine_id, idempotency_key)
        })
    }

    fn delete_machine(
        &self,
        owner_id: &str,
        machine_id: &str,
        idempotency_key: &str,
    ) -> Result<MachineRecord, CloudError> {
        self.transact(true, |fake| {
            fake.delete_machine(owner_id, machine_id, idempotency_key)
        })
    }

    fn get_machine(&self, owner_id: &str, machine_id: &str) -> Result<MachineRecord, CloudError> {
        self.transact(false, |fake| fake.get_machine(owner_id, machine_id))
    }

    fn list_machines(&self, owner_id: &str) -> Result<Vec<MachineRecord>, CloudError> {
        self.transact(false, |fake| fake.list_machines(owner_id))
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
        self.transact(true, |fake| {
            fake.append_task_event(
                owner_id, machine_id, session_id, task_id, kind, payload,
            )
        })
    }

    fn task_events_since(
        &self,
        owner_id: &str,
        machine_id: &str,
        after_sequence: u64,
    ) -> Result<Vec<TaskEvent>, CloudError> {
        self.transact(false, |fake| {
            fake.task_events_since(owner_id, machine_id, after_sequence)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AgentKind;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Barrier};

    static NEXT_TEST: AtomicU64 = AtomicU64::new(0);

    struct TestStore {
        dir: PathBuf,
        path: PathBuf,
    }

    impl TestStore {
        fn new() -> Self {
            let id = NEXT_TEST.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "agentdock-cloud-{}-{id}",
                std::process::id()
            ));
            std::fs::create_dir(&dir).unwrap();
            let path = dir.join("registry.sqlite3");
            Self { dir, path }
        }

        fn open(&self) -> DurableComputeProvider {
            DurableComputeProvider::open(&self.path).unwrap()
        }
    }

    impl Drop for TestStore {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.dir).unwrap();
        }
    }

    fn request(id: &str, owner: &str, key: &str) -> CreateMachineRequest {
        CreateMachineRequest {
            machine_id: id.into(),
            owner_id: owner.into(),
            project_id: format!("{owner}-project"),
            workspace_id: format!("{owner}-workspace"),
            agent_kind: AgentKind::Codex,
            encrypted_volume_ref: Some(format!("test-volume://{id}")),
            idempotency_key: key.into(),
        }
    }

    #[test]
    fn restart_recovers_receipts_machine_state_and_event_cursor() {
        let store = TestStore::new();
        let first = store.open();
        let created = first
            .create_machine(request("m1", "alice", "create-1"))
            .unwrap();
        let first_event = first
            .append_task_event("alice", "m1", "s1", "t1", "accepted", "one")
            .unwrap();
        first.stop_machine("alice", "m1", "stop-1").unwrap();
        drop(first);

        let restarted = store.open();
        assert_eq!(
            restarted.get_machine("alice", "m1").unwrap().state,
            MachineState::Stopped
        );
        // Replaying an old idempotency key returns its original generation/state.
        assert_eq!(
            restarted
                .create_machine(request("m1", "alice", "create-1"))
                .unwrap(),
            created
        );
        assert_eq!(
            restarted.stop_machine("alice", "m1", "stop-1").unwrap().generation,
            2
        );
        let new_event = restarted
            .append_task_event("alice", "m1", "s1", "t1", "log", "two")
            .unwrap();
        assert!(new_event.sequence > first_event.sequence);
        assert_eq!(
            restarted
                .task_events_since("alice", "m1", first_event.sequence)
                .unwrap(),
            vec![new_event]
        );
        assert_eq!(
            restarted.reconcile().unwrap().quarantined_machine_ids,
            Vec::<String>::new()
        );
    }

    #[test]
    fn conflicting_create_spec_and_cross_owner_reuse_fail_closed_after_restart() {
        let store = TestStore::new();
        store
            .open()
            .create_machine(request("m1", "alice", "create-1"))
            .unwrap();
        let provider = store.open();
        let mut changed = request("m1", "alice", "create-1");
        changed.workspace_id = "different".into();
        assert_eq!(
            provider.create_machine(changed),
            Err(CloudError::IdempotencyConflict)
        );
        assert_eq!(
            provider.create_machine(request("m1", "bob", "create-1")),
            Err(CloudError::IdempotencyConflict)
        );
        assert_eq!(
            provider.get_machine("bob", "m1"),
            Err(CloudError::Forbidden)
        );
        assert_eq!(
            provider.stop_machine("bob", "m1", "stop-b"),
            Err(CloudError::Forbidden)
        );
        assert_eq!(
            provider.append_task_event("bob", "m1", "s", "t", "log", "bad"),
            Err(CloudError::Forbidden)
        );
        assert_eq!(
            provider.task_events_since("bob", "m1", 0),
            Err(CloudError::Forbidden)
        );
        assert!(provider.list_machines("bob").unwrap().is_empty());
        assert_eq!(provider.get_machine("alice", "m1").unwrap().generation, 1);
    }

    #[test]
    fn two_independent_handles_serialize_writes_and_preserve_global_cursors() {
        let store = TestStore::new();
        let a = store.open();
        let b = store.open();
        let gate = Arc::new(Barrier::new(3));
        let mut workers = Vec::new();
        for provider in [a, b] {
            let gate = Arc::clone(&gate);
            workers.push(std::thread::spawn(move || {
                gate.wait();
                provider
                    .create_machine(request("m1", "alice", "create-1"))
                    .unwrap();
                for n in 0..10 {
                    provider
                        .append_task_event(
                            "alice",
                            "m1",
                            "s1",
                            "t1",
                            "log",
                            &n.to_string(),
                        )
                        .unwrap();
                }
            }));
        }
        gate.wait();
        for worker in workers {
            worker.join().unwrap();
        }
        let third = store.open();
        assert_eq!(third.list_machines("alice").unwrap().len(), 1);
        let events = third.task_events_since("alice", "m1", 0).unwrap();
        assert_eq!(events.len(), 20);
        for window in events.windows(2) {
            assert!(window[0].sequence < window[1].sequence);
        }
    }

    #[test]
    fn failed_transition_does_not_commit_a_partial_snapshot() {
        let store = TestStore::new();
        let provider = store.open();
        provider
            .create_machine(request("m1", "alice", "create-1"))
            .unwrap();
        provider.delete_machine("alice", "m1", "delete-1").unwrap();
        assert!(matches!(
            provider.start_machine("alice", "m1", "start-invalid"),
            Err(CloudError::InvalidTransition { .. })
        ));
        let restarted = store.open();
        assert_eq!(
            restarted.get_machine("alice", "m1").unwrap().state,
            MachineState::Deleted
        );
        assert_eq!(restarted.get_machine("alice", "m1").unwrap().generation, 2);
        assert!(restarted
            .task_events_since("alice", "m1", 0)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn restart_quarantines_interrupted_states_once_not_as_success() {
        let store = TestStore::new();
        let provider = store.open();
        provider
            .create_machine(request("m1", "alice", "create-1"))
            .unwrap();
        drop(provider);
        let conn = Connection::open(&store.path).unwrap();
        let snapshot: String = conn
            .query_row(
                "SELECT snapshot FROM cloud_registry WHERE singleton = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let mut state: FakeState = serde_json::from_str(&snapshot).unwrap();
        state.machines.get_mut("m1").unwrap().state = MachineState::Provisioning;
        conn.execute(
            "UPDATE cloud_registry SET snapshot = ?1 WHERE singleton = 1",
            params![serde_json::to_string(&state).unwrap()],
        )
        .unwrap();
        drop(conn);

        let recovered = store.open();
        let machine = recovered.get_machine("alice", "m1").unwrap();
        assert_eq!(machine.state, MachineState::Failed);
        assert_eq!(machine.generation, 2);
        assert_eq!(machine.failure_reason.as_deref(), Some(INTERRUPTED));
        assert!(store.open().reconcile().unwrap().quarantined_machine_ids.is_empty());
        assert_eq!(
            store.open().get_machine("alice", "m1").unwrap().generation,
            2
        );
    }

    #[test]
    fn invalid_snapshot_or_schema_is_rejected_without_reset() {
        let store = TestStore::new();
        let provider = store.open();
        drop(provider);
        let connection = Connection::open(&store.path).unwrap();
        connection
            .execute(
                "UPDATE cloud_registry SET snapshot = 'not-json' WHERE singleton = 1",
                [],
            )
            .unwrap();
        assert!(matches!(
            DurableComputeProvider::open(&store.path),
            Err(CloudError::CorruptRegistry(_))
        ));
        connection
            .execute(
                "UPDATE cloud_registry SET schema_version = 999 WHERE singleton = 1",
                [],
            )
            .unwrap();
        assert!(matches!(
            DurableComputeProvider::open(&store.path),
            Err(CloudError::CorruptRegistry(_))
        ));
    }
}
