use super::*;
use crate::managed_agents::{
    definition_home::EvidenceReadiness,
    device_authority::{prepare_deletion_authority_locked_with, save_deletion_snapshot},
    device_home_migration::tests::{app, context, records, write},
    device_home_operations::{
        delete::prepare_home_delete_authorized_locked, recover_home_operations_locked_with,
    },
    device_home_sync,
    retention::*,
};
use nostr::{Event, JsonUtil};
use tauri::Manager;
#[test]
fn definition_cascade_retry_keeps_instance_tombstone_archive_without_release_resurrection() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<AppState>();
    let base = crate::managed_agents::managed_agents_base_dir(app.handle()).unwrap();
    let (mut raw, _) = records();
    let mut c = context(EvidenceReadiness::Ready);
    c.scope = device_home_sync::capture_scope(&state).unwrap();
    raw[0].origin_device_id = Some(c.device.device_id.clone());
    raw[0].origin_released = Some(false);
    raw[1].device_host_binding = Some(c.proof.binding().into());
    let target = raw[1].pubkey.clone();
    write(&base, &raw);
    let permit =
        prepare_deletion_authority_locked_with(app.handle(), &state, &target, |_, _| Ok(c))
            .unwrap();
    let operation = prepare_home_delete_authorized_locked(app.handle(), &permit).unwrap();
    assert!(operation
        .signed_events
        .iter()
        .any(|e| e.kind.as_u16() == 30175));
    let mut expected_ids: std::collections::BTreeSet<_> = operation
        .signed_events
        .iter()
        .filter(|event| event.kind.as_u16() != 30175)
        .map(|event| event.id)
        .collect();
    let cascade = [target.clone()].into_iter().collect();
    commit_cascade_agents(&mut raw, &cascade, |r| {
        save_deletion_snapshot(app.handle(), r)
    })
    .unwrap();
    raw.retain(|r| r.slug.as_deref() != Some("one"));
    save_deletion_snapshot(app.handle(), &raw).unwrap();
    let path = scoped_retention_db_path(
        &base,
        &permit.scope().relay_url,
        &permit.scope().owner_pubkey,
    );
    let conn = open_retention_db(&path).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_archive BEFORE INSERT ON persona_events WHEN NEW.kind=9035 BEGIN SELECT RAISE(ABORT,'cascade archive failure');END;").unwrap();
    assert!(complete_cascade_home_operations(app.handle(), &[operation]).is_err());
    assert!(get_pending_sync(&conn).unwrap().is_empty());
    conn.execute_batch("DROP TRIGGER reject_archive").unwrap();
    tombstone_persona_at(&path, &state.signing_keys().unwrap(), "one").unwrap();
    let definition_tombstone = get_pending_sync(&conn).unwrap();
    assert_eq!(definition_tombstone.len(), 1);
    assert_eq!(definition_tombstone[0].kind, 5);
    expected_ids.insert(
        Event::from_json(&definition_tombstone[0].raw_event)
            .unwrap()
            .id,
    );
    recover_home_operations_locked_with(app.handle(), |_| panic!("removed cascade target proof"))
        .unwrap();
    let pending = get_pending_sync(&conn).unwrap();
    assert_eq!(
        pending
            .iter()
            .map(|row| Event::from_json(&row.raw_event).unwrap().id)
            .collect::<std::collections::BTreeSet<_>>(),
        expected_ids,
        "recovery after the owner definition tombstone must keep only the original instance pair",
    );
    assert!(pending.iter().all(|r| r.kind != 30175));
    assert!(pending.iter().any(|r| r.kind == 9035));
    assert_eq!(pending.iter().filter(|r| r.kind == 5).count(), 2);
    assert!(
        crate::managed_agents::persona_device_view::read_policy_records(
            &base.join("managed-agents.json")
        )
        .unwrap()
        .is_empty()
    );
}
