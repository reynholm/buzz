use super::*;
use crate::managed_agents::{definition_home::EvidenceReadiness, device_home_sync, retention::*};
use nostr::{EventBuilder, JsonUtil, Kind};

#[tokio::test]
async fn hydration_applies_signed_catalog_through_production_dispatcher_and_propagates_failure() {
    let dir = tempfile::tempdir().unwrap();
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.config_mut().identifier = dir.path().to_str().unwrap().into();
    let state = crate::app_state::build_app_state();
    let keys = nostr::Keys::generate();
    *state.keys.lock().unwrap() = keys.clone();
    *state.relay_url_override.lock().unwrap() = Some("wss://scope.example".into());
    let app = tauri::test::mock_builder()
        .manage(state)
        .build(context)
        .unwrap();
    assert_eq!(app.path().app_data_dir().unwrap(), dir.path());
    let state = app.state::<AppState>();
    let session = device_home_sync::begin_session(&state).unwrap();
    let catalog = EventBuilder::new(Kind::Custom(30178), "{}")
        .tags([nostr::Tag::identifier("team")])
        .sign_with_keys(&keys)
        .unwrap();
    let result = device_home_sync::hydrate_history(
        &state,
        &session.token,
        move |_| {
            let e = catalog.clone();
            async move { Ok(vec![e]) }
        },
        |event| {
            let app = app.handle().clone();
            async move {
                assert!(reconcile_inbound_persona_event_blocking(
                    event.as_json(),
                    "wss://scope.example".into(),
                    app
                )?
                .is_none());
                Ok(())
            }
        },
    )
    .await
    .unwrap();
    assert_eq!(result.covered_event_ids.len(), 1);
    let path = scoped_retention_db_path(
        &dir.path().join("agents"),
        "wss://scope.example",
        &keys.public_key().to_hex(),
    );
    let conn = open_retention_db(&path).unwrap();
    assert!(
        get_retained_event(&conn, 30178, &keys.public_key().to_hex(), "team")
            .unwrap()
            .is_some()
    );
    device_home_sync::finish_session(&state, &session.token).unwrap();
    // Valid signed history with no coordinate fails in the real dispatcher.
    let invalid = EventBuilder::new(Kind::Custom(30178), "{}")
        .sign_with_keys(&keys)
        .unwrap();
    let session = device_home_sync::begin_session(&state).unwrap();
    let result = device_home_sync::hydrate_history(
        &state,
        &session.token,
        move |_| {
            let e = invalid.clone();
            async move { Ok(vec![e]) }
        },
        |event| {
            let app = app.handle().clone();
            async move {
                reconcile_inbound_persona_event_blocking(
                    event.as_json(),
                    "wss://scope.example".into(),
                    app,
                )?;
                Ok(())
            }
        },
    )
    .await;
    assert!(result.is_err());
    assert_eq!(
        device_home_sync::readiness_locked(
            &state,
            &device_home_sync::capture_scope(&state).unwrap()
        )
        .unwrap(),
        EvidenceReadiness::Failed
    );
    assert!(device_home_sync::finish_session(&state, &session.token).is_err());
    assert!(
        !dir.path().join("agents/managed-agents.json").exists(),
        "catalog retention must not create agent keys or instance storage"
    );
}
