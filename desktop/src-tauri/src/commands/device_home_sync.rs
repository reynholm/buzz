//! IPC adapters for backend-owned device home hydration.
use crate::{
    app_state::AppState,
    managed_agents::{
        device_home_sync::{self, DeviceHomeHistory, DeviceHomeSyncSession},
        retention::{active_retention_scope, open_retention_db},
    },
};
use tauri::{AppHandle, Emitter, State};

/// Begin a scoped session; readiness starts Pending and has no frontend setter.
#[tauri::command]
pub fn begin_device_home_sync(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DeviceHomeSyncSession, String> {
    begin_device_home_sync_inner(&app, &state)
}
fn begin_device_home_sync_inner<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
) -> Result<DeviceHomeSyncSession, String> {
    // Revoke a previous Ready session even when opening retention now fails.
    device_home_sync::reset(state)?;
    let _guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let scope = active_retention_scope(app, state)?;
    open_retention_db(&scope.db_path)?;
    drop(_guard);
    device_home_sync::begin_session(state)
}
/// Fetch and apply exhaustive captured owner history through production reconciliation.
#[tauri::command]
pub async fn hydrate_device_home_history(
    session_token: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DeviceHomeHistory, String> {
    let scope = device_home_sync::capture_scope(&state)?;
    let keys = state.signing_keys()?;
    if keys.public_key().to_hex() != scope.owner_pubkey {
        return Err("device_home_sync_stale_session".into());
    }
    let api = crate::relay::relay_http_base_url(&scope.relay_url);
    device_home_sync::hydrate_history(
        &state,
        &session_token,
        |filter| {
            let state = &*state;
            let api = &api;
            let keys = &keys;
            async move {
                crate::relay::query_relay_at_with_keys(state, api, &[filter], keys, None).await
            }
        },
        |event| {
            use nostr::JsonUtil;
            super::personas::reconcile_inbound_persona_event(
                event.as_json(),
                scope.relay_url.clone(),
                Some(session_token.clone()),
                app.clone(),
            )
        },
    )
    .await
}
/// Complete the backend barrier only after exhaustive history and successful applies.
#[tauri::command]
pub fn finish_device_home_sync(
    session_token: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    finish_device_home_sync_inner(&session_token, &app, &state)
}
fn finish_device_home_sync_inner<R: tauri::Runtime>(
    session_token: &str,
    app: &AppHandle<R>,
    state: &AppState,
) -> Result<(), String> {
    device_home_sync::finish_session(state, session_token)?;
    let _ = app.emit("agents-data-changed", ());
    Ok(())
}
/// Invalidate only this subscription's session token.
#[tauri::command]
pub fn invalidate_device_home_sync(
    session_token: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    invalidate_device_home_sync_inner(&session_token, &app, &state)
}
fn invalidate_device_home_sync_inner<R: tauri::Runtime>(
    session_token: &str,
    app: &AppHandle<R>,
    state: &AppState,
) -> Result<(), String> {
    device_home_sync::invalidate_session(state, session_token)?;
    let _ = app.emit("agents-data-changed", ());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::Manager;
    fn app(path: &std::path::Path) -> tauri::App<tauri::test::MockRuntime> {
        let state = crate::app_state::build_app_state();
        *state.keys.lock().unwrap() = nostr::Keys::generate();
        *state.relay_url_override.lock().unwrap() = Some("wss://scope.example".into());
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = path.to_str().unwrap().into();
        tauri::test::mock_builder()
            .manage(state)
            .build(context)
            .unwrap()
    }
    #[tokio::test]
    async fn ipc_session_adapters_preserve_backend_completion_and_invalidation() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<AppState>();
        assert_eq!(app.path().app_data_dir().unwrap(), dir.path());
        let first = begin_device_home_sync_inner(app.handle(), &state).unwrap();
        assert!(finish_device_home_sync_inner(&first.token, app.handle(), &state).is_err());
        device_home_sync::hydrate_history(
            &state,
            &first.token,
            |_| async { Ok(vec![]) },
            |_| async { Ok(()) },
        )
        .await
        .unwrap();
        finish_device_home_sync_inner(&first.token, app.handle(), &state).unwrap();
        let next = begin_device_home_sync_inner(app.handle(), &state).unwrap();
        assert!(invalidate_device_home_sync_inner(&first.token, app.handle(), &state).is_err());
        assert_eq!(
            device_home_sync::readiness_locked(
                &state,
                &device_home_sync::capture_scope(&state).unwrap()
            )
            .unwrap(),
            crate::managed_agents::definition_home::EvidenceReadiness::Pending
        );
        invalidate_device_home_sync_inner(&next.token, app.handle(), &state).unwrap();
        assert!(finish_device_home_sync_inner(&next.token, app.handle(), &state).is_err());
    }
    #[tokio::test]
    async fn begin_retention_error_revokes_previous_readiness() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<AppState>();
        let first = begin_device_home_sync_inner(app.handle(), &state).unwrap();
        device_home_sync::hydrate_history(
            &state,
            &first.token,
            |_| async { Ok(vec![]) },
            |_| async { Ok(()) },
        )
        .await
        .unwrap();
        finish_device_home_sync_inner(&first.token, app.handle(), &state).unwrap();
        let scope = active_retention_scope(app.handle(), &state).unwrap();
        std::fs::write(&scope.db_path, b"corrupt").unwrap();
        assert!(begin_device_home_sync_inner(app.handle(), &state).is_err());
        assert_ne!(
            device_home_sync::readiness_locked(
                &state,
                &device_home_sync::capture_scope(&state).unwrap()
            )
            .unwrap(),
            crate::managed_agents::definition_home::EvidenceReadiness::Ready
        );
    }
}
