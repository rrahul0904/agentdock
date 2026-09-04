use agentdock_core::{Protocol, Service};
use std::collections::BTreeSet;
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("failed to execute service discovery command: {0}")]
    Command(String),
    #[error("service discovery output was not valid UTF-8")]
    Utf8,
}

pub trait ServiceDiscovery {
    fn scan(&self) -> Result<Vec<Service>, DiscoveryError>;
}

#[derive(Default)]
pub struct NativeDiscovery;

impl ServiceDiscovery for NativeDiscovery {
    fn scan(&self) -> Result<Vec<Service>, DiscoveryError> {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            scan_lsof()
        }

        #[cfg(target_os = "windows")]
        {
            scan_windows()
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            Ok(Vec::new())
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn scan_lsof() -> Result<Vec<Service>, DiscoveryError> {
    // -nP avoids DNS/service-name lookups.
    // -iTCP -sTCP:LISTEN limits output to TCP listeners.
    // -F emits machine-readable field-prefixed records.
    let output = Command::new("lsof")
        .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-Fpcn"])
        .output()
        .map_err(|e| DiscoveryError::Command(e.to_string()))?;

    if !output.status.success() {
        return Err(DiscoveryError::Command(format!(
            "lsof exited with status {}",
            output.status
        )));
    }

    let stdout = String::from_utf8(output.stdout).map_err(|_| DiscoveryError::Utf8)?;
    Ok(parse_lsof(&stdout))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn parse_lsof(input: &str) -> Vec<Service> {
    let mut pid: Option<u32> = None;
    let mut command: Option<String> = None;
    let mut seen = BTreeSet::new();
    let mut services = Vec::new();

    for line in input.lines() {
        match line.chars().next() {
            Some('p') => pid = line[1..].parse().ok(),
            Some('c') => command = Some(line[1..].to_string()),
            Some('n') => {
                if let Some(port) = extract_port(&line[1..]) {
                    let key = (pid, port);
                    if seen.insert(key) {
                        services.push(Service {
                            pid,
                            port,
                            protocol: Protocol::Tcp,
                            command: command.clone(),
                            working_directory: pid.and_then(process_cwd),
                            project: None,
                            agent: None,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    services
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn extract_port(endpoint: &str) -> Option<u16> {
    endpoint
        .rsplit(':')
        .next()
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse::<u16>().ok())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn process_cwd(pid: u32) -> Option<std::path::PathBuf> {
    let output = Command::new("lsof")
        .args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    stdout
        .lines()
        .find_map(|line| line.strip_prefix('n').map(std::path::PathBuf::from))
}

#[cfg(target_os = "windows")]
fn scan_windows() -> Result<Vec<Service>, DiscoveryError> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Get-NetTCPConnection -State Listen | Select-Object -ExpandProperty LocalPort",
        ])
        .output()
        .map_err(|e| DiscoveryError::Command(e.to_string()))?;

    let stdout = String::from_utf8(output.stdout).map_err(|_| DiscoveryError::Utf8)?;
    let mut ports = BTreeSet::new();

    for line in stdout.lines() {
        if let Ok(port) = line.trim().parse::<u16>() {
            ports.insert(port);
        }
    }

    Ok(ports
        .into_iter()
        .map(|port| Service {
            pid: None,
            port,
            protocol: Protocol::Tcp,
            command: None,
            working_directory: None,
            project: None,
            agent: None,
        })
        .collect())
}
