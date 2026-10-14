//! Background completions share the event queue's in-flight and retry lifecycle.

use super::*;

/// Authority and reply destination of a background task's original session.
#[derive(Clone, Debug)]
pub(crate) struct RuntimeContinuation {
    pub worker_index: usize,
    pub session_id: String,
    pub generation: u64,
    pub thread_tags: ThreadTags,
    pub received_at: Instant,
}

impl FlushBatch {
    /// Reply destination, including batches containing only a continuation.
    pub(crate) fn reply_tags(&self) -> ThreadTags {
        self.events
            .last()
            .map(|event| {
                let mut tags = event.routing_thread_tags();
                if tags.root_event_id.is_none() {
                    tags.root_event_id = Some(event.reply_thread());
                    tags.parent_event_id = Some(event.routing_event_id());
                }
                tags
            })
            .or_else(|| self.continuation.as_ref().map(|c| c.thread_tags.clone()))
            .unwrap_or_default()
    }
}

impl EventQueue {
    /// Enqueue once; a real queued message already supersedes the continuation.
    pub(crate) fn enqueue_continuation(
        &mut self,
        scope: SessionScope,
        continuation: RuntimeContinuation,
    ) {
        if !self.has_pending_scope(&scope) {
            self.continuations.entry(scope).or_insert(continuation);
        }
    }

    pub(crate) fn remove_continuation(&mut self, scope: &SessionScope) {
        if self.continuations.remove(scope).is_some() {
            self.retry_after.remove(scope);
            self.retry_counts.remove(scope);
        }
    }

    pub(super) fn flush_continuation(&mut self) -> Option<FlushBatch> {
        let now = Instant::now();
        let scope = self
            .continuations
            .iter()
            .filter(|(scope, _)| {
                !self.in_flight_scopes.contains(*scope)
                    && self
                        .retry_after
                        .get(*scope)
                        .is_none_or(|deadline| *deadline <= now)
            })
            .min_by_key(|(_, continuation)| continuation.received_at)
            .map(|(scope, _)| scope.clone())?;
        let continuation = self.continuations.remove(&scope)?;
        self.in_flight_scopes.insert(scope.clone());
        self.in_flight_deadlines
            .insert(scope.clone(), now + self.in_flight_deadline);
        self.in_flight_batch_sizes.insert(scope.clone(), 0);
        if let Some(root) = &continuation.thread_tags.root_event_id {
            self.in_flight_reply_threads
                .insert(scope.clone(), root.clone());
        }
        Some(FlushBatch {
            channel_id: scope.channel_id(),
            scope,
            events: Vec::new(),
            cancelled_events: Vec::new(),
            cancel_reason: None,
            continuation: Some(continuation),
        })
    }

    pub(super) fn has_ready_continuation(&self) -> bool {
        let now = Instant::now();
        self.continuations.keys().any(|scope| {
            !self.in_flight_scopes.contains(scope)
                && self
                    .retry_after
                    .get(scope)
                    .is_none_or(|deadline| *deadline <= now)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind};

    fn continuation() -> RuntimeContinuation {
        RuntimeContinuation {
            worker_index: 0,
            session_id: "original".into(),
            generation: 1,
            thread_tags: ThreadTags {
                root_event_id: Some("a".repeat(64)),
                ..Default::default()
            },
            received_at: Instant::now(),
        }
    }

    fn event(scope: &SessionScope) -> QueuedEvent {
        QueuedEvent {
            channel_id: scope.channel_id(),
            scope: scope.clone(),
            event: EventBuilder::new(Kind::Custom(9), "new request")
                .sign_with_keys(&Keys::generate())
                .unwrap(),
            prompt_tag: String::new(),
            received_at: Instant::now(),
            edit: None,
        }
    }

    #[test]
    fn continuation_serializes_messages_and_exposes_the_reply_thread() {
        let scope = SessionScope::Conversation {
            channel_id: Uuid::new_v4(),
        };
        let mut queue = EventQueue::new(DedupMode::Queue);
        queue.enqueue_continuation(scope.clone(), continuation());
        let batch = queue.flush_next().unwrap();
        assert!(batch.continuation.is_some());
        assert!(queue.is_scope_in_flight(&scope));
        assert_eq!(
            queue.in_flight_reply_thread(&scope),
            Some("a".repeat(64).as_str())
        );
        assert!(queue.push(event(&scope)));
        assert!(queue.flush_next().is_none());
        queue.mark_complete(&scope);
        assert_eq!(queue.flush_next().unwrap().events.len(), 1);
    }

    #[test]
    fn continuation_failure_keeps_backoff_and_has_a_terminal_retry_limit() {
        let scope = SessionScope::Conversation {
            channel_id: Uuid::new_v4(),
        };
        let mut queue = EventQueue::new(DedupMode::Drop);
        queue.enqueue_continuation(scope.clone(), continuation());
        for attempt in 0..=MAX_RETRIES {
            let batch = queue.flush_next().unwrap();
            let failed = queue.requeue(batch);
            queue.mark_complete(&scope);
            if attempt == MAX_RETRIES {
                assert!(failed.unwrap().continuation.is_some());
                assert!(!queue.has_undispatched_work());
            } else {
                assert!(failed.is_none());
                assert!(queue.has_undispatched_work());
                assert!(!queue.has_flushable_work());
                assert!(queue.next_retry_deadline().is_some());
                queue.retry_after.insert(scope.clone(), Instant::now());
            }
        }
    }

    #[test]
    fn real_message_supersedes_a_retrying_continuation_without_backoff() {
        let scope = SessionScope::Conversation {
            channel_id: Uuid::new_v4(),
        };
        let mut queue = EventQueue::new(DedupMode::Queue);
        queue.enqueue_continuation(scope.clone(), continuation());
        let batch = queue.flush_next().unwrap();
        assert!(queue.requeue(batch).is_none());
        queue.mark_complete(&scope);
        assert!(queue.push(event(&scope)));
        let batch = queue.flush_next().unwrap();
        assert!(batch.continuation.is_none());
        assert_eq!(batch.events.len(), 1);
        queue.mark_complete(&scope);
        assert!(!queue.has_undispatched_work());
    }

    #[test]
    fn channel_removal_drops_pending_continuation() {
        let scope = SessionScope::Conversation {
            channel_id: Uuid::new_v4(),
        };
        let mut queue = EventQueue::new(DedupMode::Queue);
        queue.enqueue_continuation(scope.clone(), continuation());
        queue.drain_channel(scope.channel_id());
        assert!(!queue.has_undispatched_work());
        assert!(queue.flush_next().is_none());
    }

    #[test]
    fn waiting_for_capacity_does_not_reset_continuation_retry_budget() {
        let scope = SessionScope::Conversation {
            channel_id: Uuid::new_v4(),
        };
        let mut queue = EventQueue::new(DedupMode::Queue);
        queue.enqueue_continuation(scope.clone(), continuation());
        for attempt in 0..=MAX_RETRIES {
            let batch = queue.flush_next().unwrap();
            let failed = queue.requeue(batch);
            queue.mark_complete(&scope);
            if attempt == MAX_RETRIES {
                assert!(failed.is_some());
            } else {
                queue.retry_after.insert(scope.clone(), Instant::now());
                let waiting = queue.flush_next().unwrap();
                queue.requeue_preserve_timestamps(waiting);
                queue.release_unstarted(&scope);
                assert!(
                    queue.next_retry_deadline().is_none(),
                    "expired retry must not spin"
                );
            }
        }
    }
}
