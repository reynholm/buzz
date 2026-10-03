use super::*;
use crate::app_state::build_app_state;
use nostr::{EventBuilder, Kind, Timestamp};
use std::sync::{Arc, Mutex};
fn state() -> AppState {
    let state = build_app_state();
    *state.keys.lock().unwrap() = nostr::Keys::generate();
    *state.relay_url_override.lock().unwrap() = Some("wss://scope.example/".into());
    state
}
fn event(state: &AppState, second: u64) -> nostr::Event {
    EventBuilder::new(Kind::Custom(30175), "{}")
        .custom_created_at(Timestamp::from(second))
        .sign_with_keys(&state.signing_keys().unwrap())
        .unwrap()
}
#[tokio::test]
async fn failed_backfill_or_apply_never_becomes_ready() {
    let state = state();
    let s = begin_session(&state).unwrap();
    assert_eq!(
        readiness_locked(&state, &capture_scope(&state).unwrap()).unwrap(),
        EvidenceReadiness::Pending
    );
    assert!(finish_session(&state, &s.token).is_err());
    assert!(hydrate_history(
        &state,
        &s.token,
        |_| async { Err("fetch failed".into()) },
        |_| async { Ok(()) }
    )
    .await
    .is_err());
    assert_eq!(
        readiness_locked(&state, &capture_scope(&state).unwrap()).unwrap(),
        EvidenceReadiness::Failed
    );
    assert!(finish_session(&state, &s.token).is_err());
    let s = begin_session(&state).unwrap();
    let e = event(&state, 1);
    assert!(hydrate_history(
        &state,
        &s.token,
        move |_| {
            let e = e.clone();
            async move { Ok(vec![e]) }
        },
        |_| async { Err("apply failed".into()) }
    )
    .await
    .is_err());
    assert_eq!(
        readiness_locked(&state, &capture_scope(&state).unwrap()).unwrap(),
        EvidenceReadiness::Failed
    );
    assert!(finish_session(&state, &s.token).is_err());
    let s = begin_session(&state).unwrap();
    hydrate_history(
        &state,
        &s.token,
        |_| async { Ok(vec![]) },
        |_| async { Ok(()) },
    )
    .await
    .unwrap();
    finish_session(&state, &s.token).unwrap();
    assert_eq!(
        readiness_locked(&state, &capture_scope(&state).unwrap()).unwrap(),
        EvidenceReadiness::Ready
    );
}
#[tokio::test]
async fn stale_sync_session_cannot_ready_new_scope() {
    let state = state();
    let old = begin_session(&state).unwrap();
    let new = begin_session(&state).unwrap();
    assert!(finish_session(&state, &old.token).is_err());
    assert!(invalidate_session(&state, &old.token).is_err());
    hydrate_history(
        &state,
        &new.token,
        |_| async { Ok(vec![]) },
        |_| async { Ok(()) },
    )
    .await
    .unwrap();
    state
        .workspace_apply_generation
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    assert!(finish_session(&state, &new.token).is_err());
    assert_eq!(
        readiness_locked(&state, &capture_scope(&state).unwrap()).unwrap(),
        EvidenceReadiness::Pending
    );
}
#[tokio::test]
async fn exhaustive_history_applies_catalog_last_and_coalesces() {
    let state = state();
    let s = begin_session(&state).unwrap();
    let keys = state.signing_keys().unwrap();
    let make = |kind, second| {
        EventBuilder::new(Kind::Custom(kind), "{}")
            .tags([nostr::Tag::identifier("coordinate")])
            .custom_created_at(Timestamp::from(second))
            .sign_with_keys(&keys)
            .unwrap()
    };
    let old = make(30177, 1);
    let newest = make(30177, 2);
    let catalog = make(30178, 3);
    let persona = make(30175, 1);
    let rows = Arc::new(Mutex::new(vec![]));
    let applied = rows.clone();
    hydrate_history(
        &state,
        &s.token,
        move |_| {
            let events = vec![
                catalog.clone(),
                newest.clone(),
                old.clone(),
                persona.clone(),
            ];
            async move { Ok(events) }
        },
        move |e| {
            applied
                .lock()
                .unwrap()
                .push((e.kind.as_u16(), e.created_at.as_secs()));
            async { Ok(()) }
        },
    )
    .await
    .unwrap();
    assert_eq!(
        *rows.lock().unwrap(),
        vec![(30177, 2), (30175, 1), (30178, 3)]
    );
    assert_eq!(
        readiness_locked(&state, &capture_scope(&state).unwrap()).unwrap(),
        EvidenceReadiness::Pending
    );
    finish_session(&state, &s.token).unwrap();
}

#[tokio::test]
async fn history_returns_all_covered_ids_only_after_successful_apply() {
    let state = state();
    let s = begin_session(&state).unwrap();
    let keys = state.signing_keys().unwrap();
    let head = |second| {
        EventBuilder::new(Kind::Custom(30177), "{}")
            .tags([nostr::Tag::identifier("instance")])
            .custom_created_at(Timestamp::from(second))
            .sign_with_keys(&keys)
            .unwrap()
    };
    let old = head(1);
    let newest = head(2);
    let mut expected = vec![old.id.to_hex(), newest.id.to_hex()];
    expected.sort();
    let result = hydrate_history(
        &state,
        &s.token,
        move |_| {
            let events = vec![newest.clone(), old.clone()];
            async move { Ok(events) }
        },
        |_| async { Ok(()) },
    )
    .await
    .unwrap();
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        serde_json::json!({"coveredEventIds":expected})
    );
}

#[tokio::test]
async fn paging_is_inclusive_exhaustive_and_deduped_before_catalog_apply() {
    let state = state();
    let s = begin_session(&state).unwrap();
    let keys = state.signing_keys().unwrap();
    let make = |kind, second, id: String| {
        EventBuilder::new(Kind::Custom(kind), id)
            .custom_created_at(Timestamp::from(second))
            .sign_with_keys(&keys)
            .unwrap()
    };
    let catalog = make(30178, 1000, "catalog".into());
    let mut first = vec![catalog.clone()];
    for i in 0..499 {
        first.push(make(30175, 999 - i, format!("filler-{i}")));
    }
    let boundary = first.last().unwrap().clone();
    let required = make(30175, 100, "required".into());
    let filters = Arc::new(Mutex::new(vec![]));
    let captured = filters.clone();
    let applied = Arc::new(Mutex::new(vec![]));
    let rows = applied.clone();
    hydrate_history(
        &state,
        &s.token,
        move |filter| {
            captured.lock().unwrap().push(filter.clone());
            let events = if filter.get("until").is_none() {
                first.clone()
            } else {
                vec![boundary.clone(), required.clone()]
            };
            async move { Ok(events) }
        },
        move |e| {
            rows.lock().unwrap().push(e.content.clone());
            async { Ok(()) }
        },
    )
    .await
    .unwrap();
    let filters = filters.lock().unwrap();
    assert_eq!(filters.len(), 2);
    assert_eq!(filters[1]["until"], 501);
    assert_eq!(
        filters[0]["authors"],
        serde_json::json!([keys.public_key().to_hex()])
    );
    assert_eq!(
        filters[0]["kinds"],
        serde_json::json!([30175, 30176, 30177, 30178, 5])
    );
    assert_eq!(filters[0]["limit"], 500);
    let applied = applied.lock().unwrap();
    assert_eq!(applied.len(), 501);
    assert_eq!(applied.last().unwrap(), "catalog");
    assert_eq!(applied.iter().filter(|s| *s == "filler-498").count(), 1);
    assert!(applied.iter().any(|s| s == "required"));
}

#[tokio::test]
async fn dense_or_invalid_history_fails_without_applying_partial_batch() {
    let state = state();
    let s = begin_session(&state).unwrap();
    let dense = vec![event(&state, 5); 500];
    let applied = Arc::new(Mutex::new(0));
    let calls = applied.clone();
    let result = hydrate_history(
        &state,
        &s.token,
        move |_| {
            let page = dense.clone();
            async move { Ok(page) }
        },
        move |_| {
            *calls.lock().unwrap() += 1;
            async { Ok(()) }
        },
    )
    .await;
    assert_eq!(
        result.err().as_deref(),
        Some("device_home_sync_dense_history_boundary")
    );
    assert_eq!(*applied.lock().unwrap(), 0);
    assert!(finish_session(&state, &s.token).is_err());
    let s = begin_session(&state).unwrap();
    let foreign = nostr::Keys::generate();
    let wrong = EventBuilder::new(Kind::Custom(30175), "{}")
        .sign_with_keys(&foreign)
        .unwrap();
    assert!(hydrate_history(
        &state,
        &s.token,
        move |_| {
            let e = wrong.clone();
            async move { Ok(vec![e]) }
        },
        |_| async { panic!("wrong-owner history must not apply") }
    )
    .await
    .is_err());
    assert!(finish_session(&state, &s.token).is_err());
}

#[tokio::test]
async fn finish_waits_for_live_apply_and_latches_error_after_ready() {
    let state = state();
    let s = begin_session(&state).unwrap();
    hydrate_history(
        &state,
        &s.token,
        |_| async { Ok(vec![]) },
        |_| async { Ok(()) },
    )
    .await
    .unwrap();
    let lease = begin_apply(&state, Some(&s.token)).unwrap();
    assert_eq!(
        finish_session(&state, &s.token).err().as_deref(),
        Some("device_home_sync_pending")
    );
    lease.complete(&Ok(())).unwrap();
    finish_session(&state, &s.token).unwrap();
    let lease = begin_apply(&state, None).unwrap();
    lease.complete(&Err("live failed".into())).unwrap();
    assert_eq!(
        readiness_locked(&state, &capture_scope(&state).unwrap()).unwrap(),
        EvidenceReadiness::Failed
    );
    assert!(finish_session(&state, &s.token).is_err());
    let replacement = begin_session(&state).unwrap();
    let dropped = begin_apply(&state, Some(&replacement.token)).unwrap();
    drop(dropped);
    assert_eq!(
        readiness_locked(&state, &capture_scope(&state).unwrap()).unwrap(),
        EvidenceReadiness::Failed
    );
}

#[tokio::test]
async fn scope_change_during_fetch_and_old_live_tokens_cannot_apply_or_ready() {
    let state = state();
    let old = begin_session(&state).unwrap();
    let fetched = event(&state, 1);
    let result = hydrate_history(
        &state,
        &old.token,
        |_| {
            *state.relay_url_override.lock().unwrap() = Some("wss://new.example".into());
            let e = fetched.clone();
            async move { Ok(vec![e]) }
        },
        |_| async { panic!("stale history must not apply") },
    )
    .await;
    assert!(result.is_err());
    assert!(begin_apply(&state, Some(&old.token)).is_err());
    assert!(finish_session(&state, &old.token).is_err());
    let new = begin_session(&state).unwrap();
    assert!(invalidate_session(&state, &old.token).is_err());
    hydrate_history(
        &state,
        &new.token,
        |_| async { Ok(vec![]) },
        |_| async { Ok(()) },
    )
    .await
    .unwrap();
    finish_session(&state, &new.token).unwrap();
    *state.keys.lock().unwrap() = nostr::Keys::generate();
    assert_eq!(
        readiness_locked(&state, &capture_scope(&state).unwrap()).unwrap(),
        EvidenceReadiness::Pending
    );
}
