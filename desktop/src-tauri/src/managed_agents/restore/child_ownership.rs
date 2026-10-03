//! Ownership of children spawned by restore until registration or cleanup.
use super::*;
use std::sync::Mutex;

/// Failed cleanup retains a live child handle for restore retry and shutdown.
#[derive(Default)]
pub(crate) struct RestoreCleanup(
    pub(super)  Mutex<
        Vec<(
            super::super::ManagedAgentRuntimeKey,
            Box<ManagedAgentProcess>,
        )>,
    >,
);

pub(super) fn complete_restore_spawn_results_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    mut spawn_results: Vec<AgentSpawnResult>,
    context_provider: impl FnOnce(
        &tauri::AppHandle<R>,
        &AppState,
    ) -> Result<
        super::super::persona_device_view::DevicePolicyContext,
        String,
    >,
    hydrate: impl FnOnce(&mut [super::super::ManagedAgentRecord]),
    persist: impl FnOnce(&mut [super::super::ManagedAgentRecord]),
    mut cleanup: impl FnMut(&mut ManagedAgentProcess) -> Result<(), String>,
) -> Result<Vec<(String, crate::commands::ProfileReconcileData)>, String> {
    let state = app.state::<AppState>();
    let started_pubkeys = spawn_results
        .iter()
        .map(|(pubkey, _)| pubkey.clone())
        .collect();
    let mut cleanup_errors = Vec::new();
    let result = complete_restore_phase_c_with(
        app,
        &started_pubkeys,
        context_provider,
        hydrate,
        persist,
        |records| {
            let mut runtimes = state
                .managed_agent_processes
                .lock()
                .map_err(|error| error.to_string())?;

            let mut successfully_spawned: Vec<(String, String)> = Vec::new();

            for (pubkey, outcome) in std::mem::take(&mut spawn_results) {
                match outcome {
                    // Skipped means a concurrent reconcile already owns a live child for
                    // this pair; leave its runtime and record state untouched.
                    SpawnOutcome::Skipped => continue,
                    SpawnOutcome::Spawned(key, process) => {
                        let Ok(record) = find_managed_agent_mut(records, &pubkey) else {
                            if let Err(error) =
                                settle_restore_child(app, key, process, &mut cleanup)
                            {
                                cleanup_errors.push(error);
                            }
                            continue;
                        };
                        // Never replace an existing tracked child, including a
                        // concurrent owner whose result was not marked Skipped.
                        if runtimes.contains_key(&key) {
                            if let Err(error) =
                                settle_restore_child(app, key, process, &mut cleanup)
                            {
                                cleanup_errors.push(error);
                            }
                            continue;
                        }
                        let now = util::now_iso();
                        let receipt = super::super::ManagedAgentRuntimeReceipt {
                            key: key.clone(),
                            pid: process.child.id(),
                            desktop_instance_id: super::super::current_instance_id(app),
                            started_at: now.clone(),
                        };
                        if let Err(error) = super::super::write_agent_runtime_receipt(app, &receipt)
                        {
                            if let Err(cleanup_error) =
                                settle_restore_child(app, key, process, &mut cleanup)
                            {
                                cleanup_errors.push(cleanup_error);
                            }
                            record.updated_at = now;
                            record.last_error = Some(error);
                            continue;
                        }
                        record.updated_at = now.clone();
                        record.runtime_pid = None;
                        record.last_started_at = Some(now);
                        record.last_stopped_at = None;
                        record.last_exit_code = None;
                        record.last_error = None;
                        runtimes.insert(
                            key.clone(),
                            super::super::ManagedAgentPairRuntime::starting(*process),
                        );
                        // Carry the spawn key's relay into profile reconciliation so
                        // the background task queries/publishes on the relay this
                        // spawn was actually keyed to — not whatever workspace is
                        // active when the task eventually executes.
                        successfully_spawned.push((pubkey, key.relay_url.clone()));
                    }
                    SpawnOutcome::Failed(error) => {
                        let Ok(record) = find_managed_agent_mut(records, &pubkey) else {
                            continue;
                        };
                        record.updated_at = util::now_iso();
                        record.last_error = Some(error);
                    }
                }
            }

            // Collect profile reconciliation data for successfully spawned agents before
            // releasing the lock. This mirrors the fire-and-forget pattern in
            // start_managed_agent — ensuring boot-restored agents get the same profile
            // self-healing as UI-started agents.
            let reconcile_personas = super::super::load_personas(app).unwrap_or_default();
            let reconcile_items: Vec<(String, crate::commands::ProfileReconcileData)> =
                successfully_spawned
                    .iter()
                    .filter_map(|(pubkey, spawn_relay)| {
                        let record = records.iter().find(|r| r.pubkey == *pubkey)?;
                        // Resolve the effective harness for the avatar-fallback
                        // derivation (the snapshot may be empty/stale for an inherited
                        // harness). Mirrors the UI start path.
                        let effective_command = crate::managed_agents::record_agent_command(
                            record,
                            &reconcile_personas,
                        );
                        Some((
                            pubkey.clone(),
                            crate::commands::ProfileReconcileData {
                                private_key_nsec: record.private_key_nsec.clone(),
                                name: record.name.clone(),
                                relay_url: record.relay_url.clone(),
                                // Pin the relay this spawn was keyed to (see the
                                // successfully_spawned push above) so the deferred
                                // task cannot resolve a post-switch workspace.
                                target_relay_url: Some(spawn_relay.clone()),
                                avatar_url: record.avatar_url.clone(),
                                auth_tag: record.auth_tag.clone(),
                                pubkey: record.pubkey.clone(),
                                agent_command: effective_command,
                                persona_id: record.persona_id.clone(),
                                about: crate::managed_agents::record_effective_description(
                                    record,
                                    &reconcile_personas,
                                ),
                            },
                        ))
                    })
                    .collect();

            drop(runtimes);
            Ok(reconcile_items)
        },
    );
    // The fallible authority/store preparation borrows results; it never owns
    // them. If it fails before applying, every newly spawned child remains here.
    for (_, outcome) in spawn_results {
        if let SpawnOutcome::Spawned(key, process) = outcome {
            if let Err(error) = settle_restore_child(app, key, process, &mut cleanup) {
                cleanup_errors.push(error);
            }
        }
    }
    if cleanup_errors.is_empty() {
        result
    } else {
        let cause = result
            .err()
            .map(|error| format!("{error}; "))
            .unwrap_or_default();
        Err(format!(
            "{cause}restore child cleanup failed: {}",
            cleanup_errors.join("; ")
        ))
    }
}

fn settle_restore_child<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    key: super::super::ManagedAgentRuntimeKey,
    mut process: Box<ManagedAgentProcess>,
    cleanup: &mut impl FnMut(&mut ManagedAgentProcess) -> Result<(), String>,
) -> Result<(), String> {
    if let Err(error) = cleanup(&mut process) {
        let state = app.state::<AppState>();
        // Recover a poisoned ownership mutex: dropping its live handles would
        // abandon them. The queue is independent of authorized runtime state.
        let mut pending = state
            .managed_agent_restore_cleanup
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let failure = format!("{} on {}: {error}", key.pubkey, key.relay_url);
        pending.push((key, process));
        return Err(failure);
    }
    Ok(())
}

/// Called under the runtime transition lock before restore or shutdown.
pub(crate) fn retry_restore_cleanup<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<(), String> {
    retry_restore_cleanup_with(app, terminate_restore_child)
}
pub(super) fn retry_restore_cleanup_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    mut cleanup: impl FnMut(&mut ManagedAgentProcess) -> Result<(), String>,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut pending = state
        .managed_agent_restore_cleanup
        .0
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let mut errors = Vec::new();
    pending.retain_mut(|(key, process)| match cleanup(process) {
        Ok(()) => false,
        Err(error) => {
            errors.push(format!("{} on {}: {error}", key.pubkey, key.relay_url));
            true
        }
    });
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "restore child cleanup retry failed: {}",
            errors.join("; ")
        ))
    }
}

pub(super) fn terminate_restore_child(process: &mut ManagedAgentProcess) -> Result<(), String> {
    super::super::terminate_process(process.child.id())?;
    wait_for_restore_child_exit(process, std::time::Duration::from_secs(1))
}

pub(super) fn wait_for_restore_child_exit(
    process: &mut ManagedAgentProcess,
    timeout: std::time::Duration,
) -> Result<(), String> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if process
            .child
            .try_wait()
            .map_err(|error| format!("failed to reap restore child: {error}"))?
            .is_some()
        {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "timed out reaping restore child {}",
                process.child.id()
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
