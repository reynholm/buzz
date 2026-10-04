//! Durable selected-target deletion, sharing the claim journal and retention transaction.
use super::super::{
    device_authority::{
        authorize_instance_authority, validate_deletion_authority, DeletionAuthority,
        InstanceAuthorityAction,
    },
    retention::{get_retained_event, open_retention_db, scoped_retention_db_path},
};
use super::*;

/// Authorize the exact private target before signing or writing an intent.
pub(crate) fn prepare_delete_in_dir(
    dir: &Path,
    context: &DevicePolicyContext,
    pubkey: &str,
    keys: &Keys,
) -> Result<HomeOperation, String> {
    let raw = read_policy_records(&dir.join("agents/managed-agents.json"))?;
    let target = raw
        .iter()
        .find(|r| r.pubkey == pubkey)
        .ok_or_else(|| format!("agent {pubkey} not found"))?;
    let d = definition(&raw, target.persona_id.as_deref());
    authorize_instance_authority(target, d.as_ref(), context, InstanceAuthorityAction::Delete)?;
    prepare_authorized_delete(dir, &context.scope, pubkey, keys)
}
fn definition(raw: &[ManagedAgentRecord], id: Option<&str>) -> Option<AgentDefinition> {
    raw.iter()
        .find(|r| r.pubkey.is_empty() && r.slug.as_deref() == id)
        .and_then(ManagedAgentRecord::to_definition_view)
}
fn target_revision(
    raw: &[ManagedAgentRecord],
    target: &ManagedAgentRecord,
) -> Result<String, String> {
    if let Some(d) = definition(raw, target.persona_id.as_deref()) {
        revision(&d, target)
    } else {
        Ok(hex::encode(Sha256::digest(
            serde_json::to_vec(&super::super::agent_events::agent_event_content(target))
                .map_err(|e| e.to_string())?,
        )))
    }
}
fn prepare_authorized_delete(
    dir: &Path,
    scope: &super::super::device_home_sync::SyncScope,
    pubkey: &str,
    keys: &Keys,
) -> Result<HomeOperation, String> {
    if keys.public_key().to_hex() != scope.owner_pubkey {
        return Err("device_home_sync_stale_session".into());
    }
    let raw = read_policy_records(&dir.join("agents/managed-agents.json"))?;
    let target = raw
        .iter()
        .find(|r| !r.pubkey.is_empty() && r.pubkey == pubkey)
        .ok_or_else(|| format!("agent {pubkey} not found"))?;
    let db = scoped_retention_db_path(&dir.join("agents"), &scope.relay_url, &scope.owner_pubkey);
    if let Some(parent) = db.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let conn = open_retention_db(&db)?;
    super::super::retention::remember_retention_scope(
        &conn,
        &scope.relay_url,
        &scope.owner_pubkey,
    )?;
    let mut events = crate::commands::prepare_agent_delete_events(
        &conn,
        keys,
        pubkey,
        target.persona_id.as_deref(),
    )?;
    if let Some(mut d) = definition(&raw, target.persona_id.as_deref()) {
        if d.share_across_devices != Some(true)
            && !raw.iter().any(|r| {
                !r.pubkey.is_empty()
                    && r.pubkey != pubkey
                    && r.persona_id.as_deref() == Some(d.id.as_str())
            })
        {
            d.origin_released = Some(true);
            let prior = get_retained_event(&conn, 30175, &scope.owner_pubkey, &d.id)?;
            d.shared = prior
                .as_ref()
                .and_then(|r| Event::from_json(&r.raw_event).ok())
                .is_some_and(|e| buzz_core_pkg::kind::event_is_shared(&e));
            events.insert(
                0,
                super::super::persona_events::build_persona_event(&d)?
                    .custom_created_at(super::super::persona_events::monotonic_created_at(
                        prior.as_ref().map(|r| r.created_at),
                    ))
                    .sign_with_keys(keys)
                    .map_err(|e| e.to_string())?,
            );
        }
    }
    Ok(HomeOperation {
        id: uuid::Uuid::new_v4().to_string(),
        relay_url: scope.relay_url.clone(),
        owner_pubkey: scope.owner_pubkey.clone(),
        kind: HomeOperationKind::Delete {
            target_pubkey: pubkey.into(),
            persona_id: target.persona_id.clone(),
            expected_store_revision: target_revision(&raw, target)?,
        },
        signed_events: events,
    })
}
/// Native entry: retain opaque pre-removal authority and original scope.
pub(crate) fn prepare_home_delete_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    context: &DevicePolicyContext,
    pubkey: &str,
) -> Result<HomeOperation, String> {
    let state = app.state::<crate::app_state::AppState>();
    let permit = super::super::device_authority::prepare_deletion_authority_locked_with(
        app,
        &state,
        pubkey,
        super::super::persona_device_view::load_device_policy_context,
    )?;
    if permit.scope() != &context.scope {
        return Err("device_home_sync_stale_session".into());
    }
    validate_deletion_authority(&state, &permit, InstanceAuthorityAction::Tombstone)?;
    validate_deletion_authority(&state, &permit, InstanceAuthorityAction::Archive)?;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let op = prepare_delete_in_dir(&dir, context, pubkey, &state.signing_keys()?)?;
    append_intent(&dir, &op)?;
    Ok(op)
}
/// Durable intent is saved while the exact target and its authority still exist.
pub(crate) fn prepare_home_delete_authorized_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    permit: &DeletionAuthority,
) -> Result<HomeOperation, String> {
    let state = app.state::<crate::app_state::AppState>();
    validate_deletion_authority(&state, permit, InstanceAuthorityAction::Tombstone)?;
    validate_deletion_authority(&state, permit, InstanceAuthorityAction::Archive)?;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let op = prepare_authorized_delete(
        &dir,
        permit.scope(),
        permit.pubkey(),
        &state.signing_keys()?,
    )?;
    append_intent(&dir, &op)?;
    Ok(op)
}
fn append_intent(dir: &Path, op: &HomeOperation) -> Result<(), String> {
    let mut ops = read_journal(dir)?;
    if !ops.iter().any(|r| r.id == op.id) {
        ops.push(op.clone());
        save_journal(dir, &ops)?;
    }
    Ok(())
}
/// One selected-target snapshot, then durable enqueue, then journal clear.
pub(crate) fn commit_delete_in_dir(
    dir: &Path,
    op: &HomeOperation,
    save: impl FnOnce(&[ManagedAgentRecord]) -> Result<(), String>,
    enqueue: impl FnOnce(&HomeOperation) -> Result<(), String>,
) -> Result<(), String> {
    commit_delete_with_journal(dir, op, save, enqueue, |ops| save_journal(dir, ops))
}
/// Only durable journal writes are injectable; the production snapshot and SQL seams remain.
pub(crate) fn commit_delete_with_journal(
    dir: &Path,
    op: &HomeOperation,
    save: impl FnOnce(&[ManagedAgentRecord]) -> Result<(), String>,
    enqueue: impl FnOnce(&HomeOperation) -> Result<(), String>,
    mut journal_save: impl FnMut(&[HomeOperation]) -> Result<(), String>,
) -> Result<(), String> {
    let mut ops = read_journal(dir)?;
    if !ops.iter().any(|r| r.id == op.id) {
        ops.push(op.clone());
        journal_save(&ops)?;
    }
    let mut raw = read_policy_records(&dir.join("agents/managed-agents.json"))?;
    let replay =
        apply_delete(&mut raw, op, None)?.ok_or_else(|| "stale deletion target".to_string())?;
    save(&raw)?;
    enqueue(&replay)?;
    ops.retain(|r| r.id != op.id);
    journal_save(&ops)
}
fn commit_snapshot(
    dir: &Path,
    op: &HomeOperation,
    save: impl FnOnce(&[ManagedAgentRecord]) -> Result<(), String>,
) -> Result<HomeOperation, String> {
    append_intent(dir, op)?;
    let mut raw = read_policy_records(&dir.join("agents/managed-agents.json"))?;
    let filtered =
        apply_delete(&mut raw, op, None)?.ok_or_else(|| "stale deletion target".to_string())?;
    save(&raw)?;
    Ok(filtered)
}
fn clear_intent(dir: &Path, op: &HomeOperation) -> Result<(), String> {
    let mut ops = read_journal(dir)?;
    ops.retain(|r| r.id != op.id);
    save_journal(dir, &ops)
}
type BindingVerifier<'a> = Option<&'a mut dyn FnMut(&str) -> Result<bool, String>>;

/// Recovery checks committed revisions and fresh proof before finishing an interrupted save.
/// A new sibling survives; only the old release is canceled, retaining the signed deletion pair.
pub(super) fn apply_delete(
    raw: &mut Vec<ManagedAgentRecord>,
    op: &HomeOperation,
    mut verify: BindingVerifier<'_>,
) -> Result<Option<HomeOperation>, String> {
    let HomeOperationKind::Delete {
        target_pubkey,
        persona_id,
        expected_store_revision,
    } = &op.kind
    else {
        return Err("invalid delete operation".into());
    };
    if let Some(target) = raw
        .iter()
        .find(|r| !r.pubkey.is_empty() && r.pubkey == *target_pubkey)
    {
        if target_revision(raw, target)? != *expected_store_revision {
            return Ok(None);
        }
        let d = definition(raw, target.persona_id.as_deref());
        if d.as_ref()
            .is_some_and(|d| d.share_across_devices != Some(true))
        {
            let binding = target
                .device_host_binding
                .as_deref()
                .ok_or_else(|| "delete binding unavailable".to_string())?;
            if let Some(verify) = verify.as_mut() {
                if !verify(binding)? {
                    return Err("home delete belongs to another host".into());
                }
            }
        }
        raw.retain(|r| r.pubkey != *target_pubkey);
    }
    let mut replay = op.clone();
    if let Some(id) = persona_id {
        let sibling = raw
            .iter()
            .any(|r| !r.pubkey.is_empty() && r.persona_id.as_deref() == Some(id.as_str()));
        if let Some(row) = raw
            .iter_mut()
            .find(|r| r.pubkey.is_empty() && r.slug.as_deref() == Some(id.as_str()))
        {
            if sibling {
                // Repair only the old release value; preserve new lineage/configuration.
                if replay
                    .signed_events
                    .iter()
                    .any(|e| e.kind.as_u16() == 30175)
                {
                    row.origin_released = Some(false);
                }
                replay.signed_events.retain(|e| e.kind.as_u16() != 30175);
            } else if let Some(event) = replay
                .signed_events
                .iter()
                .find(|e| e.kind.as_u16() == 30175)
            {
                let d = super::super::persona_events::persona_from_event(event)?;
                let mut current = row
                    .to_definition_view()
                    .ok_or_else(|| "delete definition unavailable".to_string())?;
                current.origin_released = Some(true);
                if super::super::persona_events::persona_event_content(&current)
                    == super::super::persona_events::persona_event_content(&d)
                {
                    row.origin_released = Some(true);
                } else {
                    replay.signed_events.retain(|e| e.kind.as_u16() != 30175);
                }
            }
        } else {
            replay.signed_events.retain(|e| e.kind.as_u16() != 30175);
        }
    }
    Ok(Some(replay))
}
/// Commit one authorized snapshot while existing assignment-cleanup owns its rollback.
pub(crate) fn commit_home_delete_snapshot_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    op: &HomeOperation,
) -> Result<HomeOperation, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    commit_snapshot(&dir, op, |raw| {
        super::super::device_authority::save_deletion_snapshot(app, raw)
    })
}
/// Enqueue the already committed selected deletion and clear only after durable transaction.
pub(crate) fn complete_home_delete_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    op: &HomeOperation,
) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    enqueue_in_scope(&dir, op)?;
    clear_intent(&dir, op)
}
/// Cascade completion uses the same journal; absent definitions cancel only the release.
pub(crate) fn finish_home_delete_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    op: &HomeOperation,
) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let before =
        std::fs::read(dir.join("agents/managed-agents.json")).map_err(|e| e.to_string())?;
    commit_delete_in_dir(
        &dir,
        op,
        |raw| {
            let bytes = serde_json::to_vec_pretty(raw).map_err(|e| e.to_string())?;
            if bytes != before {
                super::super::device_authority::save_deletion_snapshot(app, raw)?;
            }
            Ok(())
        },
        |op| enqueue_in_scope(&dir, op),
    )
}
