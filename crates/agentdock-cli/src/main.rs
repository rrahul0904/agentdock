use agent_attribution::enrich_agent;
use agentdock_core::ServiceClassification;
use agentdock_supervisor::{
    load_control_receipts, load_snapshot, new_control_request_id, render_control_receipts,
    render_decisions, render_projects, render_risks, render_status, render_tasks,
    render_workers, write_control_request, SupervisorControlRequest, SupervisorSnapshot,
};
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
        Some("doctor") => doctor(),
        Some("supervisor") => supervisor(&args[1..]),
        Some("console") => console(&args[1..]),
        Some("watch") => watch(&args[1..]),
        Some("control") => control(&args[1..]),
        Some("receipts") => receipts(&args[1..]),
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
                println!("{}", serde_json::to_string_pretty(&services).expect("serialize services"));
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
                let pid = service.pid.map(|pid| pid.to_string()).unwrap_or_else(|| "-".into());
                let project = service.project.as_ref().map(|p| p.name.as_str()).unwrap_or("-");
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
        "status" => "/v1/status",
        "services" if include_all => "/v1/services?all=1",
        "services" => "/v1/services",
        "projects" => "/v1/projects",
        "routes" => "/v1/routes",
        "events" => "/v1/events?limit=200",
        _ => {
            eprintln!("Unknown daemon command: {command}");
            eprintln!("Use: agentdock daemon [status|services|projects|routes|events] [--all]");
            std::process::exit(2);
        }
    };

    match http_get(&addr, path) {
        Ok(body) => match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(value) => println!("{}", serde_json::to_string_pretty(&value).expect("serialize json")),
            Err(_) => println!("{body}"),
        },
        Err(error) => {
            eprintln!("Unable to reach AgentDock daemon at {addr}: {error}");
            eprintln!("Start it with: cargo run -p agentdockd");
            std::process::exit(1);
        }
    }
}

fn http_get(addr: &str, path: &str) -> Result<String, String> {
    let mut stream = TcpStream::connect(addr).map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|error| error.to_string())?;

    let request = format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).map_err(|error| error.to_string())?;

    let mut response = String::new();
    stream.read_to_string(&mut response).map_err(|error| error.to_string())?;
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "invalid HTTP response".to_string())?;

    let status = headers.lines().next().unwrap_or_default();
    if !status.contains(" 200 ") {
        return Err(format!("{status}: {body}"));
    }
    Ok(body.to_string())
}

fn snapshot_path(args: &[String]) -> Result<String, String> {
    let index = args
        .iter()
        .position(|arg| arg == "--snapshot")
        .ok_or_else(|| "missing required --snapshot <file>".to_string())?;
    args.get(index + 1)
        .cloned()
        .ok_or_else(|| "missing value after --snapshot".to_string())
}

fn supervisor(args: &[String]) {
    let path = match snapshot_path(args) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("AgentDock supervisor refused: {error}");
            std::process::exit(2);
        }
    };

    match load_snapshot(&path) {
        Ok(snapshot) => {
            if args.iter().any(|arg| arg == "--json") {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&snapshot).expect("serialize snapshot")
                );
                return;
            }

            print!("{}", render_status(&snapshot));
            println!();
            print!("{}", render_projects(&snapshot));
            println!();
            print!("{}", render_workers(&snapshot));
            println!();
            print!("{}", render_risks(&snapshot));
            println!();
            print!("{}", render_decisions(&snapshot));
        }
        Err(error) => {
            eprintln!("AgentDock supervisor refused: {error}");
            std::process::exit(2);
        }
    }
}

fn console(args: &[String]) {
    let path = match snapshot_path(args) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("AgentDock console refused: {error}");
            std::process::exit(2);
        }
    };

    let mut snapshot = match load_snapshot(&path) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            eprintln!("AgentDock console refused: {error}");
            std::process::exit(2);
        }
    };
    let forge_root = arg_value(args, "--forge-root");

    println!("AgentDock Supervisor Console");
    println!("Snapshot: {path}");
    match forge_root.as_deref() {
        Some(root) => println!("Forge root: {root}"),
        None => println!("Forge controls: disabled (start console with --forge-root <path>)"),
    }
    println!("Type 'help' for commands.");
    print!("{}", render_status(&snapshot));

    loop {
        print!("agentdock> ");
        if std::io::stdout().flush().is_err() {
            break;
        }

        let mut line = String::new();
        match std::io::stdin().read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) => {
                eprintln!("console input failed: {error}");
                break;
            }
        }

        let input = line.trim();
        let parts = input.split_whitespace().collect::<Vec<_>>();
        match input {
            "" => {}
            "status" => print!("{}", render_status(&snapshot)),
            "projects" => print!("{}", render_projects(&snapshot)),
            "workers" => print!("{}", render_workers(&snapshot)),
            "tasks" => print!("{}", render_tasks(&snapshot)),
            "risks" => print!("{}", render_risks(&snapshot)),
            "decisions" => print!("{}", render_decisions(&snapshot)),
            "receipts" => match forge_root.as_deref() {
                Some(root) => match load_control_receipts(root, 20) {
                    Ok(items) => print!("{}", render_control_receipts(&items)),
                    Err(error) => eprintln!("receipt read refused: {error}"),
                },
                None => eprintln!(
                    "receipts disabled: start console with --forge-root <path>"
                ),
            },
            "refresh" => match load_snapshot(&path) {
                Ok(updated) => {
                    snapshot = updated;
                    println!("snapshot refreshed");
                    print!("{}", render_status(&snapshot));
                }
                Err(error) => eprintln!("refresh refused: {error}"),
            },
            "help" => {
                println!("Commands:");
                println!("  status     supervisor and machine summary");
                println!("  projects   portfolio projects and next actions");
                println!("  workers    active/configured worker state");
                println!("  tasks      task queue and blockers");
                println!("  risks      current supervisor risks");
                println!("  decisions  recent supervisor decisions");
                println!("  receipts   recent Forge control receipts");
                println!("  refresh    reload the snapshot file");
                println!("  pause <project-id> confirm");
                println!("  resume <project-id> confirm");
                println!("  project-priority <project-id> <0-100> confirm");
                println!("  task-priority <task-id> <0-100> confirm");
                println!("  quit       exit the console");
            }
            "quit" | "exit" => break,
            _ if matches!(
                parts.first().copied(),
                Some("pause" | "resume" | "project-priority" | "task-priority")
            ) => {
                if let Some(root) = forge_root.as_deref() {
                    match queue_console_control(root, &parts) {
                        Ok(request_id) => {
                            println!("Supervisor control request queued: {request_id}");
                            println!(
                                "Pending Forge daemon cycle; use 'receipts' to verify application."
                            );
                        }
                        Err(error) => eprintln!("control refused: {error}"),
                    }
                } else {
                    eprintln!(
                        "control disabled: start console with --forge-root <path>"
                    );
                }
            }
            other => eprintln!("unknown console command: {other}"),
        }
    }
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn watch(args: &[String]) {
    let path = match snapshot_path(args) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("AgentDock watch refused: {error}");
            std::process::exit(2);
        }
    };
    let interval_ms = match arg_value(args, "--interval-ms") {
        Some(value) => match value.parse::<u64>() {
            Ok(value) if (100..=60_000).contains(&value) => value,
            _ => {
                eprintln!(
                    "AgentDock watch refused: --interval-ms must be between 100 and 60000"
                );
                std::process::exit(2);
            }
        },
        None => 1_000,
    };

    let mut current = match load_snapshot(&path) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            eprintln!("AgentDock watch refused: {error}");
            std::process::exit(2);
        }
    };

    println!("AgentDock Supervisor Watch");
    println!("Snapshot: {path}");
    println!("Polling every {interval_ms}ms; press Ctrl-C to stop.");
    render_watch_snapshot(&current);

    loop {
        std::thread::sleep(Duration::from_millis(interval_ms));
        match load_snapshot(&path) {
            Ok(updated) if updated != current => {
                current = updated;
                println!();
                println!("--- supervisor state changed ---");
                render_watch_snapshot(&current);
            }
            Ok(_) => {}
            Err(error) => {
                eprintln!("snapshot refresh refused: {error}");
            }
        }
    }
}

fn render_watch_snapshot(snapshot: &SupervisorSnapshot) {
    print!("{}", render_status(snapshot));
    println!();
    print!("{}", render_projects(snapshot));
    if !snapshot.risks.is_empty() {
        println!();
        print!("{}", render_risks(snapshot));
    }
}

fn build_control_request(
    action: &str,
    target: &str,
    priority: Option<i64>,
    request_id: String,
) -> Result<SupervisorControlRequest, String> {
    let result = match action {
        "pause-project" => SupervisorControlRequest::pause_project(
            request_id,
            target.to_string(),
        ),
        "resume-project" => SupervisorControlRequest::resume_project(
            request_id,
            target.to_string(),
        ),
        "set-project-priority" => {
            let priority = priority.ok_or_else(|| {
                "set-project-priority requires an integer priority".to_string()
            })?;
            SupervisorControlRequest::set_project_priority(
                request_id,
                target.to_string(),
                priority,
            )
        }
        "set-task-priority" => {
            let priority = priority.ok_or_else(|| {
                "set-task-priority requires an integer priority".to_string()
            })?;
            SupervisorControlRequest::set_task_priority(
                request_id,
                target.to_string(),
                priority,
            )
        }
        other => return Err(format!("unsupported action {other}")),
    };
    result.map_err(|error| error.to_string())
}

fn queue_control_request(
    root: &str,
    action: &str,
    target: &str,
    priority: Option<i64>,
    request_id: Option<String>,
) -> Result<String, String> {
    let request_id = match request_id {
        Some(value) => value,
        None => new_control_request_id().map_err(|error| error.to_string())?,
    };
    let request = build_control_request(action, target, priority, request_id)?;
    write_control_request(root, &request).map_err(|error| error.to_string())?;
    Ok(request.request_id)
}

fn queue_console_control(root: &str, parts: &[&str]) -> Result<String, String> {
    let confirmed = parts.last().copied() == Some("confirm");
    if !confirmed {
        return Err("explicit trailing 'confirm' is required".into());
    }

    match parts {
        ["pause", project_id, "confirm"] => queue_control_request(
            root,
            "pause-project",
            project_id,
            None,
            None,
        ),
        ["resume", project_id, "confirm"] => queue_control_request(
            root,
            "resume-project",
            project_id,
            None,
            None,
        ),
        ["project-priority", project_id, priority, "confirm"] => {
            let value = priority
                .parse::<i64>()
                .map_err(|_| "priority must be an integer".to_string())?;
            queue_control_request(
                root,
                "set-project-priority",
                project_id,
                Some(value),
                None,
            )
        }
        ["task-priority", task_id, priority, "confirm"] => {
            let value = priority
                .parse::<i64>()
                .map_err(|_| "priority must be an integer".to_string())?;
            queue_control_request(
                root,
                "set-task-priority",
                task_id,
                Some(value),
                None,
            )
        }
        _ => Err("invalid supervisor control command shape".into()),
    }
}

fn receipts(args: &[String]) {
    let root = match arg_value(args, "--root") {
        Some(value) => value,
        None => {
            eprintln!("AgentDock receipts refused: missing required --root <forge-root>");
            std::process::exit(2);
        }
    };
    let limit = match arg_value(args, "--limit") {
        Some(value) => match value.parse::<usize>() {
            Ok(value) if (1..=100).contains(&value) => value,
            _ => {
                eprintln!("AgentDock receipts refused: --limit must be between 1 and 100");
                std::process::exit(2);
            }
        },
        None => 20,
    };

    match load_control_receipts(&root, limit) {
        Ok(items) => {
            if args.iter().any(|arg| arg == "--json") {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&items)
                        .expect("serialize supervisor control receipts")
                );
            } else {
                print!("{}", render_control_receipts(&items));
            }
        }
        Err(error) => {
            eprintln!("AgentDock receipts refused: {error}");
            std::process::exit(2);
        }
    }
}

fn control(args: &[String]) {
    if !args.iter().any(|arg| arg == "--confirm") {
        eprintln!("AgentDock control refused: explicit --confirm is required");
        std::process::exit(2);
    }
    let root = match arg_value(args, "--root") {
        Some(value) => value,
        None => {
            eprintln!("AgentDock control refused: missing required --root <forge-root>");
            std::process::exit(2);
        }
    };
    let action = match args.first().map(String::as_str) {
        Some(value) => value,
        None => {
            eprintln!("AgentDock control refused: missing action");
            std::process::exit(2);
        }
    };
    let target = match args.get(1) {
        Some(value) => value.as_str(),
        None => {
            eprintln!("AgentDock control refused: missing action target");
            std::process::exit(2);
        }
    };
    let priority = if matches!(action, "set-project-priority" | "set-task-priority") {
        match args.get(2).and_then(|value| value.parse::<i64>().ok()) {
            Some(value) => Some(value),
            None => {
                eprintln!(
                    "AgentDock control refused: {action} requires an integer priority"
                );
                std::process::exit(2);
            }
        }
    } else {
        None
    };

    match queue_control_request(
        &root,
        action,
        target,
        priority,
        arg_value(args, "--request-id"),
    ) {
        Ok(request_id) => {
            println!("Supervisor control request queued.");
            println!("  request_id: {request_id}");
            println!("  action: {action}");
            println!("Forge remains authoritative; the request is pending a daemon cycle.");
        }
        Err(error) => {
            eprintln!("AgentDock control refused: {error}");
            std::process::exit(2);
        }
    }
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
        println!("  lsof: {}", if std::process::Command::new("lsof").arg("-v").output().is_ok() { "available" } else { "missing" });
        println!("  ps: {}", if std::process::Command::new("ps").arg("--help").output().is_ok() { "available" } else { "missing" });
    }

    #[cfg(target_os = "windows")]
    {
        let powershell = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", "$PSVersionTable.PSVersion.ToString()"])
            .output();
        println!("  powershell: {}", if powershell.is_ok() { "available" } else { "missing" });
    }

    let addr = std::env::var("AGENTDOCK_ADDR").unwrap_or_else(|_| DEFAULT_DAEMON_ADDR.into());
    println!("  daemon: {}", if http_get(&addr, "/healthz").is_ok() { "reachable" } else { "not running" });
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
    println!("  agentdock daemon events");
    println!("  agentdock supervisor --snapshot <file> [--json]");
    println!("  agentdock console --snapshot <file> [--forge-root <forge-root>]");
    println!("  agentdock watch --snapshot <file> [--interval-ms 1000]");
    println!("  agentdock control pause-project <project-id> --root <forge-root> --confirm");
    println!("  agentdock control resume-project <project-id> --root <forge-root> --confirm");
    println!("  agentdock control set-project-priority <project-id> <0-100> --root <forge-root> --confirm");
    println!("  agentdock control set-task-priority <task-id> <0-100> --root <forge-root> --confirm");
    println!("  agentdock receipts --root <forge-root> [--limit 20] [--json]");
    println!("  agentdock doctor");
}
