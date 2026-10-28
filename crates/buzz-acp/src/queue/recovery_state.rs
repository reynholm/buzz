//! Shared execution fences survive copied batches and worker replacement.
use super::*;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Default)]
pub(super) struct WorkRuntime {
    pub(super) cancelled: tokio::sync::Notify,
    pub(super) finished: std::sync::atomic::AtomicBool,
    pub(super) human_required: std::sync::Mutex<Option<&'static str>>,
    pub(super) observer: std::sync::Mutex<Option<super::recovery_observer::WorkObserver>>,
    pub(super) last_state: std::sync::Mutex<Option<(&'static str, &'static str, u32)>>,
    // 0: no old execution; 1: awaiting checked retirement; 2: unverified.
    retirement: AtomicU8,
    effects: std::sync::Mutex<Effects>,
}

#[derive(Debug, Default)]
struct Effects {
    pending: HashSet<String>,
    observed: bool,
    overflow: bool,
}

impl LogicalWork {
    pub(crate) fn publication_revoked(&self) -> bool {
        self.revoked.load(Ordering::Acquire)
    }

    pub(crate) fn finish(&self) {
        self.runtime.finished.store(true, Ordering::Release);
        self.runtime.cancelled.notify_waiters();
    }

    pub(crate) async fn cancelled_signal(&self) {
        let notified = self.runtime.cancelled.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if !self.cancelled() {
            notified.await;
        }
    }

    pub(crate) async fn publication_revoked_signal(&self) {
        loop {
            let notified = self.runtime.cancelled.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.publication_revoked() {
                return;
            }
            notified.await;
        }
    }

    pub(crate) fn awaiting_retirement(&self) -> bool {
        self.runtime.retirement.load(Ordering::Acquire) == 1
    }

    pub(crate) fn retirement_unverified(&self) -> bool {
        self.runtime.retirement.load(Ordering::Acquire) == 2
    }

    pub(crate) fn require_retirement(&self) {
        self.runtime.retirement.store(1, Ordering::Release);
    }

    pub(crate) fn retired(&self, clean: bool) {
        self.runtime
            .retirement
            .store(if clean { 0 } else { 2 }, Ordering::Release);
    }

    /// Record before granting permission: a failed pipe write may be partial.
    /// Identifiers only; commands, arguments and output never enter this state.
    pub(crate) fn effect_started(&self, tool: &str) {
        let mut effects = lock_runtime(&self.runtime.effects);
        effects.observed = true;
        if effects.pending.len() >= 256 || tool.len() > 256 {
            effects.overflow = true;
        } else {
            effects.pending.insert(tool.to_owned());
        }
    }

    pub(crate) fn effect_settled(&self, tool: &str) {
        lock_runtime(&self.runtime.effects).pending.remove(tool);
    }

    pub(crate) fn uncertain_effects(&self) -> bool {
        let effects = lock_runtime(&self.runtime.effects);
        effects.overflow || !effects.pending.is_empty()
    }

    pub(crate) fn observed_effects(&self) -> bool {
        lock_runtime(&self.runtime.effects).observed
    }
}
