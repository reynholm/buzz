use super::*;
use buzz_core::session_control::{ControlFailure, ControlResult};

async fn setup() -> (
    Fixture,
    Arc<PromptContext>,
    crate::pool::AgentPool,
    Keys,
    tokio::task::JoinHandle<()>,
) {
    let fixture = Fixture::new();
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(scope().channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    let mut agent = fixture.agent(&[("COMPACT_STATUS", "completed")]).await;
    let (state, _) = owned_state(&fixture, &scope(), "old-1");
    agent.state = state;
    let mut pool = crate::pool::AgentPool::from_slots(vec![Some(agent)]);
    let generation = pool.record_scope_owner(scope(), 0);
    let mut agent = pool.try_claim(Some(&scope())).unwrap();
    agent.state.set_scope_owner_generation(scope(), generation);
    agent.acp.ensure_compaction_route("old-1");
    agent.acp.advertise_compaction("old-1");
    pool.return_agent(agent);
    (fixture, Arc::new(ctx), pool, human, server)
}

async fn enqueue(
    ctx: &PromptContext,
    pool: &crate::pool::AgentPool,
    human: &Keys,
    scope: &SessionScope,
) -> crate::session_controls::ControlJob {
    let job = authorized_control(ctx, human, scope, "!compact").await;
    let target = pool.compaction_target(scope, &ctx.session_controls, true);
    ctx.session_controls
        .queue_compaction(job.clone(), target)
        .unwrap();
    job
}

#[tokio::test]
async fn compact_waits_for_exact_busy_owner_without_cancel_or_fork() {
    let (fixture, ctx, mut pool, human, server) = setup().await;
    let owner = pool.try_claim(Some(&scope())).unwrap();
    let request = enqueue(&ctx, &pool, &human, &scope()).await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    crate::control_tasks::dispatch(&mut pool, &ctx, &tx);
    assert!(rx.try_recv().is_err());
    assert!(pool.task_map().is_empty());
    assert_eq!(
        ctx.session_controls
            .take_publications()
            .iter()
            .filter(|publication| {
                publication.command_id == request.origin.command_event_id
                    && publication.result == ControlResult::Pending
            })
            .count(),
        1,
        "a busy compact reports its wait instead of looking unconfirmed"
    );
    let admitted_deadline = ctx.session_controls.pending_compactions()[0].deadline;
    ctx.session_controls
        .queue_compaction(
            request.clone(),
            pool.compaction_target(&scope(), &ctx.session_controls, true),
        )
        .unwrap();
    assert_eq!(
        ctx.session_controls.pending_compactions()[0].deadline,
        admitted_deadline,
        "redelivery must not extend queue admission"
    );
    assert_eq!(
        ctx.session_controls.take_publications()[0].result,
        ControlResult::Pending,
        "redelivery can retry the wait receipt without executing compact"
    );
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(899)).await;
    tokio::time::resume();
    crate::control_tasks::dispatch(&mut pool, &ctx, &tx);
    assert!(
        ctx.session_controls.take_publications().is_empty(),
        "waiting is reported once"
    );
    assert!(ctx.session_controls.compact_pending(&scope()));
    assert!(!fixture.methods().contains(&"session/cancel".into()));
    assert!(!fixture.methods().contains(&"session/new".into()));
    let dispatch_started = tokio::time::Instant::now();
    pool.return_agent(owner);
    crate::control_tasks::dispatch(&mut pool, &ctx, &tx);
    let result = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.job.job.origin, request.origin);
    let execution_wait = crate::session_controls::compaction::COMPACT_EXECUTION_WAIT;
    assert!(result.job.deadline >= dispatch_started + execution_wait);
    assert!(result.job.deadline < dispatch_started + execution_wait + Duration::from_secs(1));
    assert_eq!(result.outcome, Ok(()));
    assert_eq!(result.agent.state.sessions[&scope()], "old-1");
    let config = crate::error_outcome_emission_tests::test_config();
    let (respawn_tx, _respawn_rx) = tokio::sync::mpsc::channel(8);
    let mut tasks = tokio::task::JoinSet::new();
    let mut circuits = vec![crate::SlotCircuit {
        crash_times: vec![],
        open_until: None,
        respawn_in_flight: false,
    }];
    let old = tokio::time::Instant::now() - Duration::from_secs(60);
    let mut activity = old;
    crate::control_tasks::finish(
        &mut pool,
        &ctx,
        result,
        &Default::default(),
        &config,
        &mut circuits,
        &respawn_tx,
        &mut tasks,
        None,
        &mut activity,
    );
    assert!(
        activity > old,
        "real control completion refreshes idle clocks"
    );
    assert!(!pool
        .task_map()
        .values()
        .any(|meta| meta.scope.as_ref() == Some(&scope())));
    let mut returned = pool.try_claim(Some(&scope())).unwrap();
    assert_eq!(
        returned.state.turn_counts.get(&scope()),
        None,
        "control must not increment ordinary delivered turns"
    );
    returned.acp.shutdown().await;
    pool.join_set.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn compact_valid_admission_crosses_wire_expiry_while_waiting_for_owner() {
    let (fixture, ctx, mut pool, human, server) = setup().await;
    let owner = pool.try_claim(Some(&scope())).unwrap();
    let request = authorized_control_at(
        &ctx,
        &human,
        &scope(),
        "!compact",
        nostr::Timestamp::from(nostr::Timestamp::now().as_secs() - 899),
    )
    .await;
    ctx.session_controls
        .queue_compaction(
            request.clone(),
            pool.compaction_target(&scope(), &ctx.session_controls, true),
        )
        .unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    crate::control_tasks::dispatch(&mut pool, &ctx, &tx);
    assert!(rx.try_recv().is_err());
    assert!(pool.task_map().is_empty());
    assert!(!fixture.methods().contains(&"session/prompt".into()));
    tokio::time::sleep(Duration::from_millis(2100)).await;
    assert!(!buzz_core::session_control::control_is_fresh(
        request.event.created_at.as_secs(),
        nostr::Timestamp::now().as_secs(),
    ));
    pool.return_agent(owner);
    crate::control_tasks::dispatch(&mut pool, &ctx, &tx);
    let result = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .expect("validly admitted compact must execute within its control deadline");
    assert_eq!(result.outcome, Ok(()));
    assert_eq!(
        fixture
            .methods()
            .iter()
            .filter(|method| *method == "session/prompt")
            .count(),
        1
    );
    let mut agent = result.agent;
    agent.acp.shutdown().await;
    pool.join_set.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn compact_wait_does_not_block_unrelated_scope_dispatch() {
    let (fixture, ctx, mut pool, human, server) = setup().await;
    let mut owner = pool.try_claim(Some(&scope())).unwrap();
    let mut second = fixture.agent(&[]).await;
    second.index = 1;
    // Preserve the checked-out owner's directory while adding unrelated capacity.
    pool.agents_mut().push(Some(second));
    enqueue(&ctx, &pool, &human, &scope()).await;
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    crate::control_tasks::dispatch(&mut pool, &ctx, &tx);
    let other = SessionScope::Thread {
        channel_id: scope().channel_id(),
        root_event_id: "c".repeat(64),
    };
    let event = mention(
        &human,
        other.channel_id(),
        other.root_event_id(),
        "ordinary task",
    );
    let mut queue = crate::EventQueue::new(crate::DedupMode::Queue);
    queue.push(crate::QueuedEvent {
        scope: other.clone(),
        channel_id: other.channel_id(),
        event,
        received_at: std::time::Instant::now(),
        prompt_tag: "test".into(),
        edit: None,
    });
    let dispatched = crate::dispatch_pending(
        &mut pool,
        &mut queue,
        &ctx,
        &mut tokio::time::Instant::now(),
        None,
    );
    assert_eq!(dispatched.len(), 1);
    assert_eq!(dispatched[0].0, other);
    owner.acp.shutdown().await;
    pool.join_set.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn compact_protects_background_collection_and_pending_blocks_idle() {
    let (_fixture, ctx, mut pool, human, server) = setup().await;
    enqueue(&ctx, &pool, &human, &scope()).await;
    let generation = pool.agents_mut()[0]
        .as_ref()
        .unwrap()
        .state
        .scope_owner_generations[&scope()];
    let continuation = crate::queue::RuntimeContinuation {
        work: None,
        crash_recovery: false,
        background_ready: true,
        delivery_key: Uuid::new_v4(),
        ordinary_preempted: false,
        worker_index: 0,
        session_id: "old-1".into(),
        generation,
        thread_tags: crate::ThreadTags::default(),
        received_at: std::time::Instant::now(),
    };
    pool.set_runtime_hold(
        0,
        crate::pool::RuntimeHold {
            scope: scope(),
            blocking_pids: Default::default(),
            continuation: Some(continuation.clone()),
            collection_pending: true,
        },
    );
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    crate::control_tasks::dispatch(&mut pool, &ctx, &tx);
    assert!(pool.task_map().is_empty());
    assert_eq!(ctx.session_controls.pending_scopes(), vec![scope()]);
    assert!(!crate::idle_pool_sleep_due(
        true,
        tokio::time::Instant::now() - Duration::from_secs(60),
        tokio::time::Instant::now(),
        Duration::from_secs(1),
        false,
        false,
        !ctx.session_controls.pending_scopes().is_empty(),
        false,
        false
    ));
    let mut queue = crate::EventQueue::new(crate::DedupMode::Queue);
    queue.enqueue_continuation(scope(), continuation);
    let dispatched = crate::dispatch_pending(
        &mut pool,
        &mut queue,
        &ctx,
        &mut tokio::time::Instant::now(),
        None,
    );
    assert_eq!(
        dispatched.len(),
        1,
        "waiting compact must let protected collection complete"
    );
    assert!(pool.task_map().values().all(|meta| meta
        .recoverable_batch
        .as_ref()
        .is_some_and(|batch| batch.continuation.is_some())));
    pool.join_set.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn compact_duplicate_busy_deadline_restart_removal_and_shutdown_are_terminal() {
    for ending in ["deadline", "restart", "removal", "shutdown", "panic"] {
        let (_fixture, ctx, mut pool, human, server) = setup().await;
        let request = enqueue(&ctx, &pool, &human, &scope()).await;
        let target = pool.compaction_target(&scope(), &ctx.session_controls, true);
        ctx.session_controls
            .queue_compaction(request.clone(), target)
            .unwrap();
        let mut other = request.clone();
        other.origin.command_event_id = "d".repeat(64);
        assert_eq!(
            ctx.session_controls.queue_compaction(
                other,
                pool.compaction_target(&scope(), &ctx.session_controls, true)
            ),
            Err(ControlFailure::Busy)
        );
        match ending {
            "deadline" => {
                // The production scheduler consumes its actual stored deadline.
                tokio::time::pause();
                tokio::time::advance(
                    crate::session_controls::compaction::COMPACT_QUEUE_WAIT
                        + Duration::from_secs(1),
                )
                .await;
                let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
                crate::control_tasks::dispatch(&mut pool, &ctx, &tx);
            }
            "restart" => {
                let restart = authorized_control(&ctx, &human, &scope(), "!restart all").await;
                ctx.session_controls.restart(restart).unwrap();
            }
            "removal" => ctx
                .session_controls
                .revoke_channel(scope().channel_id())
                .unwrap(),
            "shutdown" => {
                ctx.session_controls.shutdown_compactions();
            }
            "panic" => {
                let job = ctx.session_controls.pending_compactions().pop().unwrap();
                assert!(ctx.session_controls.start_compaction(&job, 0));
                ctx.session_controls.begin_compaction_effect(&job).unwrap();
                ctx.session_controls.compaction_panicked(0);
            }
            _ => unreachable!(),
        }
        assert!(!ctx.session_controls.compact_pending(&scope()), "{ending}");
        let expected = match ending {
            "deadline" => ControlFailure::Busy,
            "restart" => ControlFailure::Superseded,
            "panic" => ControlFailure::Unconfirmed,
            _ => ControlFailure::Cancelled,
        };
        assert!(
            ctx.session_controls
                .take_publications()
                .iter()
                .any(
                    |publication| publication.command_id == request.origin.command_event_id
                        && publication.result == ControlResult::Terminal(Err(expected))
                ),
            "{ending}"
        );
        if ending == "deadline" {
            tokio::time::resume();
        }
        if let Some(mut agent) = pool.try_claim(None) {
            agent.acp.shutdown().await;
        }
        server.abort();
    }
}

#[tokio::test]
async fn compact_only_lazy_work_requires_existing_session() {
    let (fixture, ctx, mut pool, human, server) = setup().await;
    let absent = SessionScope::Thread {
        channel_id: scope().channel_id(),
        root_event_id: "c".repeat(64),
    };
    let job = authorized_control(&ctx, &human, &absent, "!compact").await;
    assert_eq!(
        ctx.session_controls.queue_compaction(
            job,
            pool.compaction_target(&absent, &ctx.session_controls, true)
        ),
        Err(ControlFailure::NoSession)
    );
    assert!(ctx.session_controls.pending_compactions().is_empty());
    let mut queue = crate::EventQueue::new(crate::DedupMode::Queue);
    let mut lifecycle = crate::pool_lifecycle::PoolLifecycle::<()>::listening();
    assert_eq!(
        lifecycle.start_wake_if_due(
            crate::control_tasks::wake_pending(&mut queue, &ctx.session_controls),
            tokio::time::Instant::now()
        ),
        None
    );
    fixture
        .store
        .mark_departed(
            &scope(),
            fixture.record(&scope()).live.unwrap().lease,
            Departure::IdleSleep,
        )
        .unwrap();
    let mut old = pool.try_claim(Some(&scope())).unwrap();
    old.acp.shutdown().await;
    let empty_pool = crate::pool::AgentPool::from_slots(vec![]);
    enqueue(&ctx, &empty_pool, &human, &scope()).await;
    assert_eq!(ctx.session_controls.pending_compactions().len(), 1);
    assert_eq!(
        lifecycle.start_wake_if_due(
            crate::control_tasks::wake_pending(&mut queue, &ctx.session_controls),
            tokio::time::Instant::now()
        ),
        Some(1)
    );
    assert!(!fixture.methods().contains(&"session/new".into()));
    server.abort();
}

#[tokio::test]
async fn compact_resume_and_rpc_share_one_execution_deadline() {
    for (remaining_ms, resume_delay, prompt_delay, prompts) in
        [(1000, "0.1", "1.1", 1), (100, "0.3", "0", 0)]
    {
        let fixture = Fixture::new();
        let human = Keys::generate();
        let (mut ctx, server) = relay_context(scope().channel_id(), vec![]).await;
        ctx.session_policy = crate::scope::SessionPolicy::Thread;
        ctx.max_turn_duration = Duration::from_secs(5);
        ctx.session_controls =
            crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
        fixture.departed_record(&scope(), &ctx, &[]);
        let agent = fixture
            .agent(&[
                ("RESUME_CAP", "1"),
                ("COMPACT_STATUS", "completed"),
                ("DELAY", resume_delay),
                ("PROMPT_DELAY", prompt_delay),
            ])
            .await;
        let mut pool = crate::pool::AgentPool::from_slots(vec![Some(agent)]);
        let ctx = Arc::new(ctx);
        enqueue(&ctx, &pool, &human, &scope()).await;
        let mut job = ctx.session_controls.pending_compactions().pop().unwrap();
        assert!(
            !fixture.methods().contains(&"session/resume".into()),
            "queued waiting has not touched the provider"
        );
        let mut agent = pool.claim_compaction(&job).unwrap().unwrap();
        job.deadline = tokio::time::Instant::now() + Duration::from_millis(remaining_ms);
        assert!(ctx.session_controls.start_compaction(&job, agent.index));
        agent.state.control_dispatch = Some(job.dispatch.clone());
        let (_control_tx, control_rx) = tokio::sync::oneshot::channel();
        let mut result = tokio::time::timeout(
            Duration::from_secs(3),
            crate::pool::compaction::run_compaction_task(agent, job, ctx.clone(), control_rx),
        )
        .await
        .unwrap();
        let outcome = result.outcome;
        result.agent.acp.shutdown().await;
        pool.join_set.shutdown().await;
        server.abort();
        assert_eq!(
            outcome,
            Err(ControlFailure::Timeout),
            "resume and RPC must not each receive a fresh execution budget"
        );
        assert_eq!(
            fixture
                .methods()
                .iter()
                .filter(|method| *method == "session/prompt")
                .count(),
            prompts
        );
        assert!(!fixture.methods().contains(&"session/new".into()));
    }
}

#[tokio::test]
async fn compact_scope_cap_never_evicts_pending_and_restart_can_supersede_at_capacity() {
    let (fixture, ctx, mut pool, human, server) = setup().await;
    let base = authorized_control(&ctx, &human, &scope(), "!compact").await;
    for n in 0..128 {
        let mut request = base.clone();
        request.scope = SessionScope::Thread {
            channel_id: scope().channel_id(),
            root_event_id: format!("{n:064x}"),
        };
        request.origin.command_event_id = format!("{n:064x}");
        ctx.session_controls
            .queue_compaction(
                request,
                Ok(crate::session_controls::CompactionTarget::Disk {
                    lease: Uuid::new_v4(),
                    session_id: "existing".into(),
                }),
            )
            .unwrap();
    }
    assert_eq!(
        ctx.session_controls.queue_compaction(
            base.clone(),
            pool.compaction_target(&scope(), &ctx.session_controls, true)
        ),
        Err(ControlFailure::Busy)
    );
    assert_eq!(ctx.session_controls.pending_compactions().len(), 128);
    assert!(
        fixture.record(&scope()).control_receipts.is_empty(),
        "capacity refusal must not allocate another durable Started reservation"
    );
    let restart = authorized_control(&ctx, &human, &scope(), "!restart").await;
    assert!(matches!(
        ctx.session_controls.restart(restart),
        Err(crate::session_store::StoreError::Busy)
    ));
    let target = SessionScope::Thread {
        channel_id: scope().channel_id(),
        root_event_id: format!("{:064x}", 0),
    };
    let restart = authorized_control(&ctx, &human, &target, "!restart all").await;
    assert!(ctx.session_controls.restart(restart).unwrap());
    assert_eq!(ctx.session_controls.pending_compactions().len(), 127);
    assert_eq!(ctx.session_controls.pending_scopes().len(), 128);
    let mut agent = pool.try_claim(None).unwrap();
    agent.acp.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn compact_pending_receipt_writes_reserve_capacity_and_preserve_newer_supersession() {
    let (fixture, ctx, mut pool, human, server) = setup().await;
    let base = authorized_control(&ctx, &human, &scope(), "!compact").await;
    for n in 0..128 {
        let mut request = base.clone();
        request.scope = SessionScope::Thread {
            channel_id: scope().channel_id(),
            root_event_id: format!("{n:064x}"),
        };
        request.origin.command_event_id = format!("{n:064x}");
        ctx.session_controls
            .queue_compaction(
                request.clone(),
                Ok(crate::session_controls::CompactionTarget::Disk {
                    lease: Uuid::new_v4(),
                    session_id: "existing".into(),
                }),
            )
            .unwrap();
        let admitted = ctx
            .session_controls
            .pending_compactions()
            .into_iter()
            .find(|job| job.job.scope == request.scope)
            .unwrap();
        ctx.session_controls
            .begin_compaction_effect(&admitted)
            .unwrap();
        fixture.store.fail_writes(true);
        ctx.session_controls
            .finish_compaction_job(&admitted, Err(ControlFailure::Unconfirmed));
        fixture.store.fail_writes(false);
    }
    assert_eq!(ctx.session_controls.receipt_write_count(), 128);
    assert_eq!(
        ctx.session_controls.queue_compaction(
            base.clone(),
            pool.compaction_target(&scope(), &ctx.session_controls, true)
        ),
        Err(ControlFailure::Busy)
    );
    assert!(ctx.session_controls.pending_compactions().is_empty());
    assert_eq!(ctx.session_controls.receipt_write_count(), 128);
    let target = SessionScope::Thread {
        channel_id: scope().channel_id(),
        root_event_id: format!("{:064x}", 0),
    };
    let restart = authorized_control(&ctx, &human, &target, "!restart all").await;
    assert!(ctx.session_controls.restart(restart).unwrap());
    ctx.session_controls.sweep_orphans(128).unwrap();
    assert_eq!(ctx.session_controls.receipt_write_count(), 0);
    let prior = fixture
        .record(&target)
        .control_receipts
        .into_iter()
        .find(|receipt| receipt.command_id == format!("{:064x}", 0))
        .unwrap();
    assert_eq!(
        prior.state,
        crate::session_store::ControlReceiptState::Finished(Err(ControlFailure::Superseded))
    );
    assert!(!fixture.methods().contains(&"session/prompt".into()));
    let mut agent = pool.try_claim(None).unwrap();
    agent.acp.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn compact_actual_panic_terminalizes_started_without_ordinary_retry() {
    let (fixture, ctx, mut pool, human, server) = setup().await;
    let request = enqueue(&ctx, &pool, &human, &scope()).await;
    let admitted = ctx.session_controls.pending_compactions().pop().unwrap();
    assert!(ctx.session_controls.start_compaction(&admitted, 0));
    ctx.session_controls
        .begin_compaction_effect(&admitted)
        .unwrap();
    let agent = pool.try_claim(Some(&scope())).unwrap();
    let handle = pool.join_set.spawn(async move {
        let _agent = agent;
        panic!("compaction panic fixture");
    });
    pool.task_map_mut().insert(
        handle.id(),
        crate::pool::TaskMeta {
            permission_tx: None,
            agent_index: 0,
            channel_id: Some(scope().channel_id()),
            scope: Some(scope()),
            turn_id: request.origin.command_event_id.clone(),
            recoverable_batch: None,
            control_tx: None,
            steer_tx: None,
            control_dispatch: Some(admitted.dispatch.clone()),
            successful_steer_deliveries: Default::default(),
        },
    );
    let error = pool.join_set.join_next().await.unwrap().unwrap_err();
    let config = crate::error_outcome_emission_tests::test_config();
    let (tx, _rx) = tokio::sync::mpsc::channel(8);
    let mut tasks = tokio::task::JoinSet::new();
    let mut circuits = vec![crate::SlotCircuit {
        crash_times: vec![],
        open_until: None,
        respawn_in_flight: false,
    }];
    let mut queue = crate::EventQueue::new(crate::DedupMode::Queue);
    crate::recover_panicked_agent(
        &mut pool,
        &mut queue,
        &config,
        &ctx.session_controls,
        error,
        &mut false,
        &Default::default(),
        &mut Default::default(),
        &mut circuits,
        &tx,
        &mut tasks,
        None,
        None,
    );
    assert!(!ctx.session_controls.compact_pending(&scope()));
    assert!(!queue.has_undispatched_work());
    assert_eq!(
        fixture.record(&scope()).control_receipts[0].state,
        crate::session_store::ControlReceiptState::Finished(Err(ControlFailure::Unconfirmed))
    );
    tasks.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn compact_in_flight_restart_uses_retirement_return_and_reports_superseded() {
    let fixture = Fixture::new();
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(scope().channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    let mut agent = fixture
        .agent(&[("COMPACT_STATUS", "completed"), ("HANG_PROMPT", "1")])
        .await;
    agent.state = owned_state(&fixture, &scope(), "old-1").0;
    agent.acp.ensure_compaction_route("old-1");
    agent.acp.advertise_compaction("old-1");
    let mut pool = crate::pool::AgentPool::from_slots(vec![Some(agent)]);
    let generation = pool.record_scope_owner(scope(), 0);
    pool.agents_mut()[0]
        .as_mut()
        .unwrap()
        .state
        .set_scope_owner_generation(scope(), generation);
    let ctx = Arc::new(ctx);
    let request = enqueue(&ctx, &pool, &human, &scope()).await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    crate::control_tasks::dispatch(&mut pool, &ctx, &tx);
    tokio::time::timeout(Duration::from_secs(5), async {
        while !fixture.methods().contains(&"session/prompt".into()) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let restart = authorized_control(&ctx, &human, &scope(), "!restart all").await;
    ctx.session_controls.restart(restart).unwrap();
    let targets = pool.retirement_targets(&scope());
    for target in &targets {
        pool.mark_retire_on_return(*target);
    }
    ctx.session_controls.plan_retirement(&scope(), targets);
    assert!(crate::signal_in_flight_task_for_scope(
        &mut pool,
        &scope(),
        crate::pool::ControlSignal::Rotate
    ));
    let mut result = tokio::time::timeout(Duration::from_secs(8), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.outcome, Err(ControlFailure::Superseded));
    assert!(
        result.transport_failed,
        "cancel-drain uncertainty must survive retirement fencing"
    );
    assert!(pool.needs_retirement(&result.agent));
    assert!(ctx.session_controls.retirement_pending(&scope()));
    assert!(ctx
        .session_controls
        .take_publications()
        .iter()
        .any(
            |publication| publication.command_id == request.origin.command_event_id
                && publication.result == ControlResult::Terminal(Err(ControlFailure::Superseded))
        ));
    result.agent.acp.shutdown().await;
    pool.join_set.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn compact_uses_pre_ack_new_session_advertisement_after_ordinary_completion() {
    let fixture = Fixture::new();
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(scope().channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.context_message_limit = 0;
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    ctx.session_status = Some(
        crate::session_status::SessionStatusRuntime::open(
            fixture.dir.path(),
            buzz_core::session_status::canonical_identity(
                "wss://relay.example",
                &ctx.agent_keys.public_key().to_hex(),
            )
            .unwrap(),
        )
        .unwrap(),
    );
    let prompts = fixture.dir.path().join("new-advertised-prompts.jsonl");
    let mut agent = fixture
        .agent(&[
            ("NEW_ADVERTISE", "1"),
            ("COMPACT_STATUS", "completed"),
            ("COMPACT_ONLY_SLASH", "1"),
            ("PROMPTS", prompts.to_str().unwrap()),
        ])
        .await;
    agent.state.control_dispatch = Some(ctx.session_controls.capture(&scope()));
    let ctx = Arc::new(ctx);
    let trigger = mention(
        &human,
        scope().channel_id(),
        scope().root_event_id(),
        "ordinary task",
    );
    let agent = run_turn(agent, &ctx, batch(&scope(), trigger)).await;
    assert_eq!(agent.state.sessions[&scope()], "new-1");
    let mut pool = crate::pool::AgentPool::from_slots(vec![Some(agent)]);
    let generation = pool.record_scope_owner(scope(), 0);
    let mut agent = pool.try_claim(Some(&scope())).unwrap();
    agent.state.set_scope_owner_generation(scope(), generation);
    pool.return_agent(agent);
    enqueue(&ctx, &pool, &human, &scope()).await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    crate::control_tasks::dispatch(&mut pool, &ctx, &tx);
    let mut result = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    result.agent.acp.shutdown().await;
    pool.join_set.shutdown().await;
    server.abort();
    assert_eq!(
        result.outcome,
        Ok(()),
        "actual pre-ACK advertised compact must survive ordinary completion and binding"
    );
    assert_eq!(
        fixture
            .methods()
            .iter()
            .filter(|method| *method == "session/new")
            .count(),
        1
    );
    let frames: Vec<serde_json::Value> = std::fs::read_to_string(prompts)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(frames.len(), 2, "one ordinary and one compact prompt");
    assert_eq!(frames[1]["sessionId"], "new-1");
    assert_eq!(
        frames[1]["prompt"],
        serde_json::json!([{"type":"text","text":"/compact"}])
    );
}

#[tokio::test]
async fn compact_cached_worker_replaces_previous_observer_scope() {
    let (_fixture, ctx, mut pool, human, server) = setup().await;
    let observer = crate::observer::ObserverHandle::in_process();
    let mut agent = pool.try_claim(Some(&scope())).unwrap();
    agent.acp.set_observer(Some(observer.clone()), 0);
    agent
        .acp
        .set_observer_context(crate::observer::ObserverContext {
            channel_id: Some("previous-channel".into()),
            session_id: Some("previous-session".into()),
            turn_id: Some("previous-turn".into()),
            started_at: None,
        });
    pool.return_agent(agent);
    let request = enqueue(&ctx, &pool, &human, &scope()).await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    crate::control_tasks::dispatch(&mut pool, &ctx, &tx);
    let mut result = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.outcome, Ok(()));
    let events = observer.snapshot();
    let frames: Vec<_> = events
        .iter()
        .filter(|event| {
            event.payload["method"] == "session/prompt"
                || event.payload["method"] == "session/update"
        })
        .collect();
    assert!(frames.len() >= 2);
    for frame in frames {
        assert_eq!(
            frame.channel_id.as_deref(),
            Some(scope().channel_id().to_string().as_str())
        );
        assert_eq!(frame.session_id.as_deref(), Some("old-1"));
        assert_eq!(
            frame.turn_id.as_deref(),
            Some(request.origin.command_event_id.as_str())
        );
        assert!(frame.started_at.is_some());
    }
    result.agent.acp.shutdown().await;
    pool.join_set.shutdown().await;
    server.abort();
}

#[path = "compaction_resume_policy_tests.rs"]
mod resume_policy_tests;
