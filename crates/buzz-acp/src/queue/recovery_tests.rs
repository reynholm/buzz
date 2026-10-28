use super::*;
use nostr::{EventBuilder, Keys, Kind, Tag};

#[test]
fn kimi_retry_requires_checked_retirement_and_uncertainty_is_terminal_ready() {
    let scope = SessionScope::Conversation {
        channel_id: Uuid::new_v4(),
    };
    let mut queue = EventQueue::new(DedupMode::Queue)
        .with_kimi_recovery(RecoveryPolicy::default(), "/tmp/work");
    queue.push(event(&scope, &"a".repeat(64), "original"));
    let mut batch = queue.flush_next().unwrap();
    let work = batch.work.clone().unwrap();
    work.require_retirement();
    batch.work.as_mut().unwrap().next_attempt = Some(Instant::now());
    queue.recovery_batches.insert(scope.clone(), batch);
    queue.mark_complete(&scope);
    assert!(queue.flush_next().is_none());
    assert!(
        queue.next_retry_deadline().is_none(),
        "stopping must not spin on backoff timer"
    );
    work.retired(false);
    assert!(queue.next_work_terminal_deadline().unwrap() <= Instant::now());
    assert!(queue
        .flush_next()
        .unwrap()
        .work
        .unwrap()
        .retirement_unverified());
}

#[test]
fn kimi_effect_uncertainty_is_shared_and_not_cleared_by_another_tool() {
    let scope = SessionScope::Conversation {
        channel_id: Uuid::new_v4(),
    };
    let mut queue = EventQueue::new(DedupMode::Queue)
        .with_kimi_recovery(RecoveryPolicy::default(), "/tmp/work");
    queue.push(event(&scope, &"a".repeat(64), "original"));
    let work = queue.flush_next().unwrap().work.unwrap();
    let clone = work.clone();
    work.effect_started("one");
    clone.effect_settled("two");
    assert!(work.uncertain_effects());
    clone.effect_settled("one");
    assert!(!work.uncertain_effects());
    assert!(
        clone.observed_effects(),
        "new session cannot assume previous actions never ran"
    );
}

fn event(scope: &SessionScope, root: &str, text: &str) -> QueuedEvent {
    QueuedEvent {
        channel_id: scope.channel_id(),
        scope: scope.clone(),
        event: EventBuilder::new(Kind::Custom(9), text)
            .tag(Tag::parse(["e", root, "", "reply"]).unwrap())
            .sign_with_keys(&Keys::generate())
            .unwrap(),
        received_at: Instant::now(),
        prompt_tag: "mention".into(),
        edit: None,
    }
}

#[test]
fn kimi_retry_keeps_the_complete_original_work_apart_from_new_thread_traffic() {
    let scope = SessionScope::Conversation {
        channel_id: Uuid::new_v4(),
    };
    let mut queue = EventQueue::new(DedupMode::Queue)
        .with_kimi_recovery(RecoveryPolicy::default(), "/tmp/work");
    queue.push(event(&scope, &"a".repeat(64), "original"));
    let first = queue.flush_next().unwrap();
    let key = first.work.as_ref().unwrap().delivery_key;
    assert!(queue.requeue(first).is_none());
    queue.mark_complete(&scope);
    queue.push(event(&scope, &"b".repeat(64), "new thread"));
    assert!(
        queue.flush_next().is_none(),
        "fresh traffic must not absorb a backing-off work"
    );
    let pending = queue.recovery_batches.get_mut(&scope).unwrap();
    pending.work.as_mut().unwrap().next_attempt = Some(Instant::now());
    let retry = queue.flush_next().unwrap();
    assert_eq!(retry.events.len(), 1);
    assert_eq!(retry.events[0].event.content, "original");
    assert_eq!(retry.work.as_ref().unwrap().delivery_key, key);
    assert_eq!(retry.reply_tags().root_event_id, Some("a".repeat(64)));
    assert_eq!(retry.work.as_ref().unwrap().attempt, 1);
    assert!(
        queue.flush_next().is_none(),
        "one scope cannot execute two attempts"
    );
    queue.mark_complete(&scope);
    let next = queue.flush_next().unwrap();
    assert_eq!(next.events[0].event.content, "new thread");
    assert_ne!(next.work.as_ref().unwrap().delivery_key, key);
}

#[test]
fn kimi_backoff_cancel_is_effective_and_does_not_cancel_future_messages() {
    let scope = SessionScope::Conversation {
        channel_id: Uuid::new_v4(),
    };
    let mut queue = EventQueue::new(DedupMode::Queue)
        .with_kimi_recovery(RecoveryPolicy::default(), "/tmp/work");
    queue.push(event(&scope, &"a".repeat(64), "original"));
    let first = queue.flush_next().unwrap();
    let original = first.work.clone().unwrap();
    queue.requeue(first);
    queue.mark_complete(&scope);
    assert!(queue.cancel_logical_work(&scope));
    assert!(original.cancelled());
    assert!(!queue.has_undispatched_work());
    assert!(queue.next_retry_deadline().is_none());
    queue.push(event(&scope, &"b".repeat(64), "future"));
    assert!(!queue.flush_next().unwrap().work.unwrap().cancelled());
}

#[test]
fn kimi_retry_budget_and_jitter_are_bounded_without_wall_clock_waiting() {
    let policy = RecoveryPolicy::default();
    for (attempt, low, high) in [(1, 4., 6.), (2, 12., 18.), (3, 36., 54.), (4, 96., 144.)] {
        assert_eq!(policy.delay(attempt, 0.).unwrap().as_secs_f64(), low);
        assert_eq!(policy.delay(attempt, 1.).unwrap().as_secs_f64(), high);
    }
    assert!(policy.delay(5, 0.5).is_none());
    let scope = SessionScope::Conversation {
        channel_id: Uuid::new_v4(),
    };
    let mut queue = EventQueue::new(DedupMode::Queue).with_kimi_recovery(policy, "/tmp/work");
    queue.push(event(&scope, &"a".repeat(64), "original"));
    let mut batch = queue.flush_next().unwrap();
    batch.work.as_mut().unwrap().deadline = Instant::now();
    assert!(
        queue.requeue(batch).is_some(),
        "elapsed attempt time consumes the total budget"
    );
    queue.mark_complete(&scope);
    assert!(!queue.has_undispatched_work());
}

#[test]
fn kimi_steer_preserves_work_identity_and_delivers_the_new_message() {
    let scope = SessionScope::Conversation {
        channel_id: Uuid::new_v4(),
    };
    let mut queue = EventQueue::new(DedupMode::Queue)
        .with_kimi_recovery(RecoveryPolicy::default(), "/tmp/work");
    queue.push(event(&scope, &"a".repeat(64), "original"));
    let first = queue.flush_next().unwrap();
    let key = first.work.as_ref().unwrap().delivery_key;
    queue.push(event(
        &scope,
        &"a".repeat(64),
        "continue with this correction",
    ));
    queue.requeue_as_cancelled(first, CancelReason::Steer);
    queue.mark_complete(&scope);
    let merged = queue.flush_next().unwrap();
    assert_eq!(
        merged.events[0].event.content,
        "continue with this correction"
    );
    assert_eq!(merged.cancelled_events[0].event.content, "original");
    assert_eq!(merged.cancel_reason, Some(CancelReason::Steer));
    assert_eq!(merged.work.unwrap().delivery_key, key);
}

#[test]
fn kimi_explicit_controls_revoke_publication_after_prompt_completion() {
    for channel_removal in [false, true] {
        let scope = SessionScope::Conversation {
            channel_id: Uuid::new_v4(),
        };
        let mut queue = EventQueue::new(DedupMode::Queue)
            .with_kimi_recovery(RecoveryPolicy::default(), "/tmp/work");
        queue.push(event(&scope, &"a".repeat(64), "original"));
        let work = queue.flush_next().unwrap().work.unwrap();
        // A finalizer still owns this work after the provider slot is released.
        work.finish();
        queue.mark_complete(&scope);
        if channel_removal {
            queue.drain_channel(scope.channel_id());
        } else {
            queue.cancel_logical_work(&scope);
        }
        assert!(
            work.publication_revoked(),
            "terminal publication must remain revocable"
        );
        queue.push(event(&scope, &"a".repeat(64), "new authorized work"));
        let fresh = queue.flush_next().unwrap().work.unwrap();
        assert!(!fresh.publication_revoked());
    }
}
