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
import json, subprocess, sys
child = None
for line in sys.stdin:
    req = json.loads(line)
    with open(sys.argv[2], 'a') as log:
        log.write(line)
    if req['method'] == 'initialize':
        result = {'protocolVersion': 1, 'agentCapabilities': {}}
    elif req['method'] == 'session/new':
        result = {'sessionId': 'original-session'}
    elif req['method'] == 'session/prompt':
        if child is None:
            program = 'import os,sys,time\nwhile not os.path.exists(sys.argv[1]): time.sleep(.01)'
            child = subprocess.Popen([sys.executable, '-c', program, sys.argv[1]])
        result = {'stopReason': 'end_turn'}
    else:
        result = {}
    print(json.dumps({'jsonrpc': '2.0', 'id': req['id'], 'result': result}), flush=True)
"#;

async fn started_background_turn(
    max_turns: u32,
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
    let scope = SessionScope::Conversation {
        channel_id: Uuid::new_v4(),
    };
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

async fn finish_child(pool: &mut AgentPool, queue: &mut EventQueue, dir: &tempfile::TempDir) {
    std::fs::write(dir.path().join("finish"), "done").unwrap();
    timeout(Duration::from_secs(5), async {
        while pool.has_runtime_hold() {
            pool.queue_finished_background_work(queue);
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
