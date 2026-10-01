use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fs;
use std::path::Path;
use thiserror::Error;

pub const SUPERVISOR_SNAPSHOT_SCHEMA: &str = "supervisor-snapshot/v1";

const MAX_PROJECTS: usize = 2_000;
const MAX_WORKERS: usize = 512;
const MAX_TASKS: usize = 20_000;
const MAX_RISKS: usize = 2_000;
const MAX_DECISIONS: usize = 5_000;
const MAX_TEXT: usize = 16_000;

#[derive(Debug, Error)]
pub enum SupervisorError {
    #[error("failed to read supervisor snapshot: {0}")]
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
