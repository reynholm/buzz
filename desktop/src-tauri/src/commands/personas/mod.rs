use tauri::AppHandle;

use crate::{
    app_state::AppState,
    managed_agents::{
        current_instance_id, delete_agent_key, load_managed_agents, load_personas, load_teams,
        save_personas, stop_managed_agent_process, sync_managed_agent_processes,
        try_regenerate_nest, validate_persona_activation_change, validate_persona_deletion,
        AgentDefinition, ManagedAgentRecord,
    },
    util::now_iso,
};

fn trim_required(value: &str, label: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{label} is required"));
    }
    Ok(trimmed.to_string())
}

fn trim_optional(value: Option<String>) -> Option<String> {
    value.and_then(|candidate| {
        let trimmed = candidate.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

/// Validate the raw authored bytes before applying storage normalization.
/// This ordering is security-relevant: prohibited edge characters must be
/// rejected, never made invisible by trimming.
fn normalize_description(value: Option<String>) -> Result<Option<String>, String> {
    crate::managed_agents::validate_agent_description_text(value.as_deref())?;
    Ok(trim_optional(value))
}

#[cfg(test)]
mod description_normalization_tests {
    use super::normalize_description;

    #[test]
    fn trims_visible_whitespace_and_collapses_blank_to_none() {
        assert_eq!(
            normalize_description(Some("  A careful agent.  ".to_string())).unwrap(),
            Some("A careful agent.".to_string())
        );
        assert_eq!(
            normalize_description(Some("   ".to_string())).unwrap(),
            None
        );
    }

    #[test]
    fn rejects_prohibited_characters_at_the_edges_before_trimming() {
        for value in ["\nA careful agent.", "A careful agent.\n", "\u{feff}Agent"] {
            assert!(normalize_description(Some(value.to_string())).is_err());
        }
    }
}

mod pending;
pub(in crate::commands) use pending::retain_persona_pending;
pub(in crate::commands) use pending::retain_persona_pending_at;
pub(crate) use pending::tombstone_persona_at;
pub(super) use pending::tombstone_persona_pending;
mod create;
pub use create::create_persona;
mod sharing;
pub use sharing::set_persona_shared;
pub use sharing::update_persona_and_publish;
mod update;
pub use update::update_persona;
mod inbound;
pub use inbound::reconcile_inbound_persona_event;
#[cfg(test)]
pub(crate) use inbound::retain_inbound_catalog_witness;

#[tauri::command]
pub async fn list_personas(
    app: AppHandle,
) -> Result<Vec<crate::managed_agents::persona_device_view::PersonaDeviceView>, String> {
    tokio::task::spawn_blocking(move || list_personas_inner(&app))
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

fn list_personas_inner<R: tauri::Runtime>(
    app: &AppHandle<R>,
) -> Result<Vec<crate::managed_agents::persona_device_view::PersonaDeviceView>, String> {
    use tauri::Manager;
    let state = app.state::<AppState>();
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;
    use crate::managed_agents::{
        device_home_sync, persona_definitions_for_policy,
        persona_device_view::{self, PersonaDeviceView},
        retention::scoped_retention_db_path,
    };
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("device data directory: {e}"))?;
    let records =
        persona_device_view::read_policy_records(&directory.join("agents/managed-agents.json"))?;
    let mut personas = persona_definitions_for_policy(&records);
    // Definitions stay visible even when owner/device authority cannot be read.
    // Scope/catalog errors remain explicit and never authorize signing.
    let context = device_home_sync::capture_scope(&state).and_then(|scope| {
        let retention_path = scoped_retention_db_path(
            &directory.join("agents"),
            &scope.relay_url,
            &scope.owner_pubkey,
        );
        pending::project_persona_sharing_read_only(
            &retention_path,
            &scope.owner_pubkey,
            &mut personas,
        )?;
        let context = persona_device_view::load_device_policy_context(app, &state)?;
        if context.scope != scope || scope != device_home_sync::capture_scope(&state)? {
            return Err("device_home_sync_stale_scope".into());
        }
        Ok(context)
    });
    let instances: Vec<_> = records
        .into_iter()
        .filter(|record| !record.pubkey.is_empty())
        .collect();
    Ok(personas
        .into_iter()
        .map(|definition| match &context {
            Ok(context) => context.project(definition, &instances),
            Err(error) => PersonaDeviceView::unavailable(definition, error.clone()),
        })
        .collect())
}

#[cfg(test)]
mod device_view_tests;

#[cfg(test)]
mod delete_cascade_tests;

/// Return pubkeys of every managed agent whose definition is the given persona.
///
/// Pure helper used by `delete_persona` to determine which agent records to
/// cascade-delete. Extracted so the filtering logic can be unit-tested without
/// a full Tauri `AppHandle`.
fn collect_cascade_pubkeys(agents: &[ManagedAgentRecord], persona_id: &str) -> Vec<String> {
    agents
        .iter()
        .filter(|a| a.persona_id.as_deref() == Some(persona_id))
        .map(|a| a.pubkey.clone())
        .collect()
}

/// Names of cascade agents that are provider-deployed: non-local backend with
/// a live `backend_agent_id`.
///
/// Pure helper used by `delete_persona`'s pre-flight: the cascade is refused
/// while any exist, because deleting the local record would orphan the remote
/// deployment. Mirrors `delete_managed_agent`'s `force_remote_delete` guard.
fn collect_remote_deployed(
    agents: &[ManagedAgentRecord],
    cascade: &std::collections::HashSet<String>,
) -> Vec<String> {
    agents
        .iter()
        .filter(|a| {
            cascade.contains(&a.pubkey)
                && a.backend != crate::managed_agents::BackendKind::Local
                && a.backend_agent_id.is_some()
        })
        .map(|a| a.name.clone())
        .collect()
}

/// Remove cascade agents from `agents` and persist via the injectable `save`.
///
/// Extracted from `delete_persona` so unit tests can inject a failing save and
/// verify retry-safety without a full `AppHandle` mock: if `save` returns `Err`,
/// this function propagates it before the keyring deletions and tombstones that
/// appear after the `?` in the call site — nothing is destroyed and the command
/// is safe to retry.
fn commit_cascade_agents(
    agents: &mut Vec<ManagedAgentRecord>,
    cascade: &std::collections::HashSet<String>,
    save: impl FnOnce(&[ManagedAgentRecord]) -> Result<(), String>,
) -> Result<(), String> {
    agents.retain(|a| !cascade.contains(&a.pubkey));
    save(agents)
}

/// Local definition deletion's native adapter, before cascade/key/signing effects.
fn delete_persona_phase_with<R: tauri::Runtime, T>(
    app: &AppHandle<R>,
    state: &AppState,
    id: &str,
    context: impl FnOnce(
        &AppHandle<R>,
        &AppState,
    ) -> Result<
        crate::managed_agents::persona_device_view::DevicePolicyContext,
        String,
    >,
    effect: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    crate::managed_agents::device_authority::definition_delete_phase_locked_with(
        app, state, id, context, effect,
    )
}

#[tauri::command]
pub async fn delete_persona(id: String, app: AppHandle) -> Result<(), String> {
    use tauri::Manager;
    tokio::task::spawn_blocking(move || {
        let state = app.state::<AppState>();

        {
            // Store lock held across all three phases.
            // Lock ordering: store lock (acquired here) → process lock (per-agent in Phase 2).
            let _store_guard = state
                .managed_agents_store_lock
                .lock()
                .map_err(|error| error.to_string())?;

            delete_persona_phase_with(
                &app,
                &state,
                &id,
                crate::managed_agents::persona_device_view::load_device_policy_context,
                || {
                    // Load and validate the persona before any destructive work.
                    let mut personas = load_personas(&app)?;
                    let persona = personas
                        .iter()
                        .find(|record| record.id == id)
                        .ok_or_else(|| format!("persona {id} not found"))?;
                    let referenced_by_team = load_teams(&app)?.iter().any(|team| {
                        team.persona_ids
                            .iter()
                            .any(|persona_id| persona_id == id.as_str())
                    });
                    validate_persona_deletion(persona, referenced_by_team)?;
                    // Capture the coordinate before the record might leave the list. Only
                    // reached for non-builtin, non-team personas (both rejected above),
                    // so every deleted persona here is one this owner published.
                    let d_tag = crate::managed_agents::persona_events::persona_d_tag(persona);

                    // ── Phase 1: Stage ─────────────────────────────────────────────
                    //
                    // Load agents, sync process state, and build the cascade set. Lock
                    // ordering: store lock (held) → process lock (acquired for sync,
                    // then released before Phase 2 stops). Every fallible read/lock is
                    // here; an error leaves all state intact and the command is retryable.
                    let mut agents = crate::managed_agents::persona_device_view::read_policy_records(
                        &crate::managed_agents::managed_agents_store_path(&app)?,
                    )?;
                    let cascade: std::collections::HashSet<String> = collect_cascade_pubkeys(&agents, &id).into_iter().collect();
                    {
                        let mut runtimes = state
                            .managed_agent_processes
                            .lock()
                            .map_err(|error| error.to_string())?;
                        let (sync_changed, exited_pubkeys) = crate::managed_agents::device_authority::selected_deletion_records_with(
                            &mut agents,
                            &cascade,
                            |selected| Ok(sync_managed_agent_processes(selected, &mut runtimes, &current_instance_id(&app))),
                        )?;
                        if sync_changed {
                            crate::managed_agents::device_authority::save_deletion_snapshot(&app, &agents)?;
                        }
                        for pk in &exited_pubkeys {
                            state.clear_agent_session_caches(pk);
                        }
                        // runtimes drops here (process lock released before Phase 2).
                    }

                    // Remote-agent pre-flight: refuse the cascade before any destructive
                    // work while any target is provider-deployed. Nothing in
                    // create_managed_agent forbids a persona-linked provider agent, so
                    // this must be a runtime guard, not an assumed invariant.
                    let remote_deployed = collect_remote_deployed(&agents, &cascade);
                    if !remote_deployed.is_empty() {
                        return Err(format!(
                            "persona {id} has provider-deployed agent instances ({}); delete those agent instances first",
                            remote_deployed.join(", ")
                        ));
                    }

                    let deletion_authorities: Vec<_> = cascade.iter().map(|pk| {
                        crate::managed_agents::device_authority::prepare_deletion_authority_locked_with(
                            &app,
                            &state,
                            pk,
                            crate::managed_agents::persona_device_view::load_device_policy_context,
                        )
                    }).collect::<Result<_, _>>()?;
                    let home_operations: Vec<_> = deletion_authorities.iter()
                        .map(|permit| {
                            crate::managed_agents::device_home_operations::delete::prepare_home_delete_authorized_locked(
                                &app, permit,
                            )
                        })
                        .collect::<Result<_, _>>()?;
                    // ── Phase 2: Stop ───────────────────────────────────────────────
                    //
                    // Best-effort stop each running cascade instance. Lock ordering:
                    // store lock (held) → process lock acquired per-agent and released
                    // between stops so the process lock is not held across the full poll
                    // cycle (stop_managed_agent_process polls 100ms×10 before SIGKILL).
                    //
                    // Per-agent stop errors are swallowed — these records are deleted in
                    // Phase 3 regardless. Intentional difference from delete_managed_agent
                    // (single-agent, fatal on stop failure); here the cascade is multi-agent
                    // and deletion must proceed even if one instance cannot be stopped.
                    for pk in &cascade {
                        if let Some(rec) = agents.iter_mut().find(|a| a.pubkey == *pk) {
                            let mut runtimes = state
                                .managed_agent_processes
                                .lock()
                                .map_err(|error| error.to_string())?;
                            if let Err(e) = stop_managed_agent_process(&app, rec, &mut runtimes) {
                                eprintln!("buzz-desktop: delete_persona: failed to stop agent {pk}: {e}");
                            }
                            // runtimes drops here (per-agent, process lock not held across stops).
                        }
                    }

                    // ── Phase 3: Commit ─────────────────────────────────────────────
                    //
                    // Disk-authoritative writes first, side effects strictly after.
                    // commit_cascade_agents is an injectable seam so unit tests can
                    // verify retry-safety: a failing save propagates before any keyring
                    // deletion or tombstone occurs.
                    //
                    // Failure semantics:
                    //   agent save fails   → nothing destroyed; full cascade retries cleanly
                    //   persona save fails → cascade agents gone, persona survives; a retry
                    //                        finds an empty cascade and proceeds cleanly
                    // Keys and tombstones are enqueued only after their records leave disk.
                    if !cascade.is_empty() {
                        commit_cascade_agents(&mut agents, &cascade, |recs| {
                            crate::managed_agents::device_authority::save_deletion_snapshot(&app,recs)
                })?;
            }

            let original_len = personas.len();
            personas.retain(|record| record.id != id);
            if personas.len() == original_len {
                return Err(format!("persona {id} not found"));
            }
            save_personas(&app, &personas)?;

            // Side effects — strictly after records leave disk.
            for pk in &cascade {
                state.clear_agent_session_caches(pk);
                // Remove nsec from keyring after the record is gone.
                delete_agent_key(pk);
                // Tombstone + NIP-IA kind:9035 archive enqueue atomically; the
                // archive's `persona_id` is derived from the retained 30177 head.

            }
            complete_cascade_home_operations(&app, &home_operations)?;
            tombstone_persona_pending(&app, &state, &d_tag);

            Ok(())
            })?;

            // _store_guard drops here, before try_regenerate_nest.
        }

        try_regenerate_nest(&app);

        Ok(())
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

#[tauri::command]
pub async fn set_persona_active(
    id: String,
    active: bool,
    app: AppHandle,
) -> Result<AgentDefinition, String> {
    use tauri::Manager;
    tokio::task::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _store_guard = state
            .managed_agents_store_lock
            .lock()
            .map_err(|error| error.to_string())?;
        let mut personas = load_personas(&app)?;
        let persona = personas
            .iter_mut()
            .find(|record| record.id == id)
            .ok_or_else(|| format!("agent {id} not found"))?;

        let referenced_by_managed_agent = !active
            && load_managed_agents(&app)?
                .iter()
                .any(|agent| agent.persona_id.as_deref() == Some(id.as_str()));
        let referenced_by_team = !active
            && load_teams(&app)?.iter().any(|team| {
                team.persona_ids
                    .iter()
                    .any(|persona_id| persona_id == id.as_str())
            });

        validate_persona_activation_change(
            persona,
            active,
            referenced_by_managed_agent,
            referenced_by_team,
        )?;

        if persona.is_active == active {
            return Ok(persona.clone());
        }

        persona.is_active = active;
        persona.updated_at = now_iso();

        let updated = persona.clone();
        save_personas(&app, &personas)?;
        try_regenerate_nest(&app);
        Ok(updated)
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

pub(crate) const PNG_MAGIC: [u8; 4] = [0x89, 0x50, 0x4E, 0x47];
mod card;
mod snapshot;
pub use card::*;
#[cfg(test)]
pub(crate) use snapshot::import::decode_snapshot_from_bytes;
pub(crate) use snapshot::import::{
    parse_snapshot_payload_from_bytes, resolve_snapshot_import_behavior, MAX_SNAPSHOT_JSON_BYTES,
    MAX_SNAPSHOT_PNG_BYTES,
};
pub use snapshot::{confirm_agent_snapshot_import, preview_agent_snapshot_import};
pub use snapshot::{encode_agent_snapshot_for_send, export_agent_snapshot};

/// Complete pre-removal instance intents after the definition leaves the unified snapshot.
fn complete_cascade_home_operations<R: tauri::Runtime>(
    app: &AppHandle<R>,
    operations: &[crate::managed_agents::device_home_operations::HomeOperation],
) -> Result<(), String> {
    for operation in operations {
        crate::managed_agents::device_home_operations::delete::finish_home_delete_locked(
            app, operation,
        )?;
    }
    Ok(())
}
#[cfg(test)]
mod home_delete_tests;
