//! Scoped local runtime preflight, shared by manual and automatic restarts.
use super::*;
use crate::managed_agents::device_runtime::{self, RuntimeFence};

async fn preflight<R: tauri::Runtime>(
    app: &AppHandle<R>,
    record: ManagedAgentRecord,
    definitions: Vec<crate::managed_agents::AgentDefinition>,
    fresh: bool,
) -> Result<(), String> {
    if record.backend != BackendKind::Local {
        return Err(format!("agent {} is not a local agent", record.pubkey));
    }
    let global = crate::managed_agents::load_global_agent_config(app).unwrap_or_default();
    let model = crate::managed_agents::effective_config::resolve_effective_relay_mesh_model_id(
        &record,
        &definitions,
        &global,
    );
    ensure_relay_mesh_for_record(app, model.as_deref(), fresh).await
}
fn snapshot(
    record: &mut ManagedAgentRecord,
    definitions: &[crate::managed_agents::AgentDefinition],
) -> Result<(), String> {
    crate::managed_agents::storage::hydrate_keys(std::slice::from_mut(record));
    if let Some(id) = record.persona_id.as_deref() {
        let definition = definitions
            .iter()
            .find(|d| d.id == id)
            .ok_or(crate::managed_agents::effective_config::ORPHANED_INSTANCE_ERROR)?;
        crate::managed_agents::persona_events::apply_persona_snapshot(record, definition);
        record.updated_at = now_iso();
    }
    Ok(())
}
/// Restart pairs with admission captured before the caller stops them.
pub(in crate::commands) async fn start_local_agent_pairs_with_preflight(
    app: &AppHandle,
    state: &AppState,
    pubkey: &str,
    relay_urls: &[String],
    admission: &crate::managed_agents::AdmissionSnapshot,
) -> Result<ManagedAgentSummary, String> {
    start_local_agent_pairs_impl(app, state, pubkey, relay_urls, None, admission).await
}
/// Restart pairs only within the caller's original runtime scope.
pub(in crate::commands) async fn start_local_agent_pairs_scoped(
    app: &AppHandle,
    state: &AppState,
    pubkey: &str,
    relay_urls: &[String],
    expected: Option<&RuntimeFence>,
    admission: &crate::managed_agents::AdmissionSnapshot,
) -> Result<ManagedAgentSummary, String> {
    match expected {
        Some(expected) => {
            start_local_agent_pairs_impl(app, state, pubkey, relay_urls, Some(expected), admission)
                .await
        }
        None => {
            start_local_agent_pairs_with_preflight(app, state, pubkey, relay_urls, admission).await
        }
    }
}
async fn start_local_agent_pairs_impl(
    app: &AppHandle,
    state: &AppState,
    pubkey: &str,
    relay_urls: &[String],
    expected: Option<&RuntimeFence>,
    admission: &crate::managed_agents::AdmissionSnapshot,
) -> Result<ManagedAgentSummary, String> {
    let fence = device_runtime::runtime_preflight_with(
        app,
        state,
        pubkey,
        expected,
        crate::managed_agents::persona_device_view::load_device_policy_context,
        |r, d, _| preflight(app, r, d, false),
        |mut record, definitions, _| {
            snapshot(&mut record, &definitions)?;
            device_runtime::save_runtime_record(app, &record)?;
            retain_managed_agent_pending(app, state, &record);
            device_runtime::capture_runtime_fence(state)
        },
    )
    .await?;
    let mut errors = Vec::new();
    for relay in relay_urls {
        if let Err(error) = crate::managed_agents::runtime_commands::start_managed_agent_pair_scoped(
            pubkey.to_string(),
            relay.clone(),
            admission,
            app.clone(),
            &fence,
        ) {
            if error != crate::managed_agents::RELAY_REMOVED_ERROR {
                errors.push(format!("{relay}: {error}"));
            }
        }
    }
    if !errors.is_empty() {
        return Err(format!(
            "failed to restart one or more managed-agent runtime pairs: {}",
            errors.join("; ")
        ));
    }
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    device_runtime::assert_runtime_fence(state, &fence)?;
    device_runtime::runtime_phase_locked_with(
        app,
        state,
        pubkey,
        Some(&fence.scope),
        crate::managed_agents::persona_device_view::load_device_policy_context,
        |record, _, _| {
            let runtimes = state
                .managed_agent_processes
                .lock()
                .map_err(|e| e.to_string())?;
            summarize_from_disk(app, &record, &runtimes)
        },
    )
}
/// Caller expectations carried across local preflight.
pub(in crate::commands) struct LocalStartScope<'a> {
    pub relay: Option<&'a str>,
    pub owner: Option<&'a str>,
    pub replay_floor: Option<u64>,
    pub fence: Option<&'a RuntimeFence>,
}
/// Preflight and start one authorized local target with fresh policy and scope.
pub(in crate::commands) async fn start_local_agent_with_preflight<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    pubkey: &str,
    fresh: bool,
    requested: LocalStartScope<'_>,
) -> Result<ManagedAgentSummary, String> {
    start_local_agent_after_preflight(app, state, pubkey, requested, |model| async move {
        ensure_relay_mesh_for_record(app, model.as_deref(), fresh).await
    })
    .await
}

/// Supply the ordinary start's awaited probe so tests can remove a relay while
/// it is pending. Device authority and admission remain the production checks.
pub(in crate::commands) async fn start_local_agent_after_preflight<R, P, F>(
    app: &AppHandle<R>,
    state: &AppState,
    pubkey: &str,
    requested: LocalStartScope<'_>,
    preflight: P,
) -> Result<ManagedAgentSummary, String>
where
    R: tauri::Runtime,
    P: FnOnce(Option<String>) -> F,
    F: std::future::Future<Output = Result<(), String>>,
{
    let LocalStartScope {
        relay: expected_relay,
        owner: expected_owner,
        replay_floor,
        fence: expected,
    } = requested;
    // Capture before the mesh await. Remove-and-readd must also refuse this
    // in-flight start, while a later user action can capture fresh admission.
    let admission = crate::managed_agents::AdmissionSnapshot::capture(state);
    crate::relay::assert_expected_relay_scope(expected_relay, &relay_ws_url_with_override(state))?;
    crate::relay::assert_expected_signer(expected_owner, &workspace_owner_hex(state)?)?;

    let (record, definitions, scope, fence) = {
        let _store = state
            .managed_agents_store_lock
            .lock()
            .map_err(|e| e.to_string())?;
        let fence = device_runtime::capture_runtime_fence(state)?;
        if let Some(expected) = expected {
            device_runtime::assert_runtime_fence(state, expected)?;
        }
        device_runtime::runtime_phase_locked_with(
            app,
            state,
            pubkey,
            None,
            crate::managed_agents::persona_device_view::load_device_policy_context,
            |record, definitions, scope| Ok((record, definitions, scope, fence)),
        )?
    };
    if record.backend != BackendKind::Local {
        return Err(format!("agent {pubkey} is not a local agent"));
    }
    let global = crate::managed_agents::load_global_agent_config(app).unwrap_or_default();
    let model = crate::managed_agents::effective_config::resolve_effective_relay_mesh_model_id(
        &record,
        &definitions,
        &global,
    );
    preflight(model).await?;

    // Match pair-start's lock order: transition, store, runtimes. Hold admission
    // until registration so removal's subsequent stop sweep sees this pair.
    let transition = state
        .managed_agent_runtime_transition
        .lock()
        .map_err(|e| e.to_string())?;
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    device_runtime::assert_runtime_fence(state, &fence)?;
    device_runtime::runtime_phase_locked_with(
        app,
        state,
        pubkey,
        Some(&scope),
        crate::managed_agents::persona_device_view::load_device_policy_context,
        |mut record, definitions, scope| {
            if record.backend != BackendKind::Local {
                return Err(format!("agent {pubkey} is no longer a local agent"));
            }
            snapshot(&mut record, &definitions)?;
            let relay = crate::relay::bind_expected_relay_scope(
                Some(&scope.relay_url),
                scope.relay_url.clone(),
            )?;
            // Use the same target resolution as process launch. Legacy record
            // relay values do not override the current workspace after cutover.
            let runtime_relay =
                crate::relay::effective_agent_relay_url(&record.relay_url, relay.as_str());
            let admitted = transition.admit(&admission, &runtime_relay)?;
            let mut runtimes = state
                .managed_agent_processes
                .lock()
                .map_err(|e| e.to_string())?;
            let owner = crate::relay::bind_expected_signer(
                Some(&scope.owner_pubkey),
                scope.owner_pubkey.clone(),
            )?;
            start_managed_agent_process(
                app,
                &mut record,
                &mut runtimes,
                Some(owner.as_str()),
                &relay,
                &admitted,
                replay_floor,
            )?;
            device_runtime::save_runtime_record(app, &record)?;
            retain_managed_agent_pending(app, state, &record);
            summarize_from_disk(app, &record, &runtimes)
        },
    )
}
