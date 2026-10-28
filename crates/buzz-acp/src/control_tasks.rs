//! Schedule typed controls without chat queue accounting or affinity forks.
use super::*;
use buzz_core::session_control::ControlFailure;
use pool::compaction::{run_compaction_task, ControlTaskResult};

pub(crate) fn wake_pending(
    queue: &mut EventQueue,
    controls: &session_controls::SessionControlRuntime,
) -> bool {
    queue.has_flushable_work()
        || !controls.pending_compactions().is_empty()
        || !controls.pending_statuses().is_empty()
}

pub(crate) fn dispatch(
    pool: &mut AgentPool,
    ctx: &Arc<PromptContext>,
    tx: &mpsc::UnboundedSender<ControlTaskResult>,
) {
    for mut job in ctx.session_controls.pending_compactions() {
        let refusal = if !job.dispatch.current() {
            Some(ControlFailure::Superseded)
        } else if tokio::time::Instant::now() >= job.deadline {
            // No provider operation started: the exact owner stayed busy.
            Some(ControlFailure::Busy)
        } else {
            None
        };
        if let Some(failure) = refusal {
            ctx.session_controls
                .finish_compaction_job(&job, Err(failure));
            continue;
        }
        let mut agent = match pool.claim_compaction(&job) {
            Ok(Some(agent)) => agent,
            Ok(None) => {
                ctx.session_controls.report_compaction_wait(&job);
                continue;
            }
            Err(failure) => {
                ctx.session_controls
                    .finish_compaction_job(&job, Err(failure));
                continue;
            }
        };
        // Resume and the compact RPC share one budget, starting only once the
        // existing owner is acquired. Waiting never cancels its current work.
        job.deadline =
            tokio::time::Instant::now() + session_controls::compaction::COMPACT_EXECUTION_WAIT;
        if !ctx.session_controls.start_compaction(&job, agent.index) {
            pool.return_agent(agent);
            continue;
        }
        agent.state.control_dispatch = Some(job.dispatch.clone());
        let (control_tx, control_rx) = tokio::sync::oneshot::channel();
        let ctx_clone = ctx.clone();
        let tx = tx.clone();
        let index = agent.index;
        let scope = job.job.scope.clone();
        let dispatch = job.dispatch.clone();
        let turn_id = job.job.origin.command_event_id.clone();
        let handle = pool.join_set.spawn(async move {
            let result = run_compaction_task(agent, job, ctx_clone, control_rx).await;
            if tx.send(result).is_err() {
                tracing::error!(agent = index, "compaction completion receiver closed");
            }
        });
        pool.task_map_mut().insert(
            handle.id(),
            pool::TaskMeta {
                permission_tx: None,
                agent_index: index,
                channel_id: Some(scope.channel_id()),
                scope: Some(scope),
                turn_id,
                recoverable_batch: None,
                control_tx: Some(control_tx),
                steer_tx: None,
                control_dispatch: Some(dispatch),
                successful_steer_deliveries: HashSet::new(),
            },
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn finish(
    pool: &mut AgentPool,
    ctx: &PromptContext,
    mut result: ControlTaskResult,
    removed: &HashSet<Uuid>,
    config: &Config,
    circuits: &mut [SlotCircuit],
    respawn_tx: &mpsc::Sender<RespawnResult>,
    tasks: &mut tokio::task::JoinSet<()>,
    observer: Option<observer::ObserverHandle>,
    last_activity: &mut tokio::time::Instant,
) {
    let index = result.agent.index;
    pool.task_map_mut()
        .retain(|_, meta| meta.agent_index != index);
    ctx.session_controls
        .finish_compaction_job(&result.job, result.outcome);
    if !result.job.dispatch.current() {
        result.agent.state.invalidate_scope(&result.job.job.scope);
    }
    if let Some(dispatch) = result.agent.state.control_dispatch.take() {
        dispatch.finish();
    }
    for channel in removed {
        result.agent.state.invalidate_channel(channel);
    }
    *last_activity = tokio::time::Instant::now();
    if pool.needs_retirement(&result.agent) {
        pool.return_for_retirement(result.agent);
    } else if result.transport_failed {
        result
            .agent
            .state
            .depart_all(crate::session_store::Departure::WorkerCrash);
        pool.clear_runtime_hold(index);
        spawn_respawn_task(
            result.agent,
            config,
            &mut circuits[index],
            respawn_tx,
            tasks,
            observer,
        );
    } else {
        // Disk-only acquisition becomes authoritative only after its leased
        // provider attachment succeeded. It never increments chat turn counters.
        if result
            .agent
            .state
            .sessions
            .contains_key(&result.job.job.scope)
            && !pool.has_session_for(&result.job.job.scope)
        {
            let generation = pool.record_scope_owner(result.job.job.scope.clone(), index);
            result
                .agent
                .state
                .set_scope_owner_generation(result.job.job.scope.clone(), generation);
        }
        pool.return_agent(result.agent);
    }
}
