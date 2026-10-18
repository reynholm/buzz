//! Existing-only compaction acquisition and separately typed task completion.
use super::*;
use crate::session_controls::compaction::store_failure;
use crate::session_controls::{AdmittedSessionControl, CompactionTarget};
use crate::session_store::{ControlAttempt, Expect, LoadOutcome};
use buzz_core::session_control::{ControlFailure, ControlOutcome};

/// Only an existing, validated provider session can construct this witness.
pub(crate) struct ExistingSession {
    session_id: String,
}

pub(crate) struct ControlTaskResult {
    pub(crate) agent: OwnedAgent,
    pub(crate) job: AdmittedSessionControl,
    pub(crate) outcome: ControlOutcome,
    pub(crate) transport_failed: bool,
}

impl AgentPool {
    pub(crate) fn compaction_target(
        &self,
        scope: &SessionScope,
        controls: &crate::session_controls::SessionControlRuntime,
    ) -> Result<CompactionTarget, ControlFailure> {
        if let Some(owner) = self.session_owners.get(scope) {
            if let Some(agent) = self.agents.get(owner.agent_index).and_then(Option::as_ref) {
                if let Some(session_id) = agent.state.sessions.get(scope) {
                    return Ok(CompactionTarget::Memory {
                        worker: owner.agent_index,
                        generation: owner.generation,
                        session_id: session_id.clone(),
                    });
                }
            } else {
                // A checked-out owner retains authoritative identity on disk;
                // never send compact to an arbitrary free worker meanwhile.
                return Ok(CompactionTarget::Memory {
                    worker: owner.agent_index,
                    generation: owner.generation,
                    session_id: String::new(),
                });
            }
        }
        let store = controls
            .control_store()
            .ok_or(ControlFailure::StoreUnavailable)?;
        if store.is_poisoned(scope) {
            return Err(ControlFailure::StoreUnavailable);
        }
        match store.load(scope) {
            LoadOutcome::Found(record) => record
                .live
                .filter(|live| !live.session_id.is_empty())
                .map(|live| CompactionTarget::Disk {
                    lease: live.lease,
                    session_id: live.session_id,
                })
                .ok_or(ControlFailure::NoSession),
            LoadOutcome::Missing => Err(ControlFailure::NoSession),
            LoadOutcome::Unreadable => Err(ControlFailure::StoreUnavailable),
        }
    }

    /// No hold timeout, foreground cancellation or background-collection steal.
    pub(crate) fn claim_compaction(
        &mut self,
        job: &AdmittedSessionControl,
    ) -> Result<Option<OwnedAgent>, ControlFailure> {
        let scope = &job.job.scope;
        if self
            .task_map
            .values()
            .any(|meta| meta.scope.as_ref() == Some(scope))
            || self.runtime_holds.values().any(|hold| &hold.scope == scope)
        {
            return Ok(None);
        }
        match &job.target {
            CompactionTarget::Memory {
                worker,
                generation,
                session_id,
            } => {
                if !self.session_owners.get(scope).is_some_and(|owner| {
                    owner.agent_index == *worker && owner.generation == *generation
                }) {
                    return Err(ControlFailure::StaleTarget);
                }
                if self.retire_on_return.contains(worker) || self.runtime_holds.contains_key(worker)
                {
                    return Ok(None);
                }
                let Some(agent) = self.agents.get_mut(*worker).and_then(Option::as_mut) else {
                    return Ok(None);
                };
                if !agent
                    .state
                    .sessions
                    .get(scope)
                    .is_some_and(|sid| session_id.is_empty() || sid == session_id)
                {
                    return Err(ControlFailure::StaleTarget);
                }
                Ok(self.agents[*worker].take())
            }
            CompactionTarget::Disk { .. } => {
                // If an owner appeared since admission, it is a different target.
                if self.session_owners.contains_key(scope) {
                    return Err(ControlFailure::StaleTarget);
                }
                Ok(self.try_claim(None))
            }
        }
    }
}

pub(crate) async fn acquire_existing_session_for_compaction(
    agent: &mut OwnedAgent,
    job: &AdmittedSessionControl,
    ctx: &PromptContext,
) -> Result<ExistingSession, ControlFailure> {
    if !job.dispatch.current() {
        return Err(ControlFailure::Superseded);
    }
    let scope = &job.job.scope;
    match &job.target {
        CompactionTarget::Memory {
            worker,
            generation,
            session_id,
        } => {
            if agent.index != *worker
                || agent.state.scope_owner_generations.get(scope) != Some(generation)
            {
                return Err(ControlFailure::StaleTarget);
            }
            let sid = agent
                .state
                .sessions
                .get(scope)
                .filter(|sid| session_id.is_empty() || *sid == session_id)
                .cloned()
                .ok_or(ControlFailure::NoSession)?;
            if let Some(store) = &agent.state.store {
                if store.is_poisoned(scope) {
                    return Err(ControlFailure::StoreUnavailable);
                }
                match store.load(scope) {
                    LoadOutcome::Found(record)
                        if record.live.as_ref().is_some_and(|live| {
                            live.session_id == sid
                                && live.instance == store.instance()
                                && live.worker_index == agent.index
                                && agent.state.leases.get(scope) == Some(&live.lease)
                        }) => {}
                    LoadOutcome::Unreadable => return Err(ControlFailure::StoreUnavailable),
                    _ => return Err(ControlFailure::StaleTarget),
                }
            }
            agent.acp.ensure_compaction_route(&sid);
            Ok(ExistingSession { session_id: sid })
        }
        CompactionTarget::Disk { lease, session_id } => {
            let store = agent.state.store.clone().ok_or(ControlFailure::NoSession)?;
            if store.is_poisoned(scope) {
                return Err(ControlFailure::StoreUnavailable);
            }
            let outcome = store.load(scope);
            let live = match &outcome {
                LoadOutcome::Found(record) => {
                    record.live.as_ref().ok_or(ControlFailure::NoSession)?
                }
                LoadOutcome::Unreadable => return Err(ControlFailure::StoreUnavailable),
                LoadOutcome::Missing => return Err(ControlFailure::NoSession),
            };
            if live.lease != *lease || live.session_id != *session_id {
                return Err(ControlFailure::StaleTarget);
            }
            if live.adapter != agent.agent_name
                || live.cwd != ctx.cwd
                || live.model != agent.desired_model
            {
                return Err(ControlFailure::StaleTarget);
            }
            if !agent.acp.resume_supported() {
                return Err(ControlFailure::Unsupported);
            }
            if (live.instance != store.instance() && store.instance_alive(live.instance))
                || (live.instance == store.instance()
                    && live.departed.is_none()
                    && live.worker_index != agent.index)
            {
                return Err(ControlFailure::ScopeUnavailable);
            }
            let mut attached = live.clone();
            attached.instance = store.instance();
            attached.worker_index = agent.index;
            attached.departed = None;
            let new_lease = store
                .claim(scope, &Expect::from_outcome(&outcome), attached.clone())
                .map_err(store_failure)?;
            agent.state.leases.insert(scope.clone(), new_lease);
            let resume = tokio::time::timeout_at(
                job.deadline
                    .min(tokio::time::Instant::now() + ctx.max_turn_duration),
                agent
                    .acp
                    .session_resume(session_id, &ctx.cwd, ctx.mcp_servers.clone(), None, None),
            )
            .await;
            match resume {
                Ok(Ok(_)) => {}
                Err(_) => return Err(ControlFailure::Timeout),
                Ok(Err(
                    AcpError::Timeout(_) | AcpError::IdleTimeout(_) | AcpError::HardTimeout { .. },
                )) => return Err(ControlFailure::Timeout),
                Ok(Err(_)) => return Err(ControlFailure::ResumeFailed),
            }
            if !job.dispatch.current() {
                return Err(ControlFailure::Superseded);
            }
            store
                .commit(scope, new_lease, |_| {})
                .map_err(store_failure)?;
            agent.state.restore_delivery(scope, attached);
            agent
                .state
                .sessions
                .insert(scope.clone(), session_id.clone());
            agent.publish_session_start(
                ctx.session_status.as_ref(),
                scope,
                session_id,
                &SessionStart::Resumed {
                    reason: crate::session_store::SessionReason::HarnessRestart,
                },
            );
            Ok(ExistingSession {
                session_id: session_id.clone(),
            })
        }
    }
}

pub(crate) async fn run_compaction_task(
    mut agent: OwnedAgent,
    job: AdmittedSessionControl,
    ctx: Arc<PromptContext>,
    mut control: tokio::sync::oneshot::Receiver<ControlSignal>,
) -> ControlTaskResult {
    let mut transport_failed = false;
    let mut active_session = None;
    let execution = tokio::time::timeout_at(job.deadline, async {
        if tokio::time::Instant::now() >= job.deadline { return Err(ControlFailure::Timeout); }
        if !job.dispatch.current() { return Err(ControlFailure::Superseded); }
        let session = tokio::select! {
            result = acquire_existing_session_for_compaction(&mut agent, &job, &ctx) => match result {
                Ok(session) => session,
                Err(failure) => {
                    transport_failed = matches!(failure, ControlFailure::Timeout | ControlFailure::ResumeFailed);
                    return Err(failure);
                }
            },
            _ = &mut control => {
                transport_failed = true;
                return Err(if job.dispatch.current() { ControlFailure::Cancelled } else { ControlFailure::Superseded });
            },
        };
        active_session = Some(session.session_id.clone());
        if !agent.acp.compact_supported(&session.session_id) { return Err(ControlFailure::Unsupported); }
        agent.acp.begin_compaction(&session.session_id, job.job.origin.clone())?;
        let attempt = match ctx.session_controls.begin_compaction_effect(&job) {
            Ok(attempt) => attempt,
            Err(failure) => { agent.acp.finish_compaction(&session.session_id).ok(); return Err(failure); }
        };
        match attempt {
            ControlAttempt::PriorFinished(prior) => { agent.acp.finish_compaction(&session.session_id).ok(); return prior; }
            ControlAttempt::PriorStarted => { agent.acp.finish_compaction(&session.session_id).ok(); return Err(ControlFailure::Unconfirmed); }
            ControlAttempt::New => {}
        }
        if !job.dispatch.current() { agent.acp.finish_compaction(&session.session_id).ok(); return Err(ControlFailure::Superseded); }
        if tokio::time::Instant::now() >= job.deadline { agent.acp.finish_compaction(&session.session_id).ok(); return Err(ControlFailure::Timeout); }
        let rpc = tokio::select! {
            result = agent.acp.session_prompt_blocks_with_idle_timeout(&session.session_id, &["/compact"], ctx.idle_timeout, ctx.max_turn_duration) => result,
            _ = &mut control => {
                let result = agent.acp.cancel_with_cleanup_grace(&session.session_id, Duration::from_secs(5)).await;
                transport_failed = result.is_err();
                agent.acp.finish_compaction(&session.session_id).ok();
                return Err(if job.dispatch.current() { ControlFailure::Cancelled } else { ControlFailure::Superseded });
            }
        };
        let terminal = agent.acp.finish_compaction(&session.session_id);
        // Controls never publish an ordinary delivered turn or reuse its usage.
        let _ = agent.acp.take_turn_usage();
        match rpc {
            Ok(StopReason::Cancelled) => Err(ControlFailure::Cancelled),
            Ok(_) => terminal,
            Err(error) => {
                transport_failed = matches!(error, AcpError::AgentExited | AcpError::Io(_) | AcpError::Protocol(_) | AcpError::WriteTimeout(_) | AcpError::Timeout(_) | AcpError::IdleTimeout(_) | AcpError::HardTimeout { .. });
                Err(match error {
                    AcpError::Timeout(_) | AcpError::IdleTimeout(_) | AcpError::HardTimeout { .. } => ControlFailure::Timeout,
                    _ if terminal.is_ok() => ControlFailure::Unconfirmed,
                    _ => ControlFailure::CompactFailed,
                })
            }
        }
    }).await;
    let outcome = match execution {
        Ok(outcome) => outcome,
        Err(_) => {
            // The timed-out future borrowed the client; ownership remains here.
            // Bound cleanup separately and never return an uncertain wire as healthy.
            transport_failed = true;
            if let Some(session) = active_session {
                if agent.acp.has_in_flight_prompt() {
                    transport_failed = agent
                        .acp
                        .cancel_with_cleanup_grace(&session, Duration::from_secs(5))
                        .await
                        .is_err();
                } else {
                    transport_failed = false;
                }
                let _ = agent.acp.finish_compaction(&session);
            }
            let _ = agent.acp.take_turn_usage();
            Err(ControlFailure::Timeout)
        }
    };
    let outcome = if job.dispatch.current() {
        outcome
    } else {
        Err(ControlFailure::Superseded)
    };
    ControlTaskResult {
        agent,
        job,
        outcome,
        transport_failed,
    }
}
