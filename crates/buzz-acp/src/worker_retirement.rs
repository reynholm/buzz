//! Nonblocking replacement of reserved workers; uncertainty retains ownership.
use super::*;
use crate::acp::TerminationReport;

pub(crate) struct RetirementReturn {
    pub report: TerminationReport,
    pub retained: Option<OwnedAgent>,
    pub crash_recovery: bool,
}

struct RetirementGuard {
    index: usize,
    tx: mpsc::Sender<RespawnResult>,
    agent: Option<OwnedAgent>,
    sent: bool,
    crash_recovery: bool,
}

impl RetirementGuard {
    fn send(&mut self, result: Result<(AcpClient, u32, String)>, report: TerminationReport) {
        if self.crash_recovery {
            if let Some(work) = self
                .agent
                .as_ref()
                .and_then(|agent| agent.acp.logical_work())
            {
                work.retired(report == TerminationReport::Clean);
            }
        }
        let message = RespawnResult {
            index: self.index,
            result,
            retirement: Some(RetirementReturn {
                report,
                retained: self.agent.take(),
                crash_recovery: self.crash_recovery,
            }),
        };
        match self.tx.try_send(message) {
            Ok(()) => self.sent = true,
            Err(error) => tracing::error!(
                agent = self.index,
                "retirement result channel unavailable: {error}"
            ),
        }
    }
}
impl Drop for RetirementGuard {
    fn drop(&mut self) {
        if !self.sent {
            self.send(
                Err(anyhow::anyhow!("retirement task did not complete")),
                TerminationReport::Unverified,
            );
        }
    }
}

/// Crash recovery owns the original until checked termination. A circuit-open
/// slot still retires its old worker; it never silently drops that obligation.
pub(crate) fn recover_kimi(
    agent: OwnedAgent,
    config: &Config,
    delay: Option<Duration>,
    tx: &mpsc::Sender<RespawnResult>,
    tasks: &mut tokio::task::JoinSet<()>,
    observer: Option<observer::ObserverHandle>,
) {
    let work = agent.acp.logical_work().cloned();
    if let Some(work) = &work {
        work.require_retirement();
    }
    let index = agent.index;
    let mut guard = RetirementGuard {
        index,
        tx: tx.clone(),
        agent: Some(agent),
        sent: false,
        crash_recovery: true,
    };
    let command = config.agent_command.clone();
    let args = config.agent_args.clone();
    let env = config.persona_env_vars.clone();
    let has_codex = config.has_generated_codex_config;
    tasks.spawn(async move {
        let report = match guard.agent.as_mut() {
            Some(agent) => agent.acp.terminate().await,
            None => TerminationReport::Unverified,
        };
        if let Some(work) = &work {
            work.retired(report == TerminationReport::Clean);
        }
        if report != TerminationReport::Clean {
            guard.send(
                Err(anyhow::anyhow!("crashed Kimi retirement unconfirmed")),
                report,
            );
            return;
        }
        guard.agent.take();
        let Some(delay) = delay else {
            guard.send(Err(anyhow::anyhow!("Kimi crash circuit is open")), report);
            return;
        };
        tokio::time::sleep(delay).await;
        let result = spawn_and_init(&command, &args, &env, has_codex, index, observer).await;
        guard.send(result, report);
    });
}

/// Stop finished/cancelled/expired background work even without another event
/// or a new mention. Replacement is gated by the same checked-stop boundary.
pub(crate) fn retire_finished_kimi_background(
    pool: &mut AgentPool,
    config: &Config,
    circuits: &mut [SlotCircuit],
    tx: &mpsc::Sender<RespawnResult>,
    tasks: &mut tokio::task::JoinSet<()>,
    observer: Option<observer::ObserverHandle>,
) {
    for (index, scope) in pool.expired_kimi_background_workers() {
        if circuits[index].respawn_in_flight {
            continue;
        }
        let Some(mut agent) = pool.take_retirement_target(index, &HashSet::from([scope]), true)
        else {
            continue;
        };
        if let Some(work) = agent.acp.logical_work() {
            if !work.cancelled() {
                work.effect_started("background-result-unconfirmed");
                work.human_required("external_effect_uncertain");
            }
        }
        agent.state.invalidate_all();
        pool.mark_retire_on_return(index);
        circuits[index].respawn_in_flight = true;
        recover_kimi(
            agent,
            config,
            Some(Duration::ZERO),
            tx,
            tasks,
            observer.clone(),
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn drive(
    pool: &mut AgentPool,
    controls: &session_controls::SessionControlRuntime,
    parked: &mut HashMap<usize, Option<OwnedAgent>>,
    config: &Config,
    circuits: &mut [SlotCircuit],
    tx: &mpsc::Sender<RespawnResult>,
    tasks: &mut tokio::task::JoinSet<()>,
    observer: Option<observer::ObserverHandle>,
) {
    for (index, scopes, expired) in controls.retiring_workers() {
        pool.mark_retire_on_return(index);
        let agent = if let Some(agent) = parked.remove(&index) {
            agent
        } else {
            let Some(agent) = pool.take_retirement_target(index, &scopes, expired) else {
                continue;
            };
            Some(agent)
        };
        // This is a deliberate replacement, never a circuit-breaker crash.
        circuits[index].respawn_in_flight = true;
        controls.retirement_started(index);
        let mut guard = RetirementGuard {
            index,
            tx: tx.clone(),
            agent,
            sent: false,
            crash_recovery: false,
        };
        let command = config.agent_command.clone();
        let args = config.agent_args.clone();
        let env = config.persona_env_vars.clone();
        let has_codex = config.has_generated_codex_config;
        let observer = observer.clone();
        let controls = controls.clone();
        tasks.spawn(async move {
            let report = match guard.agent.as_mut() {
                Some(agent) => agent.acp.terminate().await,
                None => TerminationReport::Clean,
            };
            if report != TerminationReport::Clean {
                guard.send(
                    Err(anyhow::anyhow!("old worker retirement unconfirmed")),
                    report,
                );
                return;
            }
            if let Some(agent) = guard.agent.as_mut() {
                if let Err(error) = agent.prepare_retired_neighbors(&scopes, &controls) {
                    guard.send(
                        Err(anyhow::anyhow!(
                            "neighbor departure could not be persisted: {error}"
                        )),
                        TerminationReport::Unverified,
                    );
                    return;
                }
            }
            guard.agent.take();
            let result = spawn_and_init(&command, &args, &env, has_codex, index, observer).await;
            guard.send(result, report);
        });
    }
}

pub(crate) fn finish(
    pool: &mut AgentPool,
    controls: &session_controls::SessionControlRuntime,
    parked: &mut HashMap<usize, Option<OwnedAgent>>,
    result: &mut RespawnResult,
) -> bool {
    if controls.retirement_evidence_lost(result.index)
        || (result.retirement.is_none() && controls.owns_retirement(result.index))
    {
        pool.mark_retire_on_return(result.index);
        controls.lose_retirement_evidence(result.index);
        tracing::error!(
            agent = result.index,
            "replacement cannot verify the lost original worker; restart remains unconfirmed"
        );
        return true;
    }
    let Some(retirement) = result.retirement.take() else {
        return false;
    };
    if retirement.report != TerminationReport::Clean
        || (result.result.is_err() && !retirement.crash_recovery)
    {
        parked.insert(result.index, retirement.retained);
        controls.retirement_report(
            result.index,
            if retirement.report == TerminationReport::Clean {
                TerminationReport::Unverified
            } else {
                retirement.report
            },
        );
        return true;
    }
    pool.finish_worker_retirement(result.index);
    controls.retirement_report(result.index, TerminationReport::Clean);
    false
}

/// Drain every retirement owner after aborting and awaiting the task guards.
/// Unverified originals remain owned by the caller until final harness exit.
pub(crate) async fn shutdown(
    parked: &mut HashMap<usize, Option<OwnedAgent>>,
    tasks: &mut tokio::task::JoinSet<()>,
    rx: &mut mpsc::Receiver<RespawnResult>,
) -> Vec<(usize, OwnedAgent, TerminationReport)> {
    // Await cancellation before draining: RetirementGuard returns its original
    // client synchronously on drop, including cancellation during a snapshot.
    tasks.shutdown().await;
    let mut originals: Vec<_> = parked
        .drain()
        .filter_map(|(index, agent)| agent.map(|agent| (index, agent)))
        .collect();
    while let Ok(result) = rx.try_recv() {
        if let Some(agent) = result.retirement.and_then(|retirement| retirement.retained) {
            originals.push((result.index, agent));
        }
        if let Ok((mut acp, _, _)) = result.result {
            acp.shutdown().await;
        }
    }
    // The existing pool bounds the set. Each checked attempt has its existing
    // snapshot/TERM/KILL/verification bounds; concurrent cleanup avoids adding
    // those grace periods once per retained worker during orderly shutdown.
    futures_util::future::join_all(originals.into_iter().map(|(index, mut agent)| async move {
        let report = agent.acp.terminate().await;
        if report == TerminationReport::Clean {
            tracing::debug!(agent = index, "reaped retained original worker on shutdown");
            None
        } else {
            tracing::warn!(
                agent = index,
                ?report,
                "retained original worker shutdown remains unconfirmed"
            );
            Some((index, agent, report))
        }
    }))
    .await
    .into_iter()
    .flatten()
    .collect()
}

#[cfg(all(test, unix))]
#[path = "worker_retirement_shutdown_tests.rs"]
mod shutdown_tests;

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[tokio::test]
    async fn abort_during_initial_snapshot_returns_live_worker_without_signalling() {
        let acp = AcpClient::spawn(
            "/usr/bin/python3",
            &["-c".into(), "import sys; sys.stdin.read()".into()],
            &[],
            false,
        )
        .await
        .unwrap();
        let pid = acp.process_id().unwrap();
        let agent = OwnedAgent {
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
        };
        let (tx, mut rx) = mpsc::channel(1);
        let entered = Arc::new(tokio::sync::Notify::new());
        let signal = entered.clone();
        let task = tokio::spawn(async move {
            let mut guard = RetirementGuard {
                crash_recovery: false,
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
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let mut returned = rx.recv().await.unwrap().retirement.unwrap();
        assert_eq!(returned.report, TerminationReport::Unverified);
        let mut agent = returned.retained.take().unwrap();
        assert_eq!(agent.acp.process_id(), Some(pid));
        assert!(nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), None).is_ok());
        agent.acp.shutdown().await;
    }
}
