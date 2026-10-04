//! Device-policy boundaries for local definition and instance creation.
use super::{
    device_home_sync::SyncScope, persona_device_view::DevicePolicyContext, AgentDefinition,
    ManagedAgentRecord,
};
use crate::device_identity::{DeviceIdentity, HostProof};

/// Definition action governed by the common home policy.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DefinitionAction {
    CreateInstance,
    StartInstance,
    DeleteDefinition,
}
/// Give a newly created local definition explicit policy and local lineage.
pub(crate) fn stamp_new_definition(
    definition: &mut AgentDefinition,
    requested_share: Option<bool>,
    device: &DeviceIdentity,
) {
    definition.share_across_devices = Some(requested_share.unwrap_or(false));
    definition.origin_device_id = Some(device.device_id.clone());
    definition.origin_device_label = Some(device.label.clone());
    definition.origin_released = Some(false);
}
/// Bind a new instance using backend-only verified authority.
pub(crate) fn bind_new_instance(record: &mut ManagedAgentRecord, proof: &HostProof) {
    record.device_host_binding = Some(proof.binding().to_string());
}
/// Enforce the same policy used to project frontend capabilities.
pub(crate) fn authorize_definition_action(
    context: &DevicePolicyContext,
    definition: &AgentDefinition,
    instances: &[ManagedAgentRecord],
    action: DefinitionAction,
) -> Result<(), String> {
    let view = context.project(definition.clone(), instances);
    let allowed = [
        (
            DefinitionAction::CreateInstance,
            view.capabilities.can_create_instance,
        ),
        (
            DefinitionAction::StartInstance,
            view.capabilities.can_create_instance,
        ),
        (
            DefinitionAction::DeleteDefinition,
            view.capabilities.can_delete_definition,
        ),
    ]
    .into_iter()
    .any(|(candidate, allowed)| candidate == action && allowed);
    if allowed {
        return Ok(());
    }
    Err(format!(
        "{}: {}",
        view.capabilities
            .blocked_reason
            .as_deref()
            .unwrap_or("device_home_sync_failed"),
        view.home
            .and_then(|h| h.label)
            .unwrap_or_else(|| "On another device".into())
    ))
}
/// Authorize before executing any hydration, process, mint or persistence effect.
/// Explicit sharing bypasses the authority read; scope remains pinned for all actions.
pub(crate) fn creation_phase<T>(
    scope: &SyncScope,
    captured: Option<&SyncScope>,
    definition: Option<&AgentDefinition>,
    records: &[ManagedAgentRecord],
    context: impl FnOnce() -> Result<DevicePolicyContext, String>,
    effect: impl FnOnce(Option<DevicePolicyContext>) -> Result<T, String>,
) -> Result<T, String> {
    if let Some(captured) = captured {
        assert_creation_scope(scope, captured)?;
    }
    let context = match definition {
        Some(d) if d.share_across_devices != Some(true) => {
            let context = context()?;
            if context.scope != *scope {
                return Err("device_home_sync_stale_session".into());
            }
            authorize_definition_action(&context, d, records, DefinitionAction::CreateInstance)?;
            Some(context)
        }
        _ => None,
    };
    effect(context)
}
/// Fence every creation source against owner, community and workspace changes.
pub(crate) fn assert_creation_scope(
    current: &SyncScope,
    expected: &SyncScope,
) -> Result<(), String> {
    if current != expected {
        return Err("device_home_sync_stale_session".into());
    }
    Ok(())
}
/// Native command adapter: raw read and authorization precede the entire effect closure.
/// Caller holds the store lock. The injectable authority boundary never changes sequencing.
pub(crate) fn creation_phase_locked<R: tauri::Runtime, T>(
    app: &tauri::AppHandle<R>,
    state: &crate::app_state::AppState,
    persona_id: Option<&str>,
    captured: Option<&SyncScope>,
    context: impl FnOnce(
        &tauri::AppHandle<R>,
        &crate::app_state::AppState,
    ) -> Result<DevicePolicyContext, String>,
    effect: impl FnOnce(Option<DevicePolicyContext>) -> Result<T, String>,
) -> Result<T, String> {
    let scope = super::device_home_sync::capture_scope(state)?;
    let records =
        super::persona_device_view::read_policy_records(&super::managed_agents_store_path(app)?)?;
    let definitions = super::persona_definitions_for_policy(&records);
    let definition = persona_id
        .map(|id| {
            definitions
                .iter()
                .find(|d| d.id == id)
                .ok_or_else(|| format!("agent {id} not found"))
        })
        .transpose()?;
    if let Some(d) = definition {
        super::ensure_persona_is_active(&definitions, &d.id)?;
    }
    creation_phase(
        &scope,
        captured,
        definition,
        &records,
        || context(app, state),
        effect,
    )
}
/// Load current public identity only: new definitions never require host proof.
pub(crate) fn local_device<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<DeviceIdentity, String> {
    use tauri::Manager;
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("device.json");
    public_device_at(&path)
}
pub(crate) fn public_device_at(path: &std::path::Path) -> Result<DeviceIdentity, String> {
    let hostname = gethostname::gethostname();
    let label = hostname
        .to_str()
        .ok_or_else(|| "device hostname is not UTF-8".to_string())?;
    crate::device_identity::load_or_create_device_identity(path, label)
}
#[cfg(test)]
mod tests;
