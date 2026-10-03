pub(crate) mod child_ownership;
use super::{
    bestie_assignment::recover_pending_assignment_cleanup, find_managed_agent_mut,
    kill_stale_tracked_processes, load_managed_agents, load_personas, managed_agents_base_dir,
    save_managed_agents, spawn_agent_child, sync_managed_agent_processes, BackendKind,
    ManagedAgentProcess,
};
use crate::app_state::AppState;
use crate::util;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Manager;

/// Outcome of a Phase B spawn attempt for one restore candidate.
///
/// `Skipped` covers the case where a concurrently-running startup reconcile
/// already spawned and tracked this exact pair during the Phase A window (the
/// transition lock is only held from Phase B onward). Restore must then leave
/// that live child alone rather than terminate-and-respawn it — mirroring the
/// live-child guard in `start_pair` (`runtime_commands.rs`). Without this,
/// restore would kill reconcile's lazy child by its receipt and replace it with
/// an eager one, flipping the pair's laziness on a startup race.
enum SpawnOutcome {
    /// Boxed: the spawned process carries its full spawn-config snapshot, so an
    /// inline variant would make every `Skipped`/`Failed` outcome pay for it.
    Spawned(super::ManagedAgentRuntimeKey, Box<ManagedAgentProcess>),
    Skipped,
    Failed(String),
}
type AgentSpawnResult = (String, SpawnOutcome);

/// Backfill the pinned persona snapshot for pre-existing agents created before
/// the record became the spawn source of truth. Runs once at launch, before
/// `restore_managed_agents_on_launch` spawns anything, so no agent boots from an
/// empty snapshot.
///
/// Only records with a `persona_id` but no `persona_source_version` are touched.
/// Records that already have a `persona_source_version` — including those whose
/// `model`/`provider` were clobbered by the old unconditional snapshot code before
/// this fix — are skipped here; they self-heal on the next manual start via the
/// start-path re-snapshot in `start_local_agent_with_preflight`.
/// If the linked persona is gone, we log loudly and leave the record untouched —
/// it stays orphaned and `spawn_agent_child` refuses to start it (see
/// `effective_config::resolve_effective_config`'s `OrphanedInstance` arm).
pub fn backfill_persona_snapshots(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;

    let mut records = load_managed_agents(app)?;
    let needs_backfill = records
        .iter()
        .any(|r| r.persona_id.is_some() && r.persona_source_version.is_none());
    if !needs_backfill {
        return Ok(());
    }

    let personas = load_personas(app)?;
    let mut changed = false;
    for record in records.iter_mut() {
        let Some(persona_id) = record.persona_id.clone() else {
            continue;
        };
        if record.persona_source_version.is_some() {
            continue;
        }
        let Some(persona) = personas.iter().find(|p| p.id == persona_id) else {
            eprintln!(
                "buzz-desktop: persona-snapshot backfill: agent {} links persona {persona_id} which no longer exists; leaving it orphaned — spawn will refuse it",
                record.pubkey
            );
            continue;
        };
        // Layer precedence at read time: persona env < agent env. When the
        // persona leaves model/provider blank, the record's own configured
        // values are preserved — a blank persona must not clobber a
        // user-configured agent. See `apply_persona_snapshot`.
        super::persona_events::apply_persona_snapshot(record, persona);
        record.updated_at = util::now_iso();
        changed = true;
    }

    if changed {
        save_managed_agents(app, &records)?;
    }
    Ok(())
}

/// Restore managed agents that were running before the app was closed.
///
/// Split into three phases to minimise lock contention with the frontend:
///   A (under lock): sync process state, cleanup, collect agents to start
///   B (no locks):   resolve commands and spawn processes in parallel
///   C (re-lock):    write back PIDs and status to records on disk
pub async fn restore_managed_agents_on_launch(
    app: &tauri::AppHandle,
    shutdown_started: &AtomicBool,
) -> Result<(), String> {
    if shutdown_started.load(Ordering::SeqCst) {
        return Ok(());
    }

    let state = app.state::<AppState>();

    {
        let _cleanup_transition = state
            .managed_agent_runtime_transition
            .lock()
            .map_err(|error| error.to_string())?;
        child_ownership::retry_restore_cleanup(app)?;
    }

    let agents_to_start = prepare_restore_phase_a_with(
        app,
        shutdown_started,
        super::persona_device_view::load_device_policy_context,
        super::storage::hydrate_keys,
        super::storage::persist_agent_keys,
        |records, eligible| prepare_restore_processes(app, records, eligible),
    )?;

    if agents_to_start.is_empty() {
        return Ok(());
    }

    // Snapshot the workspace owner pubkey once for the legacy auth_tag fallback.
    // Read outside the per-agent spawn loop so all parallel spawns see the same
    // value and we don't lock `state.keys` repeatedly.
    let owner_hex: Option<String> = state
        .keys
        .lock()
        .map_err(|e| e.to_string())
        .ok()
        .map(|k| k.public_key().to_hex());

    #[cfg(feature = "mesh-llm")]
    let agents_to_start = {
        // Preflight against the same resolution spawn uses — `resolve_effective_config`
        // (definition → global fallback). A linked instance's own `provider`/`model`/
        // `relay_mesh` bytes never contribute. See `start_local_agent_with_preflight`
        // in `commands/agents.rs` for the identical rationale on the interactive path.
        let personas = load_personas(app).unwrap_or_default();
        let global = super::load_global_agent_config(app).unwrap_or_default();
        let mut mesh_preflight_failures = std::collections::HashSet::new();
        for record in &agents_to_start {
            let mesh_model_id = super::effective_config::resolve_effective_relay_mesh_model_id(
                record, &personas, &global,
            );
            if mesh_model_id.is_none() {
                continue;
            }
            // Auto-start after relaunch: re-resolve a live bootstrap target and
            // dial it. Skip (with an actionable error) only when no live target
            // serves this model right now.
            if let Err(error) =
                crate::commands::ensure_relay_mesh_for_record(app, mesh_model_id.as_deref(), false)
                    .await
            {
                persist_restore_error(app, &record.pubkey, error)?;
                mesh_preflight_failures.insert(record.pubkey.clone());
            }
        }
        agents_to_start
            .into_iter()
            .filter(|record| !mesh_preflight_failures.contains(&record.pubkey))
            .collect::<Vec<_>>()
    };
    if agents_to_start.is_empty() {
        return Ok(());
    }

    // Serialize spawning and runtime registration with shutdown cleanup. The
    // shutdown flag is rechecked after taking the lock so shutdown either
    // prevents this transition or waits until every child is tracked and can
    // be terminated.
    let restore_transition = state
        .managed_agent_runtime_transition
        .lock()
        .map_err(|error| error.to_string())?;
    if shutdown_started.load(Ordering::SeqCst) {
        return Ok(());
    }

    // ── Phase B (transition lock held): resolve commands and spawn in parallel ──
    let spawn_results: Vec<AgentSpawnResult> = std::thread::scope(|scope| {
        let owner_hex_ref = owner_hex.as_deref();
        let handles: Vec<_> = agents_to_start
            .iter()
            .filter(|_| !shutdown_started.load(Ordering::SeqCst))
            .map(|record| {
                let handle = scope.spawn(move || {
                    let workspace_relay =
                        crate::relay::relay_ws_url_with_override(&app.state::<AppState>());
                    let relay_url = crate::relay::effective_agent_relay_url(
                        &record.relay_url,
                        &workspace_relay,
                    );
                    let outcome =
                        match super::ManagedAgentRuntimeKey::new(record.pubkey.clone(), &relay_url)
                        {
                            Ok(key) => {
                                // F2: if a concurrent startup reconcile already
                                // tracked a live child for this exact pair during
                                // the Phase A window, leave it alone. Mirrors the
                                // live-child guard in `start_pair`.
                                let already_live = app
                                    .state::<AppState>()
                                    .managed_agent_processes
                                    .lock()
                                    .ok()
                                    .and_then(|mut runtimes| {
                                        runtimes.get_mut(&key).map(|runtime| {
                                            runtime.child.try_wait().ok().flatten().is_none()
                                        })
                                    })
                                    .unwrap_or(false);
                                if already_live {
                                    SpawnOutcome::Skipped
                                } else {
                                    match super::terminate_untracked_pair_runtime(app, &key)
                                        .and_then(|()| {
                                            // F1: restore spawns lazy, matching
                                            // reconcile and manual start. Eager on
                                            // restore buys nothing — a crashed
                                            // mid-turn session is not resumed by an
                                            // eager child — and silently reintroduces
                                            // N idle brains on every launch.
                                            spawn_agent_child(
                                                app,
                                                record,
                                                &key.relay_url,
                                                true,
                                                owner_hex_ref,
                                                None,
                                            )
                                        }) {
                                        Ok(process) => {
                                            SpawnOutcome::Spawned(key, Box::new(process))
                                        }
                                        Err(error) => SpawnOutcome::Failed(error),
                                    }
                                }
                            }
                            Err(error) => SpawnOutcome::Failed(error),
                        };
                    (record.pubkey.clone(), outcome)
                });
                handle
            })
            .collect();

        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    if spawn_results.is_empty() {
        return Ok(());
    }

    let reconcile_items = child_ownership::complete_restore_spawn_results_with(
        app,
        spawn_results,
        super::persona_device_view::load_device_policy_context,
        super::storage::hydrate_keys,
        super::storage::persist_agent_keys,
        child_ownership::terminate_restore_child,
    )?;
    drop(restore_transition);

    // ── Profile reconciliation (fire-and-forget) ────────────────────────────
    // Spawn background tasks to ensure each restored agent's kind:0 profile is
    // published on the relay. Same pattern as the UI start path.
    for (pubkey, data) in reconcile_items {
        let reconcile_app = app.clone();
        tauri::async_runtime::spawn(async move {
            let state = reconcile_app.state::<AppState>();
            if let Err(e) =
                crate::commands::reconcile_agent_profile(&state, &reconcile_app, &pubkey, &data)
                    .await
            {
                eprintln!("buzz-desktop: profile reconciliation failed for agent {pubkey}: {e}");
            }
        });
    }

    Ok(())
}

/// Phase A production orchestration with only authority, key and process boundaries injected.
fn prepare_restore_phase_a_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    shutdown_started: &AtomicBool,
    context_provider: impl FnOnce(
        &tauri::AppHandle<R>,
        &AppState,
    )
        -> Result<super::persona_device_view::DevicePolicyContext, String>,
    hydrate: impl FnOnce(&mut [super::ManagedAgentRecord]),
    persist: impl FnOnce(&mut [super::ManagedAgentRecord]),
    processes: impl FnOnce(
        &mut [super::ManagedAgentRecord],
        &std::collections::HashSet<String>,
    ) -> Result<(bool, Vec<String>), String>,
) -> Result<Vec<super::ManagedAgentRecord>, String> {
    let state = app.state::<AppState>();
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;

    if shutdown_started.load(Ordering::SeqCst) {
        return Ok(Vec::new());
    }

    let policy_records =
        super::persona_device_view::read_policy_records(&super::managed_agents_store_path(app)?)?;
    let context = if needs_auto_start_authority(&policy_records) {
        Some(context_provider(app, &state)?)
    } else {
        None
    };
    let update_pubkeys = authorized_restore_updates(&policy_records, context.as_ref())?;
    let mut selected = select_auto_start_candidates(&policy_records, context.as_ref())?;
    hydrate(&mut selected);
    let eligible: std::collections::HashSet<_> =
        selected.iter().map(|r| r.pubkey.clone()).collect();
    let mut records = policy_records;
    records.retain(|r| !r.pubkey.is_empty());
    // Hydrate only eligible candidates; copied records must not import keys.
    for record in &mut records {
        if let Some(hydrated) = selected.iter().find(|r| r.pubkey == record.pubkey) {
            record.private_key_nsec = hydrated.private_key_nsec.clone();
        }
    }
    recover_pending_assignment_cleanup(&managed_agents_base_dir(app)?, |pending_pubkey| {
        records
            .iter()
            .any(|record| record.pubkey.eq_ignore_ascii_case(pending_pubkey))
    })?;
    let (mut changed, candidates) = processes(&mut records, &eligible)?;
    let mut agents_to_start: Vec<_> = records
        .iter()
        .filter(|r| candidates.contains(&r.pubkey))
        .cloned()
        .collect();

    // Re-snapshot persona config for agents about to be restored, matching
    // the interactive spawn path so auto-start agents also pick up the
    // current persona on app launch.
    let personas_for_snapshot = super::load_personas(app).unwrap_or_default();
    for record in records.iter_mut() {
        if !agents_to_start.iter().any(|r| r.pubkey == record.pubkey) {
            continue;
        }
        let Some(persona_id) = record.persona_id.clone() else {
            continue;
        };
        let Some(persona) = personas_for_snapshot.iter().find(|p| p.id == persona_id) else {
            // Orphaned: no current persona to re-snapshot from. Leave the
            // record as-is — `spawn_agent_child` (Phase B below) refuses to
            // spawn it and Phase C persists the refusal to `last_error`.
            continue;
        };
        super::persona_events::apply_persona_snapshot(record, persona);
        record.updated_at = util::now_iso();
        changed = true;
    }
    // Re-collect to_start from the updated records so Phase B spawns the refreshed config.
    agents_to_start = records
        .iter()
        .filter(|r| agents_to_start.iter().any(|s| s.pubkey == r.pubkey))
        .cloned()
        .collect();

    if changed {
        records.retain(|r| update_pubkeys.contains(&r.pubkey));
        super::storage::save_restore_records_with(app, &records, &eligible, persist)?;
    }
    Ok(agents_to_start)
}

fn prepare_restore_processes(
    app: &tauri::AppHandle,
    records: &mut [super::ManagedAgentRecord],
    eligible: &std::collections::HashSet<String>,
) -> Result<(bool, Vec<String>), String> {
    let state = app.state::<AppState>();
    let mut runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|error| error.to_string())?;
    let (mut changed, _exited) =
        sync_managed_agent_processes(records, &mut runtimes, &super::current_instance_id(app));
    changed |= kill_stale_tracked_processes(records, &runtimes, &super::current_instance_id(app));

    let tracked_pids: Vec<u32> = runtimes
        .values()
        .map(|runtime| runtime.child.id())
        .chain(
            super::read_all_agent_runtime_receipts(app)
                .into_iter()
                .filter_map(|(path, receipt)| {
                    super::valid_agent_runtime_receipt(
                        &path,
                        &receipt,
                        &super::current_instance_id(app),
                    )
                    .then_some(receipt.pid)
                }),
        )
        .collect();
    super::sweep_orphaned_agent_processes(app, &tracked_pids);

    // System-wide sweep: enumerate all user processes and kill any known
    // agent binaries not tracked by this session. Catches orphans whose
    // PID files were already cleaned up (e.g. agent workers in their own
    // process group whose parent harness exited).
    super::sweep_system_agent_processes(&super::current_instance_id(app), &tracked_pids);

    // Dead-instance reaping: find agents belonging to Buzz instances
    // whose desktop process is no longer running and reap them.
    super::reap_dead_instance_agents(&super::current_instance_id(app), &tracked_pids);

    // Exact-path sweep: kill any buzz-acp process whose executable path
    // matches this bundle's harness binary but is not in the tracked set.
    // Complements the env-var sweep above — catches orphans that predate
    // BUZZ_MANAGED_AGENT injection or lost their PID-file receipt.
    //
    // TODO: the three sweeps above each walk the PID table independently.
    // A future consolidation should collect a single shared process snapshot
    // at the top of this block and thread it through all sweep functions,
    // replacing the three separate kernel enumerations.
    super::sweep_untracked_bundle_harnesses(&tracked_pids);

    let candidates: Vec<String> = records
        .iter()
        .filter(|record| eligible.contains(&record.pubkey))
        .map(|record| record.pubkey.clone())
        .collect();

    let mut to_start = Vec::new();
    for pubkey in &candidates {
        if let Some(runtime) = runtimes
            .iter_mut()
            .find(|(key, _)| key.pubkey == *pubkey)
            .map(|(_, runtime)| runtime)
        {
            if runtime.child.try_wait().ok().flatten().is_none() {
                continue;
            }
        }
        if let Some(record) = records.iter().find(|r| r.pubkey == *pubkey) {
            if let Some(pid) = record.runtime_pid {
                if super::process_is_running(pid) {
                    continue;
                }
            }
            to_start.push(record.clone());
        }
    }
    Ok((changed, to_start.into_iter().map(|r| r.pubkey).collect()))
}

/// Phase C production reload/writeback with only authority, key and runtime boundaries injected.
fn complete_restore_phase_c_with<R: tauri::Runtime, T>(
    app: &tauri::AppHandle<R>,
    started_pubkeys: &std::collections::HashSet<String>,
    context_provider: impl FnOnce(
        &tauri::AppHandle<R>,
        &AppState,
    )
        -> Result<super::persona_device_view::DevicePolicyContext, String>,
    hydrate: impl FnOnce(&mut [super::ManagedAgentRecord]),
    persist: impl FnOnce(&mut [super::ManagedAgentRecord]),
    apply: impl FnOnce(&mut [super::ManagedAgentRecord]) -> Result<T, String>,
) -> Result<T, String> {
    let state = app.state::<AppState>();
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let raw =
        super::persona_device_view::read_policy_records(&super::managed_agents_store_path(app)?)?;
    // Phase B releases the store lock. Derive authority from CURRENT target rows
    // plus their current structural definitions, never a stale pubkey whitelist.
    let targets: Vec<_> = raw
        .iter()
        .filter(|r| r.pubkey.is_empty() || started_pubkeys.contains(&r.pubkey))
        .cloned()
        .collect();
    let context = if super::device_home_migration::needs_private_authority(&targets) {
        Some(context_provider(app, &state)?)
    } else {
        None
    };
    let authorized = authorized_restore_updates(&raw, context.as_ref())?;
    let mut records: Vec<_> = raw
        .into_iter()
        .filter(|r| !r.pubkey.is_empty() && authorized.contains(&r.pubkey))
        .collect();
    let key_targets: std::collections::HashSet<_> =
        started_pubkeys.intersection(&authorized).cloned().collect();
    let mut selected: Vec<_> = records
        .iter()
        .filter(|r| key_targets.contains(&r.pubkey))
        .cloned()
        .collect();
    hydrate(&mut selected);
    for record in &mut records {
        if let Some(hydrated) = selected.iter().find(|r| r.pubkey == record.pubkey) {
            record.private_key_nsec = hydrated.private_key_nsec.clone();
        }
    }
    let result = apply(&mut records)?;
    super::storage::save_restore_records_with(app, &records, &key_targets, persist)?;
    Ok(result)
}

/// Housekeeping may update safe non-candidates without resolving their keys.
/// Missing context freezes private non-candidates; it never triggers a proof read.
fn authorized_restore_updates(
    records: &[super::ManagedAgentRecord],
    context: Option<&super::persona_device_view::DevicePolicyContext>,
) -> Result<std::collections::HashSet<String>, String> {
    let definitions: Vec<_> = records
        .iter()
        .filter(|r| r.pubkey.is_empty())
        .cloned()
        .collect();
    let views: Vec<_> = definitions
        .iter()
        .filter_map(super::ManagedAgentRecord::to_definition_view)
        .collect();
    let mut authorized = std::collections::HashSet::new();
    for record in records.iter().filter(|r| !r.pubkey.is_empty()) {
        let mut relevant = definitions.clone();
        relevant.push(record.clone());
        if context.is_none() && super::device_home_migration::needs_private_authority(&relevant) {
            continue;
        }
        if super::device_home_migration::auto_start_allowed(record, &views, context)? {
            authorized.insert(record.pubkey.clone());
        }
    }
    Ok(authorized)
}

fn profile_reconcile_completed(outcome: crate::commands::ProfileReconcileOutcome) -> bool {
    outcome == crate::commands::ProfileReconcileOutcome::Reconciled
}

pub(crate) fn spawn_pending_profile_reconciliations(app: &tauri::AppHandle, workspace_relay: &str) {
    let state = app.state::<AppState>();
    if !state
        .managed_agent_profile_reconcile_enabled()
        .load(Ordering::Acquire)
    {
        return;
    }
    let items = match crate::commands::load_pending_profile_reconciliations(app, workspace_relay) {
        Ok(items) => items,
        Err(error) => {
            eprintln!("buzz-desktop: failed to load pending profile reconciliations: {error}");
            return;
        }
    };

    for (pubkey, data) in items {
        let reconcile_app = app.clone();
        let relay_url = data
            .target_relay_url
            .clone()
            .unwrap_or_else(|| data.relay_url.clone());
        tauri::async_runtime::spawn(async move {
            let state = reconcile_app.state::<AppState>();
            match crate::commands::reconcile_agent_profile(&state, &reconcile_app, &pubkey, &data)
                .await
            {
                Ok(outcome) if profile_reconcile_completed(outcome) => {
                    if let Err(error) = crate::commands::mark_profile_reconciled(
                        &reconcile_app,
                        &pubkey,
                        &relay_url,
                    ) {
                        eprintln!(
                            "buzz-desktop: failed to record profile reconciliation for agent {pubkey}: {error}"
                        );
                    }
                }
                Ok(_) => {}
                Err(error) => eprintln!(
                    "buzz-desktop: profile reconciliation failed for agent {pubkey}: {error}"
                ),
            }
        });
    }
}

#[cfg(feature = "mesh-llm")]
fn persist_restore_error(
    app: &tauri::AppHandle,
    pubkey: &str,
    error: String,
) -> Result<(), String> {
    persist_restore_error_with(
        app,
        pubkey,
        error,
        super::persona_device_view::load_device_policy_context,
        |_| {},
        |_| {},
    )
}

#[cfg(feature = "mesh-llm")]
fn persist_restore_error_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    pubkey: &str,
    error: String,
    context_provider: impl FnOnce(
        &tauri::AppHandle<R>,
        &AppState,
    )
        -> Result<super::persona_device_view::DevicePolicyContext, String>,
    hydrate: impl FnOnce(&mut [super::ManagedAgentRecord]),
    persist: impl FnOnce(&mut [super::ManagedAgentRecord]),
) -> Result<(), String> {
    complete_restore_phase_c_with(
        app,
        &[pubkey.to_string()].into_iter().collect(),
        context_provider,
        hydrate,
        persist,
        |records| {
            if let Ok(record) = find_managed_agent_mut(records, pubkey) {
                record.updated_at = util::now_iso();
                record.last_error = Some(error);
            }
            Ok(())
        },
    )
}

/// Select fresh auto-start records through the home guard before lifecycle work.
pub(crate) fn select_auto_start_candidates(
    records: &[super::ManagedAgentRecord],
    context: Option<&super::persona_device_view::DevicePolicyContext>,
) -> Result<Vec<super::ManagedAgentRecord>, String> {
    let definitions: Vec<_> = records
        .iter()
        .filter(|r| r.pubkey.is_empty())
        .filter_map(super::ManagedAgentRecord::to_definition_view)
        .collect();
    let mut selected = Vec::new();
    for record in records.iter().filter(|r| {
        !r.pubkey.is_empty() && r.start_on_app_launch && r.backend == BackendKind::Local
    }) {
        if super::device_home_migration::auto_start_allowed(record, &definitions, context)? {
            selected.push(record.clone());
        }
    }
    Ok(selected)
}

/// Require host authority only for records this auto-start operation selects.
/// Structural definitions remain available for canonical sharing lookup.
pub(crate) fn needs_auto_start_authority(records: &[super::ManagedAgentRecord]) -> bool {
    let relevant: Vec<_> = records
        .iter()
        .filter(|r| {
            r.pubkey.is_empty() || (r.start_on_app_launch && r.backend == BackendKind::Local)
        })
        .cloned()
        .collect();
    super::device_home_migration::needs_private_authority(&relevant)
}

#[cfg(test)]
mod profile_reconcile_tests {
    use super::profile_reconcile_completed;
    use crate::commands::ProfileReconcileOutcome;

    #[test]
    fn skipped_reconciliation_never_retires_pending_work() {
        assert!(profile_reconcile_completed(
            ProfileReconcileOutcome::Reconciled
        ));
        assert!(!profile_reconcile_completed(
            ProfileReconcileOutcome::SkippedDisabled
        ));
    }
}

#[cfg(test)]
#[path = "restore/device_home_tests.rs"]
mod device_home_restore_tests;
