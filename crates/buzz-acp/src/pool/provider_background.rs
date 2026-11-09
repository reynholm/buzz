//! Connect negotiated provider tasks to the existing session/collection owner.
use super::*;
use crate::provider_background::{now_unix, Identity, Record, SavedEvent};
use crate::queue::EventQueue;
use crate::session_store::LoadOutcome;

impl OwnedAgent {
    fn configure_permission_context(
        &mut self,
        scope: &SessionScope,
        batch: &FlushBatch,
        session: &str,
    ) {
        let (Some(store), Some(lease), Some(continuation)) = (
            self.state.store.clone(),
            self.state.leases.get(scope),
            self.background_context
                .as_ref()
                .and_then(|hold| hold.continuation.as_ref()),
        ) else {
            return;
        };
        let identity = self
            .acp
            .provider_background_record()
            .filter(|record| record.scope == *scope && record.identity.session_id == session)
            .map(|record| record.identity.clone())
            .unwrap_or_else(|| Identity {
                session_id: session.into(),
                work_id: continuation.delivery_key,
                generation: continuation.generation,
            });
        self.acp.bind_permission_context(
            identity,
            scope.clone(),
            batch.reply_tags(),
            store,
            *lease,
        );
    }
    pub(super) async fn configure_provider_background(
        &mut self,
        scope: &SessionScope,
        batch: &FlushBatch,
        session: &str,
        owner: Option<&nostr::PublicKey>,
    ) -> Result<(), AcpError> {
        if let Some(record) = batch
            .continuation
            .as_ref()
            .and_then(|continuation| continuation.provider_recovery.as_ref())
        {
            if !self.acp.provider_background_supported() || record.identity.session_id != session {
                return Err(AcpError::Protocol(
                    "background_original_session_unavailable".into(),
                ));
            }
        }
        if !self.acp.provider_background_supported() {
            self.configure_permission_context(scope, batch, session);
            return Ok(());
        }
        let store =
            self.state.store.clone().ok_or_else(|| {
                AcpError::Protocol("background_tasks_require_session_store".into())
            })?;
        let lease =
            *self.state.leases.get(scope).ok_or_else(|| {
                AcpError::Protocol("background_tasks_require_session_lease".into())
            })?;
        let requested = batch
            .continuation
            .as_ref()
            .and_then(|continuation| continuation.provider_recovery.clone());
        // The leased registry may contain receipts and inputs newer than a queued retry.
        let durable = match store.load(scope) {
            LoadOutcome::Found(record) => record
                .live
                .and_then(|live| live.provider_background)
                .map(|record| *record),
            _ => None,
        };
        if let (Some(requested), Some(durable)) = (&requested, &durable) {
            if requested.identity != durable.identity {
                return Err(AcpError::Protocol(
                    "background_recovery_identity_mismatch".into(),
                ));
            }
        }
        let recovery_requested = requested.is_some();
        let saved = durable.or(requested);
        if recovery_requested && saved.as_ref().is_some_and(|record| !record.recoverable()) {
            return Err(AcpError::Protocol("background_work_terminal".into()));
        }
        let restoring = saved.as_ref().is_some_and(Record::recoverable);
        let record = if let Some(saved) = saved.filter(Record::recoverable) {
            if saved.identity.session_id != session {
                return Err(AcpError::Protocol(
                    "background_original_session_unavailable".into(),
                ));
            }
            saved
        } else {
            let continuation = self
                .background_context
                .as_ref()
                .and_then(|hold| hold.continuation.as_ref())
                .ok_or_else(|| AcpError::Protocol("background_origin_unavailable".into()))?;
            Record {
                authorization: None,
                needs_user: None,
                terminal_reason: None,
                parent_pending: false,
                cancellation: None,
                identity: Identity {
                    session_id: session.into(),
                    work_id: continuation.delivery_key,
                    generation: continuation.generation,
                },
                scope: scope.clone(),
                thread_tags: batch.reply_tags(),
                events: batch
                    .work
                    .as_ref()
                    .map(|work| work.original_events.iter().map(SavedEvent::from).collect())
                    .unwrap_or_else(|| batch.events.iter().map(SavedEvent::from).collect()),
                runtime_final: batch.work.as_ref().is_some_and(|work| work.runtime_final),
                deadline_unix: now_unix().saturating_add(
                    batch
                        .work
                        .as_ref()
                        .map(|work| work.remaining().as_secs())
                        .unwrap_or(1800),
                ),
                tasks: Default::default(),
                started: Default::default(),
            }
        };
        self.acp.configure_authorization_policy(
            owner.map(nostr::PublicKey::to_hex),
            !restoring
                && batch.continuation.is_none()
                && batch.cancelled_events.is_empty()
                && batch.work.as_ref().is_none_or(|work| work.attempt == 0),
            batch.continuation.is_some(),
        );
        self.acp.bind_provider_background(record, store, lease)?;
        self.configure_permission_context(scope, batch, session);
        // Accepted clarification must survive an early provider RPC failure.
        self.acp.append_provider_inputs(&batch.events)?;
        if restoring {
            self.acp.reconcile_provider_background().await?;
        }
        if let Some(continuation) = self
            .background_context
            .as_mut()
            .and_then(|hold| hold.continuation.as_mut())
        {
            continuation.provider_recovery = self.acp.provider_background_record().cloned();
        }
        if self
            .acp
            .provider_background_record()
            .is_some_and(Record::expired)
        {
            self.acp
                .suspend_provider_background("background_deadline")?;
            return Err(AcpError::WorkDeadline);
        }
        if batch.continuation.is_some() {
            self.acp.start_provider_collection()?;
        }
        Ok(())
    }
}

impl AgentPool {
    pub(crate) fn revoke_authorization_for_shutdown(&self) {
        let Some(store) = &self.store else {
            return;
        };
        let scopes: std::collections::HashSet<_> = self
            .agents
            .iter()
            .flatten()
            .filter_map(|agent| {
                agent
                    .acp
                    .provider_background_record()
                    .map(|record| record.scope.clone())
            })
            .chain(self.task_map.values().filter_map(|task| task.scope.clone()))
            .chain(self.runtime_holds.values().map(|hold| hold.scope.clone()))
            .collect();
        for scope in scopes {
            if let Err(error) = store.revoke_authorization_for_shutdown(&scope) {
                tracing::error!(%error,"shutdown authorization fence could not be persisted");
            }
        }
    }
    pub(crate) fn authorization_supported(&self) -> bool {
        self.agents
            .iter()
            .flatten()
            .any(|agent| agent.acp.authorization_supported())
    }
    pub(crate) fn accept_authorization_input(
        &self,
        scope: &SessionScope,
        source: SavedEvent,
    ) -> Result<bool, crate::session_store::StoreError> {
        self.store
            .as_ref()
            .ok_or(crate::session_store::StoreError::Unreadable)?
            .accept_authorization_input(scope, source, None)
    }
    pub(crate) fn revoke_authorization_input(
        &self,
        scope: &SessionScope,
        source: SavedEvent,
        reason: &str,
    ) -> Result<bool, crate::session_store::StoreError> {
        self.store
            .as_ref()
            .ok_or(crate::session_store::StoreError::Unreadable)?
            .accept_authorization_input(scope, source, Some(reason))
    }
    pub(crate) fn provider_background_status(
        &self,
        scope: &SessionScope,
        route: &crate::queue::ReplyRoute,
        dm: bool,
    ) -> Option<String> {
        self.agents.iter().flatten().find_map(|agent| {
            let record = agent.acp.provider_background_record()
                .filter(|record| record.scope == *scope && record.recoverable())?;
            let original = record.thread_tags.root_event_id.as_deref()
                .or(record.thread_tags.parent_event_id.as_deref());
            let same = if dm && original.is_none() {
                route.root_event_id.is_none()
            } else {
                original == Some(route.thread.as_str())
            };
            if !same {
                return None;
            }
            let running = record.tasks.values()
                .filter(|task| task.state == crate::provider_background::State::Running).count();
            Some(format!("Waiting for background work: {running} running, {} results awaiting collection. The original task and reply route are retained.",record.pending().len()))
        })
    }
    pub(crate) fn provider_background_blocks_input(&self, scope: &SessionScope) -> bool {
        self.agents.iter().flatten().any(|agent| {
            agent
                .acp
                .provider_background_record()
                .is_some_and(|record| record.scope == *scope && record.recoverable())
        })
    }
    pub(crate) fn expire_provider_background(&mut self, record: &Record) -> Result<(), AcpError> {
        // Provider identity survives restart; local ownership is assigned anew.
        // Validate both independently before changing an idle worker's hold.
        let releasable: Vec<_> = self
            .runtime_holds
            .iter()
            .filter_map(|(index, hold)| {
                let continuation = hold.continuation.as_ref()?;
                let provider = continuation.provider_recovery.as_ref()?;
                let agent = self.agents.get(*index)?.as_ref()?;
                (hold.scope == record.scope
                    && provider.scope == record.scope
                    && provider.identity == record.identity
                    && continuation.delivery_key == record.identity.work_id
                    && continuation.session_id == record.identity.session_id
                    && self.continuation_matches(agent, &hold.scope, continuation))
                .then_some(*index)
            })
            .collect();
        let result = if let Some(agent) = self.agents.iter_mut().flatten().find(|agent| {
            agent.acp.provider_background_record().is_some_and(|bound| {
                bound.identity == record.identity && bound.scope == record.scope
            })
        }) {
            agent.acp.finish_provider_parent("background_deadline")
        } else {
            self.store
                .as_ref()
                .ok_or_else(|| AcpError::Protocol("background_tasks_require_session_store".into()))
                .and_then(|store| {
                    store
                        .finish_dead_provider_background(record, "background_deadline")
                        .map_err(|_| {
                            AcpError::Protocol("background_terminal_state_not_persisted".into())
                        })
                })
        };
        // Persistence failure needs human attention, not another automatic wake.
        // A stale expiry must leave any replacement collection and OS work alone.
        for index in releasable {
            let Some(hold) = self.runtime_holds.get_mut(&index) else {
                continue;
            };
            hold.continuation = None;
            hold.collection_pending = false;
            if hold.blocking_pids.is_empty() {
                self.runtime_holds.remove(&index);
            }
        }
        result
    }
    pub(crate) fn poll_provider_background(&mut self, queue: &mut EventQueue) {
        let indexes: Vec<_> = self.runtime_holds.keys().copied().collect();
        for index in indexes {
            if !self.agents[index].as_ref().is_some_and(|agent| {
                agent.acp.provider_background_supported()
                    && agent.acp.provider_background_record().is_some()
            }) {
                continue;
            }
            let valid = self.runtime_holds.get(&index).is_some_and(|hold| {
                hold.continuation.as_ref().is_some_and(|continuation| {
                    self.agents[index].as_ref().is_some_and(|agent| {
                        self.continuation_matches(agent, &hold.scope, continuation)
                    })
                })
            });
            if !valid {
                if let Some(agent) = self.agents[index].as_mut() {
                    agent.acp.abandon_provider_background();
                }
                if self
                    .runtime_holds
                    .get(&index)
                    .is_some_and(|hold| hold.blocking_pids.is_empty())
                {
                    self.runtime_holds.remove(&index);
                }
                continue;
            }
            let Some(agent) = self.agents[index].as_mut() else {
                continue;
            };
            agent.acp.poll_provider_background();
            let Some(record) = agent.acp.provider_background_record() else {
                continue;
            };
            if record.recoverable() || agent.acp.provider_background_pending() {
                let ready = agent.acp.provider_background_pending()
                    || record.expired()
                    || (record.parent_pending && !record.outstanding());
                if let Some(hold) = self.runtime_holds.get_mut(&index) {
                    hold.collection_pending = ready;
                    if let Some(continuation) = &mut hold.continuation {
                        continuation.provider_recovery = Some(record.clone());
                        continuation.background_ready = ready;
                        queue.enqueue_continuation(hold.scope.clone(), continuation.clone());
                    }
                }
            }
        }
    }
}
