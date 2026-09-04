use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Service {
    pub pid: Option<u32>,
    pub port: u16,
    pub protocol: Protocol,
    pub command: Option<String>,
    pub working_directory: Option<PathBuf>,
    pub project: Option<ProjectIdentity>,
    pub agent: Option<AgentIdentity>,
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

impl Service {
    pub fn stable_hostname(&self) -> Option<String> {
        self.project
            .as_ref()
            .map(|project| format!("{}.localhost", slugify(&project.name)))
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
}
