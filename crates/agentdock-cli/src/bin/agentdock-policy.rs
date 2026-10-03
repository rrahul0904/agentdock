use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const REGISTRATION_SCHEMA: &str = "agent-session-registration/v1";
const DELIVERY_RECEIPT_SCHEMA: &str = "agent-policy-delivery-receipt/v1";
const MAX_JSON_BYTES: usize = 64 * 1024;
const MAX_SESSIONS: usize = 512;
const MAX_RECEIPTS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Registration {
    session_id: String,
    project_id: String,
    registered_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeliveryReceipt {
    session_id: String,
    project_id: String,
    idempotency_key: String,
    policy_sha256: String,
    outcome: String,
    reason: Option<String>,
    completed_at: String,
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match run(&args) {
        Ok(output) => print!("{output}"),
        Err(error) => {
            eprintln!("agentdock-policy refused: {error}");
            std::process::exit(2);
        }
    }
}

fn run(args: &[String]) -> Result<String, String> {
    let command = args.first().map(String::as_str).unwrap_or("help");
    if matches!(command, "help" | "--help" | "-h") {
        return Ok(help_text());
    }
    let root = arg_value(args, "--root")
        .map(PathBuf::from)
        .ok_or_else(|| "missing required --root <forge-root>".to_string())?;
    let json_output = args.iter().any(|arg| arg == "--json");

    match command {
        "sessions" => {
            let items = list_sessions(&root)?;
            if json_output {
                let values = items
                    .iter()
                    .map(|item| {
                        json!({
                            "session_id": &item.session_id,
                            "project_id": &item.project_id,
                            "registered_at": &item.registered_at,
                        })
                    })
                    .collect::<Vec<_>>();
                Ok(serde_json::to_string_pretty(&values).expect("serialize sessions") + "\n")
            } else {
                Ok(render_sessions(&items))
            }
        }
        "receipts" => {
            let limit = match arg_value(args, "--limit") {
                Some(value) => value.parse::<usize>().map_err(|_| {
                    format!("--limit must be between 1 and {MAX_RECEIPTS}")
                })?,
                None => 20,
            };
            if !(1..=MAX_RECEIPTS).contains(&limit) {
                return Err(format!(
                    "--limit must be between 1 and {MAX_RECEIPTS}"
                ));
            }
            let items = list_receipts(&root, limit)?;
            if json_output {
                let values = items
                    .iter()
                    .map(|item| {
                        json!({
                            "session_id": &item.session_id,
                            "project_id": &item.project_id,
                            "idempotency_key": &item.idempotency_key,
                            "policy_sha256": &item.policy_sha256,
                            "outcome": &item.outcome,
                            "reason": item.reason.as_deref(),
                            "completed_at": &item.completed_at,
                        })
                    })
                    .collect::<Vec<_>>();
                Ok(serde_json::to_string_pretty(&values).expect("serialize receipts") + "\n")
            } else {
                Ok(render_receipts(&items))
            }
        }
        other => Err(format!("unknown command: {other}")),
    }
}

fn help_text() -> String {
    [
        "AgentDock policy/session viewer",
        "Usage:",
        "  agentdock-policy sessions --root <forge-root> [--json]",
        "  agentdock-policy receipts --root <forge-root> [--limit 20] [--json]",
        "",
        "Read-only: this tool never reads policy request payloads and never writes Forge state.",
        "",
    ]
    .join("\n")
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn safe_id(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 128 {
        return Err(format!("{label} has unsafe identifier syntax"));
    }
    let mut chars = value.chars();
    let first = chars
        .next()
        .ok_or_else(|| format!("{label} has unsafe identifier syntax"))?;
    if !first.is_ascii_alphanumeric() {
        return Err(format!("{label} has unsafe identifier syntax"));
    }
    if chars.any(|ch| !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | ':' | '-'))) {
        return Err(format!("{label} has unsafe identifier syntax"));
    }
    Ok(())
}

fn bounded_text(value: &str, label: &str, max: usize) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > max || value.contains('\0') {
        return Err(format!("{label} must be bounded non-empty text"));
    }
    Ok(())
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn refuse_symlink(path: &Path, label: &str) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "{label} must not be a symlink: {}",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot inspect {label}: {error}")),
    }
}

fn canonical_root(root: &Path) -> Result<PathBuf, String> {
    let root =
        fs::canonicalize(root).map_err(|error| format!("Forge root is unavailable: {error}"))?;
    if !root.is_dir() {
        return Err("Forge root must be a directory".into());
    }
    Ok(root)
}

fn registry_root(root: &Path) -> Result<Option<PathBuf>, String> {
    let root = canonical_root(root)?;
    let ai = root.join(".ai");
    let registry = ai.join("agent-sessions");
    refuse_symlink(&ai, ".ai directory")?;
    refuse_symlink(&registry, "agent-session registry")?;
    if !registry.exists() {
        return Ok(None);
    }
    if !registry.is_dir() {
        return Err("agent-session registry must be a directory".into());
    }
    let resolved = fs::canonicalize(&registry)
        .map_err(|error| format!("cannot resolve agent-session registry: {error}"))?;
    if !resolved.starts_with(&root) {
        return Err("agent-session registry escaped Forge root".into());
    }
    Ok(Some(resolved))
}

fn read_json_object(path: &Path, label: &str) -> Result<Map<String, Value>, String> {
    if path.is_symlink() {
        return Err(format!("{label} symlinks are refused"));
    }
    let bytes = fs::read(path).map_err(|error| format!("cannot read {label}: {error}"))?;
    if bytes.len() > MAX_JSON_BYTES {
        return Err(format!("{label} exceeds 64 KiB"));
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("{label} must be UTF-8 JSON: {error}"))?;
    value
        .as_object()
        .cloned()
        .ok_or_else(|| format!("{label} must be a JSON object"))
}

fn strict_keys(object: &Map<String, Value>, allowed: &[&str], label: &str) -> Result<(), String> {
    let allowed = allowed.iter().copied().collect::<BTreeSet<_>>();
    let unknown = object
        .keys()
        .filter(|key| !allowed.contains(key.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if unknown.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{label} has unknown keys: {}",
            unknown.join(", ")
        ))
    }
}

fn required_string(
    object: &Map<String, Value>,
    field: &str,
    label: &str,
) -> Result<String, String> {
    object
        .get(field)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("{label} {field} must be text"))
}

fn optional_string(
    object: &Map<String, Value>,
    field: &str,
    label: &str,
) -> Result<Option<String>, String> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!("{label} {field} must be text or null")),
    }
}

fn parse_registration(path: &Path, expected_session_id: &str) -> Result<Registration, String> {
    let object = read_json_object(path, "session registration")?;
    strict_keys(
        &object,
        &[
            "schema_version",
            "session_id",
            "project_id",
            "registered_at",
        ],
        "session registration",
    )?;
    if object.get("schema_version").and_then(Value::as_str) != Some(REGISTRATION_SCHEMA) {
        return Err("unsupported session registration schema".into());
    }
    let session_id = required_string(&object, "session_id", "session registration")?;
    let project_id = required_string(&object, "project_id", "session registration")?;
    let registered_at = required_string(&object, "registered_at", "session registration")?;
    safe_id(&session_id, "session registration session_id")?;
    safe_id(&project_id, "session registration project_id")?;
    bounded_text(&registered_at, "session registration timestamp", 128)?;
    if session_id != expected_session_id {
        return Err("session directory/session registration ID mismatch".into());
    }
    Ok(Registration {
        session_id,
        project_id,
        registered_at,
    })
}

fn session_directories(root: &Path) -> Result<Vec<PathBuf>, String> {
    let Some(registry) = registry_root(root)? else {
        return Ok(Vec::new());
    };
    let mut sessions = Vec::new();
    for entry in fs::read_dir(&registry)
        .map_err(|error| format!("cannot list agent-session registry: {error}"))?
    {
        let entry = entry.map_err(|error| format!("cannot read agent-session entry: {error}"))?;
        let path = entry.path();
        if path.is_symlink() {
            return Err("registered session directories must not be symlinks".into());
        }
        if !entry
            .file_type()
            .map_err(|error| format!("cannot inspect agent-session entry: {error}"))?
            .is_dir()
        {
            continue;
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "registered session directory name must be UTF-8".to_string())?;
        safe_id(&name, "registered session directory name")?;
        sessions.push(path);
        if sessions.len() > MAX_SESSIONS {
            return Err(format!(
                "agent-session registry exceeds {MAX_SESSIONS} sessions"
            ));
        }
    }
    sessions.sort();
    Ok(sessions)
}

fn list_sessions(root: &Path) -> Result<Vec<Registration>, String> {
    let mut registrations = Vec::new();
    for session in session_directories(root)? {
        let session_id = session
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "session directory name is invalid".to_string())?;
        let registration = session.join("registration.json");
        if !registration.exists() {
            return Err(format!(
                "registered session {session_id} is missing registration.json"
            ));
        }
        registrations.push(parse_registration(&registration, session_id)?);
    }
    registrations.sort_by(|a, b| a.session_id.cmp(&b.session_id));
    Ok(registrations)
}

fn parse_delivery_receipt(
    path: &Path,
    registration: &Registration,
) -> Result<DeliveryReceipt, String> {
    let object = read_json_object(path, "agent policy delivery receipt")?;
    strict_keys(
        &object,
        &[
            "schema_version",
            "session_id",
            "project_id",
            "idempotency_key",
            "policy_sha256",
            "outcome",
            "reason",
            "completed_at",
        ],
        "agent policy delivery receipt",
    )?;
    if object.get("schema_version").and_then(Value::as_str) != Some(DELIVERY_RECEIPT_SCHEMA) {
        return Err("unsupported agent policy delivery receipt schema".into());
    }
    let session_id = required_string(&object, "session_id", "delivery receipt")?;
    let project_id = required_string(&object, "project_id", "delivery receipt")?;
    let idempotency_key = required_string(&object, "idempotency_key", "delivery receipt")?;
    let policy_sha256 = required_string(&object, "policy_sha256", "delivery receipt")?;
    let outcome = required_string(&object, "outcome", "delivery receipt")?;
    let reason = optional_string(&object, "reason", "delivery receipt")?;
    let completed_at = required_string(&object, "completed_at", "delivery receipt")?;

    safe_id(&session_id, "delivery receipt session_id")?;
    safe_id(&project_id, "delivery receipt project_id")?;
    safe_id(&idempotency_key, "delivery receipt idempotency_key")?;
    if !is_lower_sha256(&policy_sha256) {
        return Err("delivery receipt policy_sha256 must be 64 lowercase hex characters".into());
    }
    if !matches!(outcome.as_str(), "acknowledged" | "refused" | "timeout") {
        return Err("delivery receipt outcome is invalid".into());
    }
    if let Some(reason) = reason.as_deref() {
        bounded_text(reason, "delivery receipt reason", 512)?;
    }
    if outcome == "refused" && reason.is_none() {
        return Err("refused delivery receipt requires a reason".into());
    }
    bounded_text(&completed_at, "delivery receipt timestamp", 128)?;
    if session_id != registration.session_id {
        return Err("delivery receipt session mismatch".into());
    }
    if project_id != registration.project_id {
        return Err("delivery receipt project/session mismatch".into());
    }
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "delivery receipt filename is invalid".to_string())?;
    if stem != idempotency_key {
        return Err("delivery receipt filename must match idempotency_key".into());
    }
    Ok(DeliveryReceipt {
        session_id,
        project_id,
        idempotency_key,
        policy_sha256,
        outcome,
        reason,
        completed_at,
    })
}

fn list_receipts(root: &Path, limit: usize) -> Result<Vec<DeliveryReceipt>, String> {
    if !(1..=MAX_RECEIPTS).contains(&limit) {
        return Err(format!(
            "receipt limit must be between 1 and {MAX_RECEIPTS}"
        ));
    }
    let mut receipts = Vec::new();
    for session in session_directories(root)? {
        let session_id = session
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "session directory name is invalid".to_string())?;
        let registration = parse_registration(&session.join("registration.json"), session_id)?;
        let deliveries = session.join("delivery-receipts");
        refuse_symlink(&deliveries, "delivery receipt directory")?;
        if !deliveries.exists() {
            continue;
        }
        if !deliveries.is_dir() {
            return Err("delivery receipt path must be a directory".into());
        }
        for entry in fs::read_dir(&deliveries)
            .map_err(|error| format!("cannot list delivery receipts: {error}"))?
        {
            let entry =
                entry.map_err(|error| format!("cannot read delivery receipt entry: {error}"))?;
            let path = entry.path();
            if path.is_symlink() {
                return Err("delivery receipt symlinks are refused".into());
            }
            if !entry
                .file_type()
                .map_err(|error| format!("cannot inspect delivery receipt: {error}"))?
                .is_file()
            {
                continue;
            }
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            receipts.push(parse_delivery_receipt(&path, &registration)?);
            if receipts.len() > MAX_RECEIPTS {
                return Err(format!(
                    "delivery receipt inventory exceeds {MAX_RECEIPTS}"
                ));
            }
        }
    }
    receipts.sort_by(|a, b| {
        b.completed_at
            .cmp(&a.completed_at)
            .then_with(|| a.session_id.cmp(&b.session_id))
            .then_with(|| a.idempotency_key.cmp(&b.idempotency_key))
    });
    receipts.truncate(limit);
    Ok(receipts)
}

fn render_sessions(items: &[Registration]) -> String {
    let mut output = String::from("REGISTERED AGENT SESSIONS\n");
    if items.is_empty() {
        output.push_str("none\n");
        return output;
    }
    for item in items {
        output.push_str(&format!(
            "{} project={} registered_at={}\n",
            item.session_id, item.project_id, item.registered_at
        ));
    }
    output
}

fn render_receipts(items: &[DeliveryReceipt]) -> String {
    let mut output = String::from("AGENT POLICY DELIVERY RECEIPTS\n");
    if items.is_empty() {
        output.push_str("none\n");
        return output;
    }
    for item in items {
        output.push_str(&format!(
            "{} session={} project={} key={} policy_sha256={} completed_at={}\n",
            item.outcome,
            item.session_id,
            item.project_id,
            item.idempotency_key,
            item.policy_sha256,
            item.completed_at
        ));
        if let Some(reason) = item.reason.as_deref() {
            output.push_str(&format!("  reason: {reason}\n"));
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "agentdock-policy-{name}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn seed_session(root: &Path, session_id: &str, project_id: &str) -> PathBuf {
        let session = root.join(".ai/agent-sessions").join(session_id);
        fs::create_dir_all(session.join("delivery-receipts")).unwrap();
        fs::write(
            session.join("registration.json"),
            serde_json::to_vec_pretty(&json!({
                "schema_version": REGISTRATION_SCHEMA,
                "session_id": session_id,
                "project_id": project_id,
                "registered_at": "2026-10-03T07:00:00+00:00"
            }))
            .unwrap(),
        )
        .unwrap();
        session
    }

    #[test]
    fn missing_registry_is_empty() {
        let root = temp_root("missing");
        assert!(list_sessions(&root).unwrap().is_empty());
        assert!(list_receipts(&root, 20).unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lists_registration_and_delivery_receipt_without_policy_payload() {
        let root = temp_root("happy");
        let session = seed_session(&root, "session-1", "applyai");
        fs::write(
            session.join("delivery-receipts/key-1.json"),
            serde_json::to_vec_pretty(&json!({
                "schema_version": DELIVERY_RECEIPT_SCHEMA,
                "session_id": "session-1",
                "project_id": "applyai",
                "idempotency_key": "key-1",
                "policy_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "outcome": "acknowledged",
                "reason": null,
                "completed_at": "2026-10-03T07:01:00+00:00"
            }))
            .unwrap(),
        )
        .unwrap();

        let sessions = list_sessions(&root).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "session-1");
        let receipts = list_receipts(&root, 20).unwrap();
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].outcome, "acknowledged");
        let rendered = render_receipts(&receipts);
        assert!(rendered.contains("policy_sha256=aaaaaaaa"));
        assert!(!rendered.contains("deployment_mode"));
        assert!(!rendered.contains("allow_docker"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_project_mismatch() {
        let root = temp_root("mismatch");
        let session = seed_session(&root, "session-1", "applyai");
        fs::write(
            session.join("delivery-receipts/key-1.json"),
            serde_json::to_vec_pretty(&json!({
                "schema_version": DELIVERY_RECEIPT_SCHEMA,
                "session_id": "session-1",
                "project_id": "other-project",
                "idempotency_key": "key-1",
                "policy_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "outcome": "timeout",
                "reason": "bounded acknowledgement wait expired",
                "completed_at": "2026-10-03T07:01:00+00:00"
            }))
            .unwrap(),
        )
        .unwrap();
        let error = list_receipts(&root, 20).unwrap_err();
        assert!(error.contains("project/session mismatch"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_filename_idempotency_mismatch() {
        let root = temp_root("filename");
        let session = seed_session(&root, "session-1", "applyai");
        fs::write(
            session.join("delivery-receipts/wrong-name.json"),
            serde_json::to_vec_pretty(&json!({
                "schema_version": DELIVERY_RECEIPT_SCHEMA,
                "session_id": "session-1",
                "project_id": "applyai",
                "idempotency_key": "key-1",
                "policy_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "outcome": "timeout",
                "reason": "bounded acknowledgement wait expired",
                "completed_at": "2026-10-03T07:01:00+00:00"
            }))
            .unwrap(),
        )
        .unwrap();
        let error = list_receipts(&root, 20).unwrap_err();
        assert!(error.contains("filename must match idempotency_key"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refused_receipt_requires_reason() {
        let root = temp_root("refused");
        let session = seed_session(&root, "session-1", "applyai");
        fs::write(
            session.join("delivery-receipts/key-1.json"),
            serde_json::to_vec_pretty(&json!({
                "schema_version": DELIVERY_RECEIPT_SCHEMA,
                "session_id": "session-1",
                "project_id": "applyai",
                "idempotency_key": "key-1",
                "policy_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "outcome": "refused",
                "reason": null,
                "completed_at": "2026-10-03T07:01:00+00:00"
            }))
            .unwrap(),
        )
        .unwrap();
        let error = list_receipts(&root, 20).unwrap_err();
        assert!(error.contains("requires a reason"));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinked_session_directory() {
        use std::os::unix::fs::symlink;

        let root = temp_root("symlink");
        let registry = root.join(".ai/agent-sessions");
        fs::create_dir_all(&registry).unwrap();
        let outside = temp_root("outside");
        symlink(&outside, registry.join("session-1")).unwrap();
        let error = list_sessions(&root).unwrap_err();
        assert!(error.contains("must not be symlinks"));
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
}
