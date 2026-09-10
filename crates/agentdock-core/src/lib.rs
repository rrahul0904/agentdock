use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Service {
    pub pid: Option<u32>,
    pub port: u16,
    pub protocol: Protocol,
    pub bind_address: Option<String>,
    pub command: Option<String>,
    pub command_line: Option<String>,
    pub working_directory: Option<PathBuf>,
    pub project: Option<ProjectIdentity>,
    pub framework: Framework,
    pub container: Option<ContainerIdentity>,
    pub agent: Option<AgentIdentity>,
    pub classification: ServiceClassification,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectIdentity {
    pub name: String,
    pub root: PathBuf,
    pub git_root: Option<PathBuf>,
    pub git_worktree: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentIdentity {
    pub kind: AgentKind,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AgentKind {
    Codex,
    ClaudeCode,
    Cursor,
    Gemini,
    Terminal,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Framework {
    NextJs,
    Vite,
    FastApi,
    Uvicorn,
    Django,
    Rails,
    Go,
    SpringBoot,
    Node,
    Python,
    Postgres,
    Redis,
    Mailhog,
    DockerProxy,
    PodmanProxy,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ServiceClassification {
    Development,
    Infrastructure,
    System,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContainerIdentity {
    pub runtime: ContainerRuntime,
    pub id: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ContainerRuntime {
    Docker,
    Podman,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LifecycleState {
    Active,
    Stale,
    Orphaned,
}

impl LifecycleState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Stale => "stale",
            Self::Orphaned => "orphaned",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "stale" => Some(Self::Stale),
            "orphaned" => Some(Self::Orphaned),
            _ => None,
        }
    }
}

impl Service {
    pub fn stable_hostname(&self) -> Option<String> {
        self.project
            .as_ref()
            .map(|project| format!("{}.localhost", slugify(&project.name)))
    }

    pub fn is_default_visible(&self) -> bool {
        !matches!(
            self.classification,
            ServiceClassification::System | ServiceClassification::Unknown
        )
    }
}

pub fn slugify(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut previous_dash = false;

    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            previous_dash = false;
        } else if !previous_dash && !out.is_empty() {
            out.push('-');
            previous_dash = true;
        }
    }

    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_is_stable() {
        assert_eq!(slugify("My Cool_App"), "my-cool-app");
    }

    #[test]
    fn lifecycle_round_trip() {
        for state in [
            LifecycleState::Active,
            LifecycleState::Stale,
            LifecycleState::Orphaned,
        ] {
            assert_eq!(LifecycleState::parse(state.as_str()), Some(state));
        }
    }
}
