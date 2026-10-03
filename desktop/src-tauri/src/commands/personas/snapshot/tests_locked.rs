//! Locked-card import tests for `decode_snapshot_for_import`.
//!
//! Kept in a sibling file so `snapshot/tests.rs` stays under the
//! 1500-line gate; `#[path]`-included from there as a child module,
//! so `super::*` still resolves to the shared test helpers.

use super::*;
use crate::commands::personas::snapshot::import::{
    decode_snapshot_for_import, parse_snapshot_payload_from_bytes,
};
use crate::managed_agents::agent_snapshot_envelope::{
    encode_locked_snapshot_png, encrypt_snapshot_envelope, ChunkPayload, LOCKED_CARD_REFUSAL,
};

/// Build a keyed instance record holding real key material, so the
/// agent-endpoint unlock path resolves exactly as production does.
fn record_for(agent: &nostr::Keys) -> ManagedAgentRecord {
    ManagedAgentRecord {
        session_policy: Default::default(),
        pubkey: agent.public_key().to_hex(),
        slug: None,
        persona_id: Some("locked-test".to_string()),
        private_key_nsec: nostr::ToBech32::to_bech32(agent.secret_key()).unwrap(),
        ..make_definition("")
    }
}

fn locked_png(owner: &nostr::Keys, agent: &nostr::Keys) -> (AgentSnapshot, Vec<u8>) {
    let snapshot = make_snapshot(MemoryLevel::None, vec![]);
    let png = encode_locked_snapshot_png(&snapshot, owner, &agent.public_key(), None).unwrap();
    (snapshot, png)
}

/// Owner identity key unlocks a locked card; `locked` is reported true.
#[test]
fn owner_endpoint_unlocks_locked_png() {
    let (owner, agent) = (nostr::Keys::generate(), nostr::Keys::generate());
    let (snapshot, png) = locked_png(&owner, &agent);
    let (decoded, locked) = decode_snapshot_for_import(&png, Some(&owner), &[]).unwrap();
    assert_eq!(decoded, snapshot);
    assert!(locked);
}

/// A local managed-agent record holding the agent nsec unlocks the card
/// even when the owner identity does not match (e.g. re-import on the
/// agent's own machine under a different owner identity).
#[test]
fn agent_record_endpoint_unlocks_locked_png() {
    let (owner, agent) = (nostr::Keys::generate(), nostr::Keys::generate());
    let (snapshot, png) = locked_png(&owner, &agent);
    let other_identity = nostr::Keys::generate();
    let records = vec![record_for(&agent)];
    let (decoded, locked) =
        decode_snapshot_for_import(&png, Some(&other_identity), &records).unwrap();
    assert_eq!(decoded, snapshot);
    assert!(locked);
}

/// No matching endpoint → only the locked-card refusal, nothing else.
#[test]
fn stranger_fails_closed_with_refusal_only() {
    let (owner, agent) = (nostr::Keys::generate(), nostr::Keys::generate());
    let (_snapshot, png) = locked_png(&owner, &agent);
    let stranger = nostr::Keys::generate();
    let unrelated_record = record_for(&nostr::Keys::generate());
    let err = decode_snapshot_for_import(&png, Some(&stranger), &[unrelated_record]).unwrap_err();
    assert_eq!(err, LOCKED_CARD_REFUSAL);
    // And with no key material at all.
    let err = decode_snapshot_for_import(&png, None, &[]).unwrap_err();
    assert_eq!(err, LOCKED_CARD_REFUSAL);
}

/// Plain snapshots pass through unchanged with `locked == false`, with or
/// without key material in scope.
#[test]
fn plain_snapshot_passes_through_unlocked() {
    use crate::managed_agents::agent_snapshot::encode_snapshot_png;
    let snapshot = make_snapshot(MemoryLevel::None, vec![]);
    let png = encode_snapshot_png(&snapshot, None).unwrap();
    let owner = nostr::Keys::generate();
    let (decoded, locked) = decode_snapshot_for_import(&png, Some(&owner), &[]).unwrap();
    assert_eq!(decoded, snapshot);
    assert!(!locked);
    let (decoded, locked) = decode_snapshot_for_import(&png, None, &[]).unwrap();
    assert_eq!(decoded, snapshot);
    assert!(!locked);
}

/// The memory-consistency guard fires AFTER decryption too: a locked
/// envelope whose plaintext declares level none + non-empty entries is
/// rejected even for a legitimate endpoint.
#[test]
fn decrypted_manifest_memory_consistency_enforced() {
    let (owner, agent) = (nostr::Keys::generate(), nostr::Keys::generate());
    let malformed = make_snapshot(
        MemoryLevel::None,
        vec![AgentSnapshotMemoryEntry {
            slug: "core".to_string(),
            body: "leaked".to_string(),
        }],
    );
    // encrypt_snapshot_envelope does not guard memory consistency (the
    // PNG encoder does), so this constructs the malicious payload.
    let envelope = encrypt_snapshot_envelope(&malformed, &owner, &agent.public_key()).unwrap();
    let json = serde_json::to_vec(&envelope).unwrap();
    let err = decode_snapshot_for_import(&json, Some(&owner), &[]).unwrap_err();
    assert!(
        err.contains("'none' but entries are present"),
        "post-decrypt consistency guard must fire, got: {err}"
    );
}

/// Transit validation (`fetch_snapshot_bytes` path) accepts a locked PNG
/// without any key material — structural validation only, no decryption.
#[test]
fn transit_validation_accepts_locked_png_without_keys() {
    let (owner, agent) = (nostr::Keys::generate(), nostr::Keys::generate());
    let (_snapshot, png) = locked_png(&owner, &agent);
    let payload = parse_snapshot_payload_from_bytes(&png).unwrap();
    assert!(matches!(payload, ChunkPayload::Locked(_)));
}

/// The keyless plain decoder refuses locked cards with the refusal.
#[test]
fn plain_decoder_refuses_locked_cards() {
    let (owner, agent) = (nostr::Keys::generate(), nostr::Keys::generate());
    let (_snapshot, png) = locked_png(&owner, &agent);
    let err = decode_snapshot_from_bytes(&png).unwrap_err();
    assert_eq!(err, LOCKED_CARD_REFUSAL);
}

struct UnlockSecrets {
    bytes: Result<Option<Vec<u8>>, String>,
    reads: std::sync::atomic::AtomicUsize,
}
impl crate::secret_store::TestBlobBackend for UnlockSecrets {
    fn read(&self) -> Result<Option<Vec<u8>>, String> {
        self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.bytes.clone()
    }
    fn write(&self, _: &[u8]) -> Result<(), String> {
        panic!("locked import must not write secrets")
    }
}

fn owning_unlock(mode: &str) {
    use crate::managed_agents::{
        definition_home::EvidenceReadiness, device_home_migration::tests::context,
        storage::resolve_agent_key_readonly_with,
    };
    use std::{cell::RefCell, sync::Arc};
    let (owner, agent, active_owner) = (
        nostr::Keys::generate(),
        nostr::Keys::generate(),
        nostr::Keys::generate(),
    );
    assert_ne!(owner.public_key(), active_owner.public_key());
    let (snapshot, png) = locked_png(&owner, &agent);
    let context = context(EvidenceReadiness::Ready);
    let mut definition = make_definition("locked-test");
    definition.share_across_devices = Some(mode == "shared");
    let mut endpoint = record_for(&agent);
    endpoint.private_key_nsec.clear();
    endpoint.device_host_binding = match mode {
        "local" => Some(context.proof.binding().into()),
        "foreign" => Some("foreign-binding".into()),
        _ => None,
    };
    if mode == "legacy" {
        endpoint.persona_id = None;
    }
    let mut unrelated = record_for(&nostr::Keys::generate());
    unrelated.private_key_nsec.clear();
    unrelated.device_host_binding = Some(context.proof.binding().into());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("managed-agents.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&vec![definition, endpoint, unrelated]).unwrap(),
    )
    .unwrap();
    let before = std::fs::read(&path).unwrap();
    let backend = Arc::new(UnlockSecrets {
        bytes: Ok(Some(
            serde_json::to_vec(&serde_json::json!({
                format!("agent:{}", agent.public_key().to_hex()): agent.secret_key().to_secret_hex()
            }))
            .unwrap(),
        )),
        reads: Default::default(),
    });
    let mut secrets =
        crate::secret_store::SecretStore::keyring(format!("task5-fix1-{}", uuid::Uuid::new_v4()));
    secrets.test_backend = Some(backend.clone());
    let lookups = RefCell::new(Vec::new());
    let result = crate::commands::personas::snapshot::import::decode_snapshot_for_import_readonly(
        &png,
        Some(&active_owner),
        &path,
        &context.proof,
        |r| {
            lookups.borrow_mut().push(r.pubkey.clone());
            resolve_agent_key_readonly_with(r, Some(&secrets))
        },
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    if mode == "foreign" {
        assert_eq!(result.unwrap_err(), LOCKED_CARD_REFUSAL);
        assert!(lookups.borrow().is_empty());
        assert_eq!(backend.reads.load(std::sync::atomic::Ordering::SeqCst), 0);
    } else {
        let (decoded, locked) = result.unwrap();
        assert_eq!(decoded, snapshot);
        assert!(locked);
        assert_eq!(*lookups.borrow(), vec![agent.public_key().to_hex()]);
        assert_eq!(backend.reads.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}

#[test]
fn confirm_readonly_shared_unbound_endpoint_unlocks_under_another_owner() {
    owning_unlock("shared");
}
#[test]
fn confirm_readonly_definitionless_legacy_endpoint_unlocks() {
    owning_unlock("legacy");
}
#[test]
fn confirm_readonly_foreign_private_endpoint_never_reads_keys() {
    owning_unlock("foreign");
}
#[test]
fn confirm_readonly_proven_local_endpoint_reads_only_exact_recipient() {
    owning_unlock("local");
}

#[test]
fn confirm_readonly_owner_endpoint_skips_all_agent_secret_lookups() {
    let (owner, agent) = (nostr::Keys::generate(), nostr::Keys::generate());
    let (snapshot, png) = locked_png(&owner, &agent);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("managed-agents.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&vec![record_for(&agent)]).unwrap(),
    )
    .unwrap();
    let before = std::fs::read(&path).unwrap();
    let context = crate::managed_agents::device_home_migration::tests::context(
        crate::managed_agents::definition_home::EvidenceReadiness::Ready,
    );
    let (decoded, locked) =
        crate::commands::personas::snapshot::import::decode_snapshot_for_import_readonly(
            &png,
            Some(&owner),
            &path,
            &context.proof,
            |_| panic!("owner unlock must not read agent keys"),
        )
        .unwrap();
    assert_eq!(decoded, snapshot);
    assert!(locked);
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn confirm_readonly_structural_and_key_errors_fail_closed() {
    let (owner, agent) = (nostr::Keys::generate(), nostr::Keys::generate());
    let (_, png) = locked_png(&owner, &agent);
    let context = crate::managed_agents::device_home_migration::tests::context(
        crate::managed_agents::definition_home::EvidenceReadiness::Ready,
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("managed-agents.json");
    for directory in [false, true] {
        if directory {
            std::fs::create_dir(&path).unwrap();
        } else {
            std::fs::write(&path, b"broken").unwrap();
        }
        assert!(
            crate::commands::personas::snapshot::import::decode_snapshot_for_import_readonly(
                &png,
                Some(&owner),
                &path,
                &context.proof,
                |_| panic!("bad store must not read secrets"),
            )
            .is_err()
        );
        if directory {
            std::fs::remove_dir(&path).unwrap();
        } else {
            assert_eq!(std::fs::read(&path).unwrap(), b"broken");
            std::fs::remove_file(&path).unwrap();
        }
    }
    let mut record = record_for(&agent);
    record.private_key_nsec.clear();
    let mut definition = make_definition("locked-test");
    definition.share_across_devices = Some(true);
    std::fs::write(
        &path,
        serde_json::to_vec(&vec![definition, record.clone()]).unwrap(),
    )
    .unwrap();
    let before = std::fs::read(&path).unwrap();
    for key in [
        Err("vault unavailable".into()),
        Ok(None),
        Ok(Some(nostr::Keys::generate())),
    ] {
        assert!(
            crate::commands::personas::snapshot::import::decode_snapshot_for_import_readonly(
                &png,
                None,
                &path,
                &context.proof,
                |_| key,
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    // An instance's own sharing bit cannot authorize a missing linked definition.
    record.share_across_devices = Some(true);
    record.device_host_binding = Some(context.proof.binding().into());
    std::fs::write(&path, serde_json::to_vec(&vec![record]).unwrap()).unwrap();
    assert!(
        crate::commands::personas::snapshot::import::decode_snapshot_for_import_readonly(
            &png,
            None,
            &path,
            &context.proof,
            |_| panic!("missing definition must not authorize a key read"),
        )
        .is_err()
    );
}

#[test]
fn confirm_readonly_plain_snapshot_skips_all_agent_secret_lookups() {
    let snapshot = make_snapshot(MemoryLevel::None, vec![]);
    let png = crate::managed_agents::agent_snapshot::encode_snapshot_png(&snapshot, None).unwrap();
    let context = crate::managed_agents::device_home_migration::tests::context(
        crate::managed_agents::definition_home::EvidenceReadiness::Ready,
    );
    let mut record = record_for(&nostr::Keys::generate());
    record.device_host_binding = Some(context.proof.binding().into());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("managed-agents.json");
    std::fs::write(&path, serde_json::to_vec(&vec![record]).unwrap()).unwrap();
    let before = std::fs::read(&path).unwrap();
    let (decoded, locked) =
        crate::commands::personas::snapshot::import::decode_snapshot_for_import_readonly(
            &png,
            None,
            &path,
            &context.proof,
            |_| panic!("plain import must not read secrets"),
        )
        .unwrap();
    assert_eq!(decoded, snapshot);
    assert!(!locked);
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
