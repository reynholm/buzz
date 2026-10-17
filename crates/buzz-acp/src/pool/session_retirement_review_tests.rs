use super::*;

#[tokio::test]
async fn retirement_executor_preserves_concurrent_no_resume_neighbor_restart() {
    let fixture = Fixture::new();
    let a = scope();
    let c = SessionScope::Thread {
        channel_id: a.channel_id(),
        root_event_id: "cd".repeat(32),
    };
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(a.channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    let eof = fixture.dir.path().join("retirement-eof");
    let mut agent = fixture.agent(&[]).await;
    agent.acp.shutdown().await;
    let adapter = format!(
        "{ADAPTER}\nopen({:?}, 'w').close()\ntime.sleep(.3)\n",
        eof.display().to_string()
    );
    agent.acp = AcpClient::spawn(
        "/usr/bin/python3",
        &["-u".into(), "-c".into(), adapter],
        &[("LOG".into(), fixture.log().display().to_string())],
        false,
    )
    .await
    .unwrap();
    agent.acp.initialize().await.unwrap();
    agent.state = SessionState::default();
    agent.state.sessions.insert(a.clone(), "old-a".into());
    agent.state.sessions.insert(c.clone(), "old-c".into());
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    pool.record_scope_owner(a.clone(), 0);
    pool.record_scope_owner(c.clone(), 0);
    let job = authorized_control(&ctx, &human, &a, "!restart none").await;
    ctx.session_controls.restart(job).unwrap();
    ctx.session_controls.plan_retirement(&a, [0]);
    let mut config = crate::error_outcome_emission_tests::test_config();
    config.agent_command = "/usr/bin/python3".into();
    config.agent_args = vec!["-u".into(), "-c".into(), ADAPTER.into()];
    config.persona_env_vars = vec![("LOG".into(), fixture.log().display().to_string())];
    let mut circuits = vec![crate::SlotCircuit {
        crash_times: vec![],
        open_until: None,
        respawn_in_flight: false,
    }];
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
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
    tokio::time::timeout(Duration::from_secs(5), async {
        while !eof.exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let job = authorized_control(&ctx, &human, &c, "!restart all").await;
    let origin = job.origin.clone();
    ctx.session_controls.restart(job).unwrap();
    let intent = ctx.session_controls.restart_intent(&c).unwrap();
    ctx.session_controls.plan_retirement(&c, [0]);
    let mut result = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(!crate::worker_retirement::finish(
        &mut pool,
        &ctx.session_controls,
        &mut parked,
        &mut result
    ));
    let (mut acp, _, _) = result.result.unwrap();
    acp.shutdown().await;
    assert!(!ctx.session_controls.retirement_pending(&c));
    assert_eq!(
        ctx.session_controls.restart_intent(&c),
        Some(intent.clone()),
        "neighbor cleanup must preserve the concurrently admitted restart token/depth/origin"
    );
    assert_eq!(intent.origin, origin);
    assert_eq!(intent.depth, buzz_core::session_control::RestartDepth::All);
    assert!(fixture.record(&c).live.is_none());
    tasks.shutdown().await;
    server.abort();
}

struct EscapedProbe(u32);
impl Drop for EscapedProbe {
    fn drop(&mut self) {
        let _ = nix::sys::signal::killpg(
            nix::unistd::Pid::from_raw(self.0 as i32),
            nix::sys::signal::Signal::SIGKILL,
        );
    }
}

fn replacement_agent(acp: AcpClient) -> OwnedAgent {
    OwnedAgent {
        index: 0,
        acp,
        state: SessionState::default(),
        model_capabilities: None,
        desired_model: None,
        model_overridden: false,
        desired_model_request_id: None,
        desired_model_pending_ack: false,
        startup_effort: None,
        agent_name: "probe".into(),
        goose_system_prompt_supported: None,
        protocol_version: 1,
        background_context: None,
    }
}

#[tokio::test]
async fn retirement_foreground_panic_cannot_attest_replacement_or_forget_original_on_retry() {
    use crate::acp::TerminationReport;
    use buzz_core::session_control::{ControlFailure, ControlResult};
    let fixture = Fixture::new();
    let scope = scope();
    let human = Keys::generate();
    let (mut ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    let child_path = fixture.dir.path().join("escaped-child");
    let adapter = format!("import subprocess\nchild = subprocess.Popen(['/bin/sleep', '120'], start_new_session=True)\nopen({:?}, 'w').write(str(child.pid))\n{ADAPTER}", child_path.display().to_string());
    let mut acp = AcpClient::spawn(
        "/usr/bin/python3",
        &["-u".into(), "-c".into(), adapter],
        &[("LOG".into(), fixture.log().display().to_string())],
        false,
    )
    .await
    .unwrap();
    acp.initialize().await.unwrap();
    let escaped = EscapedProbe(
        std::fs::read_to_string(child_path)
            .unwrap()
            .parse()
            .unwrap(),
    );
    let agent = replacement_agent(acp);
    let mut pool = AgentPool::from_slots(vec![None]);
    pool.record_scope_owner(scope.clone(), 0);
    let trigger = Arc::new(tokio::sync::Notify::new());
    let signal = trigger.clone();
    let abort = pool.join_set.spawn(async move {
        let _owned_foreground_client = agent;
        signal.notified().await;
        panic!("foreground fixture panic");
    });
    pool.task_map_mut().insert(
        abort.id(),
        crate::pool::TaskMeta {
            agent_index: 0,
            channel_id: Some(scope.channel_id()),
            scope: Some(scope.clone()),
            turn_id: "reserved-panic".into(),
            recoverable_batch: None,
            control_tx: None,
            steer_tx: None,
            control_dispatch: Some(ctx.session_controls.capture(&scope)),
            successful_steer_deliveries: Default::default(),
        },
    );
    let first = authorized_control(&ctx, &human, &scope, "!restart none").await;
    let first_id = first.origin.command_event_id.clone();
    ctx.session_controls.restart(first).unwrap();
    pool.mark_retire_on_return(0);
    ctx.session_controls.plan_retirement(&scope, [0]);
    trigger.notify_one();
    let error = pool.join_set.join_next().await.unwrap().unwrap_err();
    let mut config = crate::error_outcome_emission_tests::test_config();
    config.agent_command = "/usr/bin/python3".into();
    config.agent_args = vec!["-u".into(), "-c".into(), ADAPTER.into()];
    config.persona_env_vars = vec![("LOG".into(), fixture.log().display().to_string())];
    let mut circuits = vec![crate::SlotCircuit {
        crash_times: vec![],
        open_until: None,
        respawn_in_flight: false,
    }];
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let mut tasks = tokio::task::JoinSet::new();
    let mut parked = HashMap::new();
    crate::recover_panicked_agent(
        &mut pool,
        &mut crate::EventQueue::new(crate::config::DedupMode::Queue),
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
    );
    // Before the fix this is the actual ordinary replacement path that lets
    // the next checked drive attest a fresh process instead of the lost one.
    if !tasks.is_empty() {
        let mut rr = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        if !crate::worker_retirement::finish(&mut pool, &ctx.session_controls, &mut parked, &mut rr)
        {
            let (acp, _, _) = rr.result.unwrap();
            pool.finish_worker_retirement(0);
            pool.return_agent(replacement_agent(acp));
            circuits[0].respawn_in_flight = false;
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
            let mut rr = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                .await
                .unwrap()
                .unwrap();
            crate::worker_retirement::finish(
                &mut pool,
                &ctx.session_controls,
                &mut parked,
                &mut rr,
            );
            if let Ok((mut acp, _, _)) = rr.result {
                acp.shutdown().await;
            }
        }
    }
    assert!(
        nix::sys::signal::killpg(nix::unistd::Pid::from_raw(escaped.0 as i32), None).is_ok(),
        "the original separate-group child outlives ordinary panic Drop"
    );
    assert!(
        ctx.session_controls.retirement_pending(&scope),
        "a replacement Clean must not release the original worker's retirement gate"
    );
    assert_eq!(
        ctx.session_controls.prior_result(&scope, &first_id),
        Some(ControlResult::Terminal(Err(ControlFailure::Unconfirmed)))
    );
    assert!(
        circuits[0].crash_times.is_empty(),
        "retirement loss must not enter ordinary crash replacement"
    );
    assert!(tasks.is_empty());
    let retry = authorized_control(&ctx, &human, &scope, "!restart").await;
    let retry_id = retry.origin.command_event_id.clone();
    assert!(ctx.session_controls.restart(retry).unwrap());
    let intent = ctx.session_controls.restart_intent(&scope).unwrap();
    ctx.session_controls.plan_retirement(&scope, [0]);
    let replacement = fixture.agent(&[]).await;
    let mut late = crate::RespawnResult {
        index: 0,
        result: Ok((replacement.acp, 1, "late replacement".into())),
        retirement: None,
    };
    assert!(
        crate::worker_retirement::finish(&mut pool, &ctx.session_controls, &mut parked, &mut late),
        "a late ordinary replacement is not evidence about the lost original"
    );
    ctx.session_controls
        .retirement_report(0, TerminationReport::Clean);
    assert!(ctx.session_controls.retirement_pending(&scope));
    assert!(ctx.session_controls.retiring_workers().is_empty());
    assert_eq!(ctx.session_controls.restart_intent(&scope), Some(intent));
    // A late Clean must retain the original evidence loss for future commands,
    // not merely leave the already terminal current gate closed.
    let later = authorized_control(&ctx, &human, &scope, "!restart all").await;
    let later_id = later.origin.command_event_id.clone();
    assert!(ctx.session_controls.restart(later).unwrap());
    ctx.session_controls.plan_retirement(&scope, [0]);
    ctx.session_controls
        .retirement_report(0, TerminationReport::Clean);
    assert!(ctx.session_controls.retirement_pending(&scope));
    assert_eq!(
        ctx.session_controls.prior_result(&scope, &later_id),
        Some(ControlResult::Terminal(Err(ControlFailure::Unconfirmed)))
    );
    assert_eq!(
        ctx.session_controls.prior_result(&scope, &retry_id),
        Some(ControlResult::Terminal(Err(ControlFailure::Unconfirmed)))
    );
    assert!(ctx
        .session_controls
        .take_publications()
        .iter()
        .all(|p| p.result != ControlResult::Terminal(Ok(()))));
    tasks.shutdown().await;
    server.abort();
}
