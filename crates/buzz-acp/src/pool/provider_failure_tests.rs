use super::*;
use crate::queue::{EventQueue, QueuedEvent};
use nostr::{EventBuilder, Keys, Kind, Tag};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
