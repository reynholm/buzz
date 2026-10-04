use super::*;
use crate::managed_agents::{
    agent_events::build_agent_event,
    definition_home::{
        classify_definition_home, definition_capabilities, EvidenceReadiness, HomeKind,
    },
    device_home_migration::tests::{app, context, definition},
    persona_device_view::read_remote_evidence,
    retention::*,
};
use nostr::{EventBuilder, JsonUtil, Kind, Timestamp};

#[test]
fn old_writer_cannot_clear_device_policy() {
    let mut current = definition();
    current.share_across_devices = Some(false);
    current.origin_device_id = Some("foreign".into());
    current.origin_device_label = Some("Home".into());
    current.origin_released = Some(false);
    let mut incoming = definition();
    incoming.display_name = "Edited".into();
    let mut personas = vec![current.clone()];
    apply_inbound_persona(&mut personas, incoming);
    assert_eq!(personas[0].display_name, "Edited");
    assert_eq!(personas[0].share_across_devices, Some(false));
    assert_eq!(personas[0].origin_device_id, current.origin_device_id);
    assert_eq!(personas[0].origin_device_label, current.origin_device_label);
    assert_eq!(personas[0].origin_released, Some(false));
}

#[test]
fn accepted_release_updates_remote_card() {
    let mut current = definition();
    current.share_across_devices = Some(false);
    current.origin_device_id = Some("foreign".into());
    current.origin_device_label = Some("Home".into());
    current.origin_released = Some(false);
    let mut incoming = current.clone();
    incoming.origin_released = Some(true);
    incoming.origin_device_label = Some("Renamed home".into());
    let mut personas = vec![current];
    apply_inbound_persona(&mut personas, incoming);
    assert_eq!(personas[0].origin_released, Some(true));
    assert_eq!(
        personas[0].origin_device_label.as_deref(),
        Some("Renamed home")
    );
}

#[test]
fn release_tombstone_orders_agree() {
    for release_first in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<AppState>();
        let keys = state.signing_keys().unwrap();
        let mut c = context(EvidenceReadiness::Ready);
        let mut d = definition();
        d.origin_device_id = Some("foreign".into());
        d.origin_released = Some(false);
        let mut record = d.clone().into_agent_record();
        record.pubkey = nostr::Keys::generate().public_key().to_hex();
        record.persona_id = Some(d.id.clone());
        let path = scoped_retention_db_path(
            &dir.path().join("agents"),
            "wss://test",
            &keys.public_key().to_hex(),
        );
        let conn = open_retention_db(&path).unwrap();
        let event = build_agent_event(&record)
            .unwrap()
            .custom_created_at(Timestamp::from(100))
            .sign_with_keys(&keys)
            .unwrap();
        retain_inbound_event(
            &conn,
            &RetainedEvent {
                kind: 30177,
                pubkey: keys.public_key().to_hex(),
                d_tag: record.pubkey.clone(),
                content: event.content.clone(),
                created_at: 100,
                raw_event: event.as_json(),
                pending_sync: false,
            },
        )
        .unwrap();
        let tombstone = EventBuilder::new(Kind::Custom(5), "")
            .tags([nostr::Tag::parse([
                "a",
                &format!("30177:{}:{}", keys.public_key().to_hex(), record.pubkey),
            ])
            .unwrap()])
            .custom_created_at(Timestamp::from(200))
            .sign_with_keys(&keys)
            .unwrap();
        let release = |d: &mut AgentDefinition| {
            d.origin_released = Some(true);
        };
        let remove = || {
            reconcile_inbound_tombstone_with_refresh(
                &tombstone,
                "wss://test",
                app.handle(),
                &state,
                || {},
            )
            .unwrap()
        };
        if release_first {
            release(&mut d);
        } else {
            remove();
        }
        c.evidence = read_remote_evidence(&path, &keys.public_key().to_hex()).unwrap();
        let h = classify_definition_home(&d, &[], &c.device, &c.proof, &c.evidence);
        assert_eq!(h.kind, HomeKind::Remote);
        assert!(
            !definition_capabilities(&d, &h, EvidenceReadiness::Ready, false).can_create_instance
        );
        if release_first {
            remove();
        } else {
            release(&mut d);
        }
        c.evidence = read_remote_evidence(&path, &keys.public_key().to_hex()).unwrap();
        assert_eq!(
            classify_definition_home(&d, &[], &c.device, &c.proof, &c.evidence).kind,
            HomeKind::Unclaimed
        );
    }
}

#[test]
fn stale_head_cannot_release_home_and_proven_home_does_not_move_on_echo() {
    for proven in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<AppState>();
        let keys = state.signing_keys().unwrap();
        let mut c = context(EvidenceReadiness::Ready);
        c.scope = crate::managed_agents::device_home_sync::capture_scope(&state).unwrap();
        let mut d = definition();
        d.share_across_devices = Some(false);
        d.origin_device_id = Some(if proven {
            c.device.device_id.clone()
        } else {
            "foreign".into()
        });
        d.origin_device_label = Some("Home".into());
        d.origin_released = Some(false);
        let mut raw = vec![d.clone().into_agent_record()];
        if proven {
            let mut i = d.clone().into_agent_record();
            i.pubkey = nostr::Keys::generate().public_key().to_hex();
            i.persona_id = Some(d.id.clone());
            i.device_host_binding = Some(c.proof.binding().into());
            i.private_key_nsec = "untouched copied bytes".into();
            raw.push(i);
        }
        let base = crate::managed_agents::managed_agents_base_dir(app.handle()).unwrap();
        crate::managed_agents::device_home_migration::tests::write(&base, &raw);
        let path = scoped_retention_db_path(&base, "wss://test", &keys.public_key().to_hex());
        let conn = open_retention_db(&path).unwrap();
        let head = crate::managed_agents::persona_events::build_persona_event(&d)
            .unwrap()
            .custom_created_at(Timestamp::from(300))
            .sign_with_keys(&keys)
            .unwrap();
        retain_inbound_event(
            &conn,
            &RetainedEvent {
                kind: 30175,
                pubkey: keys.public_key().to_hex(),
                d_tag: d.id.clone(),
                content: head.content.clone(),
                created_at: 300,
                raw_event: head.as_json(),
                pending_sync: false,
            },
        )
        .unwrap();
        let mut incoming = d.clone();
        incoming.origin_released = Some(true);
        incoming.origin_device_id = Some("other-origin".into());
        incoming.share_across_devices = Some(true);
        let mut context_once = Some(c);
        for second in [200, 400] {
            let event = crate::managed_agents::persona_events::build_persona_event(&incoming)
                .unwrap()
                .custom_created_at(Timestamp::from(second))
                .sign_with_keys(&keys)
                .unwrap();
            reconcile_inbound_persona_event_blocking_with(
                event.as_json(),
                "wss://test".into(),
                app.handle().clone(),
                |_, _| {
                    context_once
                        .take()
                        .ok_or_else(|| "proof already consumed".into())
                },
                || {},
            )
            .unwrap();
            let saved = crate::managed_agents::persona_device_view::read_policy_records(
                &base.join("managed-agents.json"),
            )
            .unwrap()
            .into_iter()
            .filter_map(|r| r.to_definition_view())
            .find(|d| d.id == "one")
            .unwrap();
            if second == 200 || proven {
                assert_eq!(saved.origin_device_id, d.origin_device_id);
                assert_eq!(saved.origin_released, Some(false));
            } else {
                assert_eq!(saved.origin_device_id.as_deref(), Some("other-origin"));
                assert_eq!(saved.origin_released, Some(true));
            }
            assert_eq!(saved.share_across_devices, Some(second == 400));
        }
    }
}

#[test]
fn owner_tombstone_still_applies_but_wrong_author_upserts_and_tombstones_do_not() {
    for wrong_author in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<AppState>();
        let owner = state.signing_keys().unwrap();
        let signer = if wrong_author {
            nostr::Keys::generate()
        } else {
            owner.clone()
        };
        let mut d = definition();
        d.origin_device_id = Some("foreign".into());
        let base = crate::managed_agents::managed_agents_base_dir(app.handle()).unwrap();
        crate::managed_agents::device_home_migration::tests::write(
            &base,
            &[d.clone().into_agent_record()],
        );
        let mut incoming = d.clone();
        incoming.display_name = "Incoming".into();
        let event = crate::managed_agents::persona_events::build_persona_event(&incoming)
            .unwrap()
            .custom_created_at(Timestamp::from(100))
            .sign_with_keys(&signer)
            .unwrap();
        reconcile_inbound_persona_event_blocking_with(
            event.as_json(),
            "wss://test".into(),
            app.handle().clone(),
            |_, _| panic!("receiver has no linked instance and needs no host proof"),
            || {},
        )
        .unwrap();
        let saved = crate::managed_agents::persona_device_view::read_policy_records(
            &base.join("managed-agents.json"),
        )
        .unwrap()
        .into_iter()
        .filter_map(|r| r.to_definition_view())
        .find(|d| d.id == "one")
        .unwrap();
        assert_eq!(
            saved.display_name,
            if wrong_author { "One" } else { "Incoming" }
        );
        let tombstone = EventBuilder::new(Kind::Custom(5), "")
            .tags([nostr::Tag::parse([
                "a",
                &format!("30175:{}:one", signer.public_key().to_hex()),
            ])
            .unwrap()])
            .custom_created_at(Timestamp::from(200))
            .sign_with_keys(&signer)
            .unwrap();
        reconcile_inbound_tombstone_with_refresh(
            &tombstone,
            "wss://test",
            app.handle(),
            &state,
            || {},
        )
        .unwrap();
        let saved = crate::managed_agents::persona_device_view::read_policy_records(
            &base.join("managed-agents.json"),
        )
        .unwrap();
        assert_eq!(
            saved
                .iter()
                .any(|r| r.pubkey.is_empty() && r.slug.as_deref() == Some("one")),
            wrong_author
        );
    }
}

#[test]
fn stale_tombstone_cannot_remove_newer_home_and_valid_owner_removal_is_receiver_only() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<AppState>();
    let owner = state.signing_keys().unwrap();
    let (mut raw, _) = crate::managed_agents::device_home_migration::tests::records();
    raw[1].device_host_binding = Some("foreign".into());
    raw[1].private_key_nsec = "untouched copied secret".into();
    let mut sibling = raw[1].clone();
    sibling.pubkey = nostr::Keys::generate().public_key().to_hex();
    sibling.private_key_nsec = "untouched sibling secret".into();
    raw.push(sibling.clone());
    let base = crate::managed_agents::managed_agents_base_dir(app.handle()).unwrap();
    crate::managed_agents::device_home_migration::tests::write(&base, &raw);
    let path = scoped_retention_db_path(&base, "wss://test", &owner.public_key().to_hex());
    let conn = open_retention_db(&path).unwrap();
    let head = build_agent_event(&raw[1])
        .unwrap()
        .custom_created_at(Timestamp::from(300))
        .sign_with_keys(&owner)
        .unwrap();
    retain_inbound_event(
        &conn,
        &RetainedEvent {
            kind: 30177,
            pubkey: owner.public_key().to_hex(),
            d_tag: raw[1].pubkey.clone(),
            content: head.content.clone(),
            created_at: 300,
            raw_event: head.as_json(),
            pending_sync: false,
        },
    )
    .unwrap();
    let refreshes = std::cell::Cell::new(0);
    for second in [250, 350] {
        let tombstone = EventBuilder::new(Kind::Custom(5), "")
            .tags([nostr::Tag::parse([
                "a",
                &format!("30177:{}:{}", owner.public_key().to_hex(), raw[1].pubkey),
            ])
            .unwrap()])
            .custom_created_at(Timestamp::from(second))
            .sign_with_keys(&owner)
            .unwrap();
        reconcile_inbound_tombstone_with_refresh(
            &tombstone,
            "wss://test",
            app.handle(),
            &state,
            || {
                refreshes.set(refreshes.get() + 1);
            },
        )
        .unwrap();
        let saved = crate::managed_agents::persona_device_view::read_policy_records(
            &base.join("managed-agents.json"),
        )
        .unwrap();
        assert_eq!(
            saved.iter().any(|r| r.pubkey == raw[1].pubkey),
            second == 250
        );
        assert_eq!(
            get_retained_event(&conn, 30177, &owner.public_key().to_hex(), &raw[1].pubkey)
                .unwrap()
                .is_some(),
            second == 250
        );
        assert_eq!(
            serde_json::to_value(saved.iter().find(|r| r.pubkey == sibling.pubkey).unwrap())
                .unwrap(),
            serde_json::to_value(&sibling).unwrap()
        );
    }
    assert_eq!(refreshes.get(), 1);
    assert!(
        get_pending_sync(&conn).unwrap().is_empty(),
        "receiver must not author tombstone/archive/profile replies"
    );
}
