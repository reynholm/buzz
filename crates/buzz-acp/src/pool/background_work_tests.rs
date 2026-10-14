use super::*;
use crate::queue::{EventQueue, QueuedEvent};
use nostr::{EventBuilder, Keys, Kind};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn worker(index: usize) -> OwnedAgent {
    OwnedAgent {
        index,
        acp: AcpClient::spawn("cat", &[], &[], false).await.unwrap(),
        state: SessionState::default(),
        model_capabilities: None,
        desired_model: None,
        model_overridden: false,
        desired_model_request_id: None,
        desired_model_pending_ack: false,
        startup_effort: None,
        agent_name: "background-test".into(),
        goose_system_prompt_supported: None,
        protocol_version: 2,
        background_context: None,
    }
}

fn scope() -> SessionScope {
    SessionScope::Thread {
        channel_id: Uuid::new_v4(),
        root_event_id: "a".repeat(64),
    }
}

fn bind(pool: &mut AgentPool, scope: &SessionScope, index: usize) {
    let generation = pool.record_scope_owner(scope.clone(), index);
    let agent = pool.agents[index].as_mut().unwrap();
    agent
        .state
        .sessions
        .insert(scope.clone(), format!("session-{scope:?}"));
    agent
        .state
        .set_scope_owner_generation(scope.clone(), generation);
}

#[tokio::test]
async fn background_hold_cannot_be_claimed_by_another_cached_session() {
    let a = scope();
    let b = scope();
    let mut pool = AgentPool::from_slots(vec![Some(worker(0).await)]);
    bind(&mut pool, &a, 0);
    bind(&mut pool, &b, 0);
    pool.set_runtime_hold(
        0,
        RuntimeHold {
            scope: a.clone(),
            blocking_pids: HashSet::from([123]),
            continuation: None,
            collection_pending: false,
        },
    );
    assert!(
        pool.try_claim(Some(&b)).is_none(),
        "B must not steal A's background work"
    );
    assert!(
        pool.try_claim(Some(&a)).is_some(),
        "A's real message supersedes its continuation"
    );
}

#[tokio::test(start_paused = true)]
async fn background_hold_does_not_arm_an_unserviceable_fork_deadline() {
    let a = scope();
    let b = scope();
    let mut pool = AgentPool::from_slots(vec![Some(worker(0).await), Some(worker(1).await)]);
    bind(&mut pool, &a, 0);
    bind(&mut pool, &b, 1);
    let busy = pool.try_claim(Some(&a)).unwrap();
    let handle = pool.join_set.spawn(std::future::pending());
    pool.task_map.insert(
        handle.id(),
        TaskMeta {
            agent_index: 0,
            channel_id: Some(Uuid::new_v4()),
            scope: Some(scope()),
            turn_id: "busy-on-another-scope".into(),
            recoverable_batch: None,
            control_tx: None,
            steer_tx: None,
            successful_steer_deliveries: HashSet::new(),
        },
    );
    pool.set_runtime_hold(
        1,
        RuntimeHold {
            scope: b,
            blocking_pids: HashSet::from([123]),
            continuation: None,
            collection_pending: false,
        },
    );
    assert!(matches!(
        pool.hold_decision(&a, tokio::time::Instant::now(), HOLD_BUSY_OWNER_TIMEOUT),
        HoldDecision::Hold { .. }
    ));
    tokio::time::advance(HOLD_BUSY_OWNER_TIMEOUT).await;
    assert!(pool.try_claim(Some(&a)).is_none());
    assert_eq!(
        pool.next_hold_deadline(HOLD_BUSY_OWNER_TIMEOUT),
        None,
        "a past deadline must not spin while the only idle slot is held"
    );
    drop(busy);
}

// The child exits only when the test creates a marker. This exercises actual
// process enumeration without scheduler-dependent sleep durations.
const ADAPTER: &str = r#"
import json, os, subprocess, sys, threading, time
child = None
cancelled = threading.Event()
write_lock = threading.Lock()
def respond(req, result=None, error=None):
    reply = {'jsonrpc': '2.0', 'id': req['id']}
    reply['error' if error else 'result'] = error if error else result
    with write_lock:
        print(json.dumps(reply), flush=True)
def prompt(req):
    text = str(req['params']['prompt'])
    collecting = 'background child process' in text
    if collecting:
        open(os.path.join(os.path.dirname(sys.argv[1]), 'collection-started'), 'w').close()
        while os.path.exists(os.path.join(os.path.dirname(sys.argv[1]), 'block-collection')) and not cancelled.is_set():
            time.sleep(.01)
        if os.path.exists(os.path.join(os.path.dirname(sys.argv[1]), 'fail-collection')):
            respond(req, error={'code': -32000, 'message': 'controlled collection failure'})
            return
    if collecting and cancelled.is_set():
        open(os.path.join(os.path.dirname(sys.argv[1]), 'cancel-received'), 'w').close()
        while os.path.exists(os.path.join(os.path.dirname(sys.argv[1]), 'hold-cancel-ack')):
            time.sleep(.01)
    next_group = os.path.join(os.path.dirname(sys.argv[1]), 'next-group')
    if collecting and os.path.exists(next_group):
        os.remove(next_group)
        program = 'import os,sys,time\nwhile not os.path.exists(sys.argv[1]): time.sleep(.01)'
        subprocess.Popen([sys.executable, '-c', program, os.path.join(os.path.dirname(sys.argv[1]), 'finish-next')])
    respond(req, {'stopReason': 'cancelled' if cancelled.is_set() else 'end_turn'})
for line in sys.stdin:
    req = json.loads(line)
    with open(sys.argv[2], 'a') as log:
        log.write(line)
    if req['method'] == 'initialize':
        respond(req, {'protocolVersion': 1, 'agentCapabilities': {}})
    elif req['method'] == 'session/new':
        respond(req, {'sessionId': 'original-session'})
    elif req['method'] == 'session/prompt':
        if child is None:
            program = 'import os,sys,time\nwhile not os.path.exists(sys.argv[1]): time.sleep(.01)'
            child = subprocess.Popen([sys.executable, '-c', program, sys.argv[1]])
        cancelled.clear()
        threading.Thread(target=prompt, args=(req,)).start()
    elif req['method'] == 'session/cancel':
        cancelled.set()
    elif 'id' in req:
        respond(req, {})
"#;

pub(crate) async fn started_background_turn(
    max_turns: u32,
) -> (
    AgentPool,
    EventQueue,
    SessionScope,
    tempfile::TempDir,
    Arc<PromptContext>,
) {
    started_background_turn_in_scope(
        max_turns,
        SessionScope::Conversation {
            channel_id: Uuid::new_v4(),
        },
        false,
    )
    .await
}

async fn started_background_turn_in_scope(
    max_turns: u32,
    scope: SessionScope,
    dm: bool,
) -> (
    AgentPool,
    EventQueue,
    SessionScope,
    tempfile::TempDir,
    Arc<PromptContext>,
) {
    let dir = tempfile::tempdir().unwrap();
    let mut agent = worker(0).await;
    agent.acp = AcpClient::spawn(
        "python3",
        &[
            "-u".into(),
            "-c".into(),
            ADAPTER.into(),
            dir.path().join("finish").to_string_lossy().into(),
            dir.path().join("requests").to_string_lossy().into(),
        ],
        &[],
        false,
    )
    .await
    .unwrap();
    agent.acp.initialize().await.unwrap();
    let mut pool = AgentPool::from_slots(vec![Some(agent), Some(worker(1).await)]);
    let event = EventBuilder::new(Kind::Custom(9), "start background work")
        .sign_with_keys(&Keys::generate())
        .unwrap();
    let mut queue = EventQueue::new(DedupMode::Queue);
    queue.push(QueuedEvent {
        channel_id: scope.channel_id(),
        scope: scope.clone(),
        event,
        prompt_tag: "test".into(),
        received_at: std::time::Instant::now(),
        edit: None,
    });
    let mut ctx = tests::make_prompt_context_no_owner();
    // Deterministic local relay rejection: no network retries or credentials.
    // Missing optional channel/profile metadata is allowed; an authoritative
    // project lookup on known metadata will exercise the failure path below.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    ctx.rest_client.base_url = format!("http://{}", listener.local_addr().unwrap());
    ctx.channel_info = ChannelInfoResolver::new(HashMap::new(), ctx.rest_client.clone());
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut request = [0; 8192];
            let _ = socket.read(&mut request).await;
            let _ = socket
                .write_all(
                    b"HTTP/1.1 400 Bad Request\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                )
                .await;
        }
    });
    if dm {
        ctx.channel_info.cache.write().unwrap().insert(
            scope.channel_id(),
            PromptChannelInfo {
                name: "DM".into(),
                channel_type: "dm".into(),
                description: None,
                project: None,
            },
        );
        ctx.channel_info.projects.write().unwrap().insert(
            scope.channel_id(),
            CachedProjectInfo {
                value: None,
                fetched_at: std::time::Instant::now(),
            },
        );
    }
    ctx.max_turns_per_session = max_turns;
    let ctx = Arc::new(ctx);
    assert_eq!(
        crate::dispatch_pending(
            &mut pool,
            &mut queue,
            &ctx,
            &mut tokio::time::Instant::now(),
            None
        )
        .len(),
        1
    );
    let result = timeout(Duration::from_secs(10), pool.result_rx.recv())
        .await
        .unwrap_or_else(|error| {
            panic!(
                "prompt timed out: {error}; ACP requests: {}",
                std::fs::read_to_string(dir.path().join("requests")).unwrap_or_default()
            )
        })
        .unwrap();
    assert!(matches!(
        result.outcome,
        PromptOutcome::Ok(StopReason::EndTurn)
    ));
    pool.task_map.clear();
    queue.mark_complete(&scope);
    pool.return_agent(result.agent);
    assert!(pool.has_runtime_hold());
    (pool, queue, scope, dir, ctx)
}

pub(crate) async fn finish_child(
    pool: &mut AgentPool,
    queue: &mut EventQueue,
    dir: &tempfile::TempDir,
) {
    std::fs::write(dir.path().join("finish"), "done").unwrap();
    timeout(Duration::from_secs(5), async {
        loop {
            pool.queue_finished_background_work(queue);
            if pool
                .runtime_holds
                .get(&0)
                .is_none_or(|hold| hold.blocking_pids.is_empty())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn background_completion_uses_dispatch_queue_and_original_session_before_rotation() {
    let (mut pool, mut queue, scope, dir, ctx) = started_background_turn(1).await;
    assert!(pool.agents[0]
        .as_ref()
        .unwrap()
        .state
        .background_rotation_pending
        .contains(&scope));
    assert_eq!(
        pool.agents[0].as_ref().unwrap().state.sessions[&scope],
        "original-session"
    );
    let thread = pool.runtime_holds[&0]
        .continuation
        .as_ref()
        .unwrap()
        .thread_tags
        .root_event_id
        .clone()
        .unwrap();
    finish_child(&mut pool, &mut queue, &dir).await;
    assert_eq!(
        crate::dispatch_pending(
            &mut pool,
            &mut queue,
            &ctx,
            &mut tokio::time::Instant::now(),
            None
        )
        .len(),
        1
    );
    assert!(queue.is_scope_in_flight(&scope));
    assert_eq!(queue.in_flight_reply_thread(&scope), Some(thread.as_str()));
    let meta = pool.task_map.values().next().unwrap();
    assert!(meta
        .recoverable_batch
        .as_ref()
        .unwrap()
        .continuation
        .is_some());
    assert!(meta.steer_tx.is_some());
    let event = EventBuilder::new(Kind::Custom(9), "new message")
        .sign_with_keys(&Keys::generate())
        .unwrap();
    queue.push(QueuedEvent {
        channel_id: scope.channel_id(),
        scope: scope.clone(),
        event,
        prompt_tag: "test".into(),
        received_at: std::time::Instant::now(),
        edit: None,
    });
    assert!(
        crate::dispatch_pending(
            &mut pool,
            &mut queue,
            &ctx,
            &mut tokio::time::Instant::now(),
            None
        )
        .is_empty(),
        "second worker cannot run the same conversation concurrently"
    );
    let result = timeout(Duration::from_secs(10), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        result.outcome,
        PromptOutcome::Ok(StopReason::EndTurn)
    ));
    assert!(
        !result.agent.state.sessions.contains_key(&scope),
        "rotation follows collection"
    );
    let requests: Vec<serde_json::Value> = std::fs::read_to_string(dir.path().join("requests"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["method"] == "session/new")
            .count(),
        1
    );
    let prompt = requests
        .iter()
        .rev()
        .find(|r| r["method"] == "session/prompt")
        .unwrap();
    assert_eq!(prompt["params"]["sessionId"], "original-session");
    assert!(prompt["params"]["prompt"][0]["text"]
        .as_str()
        .unwrap()
        .contains(&thread));
}

#[tokio::test]
async fn explicit_background_rotation_keeps_live_child_but_never_resumes_stale_session() {
    let (mut pool, mut queue, scope, dir, _) = started_background_turn(0).await;
    assert_eq!(pool.invalidate_scope_session(&scope), 1);
    assert!(
        pool.has_runtime_hold(),
        "rotation must not kill existing work"
    );
    finish_child(&mut pool, &mut queue, &dir).await;
    assert!(
        !queue.has_undispatched_work(),
        "old work must not open a new session"
    );
    assert!(pool.try_claim(None).is_some());
}

#[tokio::test]
async fn background_continuation_survives_project_context_failure_in_drop_mode() {
    let (mut pool, mut queue, scope, dir, mut ctx) = started_background_turn(0).await;
    finish_child(&mut pool, &mut queue, &dir).await;
    // Known metadata forces authoritative project resolution; the dead local
    // endpoint makes that lookup indeterminate before any ACP prompt is sent.
    let inner = Arc::get_mut(&mut ctx).unwrap();
    inner.dedup_mode = DedupMode::Drop;
    inner.channel_info.cache.write().unwrap().insert(
        scope.channel_id(),
        PromptChannelInfo {
            name: "test".into(),
            channel_type: "channel".into(),
            description: None,
            project: None,
        },
    );
    assert_eq!(
        crate::dispatch_pending(
            &mut pool,
            &mut queue,
            &ctx,
            &mut tokio::time::Instant::now(),
            None
        )
        .len(),
        1
    );
    let result = timeout(Duration::from_secs(10), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        result.outcome,
        PromptOutcome::ProjectContextIndeterminate(_)
    ));
    let retry = result
        .batch
        .expect("continuation must survive even in Drop mode");
    assert!(retry.continuation.is_some());
    assert!(queue.requeue(retry).is_none());
    queue.mark_complete(&scope);
    assert!(queue.has_undispatched_work());
    assert!(!queue.has_flushable_work(), "failure must back off");
}

#[tokio::test]
async fn observed_background_exit_during_result_delivery_still_queues_collection() {
    let (mut pool, mut queue, scope, dir, _) = started_background_turn(0).await;
    let mut agent = pool.try_claim(Some(&scope)).unwrap();
    // Poll outside the pool until it exits, preserving the completed turn's
    // recorded work exactly as a delayed result channel would.
    std::fs::write(dir.path().join("finish"), "done").unwrap();
    timeout(Duration::from_secs(5), async {
        while agent
            .acp
            .background_child_pids()
            .is_none_or(|pids| !pids.is_empty())
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    pool.return_agent(agent);
    pool.queue_finished_background_work(&mut queue);
    assert!(queue.flush_next().unwrap().continuation.is_some());
}

pub(crate) fn push_new_message(queue: &mut EventQueue, scope: &SessionScope) {
    let event = EventBuilder::new(Kind::Custom(9), "unrelated new request")
        .tags([nostr::Tag::parse(["e", &"b".repeat(64), "", "root"]).unwrap()])
        .sign_with_keys(&Keys::generate())
        .unwrap();
    queue.push(QueuedEvent {
        channel_id: scope.channel_id(),
        scope: scope.clone(),
        event,
        prompt_tag: "test".into(),
        received_at: std::time::Instant::now(),
        edit: None,
    });
}

pub(crate) fn dispatch(
    pool: &mut AgentPool,
    queue: &mut EventQueue,
    ctx: &Arc<PromptContext>,
) -> usize {
    crate::dispatch_pending(pool, queue, ctx, &mut tokio::time::Instant::now(), None).len()
}

async fn return_success(pool: &mut AgentPool, queue: &mut EventQueue) {
    let result = timeout(Duration::from_secs(10), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        result.outcome,
        PromptOutcome::Ok(StopReason::EndTurn)
    ));
    apply_result(pool, queue, result, None, &HashSet::new());
}

pub(crate) fn apply_result(
    pool: &mut AgentPool,
    queue: &mut EventQueue,
    result: PromptResult,
    rest: Option<&RestClient>,
    removed: &HashSet<Uuid>,
) {
    let config = crate::build_mcp_servers_tests::test_config();
    let mut crash_history: Vec<_> = (0..pool.agents.len())
        .map(|_| crate::SlotCircuit {
            crash_times: Vec::new(),
            open_until: None,
            respawn_in_flight: false,
        })
        .collect();
    let (respawn_tx, _) = mpsc::channel(8);
    let mut respawn_tasks = tokio::task::JoinSet::new();
    crate::handle_prompt_result(
        pool,
        queue,
        &config,
        result,
        &mut false,
        removed,
        &mut crash_history,
        &respawn_tx,
        &mut respawn_tasks,
        None,
        rest,
    );
    assert!(
        respawn_tasks.is_empty(),
        "healthy adapter must stay reusable"
    );
}

fn last_prompt(dir: &tempfile::TempDir) -> serde_json::Value {
    std::fs::read_to_string(dir.path().join("requests"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .rfind(|r| r["method"] == "session/prompt")
        .unwrap()
}

#[tokio::test]
async fn background_ready_collection_survives_message_in_channel_thread_and_dm() {
    for (scope, dm) in [
        (
            SessionScope::Conversation {
                channel_id: Uuid::new_v4(),
            },
            false,
        ),
        (scope(), false),
        (
            SessionScope::Conversation {
                channel_id: Uuid::new_v4(),
            },
            true,
        ),
    ] {
        let (mut pool, mut queue, scope, dir, ctx) =
            started_background_turn_in_scope(0, scope, dm).await;
        let original = pool.runtime_holds[&0]
            .continuation
            .as_ref()
            .unwrap()
            .thread_tags
            .clone();
        finish_child(&mut pool, &mut queue, &dir).await;
        push_new_message(&mut queue, &scope);
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
        assert!(
            pool.task_map
                .values()
                .next()
                .unwrap()
                .recoverable_batch
                .as_ref()
                .unwrap()
                .continuation
                .is_some(),
            "ready collection must precede the new request"
        );
        return_success(&mut pool, &mut queue).await;
        let prompt = last_prompt(&dir);
        assert_eq!(prompt["params"]["sessionId"], "original-session");
        assert!(prompt["params"]["prompt"][0]["text"]
            .as_str()
            .unwrap()
            .contains(original.root_event_id.as_ref().unwrap()));
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
        return_success(&mut pool, &mut queue).await;
        assert!(!queue.has_undispatched_work());
        assert!(!pool.has_runtime_hold());
    }
}

#[tokio::test]
async fn background_exit_survives_already_queued_message() {
    let (mut pool, mut queue, scope, dir, ctx) = started_background_turn(0).await;
    push_new_message(&mut queue, &scope);
    finish_child(&mut pool, &mut queue, &dir).await;
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    assert!(pool
        .task_map
        .values()
        .next()
        .unwrap()
        .recoverable_batch
        .as_ref()
        .unwrap()
        .continuation
        .is_some());
    return_success(&mut pool, &mut queue).await;
    assert!(last_prompt(&dir)["params"]["prompt"][0]["text"]
        .as_str()
        .unwrap()
        .contains("background child process"));
}

#[tokio::test]
async fn background_live_child_keeps_original_thread_after_other_thread_message() {
    let (mut pool, mut queue, scope, dir, ctx) = started_background_turn(0).await;
    let original = pool.runtime_holds[&0]
        .continuation
        .as_ref()
        .unwrap()
        .thread_tags
        .clone();
    push_new_message(&mut queue, &scope);
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    return_success(&mut pool, &mut queue).await;
    finish_child(&mut pool, &mut queue, &dir).await;
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    return_success(&mut pool, &mut queue).await;
    let prompt = last_prompt(&dir);
    let text = prompt["params"]["prompt"][0]["text"].as_str().unwrap();
    assert!(
        text.contains(original.root_event_id.as_ref().unwrap()),
        "{text}"
    );
    assert!(
        !text.contains(&"b".repeat(64)),
        "another thread must not redirect collection"
    );
}

#[tokio::test]
async fn background_backoff_message_cannot_rotate_original_session_at_max_turns_one() {
    let (mut pool, mut queue, scope, dir, ctx) = started_background_turn(1).await;
    finish_child(&mut pool, &mut queue, &dir).await;
    let batch = queue.flush_next().unwrap();
    assert!(queue.requeue(batch).is_none());
    queue.mark_complete(&scope);
    push_new_message(&mut queue, &scope);
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    return_success(&mut pool, &mut queue).await;
    assert_eq!(
        pool.agents[0]
            .as_ref()
            .unwrap()
            .state
            .sessions
            .get(&scope)
            .map(String::as_str),
        Some("original-session"),
        "rotation must wait for collection even after child exit"
    );
    assert!(queue.has_undispatched_work());
    assert!(queue.next_retry_deadline().is_some());
}

async fn wait_marker(dir: &tempfile::TempDir, name: &str) {
    timeout(Duration::from_secs(5), async {
        while !dir.path().join(name).exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn background_collection_resumes_original_session_after_steer_and_interrupt() {
    for signal in [ControlSignal::Steer, ControlSignal::Interrupt] {
        let (mut pool, mut queue, scope, dir, ctx) = started_background_turn(0).await;
        finish_child(&mut pool, &mut queue, &dir).await;
        std::fs::write(dir.path().join("block-collection"), "wait").unwrap();
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
        wait_marker(&dir, "collection-started").await;
        push_new_message(&mut queue, &scope);
        assert_eq!(
            dispatch(&mut pool, &mut queue, &ctx),
            0,
            "same scope serializes"
        );
        assert!(crate::signal_in_flight_task_for_scope(
            &mut pool,
            &scope,
            signal.clone()
        ));
        let result = timeout(Duration::from_secs(10), pool.result_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(result.outcome, PromptOutcome::Cancelled));
        assert_eq!(
            result.agent.state.sessions.get(&scope).map(String::as_str),
            Some("original-session"),
            "ordinary interruption must not invalidate the collection's original session"
        );
        assert!(result.batch.as_ref().unwrap().continuation.is_some());
        apply_result(&mut pool, &mut queue, result, None, &HashSet::new());
        std::fs::remove_file(dir.path().join("block-collection")).unwrap();
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
        assert!(pool
            .task_map
            .values()
            .next()
            .unwrap()
            .recoverable_batch
            .as_ref()
            .unwrap()
            .continuation
            .is_some());
        return_success(&mut pool, &mut queue).await;
        assert_eq!(last_prompt(&dir)["params"]["sessionId"], "original-session");
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
        return_success(&mut pool, &mut queue).await;
    }
}

#[tokio::test]
async fn background_generation_replacement_and_channel_revocation_never_resume_collection() {
    for revoked in [false, true] {
        let (mut pool, mut queue, scope, dir, ctx) = started_background_turn(0).await;
        finish_child(&mut pool, &mut queue, &dir).await;
        if revoked {
            queue.drain_channel(scope.channel_id());
            pool.invalidate_channel_sessions(scope.channel_id());
        } else {
            pool.record_scope_owner(scope.clone(), 1);
        }
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 0);
        pool.queue_finished_background_work(&mut queue);
        assert!(!pool.has_runtime_hold());
        assert!(!queue.has_undispatched_work());
        let requests = std::fs::read_to_string(dir.path().join("requests")).unwrap();
        assert_eq!(
            requests
                .lines()
                .filter(|line| line.contains("session/prompt"))
                .count(),
            1,
            "revoked or replaced ownership must never submit collection"
        );
    }
}

#[tokio::test]
async fn background_collection_blocks_heartbeat_and_idle_sleep_in_each_pool_mode() {
    for (lazy, idle_secs) in [(false, 0), (true, 0), (true, 2)] {
        let (mut pool, mut queue, _scope, dir, ctx) = started_background_turn(0).await;
        // Keep only the original worker so heartbeat cannot use another slot.
        pool.agents.pop();
        finish_child(&mut pool, &mut queue, &dir).await;
        assert!(pool.try_claim(None).is_none());
        let now = tokio::time::Instant::now();
        assert!(!crate::idle_pool_sleep_due(
            lazy,
            now - Duration::from_secs(60),
            now,
            Duration::from_secs(idle_secs),
            queue.has_in_flight(),
            false,
            queue.has_undispatched_work(),
            false,
            pool.has_runtime_hold()
        ));
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
        return_success(&mut pool, &mut queue).await;
        assert!(pool.try_claim(None).is_some());
        assert!(!queue.has_undispatched_work());
    }
}

#[tokio::test]
async fn background_success_can_start_a_new_group_without_repeating_old_collection() {
    let (mut pool, mut queue, scope, dir, ctx) = started_background_turn(1).await;
    let original = pool.runtime_holds[&0]
        .continuation
        .as_ref()
        .unwrap()
        .thread_tags
        .clone();
    finish_child(&mut pool, &mut queue, &dir).await;
    std::fs::write(dir.path().join("next-group"), "start").unwrap();
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    return_success(&mut pool, &mut queue).await;
    assert!(pool.has_runtime_hold(), "new child group retains residency");
    assert!(!queue.has_undispatched_work(), "old collection completed");
    assert_eq!(
        pool.agents[0]
            .as_ref()
            .unwrap()
            .state
            .sessions
            .get(&scope)
            .map(String::as_str),
        Some("original-session")
    );
    std::fs::write(dir.path().join("finish-next"), "done").unwrap();
    finish_child(&mut pool, &mut queue, &dir).await;
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    return_success(&mut pool, &mut queue).await;
    assert!(!pool.has_runtime_hold());
    assert!(!queue.has_undispatched_work());
    assert!(
        !pool.agents[0]
            .as_ref()
            .unwrap()
            .state
            .sessions
            .contains_key(&scope),
        "rotation follows the final group collection"
    );
    let requests = std::fs::read_to_string(dir.path().join("requests")).unwrap();
    assert_eq!(
        requests
            .lines()
            .filter(|line| line.contains("session/prompt"))
            .count(),
        3
    );
    assert!(last_prompt(&dir)["params"]["prompt"][0]["text"]
        .as_str()
        .unwrap()
        .contains(original.root_event_id.as_ref().unwrap()));
}

#[tokio::test]
async fn background_explicit_cancel_after_ordinary_control_never_resumes_collection() {
    for ordinary in [ControlSignal::Steer, ControlSignal::Interrupt] {
        let (mut pool, mut queue, scope, dir, ctx) = started_background_turn(0).await;
        finish_child(&mut pool, &mut queue, &dir).await;
        std::fs::write(dir.path().join("block-collection"), "wait").unwrap();
        std::fs::write(dir.path().join("hold-cancel-ack"), "wait").unwrap();
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
        wait_marker(&dir, "collection-started").await;
        push_new_message(&mut queue, &scope);
        assert!(crate::signal_in_flight_task_for_scope(
            &mut pool, &scope, ordinary,
        ));
        wait_marker(&dir, "cancel-received").await;
        // The ordinary control already consumed the one-shot. The later
        // explicit cancel must still revoke the checked-out collection.
        assert!(crate::cancel_background_collection_for_scope(
            &mut pool, &mut queue, &scope
        ));
        assert!(!crate::signal_in_flight_task_for_scope(
            &mut pool,
            &scope,
            ControlSignal::Cancel,
        ));
        std::fs::remove_file(dir.path().join("hold-cancel-ack")).unwrap();
        let result = timeout(Duration::from_secs(10), pool.result_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(result.outcome, PromptOutcome::Cancelled));
        apply_result(&mut pool, &mut queue, result, None, &HashSet::new());
        assert!(!queue.has_pending_continuation(&scope), "explicit cancellation must suppress the returned continuation, without a stale-session failure notice");
        assert!(!pool.has_runtime_hold());
        std::fs::remove_file(dir.path().join("block-collection")).unwrap();
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
        assert!(
            !pool.task_map.values().any(|meta| meta
                .recoverable_batch
                .as_ref()
                .is_some_and(|batch| batch.continuation.is_some())),
            "explicit cancel after an ordinary control must never revive collection"
        );
        return_success(&mut pool, &mut queue).await;
        assert!(
            !crate::cancel_background_collection_for_scope(&mut pool, &mut queue, &scope),
            "genuinely idle cancel remains a no-op"
        );
        assert!(pool.agents[0]
            .as_ref()
            .unwrap()
            .state
            .sessions
            .contains_key(&scope));
    }
}

#[tokio::test]
async fn background_lost_prompt_task_releases_pool_side_hold_before_respawn() {
    let (mut pool, mut queue, scope, dir, ctx) = started_background_turn(0).await;
    finish_child(&mut pool, &mut queue, &dir).await;
    std::fs::write(dir.path().join("block-collection"), "wait").unwrap();
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    wait_marker(&dir, "collection-started").await;
    pool.join_set.abort_all();
    let error = loop {
        if let Err(error) = pool.join_set.join_next().await.unwrap() {
            break error;
        }
    };
    let mut config = crate::build_mcp_servers_tests::test_config();
    config.agent_command = "true".into();
    config.agent_args.clear();
    let mut crash_history: Vec<_> = (0..pool.agents.len())
        .map(|_| crate::SlotCircuit {
            crash_times: Vec::new(),
            open_until: None,
            respawn_in_flight: false,
        })
        .collect();
    let (respawn_tx, _) = mpsc::channel(8);
    let mut respawn_tasks = tokio::task::JoinSet::new();
    crate::recover_panicked_agent(
        &mut pool,
        &mut queue,
        &config,
        error,
        &mut false,
        &HashSet::new(),
        &mut HashMap::new(),
        &mut crash_history,
        &respawn_tx,
        &mut respawn_tasks,
        None,
    );
    assert!(
        !pool.has_runtime_hold(),
        "a lost worker must not retain background residency during circuit cooldown"
    );
    assert!(
        queue.has_pending_continuation(&scope),
        "existing failure recovery remains queued"
    );
    respawn_tasks.abort_all();
}

#[tokio::test]
async fn background_observer_cancel_after_ordinary_control_never_resumes_collection() {
    for ordinary in [ControlSignal::Steer, ControlSignal::Interrupt] {
        let (mut pool, mut queue, scope, dir, ctx) =
            started_background_turn_in_scope(0, scope(), false).await;
        finish_child(&mut pool, &mut queue, &dir).await;
        std::fs::write(dir.path().join("block-collection"), "wait").unwrap();
        std::fs::write(dir.path().join("hold-cancel-ack"), "wait").unwrap();
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
        wait_marker(&dir, "collection-started").await;
        push_new_message(&mut queue, &scope);
        assert!(crate::signal_in_flight_task_for_scope(
            &mut pool, &scope, ordinary
        ));
        wait_marker(&dir, "cancel-received").await;
        let observer = observer::ObserverHandle::in_process();
        let payload = serde_json::json!({"channelId": scope.channel_id().to_string(), "requestId": "stop-collection"});
        crate::handle_cancel_turn_control(&payload, &mut pool, &mut queue, Some(&observer));
        assert_eq!(
            observer.snapshot()[0].payload["status"],
            "sent",
            "explicit observer cancel must revoke collection even without a sender"
        );
        std::fs::remove_file(dir.path().join("hold-cancel-ack")).unwrap();
        let result = timeout(Duration::from_secs(10), pool.result_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(result.outcome, PromptOutcome::Cancelled));
        apply_result(&mut pool, &mut queue, result, None, &HashSet::new());
        assert!(!queue.has_pending_continuation(&scope));
        assert!(!pool.has_runtime_hold());
        std::fs::remove_file(dir.path().join("block-collection")).unwrap();
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
        assert!(!pool.task_map.values().any(|meta| meta
            .recoverable_batch
            .as_ref()
            .is_some_and(|batch| batch.continuation.is_some())));
        return_success(&mut pool, &mut queue).await;
        crate::handle_cancel_turn_control(&payload, &mut pool, &mut queue, Some(&observer));
        assert_eq!(
            observer.snapshot().last().unwrap().payload["status"],
            "no_active_turn"
        );
        assert!(pool.agents[0]
            .as_ref()
            .unwrap()
            .state
            .sessions
            .contains_key(&scope));
    }
}
