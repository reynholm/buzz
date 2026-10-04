//! Device label changes use the same journal with one public retry worklist per scope.
use super::super::retention::{
    get_retained_events_by_kind, known_retention_scopes, read_retention_scope,
    scoped_retention_db_path,
};
use super::*;
use crate::device_identity::{load_existing_device_identity, DeviceIdentity};

fn scoped_definitions(
    dir: &Path,
    relay: &str,
    owner: &str,
    device: &DeviceIdentity,
    use_local: bool,
) -> Result<Vec<AgentDefinition>, String> {
    let path = scoped_retention_db_path(&dir.join("agents"), relay, owner);
    let mut definitions = vec![];
    if path.exists() {
        let conn = Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
        for row in get_retained_events_by_kind(&conn, 30175, owner)? {
            let event = Event::from_json(&row.raw_event)
                .map_err(|e| format!("label retained event: {e}"))?;
            if event.pubkey.to_hex() != owner {
                return Err("label retained owner mismatch".into());
            }
            let d = super::super::persona_events::persona_from_event(&event)?;
            if d.origin_device_id.as_deref() == Some(device.device_id.as_str()) {
                definitions.push(d);
            }
        }
    }
    if use_local {
        for d in read_policy_records(&dir.join("agents/managed-agents.json"))?
            .iter()
            .filter(|r| r.pubkey.is_empty())
            .filter_map(ManagedAgentRecord::to_definition_view)
            .filter(|d| d.origin_device_id.as_deref() == Some(device.device_id.as_str()))
        {
            if !definitions.iter().any(|r| r.id == d.id) {
                definitions.push(d);
            }
        }
    }
    definitions.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(definitions)
}
fn sign_label(
    dir: &Path,
    op: &mut HomeOperation,
    identity: &DeviceIdentity,
    keys: &Keys,
    use_local: bool,
) -> Result<(), String> {
    let HomeOperationKind::Label {
        new_label,
        affected_definition_ids,
        unresolved_scope,
    } = &mut op.kind
    else {
        return Err("invalid label operation".into());
    };
    if unresolved_scope.is_some()
        || keys.public_key().to_hex() != op.owner_pubkey
        || !op.signed_events.is_empty()
    {
        return Ok(());
    }
    let definitions =
        scoped_definitions(dir, &op.relay_url, &op.owner_pubkey, identity, use_local)?;
    let db = scoped_retention_db_path(&dir.join("agents"), &op.relay_url, &op.owner_pubkey);
    let conn = if db.exists() {
        Some(
            Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };
    // Keep exactly the recorded worklist; later deleted/moved definitions are dropped.
    affected_definition_ids.retain(|id| definitions.iter().any(|d| &d.id == id));
    for id in affected_definition_ids.iter() {
        let mut d = definitions
            .iter()
            .find(|d| &d.id == id)
            .cloned()
            .ok_or_else(|| "label definition missing".to_string())?;
        d.origin_device_label = Some(new_label.clone());
        let prior = conn
            .as_ref()
            .map(|c| super::super::retention::get_retained_event(c, 30175, &op.owner_pubkey, id))
            .transpose()?
            .flatten();
        let event = super::super::persona_events::build_persona_event(&d)?
            .custom_created_at(super::super::persona_events::monotonic_created_at(
                prior.as_ref().map(|r| r.created_at),
            ))
            .sign_with_keys(keys)
            .map_err(|e| e.to_string())?;
        op.signed_events.push(event);
    }
    Ok(())
}
/// Recover identity/store update from the durable latest label intent before publication.
pub(super) fn apply_label(
    dir: &Path,
    raw: &mut [ManagedAgentRecord],
    label: &str,
) -> Result<(), String> {
    let mut device = load_existing_device_identity(&dir.join("device.json"))?;
    if device.label != label {
        device.label = label.into();
        atomic_write_json_restricted(
            &dir.join("device.json"),
            &serde_json::to_vec(&device).map_err(|e| e.to_string())?,
        )?;
    }
    let mut changed = false;
    for r in raw.iter_mut().filter(|r| {
        r.pubkey.is_empty() && r.origin_device_id.as_deref() == Some(device.device_id.as_str())
    }) {
        if r.origin_device_label.as_deref() != Some(label) {
            r.origin_device_label = Some(label.into());
            changed = true;
        }
    }
    if changed {
        save_unified(dir, raw)?;
    }
    Ok(())
}
/// Save all scope intents before device metadata. Enqueue failures leave retry worklists.
/// `true` means publication is still queued for existing event-sync, even after durable enqueue.
pub(crate) fn set_label_in_dir(
    dir: &Path,
    identity: DeviceIdentity,
    label: &str,
    scopes: &[(String, String)],
    keys: &Keys,
    enqueue: impl FnMut(&HomeOperation) -> Result<(), String>,
) -> Result<bool, String> {
    set_label_with_metadata(dir, identity, label, scopes, keys, enqueue, apply_label)
}
/// Inject only device/store metadata writes; journal signing/ordering and SQL batching stay real.
pub(crate) fn set_label_with_metadata(
    dir: &Path,
    identity: DeviceIdentity,
    label: &str,
    scopes: &[(String, String)],
    keys: &Keys,
    mut enqueue: impl FnMut(&HomeOperation) -> Result<(), String>,
    apply: impl FnOnce(&Path, &mut [ManagedAgentRecord], &str) -> Result<(), String>,
) -> Result<bool, String> {
    let label = label.trim();
    if label.is_empty() {
        return Err("device identity label must be nonempty".into());
    }
    let mut ops = read_journal(dir)?;
    let first = ops.len();
    for (index, (relay, owner)) in scopes.iter().enumerate() {
        let definitions = scoped_definitions(dir, relay, owner, &identity, index == 0)?;
        let ids = definitions.iter().map(|d| d.id.clone()).collect::<Vec<_>>();
        if ids.is_empty() && index != 0 {
            continue;
        }
        let mut op = HomeOperation {
            id: uuid::Uuid::new_v4().to_string(),
            relay_url: relay.clone(),
            owner_pubkey: owner.clone(),
            kind: HomeOperationKind::Label {
                new_label: label.into(),
                affected_definition_ids: ids,
                unresolved_scope: None,
            },
            signed_events: vec![],
        };
        sign_label(dir, &mut op, &identity, keys, index == 0)?;
        ops.push(op);
    }
    for (file, scope) in known_retention_scopes(&dir.join("agents"))? {
        if scope.is_none() {
            ops.push(HomeOperation {
                id: uuid::Uuid::new_v4().to_string(),
                relay_url: String::new(),
                owner_pubkey: String::new(),
                kind: HomeOperationKind::Label {
                    new_label: label.into(),
                    affected_definition_ids: vec![],
                    unresolved_scope: Some(file),
                },
                signed_events: vec![],
            });
        }
    }
    if ops.len() == first {
        ops.push(HomeOperation {
            id: uuid::Uuid::new_v4().to_string(),
            relay_url: String::new(),
            owner_pubkey: keys.public_key().to_hex(),
            kind: HomeOperationKind::Label {
                new_label: label.into(),
                affected_definition_ids: vec![],
                unresolved_scope: None,
            },
            signed_events: vec![],
        });
    }
    save_journal(dir, &ops)?;
    let mut raw = read_policy_records(&dir.join("agents/managed-agents.json"))?;
    apply(dir, &mut raw, label)?;
    let queued = ops.iter().any(|op| match &op.kind {
        HomeOperationKind::Label {
            affected_definition_ids,
            unresolved_scope,
            ..
        } => !affected_definition_ids.is_empty() || unresolved_scope.is_some(),
        _ => false,
    });
    let mut done = vec![];
    for op in &ops[first..] {
        let empty_worklist = match &op.kind {
            HomeOperationKind::Label {
                affected_definition_ids,
                unresolved_scope: None,
                ..
            } => affected_definition_ids.is_empty(),
            _ => false,
        };
        if empty_worklist || (!op.signed_events.is_empty() && enqueue(op).is_ok()) {
            done.push(op.id.clone());
        }
    }
    ops.retain(|op| !done.contains(&op.id));
    save_journal(dir, &ops)?;
    Ok(queued)
}
/// Unsigned other-owner/unresolved worklists become signed only with that scope's keys.
/// Save the signed intent before enqueue; retries after that point never re-sign.
pub(crate) fn prepare_label_retries(dir: &Path, keys: &Keys) -> Result<(), String> {
    let mut ops = read_journal(dir)?;
    if !ops
        .iter()
        .any(|op| matches!(op.kind, HomeOperationKind::Label { .. }))
    {
        return Ok(());
    }
    let identity = load_existing_device_identity(&dir.join("device.json"))?;
    let original = serde_json::to_vec(&ops).map_err(|e| e.to_string())?;
    for op in &mut ops {
        if let HomeOperationKind::Label {
            unresolved_scope,
            affected_definition_ids,
            ..
        } = &mut op.kind
        {
            if let Some(file) = unresolved_scope.as_ref() {
                // Filename is a plain .db basename, never an arbitrary path from the journal.
                if Path::new(file).file_name().and_then(|f| f.to_str()) != Some(file.as_str())
                    || !file.ends_with(".db")
                {
                    return Err("invalid unresolved retention scope".into());
                }
                let path = dir.join("agents/retention").join(file);
                if let Some((relay, owner)) = read_retention_scope(&path)? {
                    if scoped_retention_db_path(&dir.join("agents"), &relay, &owner) != path {
                        return Err("resolved retention scope mismatch".into());
                    }
                    *affected_definition_ids =
                        scoped_definitions(dir, &relay, &owner, &identity, false)?
                            .iter()
                            .map(|d| d.id.clone())
                            .collect();
                    op.relay_url = relay;
                    op.owner_pubkey = owner;
                    *unresolved_scope = None;
                }
            }
            // Retry worklists belong to retained original scopes. A matching raw ID may
            // belong to another owner/relay and must not resurrect a deleted/moved head.
            sign_label(dir, op, &identity, keys, false)?;
        }
    }
    if serde_json::to_vec(&ops).map_err(|e| e.to_string())? != original {
        save_journal(dir, &ops)?;
    }
    Ok(())
}
/// Native label command adapter; caller holds the managed store lock.
pub(crate) fn set_device_label_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    state: &crate::app_state::AppState,
    label: &str,
) -> Result<crate::commands::device_identity::DeviceLabelResult, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let identity = load_existing_device_identity(&dir.join("device.json"))?;
    let active = super::super::retention::active_retention_scope(app, state)?;
    let conn = super::super::retention::open_retention_db(&active.db_path)?;
    super::super::retention::remember_retention_scope(
        &conn,
        &active.relay_url,
        &active.owner_keys.public_key().to_hex(),
    )?;
    let mut scopes = vec![(
        active.relay_url.clone(),
        active.owner_keys.public_key().to_hex(),
    )];
    for (_, scope) in known_retention_scopes(&dir.join("agents"))? {
        if let Some(scope) = scope {
            if !scopes.contains(&scope) {
                scopes.push(scope);
            }
        }
    }
    let queued = set_label_in_dir(&dir, identity, label, &scopes, &active.owner_keys, |op| {
        enqueue_in_scope(&dir, op)
    })?;
    Ok(crate::commands::device_identity::DeviceLabelResult {
        identity: load_existing_device_identity(&dir.join("device.json"))?,
        publication: if queued {
            crate::commands::device_identity::DeviceLabelPublication::Queued
        } else {
            crate::commands::device_identity::DeviceLabelPublication::Complete
        },
    })
}
