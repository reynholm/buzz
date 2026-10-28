//! Production-path tests: status published by `run_prompt_task`, routed by
//! the real ACP dispatcher, and retired by the existing lifecycle paths.

use super::super::{
    run_prompt_task, AgentPool, ChannelInfoResolver, FlushBatch, PromptContext, PromptOutcome,
    RestClient,
};
use super::*;
use crate::acp::{AcpClient, StopReason};
use crate::queue::ContextMessage;
use crate::session_store::{Departure, Expect, LiveSession, LoadOutcome, SessionStore};
use buzz_core::session_status::{
    canonical_identity, generation_lock_path, parse_snapshot, scope_file_name,
    SessionState as StatusState, StatusEnvelope, StatusReason, StatusScope,
};
use fs2::FileExt;
use nostr::{EventBuilder, Keys, Kind, Tag};
use serde_json::json;
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Fake ACP adapter. Logs methods to `$LOG` and session/new|resume params to
/// `$PARAMS`. Before answering each `session/prompt` it reads the JSON file
/// `$CONTROL` (`snapshot`, `probe`, `usage`, `delay`), records the snapshot it
/// sees and the state of the `probe` lock to `$SEEN`, then emits the listed
/// `usage_update`s (`SELF` names the prompted session).
const ADAPTER: &str = r#"
import fcntl, json, os, sys, time
log = open(os.environ['LOG'], 'a', buffering=1)
created = 0
def note(path, value):
    with open(path, 'a') as out:
        out.write(json.dumps(value) + '\n')
def probe(path):
    try:
        fd = os.open(path, os.O_RDONLY)
    except FileNotFoundError:
        return 'missing'
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        fcntl.flock(fd, fcntl.LOCK_UN)
        return 'free'
    except OSError:
        return 'held'
    finally:
        os.close(fd)
def control():
    try:
        with open(os.environ['CONTROL']) as f:
            return json.load(f)
    except FileNotFoundError:
        return {}
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
    elif method in ('session/new', 'session/resume'):
        note(os.environ['PARAMS'], {'method': method, 'params': req['params']})
        if method == 'session/new':
            created += 1
            reply['result'] = {'sessionId': os.environ.get('SID_PREFIX', 'new') + f'-{created}'}
        elif os.environ.get('RESUME_ERROR') == '1':
            reply['error'] = {'code': -32002, 'message': 'Resource not found'}
        else:
            reply['result'] = {}
    elif method == 'session/prompt':
        sid = req['params']['sessionId']
        ctl = control()
        seen = {'session': sid}
        if ctl.get('snapshot'):
            try:
                with open(ctl['snapshot']) as f:
                    seen['snapshot'] = f.read()
            except FileNotFoundError:
                seen['snapshot'] = None
        if ctl.get('probe'):
            seen['probe'] = probe(ctl['probe'])
        note(os.environ['SEEN'], seen)
        time.sleep(float(ctl.get('delay', 0)))
        for target, used, size in ctl.get('usage', []):
            update = {'sessionUpdate': 'usage_update', 'used': used, 'size': size}
            params = {'sessionId': sid if target == 'SELF' else target, 'update': update}
            print(json.dumps({'jsonrpc': '2.0', 'method': 'session/update', 'params': params}), flush=True)
        reply['result'] = {'stopReason': 'end_turn'}
    else:
        reply['result'] = {}
    print(json.dumps(reply), flush=True)
"#;

struct Harness {
    dir: tempfile::TempDir,
    store: Arc<SessionStore>,
    status: SessionStatusRuntime,
    agent_keys: Keys,
}

impl Harness {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let agent_keys = Keys::generate();
        let relay = "ws://127.0.0.1:3000";
        let agent = agent_keys.public_key().to_hex();
        let store = Arc::new(SessionStore::open(dir.path().join("store"), relay, &agent).unwrap());
        let status = SessionStatusRuntime::open(
            &dir.path().join("sessions"),
            canonical_identity(relay, &agent).unwrap(),
        )
        .unwrap();
        Self {
            dir,
            store,
            status,
            agent_keys,
        }
    }

    fn file(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn ctx(&self) -> PromptContext {
        let mut ctx = super::super::tests::make_prompt_context_no_owner();
        ctx.cwd = "/tmp".into();
        ctx.context_message_limit = 0;
        ctx.agent_keys = self.agent_keys.clone();
        ctx.session_status = Some(self.status.clone());
        ctx
    }

    fn control(&self, value: serde_json::Value) {
        std::fs::write(self.file("control.json"), value.to_string()).unwrap();
    }

    fn lines(&self, name: &str) -> Vec<serde_json::Value> {
        std::fs::read_to_string(self.file(name))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    /// Snapshot each `session/prompt` saw, in order.
    fn seen(&self) -> Vec<Option<StatusEnvelope>> {
        self.lines("seen.ndjson")
            .iter()
            .map(|seen| {
                seen["snapshot"]
                    .as_str()
                    .map(|text| parse_snapshot(text.as_bytes()).unwrap())
            })
            .collect()
    }

    fn methods(&self) -> Vec<String> {
        std::fs::read_to_string(self.file("methods.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    async fn agent(&self, index: usize, env: &[(&str, &str)]) -> OwnedAgent {
        let mut vars = vec![
            ("LOG", self.file("methods.log")),
            ("PARAMS", self.file("params.ndjson")),
            ("SEEN", self.file("seen.ndjson")),
            ("CONTROL", self.file("control.json")),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.display().to_string()))
        .collect::<Vec<_>>();
        vars.extend(env.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        let mut acp = AcpClient::spawn("python3", &["-c".into(), ADAPTER.into()], &vars, false)
            .await
            .unwrap();
        acp.initialize().await.unwrap();
        OwnedAgent {
            index,
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

    fn snapshot_path(&self, scope: &SessionScope) -> PathBuf {
        let scope = session_visibility_scope(scope);
        self.status.dir().join(scope_file_name(&scope).unwrap())
    }

    fn snapshot(&self, scope: &SessionScope) -> Option<StatusEnvelope> {
        let bytes = std::fs::read(self.snapshot_path(scope)).ok()?;
        Some(parse_snapshot(&bytes).unwrap())
    }

    fn generation_lock(&self, envelope: &StatusEnvelope) -> PathBuf {
        self.status
            .dir()
            .join(generation_lock_path(envelope.generation))
    }

    /// A resumable record for `scope`, as left by a worker that went to sleep.
    fn departed_record(&self, scope: &SessionScope, ctx: &PromptContext) {
        let live = LiveSession {
            lease: Uuid::nil(),
            adapter: "fake-acp".into(),
            session_id: "old-1".into(),
            cwd: ctx.cwd.clone(),
            model: None,
            instance: self.store.instance(),
            worker_index: 0,
            departed: None,
            turns: 0,
            standing_context_sent: false,
            delivered_event_ids: Vec::new(),
            hydrated_thread_roots: Vec::new(),
        };
        let lease = self.store.claim(scope, &Expect::Missing, live).unwrap();
        self.store
            .commit(scope, lease, |live| {
                live.departed = Some(Departure::IdleSleep);
                live.turns = 1;
            })
            .unwrap();
    }
}

fn session_visibility_scope(scope: &SessionScope) -> StatusScope {
    status_scope(scope).expect("valid status scope")
}

fn lock_is_held(path: &Path) -> bool {
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    match file.try_lock_exclusive() {
        Ok(()) => {
            FileExt::unlock(&file).unwrap();
            false
        }
        Err(_) => true,
    }
}

fn percent(envelope: &StatusEnvelope) -> Option<u8> {
    envelope.status.occupancy.and_then(|o| o.percent())
}

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

fn thread_of(channel_id: Uuid, root: &nostr::Event) -> SessionScope {
    SessionScope::Thread {
        channel_id,
        root_event_id: root.id.to_hex(),
    }
}

fn batch(scope: &SessionScope, event: nostr::Event) -> FlushBatch {
    FlushBatch {
        work: None,
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
    run_prompt_task(
        agent,
        Some(batch),
        None,
        ctx.clone(),
        result_tx,
        None,
        "turn".into(),
        crate::queue::PromptRouting::default(),
    )
    .await;
    let result = result_rx.recv().await.expect("prompt result");
    assert!(
        matches!(result.outcome, PromptOutcome::Ok(StopReason::EndTurn)),
        "turn did not end normally"
    );
    result.agent
}

/// A new thread session in channel 42 whose first turn ran on `agent`.
async fn first_turn(
    h: &Harness,
    ctx: &Arc<PromptContext>,
    agent: OwnedAgent,
    root_text: &str,
) -> (OwnedAgent, SessionScope, nostr::Event) {
    let human = Keys::generate();
    let channel_id = Uuid::from_u128(42);
    let root = mention(&human, channel_id, None, root_text);
    let scope = thread_of(channel_id, &root);
    h.control(json!({"snapshot": h.snapshot_path(&scope), "usage": [["SELF", 64, 100]]}));
    let agent = run_turn(agent, ctx, batch(&scope, root.clone())).await;
    (agent, scope, root)
}

fn follow_up(scope: &SessionScope, text: &str) -> FlushBatch {
    let root = scope.root_event_id().unwrap();
    batch(
        scope,
        mention(&Keys::generate(), scope.channel_id(), Some(root), text),
    )
}

// ---- state, reason and transition through run_prompt_task ----

#[tokio::test]
async fn new_session_status_precedes_the_prompt_and_spans_ordinary_turns() {
    let h = Harness::new();
    let ctx = Arc::new(h.ctx());
    let agent = h.agent(0, &[]).await;
    let (agent, scope, _) = first_turn(&h, &ctx, agent, "first").await;

    let seen = h.seen();
    let first = seen[0]
        .clone()
        .expect("snapshot exists before the provider prompt");
    assert_eq!(first.status.state, StatusState::New);
    assert_eq!(first.status.reason, StatusReason::FirstInThread);
    assert_eq!(first.status.occupancy, None);
    let history = first
        .status
        .history
        .expect("history is set before the prompt");
    assert_eq!((history.read, history.total), (Some(1), None));
    assert_eq!(percent(&h.snapshot(&scope).unwrap()), Some(64));

    // An ordinary turn keeps the transition.
    h.control(json!({"snapshot": h.snapshot_path(&scope), "usage": [["SELF", 66, 100]]}));
    let mut agent = run_turn(agent, &ctx, follow_up(&scope, "second")).await;
    let second = h.seen()[1].clone().unwrap();
    assert_eq!(second.status.transition_id, first.status.transition_id);
    assert_eq!(second.generation, first.generation);

    // A dropped routing entry is restored for the same provider session.
    let sid = agent.state.sessions[&scope].clone();
    agent.acp.clear_session_status(&sid);
    h.control(json!({"usage": [["SELF", 33, 100]]}));
    let mut agent = run_turn(agent, &ctx, follow_up(&scope, "third")).await;
    let current = h.snapshot(&scope).unwrap();
    assert_eq!(percent(&current), Some(33));
    assert_eq!(current.status.transition_id, first.status.transition_id);

    // The next acquisition changes the transition and never borrows 33%.
    agent.state.invalidate_scope(&scope);
    assert!(
        h.snapshot(&scope).is_none(),
        "invalidation retires the snapshot"
    );
    assert!(!lock_is_held(&h.generation_lock(&first)));
    h.control(json!({"snapshot": h.snapshot_path(&scope)}));
    let mut agent = run_turn(agent, &ctx, follow_up(&scope, "fourth")).await;
    agent.acp.shutdown().await;
    let next = h.seen().last().cloned().flatten().unwrap();
    assert_ne!(next.status.transition_id, first.status.transition_id);
    assert_eq!(next.status.state, StatusState::New);
    assert_eq!(next.status.reason, StatusReason::Discarded);
    assert_eq!(next.status.occupancy, None);
}

#[tokio::test]
async fn resumed_session_reports_idle_sleep() {
    let h = Harness::new();
    let ctx = Arc::new(h.ctx());
    let human = Keys::generate();
    let channel_id = Uuid::from_u128(42);
    let root = mention(&human, channel_id, None, "first");
    let scope = thread_of(channel_id, &root);
    h.departed_record(&scope, &ctx);
    h.control(json!({"snapshot": h.snapshot_path(&scope)}));
    let agent = h.agent(0, &[("RESUME_CAP", "1")]).await;
    let mut agent = run_turn(agent, &ctx, follow_up(&scope, "after sleep")).await;
    agent.acp.shutdown().await;
    let seen = h.seen()[0].clone().unwrap();
    assert_eq!(seen.status.state, StatusState::Resumed);
    assert_eq!(seen.status.reason, StatusReason::IdleSleep);
    assert_eq!(
        seen.status.history, None,
        "read counts describe new sessions only"
    );
}

#[tokio::test]
async fn failed_resume_reports_the_actual_fallback_reason() {
    let h = Harness::new();
    let ctx = Arc::new(h.ctx());
    let channel_id = Uuid::from_u128(42);
    let root = mention(&Keys::generate(), channel_id, None, "first");
    let scope = thread_of(channel_id, &root);
    h.departed_record(&scope, &ctx);
    h.control(json!({"snapshot": h.snapshot_path(&scope)}));
    let agent = h
        .agent(0, &[("RESUME_CAP", "1"), ("RESUME_ERROR", "1")])
        .await;
    let mut agent = run_turn(agent, &ctx, follow_up(&scope, "after sleep")).await;
    agent.acp.shutdown().await;
    let seen = h.seen()[0].clone().unwrap();
    assert_eq!(seen.status.state, StatusState::New);
    assert_eq!(
        seen.status.reason,
        StatusReason::ResumeFailed("not_found".into())
    );
}

#[test]
fn fallback_codes_outside_the_allowlist_become_unknown() {
    use crate::session_store::SessionReason;
    let wire = |code: &str| status_reason(&SessionReason::ResumeFailed(code.into())).as_wire();
    for known in [
        "not_persisted",
        "store_unreadable",
        "store_busy",
        "not_found",
        "timeout",
        "protocol",
        "rpc_-32603",
        "rpc_7",
        "rpc_-9223372036854775808",
    ] {
        assert_eq!(wire(known), format!("resume_failed:{known}"));
    }
    for leaked in [
        "Transcript /home/u/x.jsonl: EACCES",
        "transcript_eacces",
        "transcript__home_u_x_jsonl__eacces",
        "rpc_",
        "rpc_-",
        "rpc_12a",
        "rpc_--1",
        "rpc_12345678901234567890",
        "NOT_FOUND",
        "not_found ",
        "",
    ] {
        assert_eq!(wire(leaked), "resume_failed:unknown", "{leaked:?}");
    }
    assert_eq!(
        status_reason(&SessionReason::from(format!(
            "resume_failed:{}",
            "x".repeat(50)
        )))
        .as_wire(),
        "resume_failed:unknown",
        "an unrecognised stored wire string never reaches the tag"
    );
    assert_eq!(
        status_reason(&SessionReason::IdleSleep),
        StatusReason::IdleSleep
    );
}

// ---- usage routing on one worker ----

#[tokio::test]
async fn interleaved_sessions_on_one_worker_keep_their_own_occupancy() {
    let h = Harness::new();
    let ctx = Arc::new(h.ctx());
    let channel_id = Uuid::from_u128(42);
    let human = Keys::generate();
    let root_a = mention(&human, channel_id, None, "thread A");
    let root_b = mention(&human, channel_id, None, "thread B");
    let (a, b) = (
        thread_of(channel_id, &root_a),
        thread_of(channel_id, &root_b),
    );
    let agent = h.agent(0, &[]).await;

    h.control(json!({"usage": [["SELF", 64, 100]]}));
    let agent = run_turn(agent, &ctx, batch(&a, root_a)).await;
    h.control(json!({"usage": [["SELF", 81, 100]]}));
    let agent = run_turn(agent, &ctx, batch(&b, root_b)).await;
    assert_eq!(percent(&h.snapshot(&a).unwrap()), Some(64));
    assert_eq!(percent(&h.snapshot(&b).unwrap()), Some(81));

    // A late update of A during A's own turn never touches B.
    h.control(json!({"usage": [["SELF", 70, 100], ["unknown-session", 99, 100]]}));
    let agent = run_turn(agent, &ctx, follow_up(&a, "A again")).await;
    assert_eq!(percent(&h.snapshot(&a).unwrap()), Some(70));
    assert_eq!(percent(&h.snapshot(&b).unwrap()), Some(81));

    // A report for A's session during B's turn belongs to A, not to B.
    let sid_a = agent.state.sessions[&a].clone();
    h.control(json!({"usage": [[sid_a, 12, 100]]}));
    let mut agent = run_turn(agent, &ctx, follow_up(&b, "B again")).await;
    agent.acp.shutdown().await;
    assert_eq!(percent(&h.snapshot(&a).unwrap()), Some(12));
    assert_eq!(percent(&h.snapshot(&b).unwrap()), Some(81));
}

#[tokio::test]
async fn heartbeat_turn_never_touches_scope_status() {
    let h = Harness::new();
    let ctx = Arc::new(h.ctx());
    let agent = h.agent(0, &[]).await;
    let (agent, scope, _) = first_turn(&h, &ctx, agent, "first").await;
    let before = std::fs::read(h.snapshot_path(&scope)).unwrap();
    let files_before = std::fs::read_dir(h.status.dir()).unwrap().count();

    h.control(json!({"usage": [["SELF", 99, 100]]}));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    run_prompt_task(
        agent,
        None,
        Some("heartbeat".into()),
        ctx.clone(),
        tx,
        None,
        "heartbeat".into(),
        crate::queue::PromptRouting::default(),
    )
    .await;
    let result = rx.recv().await.unwrap();
    assert!(matches!(result.outcome, PromptOutcome::Ok(_)));
    let mut agent = result.agent;
    agent.acp.shutdown().await;
    assert_eq!(std::fs::read(h.snapshot_path(&scope)).unwrap(), before);
    assert_eq!(
        std::fs::read_dir(h.status.dir()).unwrap().count(),
        files_before
    );
}

// ---- lifecycle ----

#[tokio::test]
async fn rotate_during_a_delayed_turn_retires_before_late_usage() {
    let h = Harness::new();
    let ctx = Arc::new(h.ctx());
    let channel_id = Uuid::from_u128(42);
    let root = mention(&Keys::generate(), channel_id, None, "first");
    let scope = thread_of(channel_id, &root);
    h.control(json!({"delay": 0.8, "usage": [["SELF", 64, 100]]}));
    let agent = h.agent(0, &[]).await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let turn = tokio::spawn(run_prompt_task(
        agent,
        Some(batch(&scope, root)),
        None,
        ctx.clone(),
        tx,
        None,
        "turn".into(),
        crate::queue::PromptRouting::default(),
    ));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    while !h.methods().contains(&"session/prompt".to_owned()) {
        assert!(
            tokio::time::Instant::now() < deadline,
            "prompt never started: {:?}",
            h.methods()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let current = h.snapshot(&scope).expect("published before the prompt");
    retire_scope(ctx.session_status.as_ref(), &scope);
    turn.await.unwrap();
    let mut agent = rx.recv().await.unwrap().agent;
    agent.acp.shutdown().await;
    assert!(
        h.snapshot(&scope).is_none(),
        "late usage never recreates it"
    );
    assert!(!lock_is_held(&h.generation_lock(&current)));
}

#[tokio::test]
async fn channel_removal_retires_every_scope_of_that_channel() {
    let h = Harness::new();
    let ctx = Arc::new(h.ctx());
    let human = Keys::generate();
    let removed = Uuid::from_u128(42);
    let kept = Uuid::from_u128(43);
    let roots: Vec<_> = [(removed, "a"), (removed, "b"), (kept, "c")]
        .into_iter()
        .map(|(channel_id, text)| {
            let root = mention(&human, channel_id, None, text);
            (thread_of(channel_id, &root), root)
        })
        .collect();
    let scopes: Vec<_> = roots.iter().map(|(scope, _)| scope.clone()).collect();
    let mut agent = h.agent(0, &[]).await;
    for (scope, root) in roots {
        agent = run_turn(agent, &ctx, batch(&scope, root)).await;
    }
    agent.acp.shutdown().await;
    retire_channel(ctx.session_status.as_ref(), removed);
    assert!(h.snapshot(&scopes[0]).is_none());
    assert!(h.snapshot(&scopes[1]).is_none());
    assert!(h.snapshot(&scopes[2]).is_some());
}

#[tokio::test]
async fn pool_sleep_retires_status_and_wake_starts_a_new_transition() {
    let h = Harness::new();
    let ctx = Arc::new(h.ctx());
    let agent = h.agent(0, &[("RESUME_CAP", "1")]).await;
    let (agent, scope, _) = first_turn(&h, &ctx, agent, "first").await;
    let before = h.snapshot(&scope).unwrap();
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    pool.depart_idle_sessions(Departure::IdleSleep);
    assert!(h.snapshot(&scope).is_none());
    assert!(!lock_is_held(&h.generation_lock(&before)));
    let mut slept = pool.agents_mut()[0].take().unwrap();
    slept.acp.shutdown().await;

    h.control(json!({"snapshot": h.snapshot_path(&scope)}));
    let woken = h.agent(0, &[("RESUME_CAP", "1")]).await;
    let mut woken = run_turn(woken, &ctx, follow_up(&scope, "after sleep")).await;
    woken.acp.shutdown().await;
    let seen = h.seen().last().cloned().flatten().unwrap();
    assert_eq!(seen.status.state, StatusState::Resumed);
    assert_eq!(seen.status.reason, StatusReason::IdleSleep);
    assert_ne!(seen.status.transition_id, before.status.transition_id);
    assert_eq!(seen.status.occupancy, None);
}

#[tokio::test]
async fn worker_crash_retires_status_before_the_replacement_prompt() {
    let h = Harness::new();
    let ctx = Arc::new(h.ctx());
    let agent = h.agent(0, &[("RESUME_CAP", "1")]).await;
    let (mut agent, scope, _) = first_turn(&h, &ctx, agent, "first").await;
    let before = h.snapshot(&scope).unwrap();
    agent.state.invalidate_all();
    agent.acp.shutdown().await;
    assert!(h.snapshot(&scope).is_none());
    assert!(!lock_is_held(&h.generation_lock(&before)));

    h.control(json!({"snapshot": h.snapshot_path(&scope)}));
    let replacement = h.agent(0, &[("RESUME_CAP", "1")]).await;
    let mut replacement = run_turn(replacement, &ctx, follow_up(&scope, "again")).await;
    replacement.acp.shutdown().await;
    let seen = h.seen().last().cloned().flatten().unwrap();
    assert_eq!(seen.status.state, StatusState::Resumed);
    assert_eq!(seen.status.reason, StatusReason::WorkerCrash);
    assert_ne!(seen.status.transition_id, before.status.transition_id);
}

#[tokio::test]
async fn successor_acquisition_revokes_the_old_generation_before_its_prompt() {
    let h = Harness::new();
    let ctx = Arc::new(h.ctx());
    let first = h.agent(0, &[]).await;
    let (mut first, scope, _) = first_turn(&h, &ctx, first, "first").await;
    let old = h.snapshot(&scope).unwrap();

    h.control(json!({
        "snapshot": h.snapshot_path(&scope),
        "probe": h.generation_lock(&old),
        "usage": [["SELF", 20, 100]],
    }));
    let successor = h.agent(1, &[("SID_PREFIX", "other")]).await;
    let mut successor = run_turn(successor, &ctx, follow_up(&scope, "forked")).await;
    successor.acp.shutdown().await;
    let seen = h.lines("seen.ndjson");
    assert_ne!(
        seen.last().unwrap()["probe"],
        "held",
        "the predecessor generation is revoked before the successor prompt"
    );
    let new = h.snapshot(&scope).unwrap();
    assert_ne!(new.status.transition_id, old.status.transition_id);
    let successor_bytes = std::fs::read(h.snapshot_path(&scope)).unwrap();

    // Late callbacks and the stale owner's invalidation leave the successor alone.
    first.state.status[&scope].record_occupancy(99, 100);
    first.state.invalidate_scope(&scope);
    first.acp.shutdown().await;
    assert_eq!(
        std::fs::read(h.snapshot_path(&scope)).unwrap(),
        successor_bytes
    );
    assert!(lock_is_held(&h.generation_lock(&new)));
}

#[tokio::test]
async fn status_write_failure_never_fails_the_turn() {
    let h = Harness::new();
    let ctx = Arc::new(h.ctx());
    let channel_id = Uuid::from_u128(42);
    let root = mention(&Keys::generate(), channel_id, None, "first");
    let scope = thread_of(channel_id, &root);
    let dir = h.status.dir().to_path_buf();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
    let agent = h.agent(0, &[]).await;
    let mut agent = run_turn(agent, &ctx, batch(&scope, root)).await;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    agent.acp.shutdown().await;
    assert!(h.snapshot(&scope).is_none());
    let LoadOutcome::Found(record) = h.store.load(&scope) else {
        panic!("durable record missing");
    };
    assert_eq!(record.live.unwrap().session_id, "new-1");
}

// ---- final-context history coverage ----

#[derive(Clone, Default)]
struct RelayScript {
    events: Vec<nostr::Event>,
    fail_history: bool,
    fail_scan: bool,
}

struct Relay {
    history_requests: Arc<AtomicUsize>,
    server: tokio::task::JoinHandle<()>,
}

async fn read_request(socket: &mut tokio::net::TcpStream) -> Option<(String, String)> {
    use tokio::io::AsyncReadExt;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let n = socket.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
        };
        let head = String::from_utf8_lossy(&buf[..end]).to_string();
        let length = head
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())?
            })
            .unwrap_or(0);
        while buf.len() < end + 4 + length {
            let n = socket.read(&mut chunk).await.ok()?;
            if n == 0 {
                return None;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        let path = head.split_whitespace().nth(1)?.to_owned();
        let body = String::from_utf8_lossy(&buf[end + 4..end + 4 + length]).to_string();
        return Some((path, body));
    }
}

async fn relay(script: RelayScript, ctx: &mut PromptContext, channel_id: Uuid) -> Relay {
    use tokio::io::AsyncWriteExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let history_requests = Arc::new(AtomicUsize::new(0));
    let counter = history_requests.clone();
    let server = tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let Some((path, body)) = read_request(&mut socket).await else {
                continue;
            };
            // Working/reaction cleanup is fire-and-forget and may arrive after
            // the turn returns. Count history reads, not those cosmetic calls
            // or unrelated channel/profile metadata lookups.
            let filters: Vec<serde_json::Value> = serde_json::from_str(&body).unwrap_or_default();
            let reads_history = path == "/query"
                && filters.iter().any(|filter| {
                    filter.get("ids").is_some()
                        || filter["kinds"].as_array().is_some_and(|kinds| {
                            kinds
                                .iter()
                                .any(|kind| matches!(kind.as_u64(), Some(9 | 40002)))
                        })
                });
            if reads_history || path == "/count" {
                counter.fetch_add(1, Ordering::SeqCst);
            }
            let scan = body.contains("depth_limit");
            let history = path == "/query" && body.contains("\"#e\"") && !scan;
            let (status, body) = if (scan && script.fail_scan) || (history && script.fail_history) {
                ("400 Bad Request", "{}".to_owned())
            } else if path == "/count" {
                let replies = script.events.len().saturating_sub(1);
                ("200 OK", json!({ "count": replies }).to_string())
            } else {
                ("200 OK", serde_json::to_string(&script.events).unwrap())
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });
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
    Relay {
        history_requests,
        server,
    }
}

fn reply(
    keys: &Keys,
    channel_id: Uuid,
    root_id: &str,
    at: u64,
    text: &str,
    mention: Option<&Keys>,
) -> nostr::Event {
    let mut tags = vec![
        Tag::parse(["h", channel_id.to_string().as_str()]).unwrap(),
        Tag::parse(["e", root_id, "", "reply"]).unwrap(),
    ];
    if let Some(agent) = mention {
        tags.push(Tag::parse(["p", agent.public_key().to_hex().as_str()]).unwrap());
    }
    EventBuilder::new(Kind::Custom(9), text)
        .tags(tags)
        .custom_created_at(nostr::Timestamp::from(at))
        .sign_with_keys(keys)
        .unwrap()
}

fn root_event(keys: &Keys, channel_id: Uuid, mention: Option<&Keys>) -> nostr::Event {
    let mut tags = vec![Tag::parse(["h", channel_id.to_string().as_str()]).unwrap()];
    if let Some(agent) = mention {
        tags.push(Tag::parse(["p", agent.public_key().to_hex().as_str()]).unwrap());
    }
    EventBuilder::new(Kind::Custom(9), "thread opener")
        .tags(tags)
        .custom_created_at(nostr::Timestamp::from(1_000))
        .sign_with_keys(keys)
        .unwrap()
}

/// Run one cold-start turn of `trigger` against a scripted relay and return
/// the history coverage the provider prompt saw plus the history request count.
async fn cold_start(
    status: bool,
    limit: u32,
    script: impl FnOnce(&Keys) -> (RelayScript, nostr::Event, nostr::Event),
) -> (Option<HistoryCoverage>, usize, String) {
    let h = Harness::new();
    let (script, root, trigger) = script(&h.agent_keys);
    let channel_id = Uuid::from_u128(42);
    let scope = thread_of(channel_id, &root);
    let mut ctx = h.ctx();
    ctx.context_message_limit = limit;
    if !status {
        ctx.session_status = None;
    }
    let relay = relay(script, &mut ctx, channel_id).await;
    h.control(json!({"snapshot": h.snapshot_path(&scope)}));
    let agent = h.agent(0, &[]).await;
    let mut agent = run_turn(agent, &Arc::new(ctx), batch(&scope, trigger)).await;
    agent.acp.shutdown().await;
    relay.server.abort();
    let seen = h.lines("seen.ndjson");
    let prompt_snapshot = seen[0]["snapshot"].as_str().map(str::to_owned);
    let history = prompt_snapshot
        .map(|text| parse_snapshot(text.as_bytes()).unwrap())
        .and_then(|envelope| envelope.status.history);
    let methods = h.methods().join(",");
    (
        history,
        relay.history_requests.load(Ordering::SeqCst),
        methods,
    )
}

fn coverage(
    read: Option<u64>,
    total: Option<u64>,
    exact: bool,
    incomplete: bool,
) -> HistoryCoverage {
    HistoryCoverage {
        read,
        total,
        exact,
        incomplete,
    }
}

/// Root, assignment, the agent's previous answer and a new trigger, with a
/// one-message window: the assignment is pinned and the answer evicted.
fn pinned_assignment(agent: &Keys) -> (RelayScript, nostr::Event, nostr::Event) {
    let human = Keys::generate();
    let channel_id = Uuid::from_u128(42);
    let root = root_event(&human, channel_id, None);
    let root_id = root.id.to_hex();
    let assignment = reply(
        &human,
        channel_id,
        &root_id,
        2_000,
        "ASSIGNMENT",
        Some(agent),
    );
    let answer = reply(agent, channel_id, &root_id, 2_001, "ANSWER", None);
    let trigger = reply(&human, channel_id, &root_id, 2_002, "continue", Some(agent));
    let script = RelayScript {
        events: vec![root.clone(), assignment, answer, trigger.clone()],
        ..Default::default()
    };
    (script, root, trigger)
}

#[tokio::test]
async fn pinned_assignment_outside_the_window_counts_supplied_events_once() {
    let (history, _, _) = cold_start(true, 1, pinned_assignment).await;
    assert_eq!(history, Some(coverage(Some(3), Some(4), false, true)));
}

#[tokio::test]
async fn counts_need_no_extra_history_walk() {
    let (_, with_status, _) = cold_start(true, 1, pinned_assignment).await;
    let (history, without_status, _) = cold_start(false, 1, pinned_assignment).await;
    assert_eq!(history, None);
    // One batched context query, one count and one assignment scan.
    assert_eq!(with_status, 3);
    assert_eq!(with_status, without_status);
}

#[tokio::test]
async fn root_equal_to_trigger_is_an_exact_single_message() {
    let (history, _, _) = cold_start(true, 12, |agent| {
        let root = root_event(&Keys::generate(), Uuid::from_u128(42), Some(agent));
        let script = RelayScript {
            events: vec![root.clone()],
            ..Default::default()
        };
        (script, root.clone(), root)
    })
    .await;
    assert_eq!(history, Some(coverage(Some(1), Some(1), true, false)));
}

fn root_assignment(agent: &Keys, fail_scan: bool) -> (RelayScript, nostr::Event, nostr::Event) {
    let human = Keys::generate();
    let channel_id = Uuid::from_u128(42);
    let root = root_event(&human, channel_id, (!fail_scan).then_some(agent));
    let root_id = root.id.to_hex();
    let chatter = reply(&human, channel_id, &root_id, 2_000, "chatter", None);
    let trigger = reply(&human, channel_id, &root_id, 2_001, "continue", Some(agent));
    let script = RelayScript {
        events: vec![root.clone(), chatter, trigger.clone()],
        fail_scan,
        ..Default::default()
    };
    (script, root, trigger)
}

#[tokio::test]
async fn root_that_is_the_assignment_is_counted_once() {
    let (history, _, _) = cold_start(true, 12, |agent| root_assignment(agent, false)).await;
    assert_eq!(history, Some(coverage(Some(3), Some(3), true, false)));
}

#[tokio::test]
async fn failed_first_mention_scan_is_never_exact() {
    let (history, _, _) = cold_start(true, 12, |agent| root_assignment(agent, true)).await;
    assert_eq!(history, Some(coverage(Some(3), Some(3), false, true)));
}

#[tokio::test]
async fn missing_history_response_reports_possible_omission() {
    let (history, _, _) = cold_start(true, 12, |agent| {
        let (mut script, root, trigger) = root_assignment(agent, false);
        script.fail_history = true;
        (script, root, trigger)
    })
    .await;
    assert_eq!(history, Some(coverage(Some(1), None, false, true)));
}

fn message(id: &str) -> ContextMessage {
    ContextMessage {
        event_id: id.to_owned(),
        pubkey: "p".into(),
        timestamp: "t".into(),
        content: "c".into(),
    }
}

fn thread_context(
    ids: &[&str],
    total: usize,
    root_present: bool,
    truncated: bool,
) -> ConversationContext {
    ConversationContext::Thread {
        messages: ids.iter().copied().map(message).collect(),
        total,
        root_present,
        truncated,
    }
}

fn triggers(ids: &[&str]) -> std::collections::HashSet<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

fn thread_scope() -> SessionScope {
    SessionScope::Thread {
        channel_id: Uuid::from_u128(1),
        root_event_id: "a".repeat(64),
    }
}

fn conversation_scope() -> SessionScope {
    SessionScope::Conversation {
        channel_id: Uuid::from_u128(1),
    }
}

#[test]
fn legacy_messages_without_ids_omit_the_read_count() {
    let fetched = thread_context(&["r", ""], 2, true, false);
    let captured = FetchedHistory::capture(true, Some(&fetched));
    let history = history_coverage(
        &captured,
        Some(&fetched),
        &triggers(&["t"]),
        false,
        &thread_scope(),
        false,
    );
    assert_eq!(history.read, None, "never invent identities");
    assert!(!history.exact);
}

#[test]
fn read_counts_unique_supplied_ids_not_the_fetch_window() {
    // A full window from a complete-looking fetch is still only a lower bound.
    let fetched = thread_context(&["r", "a", "b", "t"], 9, true, true);
    let supplied = thread_context(&["r", "a", "b"], 9, true, true);
    let history = history_coverage(
        &FetchedHistory::capture(true, Some(&fetched)),
        Some(&supplied),
        &triggers(&["t", "a"]),
        false,
        &thread_scope(),
        false,
    );
    assert_eq!(history, coverage(Some(4), Some(9), false, true));
}

#[test]
fn delivered_context_omitted_from_the_prompt_is_not_read() {
    let fetched = thread_context(&["r", "old", "t"], 3, true, false);
    let supplied = thread_context(&["r"], 3, true, true);
    let history = history_coverage(
        &FetchedHistory::capture(true, Some(&fetched)),
        Some(&supplied),
        &triggers(&["t"]),
        false,
        &thread_scope(),
        false,
    );
    assert_eq!(history, coverage(Some(2), Some(3), false, true));
}

#[test]
fn dm_history_is_exact_only_when_untruncated() {
    let dm = |truncated| ConversationContext::Dm {
        messages: vec![message("a"), message("t")],
        total: if truncated { 3 } else { 2 },
        truncated,
    };
    let supplied = ConversationContext::Dm {
        messages: vec![message("a")],
        total: 2,
        truncated: false,
    };
    let exact = history_coverage(
        &FetchedHistory::capture(true, Some(&dm(false))),
        Some(&supplied),
        &triggers(&["t"]),
        false,
        &conversation_scope(),
        true,
    );
    assert_eq!(exact, coverage(Some(2), Some(2), true, false));
    let bound = history_coverage(
        &FetchedHistory::capture(true, Some(&dm(true))),
        Some(&supplied),
        &triggers(&["t"]),
        false,
        &conversation_scope(),
        true,
    );
    assert_eq!(bound, coverage(Some(2), Some(3), false, true));
}

#[test]
fn a_thread_total_never_describes_a_channel_conversation() {
    let fetched = thread_context(&["r", "t"], 2, true, false);
    let history = history_coverage(
        &FetchedHistory::capture(true, Some(&fetched)),
        Some(&thread_context(&["r"], 2, true, false)),
        &triggers(&["t"]),
        false,
        &conversation_scope(),
        false,
    );
    assert_eq!(history, coverage(Some(2), None, false, false));
}

#[test]
fn triggers_outside_the_fetched_context_drop_the_total() {
    let fetched = thread_context(&["r"], 1, true, false);
    let history = history_coverage(
        &FetchedHistory::capture(true, Some(&fetched)),
        Some(&fetched),
        &triggers(&["t1", "t2"]),
        false,
        &thread_scope(),
        false,
    );
    assert_eq!(history.total, None, "read may never exceed a total");
    assert!(!history.exact);
}

#[test]
fn unrequested_history_makes_no_completeness_claim() {
    let history = history_coverage(
        &FetchedHistory::capture(false, None),
        None,
        &triggers(&["t"]),
        false,
        &thread_scope(),
        false,
    );
    assert_eq!(history, coverage(Some(1), None, false, false));
}

// ---- environment wiring ----

/// A minimal ACP adapter that records the status directory it was given.
fn probe_adapter(dir: &Path) -> (PathBuf, PathBuf) {
    let log = dir.join("probe.log");
    let script = dir.join("probe-acp");
    std::fs::write(
        &script,
        format!(
            "#!/usr/bin/env python3\n\
             import json, os, sys\n\
             open({log:?}, 'a').write(os.environ.get('{env}', '<unset>') + '\\n')\n\
             for line in sys.stdin:\n\
             \x20   req = json.loads(line)\n\
             \x20   if 'id' in req:\n\
             \x20       print(json.dumps({{'jsonrpc': '2.0', 'id': req['id'], 'result': {{'protocolVersion': 1}}}}), flush=True)\n",
            log = log.display().to_string(),
            env = crate::session_status::SESSION_STATUS_DIR_ENV,
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    (script, log)
}

#[tokio::test]
async fn every_spawn_path_and_managed_mcp_get_the_current_status_dir() {
    use crate::session_status::SESSION_STATUS_DIR_ENV;
    let root = tempfile::tempdir().unwrap();
    let (script, log) = probe_adapter(root.path());
    let mut config = crate::build_mcp_servers_tests::test_config();
    config.session_store_dir = Some(root.path().join("sessions"));
    config.agent_command = script.display().to_string();
    config.agent_args = vec![];
    let mut runtime = crate::runtime::AgentRuntime::prepare(config).unwrap();
    let status = runtime.enable_session_status().expect("status runtime");
    let dir = status.dir().display().to_string();

    // Lazy start and idle rebuild initialize the pool from the startup.
    let mut pool = crate::initialize_agent_pool(&runtime.startup(None), None)
        .await
        .unwrap();
    for slot in pool.agents_mut() {
        if let Some(mut agent) = slot.take() {
            agent.acp.shutdown().await;
        }
    }
    // Refill and crash respawn spawn from the configured environment.
    let config = runtime.config();
    let (mut respawned, _, _) = crate::spawn_and_init(
        &config.agent_command,
        &config.agent_args,
        &config.persona_env_vars,
        config.has_generated_codex_config,
        1,
        None,
    )
    .await
    .unwrap();
    respawned.shutdown().await;
    let seen: Vec<String> = std::fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(seen, [dir.clone(), dir.clone()]);

    // The managed MCP server receives it explicitly, on new and resumed sessions.
    let rest = RestClient {
        http: reqwest::Client::new(),
        base_url: "http://127.0.0.1:0".into(),
        keys: config.keys.clone(),
        auth_tag_json: None,
    };
    let prompt_ctx = runtime
        .prompt_context(
            rest,
            HashMap::new(),
            crate::runtime::SessionMode::Conversation,
        )
        .unwrap();
    assert_eq!(
        prompt_ctx
            .session_status
            .as_ref()
            .map(|s| s.dir().to_path_buf()),
        Some(status.dir().to_path_buf())
    );
    let h = Harness::new();
    let mut ctx = h.ctx();
    ctx.mcp_servers = prompt_ctx.mcp_servers.clone();
    let ctx = Arc::new(ctx);
    let channel_id = Uuid::from_u128(42);
    let root_event = mention(&Keys::generate(), channel_id, None, "first");
    let scope = thread_of(channel_id, &root_event);
    let agent = h.agent(0, &[("RESUME_CAP", "1")]).await;
    let mut agent = run_turn(agent, &ctx, batch(&scope, root_event)).await;
    agent.state.depart_all(Departure::IdleSleep);
    agent.acp.shutdown().await;
    let woken = h.agent(0, &[("RESUME_CAP", "1")]).await;
    let mut woken = run_turn(woken, &ctx, follow_up(&scope, "again")).await;
    woken.acp.shutdown().await;
    let calls = h.lines("params.ndjson");
    let methods: Vec<_> = calls
        .iter()
        .map(|c| c["method"].as_str().unwrap())
        .collect();
    assert_eq!(methods, ["session/new", "session/resume"]);
    for call in &calls {
        let env = call["params"]["mcpServers"][0]["env"].as_array().unwrap();
        let forwarded: Vec<_> = env
            .iter()
            .filter(|entry| entry["name"] == SESSION_STATUS_DIR_ENV)
            .map(|entry| entry["value"].as_str().unwrap())
            .collect();
        assert_eq!(forwarded, [dir.as_str()], "{}", call["method"]);
    }
}

#[path = "session_visibility_policy_tests.rs"]
mod policy;
