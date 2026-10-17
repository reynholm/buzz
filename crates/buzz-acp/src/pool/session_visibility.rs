//! Session visibility: adapt the canonical session scope, the acquisition
//! result and the final cold-start prompt to the status runtime.
//!
//! Only `new` and `resumed` states are produced here. History counts describe
//! the unique message events actually supplied in a new session's first
//! prompt; they come from data the prompt already fetched, never from an
//! extra history walk.

use std::collections::HashSet;

use buzz_core::session_status::{
    HistoryCoverage, SessionState as StatusState, StatusReason, StatusScope, MAX_REASON_BYTES,
};
use uuid::Uuid;

use super::{ConversationContext, OwnedAgent, SessionStart, SessionState};
use crate::scope::SessionScope;
use crate::session_status::SessionStatusRuntime;
use crate::session_store::SessionReason;

const RESUME_FAILED_PREFIX: &str = "resume_failed:";

/// The status scope of a session scope; `None` for a malformed thread root.
pub(crate) fn status_scope(scope: &SessionScope) -> Option<StatusScope> {
    let status = StatusScope {
        channel_id: scope.channel_id(),
        thread_root: scope.root_event_id().map(str::to_owned),
    };
    status.validate().ok()?;
    Some(status)
}

/// Map a harness reason onto the bounded status vocabulary. An unexpected
/// failure code becomes a safe `resume_failed:<token>`, never raw text.
pub(crate) fn status_reason(reason: &SessionReason) -> StatusReason {
    if let Ok(reason) = StatusReason::from_wire(&reason.as_wire()) {
        return reason;
    }
    let code = match reason {
        SessionReason::ResumeFailed(code) => code.as_str(),
        _ => "",
    };
    let budget = MAX_REASON_BYTES - RESUME_FAILED_PREFIX.len();
    let token: String = code
        .chars()
        .map(|c| {
            let c = c.to_ascii_lowercase();
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(budget)
        .collect();
    let token = token.trim_matches('_');
    StatusReason::ResumeFailed(if token.is_empty() {
        "unknown".into()
    } else {
        token.into()
    })
}

/// Retire `scope`'s status whichever worker owns it (owner rotate).
pub(crate) fn retire_scope(runtime: Option<&SessionStatusRuntime>, scope: &SessionScope) {
    if let (Some(runtime), Some(scope)) = (runtime, status_scope(scope)) {
        runtime.retire(&scope);
    }
}

/// Retire every status of `channel_id` (the agent left the channel).
pub(crate) fn retire_channel(runtime: Option<&SessionStatusRuntime>, channel_id: Uuid) {
    if let Some(runtime) = runtime {
        runtime.retire_channel(channel_id);
    }
}

impl SessionState {
    /// Retire this worker's status generation for `scope`, if still current.
    pub(super) fn retire_status(&mut self, scope: &SessionScope) {
        if let Some(binding) = self.status.remove(scope) {
            binding.retire();
        }
    }

    /// Retire every status generation this worker owns.
    pub(super) fn retire_all_status(&mut self) {
        for (_, binding) in self.status.drain() {
            binding.retire();
        }
    }
}

impl OwnedAgent {
    /// Publish the state of a session this turn just created or resumed,
    /// before any provider prompt. Status failures never fail the turn.
    pub(super) fn publish_session_start(
        &mut self,
        runtime: Option<&SessionStatusRuntime>,
        scope: &SessionScope,
        session_id: &str,
        start: &SessionStart,
    ) {
        self.state.retire_status(scope);
        let (Some(runtime), Some(status)) = (runtime, status_scope(scope)) else {
            return;
        };
        let (state, reason) = match start {
            SessionStart::New { reason } => (StatusState::New, reason),
            SessionStart::Resumed { reason } => (StatusState::Resumed, reason),
        };
        if let Some(binding) = runtime.begin(status, session_id, state, status_reason(reason)) {
            self.acp.bind_session_status(session_id, binding.clone());
            self.state.status.insert(scope.clone(), binding);
        }
    }

    /// An ordinary turn keeps its transition: restore a dropped routing entry
    /// while the scope's current generation still belongs to `session_id`.
    pub(super) fn rebind_session_status(
        &mut self,
        runtime: Option<&SessionStatusRuntime>,
        scope: &SessionScope,
        session_id: &str,
    ) {
        if self.acp.has_session_status(session_id) {
            return;
        }
        let binding = runtime
            .zip(status_scope(scope))
            .and_then(|(runtime, status)| runtime.bind_existing(&status, session_id));
        match binding {
            Some(binding) => self.acp.bind_session_status(session_id, binding),
            None => self.acp.clear_session_status(session_id),
        }
    }
}

/// What the cold-start history fetch returned, after first-mention pinning
/// and before the session delta.
pub(super) enum FetchedHistory {
    /// No history was requested (disabled, or a plain channel message).
    NotRequested,
    /// History was requested but nothing came back.
    Missing,
    Fetched {
        thread: bool,
        total: u64,
        complete: bool,
        ids: Vec<String>,
    },
}

impl FetchedHistory {
    pub(super) fn capture(requested: bool, context: Option<&ConversationContext>) -> Self {
        let Some(context) = context else {
            return if requested {
                Self::Missing
            } else {
                Self::NotRequested
            };
        };
        let (thread, messages, total, complete) = match context {
            ConversationContext::Thread {
                messages,
                total,
                root_present,
                truncated,
            } => (true, messages, *total, *root_present && !*truncated),
            ConversationContext::Dm {
                messages,
                total,
                truncated,
            } => (false, messages, *total, !*truncated),
        };
        Self::Fetched {
            thread,
            total: total as u64,
            complete,
            ids: messages
                .iter()
                .filter(|message| !message.event_id.is_empty())
                .map(|message| message.event_id.clone())
                .collect(),
        }
    }
}

/// Coverage of a new session's first prompt.
///
/// `read` is the number of unique identified events supplied (the final
/// context plus the rendered batch); any unidentified supplied message omits
/// it. The total is used only when the fetched history describes the session
/// scope itself and never falls below `read`. Exactness needs a complete
/// fetch, nothing omitted from the prompt, a successful first-mention scan,
/// and `read == total`.
pub(super) fn history_coverage(
    fetched: &FetchedHistory,
    supplied: Option<&ConversationContext>,
    trigger_ids: &HashSet<String>,
    first_mention_incomplete: bool,
    scope: &SessionScope,
    is_dm: bool,
) -> HistoryCoverage {
    let mut ids: HashSet<&str> = trigger_ids.iter().map(String::as_str).collect();
    let mut unidentified = false;
    let messages = match supplied {
        Some(ConversationContext::Thread { messages, .. })
        | Some(ConversationContext::Dm { messages, .. }) => messages.as_slice(),
        None => &[],
    };
    for message in messages {
        if message.event_id.is_empty() {
            unidentified = true;
        } else {
            ids.insert(&message.event_id);
        }
    }
    let read = (!unidentified).then_some(ids.len() as u64);

    let (mut total, complete) = match fetched {
        FetchedHistory::NotRequested => (None, !first_mention_incomplete),
        FetchedHistory::Missing => (None, false),
        FetchedHistory::Fetched {
            thread,
            total,
            complete,
            ids: fetched_ids,
        } => {
            let omitted = fetched_ids.iter().any(|id| !ids.contains(id.as_str()));
            let describes_scope = if *thread {
                scope.is_thread()
            } else {
                is_dm && !scope.is_thread()
            };
            (
                describes_scope.then_some(*total),
                *complete && !omitted && !first_mention_incomplete,
            )
        }
    };
    if let (Some(read), Some(known)) = (read, total) {
        if read > known {
            total = None;
        }
    }
    let incomplete = !complete;
    let exact = complete
        && !matches!(fetched, FetchedHistory::NotRequested)
        && read.is_some()
        && read == total;
    HistoryCoverage {
        read,
        total,
        exact,
        incomplete,
    }
}

#[cfg(all(test, unix))]
#[path = "session_visibility_tests.rs"]
mod tests;
