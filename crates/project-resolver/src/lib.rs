use agentdock_core::ProjectIdentity;
use std::path::{Path, PathBuf};

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
}
