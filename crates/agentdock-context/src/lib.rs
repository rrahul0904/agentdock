use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

pub const CONTEXT_SCHEMA_VERSION: u8 = 1;
pub const RECEIPT_SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceMapOptions {
    /// Hard cap on filesystem entries inspected. This is a work bound, not a claim
    /// that every omitted entry was counted.
    pub max_entries: usize,
    /// Maximum recursion depth below the workspace root.
    pub max_depth: usize,
}

impl Default for WorkspaceMapOptions {
    fn default() -> Self {
        Self {
            max_entries: 2_000,
            max_depth: 5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceMap {
    pub schema_version: u8,
    pub workspace_name: String,
    pub areas: Vec<WorkspaceArea>,
    pub inspected_entries: usize,
    pub omitted_entries: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceArea {
    pub path: String,
    pub files: usize,
    pub directories: usize,
    pub extensions: BTreeMap<String, usize>,
    pub markers: Vec<String>,
}

#[derive(Debug)]
struct WalkState {
    inspected: usize,
    omitted: usize,
    truncated: bool,
}

const EXCLUDED_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".cache",
    ".idea",
    ".vscode",
    ".next",
    ".turbo",
    ".venv",
    "venv",
    "node_modules",
    "target",
    "dist",
    "build",
    "coverage",
    "vendor",
];

const MARKERS: &[&str] = &[
    "Cargo.toml",
    "package.json",
    "pnpm-workspace.yaml",
    "pyproject.toml",
    "requirements.txt",
    "go.mod",
    "pom.xml",
    "build.gradle",
    "Dockerfile",
    "docker-compose.yml",
    "README.md",
];

fn normalized_relative(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn is_excluded_dir(name: &str) -> bool {
    EXCLUDED_DIRS
        .iter()
        .any(|candidate| name.eq_ignore_ascii_case(candidate))
}

fn is_sensitive_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == ".env"
        || lower.starts_with(".env.")
        || matches!(
            lower.as_str(),
            ".npmrc"
                | ".pypirc"
                | ".netrc"
                | "credentials"
                | "credentials.json"
                | "secrets.json"
                | "id_rsa"
                | "id_ed25519"
        )
        || lower.contains("credential")
        || lower.contains("secret")
        || lower.ends_with(".pem")
        || lower.ends_with(".key")
        || lower.ends_with(".p12")
        || lower.ends_with(".pfx")
}

fn extension_label(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "<none>".to_string())
}

fn walk_area(
    root: &Path,
    path: &Path,
    depth: usize,
    options: WorkspaceMapOptions,
    state: &mut WalkState,
    area: &mut WorkspaceArea,
) -> io::Result<()> {
    if depth > options.max_depth {
        state.truncated = true;
        state.omitted += 1;
        return Ok(());
    }

    let mut entries = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_string());

    for entry in entries {
        if state.inspected >= options.max_entries {
            state.truncated = true;
            state.omitted += 1;
            continue;
        }

        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        let file_type = entry.file_type()?;

        if file_type.is_symlink() {
            state.inspected += 1;
            state.omitted += 1;
            continue;
        }

        if file_type.is_dir() && is_excluded_dir(&name) {
            state.inspected += 1;
            state.omitted += 1;
            continue;
        }

        if file_type.is_file() && is_sensitive_file(&name) {
            state.inspected += 1;
            state.omitted += 1;
            continue;
        }

        state.inspected += 1;

        if depth == 1 && MARKERS.iter().any(|marker| name == *marker) {
            area.markers.push(name.to_string());
        }

        if file_type.is_dir() {
            area.directories += 1;
            walk_area(root, &entry.path(), depth + 1, options, state, area)?;
        } else if file_type.is_file() {
            area.files += 1;
            *area
                .extensions
                .entry(extension_label(&entry.path()))
                .or_insert(0) += 1;
        }
    }

    area.markers.sort();
    area.markers.dedup();
    let _ = root;
    Ok(())
}

/// Produces a deterministic, metadata-only workspace summary.
///
/// The map does not read file contents. Known credential/key paths, VCS internals,
/// dependency trees, build caches and symlinks are excluded by default.
pub fn build_workspace_map(
    root: impl AsRef<Path>,
    options: WorkspaceMapOptions,
) -> io::Result<WorkspaceMap> {
    let root = root.as_ref();
    let workspace_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("workspace")
        .to_string();

    let mut state = WalkState {
        inspected: 0,
        omitted: 0,
        truncated: false,
    };

    let mut root_entries = fs::read_dir(root)?.collect::<Result<Vec<_>, _>>()?;
    root_entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_string());

    let mut areas = Vec::new();

    for entry in root_entries {
        if state.inspected >= options.max_entries {
            state.truncated = true;
            state.omitted += 1;
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        let file_type = entry.file_type()?;

        if file_type.is_symlink() {
            state.inspected += 1;
            state.omitted += 1;
            continue;
        }

        if file_type.is_dir() && is_excluded_dir(&name) {
            state.inspected += 1;
            state.omitted += 1;
            continue;
        }

        if file_type.is_file() && is_sensitive_file(&name) {
            state.inspected += 1;
            state.omitted += 1;
            continue;
        }

        state.inspected += 1;

        let mut area = WorkspaceArea {
            path: normalized_relative(&entry.path(), root),
            files: 0,
            directories: 0,
            extensions: BTreeMap::new(),
            markers: Vec::new(),
        };

        if file_type.is_dir() {
            area.directories = 1;
            walk_area(root, &entry.path(), 1, options, &mut state, &mut area)?;
        } else if file_type.is_file() {
            area.files = 1;
            *area
                .extensions
                .entry(extension_label(&entry.path()))
                .or_insert(0) += 1;
            if MARKERS.iter().any(|marker| name == *marker) {
                area.markers.push(name);
            }
        }

        areas.push(area);
    }

    areas.sort_by(|left, right| left.path.cmp(&right.path));

    Ok(WorkspaceMap {
        schema_version: CONTEXT_SCHEMA_VERSION,
        workspace_name,
        areas,
        inspected_entries: state.inspected,
        omitted_entries: state.omitted,
        truncated: state.truncated,
    })
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceKind {
    Memory,
    Note,
    Port,
    Runtime,
    Service,
    Tool,
    Worktree,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceFact {
    pub kind: ResourceKind,
    pub key: String,
    pub value: String,
    pub source: String,
    pub confidence: Confidence,
    /// Higher values render first.
    pub priority: u8,
    /// Sensitive facts are retained in the snapshot for explicit local handling but
    /// are never rendered into a default capsule.
    pub sensitive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextSnapshot {
    pub schema_version: u8,
    pub project_id: String,
    pub workspace: WorkspaceMap,
    pub facts: Vec<ResourceFact>,
}

impl ContextSnapshot {
    pub fn new(
        project_id: impl Into<String>,
        workspace: WorkspaceMap,
        facts: Vec<ResourceFact>,
    ) -> Self {
        Self {
            schema_version: CONTEXT_SCHEMA_VERSION,
            project_id: project_id.into(),
            workspace,
            facts,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextCapsule {
    pub schema_version: u8,
    pub text: String,
    pub max_bytes: usize,
    pub included_facts: usize,
    pub omitted_facts: usize,
    pub truncated: bool,
}

fn push_line(output: &mut String, line: &str, max_bytes: usize) -> bool {
    let separator = usize::from(!output.is_empty());
    if output.len() + separator + line.len() > max_bytes {
        return false;
    }

    if separator == 1 {
        output.push('\n');
    }
    output.push_str(line);
    true
}

/// Renders a compact, deterministic context capsule bounded in UTF-8 bytes.
///
/// Sensitive facts are omitted even when they have the highest priority.
pub fn render_context_capsule(snapshot: &ContextSnapshot, max_bytes: usize) -> ContextCapsule {
    let mut text = String::new();
    let mut truncated = false;
    let mut included_facts = 0usize;
    let sensitive_count = snapshot.facts.iter().filter(|fact| fact.sensitive).count();

    for line in [
        "[agentdock-context v1]".to_string(),
        format!("project: {}", snapshot.project_id),
        format!("workspace: {}", snapshot.workspace.workspace_name),
        format!(
            "map: areas={} inspected={} omitted={} truncated={}",
            snapshot.workspace.areas.len(),
            snapshot.workspace.inspected_entries,
            snapshot.workspace.omitted_entries,
            snapshot.workspace.truncated
        ),
    ] {
        if !push_line(&mut text, &line, max_bytes) {
            truncated = true;
            break;
        }
    }

    if !truncated {
        for area in &snapshot.workspace.areas {
            let extensions = area
                .extensions
                .iter()
                .map(|(extension, count)| format!("{extension}:{count}"))
                .collect::<Vec<_>>()
                .join(",");
            let markers = area.markers.join(",");
            let line = format!(
                "area: {} files={} dirs={} ext=[{}] markers=[{}]",
                area.path, area.files, area.directories, extensions, markers
            );
            if !push_line(&mut text, &line, max_bytes) {
                truncated = true;
                break;
            }
        }
    }

    let mut facts = snapshot
        .facts
        .iter()
        .filter(|fact| !fact.sensitive)
        .collect::<Vec<_>>();
    facts.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.key.cmp(&right.key))
            .then_with(|| left.value.cmp(&right.value))
    });

    for fact in facts {
        let line = format!(
            "fact: {:?} {}={} source={} confidence={:?}",
            fact.kind, fact.key, fact.value, fact.source, fact.confidence
        );
        if push_line(&mut text, &line, max_bytes) {
            included_facts += 1;
        } else {
            truncated = true;
        }
    }

    let omitted_facts = snapshot.facts.len().saturating_sub(included_facts);
    if sensitive_count > 0 {
        truncated = truncated || omitted_facts > 0;
    }

    ContextCapsule {
        schema_version: CONTEXT_SCHEMA_VERSION,
        text,
        max_bytes,
        included_facts,
        omitted_facts,
        truncated,
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ExperimentArm {
    Baseline,
    ContextCapsule,
}

/// Deterministic FNV-1a assignment for experimental cohorting.
///
/// This is intentionally not a cryptographic primitive and must not be used for
/// authorization, secrets, or adversarially chosen security decisions.
pub fn assign_experiment_arm(conversation_id: &str, salt: &str) -> ExperimentArm {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in salt
        .as_bytes()
        .iter()
        .copied()
        .chain(std::iter::once(0xff))
        .chain(conversation_id.as_bytes().iter().copied())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }

    if hash & 1 == 0 {
        ExperimentArm::Baseline
    } else {
        ExperimentArm::ContextCapsule
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OutcomeStatus {
    Accepted,
    Rejected,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutcomeReceipt {
    pub status: OutcomeStatus,
    pub test_command_hash: Option<String>,
    pub artifact_hash: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EfficiencyReceipt {
    pub schema_version: u8,
    pub task_id: String,
    pub conversation_id: Option<String>,
    pub arm: ExperimentArm,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub harness: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
    pub cost_microusd: Option<u64>,
    pub duration_ms: Option<u64>,
    pub tool_calls: Option<u64>,
    pub failed_tool_calls: Option<u64>,
    pub first_write_ms: Option<u64>,
    pub context_bytes: Option<u64>,
    pub context_sha256: Option<String>,
    pub outcome: OutcomeReceipt,
}

impl EfficiencyReceipt {
    pub fn validate(&self) -> Result<(), ReceiptError> {
        if self.task_id.trim().is_empty() {
            return Err(ReceiptError::EmptyTaskId);
        }

        if let (Some(failed), Some(total)) = (self.failed_tool_calls, self.tool_calls) {
            if failed > total {
                return Err(ReceiptError::FailedToolCallsExceedTotal { failed, total });
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiptError {
    EmptyTaskId,
    FailedToolCallsExceedTotal { failed: u64, total: u64 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_workspace() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("agentdock-context-{}-{nonce}", std::process::id()));
        fs::create_dir_all(root.join("src")).expect("src");
        fs::create_dir_all(root.join("node_modules/pkg")).expect("node_modules");
        fs::create_dir_all(root.join(".git")).expect("git");
        fs::write(root.join("src/lib.rs"), "pub fn hello() {}").expect("source");
        fs::write(root.join("README.md"), "readme").expect("readme");
        fs::write(root.join("Cargo.toml"), "[package]\nname='fixture'\n").expect("cargo");
        fs::write(root.join(".env"), "API_KEY=never-render").expect("env");
        fs::write(root.join("credentials.json"), "never-render").expect("credentials");
        fs::write(root.join("node_modules/pkg/index.js"), "ignored").expect("dependency");
        root
    }

    #[test]
    fn workspace_map_is_deterministic_and_excludes_sensitive_paths() {
        let root = temp_workspace();
        let options = WorkspaceMapOptions::default();

        let first = build_workspace_map(&root, options).expect("first map");
        let second = build_workspace_map(&root, options).expect("second map");
        assert_eq!(first, second);

        let json = serde_json::to_string(&first).expect("json");
        assert!(!json.contains(".env"));
        assert!(!json.contains("credentials"));
        assert!(!json.contains("node_modules"));
        assert!(!json.contains(".git"));
        assert!(json.contains("src"));
        assert!(json.contains("Cargo.toml"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn capsule_respects_budget_and_fact_priority() {
        let root = temp_workspace();
        let workspace = build_workspace_map(&root, WorkspaceMapOptions::default()).expect("map");
        let snapshot = ContextSnapshot::new(
            "project-1",
            workspace,
            vec![
                ResourceFact {
                    kind: ResourceKind::Note,
                    key: "low".into(),
                    value: "later".into(),
                    source: "test".into(),
                    confidence: Confidence::High,
                    priority: 1,
                    sensitive: false,
                },
                ResourceFact {
                    kind: ResourceKind::Port,
                    key: "preview".into(),
                    value: "7777".into(),
                    source: "registry".into(),
                    confidence: Confidence::High,
                    priority: 200,
                    sensitive: false,
                },
                ResourceFact {
                    kind: ResourceKind::Note,
                    key: "api-token".into(),
                    value: "never-render".into(),
                    source: "test".into(),
                    confidence: Confidence::High,
                    priority: 255,
                    sensitive: true,
                },
            ],
        );

        let capsule = render_context_capsule(&snapshot, 1_024);
        assert!(capsule.text.len() <= 1_024);
        assert!(!capsule.text.contains("never-render"));
        assert!(
            capsule.text.find("preview=7777").expect("high priority")
                < capsule.text.find("low=later").expect("low priority")
        );
        assert_eq!(capsule.included_facts, 2);
        assert_eq!(capsule.omitted_facts, 1);

        let tiny = render_context_capsule(&snapshot, 64);
        assert!(tiny.text.len() <= 64);
        assert!(tiny.truncated);

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn unknown_metrics_remain_null_in_receipt_json() {
        let receipt = EfficiencyReceipt {
            schema_version: RECEIPT_SCHEMA_VERSION,
            task_id: "task-1".into(),
            conversation_id: Some("conversation-1".into()),
            arm: ExperimentArm::ContextCapsule,
            provider: Some("synthetic".into()),
            model: None,
            harness: Some("agentdock".into()),
            input_tokens: None,
            output_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            cost_microusd: None,
            duration_ms: None,
            tool_calls: Some(3),
            failed_tool_calls: Some(1),
            first_write_ms: None,
            context_bytes: Some(500),
            context_sha256: None,
            outcome: OutcomeReceipt {
                status: OutcomeStatus::Unknown,
                test_command_hash: None,
                artifact_hash: None,
                notes: None,
            },
        };

        receipt.validate().expect("valid");
        let value = serde_json::to_value(&receipt).expect("serialize");
        assert!(value["input_tokens"].is_null());
        assert!(value["cost_microusd"].is_null());
        assert_ne!(value["input_tokens"], serde_json::json!(0));
    }

    #[test]
    fn invalid_failure_count_is_rejected() {
        let receipt = EfficiencyReceipt {
            schema_version: RECEIPT_SCHEMA_VERSION,
            task_id: "task-1".into(),
            conversation_id: None,
            arm: ExperimentArm::Baseline,
            provider: None,
            model: None,
            harness: None,
            input_tokens: None,
            output_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            cost_microusd: None,
            duration_ms: None,
            tool_calls: Some(1),
            failed_tool_calls: Some(2),
            first_write_ms: None,
            context_bytes: None,
            context_sha256: None,
            outcome: OutcomeReceipt {
                status: OutcomeStatus::Unknown,
                test_command_hash: None,
                artifact_hash: None,
                notes: None,
            },
        };

        assert_eq!(
            receipt.validate(),
            Err(ReceiptError::FailedToolCallsExceedTotal {
                failed: 2,
                total: 1
            })
        );
    }

    #[test]
    fn experiment_assignment_is_stable_and_has_both_arms() {
        let a = assign_experiment_arm("conversation-42", "experiment-a");
        let b = assign_experiment_arm("conversation-42", "experiment-a");
        assert_eq!(a, b);

        let arms = (0..100)
            .map(|index| assign_experiment_arm(&format!("conversation-{index}"), "experiment-a"))
            .collect::<Vec<_>>();
        assert!(arms.contains(&ExperimentArm::Baseline));
        assert!(arms.contains(&ExperimentArm::ContextCapsule));
    }
}
