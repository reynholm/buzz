//! Bounded-backoff startup reconciliation, with store I/O outside the relay loop.
use super::*;
use buzz_core::session_control::ControlResult;
use std::time::Duration;
use tokio::time::Instant;

#[derive(Debug, Default)]
pub(crate) struct SweepProgress {
    more: bool,
    failures: usize,
}

impl SessionControlRuntime {
    pub(crate) fn sweep_orphans(&self, capacity: usize) -> Result<SweepProgress, StoreError> {
        let Some(store) = &self.store else {
            return Ok(SweepProgress::default());
        };
        let local = self.retry_receipt_writes(capacity);
        let progress = store.sweep_orphan_controls(capacity.saturating_sub(local.completed))?;
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        for command_id in progress.finished {
            state.publish(Publication {
                command_id,
                result: ControlResult::Terminal(Err(ControlFailure::Unconfirmed)),
            });
        }
        Ok(SweepProgress {
            more: progress.more || local.more,
            failures: progress.failures + local.failures,
        })
    }
}

type SweepResult = Result<Result<SweepProgress, StoreError>, tokio::task::JoinError>;

pub(crate) struct OrphanReconciliation {
    task: Option<tokio::task::JoinHandle<Result<SweepProgress, StoreError>>>,
    completed: Option<SweepResult>,
    next: Option<Instant>,
    backoff: Duration,
    waiting_capacity: bool,
}

impl Default for OrphanReconciliation {
    fn default() -> Self {
        Self {
            task: None,
            completed: None,
            next: Some(Instant::now()),
            backoff: Duration::from_secs(1),
            waiting_capacity: false,
        }
    }
}

impl OrphanReconciliation {
    /// Each sweep publishes its successful prefix before returning retry status.
    pub(crate) fn drive(&mut self, controls: &SessionControlRuntime, capacity: usize) {
        if let Some(result) = self.completed.take() {
            match result {
                Ok(Ok(progress)) if progress.failures == 0 => {
                    self.backoff = Duration::from_secs(1);
                    self.next = progress.more.then(Instant::now);
                }
                failure => {
                    tracing::warn!(
                        ?failure,
                        retry_secs = self.backoff.as_secs(),
                        "orphan reconciliation incomplete; retry scheduled"
                    );
                    self.next = Some(Instant::now() + self.backoff);
                    self.backoff = (self.backoff * 2).min(Duration::from_secs(60));
                }
            }
        }
        if self.next.is_none() && self.task.is_none() && controls.has_receipt_writes() {
            self.next = Some(Instant::now());
        }
        self.waiting_capacity = capacity == 0;
        if self.task.is_none()
            && capacity > 0
            && self.next.is_some_and(|next| next <= Instant::now())
        {
            let controls = controls.clone();
            self.next = None;
            self.task = Some(tokio::task::spawn_blocking(move || {
                controls.sweep_orphans(capacity.min(128))
            }));
        }
    }

    /// Task completion and retry deadlines wake even a completely idle harness.
    pub(crate) async fn wake(&mut self) {
        if let Some(task) = &mut self.task {
            self.completed = Some(task.await);
            self.task = None;
        } else if let Some(next) = self.next.filter(|_| !self.waiting_capacity) {
            tokio::time::sleep_until(next).await;
        } else {
            std::future::pending::<()>().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scope::SessionScope;
    use crate::session_store::{ControlReceiptState, LoadOutcome};
    use futures_util::FutureExt;

    fn dead_started() -> (tempfile::TempDir, Arc<SessionStore>, SessionScope, String) {
        let dir = tempfile::tempdir().unwrap();
        let agent = nostr::Keys::generate().public_key().to_hex();
        let first = SessionStore::open(dir.path().into(), "wss://relay.example", &agent).unwrap();
        let scope = SessionScope::Conversation {
            channel_id: Uuid::new_v4(),
        };
        let command = "a".repeat(64);
        first
            .begin_control_attempt(&scope, &command, nostr::Timestamp::now().as_secs())
            .unwrap();
        drop(first);
        let store =
            Arc::new(SessionStore::open(dir.path().into(), "wss://relay.example", &agent).unwrap());
        (dir, store, scope, command)
    }

    #[tokio::test(start_paused = true)]
    async fn orphan_reconciliation_retries_failed_writes_with_capped_backoff_without_capacity_spin()
    {
        let (_dir, store, scope, command) = dead_started();
        store.fail_writes(true);
        let controls = SessionControlRuntime::new(Some(store.clone()));
        let mut reconciliation = OrphanReconciliation::default();
        reconciliation.drive(&controls, 128);
        for seconds in [1, 2, 4, 8, 16, 32, 60, 60] {
            reconciliation.wake().await;
            reconciliation.drive(&controls, 128);
            assert!(reconciliation.task.is_none());
            assert_eq!(
                reconciliation.next.unwrap() - Instant::now(),
                Duration::from_secs(seconds)
            );
            assert!(controls.take_publications().is_empty());
            assert!(
                matches!(store.load(&scope), LoadOutcome::Found(record) if record.control_receipts[0].state == ControlReceiptState::Started)
            );
            tokio::time::advance(Duration::from_secs(seconds)).await;
            reconciliation.drive(&controls, 0);
            assert!(reconciliation.task.is_none());
            assert!(
                reconciliation.wake().now_or_never().is_none(),
                "a past retry deadline with no capacity must not spin"
            );
            reconciliation.drive(&controls, 128);
            assert!(reconciliation.task.is_some());
        }
        store.fail_writes(false);
        // One failed attempt might already be running; consume it, then repair
        // the scheduled retry using the real store rather than a synthetic result.
        reconciliation.wake().await;
        reconciliation.drive(&controls, 128);
        if let Some(next) = reconciliation.next {
            tokio::time::sleep_until(next).await;
            reconciliation.drive(&controls, 128);
            reconciliation.wake().await;
            reconciliation.drive(&controls, 128);
        }
        assert_eq!(
            controls
                .take_publications()
                .into_iter()
                .map(|p| p.command_id)
                .collect::<Vec<_>>(),
            vec![command]
        );
        assert!(
            matches!(store.load(&scope), LoadOutcome::Found(record) if matches!(record.control_receipts[0].state, ControlReceiptState::Finished(_)))
        );
        assert!(reconciliation.next.is_none());
        assert!(reconciliation.wake().now_or_never().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn orphan_reconciliation_global_read_failure_retries_and_publishes_after_repair() {
        let (_dir, store, scope, command) = dead_started();
        let channel = store.channel_dir(scope.channel_id());
        let namespace = channel.parent().unwrap().to_path_buf();
        let saved = namespace.with_extension("held");
        std::fs::rename(&namespace, &saved).unwrap();
        std::fs::write(&namespace, b"temporary blocker").unwrap();
        let controls = SessionControlRuntime::new(Some(store));
        let mut reconciliation = OrphanReconciliation::default();
        reconciliation.drive(&controls, 128);
        reconciliation.wake().await;
        reconciliation.drive(&controls, 128);
        assert_eq!(
            reconciliation.next.unwrap() - Instant::now(),
            Duration::from_secs(1)
        );
        assert!(controls.take_publications().is_empty());
        std::fs::remove_file(&namespace).unwrap();
        std::fs::rename(saved, namespace).unwrap();
        tokio::time::advance(Duration::from_secs(1)).await;
        reconciliation.drive(&controls, 128);
        reconciliation.wake().await;
        reconciliation.drive(&controls, 128);
        assert_eq!(
            controls
                .take_publications()
                .into_iter()
                .map(|p| p.command_id)
                .collect::<Vec<_>>(),
            vec![command]
        );
        assert!(reconciliation.next.is_none());
    }
}
