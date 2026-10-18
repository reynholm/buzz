use super::*;
use crate::session_store::ControlReceiptState;

#[tokio::test]
async fn compact_resume_disabled_disk_refusal_and_replay_never_wake() {
    let fixture = Fixture::new();
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(scope().channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    fixture.departed_record(&scope(), &ctx, &["delivered"]);
    let original_live = fixture.record(&scope()).live;
    let pool = crate::pool::AgentPool::from_slots(vec![None]);
    let request = authorized_control(&ctx, &human, &scope(), "!compact").await;
    let failure = ctx
        .session_controls
        .queue_compaction(
            request.clone(),
            pool.compaction_target(&scope(), &ctx.session_controls, false),
        )
        .expect_err("a retained control-store session cannot authorize disk resume");
    assert_eq!(failure, ControlFailure::NoSession);
    // Match the listener's refusal publication without putting chat work in the queue.
    ctx.session_controls.queue_result(
        request.origin.command_event_id.clone(),
        ControlResult::Terminal(Err(failure)),
    );
    let publication = ctx
        .session_controls
        .take_publications()
        .pop_front()
        .unwrap();
    assert_eq!(publication.command_id, request.origin.command_event_id);
    assert_eq!(
        publication.result,
        ControlResult::Terminal(Err(ControlFailure::NoSession))
    );
    let record = fixture.record(&scope());
    assert_eq!(record.live, original_live);
    assert_eq!(record.control_receipts.len(), 1);
    assert_eq!(
        record.control_receipts[0].command_id,
        request.origin.command_event_id
    );
    assert_eq!(
        record.control_receipts[0].state,
        ControlReceiptState::Finished(Err(ControlFailure::NoSession))
    );
    let mut queue = crate::EventQueue::new(crate::DedupMode::Queue);
    let mut lifecycle = crate::pool_lifecycle::PoolLifecycle::<()>::listening();
    assert!(ctx.session_controls.pending_compactions().is_empty());
    assert!(!crate::control_tasks::wake_pending(
        &mut queue,
        &ctx.session_controls
    ));
    assert_eq!(
        lifecycle.start_wake_if_due(
            crate::control_tasks::wake_pending(&mut queue, &ctx.session_controls),
            tokio::time::Instant::now()
        ),
        None
    );

    // Rebuild the controller so only the durable receipt can answer redelivery.
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    ctx.session_controls
        .queue_compaction(
            request.clone(),
            pool.compaction_target(&scope(), &ctx.session_controls, false),
        )
        .unwrap();
    let replay = ctx
        .session_controls
        .take_publications()
        .pop_front()
        .unwrap();
    assert_eq!(replay.command_id, request.origin.command_event_id);
    assert_eq!(replay.result, publication.result);
    assert_eq!(fixture.record(&scope()), record);
    assert!(ctx.session_controls.pending_compactions().is_empty());
    assert!(!crate::control_tasks::wake_pending(
        &mut queue,
        &ctx.session_controls
    ));
    assert_eq!(
        lifecycle.start_wake_if_due(
            crate::control_tasks::wake_pending(&mut queue, &ctx.session_controls),
            tokio::time::Instant::now()
        ),
        None
    );
    assert!(
        fixture.methods().is_empty(),
        "no provider worker was initialized"
    );
    server.abort();
}

#[tokio::test]
async fn compact_resume_enabled_disk_wakes_and_resumes_without_new_session() {
    let fixture = Fixture::new();
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(scope().channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    fixture.departed_record(&scope(), &ctx, &["delivered"]);
    let mut pool = crate::pool::AgentPool::from_slots(vec![None]);
    let request = authorized_control(&ctx, &human, &scope(), "!compact").await;
    ctx.session_controls
        .queue_compaction(
            request,
            pool.compaction_target(&scope(), &ctx.session_controls, true),
        )
        .unwrap();
    assert_eq!(ctx.session_controls.pending_compactions().len(), 1);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let ctx = Arc::new(ctx);
    crate::control_tasks::dispatch(&mut pool, &ctx, &tx);
    assert!(rx.try_recv().is_err());
    assert!(fixture.methods().is_empty());
    let mut queue = crate::EventQueue::new(crate::DedupMode::Queue);
    let mut lifecycle = crate::pool_lifecycle::PoolLifecycle::listening();
    assert_eq!(
        lifecycle.start_wake_if_due(
            crate::control_tasks::wake_pending(&mut queue, &ctx.session_controls),
            tokio::time::Instant::now()
        ),
        Some(1)
    );
    let prompts = fixture.dir.path().join("policy-prompts.jsonl");
    let prompt_path = prompts.display().to_string();
    let agent = fixture
        .agent(&[
            ("RESUME_CAP", "1"),
            ("COMPACT_STATUS", "completed"),
            ("PROMPTS", prompt_path.as_str()),
        ])
        .await;
    lifecycle
        .complete_wake(
            1,
            Ok(crate::pool::AgentPool::from_slots(vec![Some(agent)])),
            tokio::time::Instant::now(),
        )
        .unwrap();
    pool = lifecycle.take_ready().unwrap();
    pool.set_session_store(Some(fixture.store.clone()));
    crate::control_tasks::dispatch(&mut pool, &ctx, &tx);
    let mut result = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.outcome, Ok(()));
    assert_eq!(result.agent.state.sessions[&scope()], "old-1");
    assert_eq!(result.agent.state.turn_counts[&scope()], 1);
    assert!(result.agent.state.deliveries[&scope()]
        .delivered_event_ids
        .contains("delivered"));
    let methods = fixture.methods();
    assert_eq!(methods, ["initialize", "session/resume", "session/prompt"]);
    let prompt: serde_json::Value =
        serde_json::from_str(std::fs::read_to_string(prompts).unwrap().trim()).unwrap();
    assert_eq!(prompt["sessionId"], "old-1");
    assert_eq!(
        prompt["prompt"],
        serde_json::json!([{"type":"text","text":"/compact"}])
    );
    result.agent.acp.shutdown().await;
    pool.join_set.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn compact_resume_disabled_memory_owner_still_compacts_without_resume() {
    let (fixture, ctx, mut pool, human, server) = setup().await;
    pool.set_session_store(None);
    let mut owner = pool.try_claim(Some(&scope())).unwrap();
    assert!(owner.state.store.is_none());
    owner.state.turn_counts.insert(scope(), 7);
    owner.state.mark_scope_delivery_success(
        scope(),
        true,
        ["delivered".into()],
        [scope().root_event_id().unwrap().into()],
    );
    let observer = crate::observer::ObserverHandle::in_process();
    owner.acp.set_observer(Some(observer.clone()), 0);
    pool.return_agent(owner);
    let request = authorized_control(&ctx, &human, &scope(), "!compact").await;
    ctx.session_controls
        .queue_compaction(
            request,
            pool.compaction_target(&scope(), &ctx.session_controls, false),
        )
        .unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    crate::control_tasks::dispatch(&mut pool, &ctx, &tx);
    let mut result = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.outcome, Ok(()));
    assert!(result.agent.state.store.is_none());
    assert_eq!(result.agent.state.sessions[&scope()], "old-1");
    assert_eq!(result.agent.state.turn_counts[&scope()], 7);
    assert!(result.agent.state.deliveries[&scope()]
        .delivered_event_ids
        .contains("delivered"));
    assert_eq!(fixture.methods(), ["initialize", "session/prompt"]);
    let events = observer.snapshot();
    let prompts: Vec<_> = events
        .iter()
        .filter(|event| event.payload["method"] == "session/prompt")
        .collect();
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].payload["params"]["sessionId"], "old-1");
    assert_eq!(
        prompts[0].payload["params"]["prompt"],
        serde_json::json!([{"type":"text","text":"/compact"}])
    );
    result.agent.acp.shutdown().await;
    pool.join_set.shutdown().await;
    server.abort();
}
