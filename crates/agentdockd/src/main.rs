mod http;

use agent_attribution::enrich_agent;
use agentdock_context::{
    build_workspace_map, render_context_capsule, Confidence, ContextSnapshot, ResourceFact,
    ResourceKind, WorkspaceMapOptions,
};
use agentdock_core::{LifecycleState, Service};
use agentdock_proxy::{ProxyTarget, TargetResolver};
use agentdock_registry::{now_ms, Registry, ServiceRecord};
use framework_detection::enrich_service;
use port_manager::PortManager;
use process_discovery::{DiscoveryOptions, NativeDiscovery, ServiceDiscovery};
use project_resolver::resolve_project;
use serde_json::{json, Value};
use std::fs;
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const DEFAULT_BIND: &str = "127.0.0.1:7317";
const DEFAULT_PROXY_BIND: &str = "127.0.0.1:7777";
const DEFAULT_INTERVAL_MS: u64 = 2_000;
const DEFAULT_ORPHAN_AFTER_MS: i64 = 30_000;

fn main() {
    if let Err(error) = run() {
        eprintln!("agentdockd failed: {error}");
        std::process::exit(1);
    }
}

#[derive(Clone)]
struct ProxyRuntime {
    enabled: bool,
    bind: SocketAddr,
}

struct DaemonState {
    registry: Arc<Mutex<Registry>>,
    ports: Arc<Mutex<PortManager>>,
    proxy: ProxyRuntime,
}

struct ApiResponse {
    status: u16,
    body: Value,
}

impl ApiResponse {
    fn new(status: u16, body: Value) -> Self {
        Self { status, body }
    }

    fn ok(body: Value) -> Self {
        Self::new(200, body)
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let bind = value_after(&args, "--bind")
        .unwrap_or_else(|| DEFAULT_BIND.to_string());
    let bind_addr = parse_bind(&bind, &args)?;

    let proxy_bind = value_after(&args, "--proxy-bind")
        .unwrap_or_else(|| DEFAULT_PROXY_BIND.to_string());
    let proxy_bind_addr = parse_bind(&proxy_bind, &args)?;

    let interval_ms = value_after(&args, "--interval-ms")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_INTERVAL_MS)
        .max(250);

    let orphan_after_ms = value_after(&args, "--orphan-after-ms")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(DEFAULT_ORPHAN_AFTER_MS);

    let include_udp = args.iter().any(|arg| arg == "--udp");
    let proxy_enabled = !args.iter().any(|arg| arg == "--no-proxy");

    let db_path = value_after(&args, "--db")
        .map(PathBuf::from)
        .unwrap_or_else(default_db_path);

    if let Some(parent) = db_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }

    let mut registry = Registry::open(&db_path)
        .map_err(|error| error.to_string())?;
    registry.set_orphan_after_ms(orphan_after_ms);

    let registry = Arc::new(Mutex::new(registry));
    let ports = Arc::new(Mutex::new(PortManager::default()));

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

    if proxy_enabled {
        let resolver: Arc<dyn TargetResolver> = Arc::new(RegistryResolver {
            registry: Arc::clone(&registry),
        });

        thread::spawn(move || {
            if let Err(error) = agentdock_proxy::serve(proxy_bind_addr, resolver) {
                eprintln!("AgentDock proxy failed: {error}");
            }
        });
    }

    let state = DaemonState {
        registry,
        ports,
        proxy: ProxyRuntime {
            enabled: proxy_enabled,
            bind: proxy_bind_addr,
        },
    };

    let listener = TcpListener::bind(bind_addr)
        .map_err(|error| format!("failed to bind local API at {bind}: {error}"))?;

    println!("AgentDock daemon {}", env!("CARGO_PKG_VERSION"));
    println!("  API: http://{bind}");
    println!("  DB: {}", db_path.display());
    println!("  scan interval: {interval_ms} ms");
    println!("  orphan threshold: {orphan_after_ms} ms");

    if proxy_enabled {
        println!("  proxy: http://{proxy_bind}");
    } else {
        println!("  proxy: disabled");
    }

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(error) = handle_client(stream, &state) {
                    eprintln!("agentdockd API error: {error}");
                }
            }
            Err(error) => eprintln!("agentdockd accept error: {error}"),
        }
    }

    Ok(())
}

struct RegistryResolver {
    registry: Arc<Mutex<Registry>>,
}

impl TargetResolver for RegistryResolver {
    fn resolve(&self, hostname: &str) -> Result<ProxyTarget, String> {
        let registry = self
            .registry
            .lock()
            .map_err(|_| "registry mutex poisoned".to_string())?;

        let Some(resolution) = registry
            .resolve_hostname(hostname)
            .map_err(|error| error.to_string())?
        else {
            return Ok(ProxyTarget::Unknown);
        };

        let Some(service) = resolution.service else {
            return Ok(ProxyTarget::Unavailable);
        };

        Ok(ProxyTarget::Forward(service_socket(&service)?))
    }
}

fn service_socket(record: &ServiceRecord) -> Result<SocketAddr, String> {
    let address = record
        .service
        .bind_address
        .as_deref()
        .unwrap_or("127.0.0.1");

    let ip = match address {
        "*" | "0.0.0.0" | "localhost" => IpAddr::from([127, 0, 0, 1]),
        "::" => "::1"
            .parse::<IpAddr>()
            .map_err(|error| error.to_string())?,
        value => value
            .parse::<IpAddr>()
            .map_err(|error| format!("unsupported service bind address {value}: {error}"))?,
    };

    Ok(SocketAddr::new(ip, record.service.port))
}

fn reconcile_once(
    registry: &Arc<Mutex<Registry>>,
    include_udp: bool,
) -> Result<(), String> {
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
    let mut services = discovery
        .scan()
        .map_err(|error| error.to_string())?;

    for service in &mut services {
        if let Some(cwd) = service.working_directory.as_deref() {
            service.project = Some(resolve_project(cwd));
        }

        enrich_service(service);
        enrich_agent(service);
    }

    Ok(services)
}

fn handle_client(
    mut stream: TcpStream,
    state: &DaemonState,
) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|error| error.to_string())?;

    let request = http::read_request(&mut stream)
        .map_err(|error| error.to_string())?;

    let response = route_api(&request, state)?;

    http::write_json(
        &mut stream,
        response.status,
        response.body,
    )
    .map_err(|error| error.to_string())
}

fn route_api(
    request: &http::HttpRequest,
    state: &DaemonState,
) -> Result<ApiResponse, String> {
    let (path, query) = split_target(&request.target);

    match (request.method.as_str(), path) {
        ("GET", "/healthz") => Ok(ApiResponse::ok(json!({
            "status": "ok",
            "version": env!("CARGO_PKG_VERSION")
        }))),

        ("GET", "/v1/status") => {
            let registry = lock_registry(state)?;
            let status = registry
                .status()
                .map_err(|error| error.to_string())?;

            Ok(ApiResponse::ok(json!({
                "daemon": {
                    "version": env!("CARGO_PKG_VERSION"),
                    "status": "running"
                },
                "proxy": {
                    "enabled": state.proxy.enabled,
                    "bind": state.proxy.bind.to_string()
                },
                "registry": status
            })))
        }

        ("GET", "/v1/services") => {
            let include_hidden = query_flag(query, "all");
            let registry = lock_registry(state)?;
            let services = registry
                .list_services(include_hidden)
                .map_err(|error| error.to_string())?;

            Ok(ApiResponse::ok(json!({
                "services": services
            })))
        }

        ("GET", "/v1/projects") => {
            let registry = lock_registry(state)?;
            let projects = registry
                .list_projects()
                .map_err(|error| error.to_string())?;

            Ok(ApiResponse::ok(json!({
                "projects": projects
            })))
        }

        ("GET", "/v1/context") => context_response(query, state),

        ("GET", "/v1/routes") => {
            let registry = lock_registry(state)?;
            let routes = registry
                .list_routes()
                .map_err(|error| error.to_string())?;

            Ok(ApiResponse::ok(json!({
                "routes": routes
            })))
        }

        ("GET", "/v1/events") => {
            let after = query_value(query, "after")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(0);

            let limit = query_value(query, "limit")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(200);

            let registry = lock_registry(state)?;
            let events = registry
                .list_events(after, limit)
                .map_err(|error| error.to_string())?;

            Ok(ApiResponse::ok(json!({
                "events": events
            })))
        }

        ("GET", "/v1/preview") => preview_response(query, state),

        ("POST", "/v1/ports/reserve") => reserve_port(request, state),

        ("POST", "/v1/ports/release") => release_port(request, state),

        ("GET" | "POST", _) => Ok(ApiResponse::new(
            404,
            json!({"error": "not_found"}),
        )),

        _ => Ok(ApiResponse::new(
            405,
            json!({"error": "method_not_allowed"}),
        )),
    }
}


fn context_response(
    query: &str,
    state: &DaemonState,
) -> Result<ApiResponse, String> {
    let Some(project_id) = query_value(query, "project_id") else {
        return Ok(ApiResponse::new(
            400,
            json!({
                "error": "project_id_required"
            }),
        ));
    };

    let max_bytes = query_value(query, "max_bytes")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(4 * 1024)
        .clamp(256, 32 * 1024);

    let max_entries = query_value(query, "max_entries")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(2_000)
        .clamp(100, 10_000);

    let max_depth = query_value(query, "max_depth")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(5)
        .clamp(1, 12);

    let (project, services) = {
        let registry = lock_registry(state)?;
        let projects = registry
            .list_projects()
            .map_err(|error| error.to_string())?;

        let Some(project) = projects
            .into_iter()
            .find(|project| project.id == project_id)
        else {
            return Ok(ApiResponse::new(
                404,
                json!({
                    "error": "project_not_found"
                }),
            ));
        };

        let services = registry
            .list_services(true)
            .map_err(|error| error.to_string())?;

        (project, services)
    };

    let workspace = build_workspace_map(
        &project.identity.root,
        WorkspaceMapOptions {
            max_entries,
            max_depth,
        },
    )
    .map_err(|error| format!("failed to map project workspace: {error}"))?;

    let facts = registry_resource_facts(
        &project.id,
        project.canonical_hostname.as_deref(),
        project.identity.git_worktree,
        &services,
    );

    let snapshot = ContextSnapshot::new(
        project.id.clone(),
        workspace,
        facts,
    );
    let capsule = render_context_capsule(&snapshot, max_bytes);

    Ok(ApiResponse::ok(json!({
        "project_id": project.id,
        "capsule": capsule,
        "source": "agentdock-registry+workspace-metadata"
    })))
}

fn registry_resource_facts(
    project_id: &str,
    canonical_hostname: Option<&str>,
    git_worktree: bool,
    services: &[ServiceRecord],
) -> Vec<ResourceFact> {
    let mut facts = Vec::new();

    facts.push(ResourceFact {
        kind: ResourceKind::Worktree,
        key: "git-worktree".into(),
        value: git_worktree.to_string(),
        source: "agentdock-registry".into(),
        confidence: Confidence::High,
        priority: 220,
        sensitive: false,
    });

    if let Some(hostname) = canonical_hostname {
        facts.push(ResourceFact {
            kind: ResourceKind::Runtime,
            key: "canonical-hostname".into(),
            value: hostname.to_string(),
            source: "agentdock-registry".into(),
            confidence: Confidence::High,
            priority: 210,
            sensitive: false,
        });
    }

    let mut project_services = services
        .iter()
        .filter(|record| {
            record.project_id.as_deref() == Some(project_id)
                && record.state == LifecycleState::Active
        })
        .collect::<Vec<_>>();
    project_services.sort_by(|left, right| left.id.cmp(&right.id));

    let omitted = project_services.len().saturating_sub(128);

    for record in project_services.into_iter().take(128) {
        let prefix = format!("service.{}", record.id);

        facts.push(ResourceFact {
            kind: ResourceKind::Port,
            key: format!("{prefix}.port"),
            value: record.service.port.to_string(),
            source: "agentdock-registry".into(),
            confidence: Confidence::High,
            priority: 240,
            sensitive: false,
        });
        facts.push(ResourceFact {
            kind: ResourceKind::Service,
            key: format!("{prefix}.protocol"),
            value: format!("{:?}", record.service.protocol),
            source: "agentdock-registry".into(),
            confidence: Confidence::High,
            priority: 200,
            sensitive: false,
        });
        facts.push(ResourceFact {
            kind: ResourceKind::Service,
            key: format!("{prefix}.framework"),
            value: format!("{:?}", record.service.framework),
            source: "agentdock-registry".into(),
            confidence: Confidence::High,
            priority: 190,
            sensitive: false,
        });
        facts.push(ResourceFact {
            kind: ResourceKind::Service,
            key: format!("{prefix}.classification"),
            value: format!("{:?}", record.service.classification),
            source: "agentdock-registry".into(),
            confidence: Confidence::High,
            priority: 180,
            sensitive: false,
        });

        if let Some(agent) = record.service.agent.as_ref() {
            facts.push(ResourceFact {
                kind: ResourceKind::Tool,
                key: format!("{prefix}.agent-kind"),
                value: format!("{:?}", agent.kind),
                source: "agentdock-registry".into(),
                confidence: Confidence::High,
                priority: 170,
                sensitive: false,
            });
        }
    }

    if omitted > 0 {
        facts.push(ResourceFact {
            kind: ResourceKind::Note,
            key: "services-omitted".into(),
            value: omitted.to_string(),
            source: "agentdock-registry".into(),
            confidence: Confidence::High,
            priority: 10,
            sensitive: false,
        });
    }

    facts
}

fn preview_response(
    query: &str,
    state: &DaemonState,
) -> Result<ApiResponse, String> {
    if !state.proxy.enabled {
        return Ok(ApiResponse::new(
            503,
            json!({
                "error": "proxy_disabled"
            }),
        ));
    }

    let Some(project_id) = query_value(query, "project_id") else {
        return Ok(ApiResponse::new(
            400,
            json!({
                "error": "project_id_required"
            }),
        ));
    };

    let registry = lock_registry(state)?;
    let projects = registry
        .list_projects()
        .map_err(|error| error.to_string())?;

    let Some(project) = projects
        .into_iter()
        .find(|project| project.id == project_id)
    else {
        return Ok(ApiResponse::new(
            404,
            json!({
                "error": "project_not_found"
            }),
        ));
    };

    let Some(hostname) = project.canonical_hostname else {
        return Ok(ApiResponse::new(
            404,
            json!({
                "error": "route_not_found"
            }),
        ));
    };

    let port = state.proxy.bind.port();
    let url = if port == 80 {
        format!("http://{hostname}")
    } else {
        format!("http://{hostname}:{port}")
    };

    Ok(ApiResponse::ok(json!({
        "project_id": project_id,
        "hostname": hostname,
        "url": url
    })))
}

fn reserve_port(
    request: &http::HttpRequest,
    state: &DaemonState,
) -> Result<ApiResponse, String> {
    let body = match parse_json_body(request) {
        Ok(body) => body,
        Err(error) => {
            return Ok(ApiResponse::new(
                400,
                json!({
                    "error": "invalid_json",
                    "message": error
                }),
            ));
        }
    };

    let Some(owner) = body
        .get("owner")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|owner| !owner.is_empty() && owner.len() <= 128)
    else {
        return Ok(ApiResponse::new(
            400,
            json!({
                "error": "valid_owner_required"
            }),
        ));
    };

    let mut ports = state
        .ports
        .lock()
        .map_err(|_| "port manager mutex poisoned".to_string())?;

    let Some(port) = ports.reserve(owner.to_string()) else {
        return Ok(ApiResponse::new(
            503,
            json!({
                "error": "no_port_available"
            }),
        ));
    };

    Ok(ApiResponse::new(
        201,
        json!({
            "port": port,
            "owner": owner,
            "ttl_seconds": 60
        }),
    ))
}

fn release_port(
    request: &http::HttpRequest,
    state: &DaemonState,
) -> Result<ApiResponse, String> {
    let body = match parse_json_body(request) {
        Ok(body) => body,
        Err(error) => {
            return Ok(ApiResponse::new(
                400,
                json!({
                    "error": "invalid_json",
                    "message": error
                }),
            ));
        }
    };

    let Some(owner) = body
        .get("owner")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|owner| !owner.is_empty())
    else {
        return Ok(ApiResponse::new(
            400,
            json!({
                "error": "owner_required"
            }),
        ));
    };

    let Some(port) = body
        .get("port")
        .and_then(Value::as_u64)
        .and_then(|port| u16::try_from(port).ok())
    else {
        return Ok(ApiResponse::new(
            400,
            json!({
                "error": "valid_port_required"
            }),
        ));
    };

    let mut ports = state
        .ports
        .lock()
        .map_err(|_| "port manager mutex poisoned".to_string())?;

    if !ports.release_owned(port, owner) {
        return Ok(ApiResponse::new(
            409,
            json!({
                "error": "reservation_not_owned",
                "port": port
            }),
        ));
    }

    Ok(ApiResponse::ok(json!({
        "released": true,
        "port": port
    })))
}

fn parse_json_body(
    request: &http::HttpRequest,
) -> Result<Value, String> {
    serde_json::from_slice(&request.body)
        .map_err(|error| format!("invalid JSON body: {error}"))
}

fn lock_registry(
    state: &DaemonState,
) -> Result<std::sync::MutexGuard<'_, Registry>, String> {
    state
        .registry
        .lock()
        .map_err(|_| "registry mutex poisoned".to_string())
}

fn parse_bind(
    value: &str,
    args: &[String],
) -> Result<SocketAddr, String> {
    let address: SocketAddr = value
        .parse()
        .map_err(|error| format!("invalid bind address {value}: {error}"))?;

    let allow_non_loopback = args
        .iter()
        .any(|arg| arg == "--allow-non-loopback");

    if !address.ip().is_loopback() && !allow_non_loopback {
        return Err(format!(
            "refusing non-loopback bind {value}; pass --allow-non-loopback to acknowledge the risk"
        ));
    }

    Ok(address)
}

fn split_target(target: &str) -> (&str, &str) {
    target
        .split_once('?')
        .unwrap_or((target, ""))
}

fn query_flag(
    query: &str,
    key: &str,
) -> bool {
    query_value(query, key)
        .map(|value| matches!(value, "1" | "true" | "yes"))
        .unwrap_or(false)
}

fn query_value<'a>(
    query: &'a str,
    key: &str,
) -> Option<&'a str> {
    query
        .split('&')
        .find_map(|pair| {
            let (candidate, value) = pair.split_once('=')?;
            (candidate == key).then_some(value)
        })
}

fn value_after(
    args: &[String],
    flag: &str,
) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn default_db_path() -> PathBuf {
    if let Ok(home) = std::env::var("AGENTDOCK_HOME") {
        return PathBuf::from(home)
            .join("agentdock.db");
    }

    let base = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    base.join(".agentdock")
        .join("agentdock.db")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_event_query() {
        let (_, query) = split_target(
            "/v1/events?after=12&limit=50"
        );

        assert_eq!(
            query_value(query, "after"),
            Some("12")
        );

        assert_eq!(
            query_value(query, "limit"),
            Some("50")
        );
    }


    #[test]
    fn context_api_uses_registry_facts_without_sensitive_files() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "agentdock-context-api-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src")).expect("src");
        std::fs::write(root.join("Cargo.toml"), "[package]\nname='fixture'\n")
            .expect("cargo");
        std::fs::write(root.join("src/lib.rs"), "pub fn fixture() {}")
            .expect("source");
        std::fs::write(root.join(".env"), "API_KEY=never-render")
            .expect("env");

        let project = agentdock_core::ProjectIdentity {
            name: "context-fixture".into(),
            root: root.clone(),
            git_root: None,
            git_worktree: true,
        };
        let service = agentdock_core::Service {
            pid: Some(1234),
            port: 4321,
            protocol: agentdock_core::Protocol::Tcp,
            bind_address: Some("127.0.0.1".into()),
            command: Some("ignored-sensitive-command".into()),
            command_line: Some("ignored --token never-render".into()),
            working_directory: Some(root.clone()),
            project: Some(project),
            framework: agentdock_core::Framework::Vite,
            container: None,
            agent: Some(agentdock_core::AgentIdentity {
                kind: agentdock_core::AgentKind::Codex,
                session_id: Some("session-not-exported".into()),
            }),
            classification: agentdock_core::ServiceClassification::Development,
        };

        let mut registry = Registry::in_memory().expect("registry");
        registry
            .reconcile(&[service], 100)
            .expect("reconcile");
        let project_id = registry
            .list_projects()
            .expect("projects")
            .into_iter()
            .next()
            .expect("project")
            .id;

        let state = DaemonState {
            registry: Arc::new(Mutex::new(registry)),
            ports: Arc::new(Mutex::new(PortManager::default())),
            proxy: ProxyRuntime {
                enabled: false,
                bind: "127.0.0.1:7777".parse().expect("bind"),
            },
        };
        let request = http::HttpRequest {
            method: "GET".into(),
            target: format!("/v1/context?project_id={project_id}&max_bytes=4096"),
            body: Vec::new(),
        };

        let response = route_api(&request, &state).expect("context response");
        assert_eq!(response.status, 200);

        let text = response.body["capsule"]["text"]
            .as_str()
            .expect("capsule text");
        assert!(text.contains("4321"));
        assert!(text.contains("Vite"));
        assert!(text.contains("Codex"));
        assert!(!text.contains(".env"));
        assert!(!text.contains("never-render"));
        assert!(!text.contains("session-not-exported"));
        assert!(!text.contains("ignored-sensitive-command"));

        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn wildcard_service_bind_targets_loopback() {
        let service = ServiceRecord {
            id: "svc_test".into(),
            project_id: None,
            service: agentdock_core::Service {
                pid: None,
                port: 3000,
                protocol: agentdock_core::Protocol::Tcp,
                bind_address: Some("*".into()),
                command: None,
                command_line: None,
                working_directory: None,
                project: None,
                framework: agentdock_core::Framework::Unknown,
                container: None,
                agent: None,
                classification: agentdock_core::ServiceClassification::Unknown,
            },
            state: agentdock_core::LifecycleState::Active,
            first_seen_ms: 0,
            last_seen_ms: 0,
            missing_since_ms: None,
        };

        assert_eq!(
            service_socket(&service).unwrap(),
            "127.0.0.1:3000"
                .parse::<SocketAddr>()
                .unwrap()
        );
    }
}
