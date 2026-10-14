//! Background collections share scope serialization, with independent retries.

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
    /// Keep one collection per scope independently of pending messages.
    pub(crate) fn enqueue_continuation(
        &mut self,
        scope: SessionScope,
        continuation: RuntimeContinuation,
    ) {
        self.continuations.entry(scope).or_insert(continuation);
    }

    pub(crate) fn remove_continuation(&mut self, scope: &SessionScope) {
        self.continuations.remove(scope);
        self.continuation_retry_after.remove(scope);
        self.continuation_retry_counts.remove(scope);
    }

    pub(crate) fn has_pending_continuation(&self, scope: &SessionScope) -> bool {
        self.continuations.contains_key(scope)
    }

    pub(super) fn flush_continuation(&mut self) -> Option<FlushBatch> {
        let now = Instant::now();
        let scope = self
            .continuations
            .iter()
            .filter(|(scope, _)| {
                !self.in_flight_scopes.contains(*scope)
                    && self
                        .continuation_retry_after
                        .get(*scope)
                        .is_none_or(|deadline| *deadline <= now)
            })
            .min_by_key(|(_, continuation)| continuation.received_at)
            .map(|(scope, _)| scope.clone())?;
        let continuation = self.continuations.remove(&scope)?;
        self.in_flight_continuations.insert(scope.clone());
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
                    .continuation_retry_after
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
                queue
                    .continuation_retry_after
                    .insert(scope.clone(), Instant::now());
            }
        }
    }

    #[test]
    fn background_retry_survives_successful_message_without_blocking_it() {
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
        assert!(
            queue.has_undispatched_work(),
            "ordinary success must keep collection"
        );
        assert!(!queue.has_flushable_work(), "collection keeps its backoff");
        assert!(queue.next_retry_deadline().is_some());
    }

    #[test]
    fn ready_background_collection_wins_over_pending_messages() {
        let scope = SessionScope::Conversation {
            channel_id: Uuid::new_v4(),
        };
        let mut queue = EventQueue::new(DedupMode::Queue);
        queue.enqueue_continuation(scope.clone(), continuation());
        queue.push(event(&scope));
        assert!(queue.flush_next().unwrap().continuation.is_some());
        queue.mark_complete(&scope);
        assert_eq!(queue.flush_next().unwrap().events.len(), 1);
    }

    #[test]
    fn background_exit_is_retained_when_message_was_queued_first() {
        let scope = SessionScope::Conversation {
            channel_id: Uuid::new_v4(),
        };
        let mut queue = EventQueue::new(DedupMode::Queue);
        queue.push(event(&scope));
        queue.enqueue_continuation(scope.clone(), continuation());
        assert!(queue.flush_next().unwrap().continuation.is_some());
    }

    #[test]
    fn background_collection_is_preserved_by_cancel_merge() {
        for reason in [CancelReason::Steer, CancelReason::Interrupt] {
            let scope = SessionScope::Conversation {
                channel_id: Uuid::new_v4(),
            };
            let mut queue = EventQueue::new(DedupMode::Queue);
            queue.enqueue_continuation(scope.clone(), continuation());
            let batch = queue.flush_next().unwrap();
            queue.push(event(&scope));
            queue.requeue_as_cancelled(batch, reason);
            queue.mark_complete(&scope);
            assert!(queue.flush_next().unwrap().continuation.is_some());
        }
    }

    #[test]
    fn background_failure_is_retained_when_new_message_waits() {
        let scope = SessionScope::Conversation {
            channel_id: Uuid::new_v4(),
        };
        let mut queue = EventQueue::new(DedupMode::Queue);
        queue.enqueue_continuation(scope.clone(), continuation());
        let batch = queue.flush_next().unwrap();
        queue.push(event(&scope));
        queue.requeue(batch);
        queue.mark_complete(&scope);
        let message = queue.flush_next().unwrap();
        assert!(message.continuation.is_none());
        queue.mark_complete(&scope);
        assert!(queue.has_undispatched_work());
        assert!(queue.next_retry_deadline().is_some());
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
                queue
                    .continuation_retry_after
                    .insert(scope.clone(), Instant::now());
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

    #[cfg(unix)]
    #[tokio::test]
    async fn background_failed_collection_keeps_finite_budget_across_messages_and_posts_notice() {
        use crate::pool::background_work_tests::{
            apply_result, dispatch, finish_child, push_new_message, started_background_turn,
        };
        use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
        let (mut pool, mut queue, scope, dir, ctx) = started_background_turn(0).await;
        finish_child(&mut pool, &mut queue, &dir).await;
        let original = queue.continuations[&scope].thread_tags.clone();
        std::fs::write(dir.path().join("fail-collection"), "fail").unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut rest = ctx.rest_client.clone();
        rest.base_url = format!("http://{}", listener.local_addr().unwrap());
        for attempt in 0..=MAX_RETRIES {
            assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
            let result =
                tokio::time::timeout(Duration::from_secs(10), pool.rx_and_join_set().0.recv())
                    .await
                    .unwrap()
                    .unwrap();
            assert!(matches!(
                result.outcome,
                crate::pool::PromptOutcome::Error(crate::acp::AcpError::AgentError { .. })
            ));
            apply_result(&mut pool, &mut queue, result, Some(&rest), &HashSet::new());
            if attempt == MAX_RETRIES {
                assert!(!queue.has_undispatched_work());
                assert!(
                    !pool.has_runtime_hold(),
                    "terminal collection must release original session"
                );
                assert!(pool.try_claim(None).is_some());
                break;
            }
            let deadline = queue.next_retry_deadline().unwrap();
            assert!(deadline > Instant::now());
            push_new_message(&mut queue, &scope);
            assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
            let message =
                tokio::time::timeout(Duration::from_secs(10), pool.rx_and_join_set().0.recv())
                    .await
                    .unwrap()
                    .unwrap();
            assert!(matches!(message.outcome, crate::pool::PromptOutcome::Ok(_)));
            apply_result(&mut pool, &mut queue, message, None, &HashSet::new());
            assert_eq!(
                queue.next_retry_deadline(),
                Some(deadline),
                "ordinary success cannot clear collection backoff"
            );
            // Advance eligibility at the queue seam; real child/ACP requests
            // and handle_prompt_result still execute on every retry.
            queue
                .continuation_retry_after
                .insert(scope.clone(), Instant::now());
        }
        let notice: nostr::Event = tokio::time::timeout(Duration::from_secs(3), async {
            let (socket, _) = listener.accept().await.unwrap();
            let mut reader = BufReader::new(socket);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            assert_eq!(line, "POST /events HTTP/1.1\r\n");
            let mut content_length = None;
            for _ in 0..64 {
                line.clear();
                assert_ne!(reader.read_line(&mut line).await.unwrap(), 0);
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    content_length = Some(value.trim().parse::<usize>().unwrap());
                }
            }
            let size = content_length.unwrap();
            assert!(size < 65536);
            let mut body = vec![0; size];
            reader.read_exact(&mut body).await.unwrap();
            reader
                .get_mut()
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
                .await
                .unwrap();
            serde_json::from_slice(&body).unwrap()
        })
        .await
        .unwrap();
        notice.verify().unwrap();
        assert!(notice.content.contains("multiple retries"));
        assert_eq!(crate::queue::routing_thread_tags(&notice, None), original);
        assert!(notice
            .tags
            .iter()
            .any(|tag| tag.as_slice() == ["h", &scope.channel_id().to_string()]));
    }
}
