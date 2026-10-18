//! Known-finalized local receipt writes share the existing reconciliation backoff.
use super::*;
use crate::session_store::{ControlReceipt, ControlReceiptState};
use buzz_core::session_control::{ControlOutcome, ControlResult};

#[derive(Clone)]
pub(super) struct ReceiptPersistence {
    pub(super) scope: SessionScope,
    receipt: ControlReceipt,
    outcome: ControlOutcome,
    operation_outcome: ControlOutcome,
}

#[derive(Default)]
pub(super) struct ReceiptWriteProgress {
    pub(super) completed: usize,
    pub(super) failures: usize,
    pub(super) more: bool,
}

impl State {
    pub(super) fn retain_receipt_write(
        &mut self,
        store: &SessionStore,
        scope: &SessionScope,
        id: &str,
        outcome: ControlOutcome,
        operation_outcome: ControlOutcome,
    ) {
        let LoadOutcome::Found(record) = store.load(scope) else {
            return;
        };
        let Some(receipt) = record.control_receipts.into_iter().find(|receipt| {
            receipt.command_id == id
                && receipt.owner_instance == store.instance()
                && receipt.state == ControlReceiptState::Started
        }) else {
            return;
        };
        if self.receipt_writes.len() >= 128 && !self.receipt_writes.contains_key(id) {
            // Admission reserves this bounded capacity before any effect.
            tracing::error!(
                command = id,
                "receipt persistence capacity invariant violated; durable Started retained"
            );
            return;
        }
        self.receipt_writes.insert(
            id.into(),
            ReceiptPersistence {
                scope: scope.clone(),
                receipt,
                outcome,
                operation_outcome,
            },
        );
    }
}

impl SessionControlRuntime {
    pub(super) fn has_receipt_writes(&self) -> bool {
        !self
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .receipt_writes
            .is_empty()
    }

    pub(super) fn retry_receipt_writes(&self, capacity: usize) -> ReceiptWriteProgress {
        let Some(store) = &self.store else {
            return ReceiptWriteProgress::default();
        };
        let pending: Vec<_> = self
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .receipt_writes
            .iter()
            .take(capacity.min(128))
            .map(|(id, entry)| (id.clone(), entry.clone()))
            .collect();
        let mut progress = ReceiptWriteProgress::default();
        for (id, entry) in pending {
            match store.retry_control_outcome(&entry.scope, &entry.receipt, &entry.outcome) {
                Ok(resolved) => {
                    let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
                    if state.receipt_writes.get(&id).is_some_and(|current| {
                        current.receipt.owner_instance == entry.receipt.owner_instance
                            && current.receipt.owner_generation == entry.receipt.owner_generation
                    }) {
                        state.receipt_writes.remove(&id);
                        if let Some(outcome) = resolved {
                            state.remember(id.clone(), outcome);
                            state.publish(Publication {
                                command_id: id,
                                result: ControlResult::Terminal(outcome),
                            });
                        }
                        progress.completed += 1;
                    }
                }
                Err(error) => {
                    progress.failures += 1;
                    tracing::warn!(%error, command = id, operation = ?entry.operation_outcome, "terminal receipt persistence deferred; provider operation will not repeat");
                }
            }
        }
        progress.more = self.has_receipt_writes();
        progress
    }

    #[cfg(all(test, unix))]
    pub(crate) fn receipt_write_count(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .receipt_writes
            .len()
    }
}
