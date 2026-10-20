//! Bind every retained original ownership path to the actual shutdown helper.
use super::*;

async fn original() -> OwnedAgent {
    let acp = AcpClient::spawn(
        "/usr/bin/python3",
        &["-c".into(), "import time; time.sleep(60)".into()],
        &[],
        false,
    )
    .await
    .unwrap();
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

fn alive(pid: u32) -> bool {
    nix::sys::signal::killpg(nix::unistd::Pid::from_raw(pid as i32), None).is_ok()
}

fn cleanup(pid: u32) {
    // Exact original group created by this test; cleanup only after measuring.
    let _ = nix::sys::signal::killpg(
        nix::unistd::Pid::from_raw(pid as i32),
        nix::sys::signal::Signal::SIGKILL,
    );
}

#[tokio::test]
async fn retirement_shutdown_retries_original_parked_after_snapshot_failure() {
    let mut agent = original().await;
    let pid = agent.acp.process_id().unwrap();
    assert_eq!(
        agent.acp.terminate_with_snapshot(|| async { None }).await,
        TerminationReport::Unverified
    );
    assert!(alive(pid));
    let mut parked = HashMap::new();
    let controls = session_controls::SessionControlRuntime::new(None);
    let mut pool = AgentPool::from_slots(vec![None]);
    let mut result = RespawnResult {
        index: 0,
        result: Err(anyhow::anyhow!("initial checked retirement unconfirmed")),
        retirement: Some(RetirementReturn {
            report: TerminationReport::Unverified,
            retained: Some(agent),
        }),
    };
    assert!(finish(&mut pool, &controls, &mut parked, &mut result));
    assert_eq!(parked[&0].as_ref().unwrap().acp.process_id(), Some(pid));
    let (_tx, mut rx) = mpsc::channel(1);
    let mut tasks = tokio::task::JoinSet::new();
    let uncertain = shutdown(&mut parked, &mut tasks, &mut rx).await;
    let original_survived = alive(pid);
    if original_survived {
        cleanup(pid);
    }
    assert!(
        !original_survived,
        "shutdown dropped the parked original without checked cleanup"
    );
    assert!(parked.is_empty());
    assert!(uncertain.is_empty());
}

#[tokio::test]
async fn retirement_shutdown_drains_original_returned_by_aborted_guard() {
    let agent = original().await;
    let pid = agent.acp.process_id().unwrap();
    let (tx, mut rx) = mpsc::channel(1);
    let entered = Arc::new(tokio::sync::Notify::new());
    let signal = entered.clone();
    let mut tasks = tokio::task::JoinSet::new();
    tasks.spawn(async move {
        let mut guard = RetirementGuard {
            index: 0,
            tx,
            agent: Some(agent),
            sent: false,
        };
        guard
            .agent
            .as_mut()
            .unwrap()
            .acp
            .terminate_with_snapshot(|| {
                let signal = signal.clone();
                async move {
                    signal.notify_one();
                    std::future::pending().await
                }
            })
            .await;
    });
    entered.notified().await;
    let mut parked = HashMap::new();
    let uncertain = shutdown(&mut parked, &mut tasks, &mut rx).await;
    let original_survived = alive(pid);
    if original_survived {
        cleanup(pid);
    }
    assert!(
        !original_survived,
        "shutdown lost the original returned by RetirementGuard after abort"
    );
    assert!(uncertain.is_empty());
    assert!(tasks.is_empty());
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn retirement_shutdown_reuses_original_evidence_after_parent_reaped() {
    let mut agent = original().await;
    let pid = agent.acp.process_id().unwrap();
    let mut first = true;
    let report = agent
        .acp
        .terminate_with_snapshot(|| {
            let capture = first;
            first = false;
            async move {
                if capture {
                    tokio::task::spawn_blocking(crate::process_tree::retirement::snapshot)
                        .await
                        .unwrap()
                } else {
                    None
                }
            }
        })
        .await;
    assert_eq!(report, TerminationReport::Unverified);
    assert_eq!(
        agent.acp.process_id(),
        None,
        "the original parent was reaped before shutdown retry"
    );
    let mut parked = HashMap::from([(0, Some(agent))]);
    let (_tx, mut rx) = mpsc::channel(1);
    let uncertain = shutdown(&mut parked, &mut tokio::task::JoinSet::new(), &mut rx).await;
    let original_survived = alive(pid);
    if original_survived {
        cleanup(pid);
    }
    assert!(!original_survived);
    assert!(
        uncertain.is_empty(),
        "retained original evidence must prove cleanup without a replacement parent"
    );
}

#[tokio::test]
async fn retirement_shutdown_retains_uncertain_original_without_fabricating_clean() {
    let mut agent = original().await;
    assert_eq!(
        agent.acp.terminate_with_snapshot(|| async { None }).await,
        TerminationReport::Unverified
    );
    // Independently stop/reap this fixture's exact child. No initial checked
    // evidence was captured; final shutdown must not invent that missing proof.
    agent.acp.shutdown().await;
    assert_eq!(agent.acp.process_id(), None);
    let mut parked = HashMap::from([(0, Some(agent))]);
    let (_tx, mut rx) = mpsc::channel(1);
    let uncertain = tokio::time::timeout(
        Duration::from_secs(1),
        shutdown(&mut parked, &mut tokio::task::JoinSet::new(), &mut rx),
    )
    .await
    .unwrap();
    assert!(parked.is_empty());
    assert_eq!(uncertain.len(), 1);
    assert_eq!(uncertain[0].0, 0);
    assert_eq!(uncertain[0].1.index, 0);
    assert_eq!(uncertain[0].2, TerminationReport::Unverified);
}
