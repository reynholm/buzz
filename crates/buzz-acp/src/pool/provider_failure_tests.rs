use super::*;
use crate::queue::{EventQueue, QueuedEvent};
use nostr::{EventBuilder, Keys, Kind, Tag};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[cfg(target_os = "linux")]
pub(crate) async fn resource_adapter(
    directory: &std::path::Path,
    failure: &str,
    trigger: &str,
) -> (AcpClient, std::path::PathBuf) {
    let script = r#"
import json, pathlib, sys
directory, failure, trigger = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3]
for line in sys.stdin:
    request = json.loads(line)
    method = request.get('method')
    with (directory / 'resource-methods.log').open('a') as log:
        log.write(str(method) + '\n')
    if method == trigger:
        state = pathlib.Path((directory / 'receipt-path').read_text())
        temporary = state.with_suffix('.next')
        temporary.write_text(json.dumps({'state': failure}))
        temporary.replace(state)
        continue
    if method == 'session/prompt' or 'id' not in request:
        continue
    result = {'protocolVersion': 1} if method == 'initialize' else {}
    if method == 'session/new':
        result = {'sessionId': 'resource-session'}
    print(json.dumps({'jsonrpc': '2.0', 'id': request['id'], 'result': result}), flush=True)
"#;
    let mut acp = AcpClient::spawn(
        "python3",
        &[
            "-u".into(),
            "-c".into(),
            script.into(),
            directory.to_string_lossy().into_owned(),
            failure.into(),
            trigger.into(),
        ],
        &[],
        false,
    )
    .await
    .unwrap();
    acp.initialize().await.unwrap();
    let group = acp.inject_unverified_resource_scope();
    std::fs::write(group.join("memory.events"), "oom_kill 0\n").unwrap();
    std::fs::write(
        directory.join("receipt-path"),
        group.parent().unwrap().join("state.json").to_str().unwrap(),
    )
    .unwrap();
    (acp, group)
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn resource_failure_during_idle_cancel_is_terminal_for_both_prompt_paths() {
    for initial in [false, true] {
        for failure in ["oom", "guard_failed"] {
            let dir = tempfile::tempdir().unwrap();
            let (acp, group) = resource_adapter(dir.path(), failure, "session/cancel").await;
            let agent = OwnedAgent {
                index: 0,
                acp,
                state: SessionState::default(),
                model_capabilities: None,
                desired_model: None,
                model_overridden: false,
                desired_model_request_id: None,
                desired_model_pending_ack: false,
                startup_effort: None,
                agent_name: "resource-probe".into(),
                goose_system_prompt_supported: None,
                protocol_version: 1,
                background_context: None,
            };
            let channel = Uuid::new_v4();
            let scope = SessionScope::Thread {
                channel_id: channel,
                root_event_id: "a".repeat(64),
            };
            let event = EventBuilder::new(Kind::Custom(9), "please continue")
                .sign_with_keys(&Keys::generate())
                .unwrap();
            let mut queue = EventQueue::new(DedupMode::Queue);
            queue.push(QueuedEvent {
                channel_id: channel,
                scope: scope.clone(),
                event,
                prompt_tag: "test".into(),
                received_at: std::time::Instant::now(),
                edit: None,
            });
            let mut ctx = tests::make_prompt_context_no_owner();
            ctx.cwd = dir.path().to_string_lossy().into_owned();
            ctx.memory_enabled = false;
            ctx.context_message_limit = 0;
            ctx.dedup_mode = DedupMode::Queue;
            ctx.idle_timeout = Duration::from_millis(100);
            ctx.initial_message = initial.then(|| "configured bootstrap".into());
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            ctx.rest_client.base_url = format!("http://{}", listener.local_addr().unwrap());
            ctx.channel_info = ChannelInfoResolver::new(HashMap::new(), ctx.rest_client.clone());
            let server = tokio::spawn(async move {
                while let Ok((mut socket, _)) = listener.accept().await {
                    let mut request = [0; 8192];
                    let _ = socket.read(&mut request).await;
                    let _ = socket.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").await;
                }
            });
            let ctx = Arc::new(ctx);
            let mut pool = AgentPool::from_slots(vec![Some(agent)]);
            assert_eq!(
                background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
                1
            );
            let result = timeout(Duration::from_secs(5), pool.result_rx.recv())
                .await
                .unwrap()
                .unwrap();
            let methods = std::fs::read_to_string(dir.path().join("resource-methods.log")).unwrap();
            assert_eq!(
                methods
                    .lines()
                    .filter(|line| *line == "session/prompt")
                    .count(),
                1
            );
            assert!(methods.lines().any(|line| line == "session/cancel"));
            assert!(
                matches!((&result.outcome, failure),
                    (PromptOutcome::Error(AcpError::ResourceLimit { .. }), "oom")
                    | (PromptOutcome::Error(AcpError::ResourceGuard(_)), "guard_failed")),
                "idle cancellation lost the terminal resource cause: initial={initial}, failure={failure}"
            );
            let config = crate::build_mcp_servers_tests::test_config();
            let mut circuits = vec![crate::SlotCircuit {
                crash_times: vec![],
                open_until: None,
                respawn_in_flight: false,
            }];
            let (tx, mut rx) = mpsc::channel(8);
            let mut tasks = JoinSet::new();
            crate::handle_prompt_result(
                &mut pool,
                &mut queue,
                &config,
                result,
                &mut false,
                &HashSet::new(),
                &mut circuits,
                &tx,
                &mut tasks,
                None,
                None,
                &mut tokio::time::Instant::now(),
            );
            assert_eq!(
                queue.queued_event_count(&scope),
                0,
                "resource failure replayed the original batch"
            );
            assert!(!queue.has_undispatched_work());
            assert_eq!(pool.live_count(), 0, "resource-failed worker was reused");
            let returned = timeout(Duration::from_secs(5), rx.recv())
                .await
                .unwrap()
                .unwrap();
            let retirement = returned
                .retirement
                .expect("checked retirement was bypassed");
            assert_eq!(retirement.report, crate::acp::TerminationReport::Unverified);
            assert!(
                returned.result.is_err(),
                "replacement started before verified cleanup"
            );
            let mut retained = retirement
                .retained
                .expect("original worker ownership was discarded");
            std::fs::write(group.join("cgroup.events"), "populated 0\n").unwrap();
            retained.acp.shutdown().await;
            tasks.shutdown().await;
            pool.join_set.shutdown().await;
            server.abort();
        }
    }
}

#[tokio::test]
async fn provider_failure_requeues_original_event_and_commits_only_after_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("prompts.jsonl");
    let mut acp = AcpClient::spawn(
        "python3",
        &[
            "-u".into(),
            "-c".into(),
            crate::acp::provider_failure_tests::ADAPTER.into(),
        ],
        &[
            ("FAIL_FIRST".into(), "1".into()),
            ("PROMPTS_LOG".into(), log.to_string_lossy().into_owned()),
        ],
        false,
    )
    .await
    .unwrap();
    acp.initialize().await.unwrap();
    let agent = OwnedAgent {
        index: 0,
        acp,
        state: SessionState::default(),
        model_capabilities: None,
        desired_model: None,
        model_overridden: false,
        desired_model_request_id: None,
        desired_model_pending_ack: false,
        startup_effort: None,
        agent_name: "provider-probe".into(),
        goose_system_prompt_supported: None,
        protocol_version: 2,
        background_context: None,
    };
    let channel = Uuid::new_v4();
    let scope = SessionScope::Thread {
        channel_id: channel,
        root_event_id: "a".repeat(64),
    };
    let event = EventBuilder::new(Kind::Custom(9), "please continue")
        .tag(Tag::parse(["h", &channel.to_string()]).unwrap())
        .tag(Tag::parse(["e", &"a".repeat(64), "", "reply"]).unwrap())
        .sign_with_keys(&Keys::generate())
        .unwrap();
    let event_id = event.id.to_hex();
    let mut queue = EventQueue::new(DedupMode::Queue);
    queue.push(QueuedEvent {
        channel_id: channel,
        scope: scope.clone(),
        event,
        prompt_tag: "test".into(),
        received_at: std::time::Instant::now(),
        edit: None,
    });
    let mut ctx = tests::make_prompt_context_no_owner();
    ctx.cwd = dir.path().to_string_lossy().into_owned();
    ctx.memory_enabled = false;
    ctx.context_message_limit = 0;
    ctx.dedup_mode = DedupMode::Queue;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    ctx.rest_client.base_url = format!("http://{}", listener.local_addr().unwrap());
    ctx.channel_info = ChannelInfoResolver::new(HashMap::new(), ctx.rest_client.clone());
    let server = tokio::spawn(async move {
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
    let ctx = Arc::new(ctx);
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    assert_eq!(
        background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
        1
    );
    let result = timeout(Duration::from_secs(10), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        result.outcome,
        PromptOutcome::Error(AcpError::AgentError { .. })
    ));
    assert!(!result
        .agent
        .state
        .deliveries
        .get(&scope)
        .is_some_and(|delivery| { delivery.delivered_event_ids.contains(&event_id) }));
    assert_eq!(
        result.agent.state.sessions.get(&scope).map(String::as_str),
        Some("provider-session")
    );
    background_work_tests::apply_result(&mut pool, &mut queue, result, None, &HashSet::new());
    assert_eq!(queue.queued_event_count(&scope), 1);
    tokio::time::sleep_until(queue.next_retry_deadline().unwrap().into()).await;
    assert_eq!(
        background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
        1
    );
    let result = timeout(Duration::from_secs(10), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        result.outcome,
        PromptOutcome::Ok(StopReason::EndTurn)
    ));
    assert!(result.agent.state.deliveries[&scope]
        .delivered_event_ids
        .contains(&event_id));
    background_work_tests::apply_result(&mut pool, &mut queue, result, None, &HashSet::new());
    assert_eq!(queue.queued_event_count(&scope), 0);
    let prompts = std::fs::read_to_string(log).unwrap();
    assert_eq!(prompts.lines().count(), 2);
    assert!(prompts.lines().all(|line| line.contains(&event_id)));
    for agent in pool.agents.iter_mut().flatten() {
        agent.acp.shutdown().await;
    }
    server.abort();
}
