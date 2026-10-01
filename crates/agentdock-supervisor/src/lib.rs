use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

pub const SUPERVISOR_SNAPSHOT_SCHEMA: &str = "supervisor-snapshot/v1";
pub const SUPERVISOR_CONTROL_SCHEMA: &str = "supervisor-control/v1";
pub const SUPERVISOR_CONTROL_RECEIPT_SCHEMA: &str = "supervisor-control-receipt/v1";

const MAX_PROJECTS: usize = 2_000;
const MAX_WORKERS: usize = 512;
const MAX_TASKS: usize = 20_000;
const MAX_RISKS: usize = 2_000;
const MAX_DECISIONS: usize = 5_000;
const MAX_TEXT: usize = 16_000;

#[derive(Debug, Error)]
pub enum SupervisorError {
    #[error("supervisor I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid supervisor snapshot json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported supervisor snapshot schema: {0}")]
    UnsupportedSchema(String),
    #[error("invalid supervisor snapshot: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SupervisorSnapshot {
    pub schema_version: String,
    pub generated_at: Option<String>,
    pub supervisor_state: String,
    #[serde(default)]
    pub projects: Vec<ProjectView>,
    #[serde(default)]
    pub workers: Vec<WorkerView>,
    #[serde(default)]
    pub tasks: Vec<TaskView>,
    #[serde(default)]
    pub risks: Vec<RiskView>,
    #[serde(default)]
    pub decisions: Vec<DecisionView>,
    pub machine: Option<MachineView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProjectView {
    pub id: String,
    pub name: String,
    pub repo: Option<String>,
    pub priority: i64,
    pub status: String,
    pub next_action: Option<String>,
    pub blocker: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WorkerView {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TaskView {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub kind: String,
    pub status: String,
    pub priority: i64,
    pub blocker: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RiskView {
    pub severity: String,
    pub code: String,
    pub message: String,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DecisionView {
    pub id: String,
    pub action: String,
    pub reason: String,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MachineView {
    pub pressure: String,
    #[serde(default)]
    pub reasons: Vec<String>,
    pub load_average_1m: Option<f64>,
    pub cpu_speed_limit_percent: Option<f64>,
    pub memory_used_mb: Option<f64>,
    pub memory_total_mb: Option<f64>,
    pub swap_used_mb: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SupervisorControlRequest {
    pub schema_version: String,
    pub request_id: String,
    pub action: String,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub priority: Option<i64>,
}

impl SupervisorControlRequest {
    pub fn pause_project(request_id: String, project_id: String) -> Result<Self, SupervisorError> {
        Self::project_action(request_id, "pause-project", project_id, None)
    }

    pub fn resume_project(request_id: String, project_id: String) -> Result<Self, SupervisorError> {
        Self::project_action(request_id, "resume-project", project_id, None)
    }

    pub fn set_project_priority(
        request_id: String,
        project_id: String,
        priority: i64,
    ) -> Result<Self, SupervisorError> {
        Self::project_action(
            request_id,
            "set-project-priority",
            project_id,
            Some(priority),
        )
    }

    pub fn set_task_priority(
        request_id: String,
        task_id: String,
        priority: i64,
    ) -> Result<Self, SupervisorError> {
        let request = Self {
            schema_version: SUPERVISOR_CONTROL_SCHEMA.into(),
            request_id,
            action: "set-task-priority".into(),
            project_id: None,
            task_id: Some(task_id),
            priority: Some(priority),
        };
        request.validate()?;
        Ok(request)
    }

    fn project_action(
        request_id: String,
        action: &str,
        project_id: String,
        priority: Option<i64>,
    ) -> Result<Self, SupervisorError> {
        let request = Self {
            schema_version: SUPERVISOR_CONTROL_SCHEMA.into(),
            request_id,
            action: action.into(),
            project_id: Some(project_id),
            task_id: None,
            priority,
        };
        request.validate()?;
        Ok(request)
    }

    pub fn validate(&self) -> Result<(), SupervisorError> {
        if self.schema_version != SUPERVISOR_CONTROL_SCHEMA {
            return Err(SupervisorError::Invalid(
                "unsupported supervisor control schema".into(),
            ));
        }
        validate_id("request_id", &self.request_id)?;
        if self.request_id.len() > 112 {
            return Err(SupervisorError::Invalid(
                "request_id cannot exceed 112 characters".into(),
            ));
        }

        match self.action.as_str() {
            "pause-project" | "resume-project" => {
                let project_id = self.project_id.as_deref().ok_or_else(|| {
                    SupervisorError::Invalid(format!("{} requires project_id", self.action))
                })?;
                validate_id("project_id", project_id)?;
                if self.task_id.is_some() || self.priority.is_some() {
                    return Err(SupervisorError::Invalid(format!(
                        "{} accepts project_id only",
                        self.action
                    )));
                }
            }
            "set-project-priority" => {
                let project_id = self.project_id.as_deref().ok_or_else(|| {
                    SupervisorError::Invalid("set-project-priority requires project_id".into())
                })?;
                validate_id("project_id", project_id)?;
                if self.task_id.is_some() {
                    return Err(SupervisorError::Invalid(
                        "set-project-priority does not accept task_id".into(),
                    ));
                }
                validate_control_priority(self.priority)?;
            }
            "set-task-priority" => {
                let task_id = self.task_id.as_deref().ok_or_else(|| {
                    SupervisorError::Invalid("set-task-priority requires task_id".into())
                })?;
                validate_id("task_id", task_id)?;
                if self.project_id.is_some() {
                    return Err(SupervisorError::Invalid(
                        "set-task-priority does not accept project_id".into(),
                    ));
                }
                validate_control_priority(self.priority)?;
            }
            _ => {
                return Err(SupervisorError::Invalid(
                    "unsupported supervisor control action".into(),
                ));
            }
        }
        Ok(())
    }
}

pub fn new_control_request_id() -> Result<String, SupervisorError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| SupervisorError::Invalid("system clock is before Unix epoch".into()))?
        .as_nanos();
    let request_id = format!("req-{nanos:x}-{}", std::process::id());
    validate_id("request_id", &request_id)?;
    Ok(request_id)
}

pub fn write_control_request(
    root: impl AsRef<Path>,
    request: &SupervisorControlRequest,
) -> Result<PathBuf, SupervisorError> {
    request.validate()?;
    let root = fs::canonicalize(root.as_ref())?;
    if !root.is_dir() {
        return Err(SupervisorError::Invalid(
            "Forge root must be a directory".into(),
        ));
    }

    let ai_dir = root.join(".ai");
    let supervisor_dir = ai_dir.join("supervisor");
    let request_dir = supervisor_dir.join("requests");
    for path in [&ai_dir, &supervisor_dir, &request_dir] {
        refuse_symlink(path)?;
    }
    fs::create_dir_all(&request_dir)?;
    for path in [&ai_dir, &supervisor_dir, &request_dir] {
        refuse_symlink(path)?;
    }

    let resolved_dir = fs::canonicalize(&request_dir)?;
    if !resolved_dir.starts_with(&root) {
        return Err(SupervisorError::Invalid(
            "supervisor request directory escaped Forge root".into(),
        ));
    }

    let final_path = resolved_dir.join(format!("{}.json", request.request_id));
    if final_path.exists() {
        return Err(SupervisorError::Invalid(format!(
            "supervisor request already exists: {}",
            request.request_id
        )));
    }

    let encoded = serde_json::to_vec_pretty(request)?;
    let temporary = resolved_dir.join(format!(
        ".{}.{}.tmp",
        request.request_id,
        std::process::id()
    ));
    let result = (|| -> Result<(), SupervisorError> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(&encoded)?;
        file.write_all(b"\n")?;
        file.sync_all()?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = file.metadata()?.permissions();
            permissions.set_mode(0o600);
            fs::set_permissions(&temporary, permissions)?;
        }

        drop(file);
        fs::rename(&temporary, &final_path)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    Ok(final_path)
}

fn validate_control_priority(priority: Option<i64>) -> Result<(), SupervisorError> {
    match priority {
        Some(value) if (0..=100).contains(&value) => Ok(()),
        _ => Err(SupervisorError::Invalid(
            "priority must be an integer between 0 and 100".into(),
        )),
    }
}

fn refuse_symlink(path: &Path) -> Result<(), SupervisorError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(SupervisorError::Invalid(format!(
                "supervisor control path must not be a symlink: {}",
                path.display()
            )))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct SupervisorControlState {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SupervisorControlReceipt {
    pub schema_version: String,
    pub request_id: String,
    pub status: String,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub entity_type: Option<String>,
    #[serde(default)]
    pub entity_id: Option<String>,
    #[serde(default)]
    pub before: SupervisorControlState,
    #[serde(default)]
    pub after: SupervisorControlState,
    #[serde(default)]
    pub applied_at: Option<String>,
    #[serde(default)]
    pub replayed: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

impl SupervisorControlReceipt {
    pub fn validate(&self) -> Result<(), SupervisorError> {
        if self.schema_version != SUPERVISOR_CONTROL_RECEIPT_SCHEMA {
            return Err(SupervisorError::Invalid(
                "unsupported supervisor control receipt schema".into(),
            ));
        }
        validate_id("receipt.request_id", &self.request_id)?;
        if self.request_id.len() > 112 {
            return Err(SupervisorError::Invalid(
                "receipt request_id cannot exceed 112 characters".into(),
            ));
        }
        if !matches!(self.status.as_str(), "applied" | "replayed" | "refused") {
            return Err(SupervisorError::Invalid(
                "receipt status must be applied, replayed, or refused".into(),
            ));
        }
        if let Some(priority) = self.before.priority {
            validate_receipt_priority(priority)?;
        }
        if let Some(priority) = self.after.priority {
            validate_receipt_priority(priority)?;
        }
        if let Some(status) = self.before.status.as_deref() {
            validate_text("receipt.before.status", status)?;
        }
        if let Some(status) = self.after.status.as_deref() {
            validate_text("receipt.after.status", status)?;
        }

        if self.status == "refused" {
            let reason = self.reason.as_deref().ok_or_else(|| {
                SupervisorError::Invalid("refused receipt requires reason".into())
            })?;
            validate_text("receipt.reason", reason)?;
            return Ok(());
        }

        let action = self
            .action
            .as_deref()
            .ok_or_else(|| SupervisorError::Invalid("applied receipt requires action".into()))?;
        if !matches!(
            action,
            "pause-project" | "resume-project" | "set-project-priority" | "set-task-priority"
        ) {
            return Err(SupervisorError::Invalid(
                "receipt has unsupported action".into(),
            ));
        }
        let entity_type = self.entity_type.as_deref().ok_or_else(|| {
            SupervisorError::Invalid("applied receipt requires entity_type".into())
        })?;
        if !matches!(entity_type, "project" | "task") {
            return Err(SupervisorError::Invalid(
                "receipt entity_type must be project or task".into(),
            ));
        }
        let entity_id = self
            .entity_id
            .as_deref()
            .ok_or_else(|| SupervisorError::Invalid("applied receipt requires entity_id".into()))?;
        validate_id("receipt.entity_id", entity_id)?;
        if let Some(applied_at) = self.applied_at.as_deref() {
            validate_text("receipt.applied_at", applied_at)?;
        }
        Ok(())
    }
}

pub fn load_control_receipts(
    root: impl AsRef<Path>,
    limit: usize,
) -> Result<Vec<SupervisorControlReceipt>, SupervisorError> {
    if !(1..=100).contains(&limit) {
        return Err(SupervisorError::Invalid(
            "receipt limit must be between 1 and 100".into(),
        ));
    }
    let root = fs::canonicalize(root.as_ref())?;
    if !root.is_dir() {
        return Err(SupervisorError::Invalid(
            "Forge root must be a directory".into(),
        ));
    }
    let ai_dir = root.join(".ai");
    let supervisor_dir = ai_dir.join("supervisor");
    let receipt_dir = supervisor_dir.join("receipts");
    for path in [&ai_dir, &supervisor_dir, &receipt_dir] {
        refuse_symlink(path)?;
    }
    if !receipt_dir.exists() {
        return Ok(Vec::new());
    }

    let resolved_dir = fs::canonicalize(&receipt_dir)?;
    if !resolved_dir.starts_with(&root) {
        return Err(SupervisorError::Invalid(
            "supervisor receipt directory escaped Forge root".into(),
        ));
    }

    let mut paths = Vec::new();
    for entry in fs::read_dir(&resolved_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_symlink() {
            return Err(SupervisorError::Invalid(
                "supervisor receipt symlinks are refused".into(),
            ));
        }
        if path.extension().and_then(|value| value.to_str()) == Some("json") {
            if !entry.file_type()?.is_file() {
                continue;
            }
            paths.push(path);
        }
    }
    paths.sort();
    paths.reverse();
    paths.truncate(limit);

    let mut receipts = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = fs::read(&path)?;
        if bytes.len() > 64 * 1024 {
            return Err(SupervisorError::Invalid(format!(
                "control receipt exceeds 64 KiB: {}",
                path.display()
            )));
        }
        let receipt: SupervisorControlReceipt = serde_json::from_slice(&bytes)?;
        receipt.validate()?;
        if path.file_stem().and_then(|value| value.to_str()) != Some(receipt.request_id.as_str()) {
            return Err(SupervisorError::Invalid(
                "receipt filename must match request_id".into(),
            ));
        }
        receipts.push(receipt);
    }
    Ok(receipts)
}

pub fn render_control_receipts(receipts: &[SupervisorControlReceipt]) -> String {
    let mut out = String::new();
    out.push_str("SUPERVISOR CONTROL RECEIPTS\n");
    if receipts.is_empty() {
        out.push_str("none\n");
        return out;
    }
    for receipt in receipts {
        out.push_str(&format!(
            "{} {} action={} target={}\n",
            receipt.request_id,
            receipt.status,
            receipt.action.as_deref().unwrap_or("-"),
            receipt.entity_id.as_deref().unwrap_or("-")
        ));
        if receipt.status == "refused" {
            if let Some(reason) = receipt.reason.as_deref() {
                out.push_str(&format!("  reason: {reason}\n"));
            }
            continue;
        }
        if receipt.before.status != receipt.after.status {
            out.push_str(&format!(
                "  status: {} -> {}\n",
                receipt.before.status.as_deref().unwrap_or("-"),
                receipt.after.status.as_deref().unwrap_or("-")
            ));
        }
        if receipt.before.priority != receipt.after.priority {
            let before = receipt
                .before
                .priority
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".into());
            let after = receipt
                .after
                .priority
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".into());
            out.push_str(&format!("  priority: {before} -> {after}\n"));
        }
    }
    out
}

fn validate_receipt_priority(priority: i64) -> Result<(), SupervisorError> {
    if !(0..=100).contains(&priority) {
        return Err(SupervisorError::Invalid(
            "receipt priority must be between 0 and 100".into(),
        ));
    }
    Ok(())
}

impl SupervisorSnapshot {
    pub fn validate(&self) -> Result<(), SupervisorError> {
        if self.schema_version != SUPERVISOR_SNAPSHOT_SCHEMA {
            return Err(SupervisorError::UnsupportedSchema(
                self.schema_version.clone(),
            ));
        }
        validate_text("supervisor_state", &self.supervisor_state)?;
        validate_bound("projects", self.projects.len(), MAX_PROJECTS)?;
        validate_bound("workers", self.workers.len(), MAX_WORKERS)?;
        validate_bound("tasks", self.tasks.len(), MAX_TASKS)?;
        validate_bound("risks", self.risks.len(), MAX_RISKS)?;
        validate_bound("decisions", self.decisions.len(), MAX_DECISIONS)?;

        let mut project_ids = std::collections::HashSet::new();
        for project in &self.projects {
            validate_id("project.id", &project.id)?;
            validate_text("project.name", &project.name)?;
            validate_text("project.status", &project.status)?;
            if let Some(repo) = project.repo.as_deref() {
                validate_text("project.repo", repo)?;
            }
            if let Some(value) = project.next_action.as_deref() {
                validate_text("project.next_action", value)?;
            }
            if let Some(value) = project.blocker.as_deref() {
                validate_text("project.blocker", value)?;
            }
            if !project_ids.insert(project.id.as_str()) {
                return Err(SupervisorError::Invalid(format!(
                    "duplicate project id: {}",
                    project.id
                )));
            }
        }

        let mut worker_ids = std::collections::HashSet::new();
        for worker in &self.workers {
            validate_id("worker.id", &worker.id)?;
            validate_text("worker.kind", &worker.kind)?;
            validate_text("worker.status", &worker.status)?;
            if let Some(project_id) = worker.project_id.as_deref() {
                validate_id("worker.project_id", project_id)?;
                require_project(project_id, &project_ids)?;
            }
            if let Some(task_id) = worker.task_id.as_deref() {
                validate_id("worker.task_id", task_id)?;
            }
            if !worker_ids.insert(worker.id.as_str()) {
                return Err(SupervisorError::Invalid(format!(
                    "duplicate worker id: {}",
                    worker.id
                )));
            }
        }

        let mut task_ids = std::collections::HashSet::new();
        for task in &self.tasks {
            validate_id("task.id", &task.id)?;
            validate_id("task.project_id", &task.project_id)?;
            require_project(&task.project_id, &project_ids)?;
            validate_text("task.title", &task.title)?;
            validate_text("task.kind", &task.kind)?;
            validate_text("task.status", &task.status)?;
            if let Some(value) = task.blocker.as_deref() {
                validate_text("task.blocker", value)?;
            }
            if !task_ids.insert(task.id.as_str()) {
                return Err(SupervisorError::Invalid(format!(
                    "duplicate task id: {}",
                    task.id
                )));
            }
        }

        for worker in &self.workers {
            if let Some(task_id) = worker.task_id.as_deref() {
                if !task_ids.contains(task_id) {
                    return Err(SupervisorError::Invalid(format!(
                        "worker {} references unknown task {}",
                        worker.id, task_id
                    )));
                }
            }
        }

        for risk in &self.risks {
            validate_text("risk.severity", &risk.severity)?;
            validate_text("risk.code", &risk.code)?;
            validate_text("risk.message", &risk.message)?;
            if let Some(project_id) = risk.project_id.as_deref() {
                validate_id("risk.project_id", project_id)?;
                require_project(project_id, &project_ids)?;
            }
        }

        let mut decision_ids = std::collections::HashSet::new();
        for decision in &self.decisions {
            validate_id("decision.id", &decision.id)?;
            validate_text("decision.action", &decision.action)?;
            validate_text("decision.reason", &decision.reason)?;
            if let Some(project_id) = decision.project_id.as_deref() {
                validate_id("decision.project_id", project_id)?;
                require_project(project_id, &project_ids)?;
            }
            if let Some(task_id) = decision.task_id.as_deref() {
                validate_id("decision.task_id", task_id)?;
                if !task_ids.contains(task_id) {
                    return Err(SupervisorError::Invalid(format!(
                        "decision {} references unknown task {}",
                        decision.id, task_id
                    )));
                }
            }
            if !decision_ids.insert(decision.id.as_str()) {
                return Err(SupervisorError::Invalid(format!(
                    "duplicate decision id: {}",
                    decision.id
                )));
            }
        }

        if let Some(machine) = &self.machine {
            validate_text("machine.pressure", &machine.pressure)?;
            for reason in &machine.reasons {
                validate_text("machine.reason", reason)?;
            }
            for (name, value) in [
                ("machine.load_average_1m", machine.load_average_1m),
                (
                    "machine.cpu_speed_limit_percent",
                    machine.cpu_speed_limit_percent,
                ),
                ("machine.memory_used_mb", machine.memory_used_mb),
                ("machine.memory_total_mb", machine.memory_total_mb),
                ("machine.swap_used_mb", machine.swap_used_mb),
            ] {
                if let Some(value) = value {
                    if !value.is_finite() || value < 0.0 {
                        return Err(SupervisorError::Invalid(format!(
                            "{name} must be finite and >= 0"
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    pub fn normalized(mut self) -> Self {
        self.projects
            .sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.id.cmp(&b.id)));
        self.workers.sort_by(|a, b| a.id.cmp(&b.id));
        self.tasks.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then_with(|| a.project_id.cmp(&b.project_id))
                .then_with(|| a.id.cmp(&b.id))
        });
        self.risks.sort_by(|a, b| {
            severity_rank(&a.severity)
                .cmp(&severity_rank(&b.severity))
                .then_with(|| a.project_id.cmp(&b.project_id))
                .then_with(|| a.code.cmp(&b.code))
        });
        self.decisions.sort_by(|a, b| a.id.cmp(&b.id));
        self
    }
}

pub fn load_snapshot(path: impl AsRef<Path>) -> Result<SupervisorSnapshot, SupervisorError> {
    let bytes = fs::read(path)?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(SupervisorError::Invalid("snapshot exceeds 8 MiB".into()));
    }
    let snapshot: SupervisorSnapshot = serde_json::from_slice(&bytes)?;
    snapshot.validate()?;
    Ok(snapshot.normalized())
}

pub fn render_status(snapshot: &SupervisorSnapshot) -> String {
    let running = snapshot
        .workers
        .iter()
        .filter(|worker| worker.status == "running")
        .count();
    let blocked_tasks = snapshot
        .tasks
        .iter()
        .filter(|task| task.status == "blocked" || task.blocker.is_some())
        .count();
    let ready_tasks = snapshot
        .tasks
        .iter()
        .filter(|task| task.status == "todo" && task.blocker.is_none())
        .count();
    let critical_risks = snapshot
        .risks
        .iter()
        .filter(|risk| severity_rank(&risk.severity) == 0)
        .count();

    let mut out = String::new();
    out.push_str("AgentDock Supervisor\n");
    out.push_str(&format!("  state: {}\n", snapshot.supervisor_state));
    out.push_str(&format!("  projects: {}\n", snapshot.projects.len()));
    out.push_str(&format!(
        "  workers: {} (running {})\n",
        snapshot.workers.len(),
        running
    ));
    out.push_str(&format!(
        "  tasks: {} (ready {}, blocked {})\n",
        snapshot.tasks.len(),
        ready_tasks,
        blocked_tasks
    ));
    out.push_str(&format!(
        "  risks: {} (critical {})\n",
        snapshot.risks.len(),
        critical_risks
    ));
    if let Some(machine) = &snapshot.machine {
        out.push_str(&format!("  machine pressure: {}\n", machine.pressure));
        if let Some(load) = machine.load_average_1m {
            out.push_str(&format!("  load 1m: {:.2}\n", load));
        }
        if let Some(limit) = machine.cpu_speed_limit_percent {
            out.push_str(&format!("  cpu speed limit: {:.1}%\n", limit));
        }
    }
    out
}

pub fn render_projects(snapshot: &SupervisorSnapshot) -> String {
    let mut out = String::new();
    out.push_str("PROJECTS\n");
    for project in &snapshot.projects {
        out.push_str(&format!(
            "[p={}] {} ({})\n",
            project.priority, project.name, project.status
        ));
        if let Some(next) = project.next_action.as_deref() {
            out.push_str(&format!("  next: {next}\n"));
        }
        if let Some(blocker) = project.blocker.as_deref() {
            out.push_str(&format!("  blocker: {blocker}\n"));
        }
    }
    out
}

pub fn render_workers(snapshot: &SupervisorSnapshot) -> String {
    let mut out = String::new();
    out.push_str("WORKERS\n");
    for worker in &snapshot.workers {
        out.push_str(&format!(
            "{} [{}] {} project={} task={}\n",
            worker.id,
            worker.kind,
            worker.status,
            worker.project_id.as_deref().unwrap_or("-"),
            worker.task_id.as_deref().unwrap_or("-")
        ));
    }
    out
}

pub fn render_tasks(snapshot: &SupervisorSnapshot) -> String {
    let mut out = String::new();
    out.push_str("TASKS\n");
    for task in &snapshot.tasks {
        out.push_str(&format!(
            "[p={}] {} {} [{}] {}\n",
            task.priority, task.project_id, task.id, task.kind, task.status
        ));
        out.push_str(&format!("  {}\n", task.title));
        if let Some(blocker) = task.blocker.as_deref() {
            out.push_str(&format!("  blocker: {blocker}\n"));
        }
    }
    out
}

pub fn render_risks(snapshot: &SupervisorSnapshot) -> String {
    let mut out = String::new();
    out.push_str("RISKS\n");
    for risk in &snapshot.risks {
        out.push_str(&format!(
            "{} {} project={}\n  {}\n",
            risk.severity.to_uppercase(),
            risk.code,
            risk.project_id.as_deref().unwrap_or("-"),
            risk.message
        ));
    }
    out
}

pub fn render_decisions(snapshot: &SupervisorSnapshot) -> String {
    let mut out = String::new();
    out.push_str("RECENT DECISIONS\n");
    for decision in &snapshot.decisions {
        out.push_str(&format!(
            "{} {} project={} task={}\n  {}\n",
            decision.id,
            decision.action,
            decision.project_id.as_deref().unwrap_or("-"),
            decision.task_id.as_deref().unwrap_or("-"),
            decision.reason
        ));
    }
    out
}

fn validate_bound(name: &str, value: usize, maximum: usize) -> Result<(), SupervisorError> {
    if value > maximum {
        return Err(SupervisorError::Invalid(format!(
            "{name} exceeds maximum of {maximum}"
        )));
    }
    Ok(())
}

fn validate_text(name: &str, value: &str) -> Result<(), SupervisorError> {
    if value.trim().is_empty() || value.len() > MAX_TEXT || value.contains('\0') {
        return Err(SupervisorError::Invalid(format!(
            "{name} must be non-empty, bounded text"
        )));
    }
    Ok(())
}

fn validate_id(name: &str, value: &str) -> Result<(), SupervisorError> {
    validate_text(name, value)?;
    if value.len() > 128
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':'))
    {
        return Err(SupervisorError::Invalid(format!(
            "{name} has unsafe identifier syntax"
        )));
    }
    Ok(())
}

fn require_project(
    project_id: &str,
    project_ids: &std::collections::HashSet<&str>,
) -> Result<(), SupervisorError> {
    if !project_ids.contains(project_id) {
        return Err(SupervisorError::Invalid(format!(
            "unknown project reference: {project_id}"
        )));
    }
    Ok(())
}

fn severity_rank(value: &str) -> u8 {
    match value.to_ascii_lowercase().as_str() {
        "critical" => 0,
        "high" => 1,
        "medium" => 2,
        "low" => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SupervisorSnapshot {
        SupervisorSnapshot {
            schema_version: SUPERVISOR_SNAPSHOT_SCHEMA.into(),
            generated_at: Some("2026-10-01T20:00:00Z".into()),
            supervisor_state: "running".into(),
            projects: vec![
                ProjectView {
                    id: "earth".into(),
                    name: "Earth Project".into(),
                    repo: Some("rrahul0904/earth-reference-rebuild".into()),
                    priority: 2,
                    status: "active".into(),
                    next_action: Some("deploy preview".into()),
                    blocker: None,
                },
                ProjectView {
                    id: "applyai".into(),
                    name: "ApplyAI".into(),
                    repo: Some("rrahul0904/applyai".into()),
                    priority: 1,
                    status: "blocked".into(),
                    next_action: Some("resolve operator config".into()),
                    blocker: Some("operator_configured=false".into()),
                },
            ],
            workers: vec![WorkerView {
                id: "coder-a".into(),
                kind: "implement".into(),
                status: "running".into(),
                project_id: Some("earth".into()),
                task_id: Some("T-1".into()),
            }],
            tasks: vec![TaskView {
                id: "T-1".into(),
                project_id: "earth".into(),
                title: "Deploy preview".into(),
                kind: "implement".into(),
                status: "todo".into(),
                priority: 1,
                blocker: None,
            }],
            risks: vec![RiskView {
                severity: "high".into(),
                code: "machine-pressure".into(),
                message: "Playwright is consuming excess CPU".into(),
                project_id: Some("earth".into()),
            }],
            decisions: vec![DecisionView {
                id: "D-1".into(),
                action: "reduce-worker-capacity".into(),
                reason: "machine pressure is high".into(),
                project_id: None,
                task_id: None,
            }],
            machine: Some(MachineView {
                pressure: "high".into(),
                reasons: vec!["high_swap_usage".into()],
                load_average_1m: Some(104.0),
                cpu_speed_limit_percent: Some(93.0),
                memory_used_mb: Some(15_000.0),
                memory_total_mb: Some(16_384.0),
                swap_used_mb: Some(7_200.0),
            }),
        }
    }

    #[test]
    fn control_request_contract_is_action_specific() {
        let pause =
            SupervisorControlRequest::pause_project("req-pause".into(), "applyai".into()).unwrap();
        assert_eq!(pause.action, "pause-project");
        assert_eq!(pause.priority, None);

        let priority =
            SupervisorControlRequest::set_task_priority("req-task".into(), "T-1".into(), 2)
                .unwrap();
        assert_eq!(priority.priority, Some(2));

        assert!(SupervisorControlRequest::set_project_priority(
            "req-bad".into(),
            "applyai".into(),
            101,
        )
        .is_err());
    }

    #[test]
    fn writes_control_request_only_under_supervisor_queue() {
        let base = std::env::temp_dir().join(format!(
            "agentdock-supervisor-control-{}",
            new_control_request_id().unwrap()
        ));
        fs::create_dir_all(&base).unwrap();
        let request =
            SupervisorControlRequest::pause_project("req-write".into(), "applyai".into()).unwrap();

        let path = write_control_request(&base, &request).unwrap();
        assert_eq!(
            path,
            fs::canonicalize(&base)
                .unwrap()
                .join(".ai/supervisor/requests/req-write.json")
        );
        let loaded: SupervisorControlRequest =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(loaded, request);
        assert!(write_control_request(&base, &request).is_err());

        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn loads_and_renders_control_receipts() {
        let base = std::env::temp_dir().join(format!(
            "agentdock-supervisor-receipts-{}",
            new_control_request_id().unwrap()
        ));
        let receipt_dir = base.join(".ai/supervisor/receipts");
        fs::create_dir_all(&receipt_dir).unwrap();
        fs::write(
            receipt_dir.join("req-1.json"),
            br#"{
              "schema_version":"supervisor-control-receipt/v1",
              "request_id":"req-1",
              "status":"applied",
              "action":"pause-project",
              "entity_type":"project",
              "entity_id":"applyai",
              "before":{"status":"active","priority":1},
              "after":{"status":"paused","priority":1},
              "applied_at":"2026-10-01T23:00:00Z",
              "replayed":false,
              "reason":null
            }"#,
        )
        .unwrap();
        fs::write(
            receipt_dir.join("req-2.json"),
            br#"{
              "schema_version":"supervisor-control-receipt/v1",
              "request_id":"req-2",
              "status":"refused",
              "action":null,
              "entity_type":null,
              "entity_id":null,
              "before":{},
              "after":{},
              "applied_at":null,
              "replayed":false,
              "reason":"unknown project"
            }"#,
        )
        .unwrap();

        let receipts = load_control_receipts(&base, 10).unwrap();
        assert_eq!(receipts.len(), 2);
        assert_eq!(receipts[0].request_id, "req-2");
        let rendered = render_control_receipts(&receipts);
        assert!(rendered.contains("req-1 applied"));
        assert!(rendered.contains("status: active -> paused"));
        assert!(rendered.contains("req-2 refused"));
        assert!(rendered.contains("reason: unknown project"));

        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn validates_and_sorts_deterministically() {
        let snapshot = sample();
        snapshot.validate().unwrap();
        let normalized = snapshot.normalized();
        assert_eq!(normalized.projects[0].id, "applyai");
        assert_eq!(normalized.projects[1].id, "earth");
    }

    #[test]
    fn rejects_unknown_project_reference() {
        let mut snapshot = sample();
        snapshot.tasks[0].project_id = "missing".into();
        assert!(snapshot.validate().is_err());
    }

    #[test]
    fn rejects_unknown_fields() {
        let json = r#"{
          "schema_version":"supervisor-snapshot/v1",
          "supervisor_state":"running",
          "projects":[],
          "workers":[],
          "tasks":[],
          "risks":[],
          "decisions":[],
          "machine":null,
          "token":"secret"
        }"#;
        assert!(serde_json::from_str::<SupervisorSnapshot>(json).is_err());
    }

    #[test]
    fn status_summary_is_truthful() {
        let rendered = render_status(&sample());
        assert!(rendered.contains("projects: 2"));
        assert!(rendered.contains("workers: 1 (running 1)"));
        assert!(rendered.contains("machine pressure: high"));
    }

    #[test]
    fn risk_order_places_critical_first() {
        let mut snapshot = sample();
        snapshot.risks.push(RiskView {
            severity: "critical".into(),
            code: "thermal".into(),
            message: "CPU throttling".into(),
            project_id: None,
        });
        let normalized = snapshot.normalized();
        assert_eq!(normalized.risks[0].severity, "critical");
    }
}
