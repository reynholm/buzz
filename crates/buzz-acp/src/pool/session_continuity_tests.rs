use super::super::{run_prompt_task, ChannelInfoResolver, FlushBatch, PromptOutcome, RestClient};
use super::*;
use crate::acp::{AcpClient, StopReason};
use nostr::{EventBuilder, Keys, Kind, Tag};
use std::collections::HashMap;
use crate::session_store::SessionRecord;
use std::time::Duration;

/// Fake ACP adapter. Logs every method to `$LOG`; `RESUME_CAP=1` advertises
/// resume, `RESUME_ERROR=1` answers resume with -32002, `DELAY=<secs>` delays
/// every session/new and session/resume answer.
const ADAPTER: &str = r#"
import json, os, sys, time
log = open(os.environ['LOG'], 'a', buffering=1)
delay = float(os.environ.get('DELAY', '0'))
created = 0
for line in sys.stdin:
    req = json.loads(line)
    method = req.get('method')
    log.write(f"{method}\n")
    if 'id' not in req or method is None:
        continue
    reply = {'jsonrpc': '2.0', 'id': req['id']}
    if method == 'initialize':
        caps = {'sessionCapabilities': {'resume': {}}} if os.environ.get('RESUME_CAP') == '1' else {}
        reply['result'] = {'protocolVersion': 1, 'agentCapabilities': caps}
    elif method == 'session/new':
        time.sleep(delay)
        created += 1
        reply['result'] = {'sessionId': f'new-{created}'}
    elif method == 'session/resume':
        time.sleep(delay)
        if os.environ.get('RESUME_ERROR') == '1':
            reply['error'] = {'code': -32002, 'message': 'Resource not found'}
        else:
            reply['result'] = {}
    elif method == 'session/prompt':
        if os.environ.get('PROMPTS'):
            with open(os.environ['PROMPTS'], 'a') as out:
                out.write(json.dumps(req['params']) + '\n')
        reply['result'] = {'stopReason': 'end_turn'}
    else:
        reply['result'] = {}
    print(json.dumps(reply), flush=True)
"#;

struct Fixture {
    dir: tempfile::TempDir,
    store: Arc<SessionStore>,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(SessionStore::open(dir.path().join("store")).unwrap());
        Self { dir, store }
    }

    fn log(&self) -> std::path::PathBuf {
        self.dir.path().join("methods.log")
    }

    fn methods(&self) -> Vec<String> {
        std::fs::read_to_string(self.log())
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    async fn agent(&self, env: &[(&str, &str)]) -> OwnedAgent {
        let mut vars = vec![("LOG".to_owned(), self.log().display().to_string())];
        vars.extend(env.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        let mut acp = AcpClient::spawn("python3", &["-c".into(), ADAPTER.into()], &vars, false)
            .await
            .unwrap();
        acp.initialize().await.unwrap();
        OwnedAgent {
            index: 0,
            acp,
            state: SessionState::with_store(Some(self.store.clone())),
            model_capabilities: None,
            desired_model: None,
            model_overridden: false,
            desired_model_request_id: None,
            desired_model_pending_ack: false,
            startup_effort: None,
            agent_name: "fake-acp".into(),
            goose_system_prompt_supported: None,
            protocol_version: 1,
            background_context: None,
        }
    }

    /// A resumable record for `scope`, as left by a worker that went to sleep.
    fn departed_record(&self, scope: &SessionScope, ctx: &PromptContext, delivered: &[&str]) {
        let lease = self
            .store
            .claim(scope, &Expect::Missing, live_session(&self.store, ctx, "old-1"))
            .unwrap();
        self.store
            .commit(scope, lease, |live| {
                live.departed = Some(Departure::IdleSleep);
                live.turns = 1;
                live.delivered_event_ids = delivered.iter().map(|id| id.to_string()).collect();
            })
            .unwrap();
    }

    fn record(&self, scope: &SessionScope) -> SessionRecord {
        match self.store.load(scope) {
            LoadOutcome::Found(record) => record,
            other => panic!("expected record, got {other:?}"),
        }
    }
}

fn scope() -> SessionScope {
    SessionScope::Thread {
        channel_id: Uuid::from_u128(42),
        root_event_id: "ab".repeat(32),
    }
}

fn ctx() -> PromptContext {
    let mut ctx = super::super::tests::make_prompt_context_no_owner();
    ctx.cwd = "/tmp".into();
    ctx
}

fn live_session(store: &SessionStore, ctx: &PromptContext, sid: &str) -> LiveSession {
    LiveSession {
        lease: Uuid::nil(),
        adapter: "fake-acp".into(),
        session_id: sid.into(),
        cwd: ctx.cwd.clone(),
        model: None,
        instance: store.instance(),
        worker_index: 0,
        departed: None,
        turns: 0,
        standing_context_sent: false,
        delivered_event_ids: Vec::new(),
        hydrated_thread_roots: Vec::new(),
    }
}

fn channel_context(scope: &SessionScope) -> NewSessionChannelContext<'_> {
    NewSessionChannelContext {
        huddle_instructions: None,
        canvas: None,
        name: None,
        scope: Some(scope),
        channel_type: None,
    }
}

// ---- decision table ----

fn me(store_instance: Uuid) -> ResumeContext {
    ResumeContext {
        adapter: "fake-acp".into(),
        cwd: "/tmp".into(),
        model: None,
        instance: store_instance,
        worker_index: 0,
        resume_supported: true,
        poisoned: false,
    }
}

fn stored(live: impl FnOnce(&mut LiveSession)) -> LoadOutcome {
    let mut session = LiveSession {
        lease: Uuid::new_v4(),
        adapter: "fake-acp".into(),
        session_id: "s-1".into(),
        cwd: "/tmp".into(),
        model: None,
        instance: Uuid::from_u128(1),
        worker_index: 0,
        departed: None,
        turns: 0,
        standing_context_sent: false,
        delivered_event_ids: Vec::new(),
        hydrated_thread_roots: Vec::new(),
    };
    live(&mut session);
    LoadOutcome::Found(SessionRecord {
        version: 1,
        live: Some(session),
        reason: None,
    })
}

fn decide(record: LoadOutcome, me: ResumeContext, alive: bool) -> ResumeDecision {
    resume_decision(&record, &me, |_| alive)
}

fn new_reason(decision: ResumeDecision) -> SessionReason {
    match decision {
        ResumeDecision::New { reason } => reason,
        other => panic!("expected a new session, got {other:?}"),
    }
}

fn resume_reason(decision: ResumeDecision) -> SessionReason {
    match decision {
        ResumeDecision::Resume { reason, .. } => reason,
        other => panic!("expected a resume, got {other:?}"),
    }
}

const ME: Uuid = Uuid::from_u128(1);

#[test]
fn decision_poisoned_is_not_persisted() {
    let mut ctx = me(ME);
    ctx.poisoned = true;
    assert_eq!(
        new_reason(decide(stored(|_| {}), ctx, false)),
        SessionReason::ResumeFailed("not_persisted".into())
    );
}

#[test]
fn decision_missing_is_first_in_thread() {
    assert_eq!(
        new_reason(decide(LoadOutcome::Missing, me(ME), false)),
        SessionReason::FirstInThread
    );
}

#[test]
fn decision_unreadable_is_resume_failed() {
    assert_eq!(
        new_reason(decide(LoadOutcome::Unreadable, me(ME), false)),
        SessionReason::ResumeFailed("store_unreadable".into())
    );
}

#[test]
fn decision_tombstone_is_discarded() {
    let tombstone = LoadOutcome::Found(SessionRecord {
        version: 1,
        live: None,
        reason: Some(SessionReason::Discarded),
    });
    assert_eq!(
        new_reason(decide(tombstone, me(ME), false)),
        SessionReason::Discarded
    );
}

#[test]
fn decision_unfinished_claim_is_first_in_thread() {
    assert_eq!(
        new_reason(decide(stored(|l| l.session_id.clear()), me(ME), false)),
        SessionReason::FirstInThread
    );
}

#[test]
fn decision_model_change_is_config_changed() {
    assert_eq!(
        new_reason(decide(stored(|l| l.model = Some("other".into())), me(ME), false)),
        SessionReason::ConfigChanged
    );
}

#[test]
fn decision_without_capability_is_unsupported() {
    let mut ctx = me(ME);
    ctx.resume_supported = false;
    assert_eq!(
        new_reason(decide(stored(|l| l.departed = Some(Departure::IdleSleep)), ctx, false)),
        SessionReason::ResumeUnsupported
    );
}

#[test]
fn decision_other_live_instance_is_live_elsewhere() {
    assert_eq!(
        new_reason(decide(stored(|l| l.instance = Uuid::from_u128(2)), me(ME), true)),
        SessionReason::LiveElsewhere
    );
}

#[test]
fn decision_dead_instance_resumes_as_harness_restart() {
    assert_eq!(
        resume_reason(decide(stored(|l| l.instance = Uuid::from_u128(2)), me(ME), false)),
        SessionReason::HarnessRestart
    );
}

#[test]
fn decision_live_on_other_worker_is_live_elsewhere() {
    assert_eq!(
        new_reason(decide(stored(|l| l.worker_index = 3), me(ME), false)),
        SessionReason::LiveElsewhere
    );
}

#[test]
fn decision_departed_resumes_with_departure_reason() {
    assert_eq!(
        resume_reason(decide(
            stored(|l| {
                l.worker_index = 3;
                l.departed = Some(Departure::IdleSleep);
            }),
            me(ME),
            false
        )),
        SessionReason::IdleSleep
    );
}

#[test]
fn decision_same_slot_unmarked_resumes_as_worker_crash() {
    assert_eq!(
        resume_reason(decide(stored(|_| {}), me(ME), false)),
        SessionReason::WorkerCrash
    );
}

// ---- SessionState with a real store ----

fn owned_state(fixture: &Fixture, scope: &SessionScope, sid: &str) -> (SessionState, Uuid) {
    let ctx = ctx();
    let lease = fixture
        .store
        .claim(scope, &Expect::Missing, live_session(&fixture.store, &ctx, sid))
        .unwrap();
    let mut state = SessionState::with_store(Some(fixture.store.clone()));
    state.sessions.insert(scope.clone(), sid.into());
    state.leases.insert(scope.clone(), lease);
    (state, lease)
}

#[test]
fn delivery_success_persists_ids() {
    let fixture = Fixture::new();
    let scope = scope();
    let (mut state, _) = owned_state(&fixture, &scope, "s-1");
    state.mark_scope_delivery_success(
        scope.clone(),
        true,
        ["e1".to_owned(), "e2".to_owned()],
        ["root".to_owned()],
    );
    let live = fixture.record(&scope).live.unwrap();
    assert_eq!(live.delivered_event_ids, ["e1", "e2"]);
    assert!(live.standing_context_sent);
    assert_eq!(live.hydrated_thread_roots, ["root"]);
}

#[test]
fn invalidate_scope_writes_tombstone() {
    let fixture = Fixture::new();
    let scope = scope();
    let (mut state, _) = owned_state(&fixture, &scope, "s-1");
    assert!(state.invalidate_scope(&scope));
    let record = fixture.record(&scope);
    assert_eq!(record.live, None);
    assert_eq!(record.reason, Some(SessionReason::Discarded));
}

#[test]
fn stale_owner_invalidation_keeps_newer_record() {
    let fixture = Fixture::new();
    let scope = scope();
    let (mut stale, _) = owned_state(&fixture, &scope, "s-a");
    let expect = Expect::from_outcome(&fixture.store.load(&scope));
    let newer = fixture
        .store
        .claim(&scope, &expect, live_session(&fixture.store, &ctx(), "s-b"))
        .unwrap();
    stale.invalidate_scope(&scope);
    let live = fixture.record(&scope).live.unwrap();
    assert_eq!(live.session_id, "s-b");
    assert_eq!(live.lease, newer);
}

#[test]
fn invalidate_all_marks_worker_crash_without_tombstone() {
    let fixture = Fixture::new();
    let scope = scope();
    let (mut state, _) = owned_state(&fixture, &scope, "s-1");
    state.invalidate_all();
    assert!(state.sessions.is_empty());
    let live = fixture.record(&scope).live.unwrap();
    assert_eq!(live.departed, Some(Departure::WorkerCrash));
}

#[test]
fn depart_all_marks_idle_sleep_and_keeps_memory() {
    let fixture = Fixture::new();
    let scope = scope();
    let (mut state, _) = owned_state(&fixture, &scope, "s-1");
    state.depart_all(Departure::IdleSleep);
    assert!(state.sessions.contains_key(&scope));
    assert_eq!(
        fixture.record(&scope).live.unwrap().departed,
        Some(Departure::IdleSleep)
    );
}

// ---- acquire_session against a fake adapter ----

#[tokio::test]
async fn resume_advertised_uses_session_resume_not_new() {
    let fixture = Fixture::new();
    let scope = scope();
    let ctx = ctx();
    fixture.departed_record(&scope, &ctx, &["e-old"]);
    let mut agent = fixture.agent(&[("RESUME_CAP", "1")]).await;
    let (sid, start) = acquire_session(&mut agent, &ctx, None, channel_context(&scope), &scope)
        .await
        .unwrap();
    agent.acp.shutdown().await;
    assert_eq!(sid, "old-1");
    assert_eq!(
        start,
        SessionStart::Resumed {
            reason: SessionReason::IdleSleep
        }
    );
    let methods = fixture.methods();
    assert!(methods.contains(&"session/resume".to_owned()));
    assert!(!methods.contains(&"session/new".to_owned()));
    assert!(agent.state.deliveries[&scope]
        .delivered_event_ids
        .contains("e-old"));
    assert_eq!(agent.state.turn_counts[&scope], 1);
    let live = fixture.record(&scope).live.unwrap();
    assert_eq!(live.departed, None);
    assert_eq!(Some(&live.lease), agent.state.leases.get(&scope));
}

#[tokio::test]
async fn resume_error_falls_back_to_new_session() {
    let fixture = Fixture::new();
    let scope = scope();
    let ctx = ctx();
    fixture.departed_record(&scope, &ctx, &["e-old"]);
    let mut agent = fixture
        .agent(&[("RESUME_CAP", "1"), ("RESUME_ERROR", "1")])
        .await;
    let (sid, start) = acquire_session(&mut agent, &ctx, None, channel_context(&scope), &scope)
        .await
        .unwrap();
    agent.acp.shutdown().await;
    assert_eq!(sid, "new-1");
    assert_eq!(
        start,
        SessionStart::New {
            reason: SessionReason::ResumeFailed("not_found".into())
        }
    );
    let methods = fixture.methods();
    let resume_at = methods.iter().position(|m| m == "session/resume").unwrap();
    let new_at = methods.iter().position(|m| m == "session/new").unwrap();
    assert!(resume_at < new_at);
    let live = fixture.record(&scope).live.unwrap();
    assert_eq!(live.session_id, "new-1");
    assert!(live.delivered_event_ids.is_empty());
}

#[tokio::test]
async fn resume_not_advertised_creates_new_without_attempt() {
    let fixture = Fixture::new();
    let scope = scope();
    let ctx = ctx();
    fixture.departed_record(&scope, &ctx, &[]);
    let mut agent = fixture.agent(&[]).await;
    let (_, start) = acquire_session(&mut agent, &ctx, None, channel_context(&scope), &scope)
        .await
        .unwrap();
    agent.acp.shutdown().await;
    assert_eq!(
        start,
        SessionStart::New {
            reason: SessionReason::ResumeUnsupported
        }
    );
    assert!(!fixture.methods().contains(&"session/resume".to_owned()));
}

#[tokio::test]
async fn failed_claim_never_calls_resume() {
    use fs2::FileExt;
    let fixture = Fixture::new();
    let scope = scope();
    let ctx = ctx();
    fixture.departed_record(&scope, &ctx, &[]);
    let channel_dir = fixture
        .dir
        .path()
        .join("store")
        .join(scope.channel_id().to_string());
    let record = std::fs::read_dir(&channel_dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "json"))
        .unwrap();
    let mut lock_path = record.into_os_string();
    lock_path.push(".lock");
    let holder = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock_path)
        .unwrap();
    holder.lock_exclusive().unwrap();
    let mut agent = fixture.agent(&[("RESUME_CAP", "1")]).await;
    let (_, start) = acquire_session(&mut agent, &ctx, None, channel_context(&scope), &scope)
        .await
        .unwrap();
    agent.acp.shutdown().await;
    FileExt::unlock(&holder).unwrap();
    assert_eq!(
        start,
        SessionStart::New {
            reason: SessionReason::ResumeFailed("store_busy".into())
        }
    );
    let methods = fixture.methods();
    assert!(!methods.contains(&"session/resume".to_owned()));
    assert!(methods.contains(&"session/new".to_owned()));
    assert!(!agent.state.leases.contains_key(&scope));
}

/// Tombstone `scope` from another thread every `period` until `until`.
fn rotate_repeatedly(
    store: Arc<SessionStore>,
    scope: SessionScope,
    first_after: Duration,
    period: Duration,
    until: Duration,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        std::thread::sleep(first_after);
        while started.elapsed() < until {
            let _ = store.discard_scope(&scope);
            std::thread::sleep(period);
        }
    })
}

#[tokio::test]
async fn resume_result_after_rotate_is_dropped() {
    let fixture = Fixture::new();
    let scope = scope();
    let ctx = ctx();
    fixture.departed_record(&scope, &ctx, &[]);
    let mut agent = fixture
        .agent(&[("RESUME_CAP", "1"), ("DELAY", "0.6")])
        .await;
    // One rotate, landing while the resume call is in flight.
    let rotate = rotate_repeatedly(
        fixture.store.clone(),
        scope.clone(),
        Duration::from_millis(200),
        Duration::from_secs(10),
        Duration::from_millis(300),
    );
    let (sid, start) = acquire_session(&mut agent, &ctx, None, channel_context(&scope), &scope)
        .await
        .unwrap();
    rotate.join().unwrap();
    agent.acp.shutdown().await;
    assert_eq!(sid, "new-1");
    assert_eq!(
        start,
        SessionStart::New {
            reason: SessionReason::Discarded
        }
    );
    assert_eq!(fixture.record(&scope).live.unwrap().session_id, "new-1");
}

#[tokio::test]
async fn new_result_after_rotate_is_dropped() {
    let fixture = Fixture::new();
    let scope = scope();
    let ctx = ctx();
    let mut agent = fixture.agent(&[("DELAY", "0.6")]).await;
    let rotate = rotate_repeatedly(
        fixture.store.clone(),
        scope.clone(),
        Duration::from_millis(200),
        Duration::from_secs(10),
        Duration::from_millis(300),
    );
    let (sid, start) = acquire_session(&mut agent, &ctx, None, channel_context(&scope), &scope)
        .await
        .unwrap();
    rotate.join().unwrap();
    agent.acp.shutdown().await;
    assert_eq!(sid, "new-2", "the session from the lost round is not used");
    assert_eq!(
        start,
        SessionStart::New {
            reason: SessionReason::Discarded
        }
    );
    assert_eq!(fixture.record(&scope).live.unwrap().session_id, "new-2");
}

#[tokio::test]
async fn ownership_lost_three_times_runs_unpersisted() {
    let fixture = Fixture::new();
    let scope = scope();
    let ctx = ctx();
    let mut agent = fixture.agent(&[("DELAY", "0.3")]).await;
    let rotate = rotate_repeatedly(
        fixture.store.clone(),
        scope.clone(),
        Duration::from_millis(50),
        Duration::from_millis(50),
        Duration::from_secs(1),
    );
    let (sid, start) = acquire_session(&mut agent, &ctx, None, channel_context(&scope), &scope)
        .await
        .unwrap();
    rotate.join().unwrap();
    agent.acp.shutdown().await;
    assert_eq!(
        start,
        SessionStart::New {
            reason: SessionReason::LiveElsewhere
        }
    );
    assert_eq!(sid, "new-4");
    assert!(!agent.state.leases.contains_key(&scope));
}

// ---- run_prompt_task: the production seams ----

fn mention(keys: &Keys, channel_id: Uuid, root: Option<&str>, text: &str) -> nostr::Event {
    let mut tags = vec![Tag::parse(["h", channel_id.to_string().as_str()]).unwrap()];
    if let Some(root) = root {
        tags.push(Tag::parse(["e", root, "", "reply"]).unwrap());
    }
    EventBuilder::new(Kind::Custom(9), text)
        .tags(tags)
        .sign_with_keys(keys)
        .unwrap()
}

fn batch(scope: &SessionScope, event: nostr::Event) -> FlushBatch {
    FlushBatch {
        continuation: None,
        channel_id: scope.channel_id(),
        scope: scope.clone(),
        events: vec![crate::queue::BatchEvent {
            edit: None,
            event,
            prompt_tag: "@mention".into(),
            received_at: std::time::Instant::now(),
        }],
        cancelled_events: vec![],
        cancel_reason: None,
    }
}

async fn run_turn(agent: OwnedAgent, ctx: &Arc<PromptContext>, batch: FlushBatch) -> OwnedAgent {
    let (result_tx, mut result_rx) = tokio::sync::mpsc::unbounded_channel();
    run_prompt_task(agent, Some(batch), None, ctx.clone(), result_tx, None, "turn".into()).await;
    let result = result_rx.recv().await.expect("prompt result");
    assert!(
        matches!(result.outcome, PromptOutcome::Ok(StopReason::EndTurn)),
        "turn did not end normally"
    );
    result.agent
}

#[tokio::test]
async fn max_turns_survives_resume() {
    let fixture = Fixture::new();
    let human = Keys::generate();
    let channel_id = Uuid::from_u128(42);
    let root = mention(&human, channel_id, None, "first");
    let scope = SessionScope::Thread {
        channel_id,
        root_event_id: root.id.to_hex(),
    };
    let mut ctx = ctx();
    ctx.context_message_limit = 0;
    ctx.max_turns_per_session = 2;
    let ctx = Arc::new(ctx);

    let agent = fixture.agent(&[("RESUME_CAP", "1")]).await;
    let mut agent = run_turn(agent, &ctx, batch(&scope, root.clone())).await;
    assert_eq!(fixture.record(&scope).live.unwrap().turns, 1);
    // The pool sleeps: sessions are marked departed and the process goes away.
    agent.state.depart_all(Departure::IdleSleep);
    agent.acp.shutdown().await;

    let woken = fixture.agent(&[("RESUME_CAP", "1")]).await;
    let follow_up = mention(&human, channel_id, Some(&root.id.to_hex()), "second");
    let mut agent = run_turn(woken, &ctx, batch(&scope, follow_up)).await;
    agent.acp.shutdown().await;
    assert!(fixture.methods().contains(&"session/resume".to_owned()));
    assert!(
        !agent.state.sessions.contains_key(&scope),
        "the second successful turn reaches max_turns and rotates"
    );
    assert_eq!(fixture.record(&scope).live, None);
}

#[tokio::test]
async fn resumed_session_gets_only_delta() {

    let fixture = Fixture::new();
    let human = Keys::generate();
    let channel_id = Uuid::from_u128(42);
    let root = mention(&human, channel_id, None, "original question");
    let root_id = root.id.to_hex();
    let old_reply = mention(&human, channel_id, Some(&root_id), "OLD-CONTEXT-ALREADY-SEEN");
    let new_reply = mention(&human, channel_id, Some(&root_id), "NEW-CONTEXT-AFTER-SLEEP");
    let trigger = mention(&human, channel_id, Some(&root_id), "follow-up mention");
    let scope = SessionScope::Thread {
        channel_id,
        root_event_id: root_id.clone(),
    };

    let (ctx, server) = relay_context(
        channel_id,
        vec![root.clone(), old_reply.clone(), new_reply.clone(), trigger.clone()],
    )
    .await;
    let ctx = Arc::new(ctx);
    fixture.departed_record(&scope, &ctx, &[&root_id, &old_reply.id.to_hex()]);

    let prompts = fixture.dir.path().join("prompts.ndjson");
    let agent = fixture
        .agent(&[("RESUME_CAP", "1"), ("PROMPTS", prompts.to_str().unwrap())])
        .await;
    let mut agent = run_turn(agent, &ctx, batch(&scope, trigger)).await;
    agent.acp.shutdown().await;
    server.abort();

    assert!(fixture.methods().contains(&"session/resume".to_owned()));
    let prompt = std::fs::read_to_string(&prompts).unwrap();
    assert!(prompt.contains("NEW-CONTEXT-AFTER-SLEEP"));
    assert!(!prompt.contains("OLD-CONTEXT-ALREADY-SEEN"));
}

// ---- pool control paths (sleep, !rotate, channel removal) ----

async fn idle_worker(fixture: &Fixture, index: usize, scope: &SessionScope, sid: &str) -> OwnedAgent {
    let acp = AcpClient::spawn("cat", &[], &[], false).await.unwrap();
    let (mut state, _) = owned_state(fixture, scope, sid);
    state.store = Some(fixture.store.clone());
    OwnedAgent {
        index,
        acp,
        state,
        model_capabilities: None,
        desired_model: None,
        model_overridden: false,
        desired_model_request_id: None,
        desired_model_pending_ack: false,
        startup_effort: None,
        agent_name: "fake-acp".into(),
        goose_system_prompt_supported: None,
        protocol_version: 1,
        background_context: None,
    }
}

fn thread_scope(channel: u128, root: &str) -> SessionScope {
    SessionScope::Thread {
        channel_id: Uuid::from_u128(channel),
        root_event_id: root.repeat(64 / root.len()),
    }
}

fn decision_now(fixture: &Fixture, scope: &SessionScope) -> ResumeDecision {
    let me = ResumeContext {
        adapter: "fake-acp".into(),
        cwd: "/tmp".into(),
        model: None,
        instance: fixture.store.instance(),
        worker_index: 0,
        resume_supported: true,
        poisoned: fixture.store.is_poisoned(scope),
    };
    resume_decision(&fixture.store.load(scope), &me, |_| false)
}

#[tokio::test]
async fn pool_sleep_marks_every_idle_session() {
    let fixture = Fixture::new();
    let (a, b) = (thread_scope(42, "a"), thread_scope(42, "b"));
    let mut pool = AgentPool::from_slots(vec![
        Some(idle_worker(&fixture, 0, &a, "s-a").await),
        Some(idle_worker(&fixture, 1, &b, "s-b").await),
    ]);
    pool.set_session_store(Some(fixture.store.clone()));
    pool.depart_idle_sessions(Departure::IdleSleep);
    for scope in [&a, &b] {
        assert_eq!(
            fixture.record(scope).live.unwrap().departed,
            Some(Departure::IdleSleep)
        );
    }
}

#[test]
fn rotate_before_first_resume_discards_disk_session() {
    let fixture = Fixture::new();
    let scope = thread_scope(42, "a");
    fixture.departed_record(&scope, &ctx(), &[]);
    // After sleep the pool has only empty slots.
    let mut pool = AgentPool::from_slots(vec![None, None]);
    pool.set_session_store(Some(fixture.store.clone()));
    pool.discard_durable_scope(&scope);
    assert_eq!(
        new_reason(decision_now(&fixture, &scope)),
        SessionReason::Discarded
    );
}

#[test]
fn rotate_leaves_sibling_thread_resumable() {
    let fixture = Fixture::new();
    let (rotated, sibling) = (thread_scope(42, "a"), thread_scope(42, "b"));
    fixture.departed_record(&rotated, &ctx(), &[]);
    fixture.departed_record(&sibling, &ctx(), &[]);
    let mut pool = AgentPool::from_slots(vec![None]);
    pool.set_session_store(Some(fixture.store.clone()));
    pool.discard_durable_scope(&rotated);
    assert_eq!(
        resume_reason(decision_now(&fixture, &sibling)),
        SessionReason::IdleSleep
    );
}

#[test]
fn channel_removal_then_readd_starts_new_session() {
    let fixture = Fixture::new();
    let (a, b, other) = (
        thread_scope(42, "a"),
        thread_scope(42, "b"),
        thread_scope(43, "a"),
    );
    for scope in [&a, &b, &other] {
        fixture.departed_record(scope, &ctx(), &[]);
    }
    let mut pool = AgentPool::from_slots(vec![None]);
    pool.set_session_store(Some(fixture.store.clone()));
    pool.discard_durable_channel(Uuid::from_u128(42));
    for scope in [&a, &b] {
        assert_eq!(
            new_reason(decision_now(&fixture, scope)),
            SessionReason::Discarded
        );
    }
    assert_eq!(
        resume_reason(decision_now(&fixture, &other)),
        SessionReason::IdleSleep
    );
}

#[test]
fn late_commit_after_rotate_is_ignored() {
    let fixture = Fixture::new();
    let scope = thread_scope(42, "a");
    let (mut state, _) = owned_state(&fixture, &scope, "s-1");
    let mut pool = AgentPool::from_slots(vec![None]);
    pool.set_session_store(Some(fixture.store.clone()));
    pool.discard_durable_scope(&scope);
    state.turn_counts.insert(scope.clone(), 5);
    state.persist_turns(&scope);
    state.mark_scope_delivery_success(scope.clone(), true, ["late".to_owned()], []);
    assert_eq!(fixture.record(&scope).live, None);
    assert!(!state.leases.contains_key(&scope));
}

/// A prompt context whose relay answers every `/query` with `events`.
async fn relay_context(
    channel_id: Uuid,
    events: Vec<nostr::Event>,
) -> (PromptContext, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let body = serde_json::to_string(&events).unwrap();
    let server = tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut request = vec![0; 16 * 1024];
            let _ = socket.read(&mut request).await;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });
    let mut ctx = ctx();
    ctx.context_message_limit = 10;
    ctx.rest_client.base_url = base_url.clone();
    ctx.channel_info = ChannelInfoResolver::new(
        HashMap::from([(
            channel_id,
            crate::relay::ChannelInfo {
                name: "thread".into(),
                channel_type: "stream".into(),
                description: None,
            },
        )]),
        RestClient {
            http: reqwest::Client::new(),
            base_url,
            keys: ctx.agent_keys.clone(),
            auth_tag_json: None,
        },
    );
    (ctx, server)
}

#[tokio::test]
async fn cold_start_prompt_includes_first_mention() {
    let fixture = Fixture::new();
    let human = Keys::generate();
    let channel_id = Uuid::from_u128(42);
    let root = mention(&human, channel_id, None, "thread opener");
    let root_id = root.id.to_hex();
    let agent_keys = Keys::generate();
    let agent_hex = agent_keys.public_key().to_hex();
    let mut events = vec![root.clone()];
    for n in 0..30 {
        let mut tags = vec![
            Tag::parse(["h", channel_id.to_string().as_str()]).unwrap(),
            Tag::parse(["e", root_id.as_str(), "", "reply"]).unwrap(),
        ];
        let text = if n == 2 {
            tags.push(Tag::parse(["p", agent_hex.as_str()]).unwrap());
            "FIRST-MENTION-TASK".to_owned()
        } else {
            format!("chatter {n}")
        };
        events.push(
            EventBuilder::new(Kind::Custom(9), text)
                .tags(tags)
                .custom_created_at(nostr::Timestamp::from(2_000 + n))
                .sign_with_keys(&human)
                .unwrap(),
        );
    }
    let trigger = events.last().unwrap().clone();
    let (mut ctx, server) = relay_context(channel_id, events).await;
    ctx.agent_keys = agent_keys;
    ctx.context_message_limit = 12;
    let ctx = Arc::new(ctx);
    let scope = SessionScope::Thread {
        channel_id,
        root_event_id: root_id,
    };
    let prompts = fixture.dir.path().join("prompts.ndjson");
    let agent = fixture
        .agent(&[("PROMPTS", prompts.to_str().unwrap())])
        .await;
    let mut agent = run_turn(agent, &ctx, batch(&scope, trigger)).await;
    agent.acp.shutdown().await;
    server.abort();
    let prompt = std::fs::read_to_string(&prompts).unwrap();
    assert!(prompt.contains("FIRST-MENTION-TASK"));
    assert!(!prompt.contains("chatter 5"), "window stays bounded");
}

// ---- session marker ----

#[test]
fn marker_for_resumed_session() {
    let start = SessionStart::Resumed {
        reason: SessionReason::IdleSleep,
    };
    assert_eq!(
        format_session_marker(&start, None),
        r#"<session state="resumed" reason="idle_sleep"/>"#
    );
}

#[test]
fn marker_for_failed_resume() {
    let start = SessionStart::New {
        reason: SessionReason::ResumeFailed("not_found".into()),
    };
    assert_eq!(
        format_session_marker(&start, None),
        r#"<session state="new" reason="resume_failed:not_found"/>"#
    );
}

#[test]
fn marker_reports_first_mention_not_found() {
    let start = SessionStart::New {
        reason: SessionReason::FirstInThread,
    };
    assert_eq!(
        format_session_marker(&start, Some(&super::super::first_mention::FirstMention::NotFound)),
        r#"<session state="new" reason="first_in_thread" first_mention="not_found"/>"#
    );
    assert_eq!(
        format_session_marker(&start, Some(&super::super::first_mention::FirstMention::InRoot)),
        r#"<session state="new" reason="first_in_thread"/>"#
    );
}

#[test]
fn marker_precedes_thread_context() {
    let mut sections = vec![
        "<context-hints/>".to_owned(),
        "<thread-context included=\"1\">x</thread-context>".to_owned(),
        "<buzz-event/>".to_owned(),
    ];
    insert_session_marker(&mut sections, "<session/>".to_owned());
    assert_eq!(sections[1], "<session/>");
    let mut bare = vec!["<buzz-event/>".to_owned()];
    insert_session_marker(&mut bare, "<session/>".to_owned());
    assert_eq!(bare[0], "<session/>");
}

#[tokio::test]
async fn marker_present_once_after_start() {
    let fixture = Fixture::new();
    let human = Keys::generate();
    let channel_id = Uuid::from_u128(42);
    let root = mention(&human, channel_id, None, "first");
    let scope = SessionScope::Thread {
        channel_id,
        root_event_id: root.id.to_hex(),
    };
    let mut ctx = ctx();
    ctx.context_message_limit = 0;
    let ctx = Arc::new(ctx);
    let prompts = fixture.dir.path().join("prompts.ndjson");
    let agent = fixture
        .agent(&[("PROMPTS", prompts.to_str().unwrap())])
        .await;
    let agent = run_turn(agent, &ctx, batch(&scope, root.clone())).await;
    let follow_up = mention(&human, channel_id, Some(&root.id.to_hex()), "second");
    let mut agent = run_turn(agent, &ctx, batch(&scope, follow_up)).await;
    agent.acp.shutdown().await;
    let prompts = std::fs::read_to_string(&prompts).unwrap();
    let lines: Vec<&str> = prompts.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains(r#"<session state=\"new\" reason=\"first_in_thread\"/>"#));
    assert!(!lines[1].contains("<session state="));
}
