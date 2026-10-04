use super::*;
use crate::managed_agents::{
    definition_home::{EvidenceReadiness, RemoteInstanceEvidence},
    device_home_migration::{
        migrate_device_homes_locked_with_archive,
        tests::{app, context, records, write},
    },
    managed_agents_base_dir,
    persona_device_view::read_policy_records,
};

#[tokio::test]
async fn completion_uses_archive_before_local_claim_without_holding_store_or_apply_lock() {
    for confirmed in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<AppState>();
        let base = managed_agents_base_dir(app.handle()).unwrap();
        let (rs, key) = records();
        let duplicate = nostr::Keys::generate().public_key().to_hex();
        write(&base, &rs);
        let session = device_home_sync::begin_session(&state).unwrap();
        device_home_sync::hydrate_history(
            &state,
            &session.token,
            |_| async { Ok(vec![]) },
            |_| async { Ok(()) },
        )
        .await
        .unwrap();
        let mut c = context(EvidenceReadiness::Ready);
        c.evidence = vec![RemoteInstanceEvidence {
            pubkey: duplicate.clone(),
            persona_id: "one".into(),
        }];
        finish_device_home_sync_after_archive(
            &state,
            |target| {
                assert_eq!(
                    target.ws_url.trim_end_matches('/'),
                    device_home_sync::capture_scope(&state).unwrap().relay_url
                );
                assert!(state.managed_agents_store_lock.try_lock().is_ok());
                assert!(state.workspace_apply_lock.try_lock().is_ok());
                std::future::ready(Ok(if confirmed { vec![duplicate] } else { vec![] }))
            },
            |archived| {
                finish_device_home_sync_with(&session.token, app.handle(), &state, |scope| {
                    assert!(state.workspace_apply_lock.try_lock().is_err());
                    assert!(state.managed_agents_store_lock.try_lock().is_err());
                    c.scope = scope.clone();
                    migrate_device_homes_locked_with_archive(app.handle(), &c, archived, |_| {
                        Ok(Some(key.clone()))
                    })
                })
            },
        )
        .await
        .unwrap();
        let result = read_policy_records(&base.join("managed-agents.json")).unwrap();
        assert_eq!(result[1].device_host_binding.is_some(), confirmed);
        assert_eq!(result[1].pubkey, rs[1].pubkey);
        assert_eq!(
            device_home_sync::readiness_locked(
                &state,
                &device_home_sync::capture_scope(&state).unwrap()
            )
            .unwrap(),
            EvidenceReadiness::Ready
        );
    }
}

#[tokio::test]
async fn completion_rejects_archive_after_owner_relay_generation_or_token_replacement() {
    for change in ["owner", "relay", "generation", "token"] {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<AppState>();
        let base = managed_agents_base_dir(app.handle()).unwrap();
        let (rs, _) = records();
        write(&base, &rs);
        let before = std::fs::read(base.join("managed-agents.json")).unwrap();
        let session = device_home_sync::begin_session(&state).unwrap();
        device_home_sync::hydrate_history(
            &state,
            &session.token,
            |_| async { Ok(vec![]) },
            |_| async { Ok(()) },
        )
        .await
        .unwrap();
        let result = finish_device_home_sync_after_archive(
            &state,
            |_| async {
                tokio::task::yield_now().await;
                match change {
                    "owner" => *state.keys.lock().unwrap() = nostr::Keys::generate(),
                    "relay" => {
                        *state.relay_url_override.lock().unwrap() =
                            Some("wss://other.example".into())
                    }
                    "generation" => {
                        state
                            .workspace_apply_generation
                            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                    }
                    "token" => {
                        device_home_sync::begin_session(&state).unwrap();
                    }
                    _ => unreachable!(),
                }
                Ok(vec![nostr::Keys::generate().public_key().to_hex()])
            },
            |archived| {
                finish_device_home_sync_inner_with_archive(
                    &session.token,
                    app.handle(),
                    &state,
                    archived,
                )
            },
        )
        .await;
        assert!(
            result
                .unwrap_err()
                .contains("device_home_sync_stale_session"),
            "{change}"
        );
        assert_eq!(
            before,
            std::fs::read(base.join("managed-agents.json")).unwrap(),
            "{change}"
        );
    }
}

#[tokio::test]
async fn completion_accepts_equivalent_relay_url_without_borrowing_another_scope() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<AppState>();
    *state.relay_url_override.lock().unwrap() = Some("wss://scope.example/".into());
    let session = device_home_sync::begin_session(&state).unwrap();
    device_home_sync::hydrate_history(
        &state,
        &session.token,
        |_| async { Ok(vec![]) },
        |_| async { Ok(()) },
    )
    .await
    .unwrap();
    finish_device_home_sync_after_archive(
        &state,
        |_| async { Ok(vec![]) },
        |archived| {
            finish_device_home_sync_inner_with_archive(
                &session.token,
                app.handle(),
                &state,
                archived,
            )
        },
    )
    .await
    .unwrap();
}

// Real HTTP boundaries: modes 0/1/2/3 reject NIP-11, stall headers/body,
// or reject /query. Mode 4 provides the signed archive; 5 has the wrong kind.
async fn archive_relay(
    duplicate: String,
) -> (
    String,
    std::sync::Arc<std::sync::atomic::AtomicU8>,
    tokio::task::JoinHandle<()>,
) {
    use axum::{
        body::Body,
        http::StatusCode,
        response::IntoResponse,
        routing::{get, post},
        Json, Router,
    };
    use std::sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    };
    let keys = nostr::Keys::generate();
    let signer = keys.public_key().to_hex();
    let snapshot = |kind| {
        serde_json::to_value(
            nostr::EventBuilder::new(nostr::Kind::Custom(kind), "")
                .tags([nostr::Tag::parse(["p", &duplicate]).unwrap()])
                .sign_with_keys(&keys)
                .unwrap(),
        )
        .unwrap()
    };
    let valid = snapshot(13535);
    let wrong_kind = snapshot(1);
    let mode = Arc::new(AtomicU8::new(0));
    let get_mode = mode.clone();
    let query_mode = mode.clone();
    let router = Router::new()
        .route(
            "/",
            get(move || {
                let mode = get_mode.load(Ordering::SeqCst);
                let signer = signer.clone();
                async move {
                    match mode {
                        0 => StatusCode::SERVICE_UNAVAILABLE.into_response(),
                        1 => std::future::pending::<axum::response::Response>().await,
                        2 => axum::http::Response::builder()
                            .header("content-type", "application/json")
                            .body(Body::from_stream(futures_util::stream::pending::<
                                Result<axum::body::Bytes, std::io::Error>,
                            >()))
                            .unwrap(),
                        _ => Json(serde_json::json!({"self": signer})).into_response(),
                    }
                }
            }),
        )
        .route(
            "/query",
            post(move || {
                let mode = query_mode.load(Ordering::SeqCst);
                let snapshot = if mode == 5 {
                    wrong_kind.clone()
                } else {
                    valid.clone()
                };
                async move {
                    if mode == 3 {
                        StatusCode::SERVICE_UNAVAILABLE.into_response()
                    } else {
                        Json(serde_json::json!([snapshot])).into_response()
                    }
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (url, mode, task)
}

#[tokio::test]
async fn archive_transport_failures_and_invalid_kind_never_complete_as_empty_evidence() {
    use super::super::identity_archive::*;
    use crate::relay_admission::{reset_rate_limit_gate, TEST_SERIAL};
    let _serial = TEST_SERIAL.lock().await;
    reset_rate_limit_gate();
    let state = crate::app_state::build_app_state();
    let (url, mode, server) = archive_relay(nostr::Keys::generate().public_key().to_hex()).await;
    *state.relay_url_override.lock().unwrap() = Some(url);
    let target = capture_relay_target(&state);
    for failure in [0, 3, 5] {
        mode.store(failure, std::sync::atomic::Ordering::SeqCst);
        let result = fetch_verified_archived_pubkeys_at_with_timeout(
            &state,
            &target,
            std::time::Duration::from_millis(100),
        )
        .await;
        assert!(
            result.is_err(),
            "mode {failure} must remain retryable failure, got {result:?}"
        );
    }
    mode.store(4, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        fetch_verified_archived_pubkeys_at(&state, &target)
            .await
            .unwrap()
            .len(),
        1
    );
    server.abort();
    reset_rate_limit_gate();
}

#[tokio::test]
async fn stalled_archive_headers_or_body_are_bounded_and_recover_same_identity_and_token() {
    use super::super::identity_archive::*;
    use crate::relay_admission::{reset_rate_limit_gate, TEST_SERIAL};
    let _serial = TEST_SERIAL.lock().await;
    reset_rate_limit_gate();
    for stalled in [0, 1, 2, 3] {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<AppState>();
        let base = managed_agents_base_dir(app.handle()).unwrap();
        let (rs, key) = records();
        write(&base, &rs);
        let duplicate = nostr::Keys::generate().public_key().to_hex();
        let (url, mode, server) = archive_relay(duplicate.clone()).await;
        *state.relay_url_override.lock().unwrap() = Some(url);
        let session = device_home_sync::begin_session(&state).unwrap();
        device_home_sync::hydrate_history(
            &state,
            &session.token,
            |_| async { Ok(vec![]) },
            |_| async { Ok(()) },
        )
        .await
        .unwrap();
        let mut c = context(EvidenceReadiness::Ready);
        c.evidence = vec![RemoteInstanceEvidence {
            pubkey: duplicate,
            persona_id: "one".into(),
        }];
        let before = std::fs::read(base.join("managed-agents.json")).unwrap();
        for (phase, success) in [(stalled, false), (4, true)] {
            mode.store(phase, std::sync::atomic::Ordering::SeqCst);
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                finish_device_home_sync_after_archive(
                    &state,
                    |target| {
                        let state = &*state;
                        async move {
                            assert!(state.workspace_apply_lock.try_lock().is_ok());
                            assert!(state.managed_agents_store_lock.try_lock().is_ok());
                            fetch_verified_archived_pubkeys_at_with_timeout(
                                state,
                                &target,
                                std::time::Duration::from_millis(100),
                            )
                            .await
                        }
                    },
                    |archived| {
                        finish_device_home_sync_with(
                            &session.token,
                            app.handle(),
                            &state,
                            |scope| {
                                c.scope = scope.clone();
                                migrate_device_homes_locked_with_archive(
                                    app.handle(),
                                    &c,
                                    archived,
                                    |_| Ok(Some(key.clone())),
                                )
                            },
                        )
                    },
                ),
            )
            .await
            .expect("archive completion must honor its total deadline");
            if success {
                result.unwrap();
                let result = read_policy_records(&base.join("managed-agents.json")).unwrap();
                assert!(result[1].device_host_binding.is_some());
                assert_eq!(result[1].pubkey, rs[1].pubkey);
                assert_eq!(
                    device_home_sync::readiness_locked(
                        &state,
                        &device_home_sync::capture_scope(&state).unwrap()
                    )
                    .unwrap(),
                    EvidenceReadiness::Ready
                );
            } else {
                let error = result.unwrap_err();
                if matches!(stalled, 1 | 2) {
                    assert_eq!(error, "device_home_archive_timeout");
                } else {
                    assert!(
                        error.starts_with("device_home_archive_unavailable"),
                        "{error}"
                    );
                }
                assert_eq!(
                    std::fs::read(base.join("managed-agents.json")).unwrap(),
                    before
                );
                assert_eq!(
                    device_home_sync::readiness_locked(
                        &state,
                        &device_home_sync::capture_scope(&state).unwrap()
                    )
                    .unwrap(),
                    EvidenceReadiness::Pending
                );
            }
        }
        server.abort();
    }
    reset_rate_limit_gate();
}
