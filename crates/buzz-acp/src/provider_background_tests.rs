use super::*;
use serde_json::json;

fn record() -> Record {
    Record {
        terminal_reason: None,
        parent_pending: false,
        cancellation: None,
        identity: Identity {
            session_id: "original".into(),
            work_id: uuid::Uuid::new_v4(),
            generation: 7,
        },
        scope: crate::scope::SessionScope::Conversation {
            channel_id: uuid::Uuid::new_v4(),
        },
        thread_tags: Default::default(),
        events: vec![],
        runtime_final: false,
        deadline_unix: u64::MAX,
        tasks: Default::default(),
        started: Default::default(),
    }
}

fn update(r: &Record, state: &str, sequence: u64) -> serde_json::Value {
    json!({"session_id":r.identity.session_id,"work_id":r.identity.work_id,"generation":7,
        "task_id":"child-1","state":state,"sequence":sequence})
}

#[test]
fn provider_background_registry_fences_identity_replays_and_acknowledgement() {
    let mut r = record();
    assert!(r.apply(&update(&r, "running", 1)).unwrap());
    let mut stale = update(&r, "collected", 2);
    stale["generation"] = json!(6);
    assert!(!r.apply(&stale).unwrap());
    assert!(r.outstanding());
    let complete = update(&r, "completed_pending_collection", 2);
    assert!(r.apply(&complete).unwrap());
    assert!(!r.apply(&complete).unwrap());
    assert_eq!(r.pending().len(), 1);
    r.start_collection();
    assert!(r.unacknowledged());
    assert!(
        r.apply(&update(&r, "collected", 3)).is_err(),
        "end_turn alone cannot acknowledge a sequence"
    );
    let mut ack = update(&r, "collected", 3);
    ack["completion_sequence"] = json!(2);
    assert!(r.apply(&ack).unwrap());
    assert!(!r.outstanding());
    assert!(!r.unacknowledged());
    assert!(!r.apply(&complete).unwrap());
    assert!(
        r.apply(&update(&r, "running", 4)).is_err(),
        "task ids cannot be recycled"
    );
}

#[test]
fn provider_background_snapshot_is_transactional_and_never_drops_pending_results() {
    let mut r = record();
    r.apply(&update(&r, "failed_pending_collection", 2))
        .unwrap();
    let before = r.clone();
    let mut bad = update(&r, "collected", 3);
    bad["completion_sequence"] = json!(1);
    let snapshot = json!({"session_id":r.identity.session_id,"work_id":r.identity.work_id,"generation":7,"tasks":[bad]});
    assert!(r.reconcile(&snapshot).is_err());
    assert_eq!(r, before);
    let snapshot = json!({"session_id":r.identity.session_id,"work_id":r.identity.work_id,"generation":7,"tasks":[]});
    r.reconcile(&snapshot).unwrap();
    assert!(
        r.outstanding(),
        "an omitted pending result is not an acknowledgement"
    );
    let restored: Record = serde_json::from_slice(&serde_json::to_vec(&r).unwrap()).unwrap();
    assert_eq!(restored.pending(), r.pending());
}

#[test]
fn provider_background_snapshot_recovers_a_durable_ack_missed_while_disconnected() {
    let mut r = record();
    r.apply(&update(&r, "running", 1)).unwrap();
    let snapshot = json!({"session_id":r.identity.session_id,"work_id":r.identity.work_id,"generation":7,
        "tasks":[{"task_id":"child-1","state":"collected","sequence":3,"completion_sequence":2}]});
    r.reconcile(&snapshot).unwrap();
    assert!(
        !r.outstanding(),
        "a durable acknowledgement is authoritative even if its pending notification was missed"
    );
}
