use super::*;
use crate::{
    device_identity::load_or_create_host_proof,
    secret_store::{SecretStore, TestBlobBackend},
};
use serde_json::json;
use std::sync::{Arc, Mutex};
struct Blob(Mutex<Option<Vec<u8>>>);
impl TestBlobBackend for Blob {
    fn read(&self) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn write(&self, bytes: &[u8]) -> Result<(), String> {
        *self.0.lock().unwrap() = Some(bytes.to_vec());
        Ok(())
    }
}
fn fixture() -> (AgentDefinition, DeviceIdentity, HostProof) {
    let definition = serde_json::from_value(json!({"id":"one", "display_name":"One", "system_prompt":"Test", "runtime":"goose", "model":"test", "provider":"test", "name_pool":[], "created_at":"now", "updated_at":"now"})).unwrap();
    let mut store = SecretStore::keyring(format!("task3-test-{}", uuid::Uuid::new_v4()));
    store.test_backend = Some(Arc::new(Blob(Mutex::new(None))));
    (
        definition,
        DeviceIdentity {
            device_id: "own".into(),
            label: "Own host".into(),
            created_at: "now".into(),
        },
        load_or_create_host_proof(&store).unwrap(),
    )
}
#[test]
fn home_matrix() {
    let (mut definition, device, proof) = fixture();
    for share in [None, Some(false), Some(true)] {
        definition.share_across_devices = share;
        for origin in [None, Some("own"), Some("foreign")] {
            definition.origin_device_id = origin.map(str::to_owned);
            definition.origin_device_label = Some("Origin label".into());
            for released in [None, Some(false), Some(true)] {
                definition.origin_released = released;
                for binding in [None, Some("foreign"), Some(proof.binding())] {
                    let mut record = definition.clone().into_agent_record();
                    record.pubkey = "instance".into();
                    record.persona_id = Some("one".into());
                    record.device_host_binding = binding.map(str::to_owned);
                    for remote in [false, true] {
                        let evidence = if remote {
                            vec![RemoteInstanceEvidence {
                                pubkey: "remote".into(),
                                persona_id: "one".into(),
                            }]
                        } else {
                            vec![]
                        };
                        let home = classify_definition_home(
                            &definition,
                            std::slice::from_ref(&record),
                            &device,
                            &proof,
                            &evidence,
                        );
                        let expected = if binding == Some(proof.binding()) {
                            HomeKind::Local
                        } else if binding == Some("foreign")
                            || remote
                            || (origin == Some("foreign") && released != Some(true))
                        {
                            HomeKind::Remote
                        } else if origin == Some("own") && released != Some(true) {
                            HomeKind::Local
                        } else {
                            HomeKind::Unclaimed
                        };
                        assert_eq!(home.kind, expected);
                        for readiness in [
                            EvidenceReadiness::Pending,
                            EvidenceReadiness::Ready,
                            EvidenceReadiness::Failed,
                        ] {
                            let caps = definition_capabilities(
                                &definition,
                                &home,
                                readiness,
                                binding == Some(proof.binding()),
                            );
                            let allowed = share == Some(true)
                                || (expected != HomeKind::Remote
                                    && (binding == Some(proof.binding())
                                        || readiness == EvidenceReadiness::Ready));
                            assert_eq!(caps.can_create_instance, allowed);
                            assert_eq!(caps.can_delete_definition, allowed);
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn release_never_overrides_active_instance() {
    let (mut definition, device, proof) = fixture();
    definition.origin_released = Some(true);
    let evidence = vec![RemoteInstanceEvidence {
        pubkey: "remote".into(),
        persona_id: "one".into(),
    }];
    assert_eq!(
        classify_definition_home(&definition, &[], &device, &proof, &evidence).kind,
        HomeKind::Remote
    );
    assert_eq!(
        classify_definition_home(&definition, &[], &device, &proof, &[]).kind,
        HomeKind::Unclaimed
    );
}
#[test]
fn remote_head_blocks_only_linked_definition() {
    let (definition, device, proof) = fixture();
    let evidence = vec![RemoteInstanceEvidence {
        pubkey: "remote".into(),
        persona_id: "other".into(),
    }];
    let home = classify_definition_home(&definition, &[], &device, &proof, &evidence);
    assert_eq!(home.kind, HomeKind::Unclaimed);
    assert!(home.remote_instance_pubkeys.is_empty());
    assert!(home.label.is_none());
}

#[test]
fn flattened_device_view_is_compatible_and_contains_no_host_authority() {
    let (definition, device, proof) = fixture();
    let binding = proof.binding().to_owned();
    let context = super::super::persona_device_view::DevicePolicyContext {
        scope: super::super::device_home_sync::SyncScope {
            owner_pubkey: "owner".into(),
            relay_url: "wss://scope".into(),
            workspace_generation: 1,
        },
        device,
        proof,
        evidence: vec![],
        readiness: EvidenceReadiness::Pending,
    };
    let view = context.project(definition.clone(), &[]);
    let value = serde_json::to_value(&view).unwrap();
    assert_eq!(value["id"], "one");
    assert!(!value.as_object().unwrap().contains_key("definition"));
    assert_eq!(value["home"]["kind"], "unclaimed");
    assert_eq!(
        value["capabilities"]["blockedReason"],
        "device_home_sync_pending"
    );
    assert!(value.get("homeError").is_none());
    assert!(!value.to_string().contains(&binding));
    let restored: AgentDefinition = serde_json::from_value(value).unwrap();
    assert_eq!(restored.id, definition.id);
}

#[test]
fn read_failure_is_not_unclaimed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.db");
    assert!(super::super::persona_device_view::read_remote_evidence(&path, "owner").is_err());
    assert!(!path.exists());
    std::fs::write(&path, b"broken database").unwrap();
    assert!(super::super::persona_device_view::read_remote_evidence(&path, "owner").is_err());
}
#[test]
fn evidence_is_owner_and_relay_scoped() {
    use super::super::{agent_events::build_agent_event, retention::*};
    use nostr::JsonUtil;
    let dir = tempfile::tempdir().unwrap();
    let owner = nostr::Keys::generate();
    let foreign = nostr::Keys::generate();
    let path = scoped_retention_db_path(dir.path(), "wss://one", &owner.public_key().to_hex());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let conn = open_retention_db(&path).unwrap();
    let (definition, _, _) = fixture();
    let mut record = definition.into_agent_record();
    record.persona_id = Some("one".into());
    record.pubkey = nostr::Keys::generate().public_key().to_hex();
    let event = build_agent_event(&record)
        .unwrap()
        .sign_with_keys(&owner)
        .unwrap();
    let retained = RetainedEvent {
        kind: 30177,
        pubkey: owner.public_key().to_hex(),
        d_tag: record.pubkey.clone(),
        content: event.content.clone(),
        created_at: event.created_at.as_secs() as i64,
        raw_event: event.as_json(),
        pending_sync: false,
    };
    retain_event(&conn, &retained).unwrap();
    let evidence = super::super::persona_device_view::read_remote_evidence(
        &path,
        &owner.public_key().to_hex(),
    )
    .unwrap();
    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0].persona_id, "one");
    assert_eq!(evidence[0].pubkey, record.pubkey);
    assert!(super::super::persona_device_view::read_remote_evidence(
        &path,
        &foreign.public_key().to_hex()
    )
    .unwrap()
    .is_empty());
    let other = scoped_retention_db_path(dir.path(), "wss://two", &owner.public_key().to_hex());
    let other_conn = open_retention_db(&other).unwrap();
    drop(other_conn);
    assert!(super::super::persona_device_view::read_remote_evidence(
        &other,
        &owner.public_key().to_hex()
    )
    .unwrap()
    .is_empty());
    let tombstone = super::super::agent_events::build_agent_delete(
        &record.pubkey,
        &owner.public_key().to_hex(),
    )
    .unwrap()
    .sign_with_keys(&owner)
    .unwrap();
    let deleted = RetainedEvent {
        kind: 5,
        pubkey: owner.public_key().to_hex(),
        d_tag: tombstone_retention_d_tag(30177, &record.pubkey),
        content: tombstone.content.clone(),
        created_at: retained.created_at,
        raw_event: tombstone.as_json(),
        pending_sync: false,
    };
    commit_inbound_tombstone_with_store(
        &conn,
        &deleted,
        30177,
        &owner.public_key().to_hex(),
        &record.pubkey,
        || Ok(()),
    )
    .unwrap();
    assert!(super::super::persona_device_view::read_remote_evidence(
        &path,
        &owner.public_key().to_hex()
    )
    .unwrap()
    .is_empty());
}
