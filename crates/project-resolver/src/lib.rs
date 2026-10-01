use agentdock_core::{ProjectIdentity, WorkspaceRootIdentity, WorkspaceScope};
use std::collections::HashSet;
use std::fmt;
use std::path::{Component, Path, PathBuf};

const PROJECT_MARKERS: &[&str] = &[
    "package.json",
    "pnpm-workspace.yaml",
    "pyproject.toml",
    "requirements.txt",
    "Cargo.toml",
    "go.mod",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "Gemfile",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceResolveError {
    EmptyWorkspace,
    InvalidRootLimit,
    TooManyRoots { actual: usize, max: usize },
    DuplicateRoot(String),
}

impl fmt::Display for WorkspaceResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyWorkspace => write!(f, "workspace must contain at least one root"),
            Self::InvalidRootLimit => write!(f, "workspace root limit must be greater than zero"),
            Self::TooManyRoots { actual, max } => {
                write!(f, "workspace contains {actual} roots but the configured limit is {max}")
            }
            Self::DuplicateRoot(root) => write!(f, "workspace contains duplicate root {root}"),
        }
    }
}

impl std::error::Error for WorkspaceResolveError {}

pub fn resolve_project(start: &Path) -> ProjectIdentity {
    let git_root = find_git_root(start);
    let root = git_root
        .clone()
        .or_else(|| find_project_marker_root(start))
        .unwrap_or_else(|| start.to_path_buf());

    let name = root
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("project")
        .to_string();

    let git_worktree = git_root
        .as_ref()
        .map(|path| path.join(".git").is_file())
        .unwrap_or(false);

    ProjectIdentity {
        name,
        root,
        git_root,
        git_worktree,
    }
}

pub fn resolve_workspace(
    starts: &[PathBuf],
    max_roots: usize,
) -> Result<WorkspaceScope, WorkspaceResolveError> {
    if starts.is_empty() {
        return Err(WorkspaceResolveError::EmptyWorkspace);
    }
    if max_roots == 0 {
        return Err(WorkspaceResolveError::InvalidRootLimit);
    }
    if starts.len() > max_roots {
        return Err(WorkspaceResolveError::TooManyRoots {
            actual: starts.len(),
            max: max_roots,
        });
    }

    let mut seen = HashSet::with_capacity(starts.len());
    let mut roots = Vec::with_capacity(starts.len());

    for start in starts {
        let project = resolve_project(start);
        let key = workspace_root_key(&project.root);

        if !seen.insert(key.clone()) {
            return Err(WorkspaceResolveError::DuplicateRoot(key));
        }

        roots.push(WorkspaceRootIdentity { key, project });
    }

    roots.sort_by(|left, right| left.key.cmp(&right.key));

    Ok(WorkspaceScope { version: 1, roots })
}

pub fn find_git_root(start: &Path) -> Option<PathBuf> {
    find_upward(start, |path| path.join(".git").exists())
}

pub fn find_project_marker_root(start: &Path) -> Option<PathBuf> {
    find_upward(start, |path| {
        PROJECT_MARKERS
            .iter()
            .any(|marker| path.join(marker).exists())
    })
}

fn workspace_root_key(root: &Path) -> String {
    let mut normalized = PathBuf::new();

    for component in root.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }

    let mut key = normalized.to_string_lossy().replace('\\', "/");
    while key.len() > 1 && key.ends_with('/') {
        key.pop();
    }

    if cfg!(windows) {
        key.make_ascii_lowercase();
    }

    key
}

fn find_upward<F>(start: &Path, predicate: F) -> Option<PathBuf>
where
    F: Fn(&Path) -> bool,
{
    let mut current = Some(start);

    while let Some(path) = current {
        if predicate(path) {
            return Some(path.to_path_buf());
        }
        current = path.parent();
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_uses_start_directory() {
        let root = std::env::temp_dir().join("example-agentdock-project");
        let project = resolve_project(&root);
        assert_eq!(project.name, "example-agentdock-project");
    }

    #[test]
    fn workspace_scope_orders_roots_deterministically() {
        let base = std::env::temp_dir();
        let root_b = base.join("agentdock-workspace-b");
        let root_a = base.join("agentdock-workspace-a");

        let workspace = resolve_workspace(&[root_b, root_a], 4).expect("workspace should resolve");

        assert_eq!(workspace.version, 1);
        assert_eq!(workspace.roots.len(), 2);
        assert!(workspace.roots[0].key < workspace.roots[1].key);
        assert_eq!(workspace.roots[0].project.name, "agentdock-workspace-a");
        assert_eq!(workspace.roots[1].project.name, "agentdock-workspace-b");
    }

    #[test]
    fn workspace_scope_rejects_duplicate_roots() {
        let root = std::env::temp_dir().join("agentdock-workspace-duplicate");

        let error =
            resolve_workspace(&[root.clone(), root], 4).expect_err("duplicate roots must fail");

        assert!(matches!(error, WorkspaceResolveError::DuplicateRoot(_)));
    }

    #[test]
    fn workspace_scope_rejects_empty_and_invalid_limits() {
        assert_eq!(
            resolve_workspace(&[], 4).expect_err("empty workspace must fail"),
            WorkspaceResolveError::EmptyWorkspace
        );

        let root = std::env::temp_dir().join("agentdock-workspace-root");
        assert_eq!(
            resolve_workspace(&[root], 0).expect_err("zero root limit must fail"),
            WorkspaceResolveError::InvalidRootLimit
        );
    }

    #[test]
    fn workspace_scope_rejects_over_budget_roots() {
        let base = std::env::temp_dir();
        let roots = [
            base.join("agentdock-workspace-one"),
            base.join("agentdock-workspace-two"),
        ];

        assert_eq!(
            resolve_workspace(&roots, 1).expect_err("over-budget roots must fail"),
            WorkspaceResolveError::TooManyRoots { actual: 2, max: 1 }
        );
    }
}
