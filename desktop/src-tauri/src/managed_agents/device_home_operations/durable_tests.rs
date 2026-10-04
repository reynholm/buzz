use super::super::retention::{get_pending_sync, open_retention_db};
use super::tests::{save, setup};
use super::*;
use std::cell::Cell;
fn home() -> (
    tempfile::TempDir,
    DevicePolicyContext,
    ManagedAgentRecord,
    Keys,
) {
    let (dir, c, i, k) = setup();
    commit_claim_in_dir(
        dir.path(),
        &c,
        "one",
        i.clone(),
        &k,
        |r| save(dir.path(), r),
        |_| Ok(()),
    )
    .unwrap();
    (dir, c, i, k)
}
#[test]
fn last_instance_delete_queues_release_and_tombstone_atomically() {
    let (dir, c, i, k) = home();
    let op = delete::prepare_delete_in_dir(dir.path(), &c, &i.pubkey, &k).unwrap();
    let mut conn = open_retention_db(&dir.path().join("test.db")).unwrap();
    delete::commit_delete_in_dir(
        dir.path(),
        &op,
        |r| save(dir.path(), r),
        |op| enqueue_home_events(&mut conn, op),
    )
    .unwrap();
    let r = read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].origin_released, Some(true));
    let pending = get_pending_sync(&conn).unwrap();
    assert_eq!(
        pending
            .iter()
            .map(|e| e.kind)
            .collect::<std::collections::BTreeSet<_>>(),
        [5, 9035, 30175].into_iter().collect()
    );
    assert!(pending
        .iter()
        .find(|e| e.kind == 9035)
        .unwrap()
        .content
        .contains("one"));
}
#[test]
fn partial_delete_and_stop_do_not_release() {
    let (dir, c, i, k) = home();
    let mut r = read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
    r[1].runtime_pid = None;
    save(dir.path(), &r).unwrap();
    assert_eq!(r[0].origin_released, Some(false));
    assert!(read_journal(dir.path()).unwrap().is_empty());
    let mut sibling = r[1].clone();
    sibling.pubkey = Keys::generate().public_key().to_hex();
    r.push(sibling.clone());
    save(dir.path(), &r).unwrap();
    let op = delete::prepare_delete_in_dir(dir.path(), &c, &i.pubkey, &k).unwrap();
    assert!(!op.signed_events.iter().any(|e| e.kind.as_u16() == 30175));
    delete::commit_delete_in_dir(dir.path(), &op, |r| save(dir.path(), r), |_| Ok(())).unwrap();
    let r = read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
    assert_eq!(r[0].origin_released, Some(false));
    assert!(r.iter().any(|r| r.pubkey == sibling.pubkey));
}
#[test]
fn recovery_reuses_signed_event_ids() {
    let (dir, c, i, k) = home();
    let op = delete::prepare_delete_in_dir(dir.path(), &c, &i.pubkey, &k).unwrap();
    let ids: Vec<_> = op.signed_events.iter().map(|e| e.id).collect();
    assert!(delete::commit_delete_in_dir(
        dir.path(),
        &op,
        |r| save(dir.path(), r),
        |_| Err("offline".into())
    )
    .is_err());
    let mut conn = open_retention_db(&dir.path().join("test.db")).unwrap();
    recover_in_dir(
        dir.path(),
        |b| Ok(c.proof.matches(b)),
        |op| {
            assert_eq!(
                op.signed_events.iter().map(|e| e.id).collect::<Vec<_>>(),
                ids
            );
            assert_eq!(op.relay_url, c.scope.relay_url);
            enqueue_home_events(&mut conn, op)
        },
    )
    .unwrap();
    assert_eq!(get_pending_sync(&conn).unwrap().len(), 3);
    assert!(read_journal(dir.path()).unwrap().is_empty());
}
#[test]
fn new_instance_cancels_obsolete_release() {
    let (dir, c, i, k) = home();
    let op = delete::prepare_delete_in_dir(dir.path(), &c, &i.pubkey, &k).unwrap();
    assert!(delete::commit_delete_in_dir(
        dir.path(),
        &op,
        |r| save(dir.path(), r),
        |_| Err("offline".into())
    )
    .is_err());
    let mut r = read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
    let mut sibling = i.clone();
    sibling.pubkey = Keys::generate().public_key().to_hex();
    sibling.device_host_binding = Some(c.proof.binding().into());
    r.push(sibling.clone());
    save(dir.path(), &r).unwrap();
    let mut conn = open_retention_db(&dir.path().join("test.db")).unwrap();
    recover_in_dir(
        dir.path(),
        |b| Ok(c.proof.matches(b)),
        |op| enqueue_home_events(&mut conn, op),
    )
    .unwrap();
    let r = read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
    assert!(r.iter().any(|r| r.pubkey == sibling.pubkey));
    assert_eq!(r[0].origin_released, Some(false));
    assert!(!get_pending_sync(&conn)
        .unwrap()
        .iter()
        .any(|e| e.kind == 30175));
    assert_eq!(get_pending_sync(&conn).unwrap().len(), 2);
}
#[test]
fn copied_instance_never_creates_home_operation() {
    let (dir, c, i, k) = home();
    let mut r = read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
    r[1].device_host_binding = Some("copied".into());
    save(dir.path(), &r).unwrap();
    let bytes = std::fs::read(dir.path().join("agents/managed-agents.json")).unwrap();
    assert!(delete::prepare_delete_in_dir(dir.path(), &c, &i.pubkey, &k).is_err());
    assert!(read_journal(dir.path()).unwrap().is_empty());
    assert_eq!(
        bytes,
        std::fs::read(dir.path().join("agents/managed-agents.json")).unwrap()
    );
}
#[test]
fn label_failure_keeps_retry_worklist() {
    let (dir, mut c, _, k) = home();
    c.device = crate::device_identity::load_or_create_device_identity(
        &dir.path().join("device.json"),
        "Before",
    )
    .unwrap();
    let mut r = read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
    r[0].origin_device_id = Some(c.device.device_id.clone());
    save(dir.path(), &r).unwrap();
    let hash = super::super::persona_events::persona_content_hash(
        &super::super::persona_events::persona_event_content(&r[0].to_definition_view().unwrap()),
    );
    let scopes = vec![
        (c.scope.relay_url.clone(), c.scope.owner_pubkey.clone()),
        ("wss://other".into(), c.scope.owner_pubkey.clone()),
    ];
    for (relay, owner) in &scopes {
        let path = super::super::retention::scoped_retention_db_path(
            &dir.path().join("agents"),
            relay,
            owner,
        );
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let conn = open_retention_db(&path).unwrap();
        super::super::retention::remember_retention_scope(&conn, relay, owner).unwrap();
        let event =
            super::super::persona_events::build_persona_event(&r[0].to_definition_view().unwrap())
                .unwrap()
                .sign_with_keys(&k)
                .unwrap();
        retain_event(
            &conn,
            &RetainedEvent {
                kind: 30175,
                pubkey: owner.clone(),
                d_tag: "one".into(),
                content: event.content.clone(),
                created_at: event.created_at.as_secs() as i64,
                raw_event: event.as_json(),
                pending_sync: false,
            },
        )
        .unwrap();
    }
    let calls = Cell::new(0);
    assert!(
        label::set_label_in_dir(dir.path(), c.device.clone(), " After ", &scopes, &k, |_| {
            calls.set(calls.get() + 1);
            Err("offline".into())
        })
        .unwrap()
    );
    assert_eq!(calls.get(), 2);
    assert_eq!(
        crate::device_identity::load_existing_device_identity(&dir.path().join("device.json"))
            .unwrap()
            .label,
        "After"
    );
    assert_eq!(read_journal(dir.path()).unwrap().len(), 2);
    let r = read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
    assert_eq!(
        hash,
        super::super::persona_events::persona_content_hash(
            &super::super::persona_events::persona_event_content(
                &r[0].to_definition_view().unwrap()
            )
        )
    );
    recover_in_dir(
        dir.path(),
        |_| panic!("label needs no instance proof"),
        |_| Ok(()),
    )
    .unwrap();
    assert!(read_journal(dir.path()).unwrap().is_empty());
}

#[test]
fn intent_save_and_each_sql_enqueue_failure_leave_no_partial_batch() {
    for fail in [0, 30175, 5, 9035] {
        let (dir, c, i, k) = home();
        let op = delete::prepare_delete_in_dir(dir.path(), &c, &i.pubkey, &k).unwrap();
        let before = std::fs::read(dir.path().join("agents/managed-agents.json")).unwrap();
        let mut conn = open_retention_db(&dir.path().join("failure.db")).unwrap();
        if fail != 0 {
            conn.execute_batch(&format!("CREATE TRIGGER reject_batch BEFORE INSERT ON persona_events WHEN NEW.kind={fail} BEGIN SELECT RAISE(ABORT,'injected enqueue'); END;")).unwrap();
        }
        let saved = Cell::new(false);
        let result = delete::commit_delete_with_journal(
            dir.path(),
            &op,
            |r| {
                saved.set(true);
                save(dir.path(), r)
            },
            |op| enqueue_home_events(&mut conn, op),
            |ops| {
                if fail == 0 {
                    Err("intent write failure".into())
                } else {
                    save_journal(dir.path(), ops)
                }
            },
        );
        assert!(result.is_err());
        assert!(get_pending_sync(&conn).unwrap().is_empty());
        if fail == 0 {
            assert!(!saved.get());
            assert_eq!(
                before,
                std::fs::read(dir.path().join("agents/managed-agents.json")).unwrap()
            );
            assert!(read_journal(dir.path()).unwrap().is_empty());
        } else {
            assert!(saved.get());
            assert_eq!(read_journal(dir.path()).unwrap().len(), 1);
            conn.execute_batch("DROP TRIGGER reject_batch").unwrap();
            recover_in_dir(
                dir.path(),
                |b| Ok(c.proof.matches(b)),
                |op| enqueue_home_events(&mut conn, op),
            )
            .unwrap();
            assert_eq!(get_pending_sync(&conn).unwrap().len(), 3);
        }
    }
}
#[test]
fn unified_save_failure_restarts_deletion_and_clear_failure_replays_exact_ids() {
    for fail_save in [true, false] {
        let (dir, c, i, k) = home();
        let op = delete::prepare_delete_in_dir(dir.path(), &c, &i.pubkey, &k).unwrap();
        let ids = op.signed_events.iter().map(|e| e.id).collect::<Vec<_>>();
        let mut conn = open_retention_db(&dir.path().join("failure.db")).unwrap();
        let writes = Cell::new(0);
        assert!(delete::commit_delete_with_journal(
            dir.path(),
            &op,
            |r| if fail_save {
                Err("unified save failure".into())
            } else {
                save(dir.path(), r)
            },
            |op| enqueue_home_events(&mut conn, op),
            |ops| {
                writes.set(writes.get() + 1);
                if writes.get() == 2 {
                    Err("journal clear failure".into())
                } else {
                    save_journal(dir.path(), ops)
                }
            }
        )
        .is_err());
        assert_eq!(read_journal(dir.path()).unwrap().len(), 1);
        if fail_save {
            assert!(get_pending_sync(&conn).unwrap().is_empty());
            assert!(
                read_policy_records(&dir.path().join("agents/managed-agents.json"))
                    .unwrap()
                    .iter()
                    .any(|r| r.pubkey == i.pubkey)
            );
        }
        recover_in_dir(
            dir.path(),
            |b| Ok(c.proof.matches(b)),
            |op| {
                assert_eq!(
                    ids,
                    op.signed_events.iter().map(|e| e.id).collect::<Vec<_>>()
                );
                enqueue_home_events(&mut conn, op)
            },
        )
        .unwrap();
        assert_eq!(get_pending_sync(&conn).unwrap().len(), 3);
        assert!(read_journal(dir.path()).unwrap().is_empty());
        assert!(
            !read_policy_records(&dir.path().join("agents/managed-agents.json"))
                .unwrap()
                .iter()
                .any(|r| r.pubkey == i.pubkey)
        );
    }
}
#[test]
fn interrupted_delete_copied_binding_and_changed_revision_fail_closed() {
    for copied in [false, true] {
        let (dir, c, i, k) = home();
        let op = delete::prepare_delete_in_dir(dir.path(), &c, &i.pubkey, &k).unwrap();
        assert!(delete::commit_delete_in_dir(
            dir.path(),
            &op,
            |_| Err("save failure".into()),
            |_| panic!("uncommitted delete enqueued")
        )
        .is_err());
        let before = std::fs::read(journal(dir.path())).unwrap();
        if copied {
            assert!(recover_in_dir(
                dir.path(),
                |_| Ok(false),
                |_| panic!("copied deletion published")
            )
            .is_err());
        } else {
            let mut raw =
                read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
            raw[1].name = "edited target".into();
            save(dir.path(), &raw).unwrap();
            recover_in_dir(
                dir.path(),
                |_| panic!("stale target proof read"),
                |_| panic!("stale deletion published"),
            )
            .unwrap();
        }
        assert_eq!(before, std::fs::read(journal(dir.path())).unwrap());
    }
}
#[test]
fn definition_cascade_recovery_never_resurrects_absent_definition() {
    let (dir, c, i, k) = home();
    let op = delete::prepare_delete_in_dir(dir.path(), &c, &i.pubkey, &k).unwrap();
    assert!(delete::commit_delete_in_dir(
        dir.path(),
        &op,
        |r| save(dir.path(), r),
        |_| Err("offline".into())
    )
    .is_err());
    save(dir.path(), &[]).unwrap();
    let mut conn = open_retention_db(&dir.path().join("cascade.db")).unwrap();
    recover_in_dir(
        dir.path(),
        |_| panic!("absent target proof"),
        |op| enqueue_home_events(&mut conn, op),
    )
    .unwrap();
    assert!(
        read_policy_records(&dir.path().join("agents/managed-agents.json"))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        get_pending_sync(&conn)
            .unwrap()
            .iter()
            .map(|r| r.kind)
            .collect::<std::collections::BTreeSet<_>>(),
        [5, 9035].into_iter().collect()
    );
    let _ = (c, i, k);
}
#[test]
fn premetadata_label_worklist_resolves_when_original_database_becomes_known() {
    let (dir, mut c, _, k) = home();
    c.device = crate::device_identity::load_or_create_device_identity(
        &dir.path().join("device.json"),
        "Before",
    )
    .unwrap();
    let mut r = read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
    r[0].origin_device_id = Some(c.device.device_id.clone());
    save(dir.path(), &r).unwrap();
    let relay = "wss://old-scope";
    let owner = k.public_key().to_hex();
    let path = super::super::retention::scoped_retention_db_path(
        &dir.path().join("agents"),
        relay,
        &owner,
    );
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut conn = open_retention_db(&path).unwrap();
    let e = super::super::persona_events::build_persona_event(&r[0].to_definition_view().unwrap())
        .unwrap()
        .sign_with_keys(&k)
        .unwrap();
    retain_event(
        &conn,
        &RetainedEvent {
            kind: 30175,
            pubkey: owner.clone(),
            d_tag: "one".into(),
            content: e.content.clone(),
            created_at: e.created_at.as_secs() as i64,
            raw_event: e.as_json(),
            pending_sync: false,
        },
    )
    .unwrap();
    assert!(
        label::set_label_in_dir(dir.path(), c.device.clone(), "New", &[], &k, |_| panic!(
            "unknown relay publication"
        ))
        .unwrap()
    );
    assert_eq!(read_journal(dir.path()).unwrap().len(), 1);
    label::prepare_label_retries(dir.path(), &k).unwrap();
    recover_in_dir(
        dir.path(),
        |_| panic!("label proof"),
        |_| panic!("unresolved label published"),
    )
    .unwrap();
    super::super::retention::remember_retention_scope(&conn, relay, &owner).unwrap();
    label::prepare_label_retries(dir.path(), &k).unwrap();
    let pending = read_journal(dir.path()).unwrap();
    let known = pending.iter().find(|op| op.relay_url == relay).unwrap();
    assert_eq!(known.signed_events.len(), 1);
    let id = known.signed_events[0].id;
    recover_in_dir(
        dir.path(),
        |_| panic!("label proof"),
        |op| enqueue_home_events(&mut conn, op),
    )
    .unwrap();
    let pending = get_pending_sync(&conn).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(Event::from_json(&pending[0].raw_event).unwrap().id, id);
    assert!(pending[0].content.contains("New"));
    assert!(read_journal(dir.path()).unwrap().is_empty());
}
#[test]
fn label_other_owner_waits_for_its_keys_and_foreign_origins_are_not_rebuilt() {
    let (dir, mut c, _, k) = home();
    c.device = crate::device_identity::load_or_create_device_identity(
        &dir.path().join("device.json"),
        "Before",
    )
    .unwrap();
    let other = Keys::generate();
    let owner = other.public_key().to_hex();
    let relay = "wss://other-owner";
    let mut r = read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
    r[0].origin_device_id = Some(c.device.device_id.clone());
    save(dir.path(), &r).unwrap();
    let path = super::super::retention::scoped_retention_db_path(
        &dir.path().join("agents"),
        relay,
        &owner,
    );
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut conn = open_retention_db(&path).unwrap();
    super::super::retention::remember_retention_scope(&conn, relay, &owner).unwrap();
    for foreign in [false, true] {
        let mut d = r[0].to_definition_view().unwrap();
        if foreign {
            d.id = "foreign".into();
            d.origin_device_id = Some("another device".into());
        }
        let e = super::super::persona_events::build_persona_event(&d)
            .unwrap()
            .sign_with_keys(&other)
            .unwrap();
        retain_event(
            &conn,
            &RetainedEvent {
                kind: 30175,
                pubkey: owner.clone(),
                d_tag: d.id,
                content: e.content.clone(),
                created_at: e.created_at.as_secs() as i64,
                raw_event: e.as_json(),
                pending_sync: false,
            },
        )
        .unwrap();
    }
    assert!(label::set_label_in_dir(
        dir.path(),
        c.device.clone(),
        "New",
        &[(relay.into(), owner.clone())],
        &k,
        |_| panic!("wrong-owner signed label")
    )
    .unwrap());
    assert!(read_journal(dir.path())
        .unwrap()
        .iter()
        .all(|op| op.signed_events.is_empty()));
    label::prepare_label_retries(dir.path(), &other).unwrap();
    recover_in_dir(
        dir.path(),
        |_| panic!("label proof"),
        |op| enqueue_home_events(&mut conn, op),
    )
    .unwrap();
    let pending = get_pending_sync(&conn).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].d_tag, "one");
    assert_eq!(pending[0].pubkey, owner);
}

#[test]
fn native_label_adapter_reports_queued_until_existing_event_sync_acknowledges() {
    use tauri::Manager;
    let dir = tempfile::tempdir().unwrap();
    let app = super::super::device_home_migration::tests::app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    let identity = crate::device_identity::load_or_create_device_identity(
        &dir.path().join("device.json"),
        "Before",
    )
    .unwrap();
    let (mut raw, _) = super::super::device_home_migration::tests::records();
    raw.pop();
    raw[0].origin_device_id = Some(identity.device_id.clone());
    save(dir.path(), &raw).unwrap();
    let result = label::set_device_label_locked(app.handle(), &state, " After ").unwrap();
    assert_eq!(result.identity.label, "After");
    assert!(matches!(
        result.publication,
        crate::commands::device_identity::DeviceLabelPublication::Queued
    ));
    let scope = super::super::retention::active_retention_scope(app.handle(), &state).unwrap();
    let conn = open_retention_db(&scope.db_path).unwrap();
    let pending = get_pending_sync(&conn).unwrap();
    assert_eq!(pending.len(), 1);
    assert!(pending[0].content.contains("After"));
    assert_eq!(std::fs::read(journal(dir.path())).unwrap(), b"[]");
    for e in pending {
        super::super::retention::mark_synced(
            &conn,
            e.kind,
            &e.pubkey,
            &e.d_tag,
            e.created_at,
            &e.content,
        )
        .unwrap();
    }
    assert!(get_pending_sync(&conn).unwrap().is_empty());
}
#[test]
fn claim_recovery_finishes_enqueue_without_mint() {
    use tauri::Manager;
    let dir = tempfile::tempdir().unwrap();
    let app = super::super::device_home_migration::tests::app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    let (mut raw, _) = super::super::device_home_migration::tests::records();
    let i = raw.pop().unwrap();
    save(dir.path(), &raw).unwrap();
    let mut c = super::super::device_home_migration::tests::context(
        super::super::definition_home::EvidenceReadiness::Ready,
    );
    c.scope = super::super::device_home_sync::capture_scope(&state).unwrap();
    assert!(commit_claim_in_dir(
        dir.path(),
        &c,
        "one",
        i.clone(),
        &state.signing_keys().unwrap(),
        |r| save(dir.path(), r),
        |_| Err("SQL unavailable".into())
    )
    .is_err());
    let op = read_journal(dir.path()).unwrap().pop().unwrap();
    let ids = op
        .signed_events
        .iter()
        .map(|e| e.id)
        .collect::<std::collections::HashSet<_>>();
    let original_owner = c.scope.owner_pubkey.clone();
    let original_relay = c.scope.relay_url.clone();
    *state.keys.lock().unwrap() = Keys::generate();
    *state.relay_url_override.lock().unwrap() = Some("wss://switched".into());
    recover_home_operations_locked_with(app.handle(), |b| Ok(c.proof.matches(b))).unwrap();
    let path = super::super::retention::scoped_retention_db_path(
        &dir.path().join("agents"),
        &original_relay,
        &original_owner,
    );
    let conn = open_retention_db(&path).unwrap();
    let pending = get_pending_sync(&conn).unwrap();
    assert_eq!(
        pending
            .iter()
            .map(|r| Event::from_json(&r.raw_event).unwrap().id)
            .collect::<std::collections::HashSet<_>>(),
        ids
    );
    let raw = read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
    assert_eq!(raw.iter().filter(|r| r.pubkey == i.pubkey).count(), 1);
    assert!(read_journal(dir.path()).unwrap().is_empty());
}

#[test]
fn label_metadata_prefix_failures_recover_identity_and_store_before_publication() {
    for device_saved in [false, true] {
        let (dir, mut c, _, k) = home();
        c.device = crate::device_identity::load_or_create_device_identity(
            &dir.path().join("device.json"),
            "Before",
        )
        .unwrap();
        let mut r = read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
        r[0].origin_device_id = Some(c.device.device_id.clone());
        save(dir.path(), &r).unwrap();
        let scopes = [(c.scope.relay_url.clone(), c.scope.owner_pubkey.clone())];
        let enqueue = Cell::new(0);
        assert!(label::set_label_with_metadata(
            dir.path(),
            c.device.clone(),
            "New",
            &scopes,
            &k,
            |_| {
                enqueue.set(enqueue.get() + 1);
                Ok(())
            },
            |dir, _, label| {
                if device_saved {
                    let mut device = c.device.clone();
                    device.label = label.into();
                    atomic_write_json_restricted(
                        &dir.join("device.json"),
                        &serde_json::to_vec(&device).unwrap(),
                    )?;
                }
                Err(if device_saved {
                    "store save failure"
                } else {
                    "device save failure"
                }
                .into())
            }
        )
        .is_err());
        assert_eq!(enqueue.get(), 0);
        assert_eq!(read_journal(dir.path()).unwrap().len(), 1);
        let ids = read_journal(dir.path()).unwrap()[0]
            .signed_events
            .iter()
            .map(|e| e.id)
            .collect::<Vec<_>>();
        recover_in_dir(
            dir.path(),
            |_| panic!("label proof"),
            |op| {
                assert_eq!(
                    op.signed_events.iter().map(|e| e.id).collect::<Vec<_>>(),
                    ids
                );
                enqueue.set(enqueue.get() + 1);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(enqueue.get(), 1);
        assert_eq!(
            crate::device_identity::load_existing_device_identity(&dir.path().join("device.json"))
                .unwrap()
                .label,
            "New"
        );
        assert_eq!(
            read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap()[0]
                .origin_device_label
                .as_deref(),
            Some("New")
        );
        assert!(read_journal(dir.path()).unwrap().is_empty());
    }
}
#[test]
fn label_intent_write_failure_does_not_change_device_or_definitions() {
    let (dir, c, _, k) = home();
    let identity = crate::device_identity::load_or_create_device_identity(
        &dir.path().join("device.json"),
        "Before",
    )
    .unwrap();
    let before = std::fs::read(dir.path().join("device.json")).unwrap();
    let store = std::fs::read(dir.path().join("agents/managed-agents.json")).unwrap();
    std::fs::remove_file(journal(dir.path())).unwrap();
    std::fs::create_dir(journal(dir.path())).unwrap();
    assert!(label::set_label_in_dir(
        dir.path(),
        identity,
        "New",
        &[(c.scope.relay_url, c.scope.owner_pubkey)],
        &k,
        |_| panic!("failed intent enqueued")
    )
    .is_err());
    assert_eq!(
        before,
        std::fs::read(dir.path().join("device.json")).unwrap()
    );
    assert_eq!(
        store,
        std::fs::read(dir.path().join("agents/managed-agents.json")).unwrap()
    );
}

#[test]
fn wrong_owner_cannot_prepare_or_journal_delete() {
    let (dir, c, i, _) = home();
    let before = std::fs::read(dir.path().join("agents/managed-agents.json")).unwrap();
    assert!(delete::prepare_delete_in_dir(dir.path(), &c, &i.pubkey, &Keys::generate()).is_err());
    assert!(read_journal(dir.path()).unwrap().is_empty());
    assert_eq!(
        before,
        std::fs::read(dir.path().join("agents/managed-agents.json")).unwrap()
    );
}
#[test]
fn label_batch_rolls_back_all_definitions_and_replays_same_signed_ids() {
    let (dir, mut c, _, k) = home();
    c.device = crate::device_identity::load_or_create_device_identity(
        &dir.path().join("device.json"),
        "Before",
    )
    .unwrap();
    let mut raw = read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
    raw[0].origin_device_id = Some(c.device.device_id.clone());
    let mut second = raw[0].clone();
    second.slug = Some("two".into());
    raw.push(second);
    save(dir.path(), &raw).unwrap();
    let scopes = [(c.scope.relay_url.clone(), c.scope.owner_pubkey.clone())];
    let mut conn = open_retention_db(&dir.path().join("label.db")).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_second BEFORE INSERT ON persona_events WHEN NEW.d_tag='two' BEGIN SELECT RAISE(ABORT,'second label head failure');END;").unwrap();
    assert!(
        label::set_label_in_dir(dir.path(), c.device.clone(), "New", &scopes, &k, |op| {
            enqueue_home_events(&mut conn, op)
        })
        .unwrap()
    );
    assert!(get_pending_sync(&conn).unwrap().is_empty());
    let ids = read_journal(dir.path()).unwrap()[0]
        .signed_events
        .iter()
        .map(|e| e.id)
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 2);
    conn.execute_batch("DROP TRIGGER reject_second").unwrap();
    recover_in_dir(
        dir.path(),
        |_| panic!("label proof"),
        |op| {
            assert_eq!(
                ids,
                op.signed_events.iter().map(|e| e.id).collect::<Vec<_>>()
            );
            enqueue_home_events(&mut conn, op)
        },
    )
    .unwrap();
    assert_eq!(get_pending_sync(&conn).unwrap().len(), 2);
    assert!(read_journal(dir.path()).unwrap().is_empty());
}

#[test]
fn native_label_retry_never_rebuilds_deleted_or_foreign_scope_from_local_id_collision() {
    use tauri::Manager;
    for deleted in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let app = super::super::device_home_migration::tests::app(dir.path());
        let state = app.state::<crate::app_state::AppState>();
        let identity = crate::device_identity::load_or_create_device_identity(
            &dir.path().join("device.json"),
            "Before",
        )
        .unwrap();
        let active_keys = state.signing_keys().unwrap();
        let active = super::super::device_home_sync::capture_scope(&state).unwrap();
        let active_path = super::super::retention::scoped_retention_db_path(
            &dir.path().join("agents"),
            &active.relay_url,
            &active.owner_pubkey,
        );
        let active_conn = open_retention_db(&active_path).unwrap();
        super::super::retention::remember_retention_scope(
            &active_conn,
            &active.relay_url,
            &active.owner_pubkey,
        )
        .unwrap();
        let (mut raw, _) = super::super::device_home_migration::tests::records();
        raw.truncate(1);
        raw[0].origin_device_id = Some(identity.device_id.clone());
        save(dir.path(), &raw).unwrap();
        let other_keys = Keys::generate();
        let owner = other_keys.public_key().to_hex();
        let relay = "wss://recorded-label-scope";
        let path = super::super::retention::scoped_retention_db_path(
            &dir.path().join("agents"),
            relay,
            &owner,
        );
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let conn = open_retention_db(&path).unwrap();
        super::super::retention::remember_retention_scope(&conn, relay, &owner).unwrap();
        let mut d = raw[0].to_definition_view().unwrap();
        let event = super::super::persona_events::build_persona_event(&d)
            .unwrap()
            .sign_with_keys(&other_keys)
            .unwrap();
        let retain = |event: &Event| {
            retain_event(
                &conn,
                &RetainedEvent {
                    kind: 30175,
                    pubkey: owner.clone(),
                    d_tag: d.id.clone(),
                    content: event.content.clone(),
                    created_at: event.created_at.as_secs() as i64,
                    raw_event: event.as_json(),
                    pending_sync: false,
                },
            )
            .unwrap();
        };
        retain(&event);
        assert!(label::set_label_in_dir(
            dir.path(),
            identity,
            "New",
            &[
                (active.relay_url, active.owner_pubkey),
                (relay.into(), owner.clone()),
            ],
            &active_keys,
            |op| enqueue_in_scope(dir.path(), op),
        )
        .unwrap());
        assert_eq!(read_journal(dir.path()).unwrap().len(), 1);
        if deleted {
            conn.execute(
                "DELETE FROM persona_events WHERE kind=30175 AND pubkey=?1 AND d_tag=?2",
                rusqlite::params![owner, d.id],
            )
            .unwrap();
        } else {
            d.origin_device_id = Some("foreign-installation".into());
            let foreign = super::super::persona_events::build_persona_event(&d)
                .unwrap()
                .custom_created_at(nostr::Timestamp::from(event.created_at.as_secs() + 1))
                .sign_with_keys(&other_keys)
                .unwrap();
            retain(&foreign);
        }
        *state.keys.lock().unwrap() = other_keys;
        *state.relay_url_override.lock().unwrap() = Some(relay.into());
        recover_home_operations_locked_with(app.handle(), |_| panic!("label host proof")).unwrap();
        assert!(
            get_pending_sync(&conn).unwrap().is_empty(),
            "deleted/foreign original scope was rebuilt from local ID collision"
        );
        assert!(read_journal(dir.path()).unwrap().is_empty());
        let current = read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
        assert_eq!(current[0].origin_device_label.as_deref(), Some("New"));
    }
}
