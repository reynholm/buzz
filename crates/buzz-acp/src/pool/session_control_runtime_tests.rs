use super::*;

// Session-control regressions construct every job through the existing author
// capability; no raw event can call the controller's effect API.
async fn authorized_control(
    ctx: &PromptContext,
    human: &Keys,
    scope: &SessionScope,
    content: &str,
) -> crate::session_controls::ControlJob {
    let event = EventBuilder::new(Kind::Custom(9), content)
        .tags([
            Tag::parse(["h", scope.channel_id().to_string().as_str()]).unwrap(),
            Tag::parse([
                "e",
                scope.root_event_id().unwrap_or(&"a".repeat(64)),
                "",
                "reply",
            ])
            .unwrap(),
            Tag::parse(["p", ctx.agent_keys.public_key().to_hex().as_str()]).unwrap(),
        ])
        .sign_with_keys(human)
        .unwrap();
    let owner = crate::OwnerCache::new(Some(human.public_key().to_hex()));
    let mut gate = crate::InboundAuthorGate::connect(
        &ctx.rest_client,
        &ctx.agent_keys.public_key().to_hex(),
        "control test",
    )
    .await;
    let authorized = crate::authorize_normal_listener_event(
        &mut gate,
        crate::relay::BuzzEvent {
            channel_id: scope.channel_id(),
            event,
            connection_generation: 0,
        },
        &crate::RespondTo::OwnerOnly,
        &Default::default(),
        &owner,
        &ctx.channel_info,
        &ctx.rest_client,
    )
    .await
    .unwrap();
    match crate::session_controls::admit_authorized(
        authorized,
        ctx,
        nostr::Timestamp::now().as_secs(),
    )
    .await
    {
        crate::session_controls::Admission::Control(job) => job,
        _ => panic!("authorized control must bypass subscription matching"),
    }
}

#[tokio::test]
async fn restart_during_delayed_acquisition_fences_old_dispatch() {
    let fixture = Fixture::new();
    let scope = scope();
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    let job = authorized_control(&ctx, &human, &scope, "!restart all").await;
    let controls = ctx.session_controls.clone();
    let mut agent = fixture.agent(&[("DELAY", "0.3")]).await;
    agent.state.control_dispatch = Some(controls.capture(&scope));
    let log = fixture.log();
    let restart = async {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !std::fs::read_to_string(&log)
                .unwrap_or_default()
                .contains("session/new")
            {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert!(controls.restart(job).unwrap());
    };
    let acquisition = acquire_session(&mut agent, &ctx, None, channel_context(&scope), &scope);
    let (result, ()) = tokio::join!(acquisition, restart);
    assert!(
        result.is_err(),
        "old acquisition must not retry and claim the new tombstone"
    );
    assert_eq!(
        fixture
            .methods()
            .iter()
            .filter(|method| *method == "session/new")
            .count(),
        1
    );
    let record = fixture.record(&scope);
    assert!(record.live.is_none());
    assert_eq!(
        record.pending_restart.unwrap().depth,
        buzz_core::session_control::RestartDepth::All
    );
    agent.acp.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn second_restart_survives_old_result() {
    let fixture = Fixture::new();
    let scope = scope();
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    let first = authorized_control(&ctx, &human, &scope, "!restart none").await;
    assert!(ctx.session_controls.restart(first.clone()).unwrap());
    let old = ctx.session_controls.capture(&scope);
    let late_ack = old.clone();
    old.clone().finish();
    assert!(
        late_ack.current(),
        "ordinary task completion must preserve a still-owned late ACK epoch"
    );
    let second = authorized_control(&ctx, &human, &scope, "!restart all").await;
    assert!(ctx.session_controls.restart(second).unwrap());
    let latest = ctx.session_controls.restart_intent(&scope).unwrap();
    old.foreground_success();
    assert!(!old.current());
    assert!(
        !ctx.session_controls.restart(first).unwrap(),
        "duplicate must not reset policy"
    );
    assert_eq!(
        ctx.session_controls.restart_intent(&scope),
        Some(latest.clone())
    );
    let mut agent = fixture.agent(&[]).await;
    agent.state.control_dispatch = Some(old);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    crate::pool::send_prompt_result(
        &tx,
        "old",
        agent,
        crate::pool::PromptSource::Channel(scope.clone()),
        PromptOutcome::Ok(StopReason::EndTurn),
        Some(batch(
            &scope,
            mention(
                &human,
                scope.channel_id(),
                scope.root_event_id(),
                "old trigger",
            ),
        )),
    );
    let mut result = rx.recv().await.unwrap();
    assert!(matches!(result.outcome, PromptOutcome::Cancelled));
    assert!(
        result.batch.is_none(),
        "retired triggering batch cannot requeue into a newer restart"
    );
    assert_eq!(ctx.session_controls.restart_intent(&scope), Some(latest));
    result.agent.acp.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn selected_restart_depths_reach_actual_prompt_and_supplied_ids() {
    use buzz_core::session_control::RestartDepth;
    for (command, depth, limit, is_dm) in [
        ("!restart none", RestartDepth::None, 0, false),
        ("!restart all", RestartDepth::All, 1, false),
        ("!restart", RestartDepth::Recent, 2, false),
        ("!restart all", RestartDepth::All, 0, true),
    ] {
        let fixture = Fixture::new();
        let human = Keys::generate();
        let channel = Uuid::from_u128(42);
        let root = if is_dm {
            EventBuilder::new(Kind::Custom(9), "ROOT-CHATTER")
                .tags([Tag::parse(["t", "dm"]).unwrap()])
                .sign_with_keys(&human)
                .unwrap()
        } else {
            mention(&human, channel, None, "ROOT-CHATTER")
        };
        let scope = if is_dm {
            SessionScope::Conversation {
                channel_id: channel,
            }
        } else {
            SessionScope::Thread {
                channel_id: channel,
                root_event_id: root.id.to_hex(),
            }
        };
        let (mut ctx, initial_server) = relay_context(channel, vec![]).await;
        let target = ctx.agent_keys.public_key().to_hex();
        let assignment = EventBuilder::new(Kind::Custom(9), "REAL-ASSIGNMENT")
            .tags([
                Tag::parse(["e", root.id.to_hex().as_str(), "", "reply"]).unwrap(),
                Tag::parse(["p", target.as_str()]).unwrap(),
            ])
            .custom_created_at(nostr::Timestamp::from(root.created_at.as_secs() + 1))
            .sign_with_keys(&human)
            .unwrap();
        let control = EventBuilder::new(Kind::Custom(9), "!restart all")
            .tags([
                Tag::parse(["e", root.id.to_hex().as_str(), "", "reply"]).unwrap(),
                Tag::parse(["p", target.as_str()]).unwrap(),
            ])
            .sign_with_keys(&human)
            .unwrap();
        let chatter = mention(&human, channel, scope.root_event_id(), "LATER-CHATTER");
        let malformed = mention(&human, channel, scope.root_event_id(), "!restart foo");
        let huge = mention(
            &human,
            channel,
            scope.root_event_id(),
            &"LARGE-OMITTED".repeat(30_000),
        );
        let events = vec![
            root.clone(),
            assignment.clone(),
            control.clone(),
            chatter.clone(),
            malformed,
            huge.clone(),
        ];
        let (history_ctx, server) = relay_context(channel, events).await;
        initial_server.abort();
        ctx.rest_client = history_ctx.rest_client;
        ctx.channel_info = if is_dm {
            ChannelInfoResolver::new(
                HashMap::from([(
                    channel,
                    crate::relay::ChannelInfo {
                        name: "dm".into(),
                        channel_type: "dm".into(),
                        description: None,
                    },
                )]),
                ctx.rest_client.clone(),
            )
        } else {
            history_ctx.channel_info
        };
        ctx.context_message_limit = limit;
        ctx.session_policy = crate::scope::SessionPolicy::Thread;
        ctx.session_controls =
            crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
        let job = authorized_control(&ctx, &human, &scope, command).await;
        let origin = job.origin.clone();
        let status = crate::session_status::SessionStatusRuntime::open(
            fixture.dir.path(),
            buzz_core::session_status::canonical_identity(
                "wss://relay.example",
                &ctx.agent_keys.public_key().to_hex(),
            )
            .unwrap(),
        )
        .unwrap();
        ctx.session_status = Some(status.clone());
        assert!(ctx.session_controls.restart(job).unwrap());
        ctx.session_controls.release_interim(&scope);
        let prompts = fixture.dir.path().join("prompts.ndjson");
        let agent = fixture
            .agent(&[("PROMPTS", prompts.to_str().unwrap())])
            .await;
        let trigger = mention(&human, channel, scope.root_event_id(), "CURRENT-TRIGGER");
        let ctx = Arc::new(ctx);
        let mut agent = run_turn(agent, &ctx, batch(&scope, trigger.clone())).await;
        let wire = std::fs::read_to_string(&prompts).unwrap();
        let display_scope = crate::pool::session_visibility::status_scope(&scope).unwrap();
        let snapshot = std::fs::read(
            status
                .dir()
                .join(buzz_core::session_status::scope_file_name(&display_scope).unwrap()),
        )
        .unwrap();
        let snapshot = buzz_core::session_status::parse_snapshot(&snapshot).unwrap();
        assert_eq!(
            snapshot.status.state,
            buzz_core::session_status::SessionState::Restarted
        );
        assert_eq!(snapshot.origin, Some(origin));

        assert!(wire.contains("REAL-ASSIGNMENT"));
        assert!(wire.contains("CURRENT-TRIGGER"));
        assert!(!wire.contains("!restart all"));
        let delivered = &agent.state.deliveries[&scope].delivered_event_ids;
        assert!(delivered.contains(&assignment.id.to_hex()));
        assert!(!delivered.contains(&control.id.to_hex()));
        if depth != RestartDepth::Recent {
            assert!(!wire.contains("LARGE-OMITTED"));
            assert!(!delivered.contains(&huge.id.to_hex()));
        }
        if depth == RestartDepth::None {
            assert!(!wire.contains("ROOT-CHATTER"));
            assert!(!wire.contains("LATER-CHATTER"));
            assert_eq!(
                delivered,
                &std::collections::HashSet::from([assignment.id.to_hex(), trigger.id.to_hex()])
            );
        }
        if depth == RestartDepth::All {
            assert!(wire.contains("!restart foo"));
        }
        assert!(
            ctx.session_controls.restart_intent(&scope).is_none(),
            "only actual foreground success consumes the captured token"
        );
        agent.acp.shutdown().await;
        server.abort();
    }
}

#[tokio::test]
async fn controls_use_author_capability_before_subscription_and_reserve_wrong_targets() {
    use crate::session_controls::{admit_authorized, Admission};
    use crate::RespondTo;
    for (policy, is_dm, is_owner, sibling, allow, expected) in [
        (RespondTo::OwnerOnly, false, true, false, false, true),
        (RespondTo::OwnerOnly, false, false, true, false, true),
        (RespondTo::OwnerOnly, false, false, false, false, false),
        (RespondTo::Allowlist, false, false, false, true, true),
        (RespondTo::Anyone, false, false, false, false, true),
        (RespondTo::Nobody, false, true, false, true, false),
        (RespondTo::Anyone, true, false, false, true, false),
        (RespondTo::Anyone, true, true, false, false, true),
        (RespondTo::Nobody, true, true, false, false, false),
    ] {
        let fixture = Fixture::new();
        let scope = scope();
        let human = Keys::generate();
        let owner_key = if is_owner {
            human.public_key().to_hex()
        } else {
            Keys::generate().public_key().to_hex()
        };
        let (mut ctx, server) = relay_context(scope.channel_id(), vec![]).await;
        ctx.session_controls =
            crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
        if is_dm {
            ctx.channel_info = ChannelInfoResolver::new(
                HashMap::from([(
                    scope.channel_id(),
                    crate::relay::ChannelInfo {
                        name: "dm".into(),
                        channel_type: "dm".into(),
                        description: None,
                    },
                )]),
                ctx.rest_client.clone(),
            );
        }
        let owner = crate::OwnerCache::new(Some(owner_key.clone()));
        owner.cache_sibling(human.public_key().to_hex(), sibling);
        let target = ctx.agent_keys.public_key().to_hex();
        let mut gate =
            crate::InboundAuthorGate::connect(&ctx.rest_client, &target, "admission test").await;
        let event = EventBuilder::new(Kind::Custom(9), "!restart none")
            .tags([
                Tag::parse(["p", target.as_str()]).unwrap(),
                // Forged workflow author tags cannot bypass the raw-author policy.
                Tag::parse(["buzz:workflow", "true"]).unwrap(),
                Tag::parse(["buzz:workflow-owner", owner_key.as_str()]).unwrap(),
                Tag::parse(["buzz:workflow-mention", target.as_str()]).unwrap(),
            ])
            .sign_with_keys(&human)
            .unwrap();
        let allowlist = if allow {
            std::collections::HashSet::from([human.public_key().to_hex()])
        } else {
            Default::default()
        };
        let auth = crate::authorize_normal_listener_event(
            &mut gate,
            crate::relay::BuzzEvent {
                event,
                channel_id: scope.channel_id(),
                connection_generation: 0,
            },
            &policy,
            &allowlist,
            &owner,
            &ctx.channel_info,
            &ctx.rest_client,
        )
        .await;
        assert_eq!(auth.is_some(), expected, "policy {policy:?} dm={is_dm}");
        if let Some(auth) = auth {
            assert!(
                matches!(
                    admit_authorized(auth, &ctx, nostr::Timestamp::now().as_secs()).await,
                    Admission::Control(_)
                ),
                "no subscription rule is required for an authorized control"
            );
        }
        assert!(
            matches!(fixture.store.load(&scope), LoadOutcome::Missing),
            "authorization alone causes zero durable mutation"
        );
        server.abort();
    }
}

#[tokio::test]
async fn stale_replay_does_not_overwrite_completed_ack() {
    use crate::session_controls::{admit_authorized, Admission};
    let fixture = Fixture::new();
    let scope = scope();
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    let job = authorized_control(&ctx, &human, &scope, "!restart").await;
    let event = job.event.clone();
    ctx.session_controls.restart(job.clone()).unwrap();
    fixture
        .store
        .finish_control_attempt(&scope, &event.id.to_hex(), &Ok(()))
        .unwrap();
    let before = fixture.record(&scope);
    let owner = crate::OwnerCache::new(Some(human.public_key().to_hex()));
    let mut gate = crate::InboundAuthorGate::connect(
        &ctx.rest_client,
        &ctx.agent_keys.public_key().to_hex(),
        "replay test",
    )
    .await;
    for now in [
        event.created_at.as_secs() + 901,
        event.created_at.as_secs() - 31,
    ] {
        let auth = crate::authorize_normal_listener_event(
            &mut gate,
            crate::relay::BuzzEvent {
                event: event.clone(),
                channel_id: scope.channel_id(),
                connection_generation: 0,
            },
            &crate::RespondTo::OwnerOnly,
            &Default::default(),
            &owner,
            &ctx.channel_info,
            &ctx.rest_client,
        )
        .await
        .unwrap();
        assert!(matches!(
            admit_authorized(auth, &ctx, now).await,
            Admission::Ignored
        ));
    }
    let after = fixture.record(&scope);
    assert_eq!(before.pending_restart, after.pending_restart);
    assert_eq!(before.control_receipts, after.control_receipts);
    // Same syntax addressed to a human is reserved but cannot enter broad rules.
    let wrong = EventBuilder::new(Kind::Custom(9), "!restart")
        .tags([Tag::parse(["p", Keys::generate().public_key().to_hex().as_str()]).unwrap()])
        .sign_with_keys(&human)
        .unwrap();
    let auth = crate::authorize_normal_listener_event(
        &mut gate,
        crate::relay::BuzzEvent {
            event: wrong.clone(),
            channel_id: scope.channel_id(),
            connection_generation: 0,
        },
        &crate::RespondTo::OwnerOnly,
        &Default::default(),
        &owner,
        &ctx.channel_info,
        &ctx.rest_client,
    )
    .await
    .unwrap();
    assert!(matches!(
        admit_authorized(auth, &ctx, nostr::Timestamp::now().as_secs()).await,
        Admission::Ignored
    ));
    let rules: Vec<crate::filter::SubscriptionRule> = serde_json::from_str(
        r#"[{"name":"broad","channels":"all","kinds":[],"require_mention":false}]"#,
    )
    .unwrap();
    assert!(crate::filter::match_event(
        &wrong,
        scope.channel_id(),
        &rules,
        &ctx.agent_keys.public_key().to_hex()
    )
    .await
    .is_none());
    server.abort();
}

#[tokio::test]
async fn busy_one_shot_already_consumed_keeps_restart_gate_and_newer_queue() {
    let fixture = Fixture::new();
    let scope = scope();
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    let old_dispatch = ctx.session_controls.capture(&scope);
    let job = authorized_control(&ctx, &human, &scope, "!restart none").await;
    ctx.session_controls.restart(job).unwrap();
    let idle = fixture.agent(&[]).await;
    let mut pool = AgentPool::from_slots(vec![Some(idle), None]);
    let handle = pool.join_set.spawn(std::future::pending::<()>());
    pool.task_map_mut().insert(
        handle.id(),
        crate::pool::TaskMeta {
            agent_index: 1,
            channel_id: Some(scope.channel_id()),
            scope: Some(scope.clone()),
            turn_id: "busy-old".into(),
            recoverable_batch: None,
            control_tx: None,
            steer_tx: None,
            control_dispatch: Some(old_dispatch),
            successful_steer_deliveries: Default::default(),
        },
    );
    assert!(!crate::signal_in_flight_task_for_scope(
        &mut pool,
        &scope,
        crate::pool::ControlSignal::Rotate
    ));
    let newer = mention(
        &human,
        scope.channel_id(),
        scope.root_event_id(),
        "ordinary newer event",
    );
    let mut queue = crate::EventQueue::new(crate::DedupMode::Queue);
    queue.push(crate::QueuedEvent {
        scope: scope.clone(),
        channel_id: scope.channel_id(),
        event: newer.clone(),
        received_at: std::time::Instant::now(),
        prompt_tag: "test".into(),
        edit: None,
    });
    let ctx = Arc::new(ctx);
    let mut activity = tokio::time::Instant::now();
    assert!(crate::dispatch_pending(&mut pool, &mut queue, &ctx, &mut activity, None).is_empty());
    assert!(ctx.session_controls.retirement_pending(&scope));
    assert!(queue.has_undispatched_work());
    let mut idle = pool.try_claim(None).unwrap();
    idle.acp.shutdown().await;
    pool.task_map_mut().clear();
    handle.abort();
    crate::dispatch_pending(&mut pool, &mut queue, &ctx, &mut activity, None);
    assert!(!ctx.session_controls.retirement_pending(&scope));
    let batch = queue.flush_next().unwrap();
    assert_eq!(batch.events[0].event.id, newer.id);
    server.abort();
}

#[tokio::test]
async fn bootstrap_success_and_failed_foreground_preserve_restart_policy() {
    let fixture = Fixture::new();
    let scope = scope();
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.initial_message = Some("configured bootstrap".into());
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    let job = authorized_control(&ctx, &human, &scope, "!restart none").await;
    ctx.session_controls.restart(job).unwrap();
    let intent = ctx.session_controls.restart_intent(&scope).unwrap();
    let agent = fixture.agent(&[("FAIL_PROMPT", "2")]).await;
    let ctx = Arc::new(ctx);
    let trigger = mention(
        &human,
        scope.channel_id(),
        scope.root_event_id(),
        "foreground task",
    );
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    run_prompt_task(
        agent,
        Some(batch(&scope, trigger.clone())),
        None,
        ctx.clone(),
        tx,
        None,
        "failed-first".into(),
    )
    .await;
    let failed = rx.recv().await.unwrap();
    assert!(matches!(failed.outcome, PromptOutcome::Error(_)));
    assert_eq!(ctx.session_controls.restart_intent(&scope), Some(intent));
    let mut agent = run_turn(failed.agent, &ctx, batch(&scope, trigger)).await;
    assert!(ctx.session_controls.restart_intent(&scope).is_none());
    agent.acp.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn no_resume_retains_only_control_policy_and_channel_revoke_fences_it() {
    let fixture = Fixture::new();
    let scope = scope();
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    let job = authorized_control(&ctx, &human, &scope, "!restart none").await;
    ctx.session_controls.restart(job).unwrap();
    let controller = ctx.session_controls.clone();
    // Pool replacement has no ownership of the controller or its control-only store.
    drop(AgentPool::from_slots(vec![None]));
    assert!(controller.restart_intent(&scope).is_some());
    let mut agent = fixture.agent(&[]).await;
    agent.state.store = None;
    let mut agent = run_turn(
        agent,
        &Arc::new(ctx),
        batch(
            &scope,
            mention(
                &human,
                scope.channel_id(),
                scope.root_event_id(),
                "new foreground",
            ),
        ),
    )
    .await;
    let record = fixture.record(&scope);
    assert!(
        record.live.is_none(),
        "no provider ID/delivery may enter the control-only store"
    );
    assert!(record.pending_restart.is_none());
    assert_eq!(record.control_receipts.len(), 1);
    let stale = controller.capture(&scope);
    controller.revoke_channel(scope.channel_id()).unwrap();
    assert!(!stale.current());
    assert!(controller.restart_intent(&scope).is_none());
    agent.acp.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn late_cancelled_result_cannot_requeue_old_batch_after_restart() {
    let fixture = Fixture::new();
    let scope = scope();
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    let old_dispatch = ctx.session_controls.capture(&scope);
    let mut queue = crate::EventQueue::new(crate::DedupMode::Queue);
    let old = mention(
        &human,
        scope.channel_id(),
        scope.root_event_id(),
        "old triggering event",
    );
    queue.push(crate::QueuedEvent {
        scope: scope.clone(),
        channel_id: scope.channel_id(),
        event: old,
        received_at: std::time::Instant::now(),
        prompt_tag: "test".into(),
        edit: None,
    });
    let old_batch = queue.flush_next().unwrap();
    let job = authorized_control(&ctx, &human, &scope, "!restart none").await;
    ctx.session_controls.restart(job).unwrap();
    let newer = mention(
        &human,
        scope.channel_id(),
        scope.root_event_id(),
        "newer queued event",
    );
    queue.push(crate::QueuedEvent {
        scope: scope.clone(),
        channel_id: scope.channel_id(),
        event: newer.clone(),
        received_at: std::time::Instant::now(),
        prompt_tag: "test".into(),
        edit: None,
    });
    let mut agent = fixture.agent(&[]).await;
    agent.state.control_dispatch = Some(old_dispatch.clone());
    let mut pool = AgentPool::from_slots(vec![None]);
    let id = pool.join_set.spawn(std::future::pending::<()>()).id();
    pool.task_map_mut().insert(
        id,
        crate::pool::TaskMeta {
            agent_index: 0,
            channel_id: Some(scope.channel_id()),
            scope: Some(scope.clone()),
            turn_id: "old".into(),
            recoverable_batch: Some(old_batch.clone()),
            control_tx: None,
            steer_tx: None,
            control_dispatch: Some(old_dispatch),
            successful_steer_deliveries: Default::default(),
        },
    );
    let result = crate::pool::PromptResult {
        agent,
        source: crate::pool::PromptSource::Channel(scope.clone()),
        turn_id: "old".into(),
        outcome: PromptOutcome::Cancelled,
        batch: Some(old_batch),
    };
    let (respawn_tx, _) = tokio::sync::mpsc::channel(8);
    let mut crash_history = vec![crate::SlotCircuit {
        crash_times: vec![],
        open_until: None,
        respawn_in_flight: false,
    }];
    let mut respawns = tokio::task::JoinSet::new();
    let mut heartbeat = false;
    let mut activity = tokio::time::Instant::now();
    crate::handle_prompt_result(
        &mut pool,
        &mut queue,
        &crate::error_outcome_emission_tests::test_config(),
        result,
        &mut heartbeat,
        &Default::default(),
        &mut crash_history,
        &respawn_tx,
        &mut respawns,
        None,
        None,
        &mut activity,
    );
    let next = queue.flush_next().unwrap();
    assert!(
        next.cancelled_events.is_empty(),
        "old cancelled trigger must not merge into the new generation"
    );
    assert_eq!(next.events.len(), 1);
    assert_eq!(next.events[0].event.id, newer.id);
    let mut returned = pool.try_claim(None).unwrap();
    returned.acp.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn unknown_thread_metadata_refuses_control_without_changing_normal_messages() {
    use crate::session_controls::{admit_authorized, Admission};
    let fixture = Fixture::new();
    let scope = scope();
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.channel_info = ChannelInfoResolver::new(Default::default(), ctx.rest_client.clone());
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    let target = ctx.agent_keys.public_key().to_hex();
    let owner = crate::OwnerCache::new(Some(human.public_key().to_hex()));
    let mut gate =
        crate::InboundAuthorGate::connect(&ctx.rest_client, &target, "unknown scope test").await;
    for content in [
        "!restart none",
        "!restart foo",
        "!cancel",
        "!shutdown",
        "ordinary stale message",
    ] {
        let event = EventBuilder::new(Kind::Custom(9), content)
            .tags([Tag::parse(["p", target.as_str()]).unwrap()])
            .sign_with_keys(&human)
            .unwrap();
        let auth = crate::authorize_normal_listener_event(
            &mut gate,
            crate::relay::BuzzEvent {
                event,
                channel_id: scope.channel_id(),
                connection_generation: 0,
            },
            &crate::RespondTo::OwnerOnly,
            &Default::default(),
            &owner,
            &ctx.channel_info,
            &ctx.rest_client,
        )
        .await
        .unwrap();
        let admission = admit_authorized(
            auth,
            &ctx,
            nostr::Timestamp::now().as_secs()
                + if content == "ordinary stale message" {
                    9001
                } else {
                    0
                },
        )
        .await;
        if content == "!restart none" {
            assert!(matches!(
                admission,
                Admission::Refused {
                    failure: buzz_core::session_control::ControlFailure::ScopeUnavailable,
                    ..
                }
            ));
        } else {
            assert!(matches!(admission, Admission::Normal(_)));
        }
    }
    assert!(matches!(fixture.store.load(&scope), LoadOutcome::Missing));
    server.abort();
}

#[tokio::test]
async fn restart_during_failed_resume_never_calls_fallback_new() {
    let fixture = Fixture::new();
    let scope = scope();
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    let status = crate::session_status::SessionStatusRuntime::open(
        fixture.dir.path(),
        buzz_core::session_status::canonical_identity(
            "wss://relay.example",
            &ctx.agent_keys.public_key().to_hex(),
        )
        .unwrap(),
    )
    .unwrap();
    let display_scope = crate::pool::session_visibility::status_scope(&scope).unwrap();
    let snapshot = status
        .dir()
        .join(buzz_core::session_status::scope_file_name(&display_scope).unwrap());
    ctx.session_status = Some(status.clone());
    fixture.departed_record(&scope, &ctx, &[]);
    let job = authorized_control(&ctx, &human, &scope, "!restart all").await;
    let controls = ctx.session_controls.clone();
    let mut agent = fixture
        .agent(&[("DELAY", "0.3"), ("RESUME_CAP", "1"), ("RESUME_ERROR", "1")])
        .await;
    let old_binding = status
        .begin(
            display_scope,
            "old-1",
            buzz_core::session_status::SessionState::Resumed,
            buzz_core::session_status::StatusReason::IdleSleep,
        )
        .unwrap();
    agent.state.status.insert(scope.clone(), old_binding);
    agent.state.control_dispatch = Some(controls.capture(&scope));
    let log = fixture.log();
    let ctx = Arc::new(ctx);
    let restart = async {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !std::fs::read_to_string(&log)
                .unwrap_or_default()
                .contains("session/resume")
            {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        controls.restart(job).unwrap();
        // Same retirement seam used immediately after admission by the listener.
        crate::pool::session_visibility::retire_scope(ctx.session_status.as_ref(), &scope);
        controls.restart_intent(&scope).unwrap()
    };
    let trigger = mention(
        &human,
        scope.channel_id(),
        scope.root_event_id(),
        "old task",
    );
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let (_, accepted) = tokio::join!(
        run_prompt_task(
            agent,
            Some(batch(&scope, trigger)),
            None,
            ctx.clone(),
            tx,
            None,
            "failed-resume".into()
        ),
        restart
    );
    let mut result = rx.recv().await.unwrap();
    result.agent.acp.shutdown().await;
    server.abort();
    assert!(matches!(result.outcome, PromptOutcome::Cancelled));
    assert!(
        !snapshot.exists(),
        "late acquisition cannot publish a replacement for the retired status"
    );
    let record = fixture.record(&scope);
    assert!(record.live.is_none());
    assert_eq!(record.pending_restart, Some(accepted));
    assert!(
        !fixture
            .methods()
            .iter()
            .any(|method| method == "session/new"),
        "an old resume error cannot initiate a new provider RPC after restart"
    );
}

// Answer history and profile queries separately so the production profile lookup
// can move an otherwise fitting assignment across the rendered history budget.
async fn profiled_history_context(
    channel: Uuid,
    events: Vec<nostr::Event>,
    profile: nostr::Event,
) -> (PromptContext, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut request = Vec::new();
            let header_end = loop {
                let mut chunk = [0; 8192];
                let n = socket.read(&mut chunk).await.unwrap();
                if n == 0 {
                    return;
                }
                request.extend_from_slice(&chunk[..n]);
                if let Some(at) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    break at + 4;
                }
            };
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|n| n.parse::<usize>().ok())
                })
                .unwrap_or(0);
            while request.len() < header_end + length {
                let mut chunk = [0; 8192];
                let n = socket.read(&mut chunk).await.unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..n]);
            }
            let filters = serde_json::from_slice::<Vec<serde_json::Value>>(&request[header_end..])
                .unwrap_or_default();
            let body = if filters.iter().any(|f| {
                f["kinds"]
                    .as_array()
                    .is_some_and(|k| k.contains(&serde_json::json!(0)))
            }) {
                serde_json::to_vec(&vec![profile.clone()]).unwrap()
            } else if filters.iter().any(|f| {
                f["kinds"]
                    .as_array()
                    .is_some_and(|k| k.contains(&serde_json::json!(9)))
            }) {
                serde_json::to_vec(&events).unwrap()
            } else {
                b"[]".to_vec()
            };
            let header = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            if socket.write_all(header.as_bytes()).await.is_ok() {
                let _ = socket.write_all(&body).await;
            }
        }
    });
    let mut ctx = ctx();
    ctx.rest_client.base_url = base_url;
    ctx.channel_info = ChannelInfoResolver::new(
        HashMap::from([(
            channel,
            crate::relay::ChannelInfo {
                name: "thread".into(),
                channel_type: "stream".into(),
                description: None,
            },
        )]),
        ctx.rest_client.clone(),
    );
    (ctx, server)
}

#[tokio::test]
async fn none_assignment_omitted_after_profile_render_is_explicitly_not_found() {
    let fixture = Fixture::new();
    let human = Keys::generate();
    let channel = Uuid::from_u128(42);
    let agent_keys = Keys::generate();
    let root = mention(&human, channel, None, "ROOT-CHATTER");
    let root_id = root.id.to_hex();
    let scope = SessionScope::Thread {
        channel_id: channel,
        root_event_id: root_id.clone(),
    };
    let assignment = |content: &str| {
        EventBuilder::new(Kind::Custom(9), content)
            .tags([
                Tag::parse(["e", root_id.as_str(), "", "reply"]).unwrap(),
                Tag::parse(["p", agent_keys.public_key().to_hex().as_str()]).unwrap(),
            ])
            .sign_with_keys(&human)
            .unwrap()
    };
    let stub = assignment("x");
    let context = crate::queue::ConversationContext::Thread {
        messages: vec![
            crate::pool::json_to_context_message(&serde_json::to_value(stub).unwrap()).unwrap(),
        ],
        total: 2,
        root_present: false,
        truncated: true,
    };
    let overhead = crate::queue::format_conversation_context(&context, None).len() - 1;
    let body = format!(
        "BOUNDARY-ASSIGNMENT{}",
        "x".repeat(
            crate::pool::session_history::HISTORY_BYTES
                - overhead
                - 40
                - "BOUNDARY-ASSIGNMENT".len()
        )
    );
    let assignment = assignment(&body);
    let profile = EventBuilder::new(
        Kind::Metadata,
        serde_json::json!({"display_name":"n".repeat(64)}).to_string(),
    )
    .sign_with_keys(&human)
    .unwrap();
    let (mut ctx, server) =
        profiled_history_context(channel, vec![root, assignment.clone()], profile).await;
    ctx.agent_keys = agent_keys;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.context_message_limit = 0;
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    let job = authorized_control(&ctx, &human, &scope, "!restart none").await;
    assert!(ctx.session_controls.restart(job).unwrap());
    ctx.session_controls.release_interim(&scope);
    let prompts = fixture.dir.path().join("boundary-prompts.ndjson");
    let agent = fixture
        .agent(&[("PROMPTS", prompts.to_str().unwrap())])
        .await;
    let trigger = mention(&human, channel, Some(&root_id), "CURRENT-TRIGGER");
    let ctx = Arc::new(ctx);
    let mut agent = run_turn(agent, &ctx, batch(&scope, trigger.clone())).await;
    let wire = std::fs::read_to_string(prompts).unwrap();
    assert!(
        !wire.contains("BOUNDARY-ASSIGNMENT"),
        "the resolved actor label pushes this assignment over the rendered limit"
    );
    assert!(wire.contains("CURRENT-TRIGGER"));
    assert!(
        wire.contains("first_mention=\\\"not_found\\\""),
        "an omitted assignment must be explicit in the actual prompt: {}",
        &wire[..wire.len().min(500)]
    );
    let supplied = &agent
        .state
        .deliveries
        .get(&scope)
        .unwrap()
        .delivered_event_ids;
    assert!(!supplied.contains(&assignment.id.to_hex()));
    assert!(supplied.contains(&trigger.id.to_hex()));
    agent.acp.shutdown().await;
    server.abort();
}
