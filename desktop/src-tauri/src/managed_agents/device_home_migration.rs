//! Claim legacy homes after history; copied JSON is never local host authority.
use super::{
    definition_home::EvidenceReadiness,
    persona_device_view::{read_policy_records, DevicePolicyContext},
    storage::{atomic_write_json_restricted, resolve_agent_key_readonly},
    AgentDefinition, ManagedAgentRecord,
};
use std::path::Path;
use tauri::Manager;

/// Migrate one unified snapshot without opportunistic keychain writes.
/// Caller owns the managed store lock; failure before persist leaves it unchanged.
#[cfg(test)]
pub(crate) fn migrate_device_homes_in_dir(
    base: &Path,
    context: &DevicePolicyContext,
    resolve: impl FnMut(&ManagedAgentRecord) -> Result<Option<nostr::Keys>, String>,
) -> Result<bool, String> {
    migrate_device_homes_in_dir_with_archive(base, context, &[], resolve)
}

/// A verified archive excludes retired duplicates from legacy conflicts only.
/// It grants no host/key authority and never claims an archived local identity.
pub(crate) fn migrate_device_homes_in_dir_with_archive(
    base: &Path,
    context: &DevicePolicyContext,
    archived: &[String],
    mut resolve: impl FnMut(&ManagedAgentRecord) -> Result<Option<nostr::Keys>, String>,
) -> Result<bool, String> {
    let path = base.join("managed-agents.json");
    let mut records = read_policy_records(&path)?;
    let mut changed = false;
    for i in 0..records.len() {
        if records[i].pubkey.is_empty() || archived.contains(&records[i].pubkey) {
            continue;
        }
        let Some(persona_id) = records[i].persona_id.as_deref() else {
            continue;
        };
        let Some(d) = records
            .iter()
            .position(|r| r.pubkey.is_empty() && r.slug.as_deref() == Some(persona_id))
        else {
            continue;
        };
        if records[d].share_across_devices == Some(true) {
            continue;
        }
        let proven = match records[i].device_host_binding.as_deref() {
            Some(binding) => {
                if !context.proof.matches(binding) {
                    continue;
                }
                true
            }
            None => false,
        };
        if !proven {
            // An absence-based legacy claim requires complete current history.
            if context.readiness != EvidenceReadiness::Ready
                || (records[d].origin_released != Some(true)
                    && records[d]
                        .origin_device_id
                        .as_deref()
                        .is_some_and(|id| id != context.device.device_id))
                || context.evidence.iter().any(|e| {
                    e.persona_id == persona_id
                        && e.pubkey != records[i].pubkey
                        && !archived.contains(&e.pubkey)
                })
            {
                continue;
            }
            let Some(keys) = resolve(&records[i])? else {
                continue;
            };
            if keys.public_key().to_hex() != records[i].pubkey {
                return Err("device home agent key does not match pubkey".into());
            }
            records[i].device_host_binding = Some(context.proof.binding().to_string());
            changed = true;
        }
        if records[d].origin_device_id.as_deref() != Some(context.device.device_id.as_str())
            || records[d].origin_device_label.as_deref() != Some(context.device.label.as_str())
            || records[d].origin_released != Some(false)
        {
            records[d].origin_device_id = Some(context.device.device_id.clone());
            records[d].origin_device_label = Some(context.device.label.clone());
            records[d].origin_released = Some(false);
            changed = true;
        }
    }
    if changed {
        let bytes = serde_json::to_vec_pretty(&records)
            .map_err(|e| format!("device home serialize: {e}"))?;
        atomic_write_json_restricted(&path, &bytes)?;
    }
    Ok(changed)
}

/// Only shared, standalone or locally proven instances may be advertised.
pub(crate) fn may_publish_local_instance(
    record: &ManagedAgentRecord,
    definition: Option<&AgentDefinition>,
    context: &DevicePolicyContext,
) -> Result<bool, String> {
    match super::device_authority::authorize_instance_authority(
        record,
        definition,
        context,
        super::device_authority::InstanceAuthorityAction::PublishHead,
    ) {
        Ok(()) => Ok(true),
        Err(error) if error.starts_with("definition_hosted_elsewhere") => Ok(false),
        Err(error) => Err(error),
    }
}

/// Publication guard shared by both event kinds. Missing private authority errors.
pub(crate) fn publication_allowed(
    record: &ManagedAgentRecord,
    definition: Option<&AgentDefinition>,
    context: Option<&DevicePolicyContext>,
) -> Result<bool, String> {
    let Some(id) = record.persona_id.as_deref() else {
        return Ok(true);
    };
    let definition = definition
        .filter(|d| d.id == id)
        .ok_or(super::effective_config::ORPHANED_INSTANCE_ERROR)?;
    if definition.share_across_devices == Some(true) {
        return Ok(true);
    }
    may_publish_local_instance(
        record,
        Some(definition),
        context.ok_or("device home publication authority unavailable")?,
    )
}

/// Require authority only for private linked records, preserving shared-only work.
pub(crate) fn needs_private_authority(records: &[ManagedAgentRecord]) -> bool {
    records.iter().any(|r| {
        !r.pubkey.is_empty()
            && r.persona_id.as_deref().is_some_and(|id| {
                !records.iter().any(|d| {
                    d.pubkey.is_empty()
                        && d.slug.as_deref() == Some(id)
                        && d.share_across_devices == Some(true)
                })
            })
    })
}

/// Persist binding and origin atomically, then queue signed heads. If retention
/// fails, the proven snapshot remains on disk and the next invocation retries
/// content-diff publication, including a run with no new migration edits.
/// Production migration orchestration with only the key lookup boundary injected.
pub(crate) fn migrate_device_homes_locked_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    context: &DevicePolicyContext,
    resolve: impl FnMut(&ManagedAgentRecord) -> Result<Option<nostr::Keys>, String>,
) -> Result<(), String> {
    migrate_device_homes_locked_with_archive(app, context, &[], resolve)
}

/// Complete legacy migration with archive evidence fetched for this exact scope.
pub(crate) fn migrate_device_homes_locked_with_archive<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    context: &DevicePolicyContext,
    archived: &[String],
    resolve: impl FnMut(&ManagedAgentRecord) -> Result<Option<nostr::Keys>, String>,
) -> Result<(), String> {
    let state = app.state::<crate::app_state::AppState>();
    if super::device_home_sync::capture_scope(&state)? != context.scope {
        return Err("device_home_sync_stale_session".into());
    }
    let base = super::managed_agents_base_dir(app)?;
    migrate_device_homes_in_dir_with_archive(&base, context, archived, resolve)?;
    let keys = state.signing_keys()?;
    if keys.public_key().to_hex() != context.scope.owner_pubkey {
        return Err("device_home_sync_stale_session".into());
    }
    let db = super::retention::scoped_retention_db_path(
        &base,
        &context.scope.relay_url,
        &context.scope.owner_pubkey,
    );
    queue_device_home_events(&base, &keys, &db, context)?;
    Ok(())
}

/// Content-diff retry of both home definition and instance heads.
pub(crate) fn queue_device_home_events(
    base: &Path,
    keys: &nostr::Keys,
    db: &Path,
    context: &DevicePolicyContext,
) -> Result<(), String> {
    crate::event_sync::migrate_personas_in_dir_with_context(base, keys, db, Some(context))?;
    super::reconcile::reconcile_agents_in_dir_with_context(base, keys, db, Some(context))?;
    Ok(())
}

/// Workspace hook: never wait for frontend history. Proven homes migrate now;
/// legacy absence claims are retried by the completion barrier.
pub(crate) fn migrate_device_homes_before_sync<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<(), String> {
    migrate_device_homes_before_sync_with(
        app,
        super::persona_device_view::load_device_policy_context,
        resolve_agent_key_readonly,
    )
}
/// Workspace adapter with authority/key boundaries injected for isolated tests.
pub(crate) fn migrate_device_homes_before_sync_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    context_provider: impl FnOnce(
        &tauri::AppHandle<R>,
        &crate::app_state::AppState,
    ) -> Result<DevicePolicyContext, String>,
    resolve: impl FnMut(&ManagedAgentRecord) -> Result<Option<nostr::Keys>, String>,
) -> Result<(), String> {
    let state = app.state::<crate::app_state::AppState>();
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let records = read_policy_records(&super::managed_agents_store_path(app)?)?;
    if !needs_private_authority(&records) {
        return Ok(());
    }
    let context = context_provider(app, &state)?;
    migrate_device_homes_locked_with(app, &context, resolve)
}

#[cfg(test)]
pub(crate) mod tests;

/// Filter auto-start work before probes, snapshot writes or spawn side effects.
/// The same proof predicate applies to restore and posthistory reconciliation.
pub(crate) fn auto_start_allowed(
    record: &ManagedAgentRecord,
    definitions: &[AgentDefinition],
    context: Option<&DevicePolicyContext>,
) -> Result<bool, String> {
    let definition = definitions
        .iter()
        .find(|d| record.persona_id.as_deref() == Some(d.id.as_str()));
    publication_allowed(record, definition, context)
}
