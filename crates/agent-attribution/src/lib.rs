use agentdock_core::{AgentIdentity, AgentKind, Service};
use std::process::Command;

const MAX_ANCESTOR_DEPTH: usize = 8;

pub fn enrich_agent(service: &mut Service) {
    let Some(pid) = service.pid else {
        return;
    };

    if let Some(identity) = detect_agent_for_process(pid) {
        service.agent = Some(identity);
    }
}

fn detect_agent_for_process(pid: u32) -> Option<AgentIdentity> {
    let mut current = Some(pid);

    for _ in 0..MAX_ANCESTOR_DEPTH {
        let current_pid = current?;
        let process = inspect_process(current_pid)?;

        if let Some(kind) = detect_kind(&process.name, process.command_line.as_deref()) {
            return Some(AgentIdentity {
                session_id: Some(format!(
                    "{}:{current_pid}",
                    kind_label(&kind)
                )),
                kind,
            });
        }

        current = process
            .parent_pid
            .filter(|parent| *parent > 0 && *parent != current_pid);
    }

    None
}

#[derive(Debug, Clone)]
struct ProcessInfo {
    parent_pid: Option<u32>,
    name: String,
    command_line: Option<String>,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn inspect_process(pid: u32) -> Option<ProcessInfo> {
    let parent_pid = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "ppid="])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|value| value.trim().parse::<u32>().ok());

    let command_line = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let name = command_line
        .as_deref()
        .and_then(first_process_token)
        .unwrap_or_default();

    if parent_pid.is_none() && command_line.is_none() {
        return None;
    }

    Some(ProcessInfo {
        parent_pid,
        name,
        command_line,
    })
}

#[cfg(target_os = "windows")]
fn inspect_process(pid: u32) -> Option<ProcessInfo> {
    let script = format!(
        "$p = Get-CimInstance Win32_Process -Filter 'ProcessId={pid}' -ErrorAction SilentlyContinue; if ($p) {{ [PSCustomObject]@{{ parent_pid = $p.ParentProcessId; name = $p.Name; command_line = $p.CommandLine }} | ConvertTo-Json -Compress }}"
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).ok()?;

    Some(ProcessInfo {
        parent_pid: value
            .get("parent_pid")
            .and_then(|value| value.as_u64())
            .and_then(|value| u32::try_from(value).ok()),
        name: value
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        command_line: value
            .get("command_line")
            .and_then(|value| value.as_str())
            .map(str::to_string),
    })
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "windows"
)))]
fn inspect_process(_pid: u32) -> Option<ProcessInfo> {
    None
}

fn detect_kind(
    name: &str,
    command_line: Option<&str>,
) -> Option<AgentKind> {
    let text = format!(
        "{} {}",
        name,
        command_line.unwrap_or_default()
    )
    .to_ascii_lowercase();

    if contains_any(
        &text,
        &[
            "@openai/codex",
            "codex app-server",
            " codex ",
            "/codex",
            "\\codex",
        ],
    ) {
        Some(AgentKind::Codex)
    } else if contains_any(
        &text,
        &[
            "@anthropic-ai/claude-code",
            "claude-code",
            " claude ",
            "/claude",
            "\\claude",
        ],
    ) {
        Some(AgentKind::ClaudeCode)
    } else if contains_any(
        &text,
        &[
            "cursor-agent",
            "cursor helper",
            "/cursor",
            "\\cursor",
            " cursor ",
        ],
    ) {
        Some(AgentKind::Cursor)
    } else if contains_any(
        &text,
        &[
            "@google/gemini-cli",
            "gemini-cli",
            " gemini ",
            "/gemini",
            "\\gemini",
        ],
    ) {
        Some(AgentKind::Gemini)
    } else {
        None
    }
}

fn first_process_token(command_line: &str) -> Option<String> {
    command_line
        .split_whitespace()
        .next()
        .map(|token| token.trim_matches('"'))
        .and_then(|token| token.rsplit(['/', '\\']).next())
        .map(str::to_string)
}

fn contains_any(
    haystack: &str,
    needles: &[&str],
) -> bool {
    needles
        .iter()
        .any(|needle| haystack.contains(needle))
}

fn kind_label(kind: &AgentKind) -> &'static str {
    match kind {
        AgentKind::Codex => "codex",
        AgentKind::ClaudeCode => "claude-code",
        AgentKind::Cursor => "cursor",
        AgentKind::Gemini => "gemini",
        AgentKind::Terminal => "terminal",
        AgentKind::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_codex() {
        assert_eq!(
            detect_kind(
                "node",
                Some("/usr/local/bin/codex app-server")
            ),
            Some(AgentKind::Codex)
        );
    }

    #[test]
    fn detects_claude_code() {
        assert_eq!(
            detect_kind(
                "node",
                Some("node @anthropic-ai/claude-code")
            ),
            Some(AgentKind::ClaudeCode)
        );
    }

    #[test]
    fn detects_cursor_agent() {
        assert_eq!(
            detect_kind(
                "cursor-agent",
                Some("cursor-agent --background")
            ),
            Some(AgentKind::Cursor)
        );
    }

    #[test]
    fn ignores_regular_node_process() {
        assert_eq!(
            detect_kind(
                "node",
                Some("node node_modules/vite/bin/vite.js")
            ),
            None
        );
    }
}
