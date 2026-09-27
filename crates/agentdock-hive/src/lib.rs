mod model;
mod adapter;
pub use model::*;
pub use adapter::*;

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HiveError {
    #[error("sqlite: {0}")] Sql(#[from] rusqlite::Error),
    #[error("json: {0}")] Json(#[from] serde_json::Error),
    #[error("filesystem: {0}")] Io(#[from] std::io::Error),
    #[error("invalid schema version or input: {0}")] Invalid(&'static str),
    #[error("session, workspace, floor or agent ownership mismatch")] Ownership,
    #[error("entity not found")] NotFound,
    #[error("idempotency key was reused for a different message")] DuplicateConflict,
    #[error("bounded message hop limit reached")] HopLimit,
    #[error("delivery lease does not belong to this session or has expired")] LeaseMismatch,
    #[error("already acknowledged; replay is forbidden")] AlreadyAcked,
    #[error("adapter failed: {0}")] Adapter(String),
}

pub type Result<T> = std::result::Result<T, HiveError>;

/// A separate, namespaced Phase-A schema inside the existing agentdock.db.
/// The daemon is the intended single owner. There is intentionally no public hive HTTP API.
pub struct Hive {
    conn: Connection,
}

fn version(v: u32) -> Result<()> {
    if v != SCHEMA_VERSION { Err(HiveError::Invalid("schema_version")) } else { Ok(()) }
}
fn ident(s: &str) -> Result<()> {
    if s.trim().is_empty() || s.len() > 128 { Err(HiveError::Invalid("identifier")) } else { Ok(()) }
}
fn workspace(path: &str) -> Result<String> {
    Ok(Path::new(path).canonicalize()?.to_string_lossy().into_owned())
}
fn new_id(tx: &Transaction<'_>) -> rusqlite::Result<String> {
    tx.query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0))
}
fn kind_text(k: &EventKind) -> std::result::Result<String, serde_json::Error> {
    serde_json::to_string(k)
}
fn event(tx: &Transaction<'_>, scope: &Scope, peer: Option<&str>, mid: Option<&str>, kind: EventKind, at: i64) -> Result<()> {
    tx.execute("INSERT INTO hive_events(floor_id, actor_agent, peer_agent, message_id, kind, at_ms) VALUES (?1,?2,?3,?4,?5,?6)",
        params![scope.floor_id, scope.agent_id, peer, mid, kind_text(&kind)?, at])?;
    Ok(())
}
fn require_scope(db: &Connection, s: &Scope) -> Result<()> {
    if workspace(&s.workspace)? != s.workspace { return Err(HiveError::Ownership); }
    let ok: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM hive_sessions s JOIN hive_agents a ON a.id=s.agent_id AND a.floor_id=s.floor_id
         JOIN hive_floors f ON f.id=s.floor_id WHERE s.id=?1 AND s.floor_id=?2 AND s.agent_id=?3
         AND s.owner_id=?4 AND s.workspace=?5 AND s.state='active' AND f.owner_id=s.owner_id AND f.workspace=s.workspace)",
        params![s.session_id,s.floor_id,s.agent_id,s.owner_id,s.workspace], |r| r.get(0))?;
    if !ok { return Err(HiveError::Ownership); }
    Ok(())
}

impl Hive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> { Self::from_connection(Connection::open(path)?) }
    pub fn in_memory() -> Result<Self> { Self::from_connection(Connection::open_in_memory()?) }
    fn from_connection(conn: Connection) -> Result<Self> {
        conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA busy_timeout=5000;
          CREATE TABLE IF NOT EXISTS hive_meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS hive_floors(id TEXT PRIMARY KEY, owner_id TEXT NOT NULL, workspace TEXT NOT NULL UNIQUE, max_hops INTEGER NOT NULL, payload TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS hive_agents(id TEXT PRIMARY KEY, floor_id TEXT NOT NULL REFERENCES hive_floors(id), payload TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS hive_sessions(id TEXT PRIMARY KEY, floor_id TEXT NOT NULL REFERENCES hive_floors(id), agent_id TEXT NOT NULL REFERENCES hive_agents(id), owner_id TEXT NOT NULL, workspace TEXT NOT NULL, state TEXT NOT NULL, payload TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS hive_tasks(id TEXT PRIMARY KEY, floor_id TEXT NOT NULL, session_id TEXT NOT NULL REFERENCES hive_sessions(id), agent_id TEXT NOT NULL REFERENCES hive_agents(id), payload TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS hive_memory_refs(id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES hive_sessions(id), payload TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS hive_mailboxes(
            id TEXT PRIMARY KEY, floor_id TEXT NOT NULL REFERENCES hive_floors(id),
            sender_agent TEXT NOT NULL REFERENCES hive_agents(id), sender_session TEXT NOT NULL REFERENCES hive_sessions(id),
            recipient_agent TEXT NOT NULL REFERENCES hive_agents(id), idem_key TEXT NOT NULL,
            state TEXT NOT NULL, payload TEXT NOT NULL, attempt INTEGER NOT NULL DEFAULT 0,
            next_attempt_ms INTEGER NOT NULL, lease_until_ms INTEGER, lease_token TEXT,
            UNIQUE(floor_id, recipient_agent, sender_agent, idem_key));
          CREATE INDEX IF NOT EXISTS idx_hive_mailbox_claim ON hive_mailboxes(floor_id,recipient_agent,state,next_attempt_ms);
          CREATE TABLE IF NOT EXISTS hive_events(seq INTEGER PRIMARY KEY AUTOINCREMENT, floor_id TEXT NOT NULL, actor_agent TEXT NOT NULL, peer_agent TEXT, message_id TEXT, kind TEXT NOT NULL, at_ms INTEGER NOT NULL);
          CREATE INDEX IF NOT EXISTS idx_hive_events_scope ON hive_events(floor_id,seq);")?;
        let previous: Option<String> = conn.query_row("SELECT value FROM hive_meta WHERE key='schema_version'", [], |r| r.get(0)).optional()?;
        if let Some(v) = previous {
            if v != SCHEMA_VERSION.to_string() { return Err(HiveError::Invalid("stored schema_version")); }
        } else {
            conn.execute("INSERT INTO hive_meta(key,value) VALUES('schema_version',?1)", [SCHEMA_VERSION.to_string()])?;
        }
        Ok(Self { conn })
    }

    pub fn add_floor(&self, f: &Floor) -> Result<()> {
        version(f.schema_version)?; ident(&f.id)?; ident(&f.owner_id)?;
        if f.max_hops == 0 || f.max_hops > 32 || workspace(&f.workspace)? != f.workspace {
            return Err(HiveError::Invalid("floor workspace/max_hops"));
        }
        self.conn.execute("INSERT INTO hive_floors(id,owner_id,workspace,max_hops,payload) VALUES(?1,?2,?3,?4,?5)",
            params![f.id,f.owner_id,f.workspace,f.max_hops,serde_json::to_string(f)?])?;
        Ok(())
    }
    pub fn add_agent(&self, a: &Agent) -> Result<()> {
        version(a.schema_version)?; ident(&a.id)?; ident(&a.role)?;
        let exists: bool = self.conn.query_row("SELECT EXISTS(SELECT 1 FROM hive_floors WHERE id=?1)", [&a.floor_id], |r|r.get(0))?;
        if !exists { return Err(HiveError::Ownership); }
        self.conn.execute("INSERT INTO hive_agents(id,floor_id,payload) VALUES(?1,?2,?3)",
            params![a.id,a.floor_id,serde_json::to_string(a)?])?;
        Ok(())
    }
    pub fn add_session(&mut self, s: &Session, at: i64) -> Result<()> {
        version(s.schema_version)?; ident(&s.id)?; ident(&s.owner_id)?;
        if s.state != SessionState::Active || workspace(&s.workspace)? != s.workspace {
            return Err(HiveError::Ownership);
        }
        let ok: bool = self.conn.query_row("SELECT EXISTS(SELECT 1 FROM hive_floors f JOIN hive_agents a ON a.floor_id=f.id WHERE f.id=?1 AND f.owner_id=?2 AND f.workspace=?3 AND a.id=?4)",
            params![s.floor_id,s.owner_id,s.workspace,s.agent_id], |r|r.get(0))?;
        if !ok { return Err(HiveError::Ownership); }
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("INSERT INTO hive_sessions(id,floor_id,agent_id,owner_id,workspace,state,payload) VALUES(?1,?2,?3,?4,?5,'active',?6)",
            params![s.id,s.floor_id,s.agent_id,s.owner_id,s.workspace,serde_json::to_string(s)?])?;
        event(&tx,&Scope::from(s),None,None,EventKind::SessionStarted,at)?;
        tx.commit()?;
        Ok(())
    }
    pub fn interrupt_session(&mut self, s: &Scope, at: i64) -> Result<()> {
        require_scope(&self.conn,s)?;
        let tx=self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("UPDATE hive_sessions SET state='interrupted' WHERE id=?1", [&s.session_id])?;
        event(&tx,s,None,None,EventKind::SessionInterrupted,at)?;
        tx.commit()?; Ok(())
    }
    /// Logical rebind after process restart; this does NOT resume a provider's own CLI session.
    pub fn resume_session(&mut self, s: &Scope, at: i64) -> Result<()> {
        if workspace(&s.workspace)? != s.workspace { return Err(HiveError::Ownership); }
        let tx=self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed=tx.execute(
            "UPDATE hive_sessions SET state='active' WHERE id=?1 AND floor_id=?2 AND agent_id=?3 AND owner_id=?4 AND workspace=?5 AND state='interrupted'
             AND EXISTS(SELECT 1 FROM hive_floors f JOIN hive_agents a ON a.floor_id=f.id WHERE f.id=?2 AND f.owner_id=?4 AND f.workspace=?5 AND a.id=?3)",
            params![s.session_id,s.floor_id,s.agent_id,s.owner_id,s.workspace])?;
        if changed!=1 { return Err(HiveError::Ownership); }
        event(&tx,s,None,None,EventKind::SessionStarted,at)?; tx.commit()?; Ok(())
    }

    pub fn send(&mut self, scope: &Scope, req: SendRequest, at: i64) -> Result<Message> {
        let tx=self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        match send_tx(&tx,scope,&req,at) {
            Ok(m) => { tx.commit()?; Ok(m) }
            Err(HiveError::HopLimit) => {
                event(&tx,scope,Some(&req.to_agent_id),req.in_reply_to.as_deref(),EventKind::HopLimitReached,at)?;
                tx.commit()?; Err(HiveError::HopLimit)
            }
            Err(e) => Err(e),
        }
    }

    /// SQLite IMMEDIATE transaction serializes claims: one recipient, one unexpired token.
    pub fn claim(&mut self, scope: &Scope, at: i64, lease_ms: i64) -> Result<Option<Delivery>> {
        if lease_ms < 1 || lease_ms > 300_000 { return Err(HiveError::Invalid("lease_ms")); }
        self.recover(at)?;
        let tx=self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_scope(&tx,scope)?;
        let candidate: Option<(String,String,u32)> = tx.query_row(
            "SELECT id,payload,attempt FROM hive_mailboxes WHERE floor_id=?1 AND recipient_agent=?2 AND state IN ('pending','retry') AND next_attempt_ms<=?3 ORDER BY next_attempt_ms,id LIMIT 1",
            params![scope.floor_id,scope.agent_id,at], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let Some((id,payload,attempt))=candidate else { tx.commit()?; return Ok(None); };
        let token=new_id(&tx)?;
        tx.execute("UPDATE hive_mailboxes SET state='leased',attempt=attempt+1,lease_token=?2,lease_until_ms=?3 WHERE id=?1",
            params![id,token,at.saturating_add(lease_ms)])?;
        event(&tx,scope,None,Some(&id),EventKind::MessageClaimed,at)?;
        let message:Message=serde_json::from_str(&payload)?;
        version(message.schema_version)?;
        tx.commit()?;
        Ok(Some(Delivery { message, token, attempt:attempt+1 }))
    }

    /// Reply and ack commit together; on crash the transaction is wholly present or absent.
    pub fn ack(&mut self, scope: &Scope, d: &Delivery, reply: Option<(MessageAct,String)>, at: i64) -> Result<Option<String>> {
        let tx=self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let source=validate_lease(&tx,scope,d,at)?;
        let mut reply_id=None;
        if let Some((act,body))=reply {
            let req=SendRequest {
                to_agent_id:source.from_agent_id.clone(),
                idempotency_key:format!("reply:{}",source.id),
                conversation_id:source.conversation_id.clone(),
                in_reply_to:Some(source.id.clone()),act,body,
            };
            match send_tx(&tx,scope,&req,at) {
                Ok(m) => reply_id=Some(m.id),
                Err(HiveError::HopLimit) => event(&tx,scope,Some(&source.from_agent_id),Some(&source.id),EventKind::HopLimitReached,at)?,
                Err(e) => return Err(e),
            }
        }
        tx.execute("UPDATE hive_mailboxes SET state='acked',lease_token=NULL,lease_until_ms=NULL WHERE id=?1 AND lease_token=?2",
            params![source.id,d.token])?;
        event(&tx,scope,Some(&source.from_agent_id),Some(&source.id),EventKind::MessageAcked,at)?;
        tx.commit()?; Ok(reply_id)
    }

    pub fn retry(&mut self, scope: &Scope, d: &Delivery, at: i64, delay_ms: i64) -> Result<DeliveryState> {
        if !(0..=300_000).contains(&delay_ms) { return Err(HiveError::Invalid("delay_ms")); }
        let tx=self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let source=validate_lease(&tx,scope,d,at)?;
        let dead=d.attempt >= MAX_ATTEMPTS;
        tx.execute("UPDATE hive_mailboxes SET state=?2,next_attempt_ms=?3,lease_token=NULL,lease_until_ms=NULL WHERE id=?1 AND lease_token=?4",
            params![source.id,if dead {"dead"} else {"retry"},at.saturating_add(delay_ms),d.token])?;
        event(&tx,scope,None,Some(&source.id),if dead {EventKind::MessageDead} else {EventKind::MessageRetried},at)?;
        tx.commit()?; Ok(if dead {DeliveryState::Dead} else {DeliveryState::Retry})
    }

    /// Reclaims only expired leases. A live lease is never stolen on daemon startup.
    pub fn recover(&mut self, at: i64) -> Result<usize> {
        let tx=self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let expired:Vec<(String,String,String,String,u32)>={
            let mut q=tx.prepare("SELECT id,floor_id,sender_agent,recipient_agent,attempt FROM hive_mailboxes WHERE state='leased' AND lease_until_ms<=?1")?;
            let rows=q.query_map([at],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
            let values=rows.collect::<rusqlite::Result<Vec<_>>>()?;
            values
        };
        for (id,floor,sender,recipient,attempt) in &expired {
            let dead=*attempt>=MAX_ATTEMPTS;
            tx.execute("UPDATE hive_mailboxes SET state=?2,lease_token=NULL,lease_until_ms=NULL,next_attempt_ms=?3 WHERE id=?1",
                params![id,if dead {"dead"} else {"retry"},at])?;
            tx.execute("INSERT INTO hive_events(floor_id,actor_agent,peer_agent,message_id,kind,at_ms) VALUES(?1,?2,?3,?4,?5,?6)",
                params![floor,recipient,sender,id,kind_text(&if dead {EventKind::MessageDead} else {EventKind::MessageRetried})?,at])?;
        }
        tx.commit()?; Ok(expired.len())
    }

    /// Explicit replay of unacked/dead mail; already-acked messages never replay.
    pub fn replay(&mut self, scope: &Scope, id: &str, at: i64) -> Result<()> {
        let tx=self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_scope(&tx,scope)?;
        let state:Option<String>=tx.query_row("SELECT state FROM hive_mailboxes WHERE id=?1 AND floor_id=?2 AND recipient_agent=?3",
            params![id,scope.floor_id,scope.agent_id],|r|r.get(0)).optional()?;
        match state.as_deref() {
            Some("acked") => return Err(HiveError::AlreadyAcked),
            Some("leased") => return Err(HiveError::LeaseMismatch),
            Some(_) => {}
            None => return Err(HiveError::Ownership),
        }
        tx.execute("UPDATE hive_mailboxes SET state='pending',attempt=0,next_attempt_ms=?2,lease_token=NULL,lease_until_ms=NULL WHERE id=?1",
            params![id,at])?;
        event(&tx,scope,None,Some(id),EventKind::MessageReplayed,at)?; tx.commit()?; Ok(())
    }

    pub fn delivery_state(&self, scope: &Scope, id: &str) -> Result<DeliveryState> {
        require_scope(&self.conn,scope)?;
        let state:Option<String>=self.conn.query_row("SELECT state FROM hive_mailboxes WHERE id=?1 AND floor_id=?2 AND recipient_agent=?3",
            params![id,scope.floor_id,scope.agent_id],|r|r.get(0)).optional()?;
        match state.as_deref() {
            Some("pending")=>Ok(DeliveryState::Pending),Some("leased")=>Ok(DeliveryState::Leased),
            Some("retry")=>Ok(DeliveryState::Retry),Some("acked")=>Ok(DeliveryState::Acked),
            Some("dead")=>Ok(DeliveryState::Dead),_=>Err(HiveError::Ownership),
        }
    }

    pub fn create_task(&mut self, scope: &Scope, title: &str, at: i64) -> Result<Task> {
        if title.trim().is_empty() || title.len()>512 { return Err(HiveError::Invalid("task title")); }
        let tx=self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_scope(&tx,scope)?;
        let t=Task {schema_version:SCHEMA_VERSION,id:new_id(&tx)?,floor_id:scope.floor_id.clone(),
            session_id:scope.session_id.clone(),agent_id:scope.agent_id.clone(),title:title.into(),state:TaskState::Todo};
        tx.execute("INSERT INTO hive_tasks(id,floor_id,session_id,agent_id,payload) VALUES(?1,?2,?3,?4,?5)",
            params![t.id,t.floor_id,t.session_id,t.agent_id,serde_json::to_string(&t)?])?;
        event(&tx,scope,None,None,EventKind::TaskChanged,at)?;tx.commit()?;Ok(t)
    }
    pub fn set_task_state(&mut self, scope: &Scope, id: &str, state: TaskState, at: i64) -> Result<Task> {
        let tx=self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_scope(&tx,scope)?;
        let json:Option<String>=tx.query_row(
            "SELECT payload FROM hive_tasks WHERE id=?1 AND floor_id=?2 AND session_id=?3 AND agent_id=?4",
            params![id,scope.floor_id,scope.session_id,scope.agent_id],|r|r.get(0)).optional()?;
        let mut t:Task=serde_json::from_str(&json.ok_or(HiveError::Ownership)?)?;
        version(t.schema_version)?;
        t.state=state;
        tx.execute("UPDATE hive_tasks SET payload=?2 WHERE id=?1",params![id,serde_json::to_string(&t)?])?;
        event(&tx,scope,None,None,EventKind::TaskChanged,at)?;tx.commit()?;Ok(t)
    }
    pub fn get_task(&self, scope: &Scope, id: &str) -> Result<Task> {
        require_scope(&self.conn,scope)?;
        let json:Option<String>=self.conn.query_row("SELECT payload FROM hive_tasks WHERE id=?1 AND floor_id=?2 AND session_id=?3 AND agent_id=?4",
            params![id,scope.floor_id,scope.session_id,scope.agent_id],|r|r.get(0)).optional()?;
        let t:Task=serde_json::from_str(&json.ok_or(HiveError::Ownership)?)?;
        version(t.schema_version)?;Ok(t)
    }
    pub fn link_memory(&mut self, scope: &Scope, relative_path: &Path, at: i64) -> Result<MemoryRef> {
        if relative_path.is_absolute() { return Err(HiveError::Ownership); }
        let root=Path::new(&scope.workspace).canonicalize()?;
        let path=root.join(relative_path).canonicalize()?;
        if !path.starts_with(&root) || !path.is_file() { return Err(HiveError::Ownership); }
        let tx=self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_scope(&tx,scope)?;
        let m=MemoryRef { schema_version:SCHEMA_VERSION,id:new_id(&tx)?,floor_id:scope.floor_id.clone(),
            session_id:scope.session_id.clone(),agent_id:scope.agent_id.clone(),path:path.to_string_lossy().into_owned() };
        tx.execute("INSERT INTO hive_memory_refs(id,session_id,payload) VALUES(?1,?2,?3)",params![m.id,m.session_id,serde_json::to_string(&m)?])?;
        event(&tx,scope,None,None,EventKind::MemoryLinked,at)?;tx.commit()?;Ok(m)
    }

    /// Append-only, agent-scoped cursor replay. Never includes message bodies.
    pub fn events(&self, scope: &Scope, after_seq: i64, limit: usize) -> Result<Vec<Event>> {
        require_scope(&self.conn,scope)?;
        let mut stmt=self.conn.prepare(
            "SELECT seq,floor_id,actor_agent,peer_agent,message_id,kind,at_ms FROM hive_events
             WHERE floor_id=?1 AND seq>?2 AND (actor_agent=?3 OR peer_agent=?3) ORDER BY seq LIMIT ?4")?;
        let raw=stmt.query_map(params![scope.floor_id,after_seq,scope.agent_id,limit.clamp(1,1000) as i64],|r|{
            Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,
                r.get::<_,Option<String>>(3)?,r.get::<_,Option<String>>(4)?,
                r.get::<_,String>(5)?,r.get::<_,i64>(6)?))
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        raw.into_iter().map(|(seq,floor_id,actor_agent_id,peer_agent_id,message_id,k,created_at_ms)|Ok(Event{
            schema_version:SCHEMA_VERSION,seq,floor_id,actor_agent_id,peer_agent_id,message_id,
            kind:serde_json::from_str(&k)?,created_at_ms
        })).collect()
    }

    pub fn agent_provider(&self, scope: &Scope) -> Result<Provider> {
        require_scope(&self.conn,scope)?;
        let raw:String=self.conn.query_row(
            "SELECT payload FROM hive_agents WHERE id=?1 AND floor_id=?2",
            params![scope.agent_id,scope.floor_id],|r|r.get(0))?;
        let agent:Agent=serde_json::from_str(&raw)?;
        version(agent.schema_version)?;
        Ok(agent.provider)
    }

    pub fn record_turn(&mut self, scope: &Scope, id: &str, kind: EventKind, at: i64) -> Result<()> {
        if !matches!(kind,EventKind::TurnStarted|EventKind::TurnSucceeded|EventKind::TurnFailed) {
            return Err(HiveError::Invalid("turn event kind"));
        }
        let tx=self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_scope(&tx,scope)?;
        event(&tx,scope,None,Some(id),kind,at)?;tx.commit()?;Ok(())
    }
}

fn validate_lease(tx: &Transaction<'_>, scope: &Scope, d: &Delivery, at: i64) -> Result<Message> {
    require_scope(tx,scope)?;
    let entry:Option<(String,String,i64,u32)>=tx.query_row(
        "SELECT payload,lease_token,lease_until_ms,attempt FROM hive_mailboxes WHERE id=?1 AND floor_id=?2 AND recipient_agent=?3 AND state='leased'",
        params![d.message.id,scope.floor_id,scope.agent_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let Some((payload,token,until,attempt))=entry else {return Err(HiveError::LeaseMismatch);};
    if token!=d.token || until<=at || attempt!=d.attempt {return Err(HiveError::LeaseMismatch);}
    let m:Message=serde_json::from_str(&payload)?;
    version(m.schema_version)?;
    if m!=d.message {return Err(HiveError::LeaseMismatch);}
    Ok(m)
}

fn send_tx(tx: &Transaction<'_>, s: &Scope, req: &SendRequest, at: i64) -> Result<Message> {
    require_scope(tx,s)?;
    ident(&req.to_agent_id)?; ident(&req.idempotency_key)?; ident(&req.conversation_id)?;
    if req.body.len()>8192 {return Err(HiveError::Invalid("message body exceeds 8 KiB"));}
    let dest:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM hive_agents WHERE id=?1 AND floor_id=?2)",
        params![req.to_agent_id,s.floor_id],|r|r.get(0))?;
    if !dest {return Err(HiveError::Ownership);}
    let hops=if let Some(parent)=&req.in_reply_to {
        let raw:Option<String>=tx.query_row("SELECT payload FROM hive_mailboxes WHERE id=?1 AND floor_id=?2 AND recipient_agent=?3",
            params![parent,s.floor_id,s.agent_id],|r|r.get(0)).optional()?;
        let p:Message=serde_json::from_str(&raw.ok_or(HiveError::Ownership)?)?;
        if p.conversation_id!=req.conversation_id {return Err(HiveError::Ownership);}
        p.hops.checked_add(1).ok_or(HiveError::HopLimit)?
    } else {0};
    let cap:u32=tx.query_row("SELECT max_hops FROM hive_floors WHERE id=?1",[&s.floor_id],|r|r.get(0))?;
    if hops>cap {return Err(HiveError::HopLimit);}
    let existing:Option<String>=tx.query_row(
        "SELECT payload FROM hive_mailboxes WHERE floor_id=?1 AND recipient_agent=?2 AND sender_agent=?3 AND idem_key=?4",
        params![s.floor_id,req.to_agent_id,s.agent_id,req.idempotency_key],|r|r.get(0)).optional()?;
    if let Some(raw)=existing {
        let m:Message=serde_json::from_str(&raw)?;
        if m.from_session_id==s.session_id && m.conversation_id==req.conversation_id &&
            m.in_reply_to==req.in_reply_to && m.act==req.act && m.body==req.body && m.hops==hops {
            return Ok(m);
        }
        return Err(HiveError::DuplicateConflict);
    }
    let m=Message {schema_version:SCHEMA_VERSION,id:new_id(tx)?,floor_id:s.floor_id.clone(),
        from_agent_id:s.agent_id.clone(),from_session_id:s.session_id.clone(),to_agent_id:req.to_agent_id.clone(),
        idempotency_key:req.idempotency_key.clone(),conversation_id:req.conversation_id.clone(),
        in_reply_to:req.in_reply_to.clone(),act:req.act,body:req.body.clone(),hops,created_at_ms:at};
    tx.execute("INSERT INTO hive_mailboxes(id,floor_id,sender_agent,sender_session,recipient_agent,idem_key,state,payload,next_attempt_ms)
        VALUES(?1,?2,?3,?4,?5,?6,'pending',?7,?8)",
        params![m.id,m.floor_id,m.from_agent_id,m.from_session_id,m.to_agent_id,m.idempotency_key,serde_json::to_string(&m)?,at])?;
    event(tx,s,Some(&m.to_agent_id),Some(&m.id),EventKind::MessageEnqueued,at)?;
    Ok(m)
}
