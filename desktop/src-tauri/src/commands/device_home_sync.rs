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
    let state_ref = &*state;
    let restore = finish_device_home_sync_deferred_restore_with(
        session_token,
        app,
        |target| async move {
            super::identity_archive::fetch_verified_archived_pubkeys_at(state_ref, &target).await
        },
        crate::managed_agents::live_process_sweeps,
    )
    .await?;
    tauri::async_runtime::spawn(async move {
        if let Err(error) = restore.run().await {
            eprintln!("buzz-desktop: deferred home restore failed: {error}");
        }
    });
    Ok(())
}

struct DeferredHomeRestore<R: tauri::Runtime, S> {
    app: AppHandle<R>,
    expected: device_home_sync::SyncScope,
    admission: crate::managed_agents::AdmissionSnapshot,
    sweeps: S,
}

impl<R: tauri::Runtime, S: FnOnce(&AppHandle<R>, &[u32])> DeferredHomeRestore<R, S> {
    async fn run(self) -> Result<(), String> {
        let state = self.app.state::<AppState>();
        retry_device_home_restore_with(&state, &self.expected, || async {
            crate::managed_agents::restore_managed_agents_on_launch(
                &self.app,
                &state.shutdown_started,
                self.admission,
                self.sweeps,
            )
            .await
        })
        .await
    }
}

// The IPC awaits completion, then schedules this owned restore. Only the network
// fetch and live process sweeps are replaceable in tests; completion and restore
// retain their actual native authority, token, admission and writeback checks.
async fn finish_device_home_sync_deferred_restore_with<R, Fut, S>(
    session_token: String,
    app: AppHandle<R>,
    fetch: impl FnOnce(super::identity_archive::RelayTarget) -> Fut,
    sweeps: S,
) -> Result<DeferredHomeRestore<R, S>, String>
where
    R: tauri::Runtime,
    Fut: std::future::Future<Output = Result<Vec<String>, String>>,
    S: FnOnce(&AppHandle<R>, &[u32]),
{
    let state = app.state::<AppState>();
    // Capture before the archive fetch and both workspace-lock waits. Removing
    // and re-adding the same relay must not bless this pending completion's start.
    let admission = crate::managed_agents::AdmissionSnapshot::capture(&state);
    let expected = finish_device_home_sync_after_archive(&state, fetch, |archived| {
        finish_device_home_sync_inner_with_archive(&session_token, &app, &state, archived)
    })
    .await?;
    Ok(DeferredHomeRestore {
        app,
        expected,
        admission,
        sweeps,
    })
}

// Network work must not hold either workspace-apply or managed-store locks.
// Revalidate the captured relay/owner/generation before consuming its snapshot.
async fn finish_device_home_sync_after_archive<Fut>(
    state: &AppState,
    fetch: impl FnOnce(super::identity_archive::RelayTarget) -> Fut,
    finish: impl FnOnce(&[String]) -> Result<device_home_sync::SyncScope, String>,
) -> Result<device_home_sync::SyncScope, String>
where
    Fut: std::future::Future<Output = Result<Vec<String>, String>>,
{
    let expected = device_home_sync::capture_scope(state)?;
    let target = super::identity_archive::capture_relay_target(state);
    if target.ws_url.trim().trim_end_matches('/') != expected.relay_url {
        return Err("device_home_sync_stale_session".into());
    }
    let archived = fetch(target).await?;
    let _apply = state.workspace_apply_lock.clone().lock_owned().await;
    if device_home_sync::capture_scope(state)? != expected {
        return Err("device_home_sync_stale_session".into());
    }
    // The actual completion adapter checks the token under its native barrier.
    finish(&archived)
}

#[cfg(test)]
#[path = "device_home_sync_archive_tests.rs"]
mod archive_tests;
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
#[cfg(test)]
fn finish_device_home_sync_inner<R: tauri::Runtime>(
    session_token: &str,
    app: &AppHandle<R>,
    state: &AppState,
) -> Result<device_home_sync::SyncScope, String> {
    finish_device_home_sync_inner_with_archive(session_token, app, state, &[])
}
fn finish_device_home_sync_inner_with_archive<R: tauri::Runtime>(
    session_token: &str,
    app: &AppHandle<R>,
    state: &AppState,
    archived: &[String],
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
        migrate_device_homes_locked_with_archive(
            app,
            &context,
            archived,
            crate::managed_agents::storage::resolve_agent_key_readonly,
        )
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

#[cfg(all(test, not(target_os = "windows")))]
mod deferred_restore_tests {
    use super::*;
    use crate::managed_agents::{
        admission_test_support::{app_with_keyless_agent, reached_spawn, TestApp, RELAY},
        load_managed_agents, readd_relay, remove_relay,
    };

    async fn hydrated(test: &TestApp) -> String {
        let state = test.app.state::<AppState>();
        let session = device_home_sync::begin_session(&state).unwrap();
        device_home_sync::hydrate_history(
            &state,
            &session.token,
            |_| async { Ok(vec![]) },
            |_| async { Ok(()) },
        )
        .await
        .unwrap();
        session.token
    }

    fn remove_and_readd(state: &AppState) {
        let scope = device_home_sync::capture_scope(state).unwrap();
        remove_relay(state, RELAY).unwrap();
        readd_relay(state, RELAY).unwrap();
        assert_eq!(device_home_sync::capture_scope(state).unwrap(), scope);
    }

    fn assert_start(test: &TestApp, removed: bool) {
        let records = load_managed_agents(test.app.handle()).unwrap();
        let error = records[0].last_error.as_deref();
        if removed {
            assert_eq!(error, None, "stale deferred restore reached spawn");
        } else {
            assert!(
                error.is_some_and(reached_spawn),
                "fresh restore did not reach spawn: {error:?}"
            );
        }
    }

    #[tokio::test]
    async fn remove_and_readd_during_archive_does_not_revive_deferred_restore() {
        for removed in [false, true] {
            let test = app_with_keyless_agent();
            let token = hydrated(&test).await;
            let state = test.app.state::<AppState>();
            let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
            let (release_tx, release_rx) = tokio::sync::oneshot::channel();
            let finish = finish_device_home_sync_deferred_restore_with(
                token,
                test.app.handle().clone(),
                |_| async {
                    entered_tx.send(()).unwrap();
                    release_rx.await.unwrap();
                    Ok(vec![])
                },
                |_, _| {},
            );
            tokio::pin!(finish);
            tokio::select! {
                result = &mut finish => panic!("archive did not pause: {}", result.is_ok()),
                _ = entered_rx => {},
            }
            if removed {
                remove_and_readd(&state);
            }
            release_tx.send(()).unwrap();
            finish.await.unwrap().run().await.unwrap();
            assert_start(&test, removed);
        }
    }

    #[tokio::test]
    async fn remove_and_readd_while_restore_waits_for_workspace_lock_does_not_revive_it() {
        for removed in [false, true] {
            let test = app_with_keyless_agent();
            let token = hydrated(&test).await;
            let state = test.app.state::<AppState>();
            let restore = finish_device_home_sync_deferred_restore_with(
                token,
                test.app.handle().clone(),
                |_| async { Ok(vec![]) },
                |_, _| {},
            )
            .await
            .unwrap();
            let apply = state.workspace_apply_lock.clone().lock_owned().await;
            let run = restore.run();
            tokio::pin!(run);
            tokio::select! {
                biased;
                result = &mut run => panic!("restore bypassed apply lock: {result:?}"),
                _ = tokio::task::yield_now() => {},
            }
            if removed {
                remove_and_readd(&state);
            }
            drop(apply);
            run.await.unwrap();
            assert_start(&test, removed);
        }
    }
}
