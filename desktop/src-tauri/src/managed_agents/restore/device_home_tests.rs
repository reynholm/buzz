use super::*;
use crate::managed_agents::{
    definition_home::EvidenceReadiness,
    device_home_migration::tests::{app, context, definition, records, write},
    persona_device_view::read_policy_records,
    storage::{hydrate_keys_with, persist_agent_keys_with, KeyStore},
};
use crate::secret_store::KeyringProbe;
use nostr::ToBech32;
use std::{cell::RefCell, collections::HashMap};

#[derive(Default)]
struct Secrets {
    entries: RefCell<HashMap<String, String>>,
    operations: RefCell<Vec<String>>,
}
impl KeyStore for Secrets {
    fn probe(&self, name: &str) -> KeyringProbe {
        self.operations.borrow_mut().push(name.into());
        KeyringProbe::ReachableButEmpty
    }
    fn load(&self, name: &str) -> Result<Option<String>, String> {
        self.operations.borrow_mut().push(name.into());
        Ok(self.entries.borrow().get(name).cloned())
    }
    fn load_all_readonly(&self) -> Result<Option<HashMap<String, String>>, String> {
        panic!("restore cannot require global secret reads")
    }
    fn write_and_verify(&self, name: &str, value: &str) -> Result<(), String> {
        self.operations.borrow_mut().push(name.into());
        self.entries.borrow_mut().insert(name.into(), value.into());
        Ok(())
    }
    fn store_all(&self, _: &HashMap<String, String>) -> Result<(), String> {
        panic!("restore cannot require global secret writes")
    }
}
fn mixed_records() -> (Vec<super::super::ManagedAgentRecord>, String) {
    let (mut raw, copied_key) = records();
    raw[1].start_on_app_launch = true;
    raw[1].private_key_nsec = copied_key.secret_key().to_bech32().unwrap();
    raw[1].device_host_binding = Some("foreign-marker".into());
    raw[1].last_error = Some("preserve the copied row".into());
    let mut shared = definition();
    shared.id = "shared".into();
    shared.share_across_devices = Some(true);
    let mut allowed = shared.clone().into_agent_record();
    let key = nostr::Keys::generate();
    allowed.pubkey = key.public_key().to_hex();
    allowed.private_key_nsec = key.secret_key().to_bech32().unwrap();
    allowed.persona_id = Some(shared.id.clone());
    allowed.start_on_app_launch = true;
    let pubkey = allowed.pubkey.clone();
    raw.push(shared.into_agent_record());
    raw.push(allowed);
    (raw, pubkey)
}
fn assert_preserved(
    base: &std::path::Path,
    original: &[super::super::ManagedAgentRecord],
    secrets: &Secrets,
) {
    let after = read_policy_records(&base.join("managed-agents.json")).unwrap();
    for excluded in [&original[0], &original[1], &original[2]] {
        let current = after
            .iter()
            .find(|r| r.pubkey == excluded.pubkey && r.slug == excluded.slug)
            .unwrap();
        assert_eq!(
            serde_json::to_value(current).unwrap(),
            serde_json::to_value(excluded).unwrap(),
            "excluded row/definition changed"
        );
    }
    let copied_name = format!("agent:{}", original[1].pubkey);
    assert!(
        !secrets.operations.borrow().contains(&copied_name),
        "foreign copy touched secret store"
    );
    assert!(!secrets.entries.borrow().contains_key(&copied_name));
}
#[test]
fn mixed_restore_phase_a_save_and_phase_c_writeback_preserve_foreign_inline_copy() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let base = super::super::managed_agents_base_dir(app.handle()).unwrap();
    let (raw, allowed) = mixed_records();
    write(&base, &raw);
    let secrets = Secrets::default();
    let candidates = prepare_restore_phase_a_with(
        app.handle(),
        &AtomicBool::new(false),
        |_, _| Ok(context(EvidenceReadiness::Ready)),
        |rs| hydrate_keys_with(&secrets, rs),
        |rs| persist_agent_keys_with(&secrets, rs),
        |_, eligible| Ok((false, eligible.iter().cloned().collect())),
    )
    .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].pubkey, allowed);
    assert_preserved(&base, &raw, &secrets);
    let phase_a = read_policy_records(&base.join("managed-agents.json")).unwrap();
    assert!(
        phase_a
            .iter()
            .find(|r| r.pubkey == allowed)
            .unwrap()
            .persona_source_version
            .is_some(),
        "Phase A must persist its refreshed snapshot"
    );
    let started = candidates.iter().map(|r| r.pubkey.clone()).collect();
    complete_restore_phase_c_with(
        app.handle(),
        &started,
        |_, _| panic!("shared-only restore targets requested authority"),
        |rs| hydrate_keys_with(&secrets, rs),
        |rs| persist_agent_keys_with(&secrets, rs),
        |rs| {
            find_managed_agent_mut(rs, &allowed)?.last_error =
                Some("injected process failure".into());
            Ok(())
        },
    )
    .unwrap();
    assert_preserved(&base, &raw, &secrets);
    let after = read_policy_records(&base.join("managed-agents.json")).unwrap();
    assert_eq!(
        after
            .iter()
            .find(|r| r.pubkey == allowed)
            .unwrap()
            .last_error
            .as_deref(),
        Some("injected process failure")
    );
    assert!(
        !secrets.operations.borrow().is_empty(),
        "allowed candidate must exercise actual key-store operations"
    );
}

#[test]
fn mixed_restore_phase_c_reload_and_save_never_import_excluded_copy() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let base = super::super::managed_agents_base_dir(app.handle()).unwrap();
    let (mut raw, allowed) = mixed_records();
    let mut disabled = raw[3].clone();
    disabled.pubkey = nostr::Keys::generate().public_key().to_hex();
    disabled.start_on_app_launch = false;
    let disabled_key = disabled.pubkey.clone();
    let disabled_inline = disabled.private_key_nsec.clone();
    raw.push(disabled);
    write(&base, &raw);
    let secrets = Secrets::default();
    complete_restore_phase_c_with(
        app.handle(),
        &[allowed.clone()].into_iter().collect(),
        |_, _| panic!("shared-only writeback targets requested authority"),
        |rs| hydrate_keys_with(&secrets, rs),
        |rs| persist_agent_keys_with(&secrets, rs),
        |rs| {
            find_managed_agent_mut(rs, &allowed)?.last_started_at =
                Some("native writeback boundary".into());
            Ok(())
        },
    )
    .unwrap();
    assert_preserved(&base, &raw, &secrets);
    let after = read_policy_records(&base.join("managed-agents.json")).unwrap();
    assert_eq!(
        after
            .iter()
            .find(|r| r.pubkey == disabled_key)
            .unwrap()
            .private_key_nsec,
        disabled_inline
    );
    assert!(!secrets
        .operations
        .borrow()
        .contains(&format!("agent:{disabled_key}")));
}

#[test]
fn proven_disabled_housekeeping_uses_captured_context_without_key_operations() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let base = super::super::managed_agents_base_dir(app.handle()).unwrap();
    let (mut raw, allowed) = mixed_records();
    let c = context(EvidenceReadiness::Ready);
    raw[2].share_across_devices = Some(false);
    raw[3].device_host_binding = Some(c.proof.binding().into());
    let mut disabled = raw[3].clone();
    disabled.pubkey = nostr::Keys::generate().public_key().to_hex();
    disabled.start_on_app_launch = false;
    let disabled_key = disabled.pubkey.clone();
    raw.push(disabled);
    write(&base, &raw);
    let secrets = Secrets::default();
    let candidates = prepare_restore_phase_a_with(
        app.handle(),
        &AtomicBool::new(false),
        |_, _| Ok(c),
        |rs| hydrate_keys_with(&secrets, rs),
        |rs| persist_agent_keys_with(&secrets, rs),
        |rs, eligible| {
            for r in rs {
                r.last_stopped_at = Some("proven housekeeping".into());
            }
            Ok((true, eligible.iter().cloned().collect()))
        },
    )
    .unwrap();
    assert_eq!(candidates[0].pubkey, allowed);
    assert_preserved(&base, &raw, &secrets);
    let after = read_policy_records(&base.join("managed-agents.json")).unwrap();
    let disabled = after.iter().find(|r| r.pubkey == disabled_key).unwrap();
    assert_eq!(
        disabled.last_stopped_at.as_deref(),
        Some("proven housekeeping")
    );
    assert_eq!(disabled.private_key_nsec, raw[4].private_key_nsec);
    assert!(!secrets
        .operations
        .borrow()
        .contains(&format!("agent:{disabled_key}")));
}

#[test]
fn disabled_safe_housekeeping_persists_without_key_operations() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let base = super::super::managed_agents_base_dir(app.handle()).unwrap();
    let (mut raw, shared) = mixed_records();
    raw[1].start_on_app_launch = false;
    raw[3].start_on_app_launch = false;
    let mut standalone = raw[3].clone();
    standalone.pubkey = nostr::Keys::generate().public_key().to_hex();
    standalone.persona_id = None;
    let standalone_key = standalone.pubkey.clone();
    raw.push(standalone);
    write(&base, &raw);
    let secrets = Secrets::default();
    let candidates = prepare_restore_phase_a_with(
        app.handle(),
        &AtomicBool::new(false),
        |_, _| panic!("disabled private row forced authority lookup"),
        |rs| hydrate_keys_with(&secrets, rs),
        |rs| persist_agent_keys_with(&secrets, rs),
        |rs, eligible| {
            assert!(eligible.is_empty());
            for r in rs {
                r.last_stopped_at = Some("housekeeping".into());
            }
            Ok((true, vec![]))
        },
    )
    .unwrap();
    assert!(candidates.is_empty());
    assert_preserved(&base, &raw, &secrets);
    let after = read_policy_records(&base.join("managed-agents.json")).unwrap();
    for key in [&shared, &standalone_key] {
        let original = raw.iter().find(|r| &r.pubkey == key).unwrap();
        let current = after.iter().find(|r| &r.pubkey == key).unwrap();
        assert_eq!(current.last_stopped_at.as_deref(), Some("housekeeping"));
        assert_eq!(current.private_key_nsec, original.private_key_nsec);
    }
    assert!(secrets.operations.borrow().is_empty());
}

#[test]
fn phase_c_fresh_authority_blocks_changed_target_and_preserves_concurrent_rows() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let base = super::super::managed_agents_base_dir(app.handle()).unwrap();
    let (raw, allowed) = mixed_records();
    write(&base, &raw);
    let secrets = Secrets::default();
    let candidates = prepare_restore_phase_a_with(
        app.handle(),
        &AtomicBool::new(false),
        |_, _| Ok(context(EvidenceReadiness::Ready)),
        |rs| hydrate_keys_with(&secrets, rs),
        |rs| persist_agent_keys_with(&secrets, rs),
        |_, eligible| Ok((false, eligible.iter().cloned().collect())),
    )
    .unwrap();
    assert_eq!(candidates[0].pubkey, allowed);
    let mut current = read_policy_records(&base.join("managed-agents.json")).unwrap();
    current
        .iter_mut()
        .find(|r| r.pubkey.is_empty() && r.slug.as_deref() == Some("shared"))
        .unwrap()
        .share_across_devices = Some(false);
    let changed = current.iter_mut().find(|r| r.pubkey == allowed).unwrap();
    changed.device_host_binding = Some("new-foreign-marker".into());
    changed.private_key_nsec = raw[3].private_key_nsec.clone();
    let mut concurrent = raw[1].clone();
    concurrent.pubkey = nostr::Keys::generate().public_key().to_hex();
    concurrent.name = "concurrently added row".into();
    current.push(concurrent);
    write(&base, &current);
    let before = serde_json::to_value(&current).unwrap();
    secrets.operations.borrow_mut().clear();
    complete_restore_phase_c_with(
        app.handle(),
        &[allowed.clone()].into_iter().collect(),
        |_, _| Ok(context(EvidenceReadiness::Ready)),
        |rs| hydrate_keys_with(&secrets, rs),
        |rs| persist_agent_keys_with(&secrets, rs),
        |rs| {
            assert!(
                rs.iter().all(|r| r.pubkey != allowed),
                "stale PhaseA target survived fresh foreign binding"
            );
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(read_policy_records(&base.join("managed-agents.json")).unwrap())
            .unwrap(),
        before
    );
    assert!(secrets.operations.borrow().is_empty());
}

#[cfg(feature = "mesh-llm")]
#[test]
fn mesh_preflight_error_writeback_preserves_excluded_inline_copy() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let base = super::super::managed_agents_base_dir(app.handle()).unwrap();
    let (raw, allowed) = mixed_records();
    write(&base, &raw);
    let secrets = Secrets::default();
    persist_restore_error_with(
        app.handle(),
        &allowed,
        "injected mesh preflight failure".into(),
        |_, _| panic!("shared-only mesh error target requested proof"),
        |rs| hydrate_keys_with(&secrets, rs),
        |rs| persist_agent_keys_with(&secrets, rs),
    )
    .unwrap();
    assert_preserved(&base, &raw, &secrets);
    let after = read_policy_records(&base.join("managed-agents.json")).unwrap();
    assert_eq!(
        after
            .iter()
            .find(|r| r.pubkey == allowed)
            .unwrap()
            .last_error
            .as_deref(),
        Some("injected mesh preflight failure")
    );
}
