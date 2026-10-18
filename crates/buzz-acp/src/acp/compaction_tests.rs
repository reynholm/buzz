use super::*;
use std::time::Duration;

#[tokio::test]
async fn initialize_advertises_compaction_without_losing_existing_capabilities() {
    let script = r#"import sys,json
for line in sys.stdin:
 r=json.loads(line)
 if r.get('method')=='initialize':
  c=r['params']['clientCapabilities']
  assert c['session']['compaction']=={}
  assert c['auth']['terminal'] is True
  assert c['_meta']['goose']['customNotifications'] is True
  assert c['_meta']['terminal-auth'] is True
  print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':{'protocolVersion':2,'agentInfo':{'name':'probe','version':'1'}}}),flush=True)
"#;
    let mut client = AcpClient::spawn(
        "/usr/bin/python3",
        &["-c".into(), script.into()],
        &[],
        false,
    )
    .await
    .unwrap();
    let result = client.initialize().await;
    client.shutdown().await;
    assert!(
        result.is_ok(),
        "fake ACP rejected actual initialize frame: {result:?}"
    );
}

fn origin() -> buzz_core::session_control::ControlOrigin {
    buzz_core::session_control::ControlOrigin {
        command_event_id: "a".repeat(64),
        actor_pubkey: "b".repeat(64),
    }
}
fn update(session: &str, id: &str, status: &str) -> serde_json::Value {
    serde_json::json!({"params":{"sessionId":session,"update":{"sessionUpdate":"compaction_update","compactionId":id,"status":status}}})
}
async fn client() -> AcpClient {
    AcpClient::spawn(
        "/usr/bin/python3",
        &["-c".into(), "import sys;sys.stdin.read()".into()],
        &[],
        false,
    )
    .await
    .unwrap()
}
#[tokio::test]
async fn compaction_standard_dispatch_terminal_only_duplicate_and_next_request() {
    let mut c = client().await;
    c.route_compaction("s", uuid::Uuid::new_v4());
    c.begin_compaction("s", origin()).unwrap();
    c.handle_session_update(&update("s", "one", "completed"));
    assert_eq!(c.finish_compaction("s"), Ok(()));
    c.begin_compaction("s", origin()).unwrap();
    c.handle_session_update(&update("s", "one", "completed"));
    assert_eq!(
        c.finish_compaction("s"),
        Err(buzz_core::session_control::ControlFailure::Unconfirmed)
    );
    c.shutdown().await;
}
#[tokio::test]
async fn compaction_standard_dispatch_never_guesses_success() {
    use buzz_core::session_control::ControlFailure as F;
    let mut c = client().await;
    for (status, expected) in [
        ("failed", F::CompactFailed),
        ("cancelled", F::Cancelled),
        ("future_status", F::Unconfirmed),
    ] {
        c.route_compaction("s", uuid::Uuid::new_v4());
        c.begin_compaction("s", origin()).unwrap();
        c.handle_session_update(&update("wrong", "one", "completed"));
        c.handle_session_update(&update("s", "one", status));
        assert_eq!(c.finish_compaction("s"), Err(expected));
    }
    c.clear_session_status("s");
    c.handle_session_update(&update("s", "old", "completed"));
    c.route_compaction("s", uuid::Uuid::new_v4());
    c.begin_compaction("s", origin()).unwrap();
    assert_eq!(c.finish_compaction("s"), Err(F::Unconfirmed));
    c.shutdown().await;
}
#[tokio::test]
async fn compaction_tracker_never_evicts_active() {
    use buzz_core::session_control::ControlFailure as F;
    let mut c = client().await;
    c.route_compaction("s", uuid::Uuid::new_v4());
    for n in 0..128 {
        c.handle_session_update(&update("s", &n.to_string(), "in_progress"));
    }
    assert_eq!(c.begin_compaction("s", origin()), Err(F::Busy));
    c.handle_session_update(&update("s", "0", "completed"));
    c.begin_compaction("s", origin()).unwrap();
    c.handle_session_update(&update("s", "new", "completed"));
    assert_eq!(c.finish_compaction("s"), Ok(()));
    c.shutdown().await;
}

#[tokio::test]
async fn typed_compaction_changes_only_transition_and_manual_actor() {
    use buzz_core::session_status::*;
    let root = tempfile::tempdir().unwrap();
    let runtime = crate::session_status::SessionStatusRuntime::open(
        root.path(),
        canonical_identity(
            "wss://relay.example",
            &nostr::Keys::generate().public_key().to_hex(),
        )
        .unwrap(),
    )
    .unwrap();
    let scope = StatusScope {
        channel_id: uuid::Uuid::new_v4(),
        thread_root: None,
    };
    let binding = runtime
        .begin(
            scope.clone(),
            "s",
            SessionState::Resumed,
            StatusReason::IdleSleep,
        )
        .unwrap();
    binding.record_occupancy(23, 100);
    binding.set_history(HistoryCoverage {
        read: Some(4),
        total: Some(4),
        exact: true,
        incomplete: false,
    });
    let mut c = client().await;
    c.bind_session_status("s", binding.clone());
    let read = || {
        parse_snapshot(
            &std::fs::read(runtime.dir().join(scope_file_name(&scope).unwrap())).unwrap(),
        )
        .unwrap()
    };
    let before = read();
    c.begin_compaction("s", origin()).unwrap();
    c.handle_session_update(&update("s", "manual", "completed"));
    let manual = read();
    assert_eq!(manual.generation, before.generation);
    assert_eq!(manual.status.occupancy, before.status.occupancy);
    assert_eq!(manual.status.history, before.status.history);
    assert_eq!(manual.status.state, SessionState::Compacted);
    assert_eq!(manual.status.reason, StatusReason::UserCompact);
    assert_eq!(manual.origin, Some(origin()));
    let mut enriched = update("s", "manual", "completed");
    enriched["params"]["update"]["summary"] = serde_json::json!({"text":"enriched"});
    c.handle_session_update(&enriched);
    assert_eq!(read().status.transition_id, manual.status.transition_id);
    c.finish_compaction("s").unwrap();
    c.handle_session_update(&update("s", "auto", "completed"));
    let auto = read();
    assert_ne!(auto.status.transition_id, manual.status.transition_id);
    assert_eq!(auto.status.reason, StatusReason::AutoCompact);
    assert_eq!(auto.origin, None);
    binding.retire();
    c.handle_session_update(&update("s", "retired", "completed"));
    assert!(!runtime
        .dir()
        .join(scope_file_name(&scope).unwrap())
        .exists());
    c.shutdown().await;
}

#[tokio::test]
async fn resumed_pre_ack_compaction_cannot_become_a_manual_completion_or_actor() {
    use buzz_core::session_status::*;
    let script = r#"import sys,json
for line in sys.stdin:
 r=json.loads(line); method=r.get('method')
 if 'id' not in r: continue
 if method=='initialize': result={'protocolVersion':2,'agentCapabilities':{'sessionCapabilities':{'resume':{}}}}
 elif method=='session/resume':
  print(json.dumps({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'s','update':{'sessionUpdate':'compaction_update','compactionId':'before-bind','status':'in_progress'}}}),flush=True)
  result={}
 elif method=='session/prompt':
  print(json.dumps({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'s','update':{'sessionUpdate':'compaction_update','compactionId':'before-bind','status':'completed'}}}),flush=True)
  result={'stopReason':'end_turn'}
 else: result={}
 print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':result}),flush=True)
"#;
    let mut c = AcpClient::spawn(
        "/usr/bin/python3",
        &["-c".into(), script.into()],
        &[],
        false,
    )
    .await
    .unwrap();
    c.initialize().await.unwrap();
    c.route_compaction("s", uuid::Uuid::new_v4());
    c.session_resume("s", "/tmp", vec![], None, None)
        .await
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let runtime = crate::session_status::SessionStatusRuntime::open(
        root.path(),
        canonical_identity(
            "wss://relay.example",
            &nostr::Keys::generate().public_key().to_hex(),
        )
        .unwrap(),
    )
    .unwrap();
    let scope = StatusScope {
        channel_id: uuid::Uuid::new_v4(),
        thread_root: None,
    };
    let binding = runtime
        .begin(
            scope.clone(),
            "s",
            SessionState::Resumed,
            StatusReason::IdleSleep,
        )
        .unwrap();
    c.bind_session_status("s", binding);
    c.begin_compaction("s", origin()).unwrap();
    c.session_prompt_blocks_with_idle_timeout(
        "s",
        &["/compact"],
        Duration::from_secs(2),
        Duration::from_secs(2),
    )
    .await
    .unwrap();
    let outcome = c.finish_compaction("s");
    c.shutdown().await;
    assert_eq!(
        outcome,
        Err(buzz_core::session_control::ControlFailure::Unconfirmed)
    );
    let snapshot = parse_snapshot(
        &std::fs::read(runtime.dir().join(scope_file_name(&scope).unwrap())).unwrap(),
    )
    .unwrap();
    assert_eq!(snapshot.status.reason, StatusReason::AutoCompact);
    assert_eq!(snapshot.origin, None);
}

#[test]
fn compaction_resume_adoption_requires_success_and_the_same_current_route() {
    let mut tracker = compaction::CompactionTracker::default();
    let old = tracker.prepare_resume("s");
    let current = tracker.prepare_resume("s");
    tracker.set_advertised("s");
    tracker.complete_resume("s", old, true);
    tracker.bind("s", uuid::Uuid::new_v4());
    assert!(
        !tracker.supported("s"),
        "stale completion adopted a newer route"
    );
    tracker.complete_resume("s", current, true);
    tracker.bind("s", uuid::Uuid::new_v4());
    assert!(!tracker.supported("s"));

    let current = tracker.prepare_resume("s");
    tracker.set_advertised("s");
    tracker.complete_resume("s", current, false);
    tracker.bind("s", uuid::Uuid::new_v4());
    assert!(!tracker.supported("s"), "failed resume remained adoptable");

    let current = tracker.prepare_resume("s");
    tracker.set_advertised("s");
    tracker.complete_resume("s", current, true);
    tracker.clear("s");
    tracker.bind("s", uuid::Uuid::new_v4());
    assert!(
        !tracker.supported("s"),
        "retired route revived its metadata"
    );
}
