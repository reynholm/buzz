use super::super::retention::{
    get_pending_sync, open_retention_db, remember_retention_scope, scoped_retention_db_path,
};
use super::tests::{save, setup};
use super::*;

fn setup_label() -> (
    tempfile::TempDir,
    crate::device_identity::DeviceIdentity,
    Keys,
    String,
) {
    let (dir, context, _, keys) = setup();
    let identity = crate::device_identity::load_or_create_device_identity(
        &dir.path().join("device.json"),
        "Before",
    )
    .unwrap();
    let mut raw = read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
    raw[0].origin_device_id = Some(identity.device_id.clone());
    save(dir.path(), &raw).unwrap();
    (dir, identity, keys, context.scope.relay_url)
}

fn retain_definition(dir: &Path, relay: &str, keys: &Keys, known: bool) -> Connection {
    let owner = keys.public_key().to_hex();
    let path = scoped_retention_db_path(&dir.join("agents"), relay, &owner);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let conn = open_retention_db(&path).unwrap();
    if known {
        remember_retention_scope(&conn, relay, &owner).unwrap();
    }
    let raw = read_policy_records(&dir.join("agents/managed-agents.json")).unwrap();
    let event =
        super::super::persona_events::build_persona_event(&raw[0].to_definition_view().unwrap())
            .unwrap()
            .custom_created_at(nostr::Timestamp::from(
                nostr::Timestamp::now().as_secs() + 600,
            ))
            .sign_with_keys(keys)
            .unwrap();
    retain_event(
        &conn,
        &RetainedEvent {
            kind: 30175,
            pubkey: owner,
            d_tag: "one".into(),
            content: event.content.clone(),
            created_at: event.created_at.as_secs() as i64,
            raw_event: event.as_json(),
            pending_sync: false,
        },
    )
    .unwrap();
    conn
}

fn assert_label(dir: &Path, expected: &str) {
    assert_eq!(
        crate::device_identity::load_existing_device_identity(&dir.join("device.json"))
            .unwrap()
            .label,
        expected
    );
    let raw = read_policy_records(&dir.join("agents/managed-agents.json")).unwrap();
    assert_eq!(raw[0].origin_device_label.as_deref(), Some(expected));
}

#[test]
fn newer_successful_label_survives_older_sql_enqueue_failure_and_recovery() {
    let (dir, identity, keys, relay) = setup_label();
    let owner = keys.public_key().to_hex();
    let mut conn = retain_definition(dir.path(), &relay, &keys, true);
    let scopes = [(relay, owner)];
    conn.execute_batch("CREATE TRIGGER reject_label BEFORE INSERT ON persona_events BEGIN SELECT RAISE(ABORT,'label enqueue failure'); END;").unwrap();
    label::set_label_in_dir(dir.path(), identity.clone(), "A", &scopes, &keys, |op| {
        enqueue_home_events(&mut conn, op)
    })
    .unwrap();
    assert_eq!(read_journal(dir.path()).unwrap().len(), 1);
    assert!(get_pending_sync(&conn).unwrap().is_empty());
    conn.execute_batch("DROP TRIGGER reject_label").unwrap();
    label::set_label_in_dir(dir.path(), identity, "B", &scopes, &keys, |op| {
        enqueue_home_events(&mut conn, op)
    })
    .unwrap();
    assert_label(dir.path(), "B");
    label::prepare_label_retries(dir.path(), &keys).unwrap();
    recover_in_dir(
        dir.path(),
        |_| panic!("label proof"),
        |op| enqueue_home_events(&mut conn, op),
    )
    .unwrap();
    assert_label(dir.path(), "B");
    assert!(get_pending_sync(&conn).unwrap()[0]
        .content
        .contains("\"origin_device_label\":\"B\""));
    assert!(read_journal(dir.path()).unwrap().is_empty());
}

#[test]
fn newer_label_preserves_other_relay_owner_and_unresolved_scope_worklists() {
    let (dir, identity, keys, relay) = setup_label();
    let other = Keys::generate();
    let owner = keys.public_key().to_hex();
    let other_owner = other.public_key().to_hex();
    let mut active = retain_definition(dir.path(), &relay, &keys, true);
    let other_relay = retain_definition(dir.path(), "wss://other-relay", &keys, true);
    let other_owner_db = retain_definition(dir.path(), "wss://other-owner", &other, true);
    let unresolved = retain_definition(dir.path(), "wss://unresolved", &keys, false);
    let scopes = [
        (relay.clone(), owner.clone()),
        ("wss://other-relay".into(), owner.clone()),
        ("wss://other-owner".into(), other_owner.clone()),
    ];
    label::set_label_in_dir(dir.path(), identity.clone(), "A", &scopes, &keys, |_| {
        Err("offline".into())
    })
    .unwrap();
    // A can already be signed with another owner's keys without reaching SQL.
    label::prepare_label_retries(dir.path(), &other).unwrap();
    let old = read_journal(dir.path()).unwrap();
    assert_eq!(old.len(), 4);
    label::set_label_in_dir(
        dir.path(),
        identity,
        "B",
        &[(relay.clone(), owner.clone())],
        &keys,
        |op| {
            if op.relay_url == relay {
                enqueue_home_events(&mut active, op)
            } else {
                Err("other relay offline".into())
            }
        },
    )
    .unwrap();
    let pending = read_journal(dir.path()).unwrap();
    assert_eq!(
        pending.len(),
        3,
        "one remaining worklist per original scope"
    );
    assert!(pending.iter().all(
        |op| matches!(&op.kind, HomeOperationKind::Label {new_label, ..} if new_label == "B")
    ));
    for signing_keys in [&keys, &other] {
        label::prepare_label_retries(dir.path(), signing_keys).unwrap();
        recover_in_dir(
            dir.path(),
            |_| panic!("label proof"),
            |op| {
                let prior = old
                    .iter()
                    .find(|old| {
                        old.relay_url == op.relay_url && old.owner_pubkey == op.owner_pubkey
                    })
                    .unwrap();
                for event in &op.signed_events {
                    assert!(event.created_at > prior.signed_events[0].created_at);
                    assert!(event.content.contains("\"origin_device_label\":\"B\""));
                    event.verify().unwrap();
                }
                enqueue_in_scope(dir.path(), op)
            },
        )
        .unwrap();
        assert_label(dir.path(), "B");
    }
    remember_retention_scope(&unresolved, "wss://unresolved", &owner).unwrap();
    label::prepare_label_retries(dir.path(), &keys).unwrap();
    recover_in_dir(
        dir.path(),
        |_| panic!("label proof"),
        |op| enqueue_in_scope(dir.path(), op),
    )
    .unwrap();
    assert_label(dir.path(), "B");
    for conn in [&active, &other_relay, &other_owner_db, &unresolved] {
        let pending = get_pending_sync(conn).unwrap();
        assert_eq!(pending.len(), 1);
        assert!(pending[0].content.contains("\"origin_device_label\":\"B\""));
    }
    assert!(read_journal(dir.path()).unwrap().is_empty());
}

#[test]
fn newer_label_is_signed_after_superseded_unenqueued_event() {
    let (dir, identity, keys, relay) = setup_label();
    let mut conn = retain_definition(dir.path(), &relay, &keys, true);
    let scopes = [(relay, keys.public_key().to_hex())];
    label::set_label_in_dir(dir.path(), identity.clone(), "A", &scopes, &keys, |_| {
        Err("offline".into())
    })
    .unwrap();
    let old = read_journal(dir.path()).unwrap()[0].signed_events[0].clone();
    label::set_label_in_dir(dir.path(), identity, "B", &scopes, &keys, |op| {
        enqueue_home_events(&mut conn, op)
    })
    .unwrap();
    let event = Event::from_json(&get_pending_sync(&conn).unwrap()[0].raw_event).unwrap();
    assert!(
        event.created_at > old.created_at,
        "newest label must dominate signed pending A"
    );
}
