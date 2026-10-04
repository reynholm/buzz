// ── Post-install auto-restart (Phase 2 of install_acp_runtime) ───────────────
//
// After a successful adapter install, restart any local agents that:
//   1. are local backend + have a live PID,
//   2. their effective command maps to the just-installed runtime,
//   3. were spawned in setup-listener mode (setup_mode stamp), AND
//   4. their readiness now computes Ready.
//
// Mirrors the two-phase shape of set_global_agent_config.

/// Outcome of a single per-agent restart attempt during post-install restart.
#[derive(Debug)]
enum InstallRestartOutcome {
    Restarted,
    FailedAfterStop,
    Skipped,
}

/// Pure predicate: should this agent be restarted after an adapter install?
///
/// Extracted for unit testing — callers must still re-verify under the lock.
/// The caller is responsible for computing `pid_alive` (via `process_is_running`)
/// before invoking this function, keeping the predicate OS-agnostic and testable
/// on all platforms.
///
/// An agent qualifies iff:
/// - it is a local backend with a live PID (`pid_alive`),
/// - its effective command maps to `runtime_id`,
/// - it was **spawned in setup-listener mode** (`setup_mode`), AND
/// - its readiness **now computes `Ready`** (install fixed the blocker).
pub(super) fn should_restart_after_install(
    is_local: bool,
    pid_alive: bool,
    runtime_matches: bool,
    setup_mode: bool,
    now_ready: bool,
) -> bool {
    is_local && pid_alive && runtime_matches && setup_mode && now_ready
}

/// Restart all setup-mode agents whose runtime matches `runtime_id` and whose
/// readiness now computes Ready.  Returns `(restarted_count, failed_restart_count)`.
pub(super) async fn restart_setup_mode_agents_after_install(
    app: &tauri::AppHandle,
    runtime_id: &str,
    expected: &crate::managed_agents::device_runtime::RuntimeFence,
) -> (u32, u32) {
    use crate::{
        app_state::AppState,
        managed_agents::{
            agent_readiness, known_acp_runtime, load_global_agent_config, load_personas,
            record_agent_command, resolve_effective_agent_env, AgentReadiness, BackendKind,
        },
    };
    use tauri::Manager;

    // ── Pre-scan: collect candidate pubkeys without holding locks ────────────
    let app_for_scan = app.clone();
    let runtime_id_owned = runtime_id.to_string();
    let candidates = tokio::task::spawn_blocking(move || {
        let records = crate::managed_agents::persona_device_view::read_policy_records(
            &crate::managed_agents::managed_agents_store_path(&app_for_scan).unwrap_or_default(),
        )
        .unwrap_or_default();
        let personas = load_personas(&app_for_scan).unwrap_or_default();
        let global = load_global_agent_config(&app_for_scan).unwrap_or_default();

        // Read the runtimes map to check setup_mode stamps.
        let state_inner = app_for_scan.state::<AppState>();
        let runtimes = state_inner
            .managed_agent_processes
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        records
            .iter()
            .filter(|record| {
                let is_local = record.backend == BackendKind::Local;
                let effective_cmd = record_agent_command(record, &personas);
                let runtime_matches =
                    known_acp_runtime(&effective_cmd).is_some_and(|r| r.id == runtime_id_owned);
                let setup_mode = runtimes
                    .iter()
                    .find(|(key, _)| key.pubkey == record.pubkey)
                    .map(|(_, p)| p.setup_mode)
                    .unwrap_or(false);
                let effective = resolve_effective_agent_env(
                    record,
                    &personas,
                    known_acp_runtime(&effective_cmd),
                    &global,
                );
                let now_ready = matches!(agent_readiness(&effective), AgentReadiness::Ready);
                let pid_alive = runtimes.iter().any(|(key, runtime)| {
                    key.pubkey.eq_ignore_ascii_case(&record.pubkey)
                        && crate::managed_agents::process_is_running(runtime.child.id())
                });
                should_restart_after_install(
                    is_local,
                    pid_alive,
                    runtime_matches,
                    setup_mode,
                    now_ready,
                )
            })
            .map(|r| r.pubkey.clone())
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();

    if candidates.is_empty() {
        return (0, 0);
    }

    let mut restarted_count: u32 = 0;
    let mut failed_restart_count: u32 = 0;

    for pubkey in &candidates {
        let outcome = restart_single_agent_after_install(app, pubkey, runtime_id, expected).await;
        match outcome {
            InstallRestartOutcome::Restarted => restarted_count += 1,
            InstallRestartOutcome::FailedAfterStop => failed_restart_count += 1,
            InstallRestartOutcome::Skipped => {}
        }
    }

    (restarted_count, failed_restart_count)
}

/// Stop-then-start a single setup-mode agent after a successful adapter install.
///
/// Mirrors `restart_local_agent_on_config_change` from `global_agent_config.rs`:
/// eligibility is re-verified under the store lock before the stop, then the
/// agent is restarted via `start_local_agent_with_preflight`.
async fn restart_single_agent_after_install(
    app: &tauri::AppHandle,
    pubkey: &str,
    runtime_id: &str,
    expected: &crate::managed_agents::device_runtime::RuntimeFence,
) -> InstallRestartOutcome {
    use crate::{
        app_state::AppState,
        managed_agents::{
            agent_readiness, current_instance_id, find_managed_agent_mut, known_acp_runtime,
            load_global_agent_config, load_personas, record_agent_command,
            resolve_effective_agent_env, stop_managed_agent_process, sync_managed_agent_processes,
            AgentReadiness, BackendKind,
        },
    };
    use tauri::Manager;

    let stop_fence = expected.clone();
    let app_for_stop = app.clone();
    let pubkey_owned = pubkey.to_string();
    let runtime_id_owned = runtime_id.to_string();
    let state = app.state::<AppState>();
    // Capture before stopping so removal during restart refuses the start.
    let admission = crate::managed_agents::AdmissionSnapshot::capture(&state);

    let stop_result = tokio::task::spawn_blocking(move || {
        let state = app_for_stop.state::<AppState>();

        let _store_guard = state
            .managed_agents_store_lock
            .lock()
            .map_err(|e| format!("failed to acquire store lock: {e}"))?;

        discovery_restart_phase_with(
            &app_for_stop,
            &state,
            &pubkey_owned,
            Some(&stop_fence),
            crate::managed_agents::persona_device_view::load_device_policy_context,
            |current, _, _| {
                let mut records = vec![current];
                let mut runtimes = state
                    .managed_agent_processes
                    .lock()
                    .map_err(|e| format!("failed to acquire runtimes lock: {e}"))?;

                // Sync process state so PID liveness reflects current reality.
                let (sync_changed, _) = sync_managed_agent_processes(
                    &mut records,
                    &mut runtimes,
                    &current_instance_id(&app_for_stop),
                );
                if sync_changed {
                    crate::managed_agents::storage::save_restore_records_with(
                        &app_for_stop,
                        &records,
                        &std::collections::HashSet::new(),
                        |_| {},
                    )?;
                }

                // Re-verify eligibility under lock.
                let record = records
                    .iter()
                    .find(|r| r.pubkey == pubkey_owned)
                    .ok_or_else(|| format!("agent {pubkey_owned} not found"))?;

                if record.backend != BackendKind::Local {
                    return Err(format!("agent {pubkey_owned} is no longer a local agent"));
                }
                let runtime_keys =
                    crate::managed_agents::managed_agent_runtime_keys(&runtimes, &pubkey_owned);
                if runtime_keys.is_empty() {
                    return Err(format!(
                        "agent {pubkey_owned} no longer has a live pair runtime after sync"
                    ));
                }

                let personas = load_personas(&app_for_stop)?;
                let global = load_global_agent_config(&app_for_stop).unwrap_or_default();

                let effective_cmd = record_agent_command(record, &personas);
                let runtime_matches =
                    known_acp_runtime(&effective_cmd).is_some_and(|r| r.id == runtime_id_owned);
                if !runtime_matches {
                    return Err(format!(
                "agent {pubkey_owned} runtime no longer matches {runtime_id_owned} under lock"
            ));
                }

                let setup_mode = runtimes
                    .iter()
                    .find(|(key, _)| key.pubkey == pubkey_owned)
                    .map(|(_, p)| p.setup_mode)
                    .unwrap_or(false);
                if !setup_mode {
                    return Err(format!(
                        "agent {pubkey_owned} is not in setup mode under lock — skipping"
                    ));
                }

                let runtime_meta = known_acp_runtime(&effective_cmd);
                let effective =
                    resolve_effective_agent_env(record, &personas, runtime_meta, &global);
                if !matches!(agent_readiness(&effective), AgentReadiness::Ready) {
                    return Err(format!(
                "agent {pubkey_owned} readiness is still NotReady after install — not bouncing"
            ));
                }

                // Stop the process.
                let record_mut = find_managed_agent_mut(&mut records, &pubkey_owned)?;
                stop_managed_agent_process(&app_for_stop, record_mut, &mut runtimes)?;
                crate::managed_agents::storage::save_restore_records_with(
                    &app_for_stop,
                    &records,
                    &std::collections::HashSet::new(),
                    |_| {},
                )?;

                Ok(runtime_keys)
            },
        )
    })
    .await;

    let runtime_keys = match stop_result {
        Ok(Ok(runtime_keys)) => runtime_keys,
        Ok(Err(e)) => {
            eprintln!("buzz-desktop: install_acp_runtime: skipping restart of {pubkey}: {e}");
            return InstallRestartOutcome::Skipped;
        }
        Err(e) => {
            eprintln!(
                "buzz-desktop: install_acp_runtime: spawn_blocking failed for stop of {pubkey}: {e}"
            );
            return InstallRestartOutcome::Skipped;
        }
    };

    let relay_urls: Vec<_> = runtime_keys.into_iter().map(|key| key.relay_url).collect();
    let state = app.state::<AppState>();
    match crate::commands::agents::runtime_start::start_local_agent_pairs_scoped(
        app,
        &state,
        pubkey,
        &relay_urls,
        Some(expected),
        &admission,
    )
    .await
    {
        Ok(_) => {
            eprintln!(
                "buzz-desktop: install_acp_runtime: restarted setup-mode agent {pubkey} after install"
            );
            InstallRestartOutcome::Restarted
        }
        Err(e) => {
            eprintln!(
                "buzz-desktop: install_acp_runtime: failed to start {pubkey} after install: {e}"
            );
            if let Err(save_err) = persist_last_error_on_install(app, pubkey, &e, expected) {
                eprintln!(
                    "buzz-desktop: install_acp_runtime: failed to persist last_error for {pubkey}: {save_err}"
                );
            }
            InstallRestartOutcome::FailedAfterStop
        }
    }
}

/// Persist a `last_error` on the agent record under the store lock.
/// Best-effort: called only after a failed restart.
fn persist_last_error_on_install(
    app: &tauri::AppHandle,
    pubkey: &str,
    error: &str,
    expected: &crate::managed_agents::device_runtime::RuntimeFence,
) -> Result<(), String> {
    use crate::app_state::AppState;
    use tauri::Manager;
    let state = app.state::<AppState>();
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| format!("failed to acquire store lock: {e}"))?;
    crate::managed_agents::device_runtime::assert_runtime_fence(&state, expected)?;
    crate::managed_agents::device_runtime::runtime_phase_locked_with(
        app,
        &state,
        pubkey,
        Some(&expected.scope),
        crate::managed_agents::persona_device_view::load_device_policy_context,
        |mut record, _, _| {
            record.last_error = Some(error.to_string());
            record.updated_at = crate::util::now_iso();
            crate::managed_agents::storage::save_restore_records_with(
                app,
                &[record],
                &std::collections::HashSet::new(),
                |_| {},
            )
        },
    )
}

/// Authorize the whole locked restart effect before any key, process or store operation.
fn discovery_restart_phase_with<R: tauri::Runtime, T>(
    app: &tauri::AppHandle<R>,
    state: &crate::app_state::AppState,
    pubkey: &str,
    expected: Option<&crate::managed_agents::device_runtime::RuntimeFence>,
    context: impl FnOnce(
        &tauri::AppHandle<R>,
        &crate::app_state::AppState,
    ) -> Result<
        crate::managed_agents::persona_device_view::DevicePolicyContext,
        String,
    >,
    effect: impl FnOnce(
        crate::managed_agents::ManagedAgentRecord,
        Vec<crate::managed_agents::AgentDefinition>,
        crate::managed_agents::device_home_sync::SyncScope,
    ) -> Result<T, String>,
) -> Result<T, String> {
    if let Some(expected) = expected {
        crate::managed_agents::device_runtime::assert_runtime_fence(state, expected)?;
    }
    crate::managed_agents::device_runtime::runtime_phase_locked_with(
        app,
        state,
        pubkey,
        expected.map(|e| &e.scope),
        context,
        effect,
    )
}
#[cfg(test)]
mod device_runtime_guard_tests {
    use super::discovery_restart_phase_with;
    use crate::managed_agents::{
        definition_home::EvidenceReadiness,
        device_home_migration::tests::{app, context, records, write},
    };
    use tauri::Manager;
    #[test]
    fn discovery_restart_phase_with_refuses_before_process_key_and_store_effects() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<crate::app_state::AppState>();
        let (mut raw, _) = records();
        raw[1].device_host_binding = Some("copied".into());
        write(
            &crate::managed_agents::managed_agents_base_dir(app.handle()).unwrap(),
            &raw,
        );
        let effects = std::cell::Cell::new(0);
        let result = discovery_restart_phase_with(
            app.handle(),
            &state,
            &raw[1].pubkey,
            None,
            |_, state| {
                let mut c = context(EvidenceReadiness::Ready);
                c.scope = crate::managed_agents::device_home_sync::capture_scope(state)?;
                Ok(c)
            },
            |_, _, _| {
                effects.set(1);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(effects.get(), 0);
    }
}
