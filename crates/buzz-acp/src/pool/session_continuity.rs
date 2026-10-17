//! Session continuity: provider sessions that outlive their worker process.
//!
//! On a session miss the next turn in a scope first consults the durable
//! [`SessionStore`] and resumes the provider session with ACP `session/resume`
//! when that is safe. Every acquisition runs in rounds of
//! *decision → claim → RPC → commit with the same lease*: the claim happens
//! before the provider call, and a commit that finds the record taken over in
//! the meantime (a rotate, a channel removal, another owner) drops the session
//! it just obtained and starts a new round.
//!
//! [`SessionState`] keeps the lease of every scope it owns, so its invalidation
//! primitives can tombstone, mark departed, or update exactly the records it
//! still owns and never a record another owner has claimed since.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

use uuid::Uuid;

use super::{
    create_or_resume_session_and_apply_model, AcpError, AgentPool, ChannelDeliveryState,
    NewSessionChannelContext, OwnedAgent, PromptContext, SessionState,
};
use crate::acp::resume_failure_code;
use crate::scope::SessionScope;
use crate::session_store::{
    append_delivered, Departure, Expect, LiveSession, LoadOutcome, SessionReason, SessionStore,
    StoreError,
};

/// Acquisition rounds before a turn gives up on owning the scope's record.
const MAX_ACQUIRE_ROUNDS: usize = 3;

/// How the provider session for a turn came to be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SessionStart {
    New { reason: SessionReason },
    Resumed { reason: SessionReason },
}

impl SessionStart {
    pub(crate) fn is_resumed(&self) -> bool {
        matches!(self, Self::Resumed { .. })
    }
}

/// What the deciding worker knows about itself.
pub(crate) struct ResumeContext {
    pub adapter: String,
    pub cwd: String,
    pub model: Option<String>,
    pub instance: Uuid,
    pub worker_index: usize,
    pub resume_supported: bool,
    pub poisoned: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ResumeDecision {
    Resume {
        live: LiveSession,
        reason: SessionReason,
    },
    New {
        reason: SessionReason,
    },
}

/// Decide whether a scope's stored session may be resumed by this worker.
pub(crate) fn resume_decision(
    record: &LoadOutcome,
    me: &ResumeContext,
    instance_alive: impl Fn(Uuid) -> bool,
) -> ResumeDecision {
    let new = |reason| ResumeDecision::New { reason };
    if me.poisoned {
        return new(SessionReason::ResumeFailed("not_persisted".into()));
    }
    let live = match record {
        LoadOutcome::Missing => return new(SessionReason::FirstInThread),
        LoadOutcome::Unreadable => {
            return new(SessionReason::ResumeFailed("store_unreadable".into()))
        }
        LoadOutcome::Found(record) => match &record.live {
            None => return new(SessionReason::Discarded),
            Some(live) => live,
        },
    };
    // A claim whose provider call never completed has no session to resume.
    if live.session_id.is_empty() {
        return new(SessionReason::FirstInThread);
    }
    if live.adapter != me.adapter || live.cwd != me.cwd || live.model != me.model {
        return new(SessionReason::ConfigChanged);
    }
    if !me.resume_supported {
        return new(SessionReason::ResumeUnsupported);
    }
    let resume = |reason| ResumeDecision::Resume {
        live: live.clone(),
        reason,
    };
    if live.instance != me.instance {
        return if instance_alive(live.instance) {
            new(SessionReason::LiveElsewhere)
        } else {
            resume(SessionReason::HarnessRestart)
        };
    }
    if let Some(departure) = live.departed {
        return resume(departure.into());
    }
    if live.worker_index != me.worker_index {
        return new(SessionReason::LiveElsewhere);
    }
    // This slot was respawned without marking its sessions departed.
    resume(SessionReason::WorkerCrash)
}

/// Open the agent's session store, or `None` when resume is disabled or the
/// directory cannot be used.
pub(crate) fn open_session_store(
    configured_dir: Option<PathBuf>,
    resume_enabled: bool,
    agent_pubkey_hex: &str,
    relay_url: &str,
) -> Option<Arc<SessionStore>> {
    if !resume_enabled {
        return None;
    }
    let dir = match configured_dir {
        Some(dir) => dir,
        None => std::env::current_dir().ok()?.join(".buzz-sessions"),
    };
    match SessionStore::open(dir.clone(), relay_url, agent_pubkey_hex) {
        Ok(store) => Some(Arc::new(store)),
        Err(err) => {
            tracing::warn!(
                target: "session_store",
                dir = %dir.display(),
                "session store unavailable, sessions will not resume: {err}"
            );
            None
        }
    }
}

impl SessionState {
    /// The result handler replaces workers after transport failures. Preserve
    /// their provider sessions just as on an observed process exit.
    pub(super) fn invalidate_after_error(
        &mut self,
        source: &super::PromptSource,
        error: &AcpError,
    ) {
        if matches!(
            error,
            AcpError::Io(_)
                | AcpError::WriteTimeout(_)
                | AcpError::Timeout(_)
                | AcpError::Protocol(_)
        ) {
            self.invalidate_all();
        } else {
            self.invalidate(source);
        }
    }

    pub(crate) fn with_store(store: Option<Arc<SessionStore>>) -> Self {
        Self {
            store,
            ..Self::default()
        }
    }

    /// Tombstone the record this state owns for `scope`, if any, and retire
    /// its status generation.
    pub(super) fn discard_durable(&mut self, scope: &SessionScope) {
        self.retire_status(scope);
        if let (Some(store), Some(lease)) = (&self.store, self.leases.remove(scope)) {
            let _ = store.discard(scope, lease);
        }
    }

    /// Mark every owned record departed and forget the leases: the worker's
    /// memory is gone, the provider sessions are not.
    pub(super) fn depart_durable_all(&mut self, departure: Departure) {
        self.retire_all_status();
        let leases = std::mem::take(&mut self.leases);
        if let Some(store) = &self.store {
            for (scope, lease) in leases {
                let _ = store.mark_departed(&scope, lease, departure);
            }
        }
    }

    /// Mark every owned record departed while keeping memory and leases
    /// (the pool is about to sleep).
    pub(crate) fn depart_all(&mut self, departure: Departure) {
        self.retire_all_status();
        if let Some(store) = &self.store {
            for (scope, lease) in &self.leases {
                let _ = store.mark_departed(scope, *lease, departure);
            }
        }
    }

    /// Record a successful delivery in the owned record.
    pub(super) fn persist_delivery(
        &mut self,
        scope: &SessionScope,
        standing_context_sent: bool,
        event_ids: &[String],
        hydrated_thread_roots: &[String],
    ) {
        self.commit_owned(scope, |live| {
            live.standing_context_sent |= standing_context_sent;
            append_delivered(&mut live.delivered_event_ids, event_ids.iter().cloned());
            for root in hydrated_thread_roots {
                if !live.hydrated_thread_roots.contains(root) {
                    live.hydrated_thread_roots.push(root.clone());
                }
            }
            let excess = live
                .hydrated_thread_roots
                .len()
                .saturating_sub(super::MAX_HYDRATED_THREAD_ROOTS_PER_SCOPE);
            live.hydrated_thread_roots.drain(..excess);
        });
    }

    /// Record the scope's turn counter after it was incremented.
    pub(crate) fn persist_turns(&mut self, scope: &SessionScope) {
        let turns = self.turn_counts.get(scope).copied().unwrap_or(0);
        self.commit_owned(scope, |live| live.turns = turns);
    }

    fn commit_owned(&mut self, scope: &SessionScope, change: impl FnOnce(&mut LiveSession)) {
        let (Some(store), Some(lease)) = (&self.store, self.leases.get(scope).copied()) else {
            return;
        };
        if let Err(StoreError::Conflict) = store.commit(scope, lease, change) {
            // Another owner took the record; this state no longer speaks for it.
            self.leases.remove(scope);
        }
    }

    fn restore_delivery(&mut self, scope: &SessionScope, live: LiveSession) {
        self.deliveries.insert(
            scope.clone(),
            ChannelDeliveryState {
                standing_context_sent: live.standing_context_sent,
                delivered_event_ids: live.delivered_event_ids.into_iter().collect(),
                hydrated_thread_roots: VecDeque::from(live.hydrated_thread_roots),
            },
        );
        if live.turns > 0 {
            self.turn_counts.insert(scope.clone(), live.turns);
        }
    }
}

impl AgentPool {
    /// Attach the session store to the pool and to every worker in it.
    pub(crate) fn set_session_store(&mut self, store: Option<Arc<SessionStore>>) {
        for agent in self.agents.iter_mut().flatten() {
            agent.state.store = store.clone();
        }
        self.store = store;
    }

    /// Before the pool sleeps: mark every idle worker's owned sessions
    /// departed so the next turn resumes them.
    pub(crate) fn depart_idle_sessions(&mut self, departure: Departure) {
        for agent in self.agents.iter_mut().flatten() {
            agent.state.depart_all(departure);
        }
    }

    /// Owner rotate: tombstone the scope's record whoever owns it, including
    /// a session that lives only on disk after a sleep or respawn. Leases in
    /// memory stay, so a late write of the previous owner is a conflict.
    pub(crate) fn discard_durable_scope(&mut self, scope: &SessionScope) {
        if let Some(store) = &self.store {
            let _ = store.discard_scope(scope);
        }
    }

    /// Channel removal: tombstone every record of the channel.
    pub(crate) fn discard_durable_channel(&mut self, channel_id: Uuid) {
        if let Some(store) = &self.store {
            let _ = store.discard_channel(channel_id);
        }
    }
}

/// Provide a provider session for `scope` on a session miss: resume the
/// stored one when safe, otherwise create a new one. On success the session's
/// delivery state is restored for a resumed session; the caller records the
/// session id and, for a new session, a fresh delivery state.
pub(super) async fn acquire_session(
    agent: &mut OwnedAgent,
    ctx: &PromptContext,
    agent_core: Option<&str>,
    channel: NewSessionChannelContext<'_>,
    scope: &SessionScope,
) -> Result<(String, SessionStart), AcpError> {
    let Some(store) = agent.state.store.clone() else {
        let sid =
            create_or_resume_session_and_apply_model(agent, ctx, agent_core, channel, None).await?;
        return Ok((
            sid,
            SessionStart::New {
                reason: SessionReason::FirstInThread,
            },
        ));
    };
    for _ in 0..MAX_ACQUIRE_ROUNDS {
        let outcome = store.load(scope);
        let me = ResumeContext {
            adapter: agent.agent_name.clone(),
            cwd: ctx.cwd.clone(),
            model: agent.desired_model.clone(),
            instance: store.instance(),
            worker_index: agent.index,
            resume_supported: agent.acp.resume_supported(),
            poisoned: store.is_poisoned(scope),
        };
        let decision = resume_decision(&outcome, &me, |instance| store.instance_alive(instance));
        if let ResumeDecision::New {
            reason: SessionReason::ResumeFailed(code),
        } = &decision
        {
            if code == "not_persisted" {
                return new_unpersisted(
                    agent,
                    ctx,
                    agent_core,
                    channel,
                    decision_reason(&decision),
                )
                .await;
            }
        }
        let claim = match &decision {
            ResumeDecision::Resume { live, .. } => LiveSession {
                instance: me.instance,
                worker_index: me.worker_index,
                departed: None,
                ..live.clone()
            },
            ResumeDecision::New { .. } => LiveSession {
                lease: Uuid::nil(),
                adapter: me.adapter.clone(),
                session_id: String::new(),
                cwd: me.cwd.clone(),
                model: me.model.clone(),
                instance: me.instance,
                worker_index: me.worker_index,
                departed: None,
                turns: 0,
                standing_context_sent: false,
                delivered_event_ids: Vec::new(),
                hydrated_thread_roots: Vec::new(),
            },
        };
        let lease = match store.claim(scope, &Expect::from_outcome(&outcome), claim) {
            Ok(lease) => lease,
            Err(StoreError::Conflict) => continue,
            Err(err) => {
                // Without a claim no resume may run; the scope is poisoned.
                let reason = match (&decision, &err) {
                    (ResumeDecision::Resume { .. }, StoreError::Busy) => {
                        SessionReason::ResumeFailed("store_busy".into())
                    }
                    (ResumeDecision::Resume { .. }, _) => {
                        SessionReason::ResumeFailed("not_persisted".into())
                    }
                    (ResumeDecision::New { reason }, _) => reason.clone(),
                };
                return new_unpersisted(agent, ctx, agent_core, channel, reason).await;
            }
        };
        // Owned from here: a crash before commit marks this claim departed.
        agent.state.leases.insert(scope.clone(), lease);

        let (sid, start, restored) = match decision {
            ResumeDecision::Resume { live, reason } => {
                match create_or_resume_session_and_apply_model(
                    agent,
                    ctx,
                    agent_core,
                    channel,
                    Some(&live.session_id),
                )
                .await
                {
                    Ok(sid) => (sid, SessionStart::Resumed { reason }, Some(live)),
                    Err(AcpError::AgentExited) => return Err(AcpError::AgentExited),
                    Err(err) => {
                        let code = resume_failure_code(&err);
                        tracing::warn!(
                            target: "pool::session",
                            "resume of session {} failed ({code}): {err}",
                            live.session_id
                        );
                        let sid = create_or_resume_session_and_apply_model(
                            agent, ctx, agent_core, channel, None,
                        )
                        .await?;
                        (
                            sid,
                            SessionStart::New {
                                reason: SessionReason::ResumeFailed(code),
                            },
                            None,
                        )
                    }
                }
            }
            ResumeDecision::New { reason } => {
                let sid =
                    create_or_resume_session_and_apply_model(agent, ctx, agent_core, channel, None)
                        .await?;
                (sid, SessionStart::New { reason }, None)
            }
        };

        let committed = store.commit(scope, lease, |live| {
            live.session_id = sid.clone();
            if restored.is_none() {
                live.turns = 0;
                live.standing_context_sent = false;
                live.delivered_event_ids.clear();
                live.hydrated_thread_roots.clear();
            }
        });
        match committed {
            Ok(()) | Err(StoreError::Busy | StoreError::Io(_) | StoreError::Unreadable) => {
                if let Some(live) = restored {
                    agent.state.restore_delivery(scope, live);
                }
                return Ok((sid, start));
            }
            Err(StoreError::Conflict) => {
                // The record was taken over during the call: the session just
                // obtained must not be used.
                agent.state.leases.remove(scope);
                tracing::info!(
                    target: "pool::session",
                    "dropping session {sid}: scope {} changed owner during acquisition",
                    scope.telemetry_label()
                );
            }
        }
    }
    new_unpersisted(
        agent,
        ctx,
        agent_core,
        channel,
        SessionReason::LiveElsewhere,
    )
    .await
}

/// The agent-facing marker for the first prompt of a new or resumed session:
/// `<session state="new|resumed" reason="..."/>`, plus
/// `first_mention="not_found"` when a cold start could not find the agent's
/// first mention in the thread.
pub(crate) fn format_session_marker(
    start: &SessionStart,
    first_mention: Option<&super::first_mention::FirstMention>,
) -> String {
    let (state, reason) = match start {
        SessionStart::New { reason } => ("new", reason),
        SessionStart::Resumed { reason } => ("resumed", reason),
    };
    let not_found = matches!(
        first_mention,
        Some(super::first_mention::FirstMention::NotFound)
    );
    format!(
        r#"<session state="{state}" reason="{}"{}/>"#,
        reason.as_wire(),
        if not_found {
            r#" first_mention="not_found""#
        } else {
            ""
        }
    )
}

/// Put the marker right before the conversation context section, or first
/// when the prompt has none.
pub(crate) fn insert_session_marker(sections: &mut Vec<String>, marker: String) {
    let at = sections
        .iter()
        .position(|section| {
            section.starts_with("<thread-context") || section.starts_with("<conversation-context")
        })
        .unwrap_or(0);
    sections.insert(at, marker);
}

fn decision_reason(decision: &ResumeDecision) -> SessionReason {
    match decision {
        ResumeDecision::Resume { reason, .. } | ResumeDecision::New { reason } => reason.clone(),
    }
}

/// A new session that this worker keeps only in memory.
async fn new_unpersisted(
    agent: &mut OwnedAgent,
    ctx: &PromptContext,
    agent_core: Option<&str>,
    channel: NewSessionChannelContext<'_>,
    reason: SessionReason,
) -> Result<(String, SessionStart), AcpError> {
    let sid =
        create_or_resume_session_and_apply_model(agent, ctx, agent_core, channel, None).await?;
    Ok((sid, SessionStart::New { reason }))
}

#[cfg(all(test, unix))]
#[path = "session_continuity_tests.rs"]
mod tests;
