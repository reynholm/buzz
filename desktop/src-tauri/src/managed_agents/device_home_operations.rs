//! Durable, secret-free home claim publication intent.
use super::{
    device_creation::{
        authorize_definition_action, bind_new_instance, stamp_new_definition, DefinitionAction,
    },
    persona_device_view::{read_policy_records, DevicePolicyContext},
    retention::{retain_event, RetainedEvent},
    storage::atomic_write_json_restricted,
    AgentDefinition, ManagedAgentRecord,
};
use nostr::{Event, JsonUtil, Keys};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use tauri::Manager;

/// Original owner/relay scope and already signed public events; never an instance secret.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct HomeOperation {
    pub id: String,
    pub relay_url: String,
    pub owner_pubkey: String,
    pub kind: HomeOperationKind,
    pub signed_events: Vec<Event>,
}
/// Target revision is a digest of the definition, instance public projection and binding.
/// Unrelated rows, key hydration and process state do not invalidate a committed claim.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) enum HomeOperationKind {
    Delete {
        target_pubkey: String,
        persona_id: Option<String>,
        expected_store_revision: String,
    },
    Label {
        new_label: String,
        affected_definition_ids: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unresolved_scope: Option<String>,
    },
    Claim {
        definition_id: String,
        target_pubkey: String,
        expected_store_revision: String,
    },
}
fn journal(path: &Path) -> std::path::PathBuf {
    path.join("device-home-operations.json")
}
fn read_journal(dir: &Path) -> Result<Vec<HomeOperation>, String> {
    match std::fs::read(journal(dir)) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_err(|e| format!("home operation journal: {e}"))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(format!("home operation read: {e}")),
    }
}
fn save_journal(dir: &Path, operations: &[HomeOperation]) -> Result<(), String> {
    let bytes = serde_json::to_vec(operations).map_err(|e| e.to_string())?;
    atomic_write_json_restricted(&journal(dir), &bytes)
}
fn revision(d: &AgentDefinition, i: &ManagedAgentRecord) -> Result<String, String> {
    let bytes = serde_json::to_vec(&(
        super::persona_events::persona_event_content(d),
        super::agent_events::agent_event_content(i),
        &i.device_host_binding,
    ))
    .map_err(|e| e.to_string())?;
    Ok(hex::encode(Sha256::digest(bytes)))
}
/// Commit the existing private definition and its new bound instance in one save.
/// Signed intent precedes that save; enqueue failures preserve recovery without another mint.
pub(crate) fn commit_claim_in_dir(
    dir: &Path,
    context: &DevicePolicyContext,
    definition_id: &str,
    mut record: ManagedAgentRecord,
    keys: &Keys,
    save: impl FnOnce(&[ManagedAgentRecord]) -> Result<(), String>,
    enqueue: impl FnOnce(&HomeOperation) -> Result<(), String>,
) -> Result<(), String> {
    if keys.public_key().to_hex() != context.scope.owner_pubkey {
        return Err("device_home_sync_stale_session".into());
    }
    let mut raw = read_policy_records(&dir.join("agents/managed-agents.json"))?;
    let index = match raw
        .iter()
        .position(|r| r.pubkey.is_empty() && r.slug.as_deref() == Some(definition_id))
    {
        Some(index) => index,
        None => {
            let definition =
                super::built_in_persona_definition(definition_id, &crate::util::now_iso())
                    .ok_or_else(|| format!("agent {definition_id} not found"))?;
            raw.push(definition.into_agent_record());
            raw.len() - 1
        }
    };
    let mut definition = raw[index]
        .to_definition_view()
        .ok_or_else(|| "invalid claim definition".to_string())?;
    authorize_definition_action(context, &definition, &raw, DefinitionAction::CreateInstance)?;
    if definition.share_across_devices == Some(true)
        || record.persona_id.as_deref() != Some(definition_id)
        || record.pubkey.is_empty()
        || raw.iter().any(|r| r.pubkey == record.pubkey)
    {
        return Err("invalid home claim target".into());
    }
    // An existing proven home's public lineage cannot be moved by a new instance.
    stamp_new_definition(&mut definition, Some(false), &context.device);
    bind_new_instance(&mut record, &context.proof);
    raw[index] = definition.clone().into_agent_record();
    raw.push(record.clone());
    let events = prepare_claim_events(dir, context, &definition, &record, keys)?;
    let operation = HomeOperation {
        id: uuid::Uuid::new_v4().to_string(),
        relay_url: context.scope.relay_url.clone(),
        owner_pubkey: context.scope.owner_pubkey.clone(),
        kind: HomeOperationKind::Claim {
            definition_id: definition_id.into(),
            target_pubkey: record.pubkey.clone(),
            expected_store_revision: revision(&definition, &record)?,
        },
        signed_events: events,
    };
    let mut operations = read_journal(dir)?;
    operations.push(operation);
    save_journal(dir, &operations)?;
    save(&raw)?;
    let operation = operations
        .last()
        .ok_or_else(|| "home operation disappeared".to_string())?;
    enqueue(operation)?;
    operations.pop();
    save_journal(dir, &operations)
}
fn prepare_claim_events(
    dir: &Path,
    context: &DevicePolicyContext,
    definition: &AgentDefinition,
    record: &ManagedAgentRecord,
    keys: &Keys,
) -> Result<Vec<Event>, String> {
    use super::{
        persona_events::monotonic_created_at,
        retention::{get_retained_event, scoped_retention_db_path},
    };
    let db = scoped_retention_db_path(
        &dir.join("agents"),
        &context.scope.relay_url,
        &context.scope.owner_pubkey,
    );
    let conn = if db.exists() {
        Some(
            Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };
    let prior = |kind, id: &str| -> Result<Option<RetainedEvent>, String> {
        conn.as_ref()
            .map(|conn| get_retained_event(conn, kind, &context.scope.owner_pubkey, id))
            .transpose()
            .map(Option::flatten)
    };
    let persona_head = prior(30175, &definition.id)?;
    let instance_head = prior(30177, &record.pubkey)?;
    let mut scoped = definition.clone();
    scoped.shared = persona_head
        .as_ref()
        .and_then(|row| Event::from_json(&row.raw_event).ok())
        .is_some_and(|e| buzz_core_pkg::kind::event_is_shared(&e));
    Ok(vec![
        super::persona_events::build_persona_event(&scoped)?
            .custom_created_at(monotonic_created_at(
                persona_head.as_ref().map(|row| row.created_at),
            ))
            .sign_with_keys(keys)
            .map_err(|e| e.to_string())?,
        super::agent_events::build_agent_event(record)?
            .custom_created_at(monotonic_created_at(
                instance_head.as_ref().map(|row| row.created_at),
            ))
            .sign_with_keys(keys)
            .map_err(|e| e.to_string())?,
    ])
}
/// Atomically retain all signed heads for the original operation scope.
pub(crate) fn enqueue_home_events(
    conn: &mut Connection,
    operation: &HomeOperation,
) -> Result<(), String> {
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    enqueue_home_events_in_transaction(&tx, operation)?;
    tx.commit().map_err(|e| format!("home events commit: {e}"))
}
/// All event kinds share the caller-owned transaction; no helper opens nested BEGIN.
fn enqueue_home_events_in_transaction(
    conn: &Connection,
    operation: &HomeOperation,
) -> Result<(), String> {
    match &operation.kind {
        HomeOperationKind::Claim {
            definition_id,
            target_pubkey,
            ..
        } => {
            if operation.signed_events.len() != 2 {
                return Err("invalid home claim event count".into());
            }
            for (event, kind, coordinate) in [
                (&operation.signed_events[0], 30175, definition_id.as_str()),
                (&operation.signed_events[1], 30177, target_pubkey.as_str()),
            ] {
                enqueue_head(conn, operation, event, kind, coordinate)?;
            }
        }
        HomeOperationKind::Delete {
            target_pubkey,
            persona_id,
            ..
        } => {
            let events = &operation.signed_events;
            if events.len() == 3 {
                let id = persona_id
                    .as_deref()
                    .ok_or_else(|| "release missing definition".to_string())?;
                enqueue_head(conn, operation, &events[0], 30175, id)?;
            } else if events.len() != 2 {
                return Err("invalid home delete event count".into());
            }
            crate::commands::enqueue_agent_delete_events(
                conn,
                &operation.owner_pubkey,
                target_pubkey,
                &events[events.len() - 2..],
            )?;
        }
        HomeOperationKind::Label {
            affected_definition_ids,
            unresolved_scope,
            ..
        } => {
            if unresolved_scope.is_some()
                || operation.signed_events.len() != affected_definition_ids.len()
            {
                return Err("unprepared device label worklist".into());
            }
            for (event, id) in operation.signed_events.iter().zip(affected_definition_ids) {
                enqueue_head(conn, operation, event, 30175, id)?;
            }
        }
    }
    Ok(())
}

fn enqueue_head(
    conn: &Connection,
    operation: &HomeOperation,
    event: &Event,
    kind: u16,
    coordinate: &str,
) -> Result<(), String> {
    event
        .verify()
        .map_err(|e| format!("home event signature: {e}"))?;
    let d = event
        .tags
        .iter()
        .find_map(|t| {
            let t = t.as_slice();
            (t.first().map(String::as_str) == Some("d"))
                .then(|| t.get(1))
                .flatten()
        })
        .ok_or_else(|| "home event coordinate missing".to_string())?;
    if event.kind.as_u16() != kind
        || event.pubkey.to_hex() != operation.owner_pubkey
        || d != coordinate
    {
        return Err("invalid home event signed scope or coordinate".into());
    }
    retain_event(
        conn,
        &RetainedEvent {
            kind: kind as u32,
            pubkey: operation.owner_pubkey.clone(),
            d_tag: d.clone(),
            content: event.content.clone(),
            created_at: event.created_at.as_secs() as i64,
            raw_event: event.as_json(),
            pending_sync: true,
        },
    )
}
/// Recover only claims whose matching bound target and definition were committed.
/// Failed saves leave intent intact and never publish or mint an uncommitted target.
pub(crate) fn recover_in_dir(
    dir: &Path,
    mut verify_binding: impl FnMut(&str) -> Result<bool, String>,
    mut enqueue: impl FnMut(&HomeOperation) -> Result<(), String>,
) -> Result<(), String> {
    let mut operations = read_journal(dir)?;
    if operations.is_empty() {
        return Ok(());
    }
    let mut raw = read_policy_records(&dir.join("agents/managed-agents.json"))?;
    let latest_label = operations.iter().rev().find_map(|op| match &op.kind {
        HomeOperationKind::Label { new_label, .. } => Some(new_label.clone()),
        _ => None,
    });
    if let Some(label) = &latest_label {
        label::apply_label(dir, &mut raw, label)?;
    }
    let mut completed = Vec::new();
    for op in &operations {
        match &op.kind {
            HomeOperationKind::Claim {
                definition_id,
                target_pubkey,
                expected_store_revision,
            } => {
                let d = raw
                    .iter()
                    .find(|r| {
                        r.pubkey.is_empty() && r.slug.as_deref() == Some(definition_id.as_str())
                    })
                    .and_then(ManagedAgentRecord::to_definition_view);
                let i = raw.iter().find(|r| {
                    r.pubkey == *target_pubkey
                        && r.persona_id.as_deref() == Some(definition_id.as_str())
                        && r.device_host_binding.is_some()
                });
                if let (Some(d), Some(i)) = (d, i) {
                    if revision(&d, i)? == *expected_store_revision {
                        let binding = i
                            .device_host_binding
                            .as_deref()
                            .ok_or_else(|| "home claim binding unavailable".to_string())?;
                        if !verify_binding(binding)? {
                            return Err("home claim belongs to another host".into());
                        }
                        enqueue(op)?;
                        completed.push(op.id.clone());
                    }
                }
            }
            HomeOperationKind::Delete { .. } => {
                let before = serde_json::to_vec(&raw).map_err(|e| e.to_string())?;
                if let Some(replay) = delete::apply_delete(&mut raw, op, Some(&mut verify_binding))?
                {
                    let base = dir.join("agents");
                    super::bestie_assignment::recover_pending_assignment_cleanup(&base, |pk| {
                        raw.iter().any(|r| r.pubkey == pk)
                    })?;
                    let HomeOperationKind::Delete { target_pubkey, .. } = &op.kind else {
                        return Err("delete operation disappeared".into());
                    };
                    super::bestie_assignment::with_agent_assignments_cleared(
                        &base,
                        target_pubkey,
                        || {
                            if serde_json::to_vec(&raw).map_err(|e| e.to_string())? != before {
                                save_unified(dir, &raw)?;
                            }
                            Ok(())
                        },
                    )?;
                    enqueue(&replay)?;
                    completed.push(op.id.clone());
                }
            }
            HomeOperationKind::Label {
                new_label,
                affected_definition_ids,
                unresolved_scope,
            } => {
                if latest_label.as_deref() != Some(new_label) {
                    completed.push(op.id.clone());
                    continue;
                }
                if unresolved_scope.is_none()
                    && !affected_definition_ids.is_empty()
                    && op.signed_events.len() == affected_definition_ids.len()
                {
                    enqueue(op)?;
                    completed.push(op.id.clone());
                } else if unresolved_scope.is_none() && affected_definition_ids.is_empty() {
                    completed.push(op.id.clone());
                }
            }
        }
    }
    operations.retain(|op| !completed.contains(&op.id));
    save_journal(dir, &operations)
}

/// Native Phase3 adapter, already under the managed store lock.
pub(crate) fn commit_home_claim_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    context: &DevicePolicyContext,
    definition_id: &str,
    record: ManagedAgentRecord,
) -> Result<(), String> {
    let state = app.state::<crate::app_state::AppState>();
    let check = || -> Result<(), String> {
        if super::device_home_sync::capture_scope(&state)? != context.scope {
            return Err("device_home_sync_stale_session".into());
        }
        Ok(())
    };
    check()?;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let keys = state.signing_keys()?;
    let mut new = vec![record];
    super::storage::persist_agent_keys(&mut new);
    let record = new
        .pop()
        .ok_or_else(|| "claim record missing".to_string())?;
    commit_claim_in_dir(
        &dir,
        context,
        definition_id,
        record,
        &keys,
        |raw| {
            check()?;
            save_unified(&dir, raw)
        },
        |operation| enqueue_in_scope(&dir, operation),
    )
}
fn save_unified(dir: &Path, raw: &[ManagedAgentRecord]) -> Result<(), String> {
    atomic_write_json_restricted(
        &dir.join("agents/managed-agents.json"),
        &serde_json::to_vec_pretty(raw).map_err(|e| e.to_string())?,
    )
}
/// Preserve every unrelated structural row; persist only the new instance key.
pub(crate) fn append_new_instance_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    record: ManagedAgentRecord,
) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let mut raw = read_policy_records(&dir.join("agents/managed-agents.json"))?;
    if raw.iter().any(|r| r.pubkey == record.pubkey) {
        return Err("duplicate new instance".into());
    }
    let mut new = vec![record];
    super::storage::persist_agent_keys(&mut new);
    raw.extend(new);
    save_unified(&dir, &raw)
}
fn enqueue_in_scope(dir: &Path, operation: &HomeOperation) -> Result<(), String> {
    let db = super::retention::scoped_retention_db_path(
        &dir.join("agents"),
        &operation.relay_url,
        &operation.owner_pubkey,
    );
    if let Some(parent) = db.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut conn = super::retention::open_retention_db(&db)?;
    super::retention::remember_retention_scope(
        &conn,
        &operation.relay_url,
        &operation.owner_pubkey,
    )?;
    enqueue_home_events(&mut conn, operation)
}
/// Save new imported definitions and instances together, preserving unrelated rows and keys.
pub(crate) fn commit_new_pairs_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    definitions: Vec<AgentDefinition>,
    instances: Vec<ManagedAgentRecord>,
) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    commit_new_pairs_in_dir(
        &dir,
        definitions,
        instances,
        super::storage::persist_agent_keys,
    )
}
/// Import commit seam; only newly minted key persistence is injectable.
pub(crate) fn commit_new_pairs_in_dir(
    dir: &Path,
    definitions: Vec<AgentDefinition>,
    mut instances: Vec<ManagedAgentRecord>,
    persist: impl FnOnce(&mut [ManagedAgentRecord]),
) -> Result<(), String> {
    let mut raw = read_policy_records(&dir.join("agents/managed-agents.json"))?;
    for d in &definitions {
        if raw
            .iter()
            .any(|r| r.pubkey.is_empty() && r.slug.as_deref() == Some(d.id.as_str()))
        {
            return Err("duplicate imported definition".into());
        }
    }
    if instances
        .iter()
        .any(|i| raw.iter().any(|r| r.pubkey == i.pubkey))
    {
        return Err("duplicate imported instance".into());
    }
    persist(&mut instances);
    raw.extend(
        definitions
            .into_iter()
            .map(AgentDefinition::into_agent_record),
    );
    raw.extend(instances);
    save_unified(dir, &raw)
}
/// Pre-migration recovery; caller owns the store lock and original journal scopes persist.
pub(crate) fn recover_home_operations_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<(), String> {
    recover_home_operations_locked_with(app, |binding| {
        let proof = crate::device_identity::load_existing_host_proof(
            &crate::secret_store::SecretStore::keyring(crate::build_identity::device_host_service()),
        )?;
        Ok(proof.matches(binding))
    })
}
/// Native recovery adapter with only the fresh authority read injected for isolated tests.
pub(crate) fn recover_home_operations_locked_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    verify_binding: impl FnMut(&str) -> Result<bool, String>,
) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    label::prepare_label_retries(
        &dir,
        &app.state::<crate::app_state::AppState>().signing_keys()?,
    )?;
    recover_in_dir(&dir, verify_binding, |operation| {
        enqueue_in_scope(&dir, operation)
    })
}

pub(crate) mod delete;
#[cfg(test)]
mod durable_tests;
pub(crate) mod label;
#[cfg(test)]
mod tests;
