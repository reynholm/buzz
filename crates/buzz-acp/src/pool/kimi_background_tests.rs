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
async fn kimi_collection_dm_listener_steer_delivers_correction() {
    exercise_collection_listener_control(true, true, crate::config::MultipleEventHandling::Steer)
        .await;
}

#[tokio::test]
async fn kimi_collection_dm_listener_interrupt_delivers_correction() {
    exercise_collection_listener_control(
        true,
        true,
        crate::config::MultipleEventHandling::Interrupt,
    )
    .await;
}

#[tokio::test]
async fn kimi_collection_channel_listener_steer_delivers_correction() {
    exercise_collection_listener_control(false, true, crate::config::MultipleEventHandling::Steer)
        .await;
}

#[tokio::test]
async fn kimi_collection_channel_listener_interrupt_delivers_correction() {
    exercise_collection_listener_control(
        false,
        true,
        crate::config::MultipleEventHandling::Interrupt,
    )
    .await;
}

#[tokio::test]
async fn kimi_collection_top_level_dm_listener_steer_delivers_correction() {
    exercise_collection_listener_control(true, false, crate::config::MultipleEventHandling::Steer)
        .await;
}

#[tokio::test]
async fn kimi_collection_top_level_dm_listener_interrupt_delivers_correction() {
    exercise_collection_listener_control(
        true,
        false,
        crate::config::MultipleEventHandling::Interrupt,
    )
    .await;
}

#[tokio::test]
async fn kimi_collection_does_not_reissue_original_slash_command() {
    let scope = SessionScope::Conversation {
        channel_id: Uuid::new_v4(),
    };
    let assignment = "/goal inspect result.csv";
    let (mut pool, mut queue, scope, dir, ctx) =
        started_background_turn_with_assignment(0, scope, false, true, None, assignment).await;
    assert_eq!(last_prompt(&dir)["params"]["prompt"][0]["text"], assignment);
    finish_child(&mut pool, &mut queue, &dir).await;
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    return_success(&mut pool, &mut queue).await;
    let prompt = last_prompt(&dir);
    assert!(
        !prompt["params"]["prompt"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("/goal"),
        "retained assignment is collection context, not a fresh adapter command"
    );
    assert!(prompt.to_string().contains(assignment));
    assert!(prompt["params"]["prompt"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()["text"]
        .as_str()
        .unwrap()
        .contains("Do not start a second copy of the task"));
    assert!(!queue.has_pending_scope(&scope));
    for mut agent in pool.agents.into_iter().flatten() {
        agent.acp.shutdown().await;
    }
}

async fn exercise_collection_listener_control(
    dm: bool,
    threaded: bool,
    mode: crate::config::MultipleEventHandling,
) {
    let root = "a".repeat(64);
    let scope = SessionScope::Conversation {
        channel_id: Uuid::new_v4(),
    };
    let (mut pool, mut queue, scope, dir, ctx) = started_background_turn_with_assignment(
        0,
        scope,
        dm,
        true,
        threaded.then_some(root.as_str()),
        "start background work",
    )
    .await;
    if !dm {
        ctx.channel_info.cache.write().unwrap().insert(
            scope.channel_id(),
            PromptChannelInfo {
                name: "Test channel".into(),
                channel_type: "stream".into(),
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
    let original = pool.runtime_holds[&0]
        .continuation
        .as_ref()
        .unwrap()
        .work
        .clone()
        .unwrap();
    finish_child(&mut pool, &mut queue, &dir).await;
    std::fs::write(dir.path().join("block-collection"), "wait").unwrap();
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    wait_marker(&dir, "collection-started").await;
    let correction = "Read the revised dataset and put the collected result in revised-result.txt";
    let mut event = EventBuilder::new(Kind::Custom(9), correction);
    if threaded {
        event = event.tag(nostr::Tag::parse(["e", root.as_str(), "", "reply"]).unwrap());
    }
    let event = event.sign_with_keys(&Keys::generate()).unwrap();
    let received_at = std::time::Instant::now();
    let accepted = queue.push(QueuedEvent {
        channel_id: scope.channel_id(),
        scope: scope.clone(),
        event: event.clone(),
        received_at,
        prompt_tag: "mention".into(),
        edit: None,
    });
    assert!(accepted, "the correction must be admitted");
    if dm {
        let candidate = crate::queue::BatchEvent {
            event: event.clone(),
            received_at,
            prompt_tag: "mention".into(),
            edit: None,
        };
        assert!(
            queue.in_flight_accepts_steer(&scope, &candidate.reply_route(), dm),
            "same-root DM route must be eligible: routing={:?}, candidate={:?}, original={:?}",
            queue.in_flight_prompt_routing(&scope),
            candidate.reply_route(),
            original.reply_tags()
        );
    }
    let (ack_tx, _ack_rx) = mpsc::unbounded_channel();
    crate::QueuedNormalListenerEvent {
        accepted,
        scope: scope.clone(),
        effective_author: event.pubkey.to_hex(),
        reaction_target_id: event.id.to_hex(),
        channel_is_dm: dm,
        steer_event: crate::queue::BatchEvent {
            event,
            received_at,
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
    assert!(
        pool.task_map.values().next().unwrap().control_tx.is_none(),
        "the first same-thread listener control must interrupt collection"
    );
    let result = timeout(Duration::from_secs(5), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(result.outcome, PromptOutcome::Cancelled));
    apply_result(&mut pool, &mut queue, result, None, &HashSet::new());
    std::fs::remove_file(dir.path().join("block-collection")).unwrap();
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    let retained = pool
        .task_map
        .values()
        .next()
        .unwrap()
        .recoverable_batch
        .as_ref()
        .unwrap();
    let work = retained.work.as_ref().unwrap();
    assert_eq!(work.delivery_key, original.delivery_key);
    assert_eq!(work.deadline, original.deadline);
    assert_eq!(work.journal_dir, original.journal_dir);
    assert_eq!(work.reply_tags(), original.reply_tags());
    assert!(retained.continuation.is_some());
    return_success(&mut pool, &mut queue).await;
    let prompt = last_prompt(&dir).to_string();
    assert!(
        prompt.contains(correction),
        "the collection prompt must deliver the admitted correction: {prompt}"
    );
    assert!(prompt.contains("Do not start a second copy of the task"));
    assert!(!queue.has_undispatched_work());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("background-launches"))
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
async fn kimi_collection_permission_crash_recovers_through_production_handler_and_queue() {
    exercise_collection_crash(false, None).await;
}

#[tokio::test]
async fn kimi_cancel_during_respawn_never_rebinds_the_old_collection() {
    exercise_collection_crash(true, None).await;
}

#[tokio::test]
async fn kimi_collection_crash_retry_listener_steer_delivers_correction() {
    exercise_collection_crash(false, Some(crate::config::MultipleEventHandling::Steer)).await;
}

#[tokio::test]
async fn kimi_collection_crash_retry_listener_interrupt_delivers_correction() {
    exercise_collection_crash(false, Some(crate::config::MultipleEventHandling::Interrupt)).await;
}

async fn exercise_collection_crash(
    cancel_during_respawn: bool,
    listener_control: Option<crate::config::MultipleEventHandling>,
) {
    let scope = SessionScope::Conversation {
        channel_id: Uuid::new_v4(),
    };
    let (mut pool, mut queue, scope, dir, mut ctx) =
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
    // The replacement cannot resume this peer's session. Supply available
    // thread history over real HTTP so the first new-session prompt must carry
    // both the retained assignment and the background result already posted.
    let root = original.reply_tags().root_event_id.unwrap();
    let result_event = EventBuilder::new(
        Kind::Custom(9),
        "Background read result: dataset has 37 rows; collect existing artifact result.csv",
    )
    .tag(nostr::Tag::parse(["e", root.as_str(), "", "reply"]).unwrap())
    .tag(nostr::Tag::parse(["h", scope.channel_id().to_string().as_str()]).unwrap())
    .sign_with_keys(&Keys::generate())
    .unwrap();
    let history = serde_json::json!([original.original_events[0].event, result_event]).to_string();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let recovery_ctx = Arc::get_mut(&mut ctx).unwrap();
    recovery_ctx.rest_client.base_url = format!("http://{}", listener.local_addr().unwrap());
    recovery_ctx.context_message_limit = 12;
    let history_server = tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut request = [0; 8192];
            let _ = socket.read(&mut request).await;
            let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", history.len(), history);
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });
    if listener_control.is_some() {
        std::fs::remove_file(dir.path().join("collection-started")).unwrap();
        std::fs::write(dir.path().join("block-collection"), "wait").unwrap();
    }
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    if let Some(mode) = listener_control {
        wait_marker(&dir, "collection-started").await;
        let event = EventBuilder::new(
            Kind::Custom(9),
            "Collect the existing result using the revised schema",
        )
        .tag(nostr::Tag::parse(["e", root.as_str(), "", "reply"]).unwrap())
        .sign_with_keys(&Keys::generate())
        .unwrap();
        let received_at = std::time::Instant::now();
        let accepted = queue.push(QueuedEvent {
            channel_id: scope.channel_id(),
            scope: scope.clone(),
            event: event.clone(),
            received_at,
            prompt_tag: "mention".into(),
            edit: None,
        });
        let (ack_tx, _ack_rx) = mpsc::unbounded_channel();
        crate::QueuedNormalListenerEvent {
            accepted,
            scope: scope.clone(),
            effective_author: event.pubkey.to_hex(),
            reaction_target_id: event.id.to_hex(),
            channel_is_dm: false,
            steer_event: crate::queue::BatchEvent {
                event,
                received_at,
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
        assert!(
            pool.task_map.values().next().unwrap().control_tx.is_none(),
            "the first ordinary control after checked crash recovery must interrupt collection"
        );
        let cancelled = timeout(Duration::from_secs(5), pool.result_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(cancelled.outcome, PromptOutcome::Cancelled));
        apply_result(&mut pool, &mut queue, cancelled, None, &HashSet::new());
        std::fs::remove_file(dir.path().join("block-collection")).unwrap();
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
        let retained = pool
            .task_map
            .values()
            .next()
            .unwrap()
            .recoverable_batch
            .as_ref()
            .unwrap();
        let work = retained.work.as_ref().unwrap();
        assert_eq!(work.delivery_key, original.delivery_key);
        assert_eq!(work.deadline, original.deadline);
        assert_eq!(work.journal_dir, original.journal_dir);
        assert_eq!(work.reply_tags(), original.reply_tags());
    }
    let result = timeout(Duration::from_secs(5), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    history_server.abort();
    assert!(matches!(result.outcome, PromptOutcome::Ok(_)));
    assert!(result.agent.state.scope_owner_generations[&scope] > continuation.generation);
    let prompt = last_prompt(&dir).to_string();
    assert!(prompt.contains(&original.delivery_key.to_string()));
    assert!(prompt.contains(original.reply_tags().root_event_id.as_ref().unwrap()));
    if listener_control.is_some() {
        assert!(prompt.contains("Collect the existing result using the revised schema"));
    }
    assert!(
        prompt.contains("start background work"),
        "a new, unresumed collection session must receive the retained assignment: {prompt}"
    );
    assert!(
        prompt.contains("dataset has 37 rows"),
        "the first replacement prompt must receive available collection history: {prompt}"
    );
    let requests: Vec<serde_json::Value> = std::fs::read_to_string(dir.path().join("requests"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request["method"] == "session/new")
            .count(),
        2
    );
    assert!(!requests
        .iter()
        .any(|request| request["method"] == "session/resume"));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("background-launches"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    apply_result(&mut pool, &mut queue, result, None, &HashSet::new());
    pool.queue_finished_background_work(&mut queue);
    assert!(
        queue.flush_next().is_none(),
        "successful collection must not queue another collection"
    );
    assert!(
        !pool.has_runtime_hold(),
        "successful collection must release its completed hold"
    );
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
