use super::device_authority::*;
use super::{
    definition_home::EvidenceReadiness,
    device_home_migration::tests::{app, context, records, write},
    device_home_sync,
};
use std::cell::Cell;
use tauri::Manager;

#[test]
fn direct_update_or_delete_copied_instance_has_no_side_effects() {
    for action in [
        InstanceAuthorityAction::Update,
        InstanceAuthorityAction::Delete,
        InstanceAuthorityAction::PublishProfile,
        InstanceAuthorityAction::PublishHead,
        InstanceAuthorityAction::Tombstone,
        InstanceAuthorityAction::Archive,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<crate::app_state::AppState>();
        let (mut raw, _) = records();
        raw[1].device_host_binding = Some("foreign".into());
        raw[1].private_key_nsec = "copied-inline-secret".into();
        write(&super::managed_agents_base_dir(app.handle()).unwrap(), &raw);
        let path = super::managed_agents_store_path(app.handle()).unwrap();
        let before = std::fs::read(&path).unwrap();
        let effects = Cell::new(0);
        let result = instance_phase_locked_with(
            app.handle(),
            &state,
            &raw[1].pubkey,
            None,
            action,
            |_, state| {
                let mut c = context(EvidenceReadiness::Ready);
                c.scope = device_home_sync::capture_scope(state)?;
                Ok(c)
            },
            |_, _, _| {
                effects.set(effects.get() + 1);
                Ok(())
            },
        );
        assert!(result.is_err(), "copied instance authorized for {action:?}");
        assert_eq!(effects.get(), 0, "key/store/signing/journal effect reached");
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}
#[test]
fn remote_delete_has_no_cascade() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    let (mut raw, _) = records();
    raw[0].origin_device_id = Some("foreign".into());
    raw[0].origin_device_label = Some("Home A".into());
    raw[1].device_host_binding = Some("foreign".into());
    write(&super::managed_agents_base_dir(app.handle()).unwrap(), &raw);
    let effects = Cell::new(0);
    let result = definition_delete_phase_locked_with(
        app.handle(),
        &state,
        "one",
        |_, state| {
            let mut c = context(EvidenceReadiness::Ready);
            c.scope = device_home_sync::capture_scope(state)?;
            Ok(c)
        },
        || {
            effects.set(1);
            Ok(())
        },
    );
    assert!(result.unwrap_err().contains("definition_hosted_elsewhere"));
    assert_eq!(effects.get(), 0);
}
#[test]
fn proven_sibling_cannot_authorize_copied_target() {
    let (mut raw, _) = records();
    let c = context(EvidenceReadiness::Ready);
    raw[1].device_host_binding = Some("foreign".into());
    let mut sibling = raw[1].clone();
    sibling.device_host_binding = Some(c.proof.binding().into());
    sibling.pubkey = "sibling".into();
    raw.push(sibling);
    assert!(authorize_instance_authority(
        &raw[1],
        raw[0].to_definition_view().as_ref(),
        &c,
        InstanceAuthorityAction::Update
    )
    .is_err());
}
#[test]
fn explicit_sharing_never_reads_private_authority() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    let (mut raw, _) = records();
    raw[0].share_across_devices = Some(true);
    raw[1].device_host_binding = Some("foreign".into());
    write(&super::managed_agents_base_dir(app.handle()).unwrap(), &raw);
    instance_phase_locked_with(
        app.handle(),
        &state,
        &raw[1].pubkey,
        None,
        InstanceAuthorityAction::PublishHead,
        |_, _| panic!("shared target must not read proof"),
        |_, _, _| Ok(()),
    )
    .unwrap();
}

#[tokio::test]
async fn original_profile_and_memory_scope_refuses_switched_owner_relay_and_subscription() {
    for change in ["owner-relay", "subscription"] {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<crate::app_state::AppState>();
        let (mut raw, _) = records();
        raw[0].share_across_devices = Some(true);
        write(&super::managed_agents_base_dir(app.handle()).unwrap(), &raw);
        let fence = super::device_runtime::capture_runtime_fence(&state).unwrap();
        if change == "owner-relay" {
            *state.keys.lock().unwrap() = nostr::Keys::generate();
            *state.relay_url_override.lock().unwrap() = Some("wss://other".into());
        } else {
            device_home_sync::begin_session(&state).unwrap();
        }
        for action in [
            InstanceAuthorityAction::PublishProfile,
            InstanceAuthorityAction::PublishHead,
        ] {
            let effects = Cell::new(0);
            let result = original_publication_with(
                app.handle(),
                &state,
                &raw[1].pubkey,
                &fence,
                action,
                |_, _| panic!("shared proof read"),
                |scope| {
                    let effects = &effects;
                    let expected = &fence.scope;
                    async move {
                        effects.set(1);
                        assert_eq!(&scope, expected);
                        Ok(())
                    }
                },
            )
            .await;
            assert!(result.is_err());
            assert_eq!(
                effects.get(),
                0,
                "original postcommit operation reached switched workspace"
            );
        }
    }
}

#[test]
fn mixed_boot_retention_keeps_explicit_sharing_proof_free_after_private_authority_failure() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    let (mut raw, _) = records();
    raw[1].device_host_binding = Some("copied".into());
    let mut shared = raw[0].clone();
    shared.slug = Some("shared".into());
    shared.share_across_devices = Some(true);
    let mut shared_instance = raw[1].clone();
    shared_instance.pubkey = nostr::Keys::generate().public_key().to_hex();
    shared_instance.persona_id = Some("shared".into());
    raw.push(shared);
    raw.push(shared_instance.clone());
    let base = super::managed_agents_base_dir(app.handle()).unwrap();
    write(&base, &raw);
    let keys = state.signing_keys().unwrap();
    let path = super::retention::scoped_retention_db_path(
        &base,
        "wss://test",
        &keys.public_key().to_hex(),
    );
    let result =
        super::reconcile::reconcile_agents_to_events_with(app.handle(), &keys, &path, |_, _| {
            Err("marker unavailable".into())
        });
    assert!(result.is_err());
    let pending =
        super::retention::get_pending_sync(&super::retention::open_retention_db(&path).unwrap())
            .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].kind, 30177);
    assert_eq!(pending[0].d_tag, shared_instance.pubkey);
}

#[test]
fn allowed_shared_deletion_housekeeping_preserves_copied_siblings_and_canonical_definition() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let (mut raw, _) = records();
    raw[0].share_across_devices = Some(true);
    let mut copied = raw[1].clone();
    copied.pubkey = nostr::Keys::generate().public_key().to_hex();
    copied.device_host_binding = Some("foreign".into());
    copied.private_key_nsec = "copied secret bytes".into();
    copied.runtime_pid = Some(999);
    raw.push(copied.clone());
    let canonical = raw[0].clone();
    let target = raw[1].pubkey.clone();
    selected_deletion_records_with(
        &mut raw,
        &[target.clone()].into_iter().collect(),
        |selected| {
            assert_eq!(selected.len(), 1);
            assert_eq!(selected[0].pubkey, target);
            selected[0].last_stopped_at = Some("stopped".into());
            Ok(())
        },
    )
    .unwrap();
    raw.retain(|r| r.pubkey != target);
    save_deletion_snapshot(app.handle(), &raw).unwrap();
    let saved = super::persona_device_view::read_policy_records(
        &super::managed_agents_store_path(app.handle()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&saved[0]).unwrap(),
        serde_json::to_value(canonical).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&saved[1]).unwrap(),
        serde_json::to_value(copied).unwrap()
    );
}
