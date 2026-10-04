use super::*;
use crate::managed_agents::{
    agent_events::build_agent_event,
    definition_home::EvidenceReadiness,
    device_home_migration::tests::{app, context, records, write},
    persona_device_view::read_policy_records,
    retention::*,
    RespondTo,
};
use nostr::JsonUtil;
use std::cell::Cell;

#[test]
fn access_revocation_stops_owned_child_despite_unavailable_proof() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<AppState>();
    let keys = state.keys.lock().unwrap().clone();
    let relay = crate::relay::relay_ws_url_with_override(&state);
    let (mut raw, _) = records();
    raw[1].device_host_binding = Some("previous-local-proof".into());
    raw[1].runtime_pid = Some(123);
    raw[1].respond_to = RespondTo::Anyone;
    let base = crate::managed_agents::managed_agents_base_dir(app.handle()).unwrap();
    write(&base, &raw);
    let mut next = raw[1].clone();
    next.respond_to = RespondTo::OwnerOnly;
    let event = build_agent_event(&next)
        .unwrap()
        .sign_with_keys(&keys)
        .unwrap();
    let stops = Cell::new(0);
    let result = reconcile_inbound_persona_event_blocking_with_stop(
        event.as_json(),
        relay,
        app.handle().clone(),
        |_, _| Err("keychain unavailable".into()),
        || {},
        |_, record, _| {
            // The external stop boundary must run before ACL persistence.
            assert!(matches!(
                read_policy_records(&base.join("managed-agents.json"))?[1].respond_to,
                RespondTo::Anyone
            ));
            stops.set(stops.get() + 1);
            record.runtime_pid = None;
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(stops.get(), 1, "revocation cannot depend on spawn proof");
    assert!(matches!(result, Some(InboundRuntimeRefresh::Local { .. })));
    let saved = read_policy_records(&base.join("managed-agents.json")).unwrap();
    assert!(matches!(saved[1].respond_to, RespondTo::OwnerOnly));
    assert_eq!(saved[1].runtime_pid, None);
    // New spawn still requires authority; stop permission cannot grant it.
    assert!(
        crate::managed_agents::device_runtime::runtime_phase_locked_with(
            app.handle(),
            &state,
            &raw[1].pubkey,
            None,
            |_, _| Err("keychain unavailable".into()),
            |_, _, _| -> Result<(), String> { panic!("unproven restart") },
        )
        .is_err()
    );
}

#[test]
fn failed_access_stop_preserves_old_acl_and_same_head_retry() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<AppState>();
    let keys = state.keys.lock().unwrap().clone();
    let relay = crate::relay::relay_ws_url_with_override(&state);
    let (mut raw, _) = records();
    let mut c = context(EvidenceReadiness::Ready);
    c.scope = crate::managed_agents::device_home_sync::capture_scope(&state).unwrap();
    raw[1].device_host_binding = Some(c.proof.binding().into());
    raw[1].runtime_pid = Some(123);
    raw[1].respond_to = RespondTo::Anyone;
    let base = crate::managed_agents::managed_agents_base_dir(app.handle()).unwrap();
    write(&base, &raw);
    let before = std::fs::read(base.join("managed-agents.json")).unwrap();
    let mut next = raw[1].clone();
    next.respond_to = RespondTo::OwnerOnly;
    let event = build_agent_event(&next)
        .unwrap()
        .sign_with_keys(&keys)
        .unwrap();
    let result = reconcile_inbound_persona_event_blocking_with_stop(
        event.as_json(),
        relay.clone(),
        app.handle().clone(),
        |_, _| Ok(c),
        || {},
        |_, _, _| Err("injected stop failure".into()),
    );
    assert!(result.is_err());
    assert!(
        std::fs::read(base.join("managed-agents.json")).unwrap() == before,
        "failed stop changed durable ACL bytes"
    );
    let conn = open_retention_db(&scoped_retention_db_path(
        &base,
        &relay,
        &keys.public_key().to_hex(),
    ))
    .unwrap();
    assert!(
        get_retained_event(&conn, 30177, &keys.public_key().to_hex(), &raw[1].pubkey)
            .unwrap()
            .is_none()
    );
    let stops = Cell::new(0);
    let result = reconcile_inbound_persona_event_blocking_with_stop(
        event.as_json(),
        relay,
        app.handle().clone(),
        |_, _| Err("proof unavailable during retry".into()),
        || {},
        |_, record, _| {
            stops.set(stops.get() + 1);
            record.runtime_pid = None;
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(stops.get(), 1);
    assert!(result.is_some());
    assert!(
        get_retained_event(&conn, 30177, &keys.public_key().to_hex(), &raw[1].pubkey)
            .unwrap()
            .is_some()
    );
    assert!(matches!(
        read_policy_records(&base.join("managed-agents.json")).unwrap()[1].respond_to,
        RespondTo::OwnerOnly
    ));
}
