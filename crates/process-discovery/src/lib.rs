use agentdock_core::{Framework, Protocol, Service, ServiceClassification};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
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

#[derive(Debug, Clone, Default)]
pub struct DiscoveryOptions {
    pub include_udp: bool,
}

#[derive(Default)]
pub struct NativeDiscovery {
    options: DiscoveryOptions,
}

impl NativeDiscovery {
    pub fn new(options: DiscoveryOptions) -> Self {
        Self { options }
    }
}

impl ServiceDiscovery for NativeDiscovery {
    fn scan(&self) -> Result<Vec<Service>, DiscoveryError> {
        let mut services = scan_tcp()?;

        if self.options.include_udp {
            services.extend(scan_udp()?);
        }

        services.sort_by_key(|service| (service.port, service.pid.unwrap_or_default()));
        Ok(services)
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn scan_tcp() -> Result<Vec<Service>, DiscoveryError> {
    scan_lsof(&["-nP", "-iTCP", "-sTCP:LISTEN", "-Fpcn"], Protocol::Tcp)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn scan_udp() -> Result<Vec<Service>, DiscoveryError> {
    scan_lsof(&["-nP", "-iUDP", "-Fpcn"], Protocol::Udp)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn scan_lsof(args: &[&str], protocol: Protocol) -> Result<Vec<Service>, DiscoveryError> {
    let output = Command::new("lsof")
        .args(args)
        .output()
        .map_err(|e| DiscoveryError::Command(e.to_string()))?;

    if !output.status.success() {
        return Err(DiscoveryError::Command(format!(
            "lsof exited with status {}",
            output.status
        )));
    }

    let stdout = String::from_utf8(output.stdout).map_err(|_| DiscoveryError::Utf8)?;
    let mut services = parse_lsof(&stdout, protocol);

    enrich_unix_process_metadata(&mut services);
    Ok(services)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn parse_lsof(input: &str, protocol: Protocol) -> Vec<Service> {
    let mut pid: Option<u32> = None;
    let mut command: Option<String> = None;
    let mut seen = BTreeSet::new();
    let mut services = Vec::new();

    for line in input.lines() {
        match line.chars().next() {
            Some('p') => {
                pid = line.get(1..).and_then(|value| value.parse().ok());
                command = None;
            }
            Some('c') => command = line.get(1..).map(str::to_string),
            Some('n') => {
                if let Some(endpoint) = line.get(1..).and_then(parse_endpoint) {
                    let key = (pid, endpoint.port, protocol_key(&protocol));
                    if seen.insert(key) {
                        services.push(empty_service(
                            pid,
                            endpoint.port,
                            protocol.clone(),
                            Some(endpoint.address),
                            command.clone(),
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    services
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn enrich_unix_process_metadata(services: &mut [Service]) {
    let pids: BTreeSet<u32> = services.iter().filter_map(|service| service.pid).collect();
    let mut cache: BTreeMap<u32, ProcessMetadata> = BTreeMap::new();

    for pid in pids {
        cache.insert(pid, inspect_unix_process(pid));
    }

    for service in services {
        if let Some(pid) = service.pid {
            if let Some(metadata) = cache.get(&pid) {
                service.working_directory = metadata.cwd.clone();
                service.command_line = metadata.command_line.clone();
            }
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[derive(Debug, Clone, Default)]
struct ProcessMetadata {
    cwd: Option<PathBuf>,
    command_line: Option<String>,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn inspect_unix_process(pid: u32) -> ProcessMetadata {
    ProcessMetadata {
        cwd: process_cwd(pid),
        command_line: process_command_line(pid),
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn process_cwd(pid: u32) -> Option<PathBuf> {
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
        .find_map(|line| line.strip_prefix('n').map(PathBuf::from))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn process_command_line(pid: u32) -> Option<String> {
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let value = stdout.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[derive(Debug)]
struct ParsedEndpoint {
    address: String,
    port: u16,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn parse_endpoint(endpoint: &str) -> Option<ParsedEndpoint> {
    let endpoint = endpoint
        .split_whitespace()
        .next()
        .unwrap_or(endpoint)
        .trim();

    let separator = endpoint.rfind(':')?;
    let (address, port_text) = endpoint.split_at(separator);
    let port = port_text.trim_start_matches(':').parse::<u16>().ok()?;

    let address = address
        .trim_matches('[')
        .trim_matches(']')
        .trim()
        .to_string();

    Some(ParsedEndpoint {
        address: if address.is_empty() {
            "*".to_string()
        } else {
            address
        },
        port,
    })
}

#[cfg(target_os = "windows")]
fn scan_tcp() -> Result<Vec<Service>, DiscoveryError> {
    scan_windows("TCP")
}

#[cfg(target_os = "windows")]
fn scan_udp() -> Result<Vec<Service>, DiscoveryError> {
    scan_windows("UDP")
}

#[cfg(target_os = "windows")]
fn scan_windows(protocol: &str) -> Result<Vec<Service>, DiscoveryError> {
    let script = if protocol == "TCP" {
        r#"
$items = Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue
foreach ($item in $items) {
  $p = Get-CimInstance Win32_Process -Filter ("ProcessId=" + $item.OwningProcess) -ErrorAction SilentlyContinue
  $name = if ($p) { $p.Name } else { "" }
  $cmd = if ($p) { $p.CommandLine } else { "" }
  Write-Output ($item.OwningProcess.ToString() + "`t" + $item.LocalAddress + "`t" + $item.LocalPort.ToString() + "`t" + $name + "`t" + $cmd)
}
"#
    } else {
        r#"
$items = Get-NetUDPEndpoint -ErrorAction SilentlyContinue
foreach ($item in $items) {
  $p = Get-CimInstance Win32_Process -Filter ("ProcessId=" + $item.OwningProcess) -ErrorAction SilentlyContinue
  $name = if ($p) { $p.Name } else { "" }
  $cmd = if ($p) { $p.CommandLine } else { "" }
  Write-Output ($item.OwningProcess.ToString() + "`t" + $item.LocalAddress + "`t" + $item.LocalPort.ToString() + "`t" + $name + "`t" + $cmd)
}
"#
    };

    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .output()
        .map_err(|e| DiscoveryError::Command(e.to_string()))?;

    if !output.status.success() {
        return Err(DiscoveryError::Command(format!(
            "PowerShell discovery exited with status {}",
            output.status
        )));
    }

    let stdout = String::from_utf8(output.stdout).map_err(|_| DiscoveryError::Utf8)?;
    let proto = if protocol == "TCP" {
        Protocol::Tcp
    } else {
        Protocol::Udp
    };

    Ok(parse_windows(&stdout, proto))
}

#[cfg(target_os = "windows")]
fn parse_windows(input: &str, protocol: Protocol) -> Vec<Service> {
    let mut seen = BTreeSet::new();
    let mut services = Vec::new();

    for line in input.lines() {
        let fields: Vec<&str> = line.split('	').collect();
        if fields.len() < 3 {
            continue;
        }

        let pid = fields[0].trim().parse::<u32>().ok();
        let address = fields[1].trim().to_string();
        let Some(port) = fields[2].trim().parse::<u16>().ok() else {
            continue;
        };
        let command = fields
            .get(3)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let command_line = fields
            .get(4)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let key = (pid, port, protocol_key(&protocol));
        if seen.insert(key) {
            let mut service =
                empty_service(pid, port, protocol.clone(), Some(address), command);
            service.command_line = command_line;
            services.push(service);
        }
    }

    services
}

fn empty_service(
    pid: Option<u32>,
    port: u16,
    protocol: Protocol,
    bind_address: Option<String>,
    command: Option<String>,
) -> Service {
    Service {
        pid,
        port,
        protocol,
        bind_address,
        command,
        command_line: None,
        working_directory: None,
        project: None,
        framework: Framework::Unknown,
        container: None,
        agent: None,
        classification: ServiceClassification::Unknown,
    }
}

fn protocol_key(protocol: &Protocol) -> u8 {
    match protocol {
        Protocol::Tcp => 1,
        Protocol::Udp => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn parses_macos_fixture() {
        let input = include_str!("../tests/fixtures/lsof_macos.txt");
        let services = parse_lsof(input, Protocol::Tcp);

        assert_eq!(services.len(), 3);
        assert_eq!(services[0].pid, Some(41234));
        assert_eq!(services[0].port, 3000);
        assert_eq!(services[0].bind_address.as_deref(), Some("127.0.0.1"));
        assert_eq!(services[1].port, 5173);
        assert_eq!(services[2].port, 8000);
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn parses_ipv6_endpoint() {
        let endpoint = parse_endpoint("[::1]:4317").expect("endpoint");
        assert_eq!(endpoint.address, "::1");
        assert_eq!(endpoint.port, 4317);
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn parses_linux_fixture() {
        let input = include_str!("../tests/fixtures/lsof_linux.txt");
        let services = parse_lsof(input, Protocol::Tcp);

        assert_eq!(services.len(), 2);
        assert_eq!(services[0].port, 5432);
        assert_eq!(services[1].port, 6379);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn parses_windows_fixture() {
        let input = include_str!("../tests/fixtures/windows_tcp.tsv");
        let services = parse_windows(input, Protocol::Tcp);

        assert_eq!(services.len(), 2);
        assert_eq!(services[0].pid, Some(41234));
        assert_eq!(services[0].port, 3000);
        assert_eq!(services[0].command.as_deref(), Some("node.exe"));
        assert_eq!(services[1].port, 8000);
    }
}
