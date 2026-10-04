use super::*;
use crate::managed_agents::{
    definition_home::EvidenceReadiness,
    device_home_migration::tests::{app, context, records, write},
    device_home_sync,
    retention::*,
};
use std::cell::Cell;
use tauri::Manager;

#[test]
fn remote_definition_rename_does_not_publish_copied_instance() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<AppState>();
    let (mut raw, _) = records();
    raw[0].share_across_devices = Some(false);
    raw[0].origin_device_id = Some("foreign".into());
    raw[1].device_host_binding = Some("foreign".into());
    raw[1].name = "One".into();
    raw[1].private_key_nsec = "copied-secret".into();
    let base = crate::managed_agents::managed_agents_base_dir(app.handle()).unwrap();
    write(&base, &raw);
    let original = raw[1].clone();
    let keys = state.signing_keys().unwrap();
    let path = scoped_retention_db_path(&base, "wss://test", &keys.public_key().to_hex());
    let conn = open_retention_db(&path).unwrap();
    let key_reads = Cell::new(0);
    let instance_heads = Cell::new(0);
    let input: UpdatePersonaRequest = serde_json::from_value(serde_json::json!({
        "id":"one",
        "displayName":"Edited",
        "systemPrompt":"Edited instructions",
        "description":"Edited about",
        "namePool":[],
    }))
    .unwrap();
    let (d, _, profiles) = update_persona_locked_with(
        input,
        app.handle(),
        &state,
        |_, _, d| {
            crate::commands::personas::retain_persona_pending_at(
                &RetentionScope {
                    db_path: path.clone(),
                    relay_url: "wss://test".into(),
                    owner_keys: keys.clone(),
                },
                d,
            )
        },
        |_, state| {
            let mut c = context(EvidenceReadiness::Ready);
            c.scope = device_home_sync::capture_scope(state)?;
            Ok(c)
        },
        |_| {
            key_reads.set(key_reads.get() + 1);
            Ok(Some(nostr::Keys::generate()))
        },
        |_, _, _| {
            instance_heads.set(instance_heads.get() + 1);
        },
        || {},
    )
    .unwrap();
    assert_eq!(d.display_name, "Edited");
    assert_eq!(d.share_across_devices, Some(false));
    assert_eq!(
        get_pending_sync(&conn)
            .unwrap()
            .iter()
            .map(|r| r.kind)
            .collect::<Vec<_>>(),
        vec![30175]
    );
    let pending = get_pending_sync(&conn).unwrap();
    let event = <nostr::Event as nostr::JsonUtil>::from_json(&pending[0].raw_event).unwrap();
    event.verify().unwrap();
    assert_eq!(event.pubkey.to_hex(), keys.public_key().to_hex());
    assert_eq!(
        crate::managed_agents::persona_events::persona_from_event(&event)
            .unwrap()
            .display_name,
        "Edited"
    );
    assert_eq!(key_reads.get(), 0, "copied instance key read");
    assert_eq!(instance_heads.get(), 0, "copied 30177 publication");
    assert!(profiles.is_empty(), "copied kind0 dispatch prepared");
    let after = crate::managed_agents::persona_device_view::read_policy_records(
        &base.join("managed-agents.json"),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(after.iter().find(|r| r.pubkey == original.pubkey).unwrap()).unwrap(),
        serde_json::to_value(original).unwrap()
    );
}
