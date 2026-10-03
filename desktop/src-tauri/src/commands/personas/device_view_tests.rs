use super::*;
use tauri::Manager;

#[test]
fn list_context_errors_are_explicit_and_preserve_only_shared_capabilities() {
    let dir = tempfile::tempdir().unwrap();
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.config_mut().identifier = dir.path().to_str().unwrap().into();
    let state = crate::app_state::build_app_state();
    *state.keys.lock().unwrap() = nostr::Keys::generate();
    let app = tauri::test::mock_builder()
        .manage(state)
        .build(context)
        .unwrap();
    assert_eq!(app.path().app_data_dir().unwrap(), dir.path());
    for policies in [
        vec![Some(true)],
        vec![None, Some(false)],
        vec![Some(true), None, Some(false)],
    ] {
        let definitions:Vec<AgentDefinition>=policies.iter().enumerate().map(|(i,policy)|serde_json::from_value(serde_json::json!({"id":format!("definition-{i}"),"display_name":"One","system_prompt":"Test","created_at":"now","updated_at":"now","share_across_devices":policy})).unwrap()).collect();
        save_personas(app.handle(), &definitions).unwrap();
        let persona_path = dir.path().join("agents/managed-agents.json");
        let before = std::fs::read(&persona_path).unwrap();
        // Missing metadata fails before a real keychain can be accessed.
        let result = list_personas_inner(app.handle());
        assert!(
            result.is_ok(),
            "a context error must not hide shared or private definitions"
        );
        let views: Vec<_> = result
            .unwrap()
            .into_iter()
            .filter(|v| !v.definition.is_builtin)
            .collect();
        assert_eq!(views.len(), definitions.len());
        for (view, policy) in views.iter().zip(&policies) {
            let json = serde_json::to_value(view).unwrap();
            assert!(json["home"].is_null());
            assert!(json["homeError"]
                .as_str()
                .unwrap()
                .contains("device identity read"));
            assert_eq!(
                json["capabilities"]["canCreateInstance"],
                policy == &Some(true)
            );
            assert_eq!(
                json["capabilities"]["canDeleteDefinition"],
                policy == &Some(true)
            );
            if policy != &Some(true) {
                assert_eq!(
                    json["capabilities"]["blockedReason"],
                    "device_home_sync_failed"
                );
            }
        }
        assert_eq!(std::fs::read(&persona_path).unwrap(), before);
        assert!(!dir.path().join("device.json").exists());
        assert!(
            !dir.path().join("agents/retention").exists(),
            "list must not create retained events"
        );
    }
    let path = dir.path().join("agents/managed-agents.json");
    std::fs::write(&path, b"corrupt").unwrap();
    assert!(list_personas_inner(app.handle()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"corrupt");
    assert!(
        !path.with_extension("json.invalid").exists(),
        "read-only list must not write repair evidence"
    );
}

#[test]
fn list_projects_retained_catalog_sharing_without_writing_or_requiring_host_proof() {
    use crate::managed_agents::{persona_events::build_persona_event, retention::*};
    use nostr::JsonUtil;
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
    let definition:AgentDefinition=serde_json::from_value(serde_json::json!({"id":"custom","display_name":"One","system_prompt":"Test","created_at":"now","updated_at":"now","share_across_devices":true})).unwrap();
    save_personas(app.handle(), std::slice::from_ref(&definition)).unwrap();
    let mut published = definition;
    published.shared = true;
    let event = build_persona_event(&published)
        .unwrap()
        .sign_with_keys(&keys)
        .unwrap();
    let path = scoped_retention_db_path(
        &dir.path().join("agents"),
        "wss://scope.example",
        &keys.public_key().to_hex(),
    );
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let conn = open_retention_db(&path).unwrap();
    retain_event(
        &conn,
        &RetainedEvent {
            kind: 30175,
            pubkey: keys.public_key().to_hex(),
            d_tag: "custom".into(),
            content: event.content.clone(),
            created_at: event.created_at.as_secs() as i64,
            raw_event: event.as_json(),
            pending_sync: false,
        },
    )
    .unwrap();
    drop(conn);
    let store_path = dir.path().join("agents/managed-agents.json");
    let before_store = std::fs::read(&store_path).unwrap();
    let before_db = std::fs::read(&path).unwrap();
    let views = list_personas_inner(app.handle()).unwrap();
    let view = views.iter().find(|v| v.definition.id == "custom").unwrap();
    assert!(
        view.definition.shared,
        "catalog projection must read the retained owner head"
    );
    assert!(view.home.is_none());
    assert!(view
        .home_error
        .as_ref()
        .unwrap()
        .contains("device identity read"));
    assert!(view.capabilities.can_create_instance);
    assert!(view.capabilities.can_delete_definition);
    assert_eq!(std::fs::read(&store_path).unwrap(), before_store);
    assert_eq!(std::fs::read(&path).unwrap(), before_db);
    assert!(!dir.path().join("device.json").exists());
}
