//! Local authoring authority; inbound owner reconciliation remains a receiver operation.
use super::{
    device_runtime::{assert_runtime_fence, capture_runtime_fence, RuntimeFence},
    persona_device_view::{read_policy_records, DevicePolicyContext},
    AgentDefinition, ManagedAgentRecord,
};
use crate::app_state::AppState;
use tauri::AppHandle;

/// Effects governed by exact instance host proof.
#[derive(Clone, Copy, Debug)]
pub(crate) enum InstanceAuthorityAction {
    Update,
    Delete,
    PublishProfile,
    PublishHead,
    Tombstone,
    Archive,
}
/// Authorize an exact linked target; a proven sibling is insufficient.
pub(crate) fn authorize_instance_authority(
    record: &ManagedAgentRecord,
    definition: Option<&AgentDefinition>,
    context: &DevicePolicyContext,
    _action: InstanceAuthorityAction,
) -> Result<(), String> {
    if record.persona_id.is_none() {
        return Ok(());
    }
    let definition = definition
        .filter(|d| Some(d.id.as_str()) == record.persona_id.as_deref())
        .ok_or(super::effective_config::ORPHANED_INSTANCE_ERROR)?;
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
/// Authorize before key hydration, mutation, signing or journal preparation. Caller holds store lock.
pub(crate) fn instance_phase_locked_with<R: tauri::Runtime, T>(
    app: &AppHandle<R>,
    state: &AppState,
    pubkey: &str,
    expected: Option<&RuntimeFence>,
    action: InstanceAuthorityAction,
    context: impl FnOnce(&AppHandle<R>, &AppState) -> Result<DevicePolicyContext, String>,
    effect: impl FnOnce(
        ManagedAgentRecord,
        Vec<AgentDefinition>,
        super::device_home_sync::SyncScope,
    ) -> Result<T, String>,
) -> Result<T, String> {
    let fence = capture_runtime_fence(state)?;
    if let Some(expected) = expected {
        assert_runtime_fence(state, expected)?;
    }
    let raw = read_policy_records(&super::managed_agents_store_path(app)?)?;
    let record = raw
        .iter()
        .find(|r| !r.pubkey.is_empty() && r.pubkey == pubkey)
        .cloned()
        .ok_or_else(|| format!("agent {pubkey} not found"))?;
    let definitions: Vec<_> = raw
        .iter()
        .filter(|r| r.pubkey.is_empty())
        .filter_map(ManagedAgentRecord::to_definition_view)
        .collect();
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
        let c = context(app, state).map_err(|e| format!("device_home_sync_failed: {e}"))?;
        super::device_creation::assert_creation_scope(&fence.scope, &c.scope)?;
        authorize_instance_authority(&record, definition, &c, action)?;
    }
    assert_runtime_fence(state, &fence)?;
    effect(record, definitions, fence.scope)
}
/// Guard a definition cascade before any durable or runtime effect. Caller holds store lock.
pub(crate) fn definition_delete_phase_locked_with<R: tauri::Runtime, T>(
    app: &AppHandle<R>,
    state: &AppState,
    id: &str,
    context: impl FnOnce(&AppHandle<R>, &AppState) -> Result<DevicePolicyContext, String>,
    effect: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let fence = capture_runtime_fence(state)?;
    let raw = read_policy_records(&super::managed_agents_store_path(app)?)?;
    let d = raw
        .iter()
        .filter(|r| r.pubkey.is_empty())
        .filter_map(ManagedAgentRecord::to_definition_view)
        .find(|d| d.id == id)
        .ok_or_else(|| format!("persona {id} not found"))?;
    if d.share_across_devices != Some(true) {
        let c = context(app, state).map_err(|e| format!("device_home_sync_failed: {e}"))?;
        super::device_creation::assert_creation_scope(&fence.scope, &c.scope)?;
        authorize_definition_deletion(&d, &raw, &c)?;
    }
    assert_runtime_fence(state, &fence)?;
    effect()
}
/// Validate the complete cascade atomically, including exact linked instance bindings.
pub(crate) fn authorize_definition_deletion(
    definition: &AgentDefinition,
    records: &[ManagedAgentRecord],
    context: &DevicePolicyContext,
) -> Result<(), String> {
    super::device_creation::authorize_definition_action(
        context,
        definition,
        records,
        super::device_creation::DefinitionAction::DeleteDefinition,
    )?;
    for record in records
        .iter()
        .filter(|r| !r.pubkey.is_empty() && r.persona_id.as_deref() == Some(definition.id.as_str()))
    {
        authorize_instance_authority(
            record,
            Some(definition),
            context,
            InstanceAuthorityAction::Delete,
        )?;
    }
    Ok(())
}
/// Authorize a postcommit publication before signing/dispatch, retaining the original scope.
pub(crate) async fn original_publication_with<
    R: tauri::Runtime,
    T,
    Fut: std::future::Future<Output = Result<T, String>>,
>(
    app: &AppHandle<R>,
    state: &AppState,
    pubkey: &str,
    fence: &RuntimeFence,
    action: InstanceAuthorityAction,
    context: impl FnOnce(&AppHandle<R>, &AppState) -> Result<DevicePolicyContext, String>,
    effect: impl FnOnce(super::device_home_sync::SyncScope) -> Fut,
) -> Result<T, String> {
    let scope = {
        let _store = state
            .managed_agents_store_lock
            .lock()
            .map_err(|e| e.to_string())?;
        instance_phase_locked_with(
            app,
            state,
            pubkey,
            Some(fence),
            action,
            context,
            |_, _, s| Ok(s),
        )?
    };
    effect(scope).await
}

/// Proof captured before removal under the store lock; cannot be fabricated by callers.
pub(crate) struct DeletionAuthority {
    pubkey: String,
    fence: RuntimeFence,
}
impl DeletionAuthority {
    /// The exact authorized instance coordinate.
    pub(crate) fn pubkey(&self) -> &str {
        &self.pubkey
    }
    /// Original public scope for Task8's durable deletion intent and enqueue.
    pub(crate) fn scope(&self) -> &super::device_home_sync::SyncScope {
        &self.fence.scope
    }
}
/// Prepare tombstone/archive authority while the canonical target and definition still exist.
pub(crate) fn prepare_deletion_authority_locked_with<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    pubkey: &str,
    context: impl FnOnce(&AppHandle<R>, &AppState) -> Result<DevicePolicyContext, String>,
) -> Result<DeletionAuthority, String> {
    instance_phase_locked_with(
        app,
        state,
        pubkey,
        None,
        InstanceAuthorityAction::Tombstone,
        context,
        |record, _, _| {
            Ok(DeletionAuthority {
                pubkey: record.pubkey,
                fence: capture_runtime_fence(state)?,
            })
        },
    )
}
/// Revalidate the original scope before signing either part of a prepared deletion.
pub(crate) fn validate_deletion_authority(
    state: &AppState,
    permit: &DeletionAuthority,
    action: InstanceAuthorityAction,
) -> Result<(), String> {
    if !matches!(
        action,
        InstanceAuthorityAction::Tombstone | InstanceAuthorityAction::Archive
    ) {
        return Err("invalid deletion authority action".into());
    }
    assert_runtime_fence(state, &permit.fence)
}

/// Persist an authorized deletion snapshot without migrating unrelated keys or definitions.
pub(crate) fn save_deletion_snapshot<R: tauri::Runtime>(
    app: &AppHandle<R>,
    records: &[ManagedAgentRecord],
) -> Result<(), String> {
    let bytes =
        serde_json::to_vec_pretty(records).map_err(|e| format!("deletion snapshot: {e}"))?;
    super::storage::atomic_write_json_restricted(&super::managed_agents_store_path(app)?, &bytes)
}
/// Run deletion housekeeping only on selected targets, preserving other rows byte-for-byte.
pub(crate) fn selected_deletion_records_with<T>(
    records: &mut [ManagedAgentRecord],
    targets: &std::collections::HashSet<String>,
    effect: impl FnOnce(&mut [ManagedAgentRecord]) -> Result<T, String>,
) -> Result<T, String> {
    let mut selected: Vec<_> = records
        .iter()
        .filter(|r| !r.pubkey.is_empty() && targets.contains(&r.pubkey))
        .cloned()
        .collect();
    let result = effect(&mut selected)?;
    for record in records.iter_mut() {
        if let Some(update) = selected.iter().find(|r| r.pubkey == record.pubkey) {
            *record = update.clone();
        }
    }
    Ok(result)
}
