//! Controller-owned retirement jobs survive the disposable worker pool.
use super::*;
use crate::acp::TerminationReport;
use buzz_core::session_control::{ControlOutcome, ControlResult};
use std::collections::{HashSet, VecDeque};

pub(crate) const FOREIGN_BACKGROUND_LIMIT: std::time::Duration =
    std::time::Duration::from_secs(600);

#[derive(Default)]
pub(super) struct Retirement {
    targets: HashSet<usize>,
    registered: bool,
    terminal: bool,
}

impl Retirement {
    pub(super) fn reset(mut self) -> Self {
        self.registered = false;
        self.terminal = false;
        self
    }
}

pub(super) struct Worker {
    since: tokio::time::Instant,
    running: bool,
    failed: bool,
    evidence_lost: bool,
}

#[derive(Clone)]
pub(crate) struct Publication {
    pub command_id: String,
    pub result: ControlResult,
}

impl SessionControlRuntime {
    /// Reserve all copies before invalidation can erase the pool's directory.
    pub(crate) fn plan_retirement(
        &self,
        scope: &SessionScope,
        targets: impl IntoIterator<Item = usize>,
    ) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let targets: HashSet<_> = targets.into_iter().collect();
        let Some(entry) = state.scopes.get_mut(scope) else {
            return;
        };
        entry.retirement.targets.extend(targets);
        entry.retirement.registered = true;
        let targets = entry.retirement.targets.clone();
        for index in targets {
            let worker = state.workers.entry(index).or_insert_with(|| Worker {
                since: tokio::time::Instant::now(),
                running: false,
                failed: false,
                evidence_lost: false,
            });
            worker.failed = worker.evidence_lost;
        }
        if state.scopes.get(scope).is_some_and(|entry| {
            entry.retirement.targets.iter().any(|index| {
                state
                    .workers
                    .get(index)
                    .is_some_and(|worker| worker.evidence_lost)
            })
        }) {
            Self::terminal(
                &mut state,
                self.store.as_deref(),
                scope,
                Err(ControlFailure::Unconfirmed),
                false,
            );
        }
        Self::finish_ready(&mut state, self.store.as_deref());
    }

    pub(crate) fn owns_retirement(&self, index: usize) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .workers
            .contains_key(&index)
    }

    pub(crate) fn retirement_evidence_lost(&self, index: usize) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .workers
            .get(&index)
            .is_some_and(|worker| worker.evidence_lost)
    }

    /// A foreground unwind already dropped the original client without a checked
    /// snapshot. No replacement or later retry can attest those escaped groups.
    pub(crate) fn lose_retirement_evidence(&self, index: usize) {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(worker) = state.workers.get_mut(&index) {
                worker.evidence_lost = true;
            }
        }
        self.retirement_report(index, TerminationReport::Unverified);
    }

    pub(crate) fn retiring_workers(&self) -> Vec<(usize, HashSet<SessionScope>, bool)> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state
            .workers
            .iter()
            .filter(|(_, worker)| !worker.running && !worker.failed && !worker.evidence_lost)
            .map(|(index, worker)| {
                let scopes = state
                    .scopes
                    .iter()
                    .filter(|(_, entry)| {
                        entry.retiring.is_some() && entry.retirement.targets.contains(index)
                    })
                    .map(|(scope, _)| scope.clone())
                    .collect();
                (
                    *index,
                    scopes,
                    worker.since.elapsed() >= FOREIGN_BACKGROUND_LIMIT,
                )
            })
            .collect()
    }

    pub(crate) fn retirement_started(&self, index: usize) {
        if let Some(worker) = self
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .workers
            .get_mut(&index)
        {
            worker.running = true;
        }
    }

    pub(crate) fn retirement_report(&self, index: usize, report: TerminationReport) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let report = if state
            .workers
            .get(&index)
            .is_some_and(|worker| worker.evidence_lost)
        {
            TerminationReport::Unverified
        } else {
            report
        };
        if report == TerminationReport::Clean {
            state.workers.remove(&index);
            for entry in state.scopes.values_mut() {
                entry.retirement.targets.remove(&index);
            }
            Self::finish_ready(&mut state, self.store.as_deref());
        } else {
            if let Some(worker) = state.workers.get_mut(&index) {
                worker.running = false;
                worker.failed = true;
            }
            let scopes: Vec<_> = state
                .scopes
                .iter()
                .filter(|(_, entry)| {
                    entry.retirement.targets.contains(&index) && !entry.retirement.terminal
                })
                .map(|(scope, _)| scope.clone())
                .collect();
            for scope in scopes {
                Self::terminal(
                    &mut state,
                    self.store.as_deref(),
                    &scope,
                    Err(ControlFailure::Unconfirmed),
                    false,
                );
            }
        }
    }

    fn finish_ready(state: &mut State, store: Option<&SessionStore>) {
        let scopes: Vec<_> = state
            .scopes
            .iter()
            .filter(|(_, entry)| {
                entry.retiring.is_some()
                    && entry.retirement.registered
                    && entry.retirement.targets.is_empty()
                    && !entry.retirement.terminal
            })
            .map(|(scope, _)| scope.clone())
            .collect();
        for scope in scopes {
            Self::terminal(state, store, &scope, Ok(()), true);
        }
    }

    fn terminal(
        state: &mut State,
        store: Option<&SessionStore>,
        scope: &SessionScope,
        outcome: ControlOutcome,
        release: bool,
    ) {
        let Some(entry) = state.scopes.get_mut(scope) else {
            return;
        };
        let Some(job) = &entry.retiring else {
            return;
        };
        // A failed terminal persist never silently releases the gate.
        if let Some(store) = store {
            if let Err(error) =
                store.finish_control_attempt(scope, &job.origin.command_event_id, &outcome)
            {
                tracing::warn!(%error, "session control result could not be persisted; restart remains gated");
                let command_id = job.origin.command_event_id.clone();
                entry.retirement.terminal = true;
                state.publish(Publication {
                    command_id,
                    result: ControlResult::Terminal(Err(ControlFailure::Unconfirmed)),
                });
                return;
            }
        }
        let command_id = job.origin.command_event_id.clone();
        entry.retirement.terminal = true;
        if release {
            entry.retiring = None;
        }
        state.publish(Publication {
            command_id,
            result: ControlResult::Terminal(outcome),
        });
    }

    pub(crate) fn publish_pending(&self, scope: &SessionScope) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(command_id) = state
            .scopes
            .get(scope)
            .and_then(|entry| entry.retiring.as_ref())
            .map(|job| job.origin.command_event_id.clone())
        {
            state.publish(Publication {
                command_id,
                result: ControlResult::Pending,
            });
        }
    }

    #[cfg(test)]
    pub(crate) fn take_publications(&self) -> VecDeque<Publication> {
        self.take_publications_up_to(128)
    }

    pub(crate) fn take_publications_up_to(&self, capacity: usize) -> VecDeque<Publication> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let count = state.publications.len().min(capacity);
        state.publications.drain(..count).collect()
    }

    pub(crate) fn recycle_neighbor(&self, scope: &SessionScope) -> Result<(), StoreError> {
        if let Some(store) = &self.store {
            store.record_worker_recycled(scope)?;
        }
        Ok(())
    }

    pub(crate) fn recycled_reason(&self, scope: &SessionScope) -> bool {
        matches!(self.store.as_ref().map(|store| store.load(scope)), Some(LoadOutcome::Found(record)) if record.reason == Some(crate::session_store::SessionReason::WorkerRecycled))
    }
}

impl SessionControlRuntime {
    pub(crate) fn prior_result(
        &self,
        scope: &SessionScope,
        command: &str,
    ) -> Option<ControlResult> {
        match self.store.as_ref()?.load(scope) {
            LoadOutcome::Found(record) => record
                .control_receipts
                .iter()
                .find(|receipt| receipt.command_id == command)
                .map(|receipt| match receipt.state {
                    crate::session_store::ControlReceiptState::Started => ControlResult::Pending,
                    crate::session_store::ControlReceiptState::Finished(outcome) => {
                        ControlResult::Terminal(outcome)
                    }
                }),
            _ => None,
        }
    }

    pub(crate) fn queue_result(&self, command_id: String, result: ControlResult) {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .publish(Publication { command_id, result });
    }
}
