//! Authorized control admission and generation fences, independent of pool lifetime.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use buzz_core::session_control::{
    control_is_fresh, is_canonical_id, ControlFailure, ControlOrigin, SessionCommand,
};
use uuid::Uuid;

use crate::scope::{SessionPolicy, SessionScope};
use crate::session_store::{LoadOutcome, RestartIntent, SessionStore, StoreError};
use crate::{AuthorizedListenerEvent, PromptContext};

/// Reserved syntax is removed before raw kind/tags are lost. A human target
/// reserves the syntax too; this predicate does not authorize execution.
pub(crate) fn reserved_raw(event: &serde_json::Value) -> bool {
    let Some(kind) = event["kind"].as_u64().and_then(|n| u16::try_from(n).ok()) else {
        return false;
    };
    SessionCommand::parse(kind, event["content"].as_str().unwrap_or_default()).is_some()
        && event["tags"].as_array().is_some_and(|tags| {
            tags.iter().any(|tag| {
                tag[0].as_str() == Some("p") && tag[1].as_str().is_some_and(is_canonical_id)
            })
        })
}

/// Only the existing listener authorization capability can construct a job.
#[derive(Clone)]
pub(crate) struct ControlJob {
    pub command: SessionCommand,
    pub origin: ControlOrigin,
    pub scope: SessionScope,
    pub dispatch_epoch: Uuid,
    pub event: nostr::Event,
}

pub(crate) enum Admission {
    Normal(AuthorizedListenerEvent),
    Ignored,
    Refused {
        event: nostr::Event,
        origin: ControlOrigin,
        failure: ControlFailure,
    },
    Control(ControlJob),
}

/// Authorization precedes this call; subscription matching follows only Normal.
pub(crate) async fn admit_authorized(
    authorized: AuthorizedListenerEvent,
    ctx: &PromptContext,
    now: u64,
) -> Admission {
    let raw = authorized.event();
    let Ok(value) = serde_json::to_value(&raw.event) else {
        return Admission::Ignored;
    };
    if !reserved_raw(&value) {
        return Admission::Normal(authorized);
    }
    let agent = ctx.agent_keys.public_key().to_hex();
    let targets_agent = value["tags"]
        .as_array()
        .is_some_and(|tags| tags.iter().any(|tag| tag[0] == "p" && tag[1] == agent));
    // Stale replays never overwrite a completed acknowledgement.
    if !targets_agent || !control_is_fresh(raw.event.created_at.as_secs(), now) {
        return Admission::Ignored;
    }
    let metadata = ctx
        .channel_info
        .resolve_channel_metadata(raw.channel_id)
        .await;
    if ctx.session_policy == SessionPolicy::Thread
        && !metadata.as_ref().is_some_and(|info| {
            matches!(
                info.channel_type.as_str(),
                "dm" | "stream" | "private" | "forum"
            )
        })
    {
        let (event, actor) = authorized.into_parts();
        let origin = ControlOrigin {
            command_event_id: event.event.id.to_hex(),
            actor_pubkey: actor,
        };
        return Admission::Refused {
            event: event.event,
            origin,
            failure: ControlFailure::ScopeUnavailable,
        };
    }
    let (raw, effective_author) = authorized.into_parts();
    let scope = SessionScope::derive(
        ctx.session_policy,
        raw.channel_id,
        metadata
            .as_ref()
            .is_some_and(|info| info.channel_type == "dm"),
        &raw.event,
    );
    let Some(command) = SessionCommand::parse(raw.event.kind.as_u16(), &raw.event.content) else {
        return Admission::Ignored;
    };
    Admission::Control(ControlJob {
        command,
        origin: ControlOrigin {
            command_event_id: raw.event.id.to_hex(),
            actor_pubkey: effective_author,
        },
        dispatch_epoch: ctx.session_controls.epoch(&scope),
        scope,
        event: raw.event,
    })
}

#[derive(Default)]
struct ScopeControl {
    lifetime: Arc<()>,
    epoch: Uuid,
    retiring: Option<ControlJob>,
    retirement: retirement::Retirement,
}

#[derive(Default)]
struct State {
    scopes: HashMap<SessionScope, ScopeControl>,
    workers: HashMap<usize, retirement::Worker>,
    publications: std::collections::VecDeque<retirement::Publication>,
}

impl State {
    fn publish(&mut self, publication: retirement::Publication) {
        if matches!(
            publication.result,
            buzz_core::session_control::ControlResult::Pending
        ) && self.publications.iter().any(|prior| {
            prior.command_id == publication.command_id
                && matches!(
                    prior.result,
                    buzz_core::session_control::ControlResult::Terminal(_)
                )
        }) {
            return;
        }
        // Receipts retain outcomes beyond this bounded delivery cache.
        self.publications
            .retain(|prior| prior.command_id != publication.command_id);
        if self.publications.len() >= 128 {
            self.publications.pop_front();
            tracing::warn!("session result delivery cache full; durable outcome retained");
        }
        self.publications.push_back(publication);
    }
    fn prune_quiescent(&mut self) {
        self.scopes
            .retain(|_, entry| entry.retiring.is_some() || Arc::strong_count(&entry.lifetime) > 1);
    }
}

/// Shared controller survives sleep, wake and replacement of the disposable pool.
#[derive(Clone, Default)]
pub(crate) struct SessionControlRuntime {
    state: Arc<Mutex<State>>,
    store: Option<Arc<SessionStore>>,
}

impl SessionControlRuntime {
    pub(crate) fn new(store: Option<Arc<SessionStore>>) -> Self {
        Self {
            store,
            ..Self::default()
        }
    }

    fn epoch(&self, scope: &SessionScope) -> Uuid {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .scopes
            .get(scope)
            .map_or(Uuid::nil(), |entry| entry.epoch)
    }

    /// Capture at dispatch, before any asynchronous metadata or provider lookup.
    pub(crate) fn capture(&self, scope: &SessionScope) -> ControlDispatch {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.prune_quiescent();
        let entry = state
            .scopes
            .entry(scope.clone())
            .or_insert_with(|| ScopeControl {
                lifetime: Arc::new(()),
                epoch: Uuid::new_v4(),
                retiring: None,
                retirement: retirement::Retirement::default(),
            });
        ControlDispatch {
            runtime: self.clone(),
            scope: scope.clone(),
            epoch: entry.epoch,
            _lifetime: entry.lifetime.clone(),
            intent: self.restart_intent(scope),
        }
    }

    /// Atomically tombstone the provider and persist intent before fencing work.
    /// A duplicate ID never replaces a newer intent or reopens retirement.
    pub(crate) fn restart(&self, mut job: ControlJob) -> Result<bool, StoreError> {
        let SessionCommand::Restart(depth) = job.command else {
            return Err(StoreError::InvalidControl);
        };
        if !control_is_fresh(
            job.event.created_at.as_secs(),
            nostr::Timestamp::now().as_secs(),
        ) {
            return Err(StoreError::StaleRequest);
        }
        let store = self.store.as_ref().ok_or(StoreError::Unreadable)?;
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let LoadOutcome::Found(record) = store.load(&job.scope) {
            if record
                .control_receipts
                .iter()
                .any(|receipt| receipt.command_id == job.origin.command_event_id)
            {
                return Ok(false);
            }
        }
        if state
            .scopes
            .values()
            .filter(|entry| entry.retiring.is_some())
            .count()
            >= 128
            && state
                .scopes
                .get(&job.scope)
                .is_none_or(|entry| entry.retiring.is_none())
        {
            return Err(StoreError::Busy);
        }
        let intent = RestartIntent {
            token: Uuid::new_v4(),
            depth,
            origin: job.origin.clone(),
        };
        let superseded = store.request_restart_superseding(&job.scope, &intent)?;
        if matches!(store.load(&job.scope), LoadOutcome::Found(record) if record.pending_restart.as_ref().map(|saved| saved.token) != Some(intent.token))
        {
            return Ok(false);
        }
        for command_id in superseded {
            state.publish(retirement::Publication {
                command_id,
                result: buzz_core::session_control::ControlResult::Terminal(Err(
                    ControlFailure::Superseded,
                )),
            });
        }
        let retirement = state
            .scopes
            .remove(&job.scope)
            .map(|entry| entry.retirement)
            .unwrap_or_default();
        job.dispatch_epoch = Uuid::new_v4();
        state.scopes.insert(
            job.scope.clone(),
            ScopeControl {
                lifetime: Arc::new(()),
                epoch: job.dispatch_epoch,
                retiring: Some(job),
                retirement: retirement.reset(),
            },
        );
        Ok(true)
    }

    pub(crate) fn restart_intent(&self, scope: &SessionScope) -> Option<RestartIntent> {
        match self.store.as_ref()?.load(scope) {
            LoadOutcome::Found(record) => record.pending_restart,
            _ => None,
        }
    }

    pub(crate) fn complete_restart(
        &self,
        scope: &SessionScope,
        token: Uuid,
    ) -> Result<(), StoreError> {
        self.store
            .as_ref()
            .ok_or(StoreError::Unreadable)?
            .complete_restart(scope, token)
    }

    pub(crate) fn retirement_pending(&self, scope: &SessionScope) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .scopes
            .get(scope)
            .is_some_and(|entry| entry.retiring.is_some())
    }

    pub(crate) fn pending_scopes(&self) -> Vec<SessionScope> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.prune_quiescent();
        state
            .scopes
            .iter()
            .filter(|(_, entry)| entry.retiring.is_some())
            .map(|(scope, _)| scope.clone())
            .collect()
    }

    /// Explicit channel revocation clears control-only records even without resume.
    pub(crate) fn revoke_channel(&self, channel: Uuid) -> Result<(), StoreError> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.scopes.retain(|scope, entry| {
            if scope.channel_id() != channel {
                return true;
            }
            entry.epoch = Uuid::new_v4();
            // Revocation fences dispatch and clears durable intent, but cannot
            // discard a checked retirement that still owns live processes.
            entry.retiring.is_some()
        });
        drop(state);
        if let Some(store) = &self.store {
            store.discard_channel(channel)?;
        }
        Ok(())
    }
}

/// Per-task snapshot. It never reads a later intent on an old dispatch's behalf.
#[derive(Clone)]
pub(crate) struct ControlDispatch {
    runtime: SessionControlRuntime,
    scope: SessionScope,
    epoch: Uuid,
    _lifetime: Arc<()>,
    pub(crate) intent: Option<RestartIntent>,
}

impl ControlDispatch {
    pub(crate) fn current(&self) -> bool {
        self.runtime.epoch(&self.scope) == self.epoch
    }

    /// Release completed dispatch metadata without reopening a retirement gate.
    pub(crate) fn finish(self) {
        let runtime = self.runtime.clone();
        drop(self);
        runtime
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .prune_quiescent();
    }

    pub(crate) fn foreground_success(&self) {
        if self.current() {
            if let Some(intent) = &self.intent {
                if let Err(error) = self.runtime.complete_restart(&self.scope, intent.token) {
                    tracing::warn!(%error, "restart policy retained after completion write failed");
                }
            }
        }
    }
}

mod retirement;
pub(crate) use retirement::Publication;

pub(crate) mod publish;
