//! Nonblocking replacement of reserved workers; uncertainty retains ownership.
use super::*;
use crate::acp::TerminationReport;

pub(crate) struct RetirementReturn {
    pub report: TerminationReport,
    pub retained: Option<OwnedAgent>,
}

struct RetirementGuard {
    index: usize,
    tx: mpsc::Sender<RespawnResult>,
    agent: Option<OwnedAgent>,
    sent: bool,
}

impl RetirementGuard {
    fn send(&mut self, result: Result<(AcpClient, u32, String)>, report: TerminationReport) {
        let message = RespawnResult {
            index: self.index,
            result,
            retirement: Some(RetirementReturn {
                report,
                retained: self.agent.take(),
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
    if retirement.report != TerminationReport::Clean || result.result.is_err() {
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
