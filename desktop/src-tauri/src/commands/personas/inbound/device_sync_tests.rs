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

#[test]
fn signed_multi_coordinate_deletion_keeps_unselected_remote_head_and_newer_recreation() {
    use crate::device_identity::{load_existing_host_proof, DeviceIdentity};
    use crate::managed_agents::{
        agent_events::build_agent_event, definition_home::*,
        persona_device_view::read_remote_evidence,
    };
    use crate::secret_store::{SecretStore, TestBlobBackend};
    use std::sync::Arc;
    struct ExistingProof;
    impl TestBlobBackend for ExistingProof {
        fn read(&self) -> Result<Option<Vec<u8>>, String> {
            Ok(Some(
                br#"{"host":"00000000-0000-4000-8000-000000000001"}"#.to_vec(),
            ))
        }
        fn write(&self, _: &[u8]) -> Result<(), String> {
            panic!("proof must only be read")
        }
    }
    let mut store = SecretStore::keyring(format!("task3-fix-proof-{}", uuid::Uuid::new_v4()));
    store.test_backend = Some(Arc::new(ExistingProof));
    let proof = load_existing_host_proof(&store).unwrap();
    let device = DeviceIdentity {
        device_id: "own".into(),
        label: "Own".into(),
        created_at: "now".into(),
    };
    let dir = tempfile::tempdir().unwrap();
    let keys = nostr::Keys::generate();
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.config_mut().identifier = dir.path().to_str().unwrap().into();
    let state = crate::app_state::build_app_state();
    *state.keys.lock().unwrap() = keys.clone();
    *state.relay_url_override.lock().unwrap() = Some("wss://scope.example".into());
    let app = tauri::test::mock_builder()
        .manage(state)
        .build(context)
        .unwrap();
    assert_eq!(app.path().app_data_dir().unwrap(), dir.path());
    // Only retained remote heads: no local instance whose key could be hydrated.
    let definition: AgentDefinition = serde_json::from_value(serde_json::json!({"id":"one","display_name":"One","system_prompt":"Test","created_at":"now","updated_at":"now","origin_released":true})).unwrap();
    let mut x = definition.clone().into_agent_record();
    x.persona_id = Some("one".into());
    x.pubkey = nostr::Keys::generate().public_key().to_hex();
    let mut y = x.clone();
    y.pubkey = nostr::Keys::generate().public_key().to_hex();
    let path = scoped_retention_db_path(
        &dir.path().join("agents"),
        "wss://scope.example",
        &keys.public_key().to_hex(),
    );
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let conn = open_retention_db(&path).unwrap();
    let retain_head = |record: &ManagedAgentRecord, second: u64| {
        let event = build_agent_event(record)
            .unwrap()
            .custom_created_at(nostr::Timestamp::from(second))
            .sign_with_keys(&keys)
            .unwrap();
        retain_event(
            &conn,
            &RetainedEvent {
                kind: 30177,
                pubkey: keys.public_key().to_hex(),
                d_tag: record.pubkey.clone(),
                content: event.content.clone(),
                created_at: second as i64,
                raw_event: event.as_json(),
                pending_sync: false,
            },
        )
        .unwrap();
    };
    retain_head(&x, 100);
    retain_head(&y, 100);
    let deletion = EventBuilder::new(Kind::Custom(5), "")
        .tags([
            nostr::Tag::parse([
                "a",
                &format!("30177:{}:{}", keys.public_key().to_hex(), x.pubkey),
            ])
            .unwrap(),
            nostr::Tag::parse([
                "a",
                &format!("30177:{}:{}", keys.public_key().to_hex(), y.pubkey),
            ])
            .unwrap(),
        ])
        .custom_created_at(nostr::Timestamp::from(200))
        .sign_with_keys(&keys)
        .unwrap();
    let verified = parse_verified_inbound_event(&deletion.as_json()).unwrap();
    let state = app.state::<AppState>();
    let refreshes = std::cell::Cell::new(0);
    let apply_delete = || {
        reconcile_inbound_tombstone_with_refresh(
            &verified,
            "wss://scope.example",
            app.handle(),
            &state,
            || {
                refreshes.set(refreshes.get() + 1);
                assert!(
                    get_retained_event(&conn, 30177, &keys.public_key().to_hex(), &x.pubkey)
                        .unwrap()
                        .is_none(),
                    "refresh runs only after the authoritative delete commits"
                );
            },
        )
        .unwrap()
    };
    apply_delete();
    assert_eq!(refreshes.get(), 1);
    assert!(
        get_retained_event(&conn, 30177, &keys.public_key().to_hex(), &x.pubkey)
            .unwrap()
            .is_none()
    );
    assert!(
        get_retained_event(&conn, 30177, &keys.public_key().to_hex(), &y.pubkey)
            .unwrap()
            .is_some()
    );
    let evidence = read_remote_evidence(&path, &keys.public_key().to_hex()).unwrap();
    assert_eq!(
        evidence.len(),
        1,
        "only the authoritative first coordinate is deleted"
    );
    assert_eq!(evidence[0].pubkey, y.pubkey);
    let home = classify_definition_home(&definition, &[], &device, &proof, &evidence);
    assert_eq!(home.kind, HomeKind::Remote);
    assert!(
        !definition_capabilities(&definition, &home, EvidenceReadiness::Ready, false)
            .can_create_instance
    );
    assert!(
        !definition_capabilities(&definition, &home, EvidenceReadiness::Ready, false)
            .can_delete_definition
    );
    // A newer recreation remains active when the older delete is replayed.
    retain_head(&x, 300);
    apply_delete();
    assert_eq!(
        refreshes.get(),
        1,
        "skipped older deletion cannot refresh the nest"
    );
    let evidence = read_remote_evidence(&path, &keys.public_key().to_hex()).unwrap();
    assert_eq!(evidence.len(), 2);
    assert!(evidence.iter().any(|head| head.pubkey == x.pubkey));
    assert!(evidence.iter().any(|head| head.pubkey == y.pubkey));
    let records = crate::managed_agents::persona_device_view::read_policy_records(
        &dir.path().join("agents/managed-agents.json"),
    )
    .unwrap();
    assert!(records.iter().all(|record| record.pubkey.is_empty()));
    assert!(!dir.path().join("device.json").exists());
}
