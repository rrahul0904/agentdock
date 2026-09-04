use agentdock_core::ProjectIdentity;
use std::path::{Path, PathBuf};

pub fn resolve_project(start: &Path) -> ProjectIdentity {
    let git_root = find_upward(start, ".git");
    let root = git_root.clone().unwrap_or_else(|| start.to_path_buf());
    let name = root
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("project")
        .to_string();

    ProjectIdentity {
        name,
        root,
        git_root,
    }
}

fn find_upward(start: &Path, marker: &str) -> Option<PathBuf> {
    let mut current = Some(start);

    while let Some(path) = current {
        if path.join(marker).exists() {
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
        let root = Path::new("/tmp/example-agentdock-project");
        let project = resolve_project(root);
        assert_eq!(project.name, "example-agentdock-project");
    }
}
