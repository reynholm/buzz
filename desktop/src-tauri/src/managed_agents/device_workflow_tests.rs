//! Two isolated stores exercise production adapters; MockRuntime and injected
//! secret/process boundaries do not prove native Wry, keychain or physical devices.
use super::{
    access_policy::{build_respond_to_env_with_policy, projected_access_with_policy},
    definition_home::{EvidenceReadiness, HomeKind},
    device_creation::{bind_new_instance, creation_phase_locked, stamp_new_definition},
    device_home_migration::tests::{app, definition},
    device_home_operations::{commit_claim_in_dir, commit_new_pairs_in_dir, enqueue_home_events},
    device_runtime::{inbound_refresh_phase_with, start_pair_phase_with},
    persona_device_view::{read_policy_records, read_remote_evidence, DevicePolicyContext},
    retention::{get_pending_sync, open_retention_db, scoped_retention_db_path},
    AgentDefinition, ManagedAgentRecord, RespondTo,
};
use crate::{
    app_state::AppState,
    device_identity::{load_or_create_device_identity, load_or_create_host_proof},
    secret_store::{SecretStore, TestBlobBackend},
};
use nostr::{Event, JsonUtil, Keys};
use std::{
    cell::Cell,
    sync::{Arc, Mutex},
};
use tauri::Manager;

struct ProofBlob(Mutex<Option<Vec<u8>>>);
impl TestBlobBackend for ProofBlob {
    fn read(&self) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn write(&self, bytes: &[u8]) -> Result<(), String> {
        *self.0.lock().unwrap() = Some(bytes.to_vec());
        Ok(())
    }
}
struct Device {
    app: tauri::App<tauri::test::MockRuntime>,
    dir: tempfile::TempDir,
    proof: SecretStore,
}
impl Device {
    fn new(owner: &Keys, label: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        *app.state::<AppState>().keys.lock().unwrap() = owner.clone();
        load_or_create_device_identity(&dir.path().join("device.json"), label).unwrap();
        let mut proof = SecretStore::keyring(format!("task12-{}", uuid::Uuid::new_v4()));
        proof.test_backend = Some(Arc::new(ProofBlob(Mutex::new(None))));
        load_or_create_host_proof(&proof).unwrap();
        let device = Self { app, dir, proof };
        open_retention_db(&device.db()).unwrap();
        device.save(&[]);
        device
    }
    fn base(&self) -> std::path::PathBuf {
        self.dir.path().join("agents")
    }
    fn db(&self) -> std::path::PathBuf {
        let scope = super::device_home_sync::capture_scope(&self.app.state::<AppState>()).unwrap();
        scoped_retention_db_path(&self.base(), &scope.relay_url, &scope.owner_pubkey)
    }
    fn context(&self, readiness: EvidenceReadiness) -> DevicePolicyContext {
        let scope = super::device_home_sync::capture_scope(&self.app.state::<AppState>()).unwrap();
        DevicePolicyContext {
            evidence: read_remote_evidence(&self.db(), &scope.owner_pubkey).unwrap(),
            scope,
            device: load_or_create_device_identity(
                &self.dir.path().join("device.json"),
                "unchanged",
            )
            .unwrap(),
            proof: load_or_create_host_proof(&self.proof).unwrap(),
            readiness,
        }
    }
    fn rows(&self) -> Vec<ManagedAgentRecord> {
        read_policy_records(&self.base().join("managed-agents.json")).unwrap()
    }
    fn save(&self, rows: &[ManagedAgentRecord]) {
        super::storage::atomic_write_json_restricted(
            &self.base().join("managed-agents.json"),
            &serde_json::to_vec_pretty(rows).unwrap(),
        )
        .unwrap();
    }
    fn seed(&self, shared: Option<bool>) -> AgentDefinition {
        let mut d = definition();
        stamp_new_definition(
            &mut d,
            shared,
            &self.context(EvidenceReadiness::Ready).device,
        );
        self.save(&[d.clone().into_agent_record()]);
        d
    }
    fn current(&self) -> AgentDefinition {
        self.rows()
            .iter()
            .filter_map(ManagedAgentRecord::to_definition_view)
            .find(|d| d.id == "one")
            .unwrap()
    }
    fn create(
        &self,
        effects: &Cell<usize>,
        readiness: EvidenceReadiness,
    ) -> Result<String, String> {
        let state = self.app.state::<AppState>();
        let _lock = state.managed_agents_store_lock.lock().unwrap();
        let result = creation_phase_locked(
            self.app.handle(),
            &state,
            Some("one"),
            None,
            |_, _| Ok(self.context(readiness)),
            |context| {
                effects.set(effects.get() + 1); // mint/key/save/publication boundary
                let mut record = self.current().into_agent_record();
                record.pubkey = Keys::generate().public_key().to_hex();
                record.persona_id = Some("one".into());
                record.start_on_app_launch = true;
                let pubkey = record.pubkey.clone();
                if let Some(context) = context {
                    commit_claim_in_dir(
                        self.dir.path(),
                        &context,
                        "one",
                        record,
                        &state.signing_keys()?,
                        |rows| {
                            self.save(rows);
                            Ok(())
                        },
                        |operation| {
                            enqueue_home_events(&mut open_retention_db(&self.db())?, operation)
                        },
                    )?;
                } else {
                    bind_new_instance(&mut record, &self.context(readiness).proof);
                    commit_new_pairs_in_dir(self.dir.path(), vec![], vec![record], |_| {})?;
                }
                Ok(pubkey)
            },
        );
        drop(_lock);
        if result.is_ok() {
            self.reconcile()?;
        }
        result
    }
    fn receive(&self, event: &Event) {
        event.verify().unwrap();
        crate::commands::reconcile_inbound_workflow_with(
            event.as_json(),
            "wss://test".into(),
            self.app.handle().clone(),
            |_, _| Ok(self.context(EvidenceReadiness::Ready)),
            || {},
        )
        .unwrap();
    }
    fn pending(&self) -> Vec<Event> {
        get_pending_sync(&open_retention_db(&self.db()).unwrap())
            .unwrap()
            .iter()
            .map(|row| Event::from_json(&row.raw_event).unwrap())
            .collect()
    }
    fn sync_to(&self, other: &Self) {
        for event in self.pending() {
            other.receive(&event);
        }
    }
    fn start(&self, pubkey: &str, effects: &Cell<usize>) -> Result<(), String> {
        let state = self.app.state::<AppState>();
        let _lock = state.managed_agents_store_lock.lock().unwrap();
        start_pair_phase_with(
            self.app.handle(),
            &state,
            pubkey,
            None,
            |_, _| Ok(self.context(EvidenceReadiness::Ready)),
            |_, _, _| {
                effects.set(effects.get() + 1);
                Ok(())
            },
        )
    }
    fn restore(&self, effects: &Cell<usize>) -> Vec<ManagedAgentRecord> {
        super::restore::prepare_restore_workflow_with(
            None,
            self.app.handle(),
            &std::sync::atomic::AtomicBool::new(false),
            |_, _| Ok(self.context(EvidenceReadiness::Ready)),
            |rows| effects.set(effects.get() + rows.len()),
            |_| {},
            |_, eligible| Ok((false, eligible.iter().cloned().collect())),
        )
        .unwrap()
    }
    fn reconcile(&self) -> Result<(), String> {
        super::reconcile::reconcile_agents_to_events_with(
            self.app.handle(),
            &self.app.state::<AppState>().signing_keys()?,
            &self.db(),
            |_, _| Ok(self.context(EvidenceReadiness::Ready)),
        )
    }
    fn refresh(&self, pubkey: &str, effects: &Cell<usize>) -> Option<()> {
        let state = self.app.state::<AppState>();
        let _lock = state.managed_agents_store_lock.lock().unwrap();
        inbound_refresh_phase_with(
            self.app.handle(),
            &state,
            pubkey,
            None,
            |_, _| Ok(self.context(EvidenceReadiness::Ready)),
            |_, _, _| {
                effects.set(effects.get() + 1);
                Ok(())
            },
        )
        .unwrap()
    }
}

#[test]
fn private_two_device_single_pubkey() {
    let owner = Keys::generate();
    let (a, b) = (
        Device::new(&owner, "Notebook A"),
        Device::new(&owner, "Notebook B"),
    );
    a.seed(None); // omitted toggle is private
    let effects_a = Cell::new(0);
    let private_pubkey = a.create(&effects_a, EvidenceReadiness::Ready).unwrap();
    a.sync_to(&b);
    let effects_b = Cell::new(0);
    for _ in 0..3 {
        // Frontend onboarding/team/profile owning callers are covered separately;
        // all reach this native creation adapter. This test does not mount them.
        for _entrypoint in ["onboarding", "team", "profile"] {
            let error = b.create(&effects_b, EvidenceReadiness::Ready).unwrap_err();
            assert!(error.contains("definition_hosted_elsewhere"));
            assert!(error.contains("Notebook A"));
        }
        assert!(b.start(&private_pubkey, &effects_b).is_err());
        assert!(b.restore(&effects_b).is_empty());
        assert!(b.refresh(&private_pubkey, &effects_b).is_none());
        b.reconcile().unwrap();
        assert!(b.pending().is_empty(), "B authored copied identity on boot");
        a.sync_to(&b); // idempotent head replay
    }
    let private_agent_pubkeys_after_repeated_b_boot: std::collections::HashSet<_> = a
        .rows()
        .into_iter()
        .chain(b.rows())
        .filter(|r| !r.pubkey.is_empty())
        .map(|r| r.pubkey)
        .collect();
    assert_eq!(private_agent_pubkeys_after_repeated_b_boot.len(), 1);
    assert_eq!(
        effects_b.get(),
        0,
        "B reached mint/process/secret side effects"
    );
    a.start(&private_pubkey, &effects_a).unwrap();
    assert_eq!(a.restore(&effects_a).len(), 1);
    assert!(a.refresh(&private_pubkey, &effects_a).is_some());
    // A keeps proof after public metadata deletion, and migration republishes new lineage.
    let old_id = a.context(EvidenceReadiness::Ready).device.device_id;
    std::fs::remove_file(a.dir.path().join("device.json")).unwrap();
    let c = a.context(EvidenceReadiness::Ready);
    assert_ne!(old_id, c.device.device_id);
    assert!(
        super::device_home_migration::migrate_device_homes_in_dir(&a.base(), &c, |_| panic!(
            "proven home read agent secret"
        ))
        .unwrap()
    );
    a.start(&private_pubkey, &effects_a).unwrap();
    // Copy full post-migration JSON, including binding, into B. No marker is copied.
    let mut copied = a.rows();
    copied
        .iter_mut()
        .find(|r| r.pubkey == private_pubkey)
        .unwrap()
        .name = "Copied edit on B".into();
    b.save(&copied);
    let mut old_writer = b.current();
    old_writer.share_across_devices = None;
    old_writer.origin_device_id = None;
    old_writer.origin_device_label = None;
    old_writer.origin_released = None;
    old_writer.display_name = "Legacy writer edit".into();
    let old_event = super::persona_events::build_persona_event(&old_writer)
        .unwrap()
        .custom_created_at(nostr::Timestamp::from(
            nostr::Timestamp::now().as_secs() + 1000,
        ))
        .sign_with_keys(&owner)
        .unwrap();
    b.receive(&old_event);
    assert_eq!(b.current().display_name, "Legacy writer edit");
    assert_eq!(b.current().share_across_devices, Some(false));
    assert_eq!(b.current().origin_device_id, a.current().origin_device_id);
    assert_eq!(b.current().origin_released, Some(false));
    assert!(b.start(&private_pubkey, &effects_b).is_err());
    assert!(b.restore(&effects_b).is_empty());
    assert!(b.refresh(&private_pubkey, &effects_b).is_none());
    b.reconcile().unwrap();
    assert!(b.pending().is_empty());
    assert_eq!(effects_b.get(), 0);
}

#[test]
fn release_then_reclaim() {
    for release_first in [true, false] {
        let owner = Keys::generate();
        let (a, b) = (
            Device::new(&owner, "Notebook A"),
            Device::new(&owner, "Notebook B"),
        );
        a.seed(Some(false));
        let effects = Cell::new(0);
        let first = a.create(&effects, EvidenceReadiness::Ready).unwrap();
        a.sync_to(&b);
        let c = a.context(EvidenceReadiness::Ready);
        let op = super::device_home_operations::delete::prepare_delete_in_dir(
            a.dir.path(),
            &c,
            &first,
            &owner,
        )
        .unwrap();
        super::device_home_operations::delete::commit_delete_in_dir(
            a.dir.path(),
            &op,
            |rows| {
                a.save(rows);
                Ok(())
            },
            |operation| enqueue_home_events(&mut open_retention_db(&a.db())?, operation),
        )
        .unwrap();
        assert!(!a.rows().iter().any(|r| r.pubkey == first));
        assert_eq!(a.current().origin_released, Some(true));
        let release = op
            .signed_events
            .iter()
            .find(|e| e.kind.as_u16() == 30175)
            .unwrap();
        let tombstone = op
            .signed_events
            .iter()
            .find(|e| e.kind.as_u16() == 5)
            .unwrap();
        let (early, late) = if release_first {
            (release, tombstone)
        } else {
            (tombstone, release)
        };
        b.receive(early);
        let before_tombstone = b
            .context(EvidenceReadiness::Ready)
            .project(b.current(), &b.rows())
            .capabilities;
        assert!(
            !before_tombstone.can_create_instance,
            "one half of release/tombstone authorized claim"
        );
        let refused = Cell::new(0);
        assert!(b.create(&refused, EvidenceReadiness::Ready).is_err());
        assert_eq!(refused.get(), 0);
        b.receive(late);
        let after_release_and_tombstone = b
            .context(EvidenceReadiness::Ready)
            .project(b.current(), &b.rows())
            .capabilities;
        assert!(after_release_and_tombstone.can_create_instance);
        assert!(b
            .create(&refused, EvidenceReadiness::Pending)
            .unwrap_err()
            .contains("device_home_sync_pending"));
        assert_eq!(refused.get(), 0);
        let second = b.create(&refused, EvidenceReadiness::Ready).unwrap();
        assert_ne!(first, second);
        b.start(&second, &refused).unwrap();
        b.sync_to(&a);
        let home_a = a
            .context(EvidenceReadiness::Ready)
            .project(a.current(), &a.rows());
        assert_eq!(home_a.home.unwrap().kind, HomeKind::Remote);
        assert!(!home_a.capabilities.can_create_instance);
        assert!(a.start(&second, &refused).is_err());
        assert_eq!(
            b.current().origin_device_id,
            Some(b.context(EvidenceReadiness::Ready).device.device_id)
        );
        assert_eq!(b.rows().iter().filter(|r| !r.pubkey.is_empty()).count(), 1);
        // Replaying the old tombstone must preserve the new identity.
        b.receive(tombstone);
        b.start(&second, &refused).unwrap();
    }
}

#[test]
fn shared_two_device_baseline() {
    let owner = Keys::generate();
    let (a, b) = (
        Device::new(&owner, "Notebook A"),
        Device::new(&owner, "Notebook B"),
    );
    let shared = a.seed(Some(true));
    let public = super::persona_events::build_persona_event(&shared)
        .unwrap()
        .sign_with_keys(&owner)
        .unwrap();
    b.receive(&public);
    let effects = Cell::new(0);
    let first = a.create(&effects, EvidenceReadiness::Failed).unwrap();
    let second = b.create(&effects, EvidenceReadiness::Pending).unwrap();
    assert_ne!(first, second);
    a.sync_to(&b);
    b.sync_to(&a);
    // Inbound 30177 retains remote identity; it does not manufacture a local
    // instance record. Each store starts/restores its own baseline-created row.
    for (device, pubkey) in [(&a, &first), (&b, &second)] {
        device.start(pubkey, &effects).unwrap();
        assert_eq!(device.restore(&effects).len(), 1);
        assert!(device.refresh(pubkey, &effects).is_some());
        device.reconcile().unwrap();
        assert_eq!(device.current().share_across_devices, Some(true));
    }
    // Explicit sharing also preserves baseline execution of an imported copy.
    let mut copied = b.rows();
    copied.push(a.rows().into_iter().find(|r| r.pubkey == first).unwrap());
    b.save(&copied);
    b.start(&first, &effects).unwrap();
    assert_eq!(b.restore(&effects).len(), 2);
}

#[test]
fn device_policy_access_matrix() {
    // Complete desktop cross-product: sharing absent/false/true x own/foreign
    // binding x Ready/Pending/Failed x all three Desktop access modes x distribution clamp.
    // Actual instruction authors are checked by buzz-acp's owning unit suite;
    // this matrix tests its production Desktop env and provider inputs.
    for sharing in [None, Some(false), Some(true)] {
        for foreign in [false, true] {
            for readiness in [
                EvidenceReadiness::Ready,
                EvidenceReadiness::Pending,
                EvidenceReadiness::Failed,
            ] {
                for mode in [
                    RespondTo::OwnerOnly,
                    RespondTo::Allowlist,
                    RespondTo::Anyone,
                ] {
                    for clamp in [false, true] {
                        let owner = Keys::generate();
                        let device = Device::new(&owner, "Matrix host");
                        let mut d = device.seed(sharing);
                        d.share_across_devices = sharing; // include actual legacy absent bytes
                        d.respond_to = Some(mode.as_str().into());
                        d.respond_to_allowlist = vec!["a".repeat(64)];
                        let mut i = d.clone().into_agent_record();
                        i.pubkey = Keys::generate().public_key().to_hex();
                        i.persona_id = Some(d.id.clone());
                        i.respond_to = mode;
                        i.respond_to_allowlist = d.respond_to_allowlist.clone();
                        i.device_host_binding = Some(if foreign {
                            "foreign-proof".into()
                        } else {
                            device.context(readiness).proof.binding().into()
                        });
                        device.save(&[d.into_agent_record(), i.clone()]);
                        let state = device.app.state::<AppState>();
                        let _lock = state.managed_agents_store_lock.lock().unwrap();
                        let effects = Cell::new(0);
                        let result = start_pair_phase_with(
                            device.app.handle(),
                            &state,
                            &i.pubkey,
                            None,
                            |_, _| Ok(device.context(readiness)),
                            |record, _, _| {
                                effects.set(1);
                                let expected_mode = if clamp { RespondTo::OwnerOnly } else { mode };
                                let env: std::collections::HashMap<_, _> =
                                    build_respond_to_env_with_policy(
                                        &record,
                                        Some(&owner.public_key().to_hex()),
                                        clamp,
                                    )?
                                    .0
                                    .into_iter()
                                    .collect();
                                assert_eq!(
                                    env.get("BUZZ_ACP_RESPOND_TO").map(String::as_str),
                                    Some(expected_mode.as_str())
                                );
                                assert_eq!(
                                    env.contains_key("BUZZ_ACP_RESPOND_TO_ALLOWLIST"),
                                    expected_mode == RespondTo::Allowlist
                                );
                                assert_eq!(env.contains_key("BUZZ_ACP_ALLOWED_RESPOND_TO"), clamp);
                                let (provider_mode, allowlist) =
                                    projected_access_with_policy(&record, clamp);
                                assert_eq!(provider_mode, expected_mode);
                                assert_eq!(
                                    allowlist,
                                    if clamp {
                                        vec![]
                                    } else {
                                        i.respond_to_allowlist.clone()
                                    }
                                );
                                Ok(())
                            },
                        );
                        let allowed = sharing == Some(true) || !foreign;
                        assert_eq!(result.is_ok(), allowed, "sharing={sharing:?} foreign={foreign} readiness={readiness:?} mode={mode:?} clamp={clamp}");
                        assert_eq!(effects.get(), usize::from(allowed));
                        assert_eq!(device.rows()[1].respond_to, mode);
                        assert_eq!(
                            device.rows()[1].respond_to_allowlist,
                            i.respond_to_allowlist
                        );
                    }
                }
            }
        }
    }
}
