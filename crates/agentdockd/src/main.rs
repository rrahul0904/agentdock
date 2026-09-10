use agentdock_core::Service;
use agentdock_registry::{now_ms, Registry};
use framework_detection::enrich_service;
use process_discovery::{DiscoveryOptions, NativeDiscovery, ServiceDiscovery};
use project_resolver::resolve_project;
use serde_json::json;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const DEFAULT_BIND: &str = "127.0.0.1:7317";
const DEFAULT_INTERVAL_MS: u64 = 2_000;
const DEFAULT_ORPHAN_AFTER_MS: i64 = 30_000;

fn main() {
    if let Err(error) = run() {
        eprintln!("agentdockd failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bind = value_after(&args, "--bind").unwrap_or_else(|| DEFAULT_BIND.to_string());
    let interval_ms = value_after(&args, "--interval-ms")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_INTERVAL_MS)
        .max(250);
    let orphan_after_ms = value_after(&args, "--orphan-after-ms")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(DEFAULT_ORPHAN_AFTER_MS);
    let include_udp = args.iter().any(|arg| arg == "--udp");

    let db_path = value_after(&args, "--db")
        .map(PathBuf::from)
        .unwrap_or_else(default_db_path);

    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }

    let mut registry = Registry::open(&db_path).map_err(|error| error.to_string())?;
    registry.set_orphan_after_ms(orphan_after_ms);
    let registry = Arc::new(Mutex::new(registry));

    reconcile_once(&registry, include_udp)?;

    {
        let registry = Arc::clone(&registry);
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(interval_ms));
            if let Err(error) = reconcile_once(&registry, include_udp) {
                eprintln!("agentdockd reconciliation error: {error}");
            }
        });
    }

    let listener = TcpListener::bind(&bind)
        .map_err(|error| format!("failed to bind local API at {bind}: {error}"))?;

    println!("AgentDock daemon {}", env!("CARGO_PKG_VERSION"));
    println!("  API: http://{bind}");
    println!("  DB: {}", db_path.display());
    println!("  scan interval: {interval_ms} ms");
    println!("  orphan threshold: {orphan_after_ms} ms");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(error) = handle_client(stream, &registry) {
                    eprintln!("agentdockd API error: {error}");
                }
            }
            Err(error) => eprintln!("agentdockd accept error: {error}"),
        }
    }

    Ok(())
}

fn reconcile_once(registry: &Arc<Mutex<Registry>>, include_udp: bool) -> Result<(), String> {
    let services = discover_services(include_udp)?;
    let mut registry = registry
        .lock()
        .map_err(|_| "registry mutex poisoned".to_string())?;
    let summary = registry
        .reconcile(&services, now_ms())
        .map_err(|error| error.to_string())?;

    if summary.discovered + summary.resumed + summary.stale + summary.orphaned > 0 {
        println!(
            "reconcile: +{} resumed={} stale={} orphaned={} active_total={}",
            summary.discovered,
            summary.resumed,
            summary.stale,
            summary.orphaned,
            summary.active_total
        );
    }
    Ok(())
}

fn discover_services(include_udp: bool) -> Result<Vec<Service>, String> {
    let discovery = NativeDiscovery::new(DiscoveryOptions { include_udp });
    let mut services = discovery.scan().map_err(|error| error.to_string())?;

    for service in &mut services {
        if let Some(cwd) = service.working_directory.as_deref() {
            service.project = Some(resolve_project(cwd));
        }
        enrich_service(service);
    }
    Ok(services)
}

fn handle_client(mut stream: TcpStream, registry: &Arc<Mutex<Registry>>) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| error.to_string())?;

    let mut buffer = [0_u8; 16 * 1024];
    let read = stream.read(&mut buffer).map_err(|error| error.to_string())?;
    if read == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&buffer[..read]);
    let request_line = request.lines().next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or("/");

    if method != "GET" {
        return write_json(&mut stream, 405, json!({"error": "method_not_allowed"}));
    }

    let (path, query) = split_target(target);
    let result = match path {
        "/healthz" => Ok(json!({
            "status": "ok",
            "version": env!("CARGO_PKG_VERSION")
        })),
        "/v1/status" => {
            let registry = registry
                .lock()
                .map_err(|_| "registry mutex poisoned".to_string())?;
            registry
                .status()
                .map(|status| json!({
                    "daemon": {
                        "version": env!("CARGO_PKG_VERSION"),
                        "status": "running"
                    },
                    "registry": status
                }))
                .map_err(|error| error.to_string())
        }
        "/v1/services" => {
            let include_hidden = query_flag(query, "all");
            let registry = registry
                .lock()
                .map_err(|_| "registry mutex poisoned".to_string())?;
            registry
                .list_services(include_hidden)
                .map(|services| json!({"services": services}))
                .map_err(|error| error.to_string())
        }
        "/v1/projects" => {
            let registry = registry
                .lock()
                .map_err(|_| "registry mutex poisoned".to_string())?;
            registry
                .list_projects()
                .map(|projects| json!({"projects": projects}))
                .map_err(|error| error.to_string())
        }
        "/v1/events" => {
            let after = query_value(query, "after")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(0);
            let limit = query_value(query, "limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(200);
            let registry = registry
                .lock()
                .map_err(|_| "registry mutex poisoned".to_string())?;
            registry
                .list_events(after, limit)
                .map(|events| json!({"events": events}))
                .map_err(|error| error.to_string())
        }
        _ => {
            write_json(&mut stream, 404, json!({"error": "not_found"}))?;
            return Ok(());
        }
    };

    match result {
        Ok(body) => write_json(&mut stream, 200, body),
        Err(error) => write_json(
            &mut stream,
            500,
            json!({"error": "internal_error", "message": error}),
        ),
    }
}

fn split_target(target: &str) -> (&str, &str) {
    target.split_once('?').unwrap_or((target, ""))
}

fn query_flag(query: &str, key: &str) -> bool {
    query_value(query, key)
        .map(|value| matches!(value, "1" | "true" | "yes"))
        .unwrap_or(false)
}

fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let (candidate, value) = pair.split_once('=')?;
        (candidate == key).then_some(value)
    })
}

fn write_json(stream: &mut TcpStream, status: u16, body: serde_json::Value) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(&body).map_err(|error| error.to_string())?;
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Internal Server Error",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(header.as_bytes())
        .and_then(|_| stream.write_all(&body))
        .map_err(|error| error.to_string())
}

fn value_after(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn default_db_path() -> PathBuf {
    if let Ok(home) = std::env::var("AGENTDOCK_HOME") {
        return PathBuf::from(home).join("agentdock.db");
    }

    let base = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    base.join(".agentdock").join("agentdock.db")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_event_query() {
        let (_, query) = split_target("/v1/events?after=12&limit=50");
        assert_eq!(query_value(query, "after"), Some("12"));
        assert_eq!(query_value(query, "limit"), Some("50"));
    }

    #[test]
    fn boolean_query_flags_are_explicit() {
        assert!(query_flag("all=1", "all"));
        assert!(!query_flag("", "all"));
    }
}
