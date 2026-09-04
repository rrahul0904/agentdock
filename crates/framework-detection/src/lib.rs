use agentdock_core::{
    ContainerIdentity, ContainerRuntime, Framework, Service, ServiceClassification,
};

const SYSTEM_COMMANDS: &[&str] = &[
    "controlcenter",
    "rapportd",
    "sharingd",
    "airportd",
    "coreaudiod",
    "bluetoothd",
    "locationd",
    "trustd",
    "distnoted",
    "configd",
    "mDNSResponder",
];

pub fn enrich_service(service: &mut Service) {
    let haystack = searchable_text(service);
    service.framework = detect_framework(&haystack);
    service.container = detect_container(&haystack);
    service.classification = classify(&haystack, &service.framework, service.project.is_some());
}

pub fn detect_framework(text: &str) -> Framework {
    let lower = text.to_ascii_lowercase();

    if contains_any(&lower, &["next dev", "next-server", "next/dist"]) {
        Framework::NextJs
    } else if contains_any(&lower, &["vite", "vitest --ui"]) {
        Framework::Vite
    } else if lower.contains("fastapi") {
        Framework::FastApi
    } else if lower.contains("uvicorn") {
        Framework::Uvicorn
    } else if contains_any(&lower, &["manage.py runserver", "django"]) {
        Framework::Django
    } else if contains_any(&lower, &["rails server", "puma"]) {
        Framework::Rails
    } else if contains_any(&lower, &["spring-boot", "org.springframework.boot"]) {
        Framework::SpringBoot
    } else if contains_any(&lower, &["postgres", "postmaster"]) {
        Framework::Postgres
    } else if lower.contains("redis-server") {
        Framework::Redis
    } else if contains_any(&lower, &["mailhog", "mailpit"]) {
        Framework::Mailhog
    } else if lower.contains("docker-proxy") || lower.contains("com.docker.backend") {
        Framework::DockerProxy
    } else if lower.contains("podman") {
        Framework::PodmanProxy
    } else if contains_any(&lower, &["node ", "/node", " node"]) || lower == "node" {
        Framework::Node
    } else if contains_any(&lower, &["python ", "python3", "/python"]) {
        Framework::Python
    } else if looks_like_go_binary(&lower) {
        Framework::Go
    } else {
        Framework::Unknown
    }
}

fn classify(text: &str, framework: &Framework, has_project: bool) -> ServiceClassification {
    let lower = text.to_ascii_lowercase();

    if SYSTEM_COMMANDS
        .iter()
        .any(|command| lower.contains(&command.to_ascii_lowercase()))
    {
        return ServiceClassification::System;
    }

    if matches!(
        framework,
        Framework::Postgres | Framework::Redis | Framework::Mailhog
    ) {
        return ServiceClassification::Infrastructure;
    }

    if has_project
        || !matches!(
            framework,
            Framework::Unknown | Framework::Postgres | Framework::Redis | Framework::Mailhog
        )
    {
        return ServiceClassification::Development;
    }

    ServiceClassification::Unknown
}

fn detect_container(text: &str) -> Option<ContainerIdentity> {
    let lower = text.to_ascii_lowercase();

    if lower.contains("docker-proxy") || lower.contains("com.docker.backend") {
        Some(ContainerIdentity {
            runtime: ContainerRuntime::Docker,
            id: None,
            name: None,
        })
    } else if lower.contains("podman") {
        Some(ContainerIdentity {
            runtime: ContainerRuntime::Podman,
            id: None,
            name: None,
        })
    } else {
        None
    }
}

fn searchable_text(service: &Service) -> String {
    [
        service.command.as_deref(),
        service.command_line.as_deref(),
        service
            .working_directory
            .as_ref()
            .and_then(|path| path.to_str()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ")
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn looks_like_go_binary(text: &str) -> bool {
    text.contains("/go-build")
        || text.contains(" go run ")
        || text.starts_with("go run ")
        || text.ends_with(".test")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_vite() {
        assert_eq!(
            detect_framework("node /repo/node_modules/vite/bin/vite.js"),
            Framework::Vite
        );
    }

    #[test]
    fn detects_fastapi_via_uvicorn() {
        assert_eq!(
            detect_framework("python -m uvicorn app.main:app --reload"),
            Framework::Uvicorn
        );
    }

    #[test]
    fn detects_postgres() {
        assert_eq!(
            detect_framework("/usr/local/bin/postgres -D /data"),
            Framework::Postgres
        );
    }
}
