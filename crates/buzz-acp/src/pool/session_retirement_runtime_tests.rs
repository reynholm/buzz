use super::*;

#[tokio::test]
async fn checked_retirement_requires_every_clean_report_and_retries_after_uncertainty() {
    use crate::acp::TerminationReport;
    use buzz_core::session_control::{ControlFailure, ControlResult};
    let fixture = Fixture::new();
    let scope = scope();
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    let job = authorized_control(&ctx, &human, &scope, "!restart none").await;
    let first_id = job.origin.command_event_id.clone();
    ctx.session_controls.restart(job).unwrap();
    ctx.session_controls.plan_retirement(&scope, [0, 1]);
    ctx.session_controls
        .retirement_report(0, TerminationReport::Clean);
    assert!(ctx.session_controls.retirement_pending(&scope));
    ctx.session_controls
        .retirement_report(1, TerminationReport::Unverified);
    assert!(ctx.session_controls.retirement_pending(&scope));
    assert!(
        ctx.session_controls.retiring_workers().is_empty(),
        "failed termination waits for an explicit retry"
    );
    assert_eq!(
        ctx.session_controls.prior_result(&scope, &first_id),
        Some(ControlResult::Terminal(Err(ControlFailure::Unconfirmed)))
    );
    let retry = authorized_control(&ctx, &human, &scope, "!restart all").await;
    assert!(ctx.session_controls.restart(retry).unwrap());
    ctx.session_controls.plan_retirement(&scope, []);
    assert_eq!(ctx.session_controls.retiring_workers().len(), 1);
    ctx.session_controls
        .retirement_report(1, TerminationReport::Clean);
    assert!(!ctx.session_controls.retirement_pending(&scope));
    assert_eq!(
        ctx.session_controls.restart_intent(&scope).unwrap().depth,
        buzz_core::session_control::RestartDepth::All
    );
    server.abort();
}

#[tokio::test]
async fn checked_retirement_second_restart_supersedes_first_without_rechecking_freshness() {
    use buzz_core::session_control::{ControlFailure, ControlResult};
    let fixture = Fixture::new();
    let scope = scope();
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    let first = authorized_control(&ctx, &human, &scope, "!restart none").await;
    let first_id = first.origin.command_event_id.clone();
    ctx.session_controls.restart(first).unwrap();
    ctx.session_controls.plan_retirement(&scope, [0]);
    let second = authorized_control(&ctx, &human, &scope, "!restart all").await;
    ctx.session_controls.restart(second).unwrap();
    ctx.session_controls.plan_retirement(&scope, [0]);
    assert_eq!(
        ctx.session_controls.prior_result(&scope, &first_id),
        Some(ControlResult::Terminal(Err(ControlFailure::Superseded)))
    );
    ctx.session_controls
        .retirement_report(0, crate::acp::TerminationReport::Clean);
    assert!(!ctx.session_controls.retirement_pending(&scope));
    server.abort();
}

#[tokio::test]
async fn checked_retirement_neighbors_resume_or_start_new_with_recycled_reason() {
    for resume in [true, false] {
        let fixture = Fixture::new();
        let scope = scope();
        let (mut ctx, server) = relay_context(scope.channel_id(), vec![]).await;
        ctx.session_controls =
            crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
        let mut agent = fixture.agent(&[("RESUME_CAP", "1")]).await;
        if resume {
            agent.state = owned_state(&fixture, &scope, "old-1").0;
        } else {
            agent.state = SessionState::default();
            agent
                .state
                .sessions
                .insert(scope.clone(), "no-resume-old".into());
        }
        agent
            .prepare_retired_neighbors(&Default::default(), &ctx.session_controls)
            .unwrap();
        let (_, start) = acquire_session(&mut agent, &ctx, None, channel_context(&scope), &scope)
            .await
            .unwrap();
        if resume {
            assert_eq!(
                start,
                SessionStart::Resumed {
                    reason: SessionReason::WorkerRecycled
                }
            );
            assert!(fixture.methods().contains(&"session/resume".into()));
        } else {
            assert_eq!(
                start,
                SessionStart::New {
                    reason: SessionReason::WorkerRecycled
                }
            );
            let record = fixture.record(&scope);
            assert!(
                record.live.is_none(),
                "no-resume controller must never persist provider identity"
            );
        }
        agent.acp.shutdown().await;
        server.abort();
    }
}

#[tokio::test]
async fn checked_retirement_admitted_job_can_complete_twenty_minutes_later() {
    let fixture = Fixture::new();
    let scope = scope();
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    let job = authorized_control(&ctx, &human, &scope, "!restart all").await;
    let id = job.origin.command_event_id.clone();
    ctx.session_controls.restart(job).unwrap();
    ctx.session_controls.plan_retirement(&scope, [0]);
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(1200)).await;
    let workers = ctx.session_controls.retiring_workers();
    assert_eq!(workers.len(), 1);
    assert!(
        workers[0].2,
        "foreign background wait expires at ten minutes"
    );
    ctx.session_controls
        .retirement_report(0, crate::acp::TerminationReport::Clean);
    assert!(!ctx.session_controls.retirement_pending(&scope));
    assert_eq!(
        ctx.session_controls.prior_result(&scope, &id),
        Some(buzz_core::session_control::ControlResult::Terminal(Ok(())))
    );
    server.abort();
}

#[tokio::test]
async fn checked_retirement_executor_waits_for_verified_kill_and_excludes_refill() {
    let fixture = Fixture::new();
    let scope = scope();
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    let adapter = format!("import signal\nsignal.signal(signal.SIGTERM, signal.SIG_IGN)\n{ADAPTER}\nwhile True: time.sleep(1)\n");
    let mut agent = fixture.agent(&[]).await;
    agent.acp.shutdown().await;
    agent.acp = AcpClient::spawn(
        "/usr/bin/python3",
        &["-u".into(), "-c".into(), adapter.clone()],
        &[("LOG".into(), fixture.log().display().to_string())],
        false,
    )
    .await
    .unwrap();
    agent.acp.initialize().await.unwrap();
    let pid = agent.acp.process_id().unwrap();
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    pool.record_scope_owner(scope.clone(), 0);
    let job = authorized_control(&ctx, &human, &scope, "!restart none").await;
    ctx.session_controls.restart(job).unwrap();
    ctx.session_controls
        .plan_retirement(&scope, pool.retirement_targets(&scope));
    let mut config = crate::error_outcome_emission_tests::test_config();
    config.agent_command = "/usr/bin/python3".into();
    config.agent_args = vec!["-u".into(), "-c".into(), adapter];
    config.persona_env_vars = vec![("LOG".into(), fixture.log().display().to_string())];
    let mut circuits = vec![crate::SlotCircuit {
        crash_times: vec![],
        open_until: None,
        respawn_in_flight: false,
    }];
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let mut tasks = tokio::task::JoinSet::new();
    let mut parked = HashMap::new();
    let started = tokio::time::Instant::now();
    crate::worker_retirement::drive(
        &mut pool,
        &ctx.session_controls,
        &mut parked,
        &config,
        &mut circuits,
        &tx,
        &mut tasks,
        None,
    );
    crate::worker_retirement::drive(
        &mut pool,
        &ctx.session_controls,
        &mut parked,
        &config,
        &mut circuits,
        &tx,
        &mut tasks,
        None,
    );
    assert!(
        circuits[0].respawn_in_flight,
        "ordinary refill is excluded while checked retirement owns the slot"
    );
    assert_eq!(tasks.len(), 1);
    assert!(ctx.session_controls.retirement_pending(&scope));
    let mut result = tokio::time::timeout(Duration::from_secs(30), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(started.elapsed() >= Duration::from_secs(15));
    assert_eq!(
        result.retirement.as_ref().unwrap().report,
        crate::acp::TerminationReport::Clean
    );
    assert!(!crate::worker_retirement::finish(
        &mut pool,
        &ctx.session_controls,
        &mut parked,
        &mut result
    ));
    assert!(!ctx.session_controls.retirement_pending(&scope));
    assert!(circuits[0].crash_times.is_empty());
    assert!(parked.is_empty());
    assert_eq!(
        nix::sys::signal::killpg(nix::unistd::Pid::from_raw(pid as i32), None),
        Err(nix::errno::Errno::ESRCH)
    );
    let (mut acp, _, _) = result.result.unwrap();
    acp.shutdown().await;
    tasks.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn checked_retirement_terminal_store_failure_retains_gate_and_reports_uncertainty() {
    let fixture = Fixture::new();
    let scope = scope();
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    let job = authorized_control(&ctx, &human, &scope, "!restart none").await;
    ctx.session_controls.restart(job).unwrap();
    ctx.session_controls.plan_retirement(&scope, [0]);
    fixture.store.fail_writes(true);
    ctx.session_controls
        .retirement_report(0, crate::acp::TerminationReport::Clean);
    assert!(ctx.session_controls.retirement_pending(&scope));
    let publications = ctx.session_controls.take_publications();
    assert_eq!(publications.len(), 1, "failed terminal persist must leave an explicit outcome rather than silent permanent pending");
    assert_eq!(
        publications[0].result,
        buzz_core::session_control::ControlResult::Terminal(Err(
            buzz_core::session_control::ControlFailure::Unconfirmed
        ))
    );
    fixture.store.fail_writes(false);
    server.abort();
}

#[tokio::test]
async fn checked_retirement_supersedes_started_compact_without_a_compact_rpc() {
    let fixture = Fixture::new();
    let scope = scope();
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    let compact = "c".repeat(64);
    fixture
        .store
        .begin_control_attempt(&scope, &compact, nostr::Timestamp::now().as_secs())
        .unwrap();
    let dispatch = ctx.session_controls.capture(&scope);
    let restart = authorized_control(&ctx, &human, &scope, "!restart none").await;
    ctx.session_controls.restart(restart).unwrap();
    assert!(!dispatch.current());
    assert_eq!(
        ctx.session_controls.prior_result(&scope, &compact),
        Some(buzz_core::session_control::ControlResult::Terminal(Err(
            buzz_core::session_control::ControlFailure::Superseded
        )))
    );
    assert!(ctx
        .session_controls
        .take_publications()
        .iter()
        .any(|result| result.command_id == compact));
    assert!(
        fixture.methods().is_empty(),
        "Task2b adds supersession bookkeeping, not a compact RPC"
    );
    server.abort();
}

#[tokio::test]
async fn checked_retirement_channel_revocation_cannot_abandon_checked_cleanup() {
    let fixture = Fixture::new();
    let scope = scope();
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    let job = authorized_control(&ctx, &human, &scope, "!restart none").await;
    ctx.session_controls.restart(job).unwrap();
    ctx.session_controls.plan_retirement(&scope, [0]);
    let stale = ctx.session_controls.capture(&scope);
    ctx.session_controls
        .revoke_channel(scope.channel_id())
        .unwrap();
    assert!(!stale.current());
    assert!(
        ctx.session_controls.retirement_pending(&scope),
        "membership loss must not discard an owned checked-retirement job"
    );
    assert!(ctx.session_controls.restart_intent(&scope).is_none());
    ctx.session_controls
        .retirement_report(0, crate::acp::TerminationReport::Clean);
    assert!(!ctx.session_controls.retirement_pending(&scope));
    server.abort();
}

#[tokio::test]
async fn checked_retirement_cannot_depart_neighbor_owned_by_a_newer_worker() {
    let fixture = Fixture::new();
    let a = scope();
    let c = SessionScope::Thread {
        channel_id: a.channel_id(),
        root_event_id: "d".repeat(64),
    };
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(a.channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    let (old_state, _) = owned_state(&fixture, &c, "old-c");
    let mut newer = fixture.record(&c).live.unwrap();
    newer.worker_index = 1;
    newer.session_id = "newer-c".into();
    newer.departed = None;
    let lease = fixture
        .store
        .claim(
            &c,
            &crate::session_store::Expect::from_outcome(&fixture.store.load(&c)),
            newer,
        )
        .unwrap();
    let mut agent = fixture.agent(&[]).await;
    agent.state = old_state;
    let mut pool = AgentPool::from_slots(vec![Some(agent), None]);
    pool.record_scope_owner(a.clone(), 0);
    let restart = authorized_control(&ctx, &human, &a, "!restart none").await;
    ctx.session_controls.restart(restart).unwrap();
    ctx.session_controls
        .plan_retirement(&a, pool.retirement_targets(&a));
    let config = crate::error_outcome_emission_tests::test_config();
    let mut circuits = vec![
        crate::SlotCircuit {
            crash_times: vec![],
            open_until: None,
            respawn_in_flight: false,
        },
        crate::SlotCircuit {
            crash_times: vec![],
            open_until: None,
            respawn_in_flight: false,
        },
    ];
    let (tx, mut rx) = tokio::sync::mpsc::channel(2);
    let mut tasks = tokio::task::JoinSet::new();
    let mut parked = HashMap::new();
    crate::worker_retirement::drive(
        &mut pool,
        &ctx.session_controls,
        &mut parked,
        &config,
        &mut circuits,
        &tx,
        &mut tasks,
        None,
    );
    let result = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    let live = fixture.record(&c).live.unwrap();
    assert_eq!(live.lease, lease);
    assert_eq!(
        live.departed, None,
        "retiring worker must not mark a newer active provider session departed"
    );
    tasks.shutdown().await;
    if let Some(mut retained) = result.retirement.unwrap().retained {
        retained.acp.shutdown().await;
    }
    server.abort();
}
