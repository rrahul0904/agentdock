use crate::{Delivery, EventKind, Hive, HiveError, Message, MessageAct, Provider, Result, Scope};

/// Adapter protocol intentionally contains no UI, approval policy, or PTY impersonation.
/// A successful CLI process exit is not proof of exactly-once external side effects.
pub trait Adapter {
    fn provider(&self) -> Provider;
    fn turn(&mut self, scope: &Scope, message: &Message) -> Result<Option<(MessageAct, String)>>;
}

/// Deterministic adapter for transaction/recovery acceptance tests. No external effects.
#[derive(Default)]
pub struct FakeAdapter {
    pub seen: Vec<String>,
}
impl Adapter for FakeAdapter {
    fn provider(&self) -> Provider { Provider::Fake }
    fn turn(&mut self, _scope: &Scope, m: &Message) -> Result<Option<(MessageAct, String)>> {
        self.seen.push(m.id.clone());
        Ok(m.act.requires_reply().then(|| (MessageAct::Inform, format!("received:{}",m.id))))
    }
}

/// Opt-in, noninteractive Codex CLI subprocess. No shell; cwd locked to the floor's
/// canonical workspace; read-only sandbox; stdin prompt; no unsafe auto-approval flags.
/// Deliberately does not parse/forward model output or claim provider session resume.
pub struct CodexCliAdapter {
    pub executable: std::path::PathBuf,
}
impl Default for CodexCliAdapter {
    fn default() -> Self { Self { executable: "codex".into() } }
}
impl Adapter for CodexCliAdapter {
    fn provider(&self) -> Provider { Provider::Codex }
    fn turn(&mut self, scope: &Scope, m: &Message) -> Result<Option<(MessageAct, String)>> {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let mut child=Command::new(&self.executable)
            .args(["exec","--sandbox","read-only","--ask-for-approval","never","-C"])
            .arg(&scope.workspace)
            .arg("-")
            .current_dir(&scope.workspace)
            .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null())
            .spawn().map_err(|e|HiveError::Adapter(e.to_string()))?;
        let written=child.stdin.take().ok_or(HiveError::Adapter("missing stdin".into()))?
            .write_all(m.body.as_bytes());
        if let Err(e)=written {
            let _=child.kill(); let _=child.wait();
            return Err(HiveError::Adapter(e.to_string()));
        }
        let status=child.wait().map_err(|e|HiveError::Adapter(e.to_string()))?;
        if !status.success() {return Err(HiveError::Adapter(format!("codex exec exited with {status}")));}
        Ok(None)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeOutcome {
    Idle,
    Completed { message_id: String, reply_id: Option<String> },
}

/// Single-turn typed bridge. The DB claim and final ack/reply are atomic separately.
/// If a process crashes while the adapter runs, its lease expires and the SAME message
/// ID is retried. External CLI work remains at-least-once, never falsely exactly-once.
pub fn run_once(
    hive: &mut Hive, scope: &Scope, adapter: &mut impl Adapter,
    at_ms: i64, lease_ms: i64, retry_delay_ms: i64,
) -> Result<BridgeOutcome> {
    let expected=hive.agent_provider(scope)?;
    if expected!=adapter.provider() {return Err(HiveError::Ownership);}
    let Some(delivery)=hive.claim(scope,at_ms,lease_ms)? else {return Ok(BridgeOutcome::Idle);};
    run_delivery(hive,scope,adapter,delivery,at_ms,retry_delay_ms)
}
fn run_delivery(
    hive: &mut Hive, scope: &Scope, adapter: &mut impl Adapter,
    delivery: Delivery, at_ms: i64, retry_delay_ms: i64,
) -> Result<BridgeOutcome> {
    let id=delivery.message.id.clone();
    hive.record_turn(scope,&id,EventKind::TurnStarted,at_ms)?;
    let reply=match adapter.turn(scope,&delivery.message) {
        Ok(reply)=>reply,
        Err(e)=>{
            hive.record_turn(scope,&id,EventKind::TurnFailed,at_ms)?;
            hive.retry(scope,&delivery,at_ms,retry_delay_ms)?;
            return Err(e);
        }
    };
    let reply_id=hive.ack(scope,&delivery,reply,at_ms)?.map(|id|id);
    hive.record_turn(scope,&id,EventKind::TurnSucceeded,at_ms)?;
    Ok(BridgeOutcome::Completed {message_id:id,reply_id})
}
