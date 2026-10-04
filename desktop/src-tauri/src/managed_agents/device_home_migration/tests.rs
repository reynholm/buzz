use super::*;
use crate::{
    device_identity::{load_or_create_host_proof, DeviceIdentity},
    managed_agents::{
        definition_home::EvidenceReadiness, device_home_sync::SyncScope,
        persona_device_view::read_policy_records,
    },
    secret_store::{SecretStore, TestBlobBackend},
};
use std::sync::{Arc, Mutex};
struct Blob(Mutex<Option<Vec<u8>>>);
impl TestBlobBackend for Blob {
    fn read(&self) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn write(&self, b: &[u8]) -> Result<(), String> {
        *self.0.lock().unwrap() = Some(b.to_vec());
        Ok(())
    }
}
pub(crate) fn context(readiness: EvidenceReadiness) -> DevicePolicyContext {
    let mut store = SecretStore::keyring(format!("task4-{}", uuid::Uuid::new_v4()));
    store.test_backend = Some(Arc::new(Blob(Mutex::new(None))));
    DevicePolicyContext {
        scope: SyncScope {
            owner_pubkey: "owner".into(),
            relay_url: "wss://test".into(),
            workspace_generation: 0,
        },
        device: DeviceIdentity {
            device_id: uuid::Uuid::new_v4().to_string(),
            label: "Test host".into(),
            created_at: "now".into(),
        },
        proof: load_or_create_host_proof(&store).unwrap(),
        evidence: vec![],
        readiness,
    }
}
pub(crate) fn definition() -> AgentDefinition {
    serde_json::from_value(serde_json::json!({"id":"one","display_name":"One","system_prompt":"Test","runtime":"goose","model":"test","provider":"test","name_pool":[],"created_at":"now","updated_at":"now"})).unwrap()
}
pub(crate) fn records() -> (Vec<ManagedAgentRecord>, nostr::Keys) {
    let d = definition();
    let keys = nostr::Keys::generate();
    let mut i = d.clone().into_agent_record();
    i.pubkey = keys.public_key().to_hex();
    i.persona_id = Some(d.id.clone());
    (vec![d.into_agent_record(), i], keys)
}
pub(crate) fn write(dir: &Path, records: &[ManagedAgentRecord]) {
    std::fs::write(
        dir.join("managed-agents.json"),
        serde_json::to_vec_pretty(records).unwrap(),
    )
    .unwrap();
}
fn read(dir: &Path) -> Vec<ManagedAgentRecord> {
    read_policy_records(&dir.join("managed-agents.json")).unwrap()
}
#[test]
fn archived_duplicate_does_not_block_verified_legacy_local_claim() {
    let dir = tempfile::tempdir().unwrap();
    let (rs, key) = records();
    let duplicate = nostr::Keys::generate().public_key().to_hex();
    let mut c = context(EvidenceReadiness::Ready);
    c.evidence = vec![
        super::super::definition_home::RemoteInstanceEvidence {
            pubkey: rs[1].pubkey.clone(),
            persona_id: "one".into(),
        },
        super::super::definition_home::RemoteInstanceEvidence {
            pubkey: duplicate.clone(),
            persona_id: "one".into(),
        },
    ];
    write(dir.path(), &rs);
    assert!(
        migrate_device_homes_in_dir_with_archive(dir.path(), &c, &[duplicate], |_| Ok(Some(
            key.clone()
        )),)
        .unwrap()
    );
    let result = read(dir.path());
    assert_eq!(result[1].pubkey, rs[1].pubkey, "keep the existing identity");
    assert_eq!(
        result[1].device_host_binding.as_deref(),
        Some(c.proof.binding())
    );
    assert_eq!(
        result[0].origin_device_id.as_deref(),
        Some(c.device.device_id.as_str())
    );
    let home = c.project(result[0].to_definition_view().unwrap(), &result);
    assert_eq!(
        home.home.unwrap().kind,
        super::super::definition_home::HomeKind::Local
    );
    assert!(home.capabilities.can_create_instance);
}

#[test]
fn archive_does_not_replace_local_key_or_host_authority() {
    for scenario in [
        "active-duplicate",
        "unavailable-archive",
        "missing-key",
        "wrong-key",
        "foreign-binding",
        "archived-local",
        "pending",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (mut rs, key) = records();
        let duplicate = nostr::Keys::generate().public_key().to_hex();
        let mut c = context(EvidenceReadiness::Ready);
        c.evidence = vec![super::super::definition_home::RemoteInstanceEvidence {
            pubkey: duplicate.clone(),
            persona_id: "one".into(),
        }];
        let archived = match scenario {
            "active-duplicate" | "unavailable-archive" => vec![],
            "archived-local" => vec![duplicate, rs[1].pubkey.clone()],
            _ => vec![duplicate],
        };
        if scenario == "foreign-binding" {
            rs[1].device_host_binding = Some("copied-marker".into());
        }
        if scenario == "pending" {
            c.readiness = EvidenceReadiness::Pending;
        }
        write(dir.path(), &rs);
        let before = std::fs::read(dir.path().join("managed-agents.json")).unwrap();
        let result =
            migrate_device_homes_in_dir_with_archive(
                dir.path(),
                &c,
                &archived,
                |_| match scenario {
                    "missing-key" => Ok(None),
                    "wrong-key" => Ok(Some(nostr::Keys::generate())),
                    _ => Ok(Some(key.clone())),
                },
            );
        if scenario == "wrong-key" {
            assert!(result.is_err());
        } else {
            assert!(!result.unwrap(), "{scenario}");
        }
        assert_eq!(
            before,
            std::fs::read(dir.path().join("managed-agents.json")).unwrap(),
            "{scenario}"
        );
    }
}
#[test]
fn legacy_claim_requires_available_key() {
    let dir = tempfile::tempdir().unwrap();
    let (rs, key) = records();
    let c = context(EvidenceReadiness::Ready);
    write(dir.path(), &rs);
    let mut calls = 0;
    assert!(!migrate_device_homes_in_dir(dir.path(), &c, |_| {
        calls += 1;
        Ok(None)
    })
    .unwrap());
    assert_eq!(calls, 1, "legacy claim must actually look up key");
    assert!(read(dir.path())[1].device_host_binding.is_none());
    assert!(migrate_device_homes_in_dir(dir.path(), &c, |_| Ok(Some(key.clone()))).unwrap());
    assert_eq!(
        read(dir.path())[1].device_host_binding.as_deref(),
        Some(c.proof.binding())
    );
}
#[test]
fn legacy_foreign_origin_is_not_claimed() {
    let dir = tempfile::tempdir().unwrap();
    let (mut rs, _) = records();
    rs[0].origin_device_id = Some("foreign".into());
    write(dir.path(), &rs);
    let before = std::fs::read(dir.path().join("managed-agents.json")).unwrap();
    assert!(!migrate_device_homes_in_dir(
        dir.path(),
        &context(EvidenceReadiness::Ready),
        |_| panic!("foreign home key lookup")
    )
    .unwrap());
    assert_eq!(
        before,
        std::fs::read(dir.path().join("managed-agents.json")).unwrap()
    );
}
#[test]
fn migration_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let (rs, key) = records();
    let c = context(EvidenceReadiness::Ready);
    write(dir.path(), &rs);
    assert!(migrate_device_homes_in_dir(dir.path(), &c, |_| Ok(Some(key.clone()))).unwrap());
    let first = std::fs::read(dir.path().join("managed-agents.json")).unwrap();
    assert!(!migrate_device_homes_in_dir(dir.path(), &c, |_| panic!(
        "already proven needs no new key"
    ))
    .unwrap());
    assert_eq!(
        first,
        std::fs::read(dir.path().join("managed-agents.json")).unwrap()
    );
}
#[test]
fn json_copy_does_not_publish() {
    let (mut rs, _) = records();
    rs[1].device_host_binding = Some("foreign-marker".into());
    let c = context(EvidenceReadiness::Ready);
    assert!(!may_publish_local_instance(&rs[1], rs[0].to_definition_view().as_ref(), &c).unwrap());
}
#[test]
fn new_public_id_reclaims_proven_home() {
    let dir = tempfile::tempdir().unwrap();
    let (mut rs, key) = records();
    let mut c = context(EvidenceReadiness::Pending);
    let metadata = dir.path().join("device.json");
    let old =
        crate::device_identity::load_or_create_device_identity(&metadata, "Old host").unwrap();
    rs[1].device_host_binding = Some(c.proof.binding().into());
    rs[0].origin_device_id = Some(old.device_id.clone());
    write(dir.path(), &rs);
    std::fs::remove_file(&metadata).unwrap();
    c.device =
        crate::device_identity::load_or_create_device_identity(&metadata, "New host").unwrap();
    assert_ne!(old.device_id, c.device.device_id);
    assert!(migrate_device_homes_in_dir(dir.path(), &c, |_| panic!(
        "proven home needs no new agent key"
    ))
    .unwrap());
    assert_eq!(
        read(dir.path())[0].origin_device_id.as_deref(),
        Some(c.device.device_id.as_str())
    );
    queue_device_home_events(dir.path(), &key, &dir.path().join("retention.db"), &c).unwrap();
    let conn =
        crate::managed_agents::retention::open_retention_db(&dir.path().join("retention.db"))
            .unwrap();
    assert!(crate::managed_agents::retention::get_pending_sync(&conn)
        .unwrap()
        .iter()
        .any(|r| r.kind == 30175 && r.content.contains(&c.device.device_id)));
}
#[test]
fn key_read_error_leaves_disk_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let (rs, _) = records();
    write(dir.path(), &rs);
    let before = std::fs::read(dir.path().join("managed-agents.json")).unwrap();
    assert!(
        migrate_device_homes_in_dir(dir.path(), &context(EvidenceReadiness::Ready), |_| Err(
            "keychain locked".into()
        ))
        .is_err()
    );
    assert_eq!(
        before,
        std::fs::read(dir.path().join("managed-agents.json")).unwrap()
    );
}
#[test]
fn absence_claim_is_deferred_until_history() {
    let dir = tempfile::tempdir().unwrap();
    let (rs, _) = records();
    write(dir.path(), &rs);
    assert!(!migrate_device_homes_in_dir(
        dir.path(),
        &context(EvidenceReadiness::Pending),
        |_| panic!("pending absence cannot authorize key lookup")
    )
    .unwrap());
    assert!(!may_publish_local_instance(
        &rs[1],
        rs[0].to_definition_view().as_ref(),
        &context(EvidenceReadiness::Pending)
    )
    .unwrap());
}

pub(crate) fn app(path: &Path) -> tauri::App<tauri::test::MockRuntime> {
    use tauri::Manager;
    let state = crate::app_state::build_app_state();
    *state.keys.lock().unwrap() = nostr::Keys::generate();
    *state.relay_url_override.lock().unwrap() = Some("wss://test".into());
    let mut ctx = tauri::test::mock_context(tauri::test::noop_assets());
    ctx.config_mut().identifier = path.to_str().unwrap().into();
    let app = tauri::test::mock_builder()
        .manage(state)
        .build(ctx)
        .unwrap();
    assert_eq!(app.path().app_data_dir().unwrap(), path);
    let state = app.state::<crate::app_state::AppState>();
    let scope =
        crate::managed_agents::retention::active_retention_scope(app.handle(), &state).unwrap();
    crate::managed_agents::retention::open_retention_db(&scope.db_path).unwrap();
    app
}
#[test]
fn copy_suppresses_both_outbound_kinds_and_restore_candidates() {
    use crate::managed_agents::{
        restore::select_auto_start_candidates,
        retention::{get_pending_sync, open_retention_db},
    };
    let dir = tempfile::tempdir().unwrap();
    let (mut rs, _) = records();
    let keys = nostr::Keys::generate();
    let c = context(EvidenceReadiness::Ready);
    rs[1].device_host_binding = Some("copied-marker".into());
    rs[1].start_on_app_launch = true;
    write(dir.path(), &rs);
    queue_device_home_events(dir.path(), &keys, &dir.path().join("retention.db"), &c).unwrap();
    let conn = open_retention_db(&dir.path().join("retention.db")).unwrap();
    assert!(
        get_pending_sync(&conn).unwrap().is_empty(),
        "copied home published30175/30177"
    );
    assert!(select_auto_start_candidates(&rs, Some(&c))
        .unwrap()
        .is_empty());
}
#[test]
fn publication_retry_is_idempotent_after_atomic_home_save() {
    use crate::managed_agents::retention::{get_pending_sync, mark_synced, open_retention_db};
    let dir = tempfile::tempdir().unwrap();
    let (rs, key) = records();
    let c = context(EvidenceReadiness::Ready);
    write(dir.path(), &rs);
    migrate_device_homes_in_dir(dir.path(), &c, |_| Ok(Some(key.clone()))).unwrap();
    let bad = dir.path().join("missing-parent").join("retention.db");
    assert!(queue_device_home_events(dir.path(), &key, &bad, &c).is_err());
    let db = dir.path().join("retention.db");
    queue_device_home_events(dir.path(), &key, &db, &c).unwrap();
    let conn = open_retention_db(&db).unwrap();
    let pending = get_pending_sync(&conn).unwrap();
    assert_eq!(pending.len(), 2);
    for r in pending {
        mark_synced(&conn, r.kind, &r.pubkey, &r.d_tag, r.created_at, &r.content).unwrap();
    }
    assert!(!migrate_device_homes_in_dir(dir.path(), &c, |_| panic!(
        "retry must not mint/resolve key"
    ))
    .unwrap());
    queue_device_home_events(dir.path(), &key, &db, &c).unwrap();
    assert!(get_pending_sync(&conn).unwrap().is_empty());
}
#[test]
fn workspace_hook_defers_legacy_but_reclaims_proven_origin_before_sync() {
    use tauri::Manager;
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    let base = super::super::managed_agents_base_dir(app.handle()).unwrap();
    let (mut rs, _) = records();
    let mut c = context(EvidenceReadiness::Pending);
    c.scope = super::super::device_home_sync::capture_scope(&state).unwrap();
    write(&base, &rs);
    migrate_device_homes_before_sync_with(
        app.handle(),
        |_, _| Ok(c),
        |_| panic!("pending legacy key lookup"),
    )
    .unwrap();
    assert!(read(&base)[1].device_host_binding.is_none());
    let mut c = context(EvidenceReadiness::Pending);
    c.scope = super::super::device_home_sync::capture_scope(&state).unwrap();
    rs[1].device_host_binding = Some(c.proof.binding().into());
    rs[0].origin_device_id = Some("old-public-id".into());
    write(&base, &rs);
    let new_id = c.device.device_id.clone();
    migrate_device_homes_before_sync_with(
        app.handle(),
        |_, _| Ok(c),
        |_| panic!("proven needs no key lookup"),
    )
    .unwrap();
    assert_eq!(
        read(&base)[0].origin_device_id.as_deref(),
        Some(new_id.as_str())
    );
    let scope = super::super::retention::active_retention_scope(app.handle(), &state).unwrap();
    let conn = super::super::retention::open_retention_db(&scope.db_path).unwrap();
    assert_eq!(
        super::super::retention::get_pending_sync(&conn)
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn workspace_proof_error_and_shared_only_behavior() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let base = super::super::managed_agents_base_dir(app.handle()).unwrap();
    let (mut rs, _) = records();
    write(&base, &rs);
    let before = std::fs::read(base.join("managed-agents.json")).unwrap();
    assert!(migrate_device_homes_before_sync_with(
        app.handle(),
        |_, _| Err("proof locked".into()),
        |_| panic!("failed proof key lookup")
    )
    .is_err());
    assert_eq!(
        before,
        std::fs::read(base.join("managed-agents.json")).unwrap()
    );
    rs[0].share_across_devices = Some(true);
    write(&base, &rs);
    migrate_device_homes_before_sync_with(
        app.handle(),
        |_, _| panic!("shared-only proof lookup"),
        |_| panic!("shared-only key lookup"),
    )
    .unwrap();
    assert!(publication_allowed(&rs[1], rs[0].to_definition_view().as_ref(), None).unwrap());
}
#[test]
fn available_key_must_match_instance_and_atomic_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let (rs, _) = records();
    write(dir.path(), &rs);
    let before = std::fs::read(dir.path().join("managed-agents.json")).unwrap();
    assert!(
        migrate_device_homes_in_dir(dir.path(), &context(EvidenceReadiness::Ready), |_| Ok(
            Some(nostr::Keys::generate())
        ))
        .is_err()
    );
    assert_eq!(
        before,
        std::fs::read(dir.path().join("managed-agents.json")).unwrap()
    );
}

#[test]
fn keyless_definition_is_not_claimed_and_later_key_error_is_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let (mut rs, key) = records();
    write(dir.path(), &rs[..1]);
    let c = context(EvidenceReadiness::Ready);
    assert!(!migrate_device_homes_in_dir(dir.path(), &c, |_| panic!(
        "keyless definition resolved key"
    ))
    .unwrap());
    let mut second = rs[1].clone();
    second.pubkey = nostr::Keys::generate().public_key().to_hex();
    rs.push(second);
    write(dir.path(), &rs);
    let before = std::fs::read(dir.path().join("managed-agents.json")).unwrap();
    let mut calls = 0;
    assert!(migrate_device_homes_in_dir(dir.path(), &c, |_| {
        calls += 1;
        if calls == 1 {
            Ok(Some(key.clone()))
        } else {
            Err("second key locked".into())
        }
    })
    .is_err());
    assert_eq!(calls, 2);
    assert_eq!(
        before,
        std::fs::read(dir.path().join("managed-agents.json")).unwrap()
    );
}
#[test]
fn stale_migration_context_cannot_write_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let base = super::super::managed_agents_base_dir(app.handle()).unwrap();
    let (rs, key) = records();
    write(&base, &rs);
    let before = std::fs::read(base.join("managed-agents.json")).unwrap();
    assert!(migrate_device_homes_locked_with(
        app.handle(),
        &context(EvidenceReadiness::Ready),
        |_| Ok(Some(key.clone()))
    )
    .is_err());
    assert_eq!(
        before,
        std::fs::read(base.join("managed-agents.json")).unwrap()
    );
}

#[test]
fn proven_home_publishes_definition_among_foreign_copies() {
    use crate::managed_agents::retention::{get_pending_sync, open_retention_db};
    let dir = tempfile::tempdir().unwrap();
    let (mut rs, key) = records();
    let c = context(EvidenceReadiness::Ready);
    rs[1].device_host_binding = Some(c.proof.binding().into());
    let mut foreign = rs[1].clone();
    foreign.pubkey = nostr::Keys::generate().public_key().to_hex();
    foreign.device_host_binding = Some("foreign-marker".into());
    rs.push(foreign);
    write(dir.path(), &rs);
    migrate_device_homes_in_dir(dir.path(), &c, |_| {
        panic!("proven and foreign records need no key lookup")
    })
    .unwrap();
    let db = dir.path().join("retention.db");
    queue_device_home_events(dir.path(), &key, &db, &c).unwrap();
    let conn = open_retention_db(&db).unwrap();
    let pending = get_pending_sync(&conn).unwrap();
    assert_eq!(
        pending.len(),
        2,
        "proven local home must publish30175 and only its30177"
    );
    assert!(pending.iter().any(|r| r.kind == 30175));
    assert!(pending
        .iter()
        .any(|r| r.kind == 30177 && r.d_tag == rs[1].pubkey));
    assert!(!pending
        .iter()
        .any(|r| r.kind == 30177 && r.d_tag == rs[2].pubkey));
}
