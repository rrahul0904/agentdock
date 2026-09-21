use agent_attribution::enrich_agent;
use agentdock_core::ServiceClassification;
use framework_detection::enrich_service;
use process_discovery::{DiscoveryOptions, NativeDiscovery, ServiceDiscovery};
use project_resolver::resolve_project;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const DEFAULT_DAEMON_ADDR: &str = "127.0.0.1:7317";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("scan") => scan(&args[1..]),
        Some("daemon") => daemon(&args[1..]),
        Some("pairing") => pairing(&args[1..]),
        Some("doctor") => doctor(),
        _ => help(),
    }
}

fn scan(args: &[String]) {
    let json = args.iter().any(|arg| arg == "--json");
    let include_all = args.iter().any(|arg| arg == "--all");
    let include_udp = args.iter().any(|arg| arg == "--udp");
    let discovery = NativeDiscovery::new(DiscoveryOptions { include_udp });

    match discovery.scan() {
        Ok(mut services) => {
            for service in &mut services {
                if let Some(cwd) = service.working_directory.as_deref() {
                    service.project = Some(resolve_project(cwd));
                }
                enrich_service(service);
                enrich_agent(service);
            }

            if !include_all {
                services.retain(|service| service.is_default_visible());
            }

            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&services).expect("serialize services")
                );
                return;
            }

            if services.is_empty() {
                println!("No matching listening services discovered.");
                println!("Tip: use agentdock scan --all to include system/unknown listeners.");
                return;
            }

            println!(
                "{:<8} {:<7} {:<16} {:<14} {:<24} {}",
                "PID", "PORT", "CLASS", "FRAMEWORK", "PROJECT", "HOSTNAME"
            );

            for service in services {
                let pid = service
                    .pid
                    .map(|pid| pid.to_string())
                    .unwrap_or_else(|| "-".into());
                let project = service
                    .project
                    .as_ref()
                    .map(|p| p.name.as_str())
                    .unwrap_or("-");
                let hostname = service.stable_hostname().unwrap_or_else(|| "-".into());
                println!(
                    "{:<8} {:<7} {:<16} {:<14} {:<24} {}",
                    pid,
                    service.port,
                    display_classification(&service.classification),
                    format!("{:?}", service.framework),
                    project,
                    hostname
                );
            }
        }
        Err(error) => {
            eprintln!("AgentDock scan failed: {error}");
            std::process::exit(1);
        }
    }
}

fn daemon(args: &[String]) {
    let command = args.first().map(String::as_str).unwrap_or("status");
    let include_all = args.iter().any(|arg| arg == "--all");
    let addr = std::env::var("AGENTDOCK_ADDR").unwrap_or_else(|_| DEFAULT_DAEMON_ADDR.to_string());

    let path = match command {
        "status" => "/v1/status".to_string(),
        "services" if include_all => "/v1/services?all=1".to_string(),
        "services" => "/v1/services".to_string(),
        "projects" => "/v1/projects".to_string(),
        "routes" => "/v1/routes".to_string(),
        "sessions" => "/v1/agent-sessions".to_string(),
        "session-logs" => {
            let Some(session_id) = args
                .get(1)
                .map(String::as_str)
                .filter(|value| !value.is_empty())
            else {
                eprintln!("session-logs requires a session ID");
                eprintln!("Use: agentdock daemon session-logs <session-id>");
                std::process::exit(2);
            };
            format!("/v1/agent-session-logs?session_id={session_id}&limit=200")
        }
        "events" => "/v1/events?limit=200".to_string(),
        _ => {
            eprintln!("Unknown daemon command: {command}");
            eprintln!(
                "Use: agentdock daemon [status|services|projects|routes|sessions|session-logs|events] [--all]"
            );
            std::process::exit(2);
        }
    };

    match http_get(&addr, &path) {
        Ok(body) => print_response(&body),
        Err(error) => daemon_error(&addr, &error),
    }
}

fn pairing(args: &[String]) {
    let command = args.first().map(String::as_str).unwrap_or("requests");
    let addr = std::env::var("AGENTDOCK_ADDR").unwrap_or_else(|_| DEFAULT_DAEMON_ADDR.to_string());
    let include_all = args.iter().any(|arg| arg == "--all");

    let result = match command {
        "create" => http_post(&addr, "/v1/pairing/challenges", "{}"),
        "requests" => {
            let path = if include_all {
                "/v1/pairing/requests?all=1"
            } else {
                "/v1/pairing/requests"
            };
            http_get(&addr, path)
        }
        "approve" | "deny" => {
            let Some(request_id) = args.get(1).filter(|value| !value.is_empty()) else {
                eprintln!("{command} requires a pairing request ID");
                eprintln!("Use: agentdock pairing {command} <request-id>");
                std::process::exit(2);
            };
            let body = serde_json::json!({"request_id": request_id}).to_string();
            http_post(
                &addr,
                if command == "approve" {
                    "/v1/pairing/requests/approve"
                } else {
                    "/v1/pairing/requests/deny"
                },
                &body,
            )
        }
        "devices" => {
            let path = if include_all {
                "/v1/paired-devices?all=1"
            } else {
                "/v1/paired-devices"
            };
            http_get(&addr, path)
        }
        "revoke" => {
            let Some(device_id) = args.get(1).filter(|value| !value.is_empty()) else {
                eprintln!("revoke requires a paired device ID");
                eprintln!("Use: agentdock pairing revoke <device-id>");
                std::process::exit(2);
            };
            let body = serde_json::json!({"device_id": device_id}).to_string();
            http_post(&addr, "/v1/paired-devices/revoke", &body)
        }
        _ => {
            eprintln!("Unknown pairing command: {command}");
            eprintln!(
                "Use: agentdock pairing [create|requests|approve|deny|devices|revoke] [id] [--all]"
            );
            std::process::exit(2);
        }
    };

    match result {
        Ok(body) => print_response(&body),
        Err(error) => daemon_error(&addr, &error),
    }
}

fn print_response(body: &str) {
    match serde_json::from_str::<serde_json::Value>(body) {
        Ok(value) => println!(
            "{}",
            serde_json::to_string_pretty(&value).expect("serialize json")
        ),
        Err(_) => println!("{body}"),
    }
}

fn daemon_error(addr: &str, error: &str) -> ! {
    eprintln!("Unable to reach AgentDock daemon at {addr}: {error}");
    eprintln!("Start it with: cargo run -p agentdockd");
    std::process::exit(1);
}

fn http_get(addr: &str, path: &str) -> Result<String, String> {
    http_request(addr, "GET", path, None)
}

fn http_post(addr: &str, path: &str, body: &str) -> Result<String, String> {
    http_request(addr, "POST", path, Some(body))
}

fn http_request(
    addr: &str,
    method: &str,
    path: &str,
    body: Option<&str>,
) -> Result<String, String> {
    let mut stream = TcpStream::connect(addr).map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|error| error.to_string())?;

    let request = match body {
        Some(body) => format!(
            "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ),
        None => format!("{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"),
    };
    stream
        .write_all(request.as_bytes())
        .map_err(|error| error.to_string())?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| error.to_string())?;
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "invalid HTTP response".to_string())?;

    let status = headers.lines().next().unwrap_or_default();
    let status_code = status
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(0);
    if !(200..300).contains(&status_code) {
        return Err(format!("{status}: {body}"));
    }
    Ok(body.to_string())
}

fn display_classification(value: &ServiceClassification) -> &'static str {
    match value {
        ServiceClassification::Development => "development",
        ServiceClassification::Infrastructure => "infrastructure",
        ServiceClassification::System => "system",
        ServiceClassification::Unknown => "unknown",
    }
}

fn doctor() {
    println!("AgentDock doctor");
    println!("  platform: {}", std::env::consts::OS);
    println!("  architecture: {}", std::env::consts::ARCH);

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        println!(
            "  lsof: {}",
            if std::process::Command::new("lsof")
                .arg("-v")
                .output()
                .is_ok()
            {
                "available"
            } else {
                "missing"
            }
        );
        println!(
            "  ps: {}",
            if std::process::Command::new("ps")
                .arg("--help")
                .output()
                .is_ok()
            {
                "available"
            } else {
                "missing"
            }
        );
    }

    #[cfg(target_os = "windows")]
    {
        let powershell = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "$PSVersionTable.PSVersion.ToString()",
            ])
            .output();
        println!(
            "  powershell: {}",
            if powershell.is_ok() {
                "available"
            } else {
                "missing"
            }
        );
    }

    let addr = std::env::var("AGENTDOCK_ADDR").unwrap_or_else(|_| DEFAULT_DAEMON_ADDR.into());
    println!(
        "  daemon: {}",
        if http_get(&addr, "/healthz").is_ok() {
            "reachable"
        } else {
            "not running"
        }
    );
}

fn help() {
    println!("AgentDock - AI local development control plane");
    println!();
    println!("Usage:");
    println!("  agentdock scan [--json] [--all] [--udp]");
    println!("  agentdock daemon status");
    println!("  agentdock daemon services [--all]");
    println!("  agentdock daemon projects");
    println!("  agentdock daemon routes");
    println!("  agentdock daemon sessions");
    println!("  agentdock daemon session-logs <session-id>");
    println!("  agentdock daemon events");
    println!("  agentdock pairing create");
    println!("  agentdock pairing requests [--all]");
    println!("  agentdock pairing approve <request-id>");
    println!("  agentdock pairing deny <request-id>");
    println!("  agentdock pairing devices [--all]");
    println!("  agentdock pairing revoke <device-id>");
    println!("  agentdock doctor");
}
