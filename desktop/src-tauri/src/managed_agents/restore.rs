pub(crate) mod child_ownership;
use super::{
    bestie_assignment::recover_pending_assignment_cleanup, find_managed_agent_mut,
    kill_stale_tracked_processes, managed_agents_base_dir, spawn_agent_child,
    sync_managed_agent_processes, BackendKind, ManagedAgentProcess,
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
    backfill_persona_snapshots_with(
        app,
        super::persona_device_view::load_device_policy_context,
        super::storage::hydrate_keys,
        super::storage::persist_agent_keys,
    )
}
/// Boot backfill uses structural records and isolated injectable secret boundaries.
fn backfill_persona_snapshots_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    context_provider: impl FnOnce(
        &tauri::AppHandle<R>,
        &AppState,
    )
        -> Result<super::persona_device_view::DevicePolicyContext, String>,
    hydrate: impl FnOnce(&mut [super::ManagedAgentRecord]),
    persist: impl FnOnce(&mut [super::ManagedAgentRecord]),
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let fence = super::device_runtime::capture_runtime_fence(&state)?;
    let raw =
        super::persona_device_view::read_policy_records(&super::managed_agents_store_path(app)?)?;
    let definition_rows: Vec<_> = raw
        .iter()
        .filter(|r| r.pubkey.is_empty())
        .cloned()
        .collect();
    let definitions = super::persona_definitions_for_policy(&definition_rows);
    let relevant: Vec<_> = raw
        .iter()
        .filter(|r| {
            r.pubkey.is_empty() || (r.persona_id.is_some() && r.persona_source_version.is_none())
        })
        .cloned()
        .collect();
    let context = if super::device_home_migration::needs_private_authority(&relevant) {
        match context_provider(app, &state) {
            Ok(c) => {
                super::device_creation::assert_creation_scope(&fence.scope, &c.scope)?;
                Some(c)
            }
            Err(e) => {
                eprintln!("buzz-desktop: private snapshot backfill skipped: {e}");
                None
            }
        }
    } else {
        None
    };
    let mut selected: Vec<_> = raw
        .into_iter()
        .filter(|r| {
            !r.pubkey.is_empty() && r.persona_id.is_some() && r.persona_source_version.is_none()
        })
        .collect();
    selected.retain(|r| {
        let definition = definitions
            .iter()
            .find(|d| r.persona_id.as_deref() == Some(d.id.as_str()));
        match (definition, context.as_ref()) {
            (Some(d), _) if d.share_across_devices == Some(true) => true,
            (Some(d), Some(c)) => {
                super::device_runtime::authorize_instance_start(r, Some(d), c).is_ok()
            }
            _ => false,
        }
    });
    super::device_runtime::assert_runtime_fence(&state, &fence)?;
    hydrate(&mut selected);
    for record in &mut selected {
        if let Some(d) = definitions
            .iter()
            .find(|d| record.persona_id.as_deref() == Some(d.id.as_str()))
        {
            super::persona_events::apply_persona_snapshot(record, d);
            record.updated_at = util::now_iso();
        }
    }
    let targets = selected.iter().map(|r| r.pubkey.clone()).collect();
    if !selected.is_empty() {
        super::storage::save_restore_records_with(app, &selected, &targets, persist)?;
    }
    Ok(())
}
/// Boot recovery must withhold owner-related snapshot effects.
pub(crate) fn run_boot_backfill_with(
    recovery_mode: bool,
    effect: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if recovery_mode {
        return Ok(());
    }
    effect()
}

/// Schedule launch restore: captures admission now, at scheduling, and
/// returns the deferred restore that runs under that snapshot. A community
/// removed after this call refuses the restore even if it is re-added before
/// the task runs. `sweeps` is Phase A's live process sweeps.
pub fn launch_restore_task<R, S>(
    app: tauri::AppHandle<R>,
    sweeps: S,
) -> impl std::future::Future<Output = Result<(), String>>
where
    R: tauri::Runtime,
    S: FnOnce(&tauri::AppHandle<R>, &[u32]),
{
    let admission = super::AdmissionSnapshot::capture(&app.state::<AppState>());
    async move {
        let state = app.state::<AppState>();
        restore_managed_agents_on_launch_with(&app, &state.shutdown_started, admission, sweeps)
            .await
    }
}

/// Phase A's sweeps of live, untracked agent processes, skipping `tracked_pids`.
pub fn live_process_sweeps(app: &tauri::AppHandle, tracked_pids: &[u32]) {
    super::sweep_orphaned_agent_processes(app, tracked_pids);

    // System-wide sweep: enumerate all user processes and kill any known
    // agent binaries not tracked by this session. Catches orphans whose
    // PID files were already cleaned up (e.g. agent workers in their own
    // process group whose parent harness exited).
    super::sweep_system_agent_processes(&super::current_instance_id(app), tracked_pids);

    // Dead-instance reaping: find agents belonging to Buzz instances
    // whose desktop process is no longer running and reap them.
    super::reap_dead_instance_agents(&super::current_instance_id(app), tracked_pids);

    // Exact-path sweep: kill any buzz-acp process whose executable path
    // matches this bundle's harness binary but is not in the tracked set.
    // Complements the env-var sweep above — catches orphans that predate
    // BUZZ_MANAGED_AGENT injection or lost their PID-file receipt.
    //
    // TODO: the three sweeps above each walk the PID table independently.
    // A future consolidation should collect a single shared process snapshot
    // at the top of this block and thread it through all sweep functions,
    // replacing the three separate kernel enumerations.
    super::sweep_untracked_bundle_harnesses(tracked_pids);
}

/// Restore managed agents that were running before the app was closed.
///
/// Split into three phases to minimise lock contention with the frontend:
///   A (under lock): sync process state, cleanup, collect agents to start
///   B (no locks):   resolve commands and spawn processes in parallel
///   C (re-lock):    write back PIDs and status to records on disk
/// Restore with the admission snapshot captured before device-home completion awaits.
pub async fn restore_managed_agents_on_launch<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    shutdown_started: &AtomicBool,
    admission: super::AdmissionSnapshot,
    sweeps: impl FnOnce(&tauri::AppHandle<R>, &[u32]),
) -> Result<(), String> {
    restore_managed_agents_on_launch_with(app, shutdown_started, admission, sweeps).await
}

async fn restore_managed_agents_on_launch_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    shutdown_started: &AtomicBool,
    admission: super::AdmissionSnapshot,
    sweeps: impl FnOnce(&tauri::AppHandle<R>, &[u32]),
) -> Result<(), String> {
    if shutdown_started.load(Ordering::SeqCst) {
        return Ok(());
    }

    let state = app.state::<AppState>();
    // `apply_workspace` still holds the apply lock for this task, so the relay
    // cannot change underneath it; pin it for the spawn loop.
    let restore_relay = crate::relay::relay_ws_url_with_override(&state);
    let restore_relay = restore_relay.as_str();

    let restore_fence = super::device_runtime::capture_runtime_fence(&state)?;
    {
        let _cleanup_transition = state
            .managed_agent_runtime_transition
            .lock()
            .map_err(|error| error.to_string())?;
        child_ownership::retry_restore_cleanup(app)?;
    }

    let agents_to_start = prepare_restore_phase_a_with(
        Some(&restore_fence),
        app,
        shutdown_started,
        super::persona_device_view::load_device_policy_context,
        super::storage::hydrate_keys,
        super::storage::persist_agent_keys,
        |records, eligible| prepare_restore_processes(app, records, eligible, sweeps),
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
        let global = super::load_global_agent_config(app).unwrap_or_default();
        let mut mesh_preflight_failures = std::collections::HashSet::new();
        for record in &agents_to_start {
            let preflight = super::device_runtime::runtime_preflight_with(
                app,
                &state,
                &record.pubkey,
                Some(&restore_fence),
                super::persona_device_view::load_device_policy_context,
                |current, definitions, _| {
                    let model = super::effective_config::resolve_effective_relay_mesh_model_id(
                        &current,
                        &definitions,
                        &global,
                    );
                    async move {
                        crate::commands::ensure_relay_mesh_for_record(app, model.as_deref(), false)
                            .await
                    }
                },
                |_, _, _| Ok(()),
            )
            .await;
            if let Err(error) = preflight {
                persist_restore_error(app, &record.pubkey, error, &restore_fence)?;
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

    let reconcile_items = spawn_and_register_restored_agents_with_fence(
        app,
        shutdown_started,
        &admission,
        restore_relay,
        &agents_to_start,
        owner_hex.as_deref(),
        &restore_fence,
    )?;

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

/// Phases B and C of launch restore: spawn `agents_to_start` on `restore_relay`
/// if it is still admitted under the snapshot `apply_workspace` scheduled the
/// restore with, then register them. Split from the sweeps above so tests can
/// drive the real admission and registration without sweeping live processes.
/// Returns the profile reconciliations to run for the agents it registered.
#[cfg(test)]
fn spawn_and_register_restored_agents<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    shutdown_started: &AtomicBool,
    admission: &super::AdmissionSnapshot,
    restore_relay: &str,
    agents_to_start: &[super::ManagedAgentRecord],
    owner_hex: Option<&str>,
) -> Result<Vec<(String, crate::commands::ProfileReconcileData)>, String> {
    let restore_fence = super::device_runtime::capture_runtime_fence(&app.state::<AppState>())?;
    spawn_and_register_restored_agents_with_fence(
        app,
        shutdown_started,
        admission,
        restore_relay,
        agents_to_start,
        owner_hex,
        &restore_fence,
    )
}

fn spawn_and_register_restored_agents_with_fence<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    shutdown_started: &AtomicBool,
    admission: &super::AdmissionSnapshot,
    restore_relay: &str,
    agents_to_start: &[super::ManagedAgentRecord],
    owner_hex: Option<&str>,
    restore_fence: &super::device_runtime::RuntimeFence,
) -> Result<Vec<(String, crate::commands::ProfileReconcileData)>, String> {
    let state = app.state::<AppState>();
    // Serialize spawning and runtime registration with shutdown cleanup. The
    // shutdown flag is rechecked after taking the lock so shutdown either
    // prevents this transition or waits until every child is tracked and can
    // be terminated.
    // ── Phase B (transition lock held): resolve commands and spawn in parallel ──
    let spawned = spawn_if_admitted(
        &state,
        shutdown_started,
        admission,
        restore_relay,
        |admitted| {
            std::thread::scope(|scope| {
                let owner_hex_ref = owner_hex;
                let restore_fence_ref = restore_fence;
                let handles: Vec<_> = agents_to_start
                    .iter()
                    .filter(|_| !shutdown_started.load(Ordering::SeqCst))
                    .map(|record| {
                        let handle =
                            scope.spawn(move || {
                                let state = app.state::<AppState>();
                                let outcome = (|| {
                        let _store = state
                            .managed_agents_store_lock
                            .lock()
                            .map_err(|e| e.to_string())?;
                        super::device_runtime::assert_runtime_fence(&state, restore_fence_ref)?;
                        super::device_runtime::restore_spawn_phase_with(
                            app,
                            &state,
                            &record.pubkey,
                            Some(&restore_fence_ref.scope),
                            super::persona_device_view::load_device_policy_context,
                            |mut current, _, scope| {
                                super::storage::hydrate_keys(std::slice::from_mut(&mut current));
                                let record = &current;
                                let workspace_relay = scope.relay_url;
                                let relay_url = crate::relay::effective_agent_relay_url(
                                    &record.relay_url,
                                    &workspace_relay,
                                );
                                let outcome = match super::ManagedAgentRuntimeKey::new(
                                    record.pubkey.clone(),
                                    &relay_url,
                                ) {
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
                                                    runtime
                                                        .child
                                                        .try_wait()
                                                        .ok()
                                                        .flatten()
                                                        .is_none()
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
                                                        admitted,
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
                                Ok(outcome)
                            },
                        )
                    })()
                    .unwrap_or_else(SpawnOutcome::Failed);
                                (record.pubkey.clone(), outcome)
                            });
                        handle
                    })
                    .collect();

                handles
                    .into_iter()
                    .map(|h| h.join().unwrap())
                    .collect::<Vec<AgentSpawnResult>>()
            })
        },
    )?;
    let Some((restore_transition, spawn_results)) = spawned else {
        return Ok(Vec::new());
    };

    if spawn_results.is_empty() {
        return Ok(Vec::new());
    }

    let reconcile_items = child_ownership::complete_restore_spawn_results_with(
        (app, Some(restore_fence)),
        spawn_results,
        super::persona_device_view::load_device_policy_context,
        super::storage::hydrate_keys,
        super::storage::persist_agent_keys,
        child_ownership::terminate_restore_child,
    )?;
    drop(restore_transition);

    Ok(reconcile_items)
}

/// Phase A production orchestration with only authority, key and process boundaries injected.
fn prepare_restore_phase_a_with<R: tauri::Runtime>(
    expected: Option<&super::device_runtime::RuntimeFence>,
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

    let fence = super::device_runtime::capture_runtime_fence(&state)?;
    if let Some(expected) = expected {
        super::device_runtime::assert_runtime_fence(&state, expected)?;
    }

    let policy_records =
        super::persona_device_view::read_policy_records(&super::managed_agents_store_path(app)?)?;
    let context = if needs_auto_start_authority(&policy_records) {
        auto_start_context_result(&policy_records, context_provider(app, &state))?
    } else {
        None
    };
    if let Some(context) = &context {
        super::device_creation::assert_creation_scope(&fence.scope, &context.scope)?;
    }
    super::device_runtime::assert_runtime_fence(&state, &fence)?;
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
    let personas_for_snapshot = super::load_personas(app)?;
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

fn prepare_restore_processes<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    records: &mut [super::ManagedAgentRecord],
    eligible: &std::collections::HashSet<String>,
    sweeps: impl FnOnce(&tauri::AppHandle<R>, &[u32]),
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
    sweeps(app, &tracked_pids);

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
    expected: Option<&super::device_runtime::RuntimeFence>,
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
    if let Some(expected) = expected {
        super::device_runtime::assert_runtime_fence(&state, expected)?;
    }
    let fence = super::device_runtime::capture_runtime_fence(&state)?;
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
    if let Some(context) = &context {
        super::device_creation::assert_creation_scope(&fence.scope, &context.scope)?;
    }
    super::device_runtime::assert_runtime_fence(&state, &fence)?;
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
fn persist_restore_error<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    pubkey: &str,
    error: String,
    expected: &super::device_runtime::RuntimeFence,
) -> Result<(), String> {
    persist_restore_error_with(
        Some(expected),
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
    expected: Option<&super::device_runtime::RuntimeFence>,
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
        expected,
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
        if let Some(reason) =
            super::device_runtime::runtime_start_refusal(record, &definitions, context)
        {
            eprintln!("buzz-desktop: runtime skipped {}: {reason}", record.pubkey);
        } else {
            selected.push(record.clone());
        }
    }
    Ok(selected)
}

/// Mixed jobs retain explicitly shared candidates when private authority is unavailable.
pub(crate) fn auto_start_context_result(
    records: &[super::ManagedAgentRecord],
    result: Result<super::persona_device_view::DevicePolicyContext, String>,
) -> Result<Option<super::persona_device_view::DevicePolicyContext>, String> {
    match result {
        Ok(context) => Ok(Some(context)),
        Err(error) => {
            let shared = select_auto_start_candidates(records, None)?;
            if shared.is_empty() {
                return Err(error);
            }
            eprintln!("buzz-desktop: private runtime candidates skipped: device_home_sync_failed: {error}");
            Ok(None)
        }
    }
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

/// Restore's check-then-spawn step. Takes the runtime transition lock and runs
/// `spawn` only if shutdown has not started and `restore_relay` is still
/// admitted under the snapshot `apply_workspace` captured when it scheduled
/// this restore; otherwise spawns nothing and returns `None`. On success the
/// lock is returned still held, so the caller registers the spawned children
/// before shutdown or a removal's stop sweep can run.
fn spawn_if_admitted<'a, T>(
    state: &'a AppState,
    shutdown_started: &AtomicBool,
    admission: &super::AdmissionSnapshot,
    restore_relay: &str,
    spawn: impl FnOnce(&super::Admitted<'_>) -> T,
) -> Result<Option<(std::sync::MutexGuard<'a, super::RelayAdmissions>, T)>, String> {
    let transition = state
        .managed_agent_runtime_transition
        .lock()
        .map_err(|error| error.to_string())?;
    if shutdown_started.load(Ordering::SeqCst) {
        return Ok(None);
    }
    let spawned = match transition.admit(admission, restore_relay) {
        Ok(admitted) => spawn(&admitted),
        Err(error) => {
            eprintln!("buzz-desktop: skipping managed agent restore: {error}");
            return Ok(None);
        }
    };
    Ok(Some((transition, spawned)))
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

#[cfg(test)]
mod runtime_backfill_tests {
    use super::*;
    use crate::managed_agents::{
        device_home_migration::tests::{app, records, write},
        persona_device_view::read_policy_records,
    };
    #[test]
    fn boot_backfill_skips_copied_private_and_preserves_shared_metadata_path() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let base = super::super::managed_agents_base_dir(app.handle()).unwrap();
        let (mut raw, _) = records();
        raw[1].device_host_binding = Some("copied".into());
        let mut d = crate::managed_agents::device_home_migration::tests::definition();
        d.id = "shared".into();
        d.share_across_devices = Some(true);
        let mut i = d.clone().into_agent_record();
        i.persona_id = Some(d.id.clone());
        i.pubkey = nostr::Keys::generate().public_key().to_hex();
        let shared_key = i.pubkey.clone();
        raw.push(d.into_agent_record());
        raw.push(i);
        write(&base, &raw);
        backfill_persona_snapshots_with(
            app.handle(),
            |_, _| Err("isolated proof unavailable".into()),
            |_| {},
            |_| {},
        )
        .unwrap();
        let after = read_policy_records(&base.join("managed-agents.json")).unwrap();
        assert!(
            after
                .iter()
                .find(|r| r.pubkey == raw[1].pubkey)
                .unwrap()
                .persona_source_version
                .is_none(),
            "copied metadata mutated at boot"
        );
        assert!(after
            .iter()
            .find(|r| r.pubkey == shared_key)
            .unwrap()
            .persona_source_version
            .is_some());
    }
    #[test]
    fn recovery_boot_has_no_backfill_effect() {
        let count = std::cell::Cell::new(0);
        run_boot_backfill_with(true, || {
            count.set(1);
            Ok(())
        })
        .unwrap();
        assert_eq!(count.get(), 0);
        run_boot_backfill_with(false, || {
            count.set(1);
            Ok(())
        })
        .unwrap();
        assert_eq!(count.get(), 1);
    }
}
#[cfg(test)]
mod mixed_restore_tests {
    use super::*;
    use crate::managed_agents::device_home_migration::tests::{app, definition, records, write};
    #[test]
    fn mixed_selected_restore_keeps_shared_when_private_authority_is_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let base = super::super::managed_agents_base_dir(app.handle()).unwrap();
        let (mut raw, _) = records();
        raw[1].start_on_app_launch = true;
        let mut d = definition();
        d.id = "shared".into();
        d.share_across_devices = Some(true);
        let mut i = d.clone().into_agent_record();
        i.persona_id = Some(d.id.clone());
        i.pubkey = nostr::Keys::generate().public_key().to_hex();
        i.start_on_app_launch = true;
        let key = i.pubkey.clone();
        raw.push(d.into_agent_record());
        raw.push(i);
        write(&base, &raw);
        let selected = prepare_restore_phase_a_with(
            None,
            app.handle(),
            &AtomicBool::new(false),
            |_, _| Err("injected proof unavailable".into()),
            |selected| {
                assert_eq!(selected.len(), 1);
                assert_eq!(selected[0].pubkey, key);
            },
            |_| {},
            |_, eligible| {
                assert_eq!(eligible.len(), 1);
                assert!(eligible.contains(&key));
                Ok((false, vec![key.clone()]))
            },
        )
        .unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].pubkey, key);
    }
}

/// Test access to production restore selection, hydration and snapshot sequencing.
#[cfg(test)]
pub(crate) fn prepare_restore_workflow_with<R: tauri::Runtime>(
    expected: Option<&super::device_runtime::RuntimeFence>,
    app: &tauri::AppHandle<R>,
    shutdown_started: &AtomicBool,
    context: impl FnOnce(
        &tauri::AppHandle<R>,
        &AppState,
    ) -> Result<super::persona_device_view::DevicePolicyContext, String>,
    hydrate: impl FnOnce(&mut [super::ManagedAgentRecord]),
    persist: impl FnOnce(&mut [super::ManagedAgentRecord]),
    processes: impl FnOnce(
        &mut [super::ManagedAgentRecord],
        &std::collections::HashSet<String>,
    ) -> Result<(bool, Vec<String>), String>,
) -> Result<Vec<super::ManagedAgentRecord>, String> {
    prepare_restore_phase_a_with(
        expected,
        app,
        shutdown_started,
        context,
        hydrate,
        persist,
        processes,
    )
}
// `build_app_state()` pulls in native Windows DLLs unavailable on the CI runner.
#[cfg(all(test, not(target_os = "windows")))]
mod launch_restore_admission_tests {
    use super::spawn_if_admitted;
    use crate::app_state::{build_app_state, AppState};
    use crate::managed_agents::AdmissionSnapshot;
    use crate::relay::{relay_api_base_url_with_override, relay_ws_url_with_override};
    use std::cell::Cell;
    use std::sync::atomic::AtomicBool;

    const RELAY: &str = "wss://removed.example";

    /// Runs restore's spawn step with `admission` and returns how many times it spawned.
    fn spawns(state: &AppState, shutdown: &AtomicBool, admission: &AdmissionSnapshot) -> u32 {
        let count = Cell::new(0);
        let gate = spawn_if_admitted(state, shutdown, admission, RELAY, |_| {
            count.set(count.get() + 1)
        });
        assert_eq!(gate.unwrap().is_some(), count.get() == 1);
        count.get()
    }

    fn remove(state: &AppState) {
        crate::managed_agents::remove_relay(state, RELAY).unwrap();
    }

    #[test]
    fn restore_scheduled_before_a_removal_spawns_nothing_even_after_readd() {
        let state = build_app_state();
        let shutdown = AtomicBool::new(false);
        *state.relay_url_override.lock().unwrap() = Some(RELAY.into());
        // What `apply_workspace` captures when it schedules the restore.
        let scheduled = AdmissionSnapshot::capture(&state);

        remove(&state);
        crate::managed_agents::readd_relay(&state, RELAY).unwrap();

        assert_eq!(spawns(&state, &shutdown, &scheduled), 0);
        assert_eq!(relay_ws_url_with_override(&state), RELAY);
        assert_eq!(
            relay_api_base_url_with_override(&state),
            "https://removed.example"
        );
        // A restore scheduled after the re-add spawns normally.
        let fresh = AdmissionSnapshot::capture(&state);
        assert_eq!(spawns(&state, &shutdown, &fresh), 1);
    }

    #[test]
    fn shutdown_spawns_nothing_even_when_admitted() {
        let state = build_app_state();
        let shutdown = AtomicBool::new(true);
        let admission = AdmissionSnapshot::capture(&state);
        assert_eq!(spawns(&state, &shutdown, &admission), 0);
    }
}

#[cfg(all(test, not(target_os = "windows")))]
#[path = "restore_admission_tests.rs"]
mod admission_entry_tests;
