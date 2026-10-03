//! IPC adapters for backend-owned device home hydration.
use crate::{
    app_state::AppState,
    managed_agents::{
        device_home_sync::{self, DeviceHomeHistory, DeviceHomeSyncSession},
        retention::{active_retention_scope, open_retention_db},
    },
};
use tauri::{AppHandle, Emitter, Manager, State};

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
pub async fn finish_device_home_sync(
    session_token: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let apply_lock = state.workspace_apply_lock.clone().lock_owned().await;
    let expected = finish_device_home_sync_inner(&session_token, &app, &state)?;
    drop(apply_lock);
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if let Err(error) = retry_device_home_restore_with(&state, &expected, || async {
            crate::managed_agents::restore_managed_agents_on_launch(&app, &state.shutdown_started)
                .await
        })
        .await
        {
            eprintln!("buzz-desktop: deferred home restore failed: {error}");
        }
    });
    Ok(())
}
// Serialize posthistory retry with workspace applies and retain the completion's
// verified scope. Boundary injection leaves this scheduling fence in native tests.
async fn retry_device_home_restore_with<Fut>(
    state: &AppState,
    expected: &device_home_sync::SyncScope,
    restore: impl FnOnce() -> Fut,
) -> Result<(), String>
where
    Fut: std::future::Future<Output = Result<(), String>>,
{
    let _apply = state.workspace_apply_lock.clone().lock_owned().await;
    if device_home_sync::capture_scope(state)? != *expected {
        return Ok(());
    }
    restore().await
}
fn finish_device_home_sync_inner<R: tauri::Runtime>(
    session_token: &str,
    app: &AppHandle<R>,
    state: &AppState,
) -> Result<device_home_sync::SyncScope, String> {
    finish_device_home_sync_with(session_token, app, state, |scope| {
        use crate::managed_agents::{device_home_migration::*, persona_device_view::*};
        let records = read_policy_records(&crate::managed_agents::managed_agents_store_path(app)?)?;
        if !needs_private_authority(&records) {
            return Ok(());
        }
        let context = load_device_policy_context_at(
            app,
            scope.clone(),
            crate::managed_agents::definition_home::EvidenceReadiness::Ready,
        )?;
        migrate_device_homes_locked(app, &context)
    })
}
// Boundary injection preserves the actual completion adapter/barrier in native tests.
fn finish_device_home_sync_with<R: tauri::Runtime>(
    session_token: &str,
    app: &AppHandle<R>,
    state: &AppState,
    migrate: impl FnOnce(&device_home_sync::SyncScope) -> Result<(), String>,
) -> Result<device_home_sync::SyncScope, String> {
    let scope = device_home_sync::finish_session_with(state, session_token, migrate)?;
    let _ = app.emit("agents-data-changed", ());
    Ok(scope)
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

#[cfg(test)]
mod migration_tests {
    use super::*;
    use crate::managed_agents::{
        definition_home::EvidenceReadiness,
        device_home_migration::{
            migrate_device_homes_locked_with,
            tests::{app, context, records, write},
        },
        managed_agents_base_dir,
        persona_device_view::read_policy_records,
    };
    #[tokio::test]
    async fn finish_adapter_migrates_before_ready_under_actual_barrier() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<AppState>();
        let base = managed_agents_base_dir(app.handle()).unwrap();
        let (rs, key) = records();
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
        finish_device_home_sync_with(&session.token, app.handle(), &state, |scope| {
            assert!(state.managed_agents_store_lock.try_lock().is_err());
            assert!(
                state.device_home_sync.try_lock().is_err(),
                "Ready cannot be observed before migration"
            );
            c.scope = scope.clone();
            migrate_device_homes_locked_with(app.handle(), &c, |_| Ok(Some(key.clone())))
        })
        .unwrap();
        assert!(
            read_policy_records(&base.join("managed-agents.json")).unwrap()[1]
                .device_host_binding
                .is_some()
        );
        assert_eq!(
            device_home_sync::readiness_locked(
                &state,
                &device_home_sync::capture_scope(&state).unwrap()
            )
            .unwrap(),
            EvidenceReadiness::Ready
        );
    }
    #[tokio::test]
    async fn finish_adapter_key_error_latches_failed_and_preserves_store() {
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
        let mut c = context(EvidenceReadiness::Ready);
        assert!(
            finish_device_home_sync_with(&session.token, app.handle(), &state, |scope| {
                c.scope = scope.clone();
                migrate_device_homes_locked_with(
                    app.handle(),
                    &c,
                    |_| Err("keychain locked".into()),
                )
            })
            .is_err()
        );
        assert_eq!(
            before,
            std::fs::read(base.join("managed-agents.json")).unwrap()
        );
        assert_eq!(
            device_home_sync::readiness_locked(
                &state,
                &device_home_sync::capture_scope(&state).unwrap()
            )
            .unwrap(),
            EvidenceReadiness::Failed
        );
        assert!(
            finish_device_home_sync_with(&session.token, app.handle(), &state, |_| panic!(
                "failed migration cannot retry as Ready"
            ))
            .is_err()
        );
    }
    #[tokio::test]
    async fn returned_completion_scope_fences_deferred_restore() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<AppState>();
        let session = device_home_sync::begin_session(&state).unwrap();
        device_home_sync::hydrate_history(
            &state,
            &session.token,
            |_| async { Ok(vec![]) },
            |_| async { Ok(()) },
        )
        .await
        .unwrap();
        let verified =
            finish_device_home_sync_with(&session.token, app.handle(), &state, |_| Ok(())).unwrap();
        let count = std::cell::Cell::new(0);
        retry_device_home_restore_with(&state, &verified, || async {
            count.set(count.get() + 1);
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(count.get(), 1);
        state
            .workspace_apply_generation
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        retry_device_home_restore_with(&state, &verified, || async {
            panic!("replacement workspace stole old completion")
        })
        .await
        .unwrap();
    }
}
