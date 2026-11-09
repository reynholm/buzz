use super::*;

use super::provider_background_fixture::ADAPTER;

async fn setup() -> (
    AgentPool,
    EventQueue,
    SessionScope,
    tempfile::TempDir,
    Arc<PromptContext>,
) {
    setup_with_native_steering(false).await
}

async fn setup_with_native_steering(
    native: bool,
) -> (
    AgentPool,
    EventQueue,
    SessionScope,
    tempfile::TempDir,
    Arc<PromptContext>,
) {
    setup_with_options(native, false).await
}

async fn setup_with_options(
    native: bool,
    authorization: bool,
) -> (
    AgentPool,
    EventQueue,
    SessionScope,
    tempfile::TempDir,
    Arc<PromptContext>,
) {
    let dir = tempfile::tempdir().unwrap();
    if authorization {
        std::fs::write(dir.path().join("authorization-fixture"), "").unwrap();
    }
    if native {
        std::fs::write(dir.path().join("native-steering"), "").unwrap();
    }
    let command;
    let args;
    if authorization {
        use std::os::unix::fs::PermissionsExt;
        let wrapper = dir.path().join("b24-cowork-code-cli");
        std::fs::write(&wrapper, format!("#!/usr/bin/env python3\n{ADAPTER}")).unwrap();
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
        command = wrapper.to_string_lossy().into_owned();
        args = vec![dir.path().to_string_lossy().into_owned()];
    } else {
        command = "python3".into();
        args = vec![
            "-u".into(),
            "-c".into(),
            ADAPTER.into(),
            dir.path().to_string_lossy().into(),
        ];
    }
    let mut acp = AcpClient::spawn(&command, &args, &[], false).await.unwrap();
    acp.request_runtime_final(true);
    acp.initialize().await.unwrap();
    let scope = scope();
    let mut ctx = tests::make_prompt_context_no_owner();
    ctx.cwd = dir.path().to_string_lossy().into();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    ctx.rest_client.base_url = format!("http://{}", listener.local_addr().unwrap());
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
    ctx.max_turns_per_session = 1;
    ctx.channel_info.cache.write().unwrap().insert(
        scope.channel_id(),
        PromptChannelInfo {
            name: "Test".into(),
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
    let store = Arc::new(
        crate::session_store::SessionStore::open(
            dir.path().join("sessions"),
            "wss://relay.example",
            &ctx.agent_keys.public_key().to_hex(),
        )
        .unwrap(),
    );
    let mut pool = AgentPool::from_slots(vec![Some(owned_worker(0, acp))]);
    pool.set_session_store(Some(store));
    let mut queue = EventQueue::new(DedupMode::Queue);
    let event = EventBuilder::new(
        Kind::Custom(9),
        "Research three topics in background and combine the existing results",
    )
    .sign_with_keys(&Keys::generate())
    .unwrap();
    queue.push(QueuedEvent {
        channel_id: scope.channel_id(),
        scope: scope.clone(),
        event,
        received_at: std::time::Instant::now(),
        prompt_tag: "mention".into(),
        edit: None,
    });
    (pool, queue, scope, dir, Arc::new(ctx))
}

#[tokio::test]
async fn provider_background_three_in_process_results_hold_then_collect_without_new_input() {
    let (mut pool, mut queue, scope, dir, ctx) = setup().await;
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    let first = timeout(Duration::from_secs(10), pool.result_rx.recv())
        .await
        .unwrap_or_else(|_| {
            panic!(
                "requests: {}",
                std::fs::read_to_string(dir.path().join("requests")).unwrap_or_default()
            )
        })
        .unwrap();
    assert!(matches!(
        first.outcome,
        PromptOutcome::Ok(StopReason::EndTurn)
    ));
    apply_result(&mut pool, &mut queue, first, None, &HashSet::new());
    assert!(
        pool.has_runtime_hold(),
        "provider-internal tasks have no OS child PID but still protect the worker"
    );
    assert!(pool.agents[0]
        .as_ref()
        .unwrap()
        .state
        .sessions
        .contains_key(&scope));
    for i in 0..3 {
        std::fs::write(dir.path().join(format!("child-{i}")), "done").unwrap();
    }
    timeout(Duration::from_secs(5), async {
        loop {
            pool.queue_finished_background_work(&mut queue);
            if pool.agents[0]
                .as_ref()
                .unwrap()
                .acp
                .provider_background_record()
                .is_some_and(|record| record.pending().len() == 3)
                && dispatch(&mut pool, &mut queue, &ctx) == 1
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    return_success(&mut pool, &mut queue).await;
    assert!(!pool.has_runtime_hold());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("launches"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("collections"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert!(queue.flush_next().is_none());
    for mut agent in pool.agents.into_iter().flatten() {
        agent.acp.shutdown().await;
    }
}

async fn complete_tasks(pool: &mut AgentPool, queue: &mut EventQueue, dir: &tempfile::TempDir) {
    for i in 0..3 {
        std::fs::write(dir.path().join(format!("child-{i}")), "done").unwrap();
    }
    timeout(Duration::from_secs(5), async {
        loop {
            pool.queue_finished_background_work(queue);
            if pool.agents[0]
                .as_ref()
                .unwrap()
                .acp
                .provider_background_record()
                .is_some_and(|record| record.pending().len() == 3)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn provider_background_end_turn_without_ack_preserves_results_and_retry_identity() {
    let (mut pool, mut queue, scope, dir, ctx) = setup().await;
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    return_success(&mut pool, &mut queue).await;
    let key = pool.runtime_holds[&0]
        .continuation
        .as_ref()
        .unwrap()
        .delivery_key;
    complete_tasks(&mut pool, &mut queue, &dir).await;
    std::fs::write(dir.path().join("omit-ack"), "pending").unwrap();
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
    assert!(pool.has_runtime_hold());
    assert_eq!(
        pool.runtime_holds[&0]
            .continuation
            .as_ref()
            .unwrap()
            .delivery_key,
        key
    );
    assert!(queue.next_retry_deadline().is_some());
    std::fs::remove_file(dir.path().join("omit-ack")).unwrap();
    timeout(Duration::from_secs(10), async {
        while dispatch(&mut pool, &mut queue, &ctx) == 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    return_success(&mut pool, &mut queue).await;
    assert!(!pool.has_background_collection_for_scope(&scope));
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
async fn provider_background_restart_reconciles_the_original_session_without_relaunch() {
    for scenario in ["collect", "expire", "expire_stale_local"] {
        let (mut pool, queue, scope, dir, ctx) = setup().await;
        let mut queue = if scenario == "collect" {
            queue
        } else {
            queue.with_runtime_delivery(dir.path().to_str().unwrap(), 20)
        };
        // A prior owner advances local ownership before the original launch.
        pool.record_scope_owner(scope.clone(), 0);
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
        return_success(&mut pool, &mut queue).await;
        for mut agent in pool.agents.iter_mut().filter_map(Option::take) {
            agent.acp.shutdown().await;
        }
        drop(pool);
        drop(queue);
        let store = Arc::new(
            crate::session_store::SessionStore::open(
                dir.path().join("sessions"),
                "wss://relay.example",
                &ctx.agent_keys.public_key().to_hex(),
            )
            .unwrap(),
        );
        assert!(store.pending_provider_work(&[]).unwrap().is_empty());
        let records = store.pending_provider_work(&[scope.channel_id()]).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].identity.generation, 2);
        let mut queue = EventQueue::new(DedupMode::Queue);
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
        acp.request_runtime_final(true);
        acp.initialize().await.unwrap();
        let mut pool = AgentPool::from_slots(vec![Some(owned_worker(0, acp))]);
        pool.set_session_store(Some(store));
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
        return_success(&mut pool, &mut queue).await;
        assert!(pool.has_runtime_hold());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("launches"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        let continuation = pool.runtime_holds[&0].continuation.as_ref().unwrap();
        assert_eq!(continuation.generation, 1);
        assert_eq!(
            continuation
                .provider_recovery
                .as_ref()
                .unwrap()
                .identity
                .generation,
            2
        );
        complete_tasks(&mut pool, &mut queue, &dir).await;
        assert!(pool.runtime_holds[&0].collection_pending);
        if scenario == "collect" {
            assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
            return_success(&mut pool, &mut queue).await;
            assert!(!pool.has_runtime_hold());
        } else {
            let remaining = pool.agents[0]
                .as_ref()
                .unwrap()
                .acp
                .provider_background_record()
                .unwrap()
                .remaining();
            tokio::time::sleep(remaining + Duration::from_millis(20)).await;
            if scenario == "expire_stale_local" {
                let generation = pool.record_scope_owner(scope.clone(), 0);
                pool.agents[0]
                    .as_mut()
                    .unwrap()
                    .state
                    .set_scope_owner_generation(scope.clone(), generation);
            }
            assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 0);
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
            if scenario == "expire_stale_local" {
                assert!(
                    pool.has_runtime_hold(),
                    "expiry must preserve a hold whose local owner changed"
                );
                pool.queue_finished_background_work(&mut queue);
            }
            assert!(
                !pool.has_runtime_hold(),
                "terminal expiry must release gen2 work restored under local gen1"
            );
            pool.queue_finished_background_work(&mut queue);
            assert!(!queue.has_pending_continuation(&scope));
            let other = SessionScope::Thread {
                channel_id: scope.channel_id(),
                root_event_id: "b".repeat(64),
            };
            let event = EventBuilder::new(
                Kind::Custom(9),
                "Start independent work in the other thread",
            )
            .tag(nostr::Tag::parse(["e", &"b".repeat(64), "", "reply"]).unwrap())
            .sign_with_keys(&Keys::generate())
            .unwrap();
            queue.push(QueuedEvent {
                channel_id: other.channel_id(),
                scope: other,
                event,
                received_at: std::time::Instant::now(),
                prompt_tag: "mention".into(),
                edit: None,
            });
            assert_eq!(
                dispatch(&mut pool, &mut queue, &ctx),
                1,
                "the only worker must admit another scope after expiry"
            );
            return_success(&mut pool, &mut queue).await;
        }
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
            if scenario == "collect" { 1 } else { 2 }
        );
        assert_eq!(
            requests
                .iter()
                .filter(|request| request["method"] == "session/resume")
                .count(),
            1
        );
        assert_eq!(
            requests
                .iter()
                .filter(|request| request["method"] == "session/prompt")
                .count(),
            2
        );
        for mut agent in pool.agents.into_iter().flatten() {
            agent.acp.shutdown().await;
        }
    }
}

#[tokio::test]
async fn provider_background_owner_cancel_cannot_be_reenqueued_by_late_completion() {
    let (mut pool, mut queue, scope, dir, ctx) = setup().await;
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    return_success(&mut pool, &mut queue).await;
    crate::handle_owner_cancel_for_scope(
        &mut pool,
        &mut queue,
        &ctx.session_controls,
        &scope,
        None,
        None,
    )
    .await;
    for i in 0..3 {
        std::fs::write(dir.path().join(format!("child-{i}")), "done").unwrap();
    }
    for _ in 0..5 {
        pool.queue_finished_background_work(&mut queue);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        !queue.has_pending_scope(&scope),
        "late provider results cannot resurrect explicit cancellation"
    );
    assert!(!pool.has_runtime_hold());
    let requests: Vec<serde_json::Value> = std::fs::read_to_string(dir.path().join("requests"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let cancel = requests
        .iter()
        .find(|request| request["method"] == "_buzz/background_tasks/cancel")
        .unwrap();
    assert!(
        cancel.get("id").is_some(),
        "provider cancellation requires a durable acknowledgement"
    );
    let store = pool.agents[0]
        .as_ref()
        .unwrap()
        .state
        .store
        .as_ref()
        .unwrap();
    let crate::session_store::LoadOutcome::Found(saved) = store.load(&scope) else {
        panic!("missing cancellation tombstone");
    };
    assert!(saved.live.is_none());
    let receipt = saved.background_cancellation.unwrap();
    assert!(receipt.acknowledged);
    let permission: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join("cancel-permission")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        permission["result"]["outcome"]["outcome"], "cancelled",
        "no late action is approved while cancellation is being acknowledged"
    );
    assert_eq!(receipt.cause, "user_cancel");
    assert_eq!(
        serde_json::json!(receipt.cancel_id),
        cancel["params"]["cancel_id"]
    );
    for mut agent in pool.agents.into_iter().flatten() {
        agent.acp.shutdown().await;
    }
}

#[tokio::test]
async fn provider_background_mismatched_cancel_ack_is_unconfirmed_and_remains_revoked() {
    let (mut pool, mut queue, scope, dir, ctx) = setup().await;
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    return_success(&mut pool, &mut queue).await;
    std::fs::write(dir.path().join("invalid-cancel-ack"), "").unwrap();
    let outcome = crate::handle_owner_cancel_for_scope(
        &mut pool,
        &mut queue,
        &ctx.session_controls,
        &scope,
        None,
        None,
    )
    .await;
    assert_eq!(
        outcome,
        crate::session_controls::status::StatusCancellation::Unconfirmed
    );
    let agent = pool.agents[0].as_ref().unwrap();
    let crate::session_store::LoadOutcome::Found(saved) =
        agent.state.store.as_ref().unwrap().load(&scope)
    else {
        panic!("missing cancellation tombstone");
    };
    assert!(saved.live.is_none());
    assert!(!saved.background_cancellation.unwrap().acknowledged);
    pool.poll_provider_background(&mut queue);
    assert!(!queue.has_pending_scope(&scope));
    assert!(!pool.has_runtime_hold());
    for mut agent in pool.agents.into_iter().flatten() {
        agent.acp.shutdown().await;
    }
}

#[tokio::test]
async fn provider_background_ordinary_input_queues_without_cancelling_the_running_prompt() {
    for mode in [
        crate::config::MultipleEventHandling::Steer,
        crate::config::MultipleEventHandling::Interrupt,
    ] {
        let (mut pool, mut queue, scope, dir, ctx) = setup().await;
        std::fs::write(dir.path().join("block-prompt"), "wait").unwrap();
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
        wait_marker(&dir, "prompt-started").await;
        let original = queue.in_flight_reply_thread(&scope).unwrap().to_owned();
        let event = EventBuilder::new(
            Kind::Custom(9),
            "Use the revised dataset when collecting the existing results",
        )
        .tag(nostr::Tag::parse(["e", original.as_str(), "", "reply"]).unwrap())
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
        let (tx, _rx) = mpsc::unbounded_channel();
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
            &tx,
        );
        assert!(
            pool.task_map.values().next().unwrap().control_tx.is_some(),
            "ordinary input must not cancel a provider without native steering"
        );
        std::fs::remove_file(dir.path().join("block-prompt")).unwrap();
        return_success(&mut pool, &mut queue).await;
        assert_eq!(
            dispatch(&mut pool, &mut queue, &ctx),
            0,
            "clarification waits until result collection"
        );
        complete_tasks(&mut pool, &mut queue, &dir).await;
        assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
        return_success(&mut pool, &mut queue).await;
        assert!(
            last_prompt(&dir)
                .to_string()
                .contains("Use the revised dataset when collecting the existing results"),
            "prompt: {}; queued: {}; requests: {}",
            last_prompt(&dir),
            queue.has_undispatched_work(),
            std::fs::read_to_string(dir.path().join("requests")).unwrap()
        );
        assert!(
            !queue.has_undispatched_work(),
            "the collection must incorporate queued same-route clarification"
        );
        for mut agent in pool.agents.into_iter().flatten() {
            agent.acp.shutdown().await;
        }
    }
}

#[tokio::test]
async fn provider_background_status_reads_state_without_cancelling_or_creating_a_model_turn() {
    let (mut pool, mut queue, scope, _dir, ctx) = setup().await;
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    return_success(&mut pool, &mut queue).await;
    let root = pool.runtime_holds[&0]
        .continuation
        .as_ref()
        .unwrap()
        .thread_tags
        .root_event_id
        .clone()
        .unwrap();
    let make = |text: &str, parent: &str| {
        let event = EventBuilder::new(Kind::Custom(9), text)
            .tag(nostr::Tag::parse(["e", parent, "", "reply"]).unwrap())
            .sign_with_keys(&Keys::generate())
            .unwrap();
        crate::NormalListenerIngress {
            effective_author: event.pubkey.to_hex(),
            buzz_event: crate::relay::BuzzEvent {
                event,
                channel_id: scope.channel_id(),
                connection_generation: 0,
            },
            prompt_tag: "mention".into(),
            edit: None,
        }
    };
    let (text, tags) = make("!status", &root)
        .runtime_status(&pool, &queue, &scope, false)
        .expect("status must read the runtime snapshot");
    assert!(text.contains("3 running"));
    assert_eq!(tags.parent_event_id.as_deref(), Some(root.as_str()));
    assert!(make("quoted !status does not control the task", &root)
        .runtime_status(&pool, &queue, &scope, false)
        .is_none());
    assert!(make("!status", &"f".repeat(64))
        .runtime_status(&pool, &queue, &scope, false)
        .is_none());
    assert!(pool.has_runtime_hold());
    assert!(!queue.has_undispatched_work());
    for mut agent in pool.agents.into_iter().flatten() {
        agent.acp.shutdown().await;
    }
}

#[tokio::test]
async fn provider_background_runtime_cancel_persists_intent_when_outer_fence_drops_prompt() {
    let (mut pool, queue, scope, dir, ctx) = setup().await;
    let mut queue = queue.with_runtime_delivery(dir.path().to_str().unwrap(), 1800);
    std::fs::write(dir.path().join("block-prompt"), "wait").unwrap();
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    wait_marker(&dir, "prompt-started").await;
    crate::handle_owner_cancel_for_scope(
        &mut pool,
        &mut queue,
        &ctx.session_controls,
        &scope,
        None,
        None,
    )
    .await;
    let mut result = timeout(Duration::from_secs(5), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    let store = result.agent.state.store.as_ref().unwrap();
    let crate::session_store::LoadOutcome::Found(saved) = store.load(&scope) else {
        panic!("missing cancellation record");
    };
    std::fs::remove_file(dir.path().join("block-prompt")).unwrap();
    result.agent.acp.shutdown().await;
    let cancellation = saved.background_cancellation.expect(
        "outer cancellation must persist the provider intent before invalidating its lease",
    );
    assert_eq!(cancellation.cause, "user_cancel");
    assert!(
        !cancellation.acknowledged,
        "blocked provider cannot confirm cancellation"
    );
}

#[path = "provider_background_review_tests.rs"]
mod review_tests;
