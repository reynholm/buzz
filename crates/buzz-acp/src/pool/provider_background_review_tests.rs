//! Regression journeys for the independently reviewed lifecycle boundaries.
use super::*;

fn clarification(queue: &mut EventQueue, scope: &SessionScope, root: &str) -> nostr::Event {
    let event = EventBuilder::new(
        Kind::Custom(9),
        "Use the corrected dataset for the existing results",
    )
    .tag(nostr::Tag::parse(["e", root, "", "reply"]).unwrap())
    .sign_with_keys(&Keys::generate())
    .unwrap();
    queue.push(QueuedEvent {
        channel_id: scope.channel_id(),
        scope: scope.clone(),
        event: event.clone(),
        received_at: std::time::Instant::now(),
        prompt_tag: "mention".into(),
        edit: None,
    });
    event
}

#[tokio::test]
async fn provider_background_snapshot_failure_preserves_clarification_and_unstarted_batch() {
    let (mut pool, mut queue, scope, dir, ctx) = setup().await;
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    return_success(&mut pool, &mut queue).await;
    complete_tasks(&mut pool, &mut queue, &dir).await;
    let root = pool.runtime_holds[&0]
        .continuation
        .as_ref()
        .unwrap()
        .thread_tags
        .root_event_id
        .clone()
        .unwrap();
    let input = clarification(&mut queue, &scope, &root);
    let batch = queue.flush_next().unwrap();
    assert!(batch.work.is_none());
    assert_eq!(batch.events[0].event.id, input.id);
    queue.requeue_preserve_timestamps(batch);
    queue.mark_complete(&scope);
    let batch = queue.flush_next().unwrap();
    assert_eq!(
        batch.events.first().map(|input| input.event.id),
        Some(input.id),
        "capacity deferral must retain the entire collection input"
    );
    queue.requeue_preserve_timestamps(batch);
    queue.mark_complete(&scope);
    std::fs::write(dir.path().join("fail-snapshot"), "").unwrap();
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    let result = timeout(Duration::from_secs(10), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        result.outcome,
        PromptOutcome::Error(AcpError::AgentError { .. })
    ));
    let crate::session_store::LoadOutcome::Found(saved) =
        result.agent.state.store.as_ref().unwrap().load(&scope)
    else {
        panic!("missing provider registry")
    };
    assert!(
        saved
            .live
            .unwrap()
            .provider_background
            .unwrap()
            .events
            .iter()
            .any(|saved| saved.event.id == input.id),
        "accepted input must be durable before the first fallible snapshot"
    );
    apply_result(&mut pool, &mut queue, result, None, &HashSet::new());
    std::fs::remove_file(dir.path().join("fail-snapshot")).unwrap();
    timeout(Duration::from_secs(10), async {
        while dispatch(&mut pool, &mut queue, &ctx) == 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    return_success(&mut pool, &mut queue).await;
    assert!(
        last_prompt(&dir).to_string().contains(&input.content),
        "snapshot retry must render the retained clarification"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("launches"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    for mut agent in pool.agents.into_iter().flatten() {
        agent.acp.shutdown().await;
    }
}

#[tokio::test]
async fn provider_background_native_collection_refusal_queues_ordinary_input_without_cancel() {
    for mode in [
        crate::config::MultipleEventHandling::Steer,
        crate::config::MultipleEventHandling::Interrupt,
    ] {
        let (mut pool, mut queue, scope, dir, ctx) = setup_with_native_steering(true).await;
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
        return_success(&mut pool, &mut queue).await;
        complete_tasks(&mut pool, &mut queue, &dir).await;
        std::fs::write(dir.path().join("block-collection"), "").unwrap();
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
        wait_marker(&dir, "collection-started").await;
        let root = queue.in_flight_reply_thread(&scope).unwrap().to_owned();
        let input = clarification(&mut queue, &scope, &root);
        let (ack_tx, _ack_rx) = mpsc::unbounded_channel();
        crate::QueuedNormalListenerEvent {
            accepted: true,
            scope: scope.clone(),
            effective_author: input.pubkey.to_hex(),
            reaction_target_id: input.id.to_hex(),
            channel_is_dm: false,
            steer_event: crate::queue::BatchEvent {
                event: input.clone(),
                received_at: std::time::Instant::now(),
                prompt_tag: "mention".into(),
                edit: None,
            },
        }
        .steer_or_interrupt(
            mode,
            &ctx.session_controls,
            None,
            &mut pool,
            &mut queue,
            &ack_tx,
        );
        let untouched = pool.task_map.values().next().unwrap().control_tx.is_some();
        std::fs::remove_file(dir.path().join("block-collection")).unwrap();
        let mut result = timeout(Duration::from_secs(10), pool.result_rx.recv())
            .await
            .unwrap()
            .unwrap();
        result.agent.acp.shutdown().await;
        assert!(
            untouched,
            "synchronous refusal with both capabilities must not take the cancellation sender"
        );
        apply_result(&mut pool, &mut queue, result, None, &HashSet::new());
        let batch = queue.flush_next().expect("ordinary input stays queued");
        assert_eq!(batch.events[0].event.id, input.id);
        let requests = std::fs::read_to_string(dir.path().join("requests")).unwrap();
        assert!(!requests
            .lines()
            .any(
                |line| serde_json::from_str::<serde_json::Value>(line).unwrap()["method"]
                    == "session/cancel"
            ));
    }
}

#[tokio::test]
async fn provider_background_runtime_collection_has_bounded_retry_with_original_identity() {
    let (mut pool, queue, scope, dir, ctx) = setup().await;
    let mut queue = queue.with_runtime_delivery(dir.path().to_str().unwrap(), 1800);
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    return_success(&mut pool, &mut queue).await;
    complete_tasks(&mut pool, &mut queue, &dir).await;
    let identity = pool.agents[0]
        .as_ref()
        .unwrap()
        .acp
        .provider_background_record()
        .unwrap()
        .identity
        .clone();
    std::fs::write(dir.path().join("omit-ack"), "").unwrap();
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    let result = timeout(Duration::from_secs(10), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        result.outcome,
        PromptOutcome::Error(AcpError::BackgroundCollectionPending)
    ));
    apply_result(&mut pool, &mut queue, result, None, &HashSet::new());
    assert!(
        queue.has_pending_continuation(&scope),
        "runtime-final's no-execution-retry policy must not drop pending child collection"
    );
    let mut retry = timeout(Duration::from_secs(10), async {
        loop {
            if let Some(batch) = queue.flush_next() {
                break batch;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(retry.work.as_ref().unwrap().delivery_key, identity.work_id);
    assert_eq!(retry.work.as_ref().unwrap().attempt, 1);
    let saved = retry
        .continuation
        .as_ref()
        .unwrap()
        .provider_recovery
        .as_ref()
        .unwrap();
    assert_eq!(saved.identity, identity);
    assert_eq!(saved.pending().len(), 3);
    retry.work.as_mut().unwrap().attempt = crate::queue::RecoveryPolicy::default().max_retries;
    assert!(
        queue.requeue(retry).is_some(),
        "collection retries must be bounded independently of runtime execution retries"
    );
    for mut agent in pool.agents.into_iter().flatten() {
        agent.acp.shutdown().await;
    }
}

#[tokio::test]
async fn provider_background_runtime_collection_keeps_original_deadline_through_snapshot_and_prompt(
) {
    for stage in ["snapshot", "collection"] {
        let (mut pool, queue, scope, dir, mut ctx) = setup().await;
        Arc::get_mut(&mut ctx).unwrap().max_turn_duration = Duration::from_secs(60);
        let mut queue = queue.with_runtime_delivery(dir.path().to_str().unwrap(), 20);
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
        return_success(&mut pool, &mut queue).await;
        complete_tasks(&mut pool, &mut queue, &dir).await;
        let marker = format!("block-{stage}");
        std::fs::write(dir.path().join(&marker), "").unwrap();
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
        wait_marker(&dir, &format!("{stage}-started")).await;
        let bounded = timeout(Duration::from_secs(22), pool.result_rx.recv()).await;
        std::fs::remove_file(dir.path().join(&marker)).unwrap();
        let timed_out = bounded.is_err();
        let mut result = match bounded {
            Ok(result) => result.unwrap(),
            Err(_) => timeout(Duration::from_secs(10), pool.result_rx.recv())
                .await
                .unwrap()
                .unwrap(),
        };
        result.agent.acp.shutdown().await;
        assert!(
            !timed_out,
            "{stage} exceeded the original twenty-second work deadline"
        );
        assert!(
            matches!(
                result.outcome,
                PromptOutcome::Error(AcpError::WorkDeadlinePoisoned)
                    | PromptOutcome::Timeout(_)
                    | PromptOutcome::Error(AcpError::WorkDeadline)
            ),
            "unexpected {stage} outcome"
        );
        assert_eq!(result.source.scope(), Some(&scope));
    }
}

#[tokio::test]
async fn provider_background_restart_after_last_receipt_resumes_unfinished_parent_only() {
    let (mut pool, queue, scope, dir, ctx) = setup().await;
    let mut queue = queue.with_runtime_delivery(dir.path().to_str().unwrap(), 1800);
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    return_success(&mut pool, &mut queue).await;
    complete_tasks(&mut pool, &mut queue, &dir).await;
    let store = pool.agents[0]
        .as_ref()
        .unwrap()
        .state
        .store
        .clone()
        .unwrap();
    let identity = pool.agents[0]
        .as_ref()
        .unwrap()
        .acp
        .provider_background_record()
        .unwrap()
        .identity
        .clone();
    std::fs::write(dir.path().join("block-final"), "").unwrap();
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    wait_marker(&dir, "results-acked").await;
    timeout(Duration::from_secs(5), async {
        loop {
            if let crate::session_store::LoadOutcome::Found(saved) = store.load(&scope) {
                if saved
                    .live
                    .as_ref()
                    .unwrap()
                    .provider_background
                    .as_ref()
                    .unwrap()
                    .tasks
                    .values()
                    .all(|task| task.state == crate::provider_background::State::Collected)
                {
                    break;
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!dir
        .path()
        .join(".buzz-runtime-deliveries/journal.json")
        .exists());
    pool.join_set.abort_all();
    while pool.join_set.join_next().await.is_some() {}
    drop(pool);
    drop(queue);
    drop(store);
    let store = Arc::new(
        crate::session_store::SessionStore::open(
            dir.path().join("sessions"),
            "wss://relay.example",
            &ctx.agent_keys.public_key().to_hex(),
        )
        .unwrap(),
    );
    let records = store.pending_provider_work(&[scope.channel_id()]).unwrap();
    assert_eq!(
        records.len(),
        1,
        "durable child receipts do not complete the parent before its final answer"
    );
    assert!(!records[0].outstanding());
    assert_eq!(records[0].identity, identity);
    let mut queue =
        EventQueue::new(DedupMode::Queue).with_runtime_delivery(dir.path().to_str().unwrap(), 1800);
    queue.restore_provider_background(records[0].clone(), dir.path().to_str().unwrap());
    let mut acp = AcpClient::spawn(
        "python3",
        &[
            "-u".into(),
            "-c".into(),
            ADAPTER.into(),
            dir.path().to_string_lossy().into(),
        ],
        &[],
        false,
    )
    .await
    .unwrap();
    acp.initialize().await.unwrap();
    let mut pool = AgentPool::from_slots(vec![Some(owned_worker(0, acp))]);
    pool.set_session_store(Some(store));
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    wait_marker(&dir, "parent-resumed").await;
    let prompt = last_prompt(&dir);
    assert_eq!(
        prompt["params"]["_meta"]["buzz"]["backgroundTasks"]["identity"],
        serde_json::to_value(identity).unwrap()
    );
    assert_eq!(
        prompt["params"]["_meta"]["buzz"]["backgroundTasks"]["collect"],
        serde_json::json!([])
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("launches"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    pool.join_set.abort_all();
    while pool.join_set.join_next().await.is_some() {}
}

#[tokio::test]
async fn provider_background_expired_idle_collection_never_reenters_provider() {
    for runtime in [false, true] {
        let (mut pool, queue, scope, dir, ctx) = setup().await;
        let mut queue = if runtime {
            queue.with_runtime_delivery(dir.path().to_str().unwrap(), 1800)
        } else {
            queue
        };
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
        return_success(&mut pool, &mut queue).await;
        complete_tasks(&mut pool, &mut queue, &dir).await;
        let mut batch = queue.flush_next().unwrap();
        batch
            .continuation
            .as_mut()
            .unwrap()
            .provider_recovery
            .as_mut()
            .unwrap()
            .deadline_unix = 1;
        queue.requeue_preserve_timestamps(batch);
        queue.mark_complete(&scope);
        let before = std::fs::read_to_string(dir.path().join("requests")).unwrap();
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 0);
        assert!(!pool.has_runtime_hold());
        assert!(!queue.has_pending_continuation(&scope));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("requests")).unwrap(),
            before
        );
        let agent = pool.agents[0].as_ref().unwrap();
        assert_eq!(
            agent
                .acp
                .provider_background_record()
                .unwrap()
                .terminal_reason
                .as_deref(),
            Some("background_deadline")
        );
        for mut agent in pool.agents.into_iter().flatten() {
            agent.acp.shutdown().await;
        }
    }
}
