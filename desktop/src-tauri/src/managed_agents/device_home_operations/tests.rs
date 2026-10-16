use super::super::{
    definition_home::EvidenceReadiness,
    device_home_migration::tests::{context, records, write},
    persona_device_view::read_policy_records,
    retention::{get_pending_sync, open_retention_db},
    storage::atomic_write_json_restricted,
};
use super::*;
use std::cell::Cell;
pub(super) fn setup() -> (
    tempfile::TempDir,
    DevicePolicyContext,
    ManagedAgentRecord,
    Keys,
) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("agents")).unwrap();
    let (mut rs, key) = records();
    let i = rs.pop().unwrap();
    rs[0].origin_device_id = Some("device A".into());
    rs[0].origin_device_label = Some("Device A".into());
    rs[0].origin_released = Some(true);
    write(&dir.path().join("agents"), &rs);
    let owner = Keys::generate();
    let mut c = context(EvidenceReadiness::Ready);
    c.device.label = "Device B".into();
    c.scope.owner_pubkey = owner.public_key().to_hex();
    let _ = key;
    (dir, c, i, owner)
}
pub(super) fn save(dir: &Path, rs: &[ManagedAgentRecord]) -> Result<(), String> {
    atomic_write_json_restricted(
        &dir.join("agents/managed-agents.json"),
        &serde_json::to_vec_pretty(rs).unwrap(),
    )
}
#[test]
fn released_definition_is_claimed_without_restart() {
    let (dir, c, i, owner) = setup();
    let saves = Cell::new(0);
    commit_claim_in_dir(
        dir.path(),
        &c,
        "one",
        i,
        &owner,
        |rs| {
            saves.set(saves.get() + 1);
            save(dir.path(), rs)
        },
        |_| Ok(()),
    )
    .unwrap();
    let rs = read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
    assert_eq!(rs.len(), 2);
    assert_eq!(saves.get(), 1);
    assert_eq!(
        rs[0].origin_device_id.as_deref(),
        Some(c.device.device_id.as_str())
    );
    assert_eq!(
        rs[0].origin_device_label.as_deref(),
        Some(c.device.label.as_str())
    );
    assert_eq!(rs[0].origin_released, Some(false));
    assert_eq!(
        rs[1].device_host_binding.as_deref(),
        Some(c.proof.binding())
    );
}
#[test]
fn claim_retention_failure_is_recoverable() {
    let (dir, c, i, owner) = setup();
    let pubkey = i.pubkey.clone();
    let result = commit_claim_in_dir(
        dir.path(),
        &c,
        "one",
        i,
        &owner,
        |rs| save(dir.path(), rs),
        |_| Err("retention unavailable".into()),
    );
    assert!(result.is_err());
    let journal = dir.path().join("device-home-operations.json");
    let bytes = std::fs::read(&journal).unwrap();
    let json = String::from_utf8(bytes.clone()).unwrap();
    assert!(!json.contains("private_key"));
    assert!(!json.contains("auth_tag"));
    assert!(!json.contains(c.proof.binding()));
    let operations: Vec<HomeOperation> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].signed_events.len(), 2);
    let mut conn = open_retention_db(&dir.path().join("pending.db")).unwrap();
    recover_in_dir(
        dir.path(),
        |binding| Ok(c.proof.matches(binding)),
        |op| enqueue_home_events(dir.path(), &mut conn, op),
    )
    .unwrap();
    assert_eq!(get_pending_sync(&conn).unwrap().len(), 2);
    let rs = read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
    assert_eq!(rs.iter().filter(|r| r.pubkey == pubkey).count(), 1);
    recover_in_dir(
        dir.path(),
        |binding| Ok(c.proof.matches(binding)),
        |_| panic!("second enqueue"),
    )
    .unwrap();
    assert_eq!(std::fs::read(journal).unwrap(), b"[]");
}
#[test]
fn failed_claim_save_never_publishes_uncommitted_intent() {
    let (dir, c, i, owner) = setup();
    let calls = Cell::new(0);
    assert!(commit_claim_in_dir(
        dir.path(),
        &c,
        "one",
        i,
        &owner,
        |_| Err("disk full".into()),
        |_| {
            calls.set(1);
            Ok(())
        }
    )
    .is_err());
    recover_in_dir(
        dir.path(),
        |binding| Ok(c.proof.matches(binding)),
        |_| {
            calls.set(1);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(calls.get(), 0);
    let journal: Vec<HomeOperation> = serde_json::from_slice(
        &std::fs::read(dir.path().join("device-home-operations.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(journal.len(), 1);
}
#[test]
fn claim_beats_future_release_preserves_catalog_sharing_and_replays_exact_ids() {
    use super::super::{
        persona_events::build_persona_event,
        retention::{get_retained_event, scoped_retention_db_path},
    };
    let (dir, c, i, owner) = setup();
    let db = scoped_retention_db_path(
        &dir.path().join("agents"),
        &c.scope.relay_url,
        &c.scope.owner_pubkey,
    );
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let mut conn = open_retention_db(&db).unwrap();
    let mut rs = read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
    rs[0].source_team_persona_slug = Some("CodeReviewer".into());
    save(dir.path(), &rs).unwrap();
    let mut d = rs[0].to_definition_view().unwrap();
    d.shared = true;
    let future = nostr::Timestamp::from(nostr::Timestamp::now().as_secs() + 10000);
    let prior = build_persona_event(&d)
        .unwrap()
        .custom_created_at(future)
        .sign_with_keys(&owner)
        .unwrap();
    retain_event(
        &conn,
        &RetainedEvent {
            kind: 30175,
            pubkey: c.scope.owner_pubkey.clone(),
            d_tag: "codereviewer".into(),
            content: prior.content.clone(),
            created_at: future.as_secs() as i64,
            raw_event: prior.as_json(),
            pending_sync: false,
        },
    )
    .unwrap();
    assert!(commit_claim_in_dir(
        dir.path(),
        &c,
        "one",
        i,
        &owner,
        |rs| save(dir.path(), rs),
        |_| Err("offline".into())
    )
    .is_err());
    let ops = read_journal(dir.path()).unwrap();
    let event = &ops[0].signed_events[0];
    assert!(event.created_at > future);
    assert!(buzz_core_pkg::kind::event_is_shared(event));
    let ids: Vec<_> = ops[0].signed_events.iter().map(|e| e.id).collect();
    recover_in_dir(
        dir.path(),
        |binding| Ok(c.proof.matches(binding)),
        |op| enqueue_home_events(dir.path(), &mut conn, op),
    )
    .unwrap();
    let head = get_retained_event(&conn, 30175, &c.scope.owner_pubkey, "codereviewer")
        .unwrap()
        .unwrap();
    assert_eq!(Event::from_json(head.raw_event).unwrap().id, ids[0]);
    assert_eq!(get_pending_sync(&conn).unwrap().len(), 2);
}
#[test]
fn claim_transaction_failure_retains_intent_and_never_half_enqueues() {
    let (dir, c, i, owner) = setup();
    let mut conn = open_retention_db(&dir.path().join("transaction.db")).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_instance BEFORE INSERT ON persona_events WHEN NEW.kind=30177 BEGIN SELECT RAISE(FAIL, 'injected second enqueue failure'); END;").unwrap();
    assert!(commit_claim_in_dir(
        dir.path(),
        &c,
        "one",
        i,
        &owner,
        |rs| save(dir.path(), rs),
        |op| enqueue_home_events(dir.path(), &mut conn, op)
    )
    .is_err());
    assert!(get_pending_sync(&conn).unwrap().is_empty());
    assert_eq!(read_journal(dir.path()).unwrap().len(), 1);
    conn.execute_batch("DROP TRIGGER reject_instance").unwrap();
    recover_in_dir(
        dir.path(),
        |binding| Ok(c.proof.matches(binding)),
        |op| enqueue_home_events(dir.path(), &mut conn, op),
    )
    .unwrap();
    assert_eq!(get_pending_sync(&conn).unwrap().len(), 2);
}
#[test]
fn changed_bound_target_or_lineage_never_recovers_stale_claim() {
    for change_binding in [false, true] {
        let (dir, c, i, owner) = setup();
        assert!(commit_claim_in_dir(
            dir.path(),
            &c,
            "one",
            i,
            &owner,
            |rs| save(dir.path(), rs),
            |_| Err("offline".into())
        )
        .is_err());
        let mut raw = read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
        if change_binding {
            raw[1].device_host_binding = Some("different binding".into());
        } else {
            raw[0].origin_device_id = Some("different home".into());
        }
        save(dir.path(), &raw).unwrap();
        recover_in_dir(
            dir.path(),
            |binding| Ok(c.proof.matches(binding)),
            |_| panic!("stale claim published"),
        )
        .unwrap();
        assert_eq!(read_journal(dir.path()).unwrap().len(), 1);
    }
}
#[test]
fn builtin_first_instance_materializes_bound_default_private_definition_atomically() {
    let (dir, c, mut i, owner) = setup();
    save(dir.path(), &[]).unwrap();
    i.persona_id = Some("builtin:fizz".into());
    commit_claim_in_dir(
        dir.path(),
        &c,
        "builtin:fizz",
        i,
        &owner,
        |rs| save(dir.path(), rs),
        |_| Ok(()),
    )
    .unwrap();
    let raw = read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
    assert_eq!(raw.len(), 2);
    assert_eq!(raw[0].share_across_devices, Some(false));
    assert_eq!(
        raw[0].origin_device_id.as_deref(),
        Some(c.device.device_id.as_str())
    );
    assert_eq!(
        raw[1].device_host_binding.as_deref(),
        Some(c.proof.binding())
    );
}

#[test]
fn copied_or_unavailable_proof_never_recovers_claim_and_preserves_intent_bytes() {
    let (dir, c, i, owner) = setup();
    assert!(commit_claim_in_dir(
        dir.path(),
        &c,
        "one",
        i,
        &owner,
        |rs| save(dir.path(), rs),
        |_| Err("offline".into())
    )
    .is_err());
    let before = std::fs::read(journal(dir.path())).unwrap();
    for unavailable in [false, true] {
        let other =
            crate::managed_agents::device_home_migration::tests::context(EvidenceReadiness::Ready);
        let calls = Cell::new(0);
        let result = recover_in_dir(
            dir.path(),
            |binding| {
                if unavailable {
                    Err("host proof unavailable".into())
                } else {
                    Ok(other.proof.matches(binding))
                }
            },
            |_| {
                calls.set(1);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(calls.get(), 0);
        assert_eq!(std::fs::read(journal(dir.path())).unwrap(), before);
    }
}
#[test]
fn new_import_pair_preserves_foreign_records_and_persists_only_new_keys() {
    let (dir, c, mut i, _) = setup();
    let mut raw = read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
    let mut foreign = i.clone();
    foreign.pubkey = "foreign".into();
    foreign.private_key_nsec = "protected-inline-secret".into();
    foreign.device_host_binding = Some("foreign host".into());
    raw.push(foreign.clone());
    save(dir.path(), &raw).unwrap();
    let mut d = crate::managed_agents::device_home_migration::tests::definition();
    d.id = "import".into();
    stamp_new_definition(&mut d, None, &c.device);
    i.persona_id = Some(d.id.clone());
    bind_new_instance(&mut i, &c.proof);
    let pubkey = i.pubkey.clone();
    let calls = Cell::new(0);
    commit_new_pairs_in_dir(dir.path(), vec![d], vec![i], |targets| {
        calls.set(calls.get() + 1);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].pubkey, pubkey);
        targets[0].private_key_nsec.clear();
    })
    .unwrap();
    assert_eq!(calls.get(), 1);
    let after = read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
    assert_eq!(after.len(), 4);
    assert_eq!(
        serde_json::to_value(after.iter().find(|r| r.pubkey == "foreign").unwrap()).unwrap(),
        serde_json::to_value(foreign).unwrap()
    );
    assert_eq!(
        after
            .iter()
            .filter(|r| r.slug.as_deref() == Some("import"))
            .count(),
        1
    );
    assert_eq!(
        after
            .iter()
            .filter(|r| r.persona_id.as_deref() == Some("import"))
            .count(),
        1
    );
}

#[test]
fn builtin_and_source_slug_claims_enqueue_canonical_persona_heads() {
    for (id, source) in [
        ("builtin:fizz", None),
        ("builtin:honey", None),
        ("one", Some("CodeReviewer")),
    ] {
        let (dir, c, mut instance, keys) = setup();
        if let Some(source) = source {
            let mut raw =
                read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
            raw[0].source_team_persona_slug = Some(source.into());
            save(dir.path(), &raw).unwrap();
        } else {
            save(dir.path(), &[]).unwrap();
        }
        instance.persona_id = Some(id.into());
        let mut conn = open_retention_db(&dir.path().join("claim.db")).unwrap();
        commit_claim_in_dir(
            dir.path(),
            &c,
            id,
            instance,
            &keys,
            |raw| save(dir.path(), raw),
            |op| enqueue_home_events(dir.path(), &mut conn, op),
        )
        .unwrap();
        let pending = get_pending_sync(&conn).unwrap();
        let head = pending.iter().find(|row| row.kind == 30175).unwrap();
        let expected = source
            .map(str::to_lowercase)
            .unwrap_or_else(|| id.replace(':', "-"));
        assert_eq!(head.d_tag, expected);
        Event::from_json(&head.raw_event).unwrap().verify().unwrap();
        let raw = read_policy_records(&dir.path().join("agents/managed-agents.json")).unwrap();
        assert_eq!(raw[0].slug.as_deref(), Some(id));
        assert!(read_journal(dir.path()).unwrap().is_empty());
    }
}

#[test]
fn builtin_journal_recovery_preserves_signed_bytes_and_rejects_wrong_scope_atomically() {
    for id in ["builtin:fizz", "builtin:honey"] {
        let (dir, c, mut instance, keys) = setup();
        save(dir.path(), &[]).unwrap();
        instance.persona_id = Some(id.into());
        assert!(commit_claim_in_dir(
            dir.path(),
            &c,
            id,
            instance,
            &keys,
            |raw| save(dir.path(), raw),
            |_| Err("offline".into()),
        )
        .is_err());
        let op = read_journal(dir.path()).unwrap().pop().unwrap();
        let signed: Vec<_> = op.signed_events.iter().map(Event::as_json).collect();
        let original = std::fs::read(journal(dir.path())).unwrap();
        let mut conn = open_retention_db(&dir.path().join("recovery.db")).unwrap();
        for mutation in [
            "persona_coordinate",
            "coordinate",
            "owner",
            "kind",
            "signature",
        ] {
            let mut invalid = op.clone();
            if mutation == "owner" {
                invalid.owner_pubkey = Keys::generate().public_key().to_hex();
            } else {
                let mut json = serde_json::to_value(&invalid.signed_events[1]).unwrap();
                if mutation == "signature" {
                    json["content"] = serde_json::json!("tampered");
                    invalid.signed_events[1] = serde_json::from_value(json).unwrap();
                } else {
                    let index = if mutation == "persona_coordinate" {
                        0
                    } else {
                        1
                    };
                    let original_event = &op.signed_events[index];
                    let kind = if mutation == "kind" {
                        nostr::Kind::from(30175u16)
                    } else {
                        original_event.kind
                    };
                    let tags = original_event.tags.iter().cloned().map(|tag| {
                        if mutation != "kind"
                            && tag.as_slice().first().map(String::as_str) == Some("d")
                        {
                            nostr::Tag::parse(["d", "wrong"]).unwrap()
                        } else {
                            tag
                        }
                    });
                    invalid.signed_events[index] =
                        nostr::EventBuilder::new(kind, original_event.content.clone())
                            .tags(tags)
                            .custom_created_at(original_event.created_at)
                            .sign_with_keys(&keys)
                            .unwrap();
                }
            }
            assert!(enqueue_home_events(dir.path(), &mut conn, &invalid).is_err());
            assert!(get_pending_sync(&conn).unwrap().is_empty());
            assert_eq!(std::fs::read(journal(dir.path())).unwrap(), original);
        }
        recover_in_dir(
            dir.path(),
            |b| Ok(c.proof.matches(b)),
            |op| {
                assert_eq!(
                    op.signed_events
                        .iter()
                        .map(Event::as_json)
                        .collect::<Vec<_>>(),
                    signed
                );
                enqueue_home_events(dir.path(), &mut conn, op)
            },
        )
        .unwrap();
        let pending = get_pending_sync(&conn).unwrap();
        assert_eq!(pending.len(), 2);
        for event in signed {
            assert!(pending.iter().any(|row| row.raw_event == event));
        }
        assert!(read_journal(dir.path()).unwrap().is_empty());
    }
}
