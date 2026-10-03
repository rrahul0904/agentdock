use agentdock_hive::*;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture {
    dir: PathBuf, db: PathBuf, root: String, hive: Hive, scopes: Vec<Scope>,
}
impl Drop for Fixture {
    fn drop(&mut self) { let _=std::fs::remove_dir_all(&self.dir); }
}
fn unique_dir() -> PathBuf {
    let n=SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("agentdock-hive-{}-{n}",std::process::id()))
}
fn scope(floor:&str, agent:&str, owner:&str, root:&str) -> Scope {
    Scope {floor_id:floor.into(),agent_id:agent.into(),session_id:format!("{agent}-session"),
        owner_id:owner.into(),workspace:root.into()}
}
fn make_floor(h:&mut Hive, id:&str, owner:&str, root:&str, names:&[&str], cap:u32, at:i64) -> Vec<Scope> {
    h.add_floor(&Floor {schema_version:SCHEMA_VERSION,id:id.into(),owner_id:owner.into(),workspace:root.into(),max_hops:cap}).unwrap();
    names.iter().map(|name|{
        h.add_agent(&Agent {schema_version:SCHEMA_VERSION,id:(*name).into(),floor_id:id.into(),role:(*name).into(),provider:Provider::Fake}).unwrap();
        let sc=scope(id,name,owner,root);
        h.add_session(&Session {schema_version:SCHEMA_VERSION,id:sc.session_id.clone(),floor_id:id.into(),
            agent_id:(*name).into(),owner_id:owner.into(),workspace:root.into(),state:SessionState::Active},at).unwrap();
        sc
    }).collect()
}
fn fixture(cap:u32) -> Fixture {
    let dir=unique_dir();std::fs::create_dir_all(&dir).unwrap();
    let root=dir.canonicalize().unwrap().to_string_lossy().to_string();
    let db=dir.join("agentdock.db");let mut hive=Hive::open(&db).unwrap();
    let scopes=make_floor(&mut hive,"floor-1","owner-1",&root,&["a","b","c"],cap,0);
    Fixture{dir,db,root,hive,scopes}
}
fn send(to:&str,key:&str,act:MessageAct,parent:Option<&Message>,body:&str) -> SendRequest {
    SendRequest {to_agent_id:to.into(),idempotency_key:key.into(),
        conversation_id:parent.map(|m|m.conversation_id.clone()).unwrap_or_else(||"conversation-1".into()),
        in_reply_to:parent.map(|m|m.id.clone()),act,body:body.into()}
}

#[test]
fn three_agent_floor_routes_replies_and_replays_task_events() {
    let mut f=fixture(4);let a=f.scopes[0].clone();let b=f.scopes[1].clone();let c=f.scopes[2].clone();
    let first=f.hive.send(&a,send("b","k1",MessageAct::Request,None,"research"),1).unwrap();
    let mut fake=FakeAdapter::default();
    let result=run_once(&mut f.hive,&b,&mut fake,2,100,1).unwrap();
    let reply_id=match result {BridgeOutcome::Completed{message_id,reply_id}=>{
        assert_eq!(message_id,first.id);reply_id.unwrap()},_=>panic!("expected delivery")};
    assert_eq!(fake.seen,vec![first.id.clone()]);
    let back=f.hive.claim(&a,3,100).unwrap().unwrap();
    assert_eq!(back.message.id,reply_id);
    assert_eq!(back.message.hops,1);
    assert_eq!(back.message.act,MessageAct::Inform);
    f.hive.ack(&a,&back,None,3).unwrap();
    let forwarded=f.hive.send(&b,send("c","k2",MessageAct::Inform,None,"synthesis"),4).unwrap();
    let mut task=f.hive.create_task(&c,"verify synthesis",4).unwrap();
    task=f.hive.set_task_state(&c,&task.id,TaskState::Doing,5).unwrap();
    assert_eq!(task.state,TaskState::Doing);
    let delivery=f.hive.claim(&c,5,100).unwrap().unwrap();
    assert_eq!(delivery.message.id,forwarded.id);
    f.hive.ack(&c,&delivery,None,6).unwrap();
    task=f.hive.set_task_state(&c,&task.id,TaskState::Done,7).unwrap();
    assert_eq!(f.hive.get_task(&c,&task.id).unwrap().state,TaskState::Done);
    let first_page=f.hive.events(&c,0,2).unwrap();assert_eq!(first_page.len(),2);
    let next=f.hive.events(&c,first_page.last().unwrap().seq,100).unwrap();
    assert!(!next.is_empty());assert!(next.iter().all(|ev|ev.floor_id=="floor-1"));
    let json=serde_json::to_string(&task).unwrap();
    assert_eq!(serde_json::from_str::<Task>(&json).unwrap(),task);
}

#[test]
fn restart_recovers_expired_lease_and_deduplicates_delivery() {
    let mut f=fixture(3);let a=f.scopes[0].clone();let b=f.scopes[1].clone();
    let m=f.hive.send(&a,send("b","stable-key",MessageAct::Inform,None,"work"),1).unwrap();
    let same=f.hive.send(&a,send("b","stable-key",MessageAct::Inform,None,"work"),2).unwrap();
    assert_eq!(m.id,same.id);
    assert!(matches!(f.hive.send(&a,send("b","stable-key",MessageAct::Inform,None,"different"),3),Err(HiveError::DuplicateConflict)));
    let d=f.hive.claim(&b,4,5).unwrap().unwrap();assert_eq!(d.attempt,1);
    drop(d);
    let mut reopened=Hive::open(&f.db).unwrap();
    assert_eq!(reopened.recover(8).unwrap(),0); // unexpired lease cannot be stolen
    assert!(reopened.claim(&b,8,5).unwrap().is_none());
    assert_eq!(reopened.recover(9).unwrap(),1);
    let recovered=reopened.claim(&b,9,5).unwrap().unwrap();
    assert_eq!(recovered.message.id,m.id);assert_eq!(recovered.attempt,2);
    reopened.ack(&b,&recovered,None,10).unwrap();
    assert_eq!(reopened.delivery_state(&b,&m.id).unwrap(),DeliveryState::Acked);
    assert!(reopened.claim(&b,15,5).unwrap().is_none());
    assert!(matches!(reopened.replay(&b,&m.id,16),Err(HiveError::AlreadyAcked)));
}

#[test]
fn bounded_hops_and_reply_are_enforced_by_router() {
    let mut f=fixture(1);let a=f.scopes[0].clone();let b=f.scopes[1].clone();
    let m=f.hive.send(&a,send("b","one",MessageAct::Request,None,"question"),1).unwrap();
    let claim=f.hive.claim(&b,2,100).unwrap().unwrap();
    let reply_id=f.hive.ack(&b,&claim,Some((MessageAct::Query,"follow-up".into())),3).unwrap().unwrap();
    let back=f.hive.claim(&a,4,100).unwrap().unwrap();assert_eq!(back.message.id,reply_id);
    assert_eq!(back.message.hops,1);
    assert!(matches!(f.hive.send(&a,send("b","three",MessageAct::Request,Some(&back.message),"ping"),5),Err(HiveError::HopLimit)));
    assert_eq!(f.hive.ack(&a,&back,Some((MessageAct::Inform,"pong".into())),5).unwrap(),None);
    assert!(f.hive.events(&a,0,100).unwrap().iter().any(|e|e.kind==EventKind::HopLimitReached));
}

#[test]
fn strict_floor_session_workspace_and_memory_isolation() {
    let mut f=fixture(2);let a=f.scopes[0].clone();let b=f.scopes[1].clone();
    let other=f.dir.join("different");std::fs::create_dir(&other).unwrap();
    let other=other.canonicalize().unwrap().to_string_lossy().into_owned();
    let external=make_floor(&mut f.hive,"floor-2","owner-2",&other,&["x"],2,0).pop().unwrap();
    assert!(matches!(f.hive.send(&a,send("x","cross",MessageAct::Inform,None,"no"),1),Err(HiveError::Ownership)));
    let m=f.hive.send(&a,send("b","one",MessageAct::Inform,None,"private"),1).unwrap();
    assert!(matches!(f.hive.delivery_state(&external,&m.id),Err(HiveError::Ownership)));
    let mut forged=b.clone();forged.owner_id="owner-2".into();
    assert!(matches!(f.hive.claim(&forged,1,10),Err(HiveError::Ownership)));
    forged=b.clone();forged.workspace=other.clone();
    assert!(matches!(f.hive.events(&forged,0,100),Err(HiveError::Ownership)));
    assert!(matches!(f.hive.replay(&external,&m.id,1),Err(HiveError::Ownership)));
    std::fs::write(f.dir.join("inside.txt"),"memory").unwrap();
    let linked=f.hive.link_memory(&a,Path::new("inside.txt"),2).unwrap();
    assert!(linked.path.starts_with(&f.root));
    assert!(f.hive.link_memory(&a,Path::new("../outside"),3).is_err());
    assert!(f.hive.link_memory(&a,Path::new(&other),3).is_err());
}

#[test]
fn bounded_retries_dead_letter_and_explicit_unacked_replay() {
    let mut f=fixture(2);let a=f.scopes[0].clone();let b=f.scopes[1].clone();
    let m=f.hive.send(&a,send("b","retry",MessageAct::Inform,None,"work"),1).unwrap();
    for attempt in 1..=MAX_ATTEMPTS {
        let d=f.hive.claim(&b,attempt as i64,10).unwrap().unwrap();
        assert_eq!(d.attempt,attempt);
        let result=f.hive.retry(&b,&d,attempt as i64,0).unwrap();
        assert_eq!(result,if attempt==MAX_ATTEMPTS {DeliveryState::Dead} else {DeliveryState::Retry});
    }
    assert!(f.hive.claim(&b,10,10).unwrap().is_none());
    f.hive.replay(&b,&m.id,11).unwrap();
    let d=f.hive.claim(&b,11,10).unwrap().unwrap();
    assert_eq!(d.attempt,1);assert_eq!(d.message.id,m.id);
    f.hive.ack(&b,&d,None,12).unwrap();
    assert!(matches!(f.hive.ack(&b,&d,None,13),Err(HiveError::LeaseMismatch)));
}

#[test]
fn process_kill_during_fake_turn_then_daemon_restart() {
    let mut f=fixture(3);let a=f.scopes[0].clone();let b=f.scopes[1].clone();
    let m=f.hive.send(&a,send("b","crash",MessageAct::Inform,None,"work"),1).unwrap();
    let marker=f.dir.join("child-claimed");
    let mut child=std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact").arg("crash_child").arg("--nocapture")
        .env("HIVE_CRASH_DB",&f.db).env("HIVE_CRASH_ROOT",&f.root).env("HIVE_CRASH_MARKER",&marker)
        .spawn().unwrap();
    let mut ready=false;
    for _ in 0..500 {
        if marker.exists() {ready=true;break;}
        if child.try_wait().unwrap().is_some(){break;}
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    if !ready {let _=child.kill();let _=child.wait();panic!("child did not claim delivery");}
    child.kill().unwrap();let _=child.wait();
    // The child died after claiming and before committing an ack. Reopen the same database.
    let mut recovered=Hive::open(&f.db).unwrap();
    assert_eq!(recovered.recover(50).unwrap(),1);
    let d=recovered.claim(&b,50,30).unwrap().unwrap();
    assert_eq!(d.message.id,m.id);assert_eq!(d.attempt,2);
    recovered.ack(&b,&d,None,51).unwrap();
    assert!(recovered.claim(&b,90,30).unwrap().is_none());
}
#[test]
fn crash_child() {
    let Ok(db)=std::env::var("HIVE_CRASH_DB") else {return};
    let root=std::env::var("HIVE_CRASH_ROOT").unwrap();
    let marker=std::env::var("HIVE_CRASH_MARKER").unwrap();
    let mut hive=Hive::open(db).unwrap();
    let b=scope("floor-1","b","owner-1",&root);
    let d=hive.claim(&b,10,30).unwrap().unwrap();
    // Simulate an in-progress adapter turn with a durable lease and no ack yet.
    let mut fake=FakeAdapter::default();
    use agentdock_hive::Adapter;
    let _=fake.turn(&b,&d.message).unwrap();
    std::fs::write(marker,d.message.id).unwrap();
    std::thread::sleep(std::time::Duration::from_secs(30));
}
