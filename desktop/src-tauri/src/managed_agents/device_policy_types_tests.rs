//! Compatibility seams for device policy; no runtime authorization here.
use super::persona_events::{persona_content_hash, persona_event_content, PersonaEventContent};
use super::*;
use serde_json::{json, Value};

fn legacy_definition() -> AgentDefinition {
    serde_json::from_value(json!({
        "id": "test-agent", "display_name": "Test Agent",
        "avatar_url": "https://example.com/avatar.png",
        "system_prompt": "You are a test assistant.", "runtime": "goose",
        "model": "claude-opus-4", "provider": "anthropic",
        "name_pool": ["Alpha", "Beta"], "created_at": "now", "updated_at": "now"
    }))
    .unwrap()
}

fn with_policy(value: impl serde::Serialize, share: bool) -> Value {
    let mut value = serde_json::to_value(value).unwrap();
    let object = value.as_object_mut().unwrap();
    object.insert("share_across_devices".into(), json!(share));
    object.insert("origin_device_id".into(), json!("device-a"));
    object.insert("origin_device_label".into(), json!("Laptop"));
    object.insert("origin_released".into(), json!(false));
    value
}

#[test]
fn legacy_policy_bytes_and_hash_stay_identical() {
    const REFERENCE: &str = r#"{"display_name":"Test Agent","system_prompt":"You are a test assistant.","avatar_url":"https://example.com/avatar.png","runtime":"goose","model":"claude-opus-4","provider":"anthropic","name_pool":["Alpha","Beta"]}"#;
    let before = persona_event_content(&legacy_definition());
    assert_eq!(serde_json::to_vec(&before).unwrap(), REFERENCE.as_bytes());
    for share in [true, false] {
        let after: PersonaEventContent =
            serde_json::from_value(with_policy(&before, share)).unwrap();
        assert_eq!(persona_content_hash(&before), persona_content_hash(&after));
        assert_eq!(
            serde_json::to_value(&after).unwrap()["share_across_devices"],
            json!(share)
        );
    }
    let record = legacy_definition().into_agent_record();
    let restored: ManagedAgentRecord =
        serde_json::from_value(serde_json::to_value(record).unwrap()).unwrap();
    assert!(serde_json::to_value(restored)
        .unwrap()
        .get("device_host_binding")
        .is_none());
}

#[test]
fn policy_roundtrip_survives_unified_projection() {
    for share in [true, false] {
        let definition: AgentDefinition =
            serde_json::from_value(with_policy(legacy_definition(), share)).unwrap();
        let record = definition.into_agent_record();
        for value in [
            serde_json::to_value(&record).unwrap(),
            serde_json::to_value(record.to_definition_view().unwrap()).unwrap(),
        ] {
            assert_eq!(value["share_across_devices"], json!(share));
            assert_eq!(value["origin_device_id"], json!("device-a"));
            assert_eq!(value["origin_device_label"], json!("Laptop"));
            assert_eq!(value["origin_released"], json!(false));
        }
        let keys = nostr::Keys::generate();
        let event = persona_events::build_persona_event(&record.to_definition_view().unwrap())
            .unwrap()
            .sign_with_keys(&keys)
            .unwrap();
        let parsed = persona_events::persona_from_event(&event).unwrap();
        let value = serde_json::to_value(parsed).unwrap();
        assert_eq!(value["share_across_devices"], json!(share));
        assert_eq!(value["origin_device_id"], json!("device-a"));
        assert_eq!(value["origin_device_label"], json!("Laptop"));
        assert_eq!(value["origin_released"], json!(false));
        assert!(event.content.ends_with(&format!(
            r#","share_across_devices":{share},"origin_device_id":"device-a","origin_device_label":"Laptop","origin_released":false}}"#
        )));
    }
}

#[test]
fn portable_exports_exclude_device_metadata() {
    let legacy = legacy_definition().into_agent_record();
    let expected_event = serde_json::to_value(agent_events::agent_event_content(&legacy)).unwrap();
    let mut value = with_policy(&legacy, false);
    value["device_host_binding"] = json!("local-marker");
    let record: ManagedAgentRecord = serde_json::from_value(value).unwrap();
    assert_eq!(
        serde_json::to_value(&record).unwrap()["device_host_binding"],
        json!("local-marker")
    );
    let keys = nostr::Keys::generate();
    let event = agent_events::build_agent_event(&record)
        .unwrap()
        .sign_with_keys(&keys)
        .unwrap();
    let event_json: Value = serde_json::from_str(&event.content).unwrap();
    assert_eq!(
        event_json, expected_event,
        "device metadata changed public 30177 output"
    );
    let snapshot = serde_json::to_value(agent_snapshot::build_snapshot(
        &record,
        agent_snapshot::MemoryLevel::None,
        vec![],
        None,
    ))
    .unwrap();
    for field in [
        "share_across_devices",
        "origin_device_id",
        "origin_device_label",
        "origin_released",
        "device_host_binding",
        "shareAcrossDevices",
        "originDeviceId",
        "originDeviceLabel",
        "originReleased",
        "deviceHostBinding",
    ] {
        assert!(event_json.get(field).is_none(), "30177 leaked {field}");
        assert!(snapshot.get(field).is_none(), "snapshot leaked {field}");
        assert!(
            snapshot["definition"].get(field).is_none(),
            "snapshot definition leaked {field}"
        );
    }
}

#[test]
fn creation_policy_uses_camel_case_ipc_and_legacy_default() {
    for permission in [None, Some(false), Some(true)] {
        let mut input = json!({"displayName":"Agent", "systemPrompt":""});
        if let Some(permission) = permission {
            input["shareAcrossDevices"] = json!(permission);
        }
        let request: CreatePersonaRequest = serde_json::from_value(input).unwrap();
        assert_eq!(request.share_across_devices, permission);
    }
}
