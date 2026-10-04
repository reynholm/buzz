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
                std::future::ready(if confirmed { vec![duplicate] } else { vec![] })
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
                vec![nostr::Keys::generate().public_key().to_hex()]
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
        |_| async { vec![] },
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
