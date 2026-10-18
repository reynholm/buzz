//! Bounded manual control jobs; provider execution belongs to the exact owner.
use super::*;
use crate::session_store::{ControlAttempt, ControlReceiptState};
use buzz_core::session_control::{ControlOutcome, ControlResult};
use std::time::Duration;

pub(crate) const COMPACT_WAIT: Duration = Duration::from_secs(120);

#[derive(Clone)]
pub(crate) enum CompactionTarget {
    Memory {
        worker: usize,
        generation: u64,
        session_id: String,
    },
    Disk {
        lease: Uuid,
        session_id: String,
    },
}

#[derive(Clone)]
pub(crate) struct AdmittedSessionControl {
    pub(crate) job: ControlJob,
    pub(crate) dispatch: ControlDispatch,
    pub(crate) target: CompactionTarget,
    pub(crate) deadline: tokio::time::Instant,
}

pub(super) struct PendingCompaction {
    pub(super) admitted: AdmittedSessionControl,
    pub(super) worker: Option<usize>,
}

impl SessionControlRuntime {
    pub(crate) fn control_store(&self) -> Option<Arc<SessionStore>> {
        self.store.clone()
    }

    pub(crate) fn queue_compaction(
        &self,
        job: ControlJob,
        target: Result<CompactionTarget, ControlFailure>,
    ) -> Result<(), ControlFailure> {
        let cached = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.prune_outcomes();
            state.outcomes.get(&job.origin.command_event_id).copied()
        };
        if let Some((_, outcome)) = cached {
            self.queue_result(
                job.origin.command_event_id,
                ControlResult::Terminal(outcome),
            );
            return Ok(());
        }
        // Durable redelivery precedes in-memory dedup and cannot rerun an effect.
        if let Some(store) = &self.store {
            match store.load(&job.scope) {
                LoadOutcome::Found(record) => {
                    if let Some(receipt) = record
                        .control_receipts
                        .iter()
                        .find(|receipt| receipt.command_id == job.origin.command_event_id)
                    {
                        if let ControlReceiptState::Finished(result) = receipt.state {
                            self.queue_result(
                                job.origin.command_event_id,
                                ControlResult::Terminal(result),
                            );
                        }
                        return Ok(());
                    }
                }
                LoadOutcome::Unreadable => return Err(ControlFailure::StoreUnavailable),
                LoadOutcome::Missing => {}
            }
        }
        let target = match target {
            Ok(target) => target,
            Err(failure) => return self.refuse_compaction(&job, failure),
        };
        let dispatch = self.capture(&job.scope);
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.prune_outcomes();
        if let Some(pending) = state.compactions.get(&job.scope) {
            if pending.admitted.job.origin.command_event_id == job.origin.command_event_id {
                return Ok(());
            }
            drop(state);
            return self.refuse_compaction(&job, ControlFailure::Busy);
        }
        let scopes: std::collections::HashSet<_> = state
            .scopes
            .iter()
            .filter(|(_, entry)| entry.retiring.is_some())
            .map(|(scope, _)| scope)
            .chain(state.compactions.keys())
            .collect();
        if scopes.len() + state.receipt_writes.len() >= 128
            || state
                .scopes
                .get(&job.scope)
                .is_some_and(|entry| entry.retiring.is_some())
        {
            drop(state);
            return self.refuse_compaction(&job, ControlFailure::Busy);
        }
        let scope = job.scope.clone();
        state.compactions.insert(
            scope,
            PendingCompaction {
                admitted: AdmittedSessionControl {
                    job,
                    dispatch,
                    target,
                    deadline: tokio::time::Instant::now() + COMPACT_WAIT,
                },
                worker: None,
            },
        );
        Ok(())
    }

    fn refuse_compaction(
        &self,
        job: &ControlJob,
        mut failure: ControlFailure,
    ) -> Result<(), ControlFailure> {
        let operation_outcome = Err(failure);
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let pending_scopes = state
            .scopes
            .iter()
            .filter(|(_, entry)| entry.retiring.is_some())
            .map(|(scope, _)| scope)
            .chain(state.compactions.keys())
            .collect::<std::collections::HashSet<_>>()
            .len();
        if pending_scopes + state.receipt_writes.len() >= 128 {
            state.remember(
                job.origin.command_event_id.clone(),
                Err(ControlFailure::Busy),
            );
            return Err(ControlFailure::Busy);
        }
        if let Some(store) = &self.store {
            match store.begin_admitted_control_attempt(
                &job.scope,
                &job.origin.command_event_id,
                job.event.created_at.as_secs(),
                job.admitted_at,
            ) {
                Ok(ControlAttempt::New) => {
                    if let Err(error) = store.finish_control_attempt(
                        &job.scope,
                        &job.origin.command_event_id,
                        &Err(failure),
                    ) {
                        failure = store_failure(error);
                        state.retain_receipt_write(
                            store,
                            &job.scope,
                            &job.origin.command_event_id,
                            Err(failure),
                            operation_outcome,
                        );
                    }
                }
                Ok(ControlAttempt::PriorFinished(outcome)) => {
                    state.publish(Publication {
                        command_id: job.origin.command_event_id.clone(),
                        result: ControlResult::Terminal(outcome),
                    });
                    return Ok(());
                }
                Ok(ControlAttempt::PriorStarted) => return Ok(()),
                Err(error) => failure = store_failure(error),
            }
        }
        state.remember(job.origin.command_event_id.clone(), Err(failure));
        Err(failure)
    }

    pub(crate) fn compact_pending(&self, scope: &SessionScope) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .compactions
            .contains_key(scope)
    }

    pub(crate) fn pending_compactions(&self) -> Vec<AdmittedSessionControl> {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .compactions
            .values()
            .filter(|pending| pending.worker.is_none())
            .map(|pending| pending.admitted.clone())
            .collect()
    }

    pub(crate) fn start_compaction(&self, job: &AdmittedSessionControl, worker: usize) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let Some(pending) = state.compactions.get_mut(&job.job.scope) else {
            return false;
        };
        if pending.worker.is_some() || pending.admitted.job.origin != job.job.origin {
            return false;
        }
        pending.worker = Some(worker);
        true
    }

    pub(crate) fn begin_compaction_effect(
        &self,
        job: &AdmittedSessionControl,
    ) -> Result<ControlAttempt, ControlFailure> {
        if !job.dispatch.current() {
            return Err(ControlFailure::Superseded);
        }
        self.store
            .as_ref()
            .ok_or(ControlFailure::StoreUnavailable)?
            .begin_admitted_control_attempt(
                &job.job.scope,
                &job.job.origin.command_event_id,
                job.job.event.created_at.as_secs(),
                job.job.admitted_at,
            )
            .map_err(store_failure)
    }

    pub(crate) fn finish_compaction_job(
        &self,
        job: &AdmittedSessionControl,
        mut outcome: ControlOutcome,
    ) {
        if !job.dispatch.current() {
            outcome = Err(ControlFailure::Superseded);
        }
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        // A restart/removal already terminalized this job; its first result wins.
        if !state
            .compactions
            .get(&job.job.scope)
            .is_some_and(|pending| pending.admitted.job.origin == job.job.origin)
        {
            return;
        }
        let operation_outcome = outcome;
        if let Some(store) = &self.store {
            if let Err(error) = store.finish_control_attempt(
                &job.job.scope,
                &job.job.origin.command_event_id,
                &outcome,
            ) {
                // Pre-execution refusal has no Started marker. Persist a receipt
                // when possible so a replay returns that same refusal.
                match store.begin_admitted_control_attempt(
                    &job.job.scope,
                    &job.job.origin.command_event_id,
                    job.job.event.created_at.as_secs(),
                    job.job.admitted_at,
                ) {
                    Ok(ControlAttempt::New | ControlAttempt::PriorStarted) => {
                        if let Err(error) = store.finish_control_attempt(
                            &job.job.scope,
                            &job.job.origin.command_event_id,
                            &outcome,
                        ) {
                            tracing::warn!(%error, "compaction terminal receipt could not be persisted");
                            outcome = Err(ControlFailure::StoreUnavailable);
                            state.retain_receipt_write(
                                store,
                                &job.job.scope,
                                &job.job.origin.command_event_id,
                                outcome,
                                operation_outcome,
                            );
                        }
                    }
                    Ok(ControlAttempt::PriorFinished(prior)) => outcome = prior,
                    Err(_) => {
                        tracing::warn!(%error, "compaction refusal receipt unavailable");
                    }
                }
            }
        }
        state.compactions.remove(&job.job.scope);
        state.remember(job.job.origin.command_event_id.clone(), outcome);
        state.publish(Publication {
            command_id: job.job.origin.command_event_id.clone(),
            result: ControlResult::Terminal(outcome),
        });
    }

    pub(crate) fn shutdown_compactions(&self) -> std::collections::HashSet<usize> {
        let jobs: Vec<_> = self
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .compactions
            .values()
            .map(|pending| (pending.admitted.clone(), pending.worker))
            .collect();
        let workers = jobs.iter().filter_map(|(_, worker)| *worker).collect();
        for (job, worker) in jobs {
            self.finish_compaction_job(
                &job,
                Err(if worker.is_some() {
                    ControlFailure::Unconfirmed
                } else {
                    ControlFailure::Cancelled
                }),
            );
        }
        workers
    }

    pub(crate) fn compaction_panicked(&self, worker: usize) {
        let jobs: Vec<_> = self
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .compactions
            .values()
            .filter(|pending| pending.worker == Some(worker))
            .map(|pending| pending.admitted.clone())
            .collect();
        for job in jobs {
            self.finish_compaction_job(&job, Err(ControlFailure::Unconfirmed));
        }
    }

    pub(crate) fn compaction_deadline(&self) -> Option<tokio::time::Instant> {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .compactions
            .values()
            .filter(|pending| pending.worker.is_none())
            .map(|pending| pending.admitted.deadline)
            .min()
    }
}

impl State {
    fn prune_outcomes(&mut self) {
        self.outcomes
            .retain(|_, (at, _)| at.elapsed() < Duration::from_secs(86400));
    }
    pub(super) fn remember(&mut self, id: String, result: ControlOutcome) {
        self.prune_outcomes();
        if self.outcomes.len() >= 128 {
            if let Some(oldest) = self
                .outcomes
                .iter()
                .min_by_key(|(_, (at, _))| *at)
                .map(|(id, _)| id.clone())
            {
                self.outcomes.remove(&oldest);
            }
        }
        self.outcomes
            .insert(id, (tokio::time::Instant::now(), result));
    }
}

pub(crate) fn store_failure(error: StoreError) -> ControlFailure {
    match error {
        StoreError::Busy => ControlFailure::Busy,
        StoreError::Conflict => ControlFailure::StaleTarget,
        StoreError::StaleRequest => ControlFailure::StaleRequest,
        _ => ControlFailure::StoreUnavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(start_paused = true)]
    async fn control_outcome_retention_is_bounded() {
        let mut state = State::default();
        for n in 0..1024 {
            state.remember(format!("{n:064x}"), Ok(()));
        }
        assert_eq!(state.outcomes.len(), 128);
        tokio::time::advance(Duration::from_secs(86400)).await;
        state.prune_outcomes();
        assert!(state.outcomes.is_empty());
    }
}
