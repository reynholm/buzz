//! Status turns get finite priority in their scope without owning human queue input.
use super::*;
use pool::restart_status::{run_status_task, StatusTaskResult};

pub(crate) fn dispatch(
    pool: &mut AgentPool,
    ctx: &Arc<PromptContext>,
    tx: &mpsc::UnboundedSender<StatusTaskResult>,
    typing_channels: &mut HashMap<scope::SessionScope, ThreadTags>,
) {
    for mut job in ctx.session_controls.pending_statuses() {
        job.deadline = job
            .deadline
            .min(tokio::time::Instant::now() + ctx.max_turn_duration);
        let scope = job.saved.scope.clone();
        if tokio::time::Instant::now() >= job.deadline {
            if ctx.session_controls.start_status(&job) {
                let dispatch = ctx.session_controls.capture(&scope);
                let ctx = ctx.clone();
                pool.join_set
                    .spawn(pool::restart_status::report_expired(job, ctx, dispatch));
            }
            continue;
        }
        if pool
            .task_map()
            .values()
            .any(|meta| meta.scope.as_ref() == Some(&scope))
        {
            continue;
        }
        let Some(mut agent) = pool.try_claim(Some(&scope)) else {
            continue;
        };
        if !ctx.session_controls.start_status(&job) {
            pool.return_agent(agent);
            continue;
        }
        let generation = pool.record_scope_owner(scope.clone(), agent.index);
        agent
            .state
            .set_scope_owner_generation(scope.clone(), generation);
        let dispatch = ctx.session_controls.capture(&scope);
        agent.state.control_dispatch = Some(dispatch.clone());
        let index = agent.index;
        let turn_id = job.saved.origin.command_event_id.clone();
        // Operational status turns use the same scoped activity heartbeat as
        // ordinary prompts, through preparation and final reply publication.
        typing_channels.insert(
            scope.clone(),
            pool::restart_status::reply_tags(
                &job,
                ctx.channel_info.is_known_dm(scope.channel_id()),
            ),
        );
        let (control_tx, control_rx) = tokio::sync::oneshot::channel();
        let ctx = ctx.clone();
        let tx = tx.clone();
        let task_dispatch = dispatch.clone();
        let handle = pool.join_set.spawn(async move {
            if tx
                .send(run_status_task(agent, job, ctx, task_dispatch, control_rx).await)
                .is_err()
            {
                tracing::error!(agent = index, "restart status completion receiver closed");
            }
        });
        pool.task_map_mut().insert(
            handle.id(),
            pool::TaskMeta {
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
    mut result: StatusTaskResult,
    typing_channels: &mut HashMap<scope::SessionScope, ThreadTags>,
    removed: &HashSet<Uuid>,
    config: &Config,
    circuits: &mut [SlotCircuit],
    respawn_tx: &mpsc::Sender<RespawnResult>,
    tasks: &mut tokio::task::JoinSet<()>,
    observer: Option<observer::ObserverHandle>,
    last_activity: &mut tokio::time::Instant,
) {
    typing_channels.remove(&result.job.saved.scope);
    let index = result.agent.index;
    pool.task_map_mut()
        .retain(|_, meta| meta.agent_index != index);
    ctx.session_controls.finish_status(&result.job);
    if let Some(dispatch) = result.agent.state.control_dispatch.take() {
        if !dispatch.current() {
            result.agent.state.invalidate_scope(&result.job.saved.scope);
        }
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
            .depart_all(session_store::Departure::WorkerCrash);
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
        pool.return_agent(result.agent);
    }
}
