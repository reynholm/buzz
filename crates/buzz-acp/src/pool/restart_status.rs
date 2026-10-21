//! The restart status is a typed operational turn, never a synthetic human event.
use super::*;
use crate::session_controls::status::StatusJob;
use crate::session_store::RestartStatusPhase;
use tokio::time::{timeout_at, Instant};

pub(crate) struct StatusTaskResult {
    pub agent: OwnedAgent,
    pub job: StatusJob,
    pub transport_failed: bool,
}

const REQUEST: &str = "The session has been restarted. Briefly report the current task status using only the context supplied to this session: the task as you understand it, what is known to be done or still open, and the next step if clear. State any missing context explicitly. Distinguish missing task details from missing messages; use the supplied messages and history coverage markers (included, total, truncated) for any coverage claim. Never call a supplied message absent. Omitted history does not prove that no work was done or that a prior request was unfulfilled; report unknown outcomes as unknown. Do not invent progress. Use the same language as the human conversation in this thread, even though this operational request is in English. Write one short message. This is a status request only: do not continue task execution, inspect files or logs, reconstruct omitted history, launch tools or subagents, or send messages yourself. Return only the status text. The harness will sign and deliver your text to this thread; no delivery tool is needed.";

fn protocol(message: &str) -> AcpError {
    AcpError::Protocol(message.into())
}

pub(crate) async fn run_status_task(
    mut agent: OwnedAgent,
    mut job: StatusJob,
    ctx: Arc<PromptContext>,
    dispatch: crate::session_controls::ControlDispatch,
    mut cancel: tokio::sync::oneshot::Receiver<ControlSignal>,
) -> StatusTaskResult {
    let controls = &ctx.session_controls;
    agent.state.control_dispatch = Some(dispatch.clone());
    let result = tokio::select! {
        biased;
        _ = &mut cancel => { controls.cancel_status_for_token(&job.saved.scope, job.saved.token); Err(protocol("restart status cancelled")) },
        result = tokio::time::timeout_at(job.deadline, execute(&mut agent, &mut job, &dispatch, &ctx)) => {
            result.unwrap_or_else(|_| Err(protocol("restart status deadline expired")))
        }
    };
    // Capture must never leak into later normal turns, including timeout paths.
    agent.acp.take_status_text();
    let mut transport_failed = result.is_err();
    if let Err(error) = result {
        tracing::warn!(%error, "restart succeeded but status follow-up was not confirmed");
        if agent.acp.has_in_flight_prompt() {
            transport_failed |= agent
                .acp
                .cancel_with_cleanup_grace(
                    agent
                        .state
                        .sessions
                        .get(&job.saved.scope)
                        .map(String::as_str)
                        .unwrap_or_default(),
                    CONTROL_CANCEL_GRACE,
                )
                .await
                .is_err();
        }
        // Failure feedback has its own small delivery grace. It releases no new
        // model authority and cannot hold human queue priority past 120 seconds.
        if job.saved.active() && dispatch.current() {
            job.deadline = job.deadline.max(Instant::now() + Duration::from_secs(5));
            let _ = timeout_at(
                Instant::now() + Duration::from_secs(5),
                publish_failure(&mut job, &dispatch, &ctx),
            )
            .await;
        }
        if job.saved.active() {
            let prepared = job.saved.prepared.clone();
            let failure_notice = job.saved.failure_notice;
            if let Err(persist_error) = controls.transition_status(
                &mut job,
                RestartStatusPhase::Unconfirmed,
                prepared,
                failure_notice,
                false,
            ) {
                tracing::warn!(%persist_error, "restart status uncertainty remains in its durable attempt record");
            }
        }
    }
    controls.finish_status(&job);
    StatusTaskResult {
        agent,
        job,
        transport_failed,
    }
}

async fn execute(
    agent: &mut OwnedAgent,
    job: &mut StatusJob,
    dispatch: &crate::session_controls::ControlDispatch,
    ctx: &PromptContext,
) -> Result<(), AcpError> {
    let controls = &ctx.session_controls;
    if !controls.status_current(job, dispatch)
        || !controls.status_authorized(&job.saved, ctx).await
        || !controls.status_current(job, dispatch)
    {
        return Err(protocol("restart status authority changed"));
    }
    if job.saved.phase == RestartStatusPhase::Prepared {
        return publish_prepared(job, dispatch, ctx).await;
    }
    if job.saved.phase != RestartStatusPhase::Pending {
        return Err(protocol("restart status attempt already started"));
    }
    // Persist before acquisition or model dispatch. An uncertain Started record
    // cannot authorize another model attempt after reconstruction.
    controls
        .transition_status(job, RestartStatusPhase::Started, None, false, false)
        .map_err(|_| protocol("restart status attempt could not be persisted"))?;
    let scope = job.saved.scope.clone();
    let channel_id = scope.channel_id();
    let channel_info = ctx.channel_info.resolve_channel_metadata(channel_id).await;
    let (is_dm, title, channel_type) =
        resolve_new_session_channel_context(channel_info.as_ref()).await;
    let core = if ctx.memory_enabled {
        if let Some(owner) = &ctx.agent_owner_pubkey {
            timeout(
                Duration::from_secs(3),
                crate::engram_fetch::build_core_section(&ctx.rest_client, &ctx.agent_keys, owner),
            )
            .await
            .ok()
            .flatten()
        } else {
            None
        }
    } else {
        None
    };
    let canvas = if !is_dm {
        fetch_canvas_section(channel_id, &ctx.rest_client).await
    } else {
        None
    };
    let huddle = if let Some(owner) = &ctx.agent_owner_pubkey {
        fetch_huddle_instructions(channel_id, owner, &ctx.rest_client).await
    } else {
        None
    };
    let (session, start) = session_continuity::acquire_session(
        agent,
        ctx,
        core.as_deref(),
        NewSessionChannelContext {
            huddle_instructions: huddle.as_deref(),
            canvas: canvas.as_deref(),
            name: title.as_deref(),
            scope: Some(&scope),
            channel_type: channel_type.as_deref(),
        },
        &scope,
    )
    .await?;
    agent.state.sessions.insert(scope.clone(), session.clone());
    agent
        .state
        .deliveries
        .insert(scope.clone(), ChannelDeliveryState::default());
    if let Some(core) = &core {
        agent
            .state
            .core_sections
            .insert(scope.clone(), core.clone());
    }
    if let Some(canvas) = &canvas {
        agent
            .state
            .canvas_sections
            .insert(scope.clone(), canvas.clone());
    }
    agent.publish_session_start(ctx.session_status.as_ref(), &scope, &session, &start);
    agent.acp.notify_session_spawned(&session);
    let target = context_target(job, is_dm);
    let depth = dispatch
        .intent
        .as_ref()
        .filter(|intent| intent.token == job.saved.token)
        .map(|intent| intent.depth)
        .ok_or_else(|| protocol("restart status lost its captured token"))?;
    let humans = super::status_language::trusted_authors(&job.saved.origin.actor_pubkey, ctx).await;
    // The selected-history reader already has a bounded newest page even for
    // None. Only a language code survives evidence excluded by that policy.
    let (mut context, mention, language) =
        session_history::selected_with_language(&target, channel_id, depth, ctx, &humans).await;
    if depth == buzz_core::session_control::RestartDepth::Recent {
        let recent = if ctx.context_message_limit > 0 {
            fetch_conversation_context_for_target(channel_id, &target, ctx, false, &HashSet::new())
                .await
        } else {
            None
        };
        merge_recent(&mut context, recent);
    }
    let pins = match &mention {
        Some(first_mention::FirstMention::Found(event)) => event["id"]
            .as_str()
            .map(|id| HashSet::from([id.to_owned()]))
            .unwrap_or_default(),
        _ => HashSet::new(),
    };
    session_history::enforce_render_budget(&mut context, None, &pins);
    let ids = conversation_context_event_ids(context.as_ref());
    if let Some(binding) = agent.state.status.get(&scope) {
        let fetched = session_visibility::FetchedHistory::capture(true, context.as_ref());
        binding.set_history(session_visibility::history_coverage(
            &fetched,
            context.as_ref(),
            &HashSet::new(),
            matches!(mention, Some(first_mention::FirstMention::NotFound)),
            &scope,
            is_dm,
        ));
    }
    let standing = crate::queue::StandingContext {
        base_prompt: ctx.base_prompt.as_deref(),
        system_prompt: ctx.system_prompt.as_deref(),
        team_instructions: ctx.team_instructions.as_deref(),
        agent_core: core.as_deref(),
        huddle_instructions: huddle.as_deref(),
        agent_canvas: canvas.as_deref(),
    };
    let mut sections = Vec::new();
    if !agent.has_system_prompt_support() {
        sections.extend(standing.sections());
    }
    sections.push(session_continuity::format_session_marker(
        &start,
        mention.as_ref(),
    ));
    if let Some(context) = &context {
        sections.push(crate::queue::format_conversation_context(context, None));
    }
    let language_instruction = match language {
        "ru" => " Write this status in Russian.",
        "en" => " Write this status in English.",
        _ => "",
    };
    sections.push(format!(
        "<restart-status conversation-language=\"{language}\">\n{REQUEST}{language_instruction}\n</restart-status>"
    ));
    if !controls.status_current(job, dispatch) {
        return Err(protocol("restart status superseded before prompt"));
    }
    agent.acp.begin_status_capture(&session);
    let reason = agent
        .acp
        .session_prompt_with_idle_timeout(
            &session,
            &sections.join("\n\n"),
            ctx.idle_timeout.min(Duration::from_secs(120)),
            ctx.max_turn_duration.min(Duration::from_secs(120)),
        )
        .await?;
    let text = agent
        .acp
        .take_status_text()
        .ok_or_else(|| protocol("restart status returned no complete text"))?;
    if reason != StopReason::EndTurn {
        return Err(protocol("restart status did not complete normally"));
    }
    if !controls.status_current(job, dispatch) {
        return Err(protocol("restart status superseded after prompt"));
    }
    let thread_ref = if is_dm {
        None
    } else {
        let root = nostr::EventId::from_hex(&crate::queue::reply_thread(&job.saved.command, None))
            .map_err(|_| protocol("restart status has no valid thread root"))?;
        Some(buzz_sdk::ThreadRef {
            root_event_id: root,
            parent_event_id: root,
        })
    };
    let status_tags = agent
        .state
        .status
        .get(&scope)
        .and_then(crate::session_status::StatusBinding::event_tags)
        .unwrap_or_default()
        .into_iter()
        .map(nostr::Tag::parse)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| protocol("restart status metadata could not be serialized"))?;
    let event =
        buzz_sdk::build_message(channel_id, &text, thread_ref.as_ref(), &[], false, &[], &[])
            .map_err(|_| protocol("restart status could not be built"))?
            .tags(status_tags)
            .sign_with_keys(&ctx.agent_keys)
            .map_err(|_| protocol("restart status could not be signed"))?;
    controls
        .transition_status(job, RestartStatusPhase::Prepared, Some(event), false, false)
        .map_err(|_| protocol("restart status bytes could not be persisted"))?;
    publish_prepared(job, dispatch, ctx).await?;
    let standing_sent = !agent.has_system_prompt_support();
    record_scope_delivery_success(
        agent,
        scope.clone(),
        standing_sent,
        &ids,
        &match target {
            ContextTarget::Thread(root) => HashSet::from([root]),
            _ => HashSet::new(),
        },
    );
    if ctx.max_turns_per_session > 0 {
        let count = agent.state.turn_counts.entry(scope.clone()).or_insert(0);
        *count = count.saturating_add(1);
        agent.state.persist_turns(&scope);
    }
    let usage = agent.acp.take_turn_usage();
    publish_agent_turn_metric(
        ctx,
        usage,
        Some(channel_id),
        &session,
        &format!("{}:status", job.saved.origin.command_event_id),
        Some(buzz_core::agent_turn_metric::StopReason::EndTurn),
    )
    .await;
    Ok(())
}

fn context_target(job: &StatusJob, is_dm: bool) -> ContextTarget {
    if is_dm {
        ContextTarget::Dm
    } else {
        ContextTarget::Thread(crate::queue::reply_thread(&job.saved.command, None))
    }
}
pub(crate) fn reply_tags(job: &StatusJob, is_dm: bool) -> ThreadTags {
    if is_dm {
        return ThreadTags::default();
    }
    let root = crate::queue::reply_thread(&job.saved.command, None);
    ThreadTags {
        root_event_id: Some(root.clone()),
        parent_event_id: Some(root),
        ..Default::default()
    }
}
fn merge_recent(context: &mut Option<ConversationContext>, recent: Option<ConversationContext>) {
    let Some(recent) = recent else {
        return;
    };
    let (
        Some(ConversationContext::Thread {
            messages,
            total,
            truncated,
            ..
        })
        | Some(ConversationContext::Dm {
            messages,
            total,
            truncated,
        }),
        ConversationContext::Thread {
            messages: tail,
            total: count,
            truncated: cut,
            ..
        }
        | ConversationContext::Dm {
            messages: tail,
            total: count,
            truncated: cut,
        },
    ) = (context.as_mut(), recent)
    else {
        return;
    };
    let mut ids: HashSet<_> = messages
        .iter()
        .map(|message| message.event_id.clone())
        .collect();
    messages.extend(
        tail.into_iter()
            .filter(|message| ids.insert(message.event_id.clone())),
    );
    messages.sort_by(|a, b| (&a.timestamp, &a.event_id).cmp(&(&b.timestamp, &b.event_id)));
    *total = (*total).max(count).max(messages.len());
    *truncated = *truncated || cut || *total > messages.len();
}
async fn publish_prepared(
    job: &mut StatusJob,
    dispatch: &crate::session_controls::ControlDispatch,
    ctx: &PromptContext,
) -> Result<(), AcpError> {
    let event = if job.saved.failure_notice {
        job.saved.notice.clone()
    } else {
        job.saved.prepared.clone()
    }
    .ok_or_else(|| protocol("restart status has no prepared bytes"))?;
    if event.pubkey != ctx.agent_keys.public_key() || event.verify().is_err() {
        return Err(protocol(
            "restart status prepared signer does not match target agent",
        ));
    }
    let channel = job.saved.scope.channel_id().to_string();
    let route = crate::queue::parse_thread_tags(&event);
    let expected_root = if job.saved.scope.is_thread() {
        job.saved.scope.root_event_id().map(str::to_owned)
    } else {
        ctx.channel_info
            .resolve_channel_metadata(job.saved.scope.channel_id())
            .await
            .filter(|info| info.channel_type != "dm")
            .map(|_| crate::queue::reply_thread(&job.saved.command, None))
    };
    if route.root_event_id != expected_root
        || !route.mentioned_pubkeys.is_empty()
        || event
            .tags
            .iter()
            .any(|tag| tag.as_slice().first().is_some_and(|key| key == "broadcast"))
        || !event
            .tags
            .iter()
            .any(|tag| tag.as_slice() == ["h", channel.as_str()])
    {
        return Err(protocol(
            "restart status prepared route does not match its scope",
        ));
    }
    for attempt in 0..3 {
        if !ctx.session_controls.status_current(job, dispatch)
            || !ctx
                .session_controls
                .status_authorized(&job.saved, ctx)
                .await
        {
            return Err(protocol("restart status publication authority changed"));
        }
        // This synchronous fence is the publication authorization point. An
        // already sent HTTP request cannot be retracted by later cancellation.
        if !ctx.session_controls.status_current(job, dispatch) {
            return Err(protocol("restart status publication superseded"));
        }
        if timeout_at(
            job.deadline.min(Instant::now() + Duration::from_secs(5)),
            ctx.rest_client.submit_event_once_exact(&event),
        )
        .await
        .is_ok_and(|result| result.is_ok())
        {
            if !ctx.session_controls.status_current(job, dispatch) {
                return Err(protocol("restart status superseded before confirmation"));
            }
            let mut next = job.saved.clone();
            let consume = !next.failure_notice;
            next.phase = if next.failure_notice {
                RestartStatusPhase::Failed
            } else {
                RestartStatusPhase::Completed
            };
            next.notice_confirmed = next.failure_notice;
            ctx.session_controls
                .persist_status(job, next, consume)
                .map_err(|_| protocol("restart status confirmation could not be persisted"))?;
            return Ok(());
        }
        if attempt < 2 {
            tokio::time::sleep_until(
                job.deadline
                    .min(Instant::now() + Duration::from_millis(250 << attempt)),
            )
            .await;
        }
    }
    Err(protocol("restart status delivery unconfirmed"))
}
async fn publish_failure(
    job: &mut StatusJob,
    dispatch: &crate::session_controls::ControlDispatch,
    ctx: &PromptContext,
) -> Result<(), AcpError> {
    if !ctx.session_controls.status_current(job, dispatch)
        || !ctx
            .session_controls
            .status_authorized(&job.saved, ctx)
            .await
        || !ctx.session_controls.status_current(job, dispatch)
    {
        return Err(protocol("restart status feedback authority changed"));
    }
    let metadata = ctx
        .channel_info
        .resolve_channel_metadata(job.saved.scope.channel_id())
        .await;
    let is_dm = metadata
        .as_ref()
        .is_some_and(|info| info.channel_type == "dm");
    if !ctx.session_controls.status_current(job, dispatch) {
        return Err(protocol(
            "restart status feedback superseded before preparation",
        ));
    }
    let event = build_failure_notice_event(&ctx.agent_keys, job.saved.scope.channel_id(), &reply_tags(job, is_dm), "The session restarted, but I couldn't confirm its task-status reply. Your next message can continue the conversation.").ok_or_else(|| protocol("restart status failure notice could not be signed"))?;
    let mut next = job.saved.clone();
    next.phase = RestartStatusPhase::Prepared;
    next.failure_notice = true;
    if next.notice.is_none() {
        next.notice = Some(event);
    }
    ctx.session_controls
        .persist_status(job, next, false)
        .map_err(|_| protocol("restart status failure notice could not be persisted"))?;
    tokio::time::timeout_at(job.deadline, publish_prepared(job, dispatch, ctx))
        .await
        .map_err(|_| protocol("restart status failure notice deadline expired"))?
}

/// Expiry or a known local panic reports failure without acquiring a provider.
pub(crate) async fn report_expired(
    mut job: StatusJob,
    ctx: Arc<PromptContext>,
    dispatch: crate::session_controls::ControlDispatch,
) {
    job.deadline = Instant::now() + Duration::from_secs(5);
    if timeout_at(job.deadline, publish_failure(&mut job, &dispatch, &ctx))
        .await
        .is_err()
        || job.saved.active()
    {
        let prepared = job.saved.prepared.clone();
        let failure = job.saved.failure_notice;
        if let Err(error) = ctx.session_controls.transition_status(
            &mut job,
            RestartStatusPhase::Unconfirmed,
            prepared,
            failure,
            false,
        ) {
            tracing::warn!(%error, "expired restart status feedback remains uncertain");
        }
    }
    ctx.session_controls.finish_status(&job);
    dispatch.finish();
}
