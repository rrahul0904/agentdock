use process_discovery::{NativeDiscovery, ServiceDiscovery};
use project_resolver::resolve_project;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("scan") => scan(args.iter().any(|arg| arg == "--json")),
        Some("doctor") => doctor(),
        _ => help(),
    }
}

fn scan(json: bool) {
    let discovery = NativeDiscovery::default();

    match discovery.scan() {
        Ok(mut services) => {
            for service in &mut services {
                if let Some(cwd) = service.working_directory.as_deref() {
                    service.project = Some(resolve_project(cwd));
                }
            }

            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&services)
                        .expect("service model should always serialize")
                );
                return;
            }

            if services.is_empty() {
                println!("No listening TCP services discovered.");
                return;
            }

            println!("{:<8} {:<8} {:<24} {}", "PID", "PORT", "PROJECT", "HOSTNAME");
            for service in services {
                let pid = service.pid.map(|p| p.to_string()).unwrap_or_else(|| "-".into());
                let project = service
                    .project
                    .as_ref()
                    .map(|p| p.name.as_str())
                    .unwrap_or("-");
                let hostname = service.stable_hostname().unwrap_or_else(|| "-".into());
                println!("{:<8} {:<8} {:<24} {}", pid, service.port, project, hostname);
            }
        }
        Err(error) => {
            eprintln!("AgentDock scan failed: {error}");
            std::process::exit(1);
        }
    }
}

fn doctor() {
    println!("AgentDock doctor");
    println!("  platform: {}", std::env::consts::OS);
    println!("  architecture: {}", std::env::consts::ARCH);

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let lsof = std::process::Command::new("lsof").arg("-v").output();
        println!("  lsof: {}", if lsof.is_ok() { "available" } else { "missing" });
    }
}

fn help() {
    println!("AgentDock - AI local development control plane");
    println!();
    println!("Usage:");
    println!("  agentdock scan [--json]");
    println!("  agentdock doctor");
}
