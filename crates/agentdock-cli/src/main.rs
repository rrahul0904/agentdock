use agentdock_core::ServiceClassification;
use framework_detection::enrich_service;
use process_discovery::{DiscoveryOptions, NativeDiscovery, ServiceDiscovery};
use project_resolver::resolve_project;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("scan") => scan(&args[1..]),
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
            }

            if !include_all {
                services.retain(|service| service.is_default_visible());
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
                println!("No matching listening services discovered.");
                println!("Tip: use `agentdock scan --all` to include system/unknown listeners.");
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
                    .map(|project| project.name.as_str())
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
        let lsof = std::process::Command::new("lsof").arg("-v").output();
        let ps = std::process::Command::new("ps").arg("--help").output();

        println!(
            "  lsof: {}",
            if lsof.is_ok() { "available" } else { "missing" }
        );
        println!("  ps: {}", if ps.is_ok() { "available" } else { "missing" });
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
}

fn help() {
    println!("AgentDock - AI local development control plane");
    println!();
    println!("Usage:");
    println!("  agentdock scan [--json] [--all] [--udp]");
    println!("  agentdock doctor");
}
