//! Safe work states continue across prompt completion, backoff and generations.
use super::*;

#[derive(Clone)]
pub(super) struct WorkObserver {
    handle: crate::observer::ObserverHandle,
    index: usize,
    process: Uuid,
}

impl std::fmt::Debug for WorkObserver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WorkObserver")
            .field("process", &self.process)
            .finish()
    }
}

impl LogicalWork {
    pub(crate) fn bind_observer(
        &self,
        handle: Option<crate::observer::ObserverHandle>,
        index: usize,
        process: Uuid,
    ) {
        *lock_runtime(&self.runtime.observer) = handle.map(|handle| WorkObserver {
            handle,
            index,
            process,
        });
    }

    pub(crate) fn human_required(&self, reason: &'static str) {
        *lock_runtime(&self.runtime.human_required) = Some(reason);
    }

    pub(crate) fn needs_human(&self) -> bool {
        lock_runtime(&self.runtime.human_required).is_some()
    }

    pub(crate) fn human_reason(&self) -> Option<&'static str> {
        *lock_runtime(&self.runtime.human_required)
    }

    pub(crate) fn transition(&self, state: &'static str, reason: &'static str) {
        let Some(observer) = lock_runtime(&self.runtime.observer).clone() else {
            return;
        };
        let mut last = lock_runtime(&self.runtime.last_state);
        let value = (state, reason, self.attempt);
        if last.as_ref() == Some(&value) {
            return;
        }
        *last = Some(value);
        drop(last);
        observer.handle.emit("work_state", Some(observer.index),
            &crate::observer::context_for(Some(self.scope.channel_id()), None, None),
            serde_json::json!({
                "state":state, "reason":reason, "workId":self.delivery_key,
                "deliveryKey":self.delivery_key,"attempt":self.attempt,
                "scope":self.scope.telemetry_label(),
                "threadRootEventId":self.reply_tags().root_event_id,
                "processGeneration":observer.process,
                "remainingSecs":self.remaining().as_secs_f64(),
                "nextAttemptInSecs":self.next_attempt.map(|next| next.saturating_duration_since(Instant::now()).as_secs_f64()),
            }));
    }
}
