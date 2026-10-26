//! Scheduling regressions with a fully held pool.
use super::*;

#[tokio::test]
async fn current_shared_snapshot_retains_actual_work_in_its_slot() {
    let original = scope();
    let agent = worker(0).await;
    let pid = agent.acp.process_id().unwrap();
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    pool.set_runtime_hold(
        0,
        RuntimeHold {
            scope: original,
            blocking_pids: HashSet::new(),
            continuation: None,
            collection_pending: false,
        },
    );
    let tree = crate::process_tree::parse_instance_snapshot(&format!("{pid} 1 S Mon Oct 26 10:00:00 2026 adapter\n424242 {pid} S Mon Oct 26 10:00:01 2026 python3 work.py\n")).unwrap();
    let requests = pool.background_poll_requests();
    pool.apply_background_snapshot(
        &requests,
        Some(&tree),
        &mut EventQueue::new(DedupMode::Queue),
    );
    assert_eq!(
        pool.runtime_holds[&0].blocking_pids,
        HashSet::from([424242])
    );
    assert!(pool.try_claim(Some(&scope())).is_none());
}

#[tokio::test]
async fn completed_empty_turn_prepares_observation_before_listener_return() {
    let original = scope();
    let mut agent = worker(0).await;
    agent.background_context = Some(RuntimeHold {
        scope: original.clone(),
        blocking_pids: HashSet::new(),
        continuation: None,
        collection_pending: false,
    });
    agent
        .acp
        .background_child_snapshots
        .push_back(Some(HashSet::new()));
    let mut pool = AgentPool::from_slots(vec![None]);
    send_prompt_result(
        &pool.result_tx(),
        "empty-turn",
        agent,
        PromptSource::Channel(original),
        PromptOutcome::Ok(StopReason::EndTurn),
        None,
    );
    let result = pool.result_rx.try_recv().unwrap();
    pool.return_agent(result.agent);
    assert!(
        !pool.has_runtime_hold(),
        "a known empty turn must not become an uncertainty hold on return"
    );
}

#[tokio::test]
async fn newly_registered_service_does_not_invent_collection_or_hide_reused_work_pid() {
    for reused in [false, true] {
        let original = scope();
        let mut pool = AgentPool::from_slots(vec![Some(worker(0).await)]);
        bind(&mut pool, &original, 0);
        let mut agent = pool.try_claim(Some(&original)).unwrap();
        let session = agent.state.sessions[&original].clone();
        agent.remember_background_context(&PromptSource::Channel(original.clone()), None, &session);
        let pid = agent.acp.process_id().unwrap();
        let census = |second: &str| {
            crate::process_tree::parse_instance_snapshot(&format!(
            "{pid} 1 S Mon Oct 26 10:00:00 2026 adapter\n424242 {pid} S Mon Oct 26 10:00:{second} 2026 wrapper\n424243 424242 S Mon Oct 26 10:00:{second} 2026 node ./src/index.mjs\n"
        )).unwrap()
        };
        let first = census("01");
        let children = agent.acp.background_child_pids_in(&first).unwrap();
        assert_eq!(children, HashSet::from([424242]));
        agent.background_context.as_mut().unwrap().blocking_pids = children.clone();
        agent.acp.return_children = Some(Some(children));
        pool.return_agent(agent);
        let mut registered = census(if reused { "02" } else { "01" });
        registered.register_service_instances(424242, &registered.service_instances(424242));
        let requests = pool.background_poll_requests();
        let mut queue = EventQueue::new(DedupMode::Queue);
        pool.apply_background_snapshot(&requests, Some(&registered), &mut queue);
        assert_eq!(
            queue.has_pending_continuation(&original),
            reused,
            "only an actual old process exit should collect; reused={reused}"
        );
        assert_eq!(pool.has_runtime_hold(), reused);
    }
}

#[tokio::test]
async fn checked_restart_of_held_scope_releases_slot_and_retains_ordinary_input() {
    let (mut pool, mut queue, original, _dir, _ctx) = started_background_turn(0).await;
    push_new_message(&mut queue, &original);
    let before = queue.flush_next().unwrap();
    let event = before.events[0].event.id;
    let age = before.events[0].received_at;
    queue.requeue_preserve_timestamps(before);
    queue.release_unstarted(&original);
    let target = pool.retirement_targets(&original)[0];
    pool.mark_retire_on_return(target);
    let mut retiring = pool
        .take_retirement_target(target, &HashSet::from([original.clone()]), false)
        .unwrap();
    assert_eq!(
        retiring.acp.terminate().await,
        crate::acp::TerminationReport::Clean
    );
    assert!(!pool.has_runtime_hold());
    let retained = queue
        .flush_next()
        .expect("ordinary message survives checked retirement");
    assert_eq!(retained.scope, original);
    assert_eq!(retained.events[0].event.id, event);
    assert_eq!(retained.events[0].received_at, age);
}

#[tokio::test]
async fn returning_worker_does_not_collect_process_snapshot_on_event_loop() {
    let mut agent = worker(0).await;
    agent.background_context = Some(RuntimeHold {
        scope: scope(),
        blocking_pids: HashSet::new(),
        continuation: None,
        collection_pending: false,
    });
    agent.acp.background_child_snapshots.push_back(None);
    let mut pool = AgentPool::from_slots(vec![None]);
    pool.return_agent(agent);
    assert_eq!(
        pool.agents[0]
            .as_ref()
            .unwrap()
            .acp
            .background_child_snapshots
            .len(),
        1,
        "the main-loop return path must not perform enumeration"
    );
    assert!(
        pool.has_runtime_hold(),
        "missing return evidence is uncertainty"
    );
}

#[tokio::test]
async fn stale_and_failed_shared_snapshots_preserve_current_hold() {
    let original = scope();
    let mut pool = AgentPool::from_slots(vec![Some(worker(0).await)]);
    let make_hold = || RuntimeHold {
        scope: original.clone(),
        blocking_pids: HashSet::from([123]),
        continuation: None,
        collection_pending: false,
    };
    pool.set_runtime_hold(0, make_hold());
    let requests = pool.background_poll_requests();
    let mut queue = EventQueue::new(DedupMode::Queue);
    pool.apply_background_snapshot(&requests, None, &mut queue);
    assert!(pool.has_runtime_hold(), "failure must not become empty");
    pool.set_runtime_hold(0, make_hold());
    let empty = crate::process_tree::ProcessTree::default();
    pool.apply_background_snapshot(&requests, Some(&empty), &mut queue);
    assert!(
        pool.has_runtime_hold(),
        "replaced hold must reject an old result"
    );
    let requests = pool.background_poll_requests();
    pool.agents[0] = Some(worker(0).await);
    pool.apply_background_snapshot(&requests, Some(&empty), &mut queue);
    assert!(
        pool.has_runtime_hold(),
        "replacement worker must reject old result"
    );
    let requests = pool.background_poll_requests();
    pool.apply_background_snapshot(&requests, Some(&empty), &mut queue);
    assert!(
        !pool.has_runtime_hold(),
        "current known-empty snapshot releases it"
    );
}

#[tokio::test]
async fn unavailable_oldest_scope_does_not_block_available_held_owner() {
    let available = scope();
    let unavailable = scope();
    let mut pool = AgentPool::from_slots(vec![Some(worker(0).await)]);
    bind(&mut pool, &available, 0);
    pool.set_runtime_hold(
        0,
        RuntimeHold {
            scope: available.clone(),
            blocking_pids: HashSet::from([123]),
            continuation: None,
            collection_pending: false,
        },
    );
    let mut queue = EventQueue::new(DedupMode::Queue);
    let original_age = std::time::Instant::now() - Duration::from_secs(30);
    queue.set_retry_count_for_test(unavailable.clone(), crate::queue::MAX_RETRIES);
    let event = EventBuilder::new(Kind::Custom(9), "oldest unavailable request")
        .sign_with_keys(&Keys::generate())
        .unwrap();
    let original_id = event.id;
    queue.push(QueuedEvent {
        channel_id: unavailable.channel_id(),
        scope: unavailable.clone(),
        event,
        prompt_tag: "test".into(),
        received_at: original_age,
        edit: None,
    });
    push_new_message(&mut queue, &available);
    let ctx = Arc::new(tests::make_prompt_context_no_owner());
    assert_eq!(
        dispatch(&mut pool, &mut queue, &ctx),
        1,
        "the later owner must start despite the unclaimable oldest request"
    );
    assert!(queue.is_scope_in_flight(&available));
    assert!(!queue.is_scope_in_flight(&unavailable));
    // Repeated passes must terminate and preserve exactly the same input.
    for _ in 0..3 {
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 0);
    }
    let retained = queue.flush_next().unwrap();
    assert_eq!(retained.scope, unavailable);
    assert_eq!(retained.events.len(), 1);
    assert_eq!(retained.events[0].event.id, original_id);
    assert_eq!(retained.events[0].received_at, original_age);
    assert!(queue.flush_next().is_none());
    assert!(
        queue.requeue(retained).is_some(),
        "capacity holds must preserve the exhausted retry budget"
    );
    pool.join_set.abort_all();
}
