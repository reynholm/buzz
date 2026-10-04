//! Read-only runtime authorization. Callers holding the store lock must not reacquire it.
use super::{
    device_home_sync::SyncScope, persona_device_view::DevicePolicyContext, AgentDefinition,
    ManagedAgentRecord,
};

/// Authorize the exact target binding, even when a sibling proves a local home.
pub(crate) fn authorize_instance_start(
    record: &ManagedAgentRecord,
    definition: Option<&AgentDefinition>,
    context: &DevicePolicyContext,
) -> Result<(), String> {
    if record.persona_id.is_none() {
        return Ok(());
    }
    let definition = definition.ok_or(super::effective_config::ORPHANED_INSTANCE_ERROR)?;
    super::device_creation::authorize_definition_action(
        context,
        definition,
        std::slice::from_ref(record),
        super::device_creation::DefinitionAction::StartInstance,
    )?;
    if definition.share_across_devices == Some(true) {
        return Ok(());
    }
    if !record
        .device_host_binding
        .as_deref()
        .is_some_and(|b| context.proof.matches(b))
    {
        return Err(format!(
            "definition_hosted_elsewhere: {}",
            definition
                .origin_device_label
                .as_deref()
                .unwrap_or("На другом устройстве")
        ));
    }
    Ok(())
}

/// Stable, per-target skip reason for batch runtime selection without secret reads.
pub(crate) fn runtime_start_refusal(
    record: &ManagedAgentRecord,
    definitions: &[AgentDefinition],
    context: Option<&DevicePolicyContext>,
) -> Option<String> {
    let id = record.persona_id.as_deref()?;
    let Some(definition) = definitions.iter().find(|d| d.id == id) else {
        return Some(super::effective_config::ORPHANED_INSTANCE_ERROR.into());
    };
    if definition.share_across_devices == Some(true) {
        return None;
    }
    match context {
        Some(context) => authorize_instance_start(record, Some(definition), context).err(),
        None => Some("device_home_sync_failed: private runtime authority unavailable".into()),
    }
}

/// Authorize a freshly loaded target. The caller holds the managed store mutex.
pub(crate) fn runtime_phase_locked_with<R: tauri::Runtime, T>(
    app: &tauri::AppHandle<R>,
    state: &crate::app_state::AppState,
    pubkey: &str,
    expected: Option<&SyncScope>,
    context: impl FnOnce(
        &tauri::AppHandle<R>,
        &crate::app_state::AppState,
    ) -> Result<DevicePolicyContext, String>,
    effect: impl FnOnce(ManagedAgentRecord, Vec<AgentDefinition>, SyncScope) -> Result<T, String>,
) -> Result<T, String> {
    let fence = capture_runtime_fence(state)?;
    let scope = fence.scope.clone();
    let raw =
        super::persona_device_view::read_policy_records(&super::managed_agents_store_path(app)?)?;
    let record = raw
        .iter()
        .find(|r| r.pubkey == pubkey)
        .cloned()
        .ok_or_else(|| format!("agent {pubkey} not found"))?;
    let definition_rows: Vec<_> = raw
        .iter()
        .filter(|r| r.pubkey.is_empty())
        .cloned()
        .collect();
    let definitions = super::persona_definitions_for_policy(&definition_rows);
    if let Some(expected) = expected {
        super::device_creation::assert_creation_scope(&scope, expected)?;
    }
    let definition = record
        .persona_id
        .as_deref()
        .map(|id| {
            definitions
                .iter()
                .find(|d| d.id == id)
                .ok_or(super::effective_config::ORPHANED_INSTANCE_ERROR)
        })
        .transpose()?;
    if definition.is_some_and(|d| d.share_across_devices != Some(true)) {
        let context = context(app, state).map_err(|e| format!("device_home_sync_failed: {e}"))?;
        super::device_creation::assert_creation_scope(&scope, &context.scope)?;
        authorize_instance_start(&record, definition, &context)?;
    }
    assert_runtime_fence(state, &fence)?;
    effect(record, definitions, scope)
}

/// Owning process-launch seam; authority is resolved before log/process effects.
pub(crate) fn spawn_child_phase_with<R: tauri::Runtime, T>(
    app: &tauri::AppHandle<R>,
    state: &crate::app_state::AppState,
    pubkey: &str,
    expected: Option<&SyncScope>,
    context: impl FnOnce(
        &tauri::AppHandle<R>,
        &crate::app_state::AppState,
    ) -> Result<DevicePolicyContext, String>,
    effect: impl FnOnce(ManagedAgentRecord, Vec<AgentDefinition>, SyncScope) -> Result<T, String>,
) -> Result<T, String> {
    runtime_phase_locked_with(app, state, pubkey, expected, context, effect)
}
/// Owning start/restart seam; protects reuse and receipt termination.
pub(crate) fn start_pair_phase_with<R: tauri::Runtime, T>(
    app: &tauri::AppHandle<R>,
    state: &crate::app_state::AppState,
    pubkey: &str,
    expected: Option<&SyncScope>,
    context: impl FnOnce(
        &tauri::AppHandle<R>,
        &crate::app_state::AppState,
    ) -> Result<DevicePolicyContext, String>,
    effect: impl FnOnce(ManagedAgentRecord, Vec<AgentDefinition>, SyncScope) -> Result<T, String>,
) -> Result<T, String> {
    runtime_phase_locked_with(app, state, pubkey, expected, context, effect)
}
/// Restore's final start seam; protects adoption, termination and launch.
pub(crate) fn restore_spawn_phase_with<R: tauri::Runtime, T>(
    app: &tauri::AppHandle<R>,
    state: &crate::app_state::AppState,
    pubkey: &str,
    expected: Option<&SyncScope>,
    context: impl FnOnce(
        &tauri::AppHandle<R>,
        &crate::app_state::AppState,
    ) -> Result<DevicePolicyContext, String>,
    effect: impl FnOnce(ManagedAgentRecord, Vec<AgentDefinition>, SyncScope) -> Result<T, String>,
) -> Result<T, String> {
    runtime_phase_locked_with(app, state, pubkey, expected, context, effect)
}
/// Provider's final deployment seam, inside its serialized invocation phase.
pub(crate) fn provider_phase_with<R: tauri::Runtime, T>(
    app: &tauri::AppHandle<R>,
    state: &crate::app_state::AppState,
    pubkey: &str,
    expected: Option<&SyncScope>,
    context: impl FnOnce(
        &tauri::AppHandle<R>,
        &crate::app_state::AppState,
    ) -> Result<DevicePolicyContext, String>,
    effect: impl FnOnce(ManagedAgentRecord, Vec<AgentDefinition>, SyncScope) -> Result<T, String>,
) -> Result<T, String> {
    runtime_phase_locked_with(app, state, pubkey, expected, context, effect)
}
/// Access-policy inbound seam; receiving an owner head remains unrestricted.
pub(crate) fn inbound_refresh_phase_with<R: tauri::Runtime, T>(
    app: &tauri::AppHandle<R>,
    state: &crate::app_state::AppState,
    pubkey: &str,
    expected: Option<&SyncScope>,
    context: impl FnOnce(
        &tauri::AppHandle<R>,
        &crate::app_state::AppState,
    ) -> Result<DevicePolicyContext, String>,
    effect: impl FnOnce(ManagedAgentRecord, Vec<AgentDefinition>, SyncScope) -> Result<T, String>,
) -> Result<Option<T>, String> {
    match runtime_phase_locked_with(app, state, pubkey, expected, context, |r, d, s| {
        Ok(effect(r, d, s))
    }) {
        Ok(result) => result.map(Some),
        Err(error) => {
            eprintln!("buzz-desktop: inbound runtime refresh skipped for {pubkey}: {error}");
            Ok(None)
        }
    }
}

/// Keep untouched copied records and inline secrets byte-equivalent during a target update.
pub(crate) fn save_runtime_record<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    record: &ManagedAgentRecord,
) -> Result<(), String> {
    super::storage::save_restore_records_with(
        app,
        std::slice::from_ref(record),
        &[record.pubkey.clone()].into_iter().collect(),
        super::storage::persist_agent_keys,
    )
}
/// Runtime continuation fence includes backend subscription replacement, even on one workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RuntimeFence {
    pub scope: SyncScope,
    sync_generation: u64,
}
/// Capture owner, relay, workspace and subscription generation under the store mutex.
pub(crate) fn capture_runtime_fence(
    state: &crate::app_state::AppState,
) -> Result<RuntimeFence, String> {
    Ok(RuntimeFence {
        scope: super::device_home_sync::capture_scope(state)?,
        sync_generation: super::device_home_sync::runtime_generation_locked(state)?,
    })
}
/// Reject a continuation whose original runtime scope has been replaced.
pub(crate) fn assert_runtime_fence(
    state: &crate::app_state::AppState,
    expected: &RuntimeFence,
) -> Result<(), String> {
    if capture_runtime_fence(state)? != *expected {
        return Err("device_home_sync_stale_session".into());
    }
    Ok(())
}
/// Native preflight orchestration: no store mutex crosses the asynchronous probe.
pub(crate) async fn runtime_preflight_with<R, T, F, Fut>(
    app: &tauri::AppHandle<R>,
    state: &crate::app_state::AppState,
    pubkey: &str,
    expected: Option<&RuntimeFence>,
    mut context: impl FnMut(
        &tauri::AppHandle<R>,
        &crate::app_state::AppState,
    ) -> Result<DevicePolicyContext, String>,
    preflight: F,
    effect: impl FnOnce(ManagedAgentRecord, Vec<AgentDefinition>, SyncScope) -> Result<T, String>,
) -> Result<T, String>
where
    R: tauri::Runtime,
    F: FnOnce(ManagedAgentRecord, Vec<AgentDefinition>, SyncScope) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    let (record, definitions, scope, fence) = {
        let _store = state
            .managed_agents_store_lock
            .lock()
            .map_err(|e| e.to_string())?;
        let fence = capture_runtime_fence(state)?;
        if let Some(expected) = expected {
            assert_runtime_fence(state, expected)?;
        }
        runtime_phase_locked_with(app, state, pubkey, None, &mut context, |r, d, s| {
            Ok((r, d, s, fence))
        })?
    };
    preflight(record.clone(), definitions.clone(), scope.clone()).await?;
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    assert_runtime_fence(state, &fence)?;
    runtime_phase_locked_with(app, state, pubkey, Some(&scope), context, effect)
}
