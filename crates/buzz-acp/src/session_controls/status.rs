//! A finite, independently fenced first turn after checked Restart retirement.
use super::*;
use crate::session_store::{RestartStatus, RestartStatusPhase};
use std::collections::HashSet;
use tokio::time::{Duration, Instant};

#[derive(Clone)]
pub(crate) struct StatusJob {
    pub saved: RestartStatus,
    pub deadline: Instant,
    pub busy: bool,
}

#[derive(Clone)]
pub(super) struct Authority {
    respond_to: crate::RespondTo,
    allowlist: HashSet<String>,
    owner: Option<String>,
}

impl SessionControlRuntime {
    pub(crate) fn configure_status_authority(
        &self,
        respond_to: crate::RespondTo,
        allowlist: HashSet<String>,
        owner: Option<String>,
    ) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.status_authority.is_some() {
            let scopes: Vec<_> = state.statuses.keys().cloned().collect();
            for scope in scopes {
                if let Some(entry) = state.scopes.get_mut(&scope) {
                    entry.epoch = Uuid::new_v4();
                }
            }
        }
        state.status_authority = Some(Authority {
            respond_to,
            allowlist,
            owner,
        });
    }

    pub(crate) async fn status_authorized(
        &self,
        status: &RestartStatus,
        ctx: &PromptContext,
    ) -> bool {
        let agent = ctx.agent_keys.public_key().to_hex();
        let metadata = ctx
            .channel_info
            .resolve_channel_metadata(status.scope.channel_id())
            .await;
        let is_dm = metadata
            .as_ref()
            .is_some_and(|info| info.channel_type == "dm");
        if SessionScope::derive(
            ctx.session_policy,
            status.scope.channel_id(),
            is_dm,
            &status.command,
        ) != status.scope
            || !status.command.tags.iter().any(|tag| {
                let parts = tag.as_slice();
                parts.first().is_some_and(|part| part == "p")
                    && parts.get(1).is_some_and(|part| part == &agent)
            })
        {
            return false;
        }
        let authority = self
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .status_authority
            .clone();
        let Some(authority) = authority else {
            return false;
        };
        let mut gate = crate::InboundAuthorGate::connect(
            &ctx.rest_client,
            &ctx.agent_keys.public_key().to_hex(),
            "restart status",
        )
        .await;
        let owner = crate::OwnerCache::new(authority.owner);
        let event = crate::relay::BuzzEvent {
            channel_id: status.scope.channel_id(),
            event: status.command.clone(),
            connection_generation: 0,
        };
        let Some(admitted) = crate::authorize_normal_listener_event(
            &mut gate,
            event,
            &authority.respond_to,
            &authority.allowlist,
            &owner,
            &ctx.channel_info,
            &ctx.rest_client,
        )
        .await
        else {
            return false;
        };
        let (_, actor) = admitted.into_parts();
        actor == status.origin.actor_pubkey
    }

    pub(super) fn status_ready(
        state: &mut State,
        store: Option<&SessionStore>,
        scope: &SessionScope,
    ) {
        let Some(store) = store else {
            return;
        };
        if let LoadOutcome::Found(record) = store.load(scope) {
            if let Some(saved) = record.restart_status.filter(|status| status.active()) {
                let remaining = saved
                    .deadline
                    .saturating_sub(nostr::Timestamp::now().as_secs())
                    .min(120);
                state.statuses.insert(
                    scope.clone(),
                    StatusJob {
                        saved,
                        deadline: Instant::now() + Duration::from_secs(remaining),
                        busy: false,
                    },
                );
            }
        }
    }

    pub(crate) fn pending_statuses(&self) -> Vec<StatusJob> {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .statuses
            .values()
            .filter(|job| !job.busy)
            .cloned()
            .collect()
    }
    // Cancellation targets include waiting retirement and scheduled workerless
    // feedback, which are deliberately excluded from the model scheduling list.
    pub(crate) fn cancellable_status_scopes(&self) -> Vec<SessionScope> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state
            .statuses
            .keys()
            .cloned()
            .chain(
                state
                    .scopes
                    .iter()
                    .filter(|(_, entry)| {
                        entry.restart_status_token.is_some()
                            && entry
                                .retiring
                                .as_ref()
                                .is_some_and(|job| job.dispatch_epoch == entry.epoch)
                    })
                    .map(|(scope, _)| scope.clone()),
            )
            .collect::<HashSet<_>>()
            .into_iter()
            .collect()
    }
    pub(crate) fn status_pending(&self, scope: &SessionScope) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .statuses
            .get(scope)
            .is_some_and(|job| Instant::now() < job.deadline)
    }
    pub(crate) fn status_deadline(&self) -> Option<Instant> {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .statuses
            .values()
            .filter(|job| !job.busy)
            .map(|job| job.deadline)
            .min()
    }
    pub(crate) fn start_status(&self, job: &StatusJob) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let Some(pending) = state
            .statuses
            .get_mut(&job.saved.scope)
            .filter(|pending| pending.saved == job.saved && !pending.busy)
        else {
            return false;
        };
        pending.busy = true;
        true
    }
    pub(crate) fn status_current(&self, job: &StatusJob, dispatch: &ControlDispatch) -> bool {
        if !dispatch.current() || Instant::now() >= job.deadline {
            return false;
        }
        let Some(store) = self.store.as_ref() else {
            return false;
        };
        matches!(store.load(&job.saved.scope), LoadOutcome::Found(record) if record.pending_restart.as_ref().map(|intent| intent.token) == Some(job.saved.token) && record.restart_status.as_ref() == Some(&job.saved) && job.saved.active())
    }
    pub(crate) fn persist_status(
        &self,
        job: &mut StatusJob,
        next: RestartStatus,
        consume: bool,
    ) -> Result<(), StoreError> {
        self.store
            .as_ref()
            .ok_or(StoreError::Unreadable)?
            .status_transition(&job.saved, &next, consume)?;
        job.saved = next;
        Ok(())
    }
    pub(crate) fn transition_status(
        &self,
        job: &mut StatusJob,
        phase: RestartStatusPhase,
        prepared: Option<nostr::Event>,
        failure_notice: bool,
        consume: bool,
    ) -> Result<(), StoreError> {
        let store = self.store.as_ref().ok_or(StoreError::Unreadable)?;
        let mut next = job.saved.clone();
        next.phase = phase;
        next.prepared = prepared;
        next.failure_notice = failure_notice;
        next.owner = store.instance();
        store.status_transition(&job.saved, &next, consume)?;
        job.saved = next;
        Ok(())
    }
    pub(crate) fn finish_status(&self, job: &StatusJob) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state
            .statuses
            .get(&job.saved.scope)
            .is_some_and(|pending| pending.saved.token == job.saved.token)
        {
            state.statuses.remove(&job.saved.scope);
        }
    }
    pub(crate) fn cancel_status(&self, scope: &SessionScope) -> bool {
        self.cancel_status_outcome(scope) != StatusCancellation::None
    }
    pub(crate) fn cancel_status_outcome(&self, scope: &SessionScope) -> StatusCancellation {
        self.cancel_status_expected(scope, None)
    }
    pub(crate) fn cancel_status_for_token(&self, scope: &SessionScope, token: Uuid) -> bool {
        self.cancel_status_expected(scope, Some(token)) != StatusCancellation::None
    }
    fn cancel_status_expected(
        &self,
        scope: &SessionScope,
        token: Option<Uuid>,
    ) -> StatusCancellation {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let local_token = state
            .statuses
            .get(scope)
            .map(|job| job.saved.token)
            .or_else(|| {
                state
                    .scopes
                    .get(scope)
                    .filter(|entry| entry.retiring.is_some())
                    .and_then(|entry| entry.restart_status_token)
            });
        if token
            .zip(local_token)
            .is_some_and(|(expected, current)| expected != current)
        {
            return StatusCancellation::None;
        }
        // Local cancellation authority comes from the accepted job, not a
        // successful store read. Revoke it before any potentially failing I/O.
        if local_token.is_some() {
            state.statuses.remove(scope);
            if let Some(entry) = state.scopes.get_mut(scope) {
                entry.epoch = Uuid::new_v4();
            }
        }
        let saved = match self.store.as_ref().map(|store| store.load(scope)) {
            Some(LoadOutcome::Found(record)) => record.restart_status,
            _ if local_token.is_some() => {
                tracing::warn!("restart status cancellation record unavailable; local effects fenced and persistence unconfirmed");
                return StatusCancellation::Unconfirmed;
            }
            _ => return StatusCancellation::None,
        };
        let Some(saved) = saved.filter(|saved| {
            (saved.active() || saved.phase == RestartStatusPhase::WaitingRetirement)
                && token.is_none_or(|token| saved.token == token)
                && local_token.is_none_or(|local| saved.token == local)
        }) else {
            // Confirmed terminal status is preserved even if the busy cache lags.
            return StatusCancellation::None;
        };
        if local_token.is_none() {
            if let Some(entry) = state.scopes.get_mut(scope) {
                entry.epoch = Uuid::new_v4();
            }
        }
        let mut job = StatusJob {
            saved,
            deadline: Instant::now(),
            busy: false,
        };
        let prepared = job.saved.prepared.clone();
        let failure = job.saved.failure_notice;
        let persistence = self.transition_status(
            &mut job,
            RestartStatusPhase::Cancelled,
            prepared,
            failure,
            false,
        );
        if let Err(error) = &persistence {
            tracing::warn!(%error, "restart status cancellation could not be persisted; attempt remains uncertain");
        }
        if persistence.is_ok() {
            StatusCancellation::Cancelled
        } else {
            StatusCancellation::Unconfirmed
        }
    }
    /// A known-local panic authorizes failure feedback, never a model replay.
    pub(crate) fn status_panicked(&self, scope: &SessionScope, token: Uuid) -> Option<StatusJob> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state
            .statuses
            .get(scope)
            .filter(|job| job.saved.token == token)?;
        let store = self.store.as_ref()?;
        let LoadOutcome::Found(record) = store.load(scope) else {
            return None;
        };
        let saved = record.restart_status.filter(|saved| {
            saved.token == token && saved.owner == store.instance() && saved.active()
        })?;
        let job = StatusJob {
            saved,
            deadline: Instant::now(),
            busy: true,
        };
        state.statuses.insert(scope.clone(), job.clone());
        if let Some(entry) = state.scopes.get_mut(scope) {
            entry.epoch = Uuid::new_v4();
        }
        Some(job)
    }
    pub(crate) fn shutdown_statuses(&self) -> HashSet<SessionScope> {
        let scopes: Vec<_> = self
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .statuses
            .keys()
            .cloned()
            .collect();
        for scope in &scopes {
            self.cancel_status(scope);
        }
        scopes.into_iter().collect()
    }
}

/// Cancellation signals are immediate; durable confirmation is independent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusCancellation {
    None,
    Cancelled,
    Unconfirmed,
}
