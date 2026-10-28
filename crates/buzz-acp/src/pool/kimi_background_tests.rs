//! Kimi work identity and checked crash recovery through the production queue.
use super::*;

#[tokio::test]
async fn kimi_background_collection_retains_original_work_and_queues_fresh_traffic() {
    let scope = SessionScope::Conversation {
        channel_id: Uuid::new_v4(),
    };
    let (mut pool, mut queue, scope, dir, ctx) =
        started_background_turn_in_scope(0, scope, false, true).await;
    let original = pool.runtime_holds[&0]
        .continuation
        .as_ref()
        .unwrap()
        .work
        .clone()
        .unwrap();
    assert!(last_prompt(&dir)
        .to_string()
        .contains(&original.delivery_key.to_string()));
    push_new_message(&mut queue, &scope);
    assert!(
        queue.flush_next().is_none(),
        "fresh traffic cannot replace the waiting work"
    );
    finish_child(&mut pool, &mut queue, &dir).await;
    let collection = queue.flush_next().unwrap();
    assert!(collection.continuation.as_ref().unwrap().background_ready);
    let work = collection.work.as_ref().unwrap();
    assert_eq!(work.delivery_key, original.delivery_key);
    assert_eq!(work.deadline, original.deadline);
    assert_eq!(
        work.original_events[0].event.id,
        original.original_events[0].event.id
    );
    assert_eq!(
        collection.reply_tags().root_event_id,
        original.reply_tags().root_event_id
    );
    queue.requeue_preserve_timestamps(collection);
    queue.release_unstarted(&scope);
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    return_success(&mut pool, &mut queue).await;
    for mut agent in pool.agents.into_iter().flatten() {
        agent.acp.shutdown().await;
    }
}

#[tokio::test]
async fn kimi_collection_permission_crash_recovers_through_production_handler_and_queue() {
    exercise_collection_crash(false).await;
}

#[tokio::test]
async fn kimi_cancel_during_respawn_never_rebinds_the_old_collection() {
    exercise_collection_crash(true).await;
}

async fn exercise_collection_crash(cancel_during_respawn: bool) {
    let scope = SessionScope::Conversation {
        channel_id: Uuid::new_v4(),
    };
    let (mut pool, mut queue, scope, dir, ctx) =
        started_background_turn_in_scope(0, scope, false, true).await;
    pool.agents[1].take().unwrap().acp.shutdown().await;
    let continuation = pool.runtime_holds[&0].continuation.clone().unwrap();
    let original = continuation.work.clone().unwrap();
    let process = pool.agents[0].as_ref().unwrap().acp.process_instance;
    finish_child(&mut pool, &mut queue, &dir).await;
    std::fs::write(
        dir.path().join("crash-collection"),
        "crash after read permission",
    )
    .unwrap();
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    let result = timeout(Duration::from_secs(5), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(
        result.outcome.requires_retirement(),
        "collection must observe a child crash"
    );
    assert!(
        !original.uncertain_effects(),
        "a declared read did not start a mutating action"
    );
    let mut config = crate::build_mcp_servers_tests::test_config();
    config.agent_command = "python3".into();
    config.agent_args = vec![
        "-u".into(),
        "-c".into(),
        background_adapter(true),
        dir.path().join("finish").to_string_lossy().into(),
        dir.path().join("requests").to_string_lossy().into(),
    ];
    let mut circuits: Vec<_> = (0..2)
        .map(|_| crate::SlotCircuit {
            crash_times: vec![],
            open_until: Some(std::time::Instant::now()),
            respawn_in_flight: false,
        })
        .collect();
    let (tx, mut rx) = mpsc::channel(8);
    let mut tasks = tokio::task::JoinSet::new();
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
    assert!(original.awaiting_retirement());
    assert_eq!(
        dispatch(&mut pool, &mut queue, &ctx),
        0,
        "no attempt before checked stop"
    );
    if cancel_during_respawn {
        crate::handle_owner_cancel_for_scope(
            &mut pool,
            &mut queue,
            &ctx.session_controls,
            &scope,
            None,
        );
        assert!(original.cancelled());
    }
    let mut returned = timeout(Duration::from_secs(10), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        returned.retirement.as_ref().unwrap().report,
        crate::acp::TerminationReport::Clean
    );
    let mut parked = HashMap::new();
    assert!(!crate::worker_retirement::finish(
        &mut pool,
        &ctx.session_controls,
        &mut parked,
        &mut returned
    ));
    assert!(parked.is_empty());
    let (acp, protocol, name) = returned.result.unwrap();
    assert_ne!(acp.process_instance, process);
    let mut replacement = owned_worker(0, acp);
    replacement.protocol_version = protocol;
    replacement.agent_name = name;
    pool.return_agent(replacement);
    if cancel_during_respawn {
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 0);
        assert!(!queue.has_pending_scope(&scope));
        tasks.shutdown().await;
        for mut agent in pool.agents.into_iter().flatten() {
            agent.acp.shutdown().await;
        }
        return;
    }
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    let result = timeout(Duration::from_secs(5), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(result.outcome, PromptOutcome::Ok(_)));
    assert!(result.agent.state.scope_owner_generations[&scope] > continuation.generation);
    let prompt = last_prompt(&dir).to_string();
    assert!(prompt.contains(&original.delivery_key.to_string()));
    assert!(prompt.contains(original.reply_tags().root_event_id.as_ref().unwrap()));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("background-launches"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    apply_result(&mut pool, &mut queue, result, None, &HashSet::new());
    tasks.shutdown().await;
    for mut agent in pool.agents.into_iter().flatten() {
        agent.acp.shutdown().await;
    }
}

#[tokio::test]
async fn kimi_cancelled_background_hold_is_checked_stopped_without_new_input() {
    let scope = SessionScope::Conversation {
        channel_id: Uuid::new_v4(),
    };
    let (mut pool, mut queue, scope, dir, ctx) =
        started_background_turn_in_scope(0, scope, false, true).await;
    let work = pool.runtime_holds[&0]
        .continuation
        .as_ref()
        .unwrap()
        .work
        .clone()
        .unwrap();
    crate::handle_owner_cancel_for_scope(
        &mut pool,
        &mut queue,
        &ctx.session_controls,
        &scope,
        None,
    );
    assert!(work.cancelled());
    let mut config = crate::build_mcp_servers_tests::test_config();
    config.agent_command = "python3".into();
    config.agent_args = vec![
        "-u".into(),
        "-c".into(),
        background_adapter(true),
        dir.path().join("finish").to_string_lossy().into(),
        dir.path().join("requests").to_string_lossy().into(),
    ];
    let mut circuits: Vec<_> = (0..2)
        .map(|_| crate::SlotCircuit {
            crash_times: vec![],
            open_until: None,
            respawn_in_flight: false,
        })
        .collect();
    let (tx, mut rx) = mpsc::channel(8);
    let mut tasks = tokio::task::JoinSet::new();
    crate::worker_retirement::retire_finished_kimi_background(
        &mut pool,
        &config,
        &mut circuits,
        &tx,
        &mut tasks,
        None,
    );
    assert!(work.awaiting_retirement());
    assert!(circuits[0].respawn_in_flight);
    assert!(pool.agents[0].is_none());
    assert!(!pool.has_runtime_hold());
    let mut returned = timeout(Duration::from_secs(32), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        returned.retirement.as_ref().unwrap().report,
        crate::acp::TerminationReport::Clean
    );
    let mut parked = HashMap::new();
    assert!(!crate::worker_retirement::finish(
        &mut pool,
        &ctx.session_controls,
        &mut parked,
        &mut returned
    ));
    let (mut replacement, _, _) = returned.result.unwrap();
    replacement.shutdown().await;
    assert!(queue.flush_next().is_none());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("background-launches"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    tasks.shutdown().await;
    for mut agent in pool.agents.into_iter().flatten() {
        agent.acp.shutdown().await;
    }
}

#[tokio::test]
async fn kimi_background_deadline_wake_requires_reconciliation_before_retirement() {
    let scope = SessionScope::Conversation {
        channel_id: Uuid::new_v4(),
    };
    let (mut pool, mut queue, scope, dir, ctx) =
        started_background_turn_in_scope(0, scope, false, true).await;
    let mut continuation = pool.runtime_holds[&0].continuation.clone().unwrap();
    let mut work = continuation.work.clone().unwrap();
    work.deadline = std::time::Instant::now();
    continuation.work = Some(work.clone());
    continuation.background_ready = false;
    queue.remove_continuation(&scope);
    queue.enqueue_continuation(scope.clone(), continuation);
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 0);
    assert!(
        work.needs_human(),
        "deadline wake runs before retirement; the background result is still unknown"
    );
    assert!(work.cancelled(), "terminal work must never be redispatched");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("background-launches"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    std::fs::write(dir.path().join("finish"), "done").unwrap();
    for mut agent in pool.agents.into_iter().flatten() {
        agent.acp.shutdown().await;
    }
}
