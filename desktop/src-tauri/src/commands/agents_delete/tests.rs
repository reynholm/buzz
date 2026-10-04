use super::*;
use crate::managed_agents::{
    definition_home::EvidenceReadiness,
    device_authority::prepare_deletion_authority_locked_with,
    device_home_migration::tests::{app, context, records, write},
    device_home_operations::{
        delete::prepare_home_delete_authorized_locked, recover_home_operations_locked_with,
    },
    device_home_sync,
    retention::*,
};
use nostr::JsonUtil;
use std::cell::Cell;
use tauri::Manager;
#[test]
fn prepared_private_command_failure_replays_original_scope_and_preserves_copied_sibling() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<AppState>();
    let base = managed_agents_base_dir(app.handle()).unwrap();
    let (mut raw, _) = records();
    let mut c = context(EvidenceReadiness::Ready);
    c.scope = device_home_sync::capture_scope(&state).unwrap();
    raw[0].origin_device_id = Some(c.device.device_id.clone());
    raw[0].origin_released = Some(false);
    raw[1].device_host_binding = Some(c.proof.binding().into());
    let binding = c.proof.binding().to_string();
    let target = raw[1].pubkey.clone();
    let mut copied = raw[1].clone();
    copied.pubkey = nostr::Keys::generate().public_key().to_hex();
    copied.private_key_nsec = "copied-inline-secret".into();
    copied.device_host_binding = Some("another host".into());
    raw.push(copied.clone());
    write(&base, &raw);
    let permit =
        prepare_deletion_authority_locked_with(app.handle(), &state, &target, |_, _| Ok(c))
            .unwrap();
    let operation = prepare_home_delete_authorized_locked(app.handle(), &permit).unwrap();
    let ids = operation
        .signed_events
        .iter()
        .map(|e| e.id)
        .collect::<Vec<_>>();
    let path = scoped_retention_db_path(
        &base,
        &permit.scope().relay_url,
        &permit.scope().owner_pubkey,
    );
    let mut conn = open_retention_db(&path).unwrap();
    crate::managed_agents::bestie_assignment::replace_assignment(&mut conn, &target).unwrap();
    conn.execute_batch("CREATE TRIGGER fail_archive BEFORE INSERT ON persona_events WHEN NEW.kind=9035 BEGIN SELECT RAISE(ABORT,'command archive failure');END;").unwrap();
    let stopped = Cell::new(false);
    let cleaned = Cell::new(false);
    assert!(commit_prepared_agent_delete_with(
        app.handle(),
        &state,
        &permit,
        &operation,
        &mut raw,
        |_| {
            stopped.set(true);
            Ok(())
        },
        || cleaned.set(true)
    )
    .is_err());
    assert!(stopped.get());
    assert!(cleaned.get());
    assert!(get_pending_sync(&conn).unwrap().is_empty());
    assert!(
        crate::managed_agents::bestie_assignment::get_assignment(&conn)
            .unwrap()
            .is_none()
    );
    let current = crate::managed_agents::persona_device_view::read_policy_records(
        &base.join("managed-agents.json"),
    )
    .unwrap();
    assert!(!current.iter().any(|r| r.pubkey == target));
    assert_eq!(
        serde_json::to_value(current.iter().find(|r| r.pubkey == copied.pubkey).unwrap()).unwrap(),
        serde_json::to_value(copied).unwrap()
    );
    assert!(current[0].pubkey.is_empty());
    assert!(current[0].private_key_nsec.is_empty());
    conn.execute_batch("DROP TRIGGER fail_archive").unwrap();
    *state.keys.lock().unwrap() = nostr::Keys::generate();
    *state.relay_url_override.lock().unwrap() = Some("wss://switched".into());
    recover_home_operations_locked_with(app.handle(), |b| Ok(b == binding)).unwrap();
    let pending = get_pending_sync(&conn).unwrap();
    assert_eq!(pending.len(), 2);
    let replay = pending
        .iter()
        .map(|r| nostr::Event::from_json(&r.raw_event).unwrap().id)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(replay, ids.into_iter().collect());
    let switched = active_retention_scope(app.handle(), &state).unwrap();
    assert!(!switched.db_path.exists());
}
#[test]
fn prepared_last_instance_command_queues_release_and_stop_error_preserves_record() {
    for stop_error in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<AppState>();
        let base = managed_agents_base_dir(app.handle()).unwrap();
        let (mut raw, _) = records();
        let mut c = context(EvidenceReadiness::Ready);
        c.scope = device_home_sync::capture_scope(&state).unwrap();
        raw[0].origin_device_id = Some(c.device.device_id.clone());
        raw[0].origin_released = Some(false);
        raw[1].device_host_binding = Some(c.proof.binding().into());
        let target = raw[1].pubkey.clone();
        write(&base, &raw);
        let before = std::fs::read(base.join("managed-agents.json")).unwrap();
        let permit =
            prepare_deletion_authority_locked_with(app.handle(), &state, &target, |_, _| Ok(c))
                .unwrap();
        let operation = prepare_home_delete_authorized_locked(app.handle(), &permit).unwrap();
        assert_eq!(
            serde_json::from_slice::<
                Vec<crate::managed_agents::device_home_operations::HomeOperation>,
            >(&std::fs::read(dir.path().join("device-home-operations.json")).unwrap())
            .unwrap()
            .len(),
            1
        );
        let db_path = scoped_retention_db_path(
            &base,
            &permit.scope().relay_url,
            &permit.scope().owner_pubkey,
        );
        let mut assignments_conn = open_retention_db(&db_path).unwrap();
        crate::managed_agents::bestie_assignment::replace_assignment(
            &mut assignments_conn,
            &target,
        )
        .unwrap();
        let cleaned = Cell::new(false);
        let result = commit_prepared_agent_delete_with(
            app.handle(),
            &state,
            &permit,
            &operation,
            &mut raw,
            |_| {
                if stop_error {
                    Err("stop failure".into())
                } else {
                    Ok(())
                }
            },
            || cleaned.set(true),
        );
        let conn = open_retention_db(&scoped_retention_db_path(
            &base,
            &permit.scope().relay_url,
            &permit.scope().owner_pubkey,
        ))
        .unwrap();
        if stop_error {
            assert!(result.is_err());
            assert!(!cleaned.get());
            assert!(
                crate::managed_agents::bestie_assignment::get_assignment(&assignments_conn)
                    .unwrap()
                    .is_some()
            );
            assert_eq!(
                before,
                std::fs::read(base.join("managed-agents.json")).unwrap()
            );
            assert!(get_pending_sync(&conn).unwrap().is_empty());
        } else {
            result.unwrap();
            assert!(cleaned.get());
            assert!(
                crate::managed_agents::bestie_assignment::get_assignment(&assignments_conn)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                get_pending_sync(&conn)
                    .unwrap()
                    .iter()
                    .map(|r| r.kind)
                    .collect::<std::collections::BTreeSet<_>>(),
                [5, 9035, 30175].into_iter().collect()
            );
            let raw = crate::managed_agents::persona_device_view::read_policy_records(
                &base.join("managed-agents.json"),
            )
            .unwrap();
            assert_eq!(raw.len(), 1);
            assert_eq!(raw[0].origin_released, Some(true));
        }
    }
}
