//! Real store and invocation orchestration; only native provider/payload boundaries
//! are injected. A held provider request must leave unrelated store work available.
use super::*;
use crate::managed_agents::{
    device_home_migration::tests::{app, write},
    device_runtime::capture_runtime_fence,
    persona_device_view::read_policy_records,
    ManagedAgentRecord,
};
use std::sync::mpsc;
use std::time::Duration;
use tauri::Manager;

fn provider_record() -> ManagedAgentRecord {
    let mut record = super::tests::record();
    record.created_at = "original-incarnation".into();
    record.backend = BackendKind::Provider {
        id: "test-provider".into(),
        config: serde_json::json!({"cluster": "original"}),
    };
    record
}

fn prepare(
    app: &AppHandle<tauri::test::MockRuntime>,
    state: &AppState,
    record: &ManagedAgentRecord,
) -> Result<(std::path::PathBuf, serde_json::Value), String> {
    let scope = crate::managed_agents::device_home_sync::capture_scope(state)?;
    Ok((
        "test-provider".into(),
        serde_json::json!({
            "relay_url": scope.relay_url,
            "launch": {
                "owner_pubkey": scope.owner_pubkey,
                "env": crate::managed_agents::load_global_agent_config(app)?.env_vars,
            },
            "name": record.name,
            "respond_to": record.respond_to,
            "respond_to_allowlist": record.respond_to_allowlist,
        }),
    ))
}

#[test]
fn delayed_provider_allows_independent_store_action() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<AppState>();
    let base = crate::managed_agents::managed_agents_base_dir(app.handle()).unwrap();
    let original = provider_record();
    write(&base, std::slice::from_ref(&original));
    let fence = capture_runtime_fence(&state).unwrap();
    let handle = app.handle().clone();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let deploy = std::thread::spawn(move || {
        let state = handle.state::<AppState>();
        invoke_provider_with(
            &handle,
            &state,
            "agent",
            &fence,
            |_, _| panic!("legacy provider must not require host proof"),
            prepare,
            |_, _, config| {
                assert_eq!(config["cluster"], "original");
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok("provider-result".into())
            },
        )
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let store_available = match state.managed_agents_store_lock.try_lock() {
        Ok(_store) => {
            let mut sibling = original.clone();
            sibling.pubkey = "unrelated-agent".into();
            sibling.name = "Independent edit".into();
            write(&base, &[original, sibling]);
            true
        }
        Err(_) => false,
    };
    release_tx.send(()).unwrap();
    let result = deploy.join().unwrap();
    assert!(
        store_available,
        "delayed provider held global managed store lock"
    );
    result.unwrap();
    let rows = read_policy_records(&base.join("managed-agents.json")).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].name, "Independent edit");
    assert_eq!(rows[0].backend_agent_id.as_deref(), Some("provider-result"));
}

#[test]
fn workspace_replacement_during_prepare_refuses_external_deploy() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<AppState>();
    let base = crate::managed_agents::managed_agents_base_dir(app.handle()).unwrap();
    let original = provider_record();
    write(&base, std::slice::from_ref(&original));
    let before = std::fs::read(base.join("managed-agents.json")).unwrap();
    let fence = capture_runtime_fence(&state).unwrap();
    let invoked = std::cell::Cell::new(false);
    let result = invoke_provider_with(
        app.handle(),
        &state,
        "agent",
        &fence,
        |_, _| panic!("legacy proof"),
        |app, state, record| {
            let inputs = prepare(app, state, record)?;
            state
                .workspace_apply_generation
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            Ok(inputs)
        },
        |_, _, _| {
            invoked.set(true);
            Ok("stale-deploy".into())
        },
    );
    assert!(result.is_err());
    assert!(
        !invoked.get(),
        "stale prepared scope reached external deploy"
    );
    assert!(std::fs::read(base.join("managed-agents.json")).unwrap() == before);
}

fn changed_during_deploy(
    change: impl FnOnce(&AppHandle<tauri::test::MockRuntime>, &AppState, &mut ManagedAgentRecord),
    provider_result: Result<String, String>,
) -> (Result<(), String>, Vec<ManagedAgentRecord>, Vec<u8>) {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<AppState>();
    let base = crate::managed_agents::managed_agents_base_dir(app.handle()).unwrap();
    let original = provider_record();
    write(&base, std::slice::from_ref(&original));
    let fence = capture_runtime_fence(&state).unwrap();
    let mut expected_bytes = Vec::new();
    let result = invoke_provider_with(
        app.handle(),
        &state,
        "agent",
        &fence,
        |_, _| panic!("legacy provider must not require host proof"),
        prepare,
        |_, _, _| {
            let mut current = original.clone();
            change(app.handle(), &state, &mut current);
            let _store = state.managed_agents_store_lock.lock().unwrap();
            write(&base, &[current]);
            expected_bytes = std::fs::read(base.join("managed-agents.json")).unwrap();
            provider_result
        },
    );
    let actual_bytes = std::fs::read(base.join("managed-agents.json")).unwrap();
    let rows = read_policy_records(&base.join("managed-agents.json")).unwrap();
    if result.is_err() && result.as_ref().unwrap_err() != "old provider failure" {
        assert_eq!(
            actual_bytes, expected_bytes,
            "stale deploy mutated the store"
        );
    }
    (result, rows, actual_bytes)
}

#[test]
fn recreated_same_pubkey_refuses_stale_provider_success_and_failure() {
    for provider_result in [Ok("old-id".into()), Err("old provider failure".into())] {
        let (result, rows, _) = changed_during_deploy(
            |app, state, record| {
                let _store = state.managed_agents_store_lock.lock().unwrap();
                let base = crate::managed_agents::managed_agents_base_dir(app).unwrap();
                write(&base, &[]); // delete before recreating the same pubkey
                record.created_at = "replacement-incarnation".into();
            },
            provider_result,
        );
        assert!(
            result.is_err(),
            "stale deploy accepted replacement incarnation"
        );
        assert!(result.unwrap_err().contains("provider_deploy_stale_target"));
        assert_eq!(rows[0].created_at, "replacement-incarnation");
        assert_eq!(rows[0].backend_agent_id, None);
        assert_eq!(rows[0].last_error, None);
    }
}

#[test]
fn changed_backend_config_refuses_stale_provider_result() {
    let (result, rows, _) = changed_during_deploy(
        |_, _, record| {
            record.backend = BackendKind::Provider {
                id: "test-provider".into(),
                config: serde_json::json!({"cluster": "replacement"}),
            };
        },
        Ok("old-id".into()),
    );
    assert!(
        result.is_err(),
        "stale deploy accepted replacement provider config"
    );
    assert!(result.unwrap_err().contains("provider_deploy_stale_target"));
    assert_eq!(rows[0].backend_agent_id, None);
}

#[test]
fn workspace_or_subscription_change_refuses_stale_provider_result() {
    for subscription_change in [false, true] {
        let (result, rows, _) = changed_during_deploy(
            |_, state, _| {
                if subscription_change {
                    crate::managed_agents::device_home_sync::begin_session(state).unwrap();
                } else {
                    state
                        .workspace_apply_generation
                        .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                }
            },
            Ok("old-id".into()),
        );
        assert_eq!(result.unwrap_err(), "device_home_sync_stale_session");
        assert_eq!(rows[0].backend_agent_id, None);
    }
}

#[test]
fn changed_effective_payload_refuses_stale_provider_result() {
    let (result, rows, _) = changed_during_deploy(
        |app, state, _| {
            let _store = state.managed_agents_store_lock.lock().unwrap();
            let base = crate::managed_agents::managed_agents_base_dir(app).unwrap();
            crate::managed_agents::storage::atomic_write_json_restricted(
                &base.join("global-agent-config.json"),
                br#"{"env_vars":{"NEW_SETTING":"new value"}}"#,
            )
            .unwrap();
        },
        Ok("old-id".into()),
    );
    assert!(
        result.is_err(),
        "stale deploy accepted changed effective payload"
    );
    assert!(result.unwrap_err().contains("provider_deploy_stale_target"));
    assert_eq!(rows[0].backend_agent_id, None);
}
