use super::super::{
    definition_home::EvidenceReadiness,
    device_home_migration::tests::{context, definition, records},
};
use super::*;
use std::cell::Cell;
#[test]
fn remote_create_has_no_mint_publish_or_save() {
    for readiness in [
        EvidenceReadiness::Ready,
        EvidenceReadiness::Pending,
        EvidenceReadiness::Failed,
    ] {
        let c = context(readiness);
        let mut d = definition();
        if readiness == EvidenceReadiness::Ready {
            d.origin_device_id = Some("other".into());
            d.origin_device_label = Some("Remote host".into());
        }
        let scope = c.scope.clone();
        let (mint, save, publish) = (Cell::new(0), Cell::new(0), Cell::new(0));
        let result = creation_phase(
            &scope,
            None,
            Some(&d),
            &[],
            || Ok(c),
            |_| {
                mint.set(1);
                save.set(1);
                publish.set(1);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!((mint.get(), save.get(), publish.get()), (0, 0, 0));
    }
}
#[test]
fn create_rechecks_home_after_await() {
    let c = context(EvidenceReadiness::Ready);
    let scope = c.scope.clone();
    let mut d = definition();
    creation_phase(&scope, None, Some(&d), &[], || Ok(c), |_| Ok(())).unwrap();
    d.origin_device_id = Some("new home".into());
    let saved = Cell::new(false);
    assert!(creation_phase(
        &scope,
        Some(&scope),
        Some(&d),
        &[],
        || Ok(context(EvidenceReadiness::Ready)),
        |_| {
            saved.set(true);
            Ok(())
        }
    )
    .is_err());
    assert!(!saved.get());
    assert!(creation_phase(
        &scope,
        Some(&scope),
        Some(&definition()),
        &[],
        || Ok(context(EvidenceReadiness::Pending)),
        |_| {
            saved.set(true);
            Ok(())
        }
    )
    .is_err());
    assert!(!saved.get());
}
#[test]
fn create_scope_is_pinned() {
    let c = context(EvidenceReadiness::Ready);
    let captured = c.scope.clone();
    for scope in [
        SyncScope {
            relay_url: "wss://other".into(),
            ..captured.clone()
        },
        SyncScope {
            owner_pubkey: "other".into(),
            ..captured.clone()
        },
        SyncScope {
            workspace_generation: 1,
            ..captured.clone()
        },
    ] {
        let calls = Cell::new(0);
        assert!(creation_phase(
            &scope,
            Some(&captured),
            None,
            &[],
            || panic!("proof"),
            |_| {
                calls.set(1);
                Ok(())
            }
        )
        .is_err());
        assert_eq!(calls.get(), 0);
    }
}
#[test]
fn shared_creation_never_reads_host_authority() {
    let mut d = definition();
    d.share_across_devices = Some(true);
    let c = context(EvidenceReadiness::Pending);
    assert!(creation_phase(
        &c.scope,
        None,
        Some(&d),
        &[],
        || panic!("shared proof read"),
        |_| Ok(())
    )
    .is_ok());
}
#[test]
fn all_new_sources_default_private() {
    let c = context(EvidenceReadiness::Ready);
    for request in [None, Some(false), Some(true)] {
        let mut d = definition();
        d.origin_device_id = Some("foreign".into());
        d.origin_released = Some(true);
        stamp_new_definition(&mut d, request, &c.device);
        assert_eq!(d.share_across_devices, Some(request.unwrap_or(false)));
        assert_eq!(
            d.origin_device_id.as_deref(),
            Some(c.device.device_id.as_str())
        );
        assert_eq!(
            d.origin_device_label.as_deref(),
            Some(c.device.label.as_str())
        );
        assert_eq!(d.origin_released, Some(false));
    }
}
#[test]
fn sharing_does_not_change_respond_to() {
    use super::super::RespondTo;
    let c = context(EvidenceReadiness::Ready);
    for shared in [false, true] {
        for mode in [
            RespondTo::OwnerOnly,
            RespondTo::Allowlist,
            RespondTo::Anyone,
        ] {
            let mut d = definition();
            d.respond_to = Some(mode.as_str().into());
            d.respond_to_allowlist = vec!["pubkey".into()];
            d.parallelism = Some(3);
            let original = d.clone();
            stamp_new_definition(&mut d, Some(shared), &c.device);
            assert_eq!(
                (d.respond_to, d.respond_to_allowlist, d.parallelism),
                (
                    original.respond_to,
                    original.respond_to_allowlist,
                    original.parallelism
                )
            );
            let (mut rs, _) = records();
            let i = &mut rs[1];
            i.respond_to = mode;
            i.respond_to_allowlist = vec!["pubkey".into()];
            bind_new_instance(i, &c.proof);
            assert_eq!(
                (i.respond_to, i.respond_to_allowlist.clone()),
                (mode, vec!["pubkey".to_string()])
            );
            assert_eq!(i.device_host_binding.as_deref(), Some(c.proof.binding()));
        }
    }
}
#[test]
fn builtin_materialization_defaults_private_on_current_device() {
    use tauri::Manager;
    let dir = tempfile::tempdir().unwrap();
    let app = super::super::device_home_migration::tests::app(dir.path());
    let device = local_device(app.handle()).unwrap();
    let definitions = super::super::load_personas(app.handle()).unwrap();
    assert!(!definitions.is_empty());
    for d in definitions {
        assert_eq!(d.share_across_devices, Some(false));
        assert_eq!(
            d.origin_device_id.as_deref(),
            Some(device.device_id.as_str())
        );
        assert_eq!(d.origin_released, Some(false));
    }
    assert_eq!(app.path().app_data_dir().unwrap(), dir.path());
}
#[test]
fn native_creation_adapter_refuses_before_all_effects_and_preserves_structural_bytes() {
    use tauri::Manager;
    let dir = tempfile::tempdir().unwrap();
    let app = super::super::device_home_migration::tests::app(dir.path());
    let (mut rs, _) = records();
    rs[0].origin_device_id = Some("foreign".into());
    rs[0].origin_device_label = Some("A".into());
    rs[1].private_key_nsec = "protected-inline-key".into();
    super::super::device_home_migration::tests::write(&dir.path().join("agents"), &rs);
    let path = dir.path().join("agents/managed-agents.json");
    let before = std::fs::read(&path).unwrap();
    let state = app.state::<crate::app_state::AppState>();
    let scope = super::super::device_home_sync::capture_scope(&state).unwrap();
    let mut c = context(EvidenceReadiness::Ready);
    c.scope = scope.clone();
    let effects = Cell::new(0);
    let _guard = state.managed_agents_store_lock.lock().unwrap();
    let result = creation_phase_locked(
        app.handle(),
        &state,
        Some("one"),
        Some(&scope),
        |_, _| Ok(c),
        |_| {
            effects.set(1);
            Ok(())
        },
    );
    assert!(result
        .unwrap_err()
        .contains("definition_hosted_elsewhere: A"));
    assert_eq!(effects.get(), 0);
    assert_eq!(std::fs::read(path).unwrap(), before);
}
#[tokio::test]
async fn native_creation_adapter_rechecks_real_readiness_and_scope_after_await() {
    use super::super::device_home_sync::{
        begin_session, capture_scope, finish_session, hydrate_history, readiness_locked,
    };
    use tauri::Manager;
    for change in ["readiness", "owner", "relay", "home", "failed"] {
        let dir = tempfile::tempdir().unwrap();
        let app = super::super::device_home_migration::tests::app(dir.path());
        let state = app.state::<crate::app_state::AppState>();
        let session = begin_session(&state).unwrap();
        hydrate_history(
            &state,
            &session.token,
            |_| async { Ok(vec![]) },
            |_| async { Ok(()) },
        )
        .await
        .unwrap();
        finish_session(&state, &session.token).unwrap();
        super::super::device_home_migration::tests::write(
            &dir.path().join("agents"),
            &[definition().into_agent_record()],
        );
        let captured = capture_scope(&state).unwrap();
        let provider = |_: &tauri::AppHandle<tauri::test::MockRuntime>,
                        state: &crate::app_state::AppState| {
            let mut c = context(EvidenceReadiness::Ready);
            c.scope = capture_scope(state)?;
            c.readiness = readiness_locked(state, &c.scope)?;
            Ok(c)
        };
        {
            let _guard = state.managed_agents_store_lock.lock().unwrap();
            creation_phase_locked(
                app.handle(),
                &state,
                Some("one"),
                Some(&captured),
                provider,
                |_| Ok(()),
            )
            .unwrap();
        }
        tokio::task::yield_now().await;
        match change {
            "owner" => *state.keys.lock().unwrap() = nostr::Keys::generate(),
            "relay" => *state.relay_url_override.lock().unwrap() = Some("wss://changed".into()),
            "home" => {
                let mut d = definition();
                d.origin_device_id = Some("other home".into());
                d.origin_device_label = Some("Other machine".into());
                super::super::device_home_migration::tests::write(
                    &dir.path().join("agents"),
                    &[d.into_agent_record()],
                );
            }
            "failed" => {
                let replacement = begin_session(&state).unwrap();
                assert!(hydrate_history(
                    &state,
                    &replacement.token,
                    |_| async { Err("relay offline".into()) },
                    |_| async { Ok(()) }
                )
                .await
                .is_err());
            }
            _ => {
                begin_session(&state).unwrap();
            }
        }
        let effects = Cell::new(0);
        let _guard = state.managed_agents_store_lock.lock().unwrap();
        let result = creation_phase_locked(
            app.handle(),
            &state,
            Some("one"),
            Some(&captured),
            provider,
            |_| {
                effects.set(1);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(effects.get(), 0);
    }
}
#[test]
fn native_shared_adapter_bypasses_unavailable_proof_even_with_foreign_private_rows() {
    use tauri::Manager;
    let dir = tempfile::tempdir().unwrap();
    let app = super::super::device_home_migration::tests::app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    let (mut raw, _) = records();
    let mut shared = definition();
    shared.id = "shared".into();
    shared.share_across_devices = Some(true);
    raw.push(shared.into_agent_record());
    raw[1].device_host_binding = Some("foreign".into());
    raw[1].private_key_nsec = "protected".into();
    super::super::device_home_migration::tests::write(&dir.path().join("agents"), &raw);
    let before = std::fs::read(dir.path().join("agents/managed-agents.json")).unwrap();
    let _guard = state.managed_agents_store_lock.lock().unwrap();
    creation_phase_locked(
        app.handle(),
        &state,
        Some("shared"),
        None,
        |_, _| panic!("shared proof read"),
        |c| {
            assert!(c.is_none());
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(
        std::fs::read(dir.path().join("agents/managed-agents.json")).unwrap(),
        before
    );
}
#[test]
fn changed_authority_scope_and_corrupt_structural_store_fail_before_effects() {
    use tauri::Manager;
    let mut c = context(EvidenceReadiness::Ready);
    let captured = c.scope.clone();
    c.scope.relay_url = "wss://changed".into();
    let effects = Cell::new(0);
    assert!(creation_phase(
        &captured,
        None,
        Some(&definition()),
        &[],
        || Ok(c),
        |_| {
            effects.set(1);
            Ok(())
        }
    )
    .is_err());
    assert_eq!(effects.get(), 0);
    let dir = tempfile::tempdir().unwrap();
    let app = super::super::device_home_migration::tests::app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    let path = dir.path().join("agents/managed-agents.json");
    std::fs::write(&path, b"broken").unwrap();
    let _guard = state.managed_agents_store_lock.lock().unwrap();
    let error = creation_phase_locked(
        app.handle(),
        &state,
        None,
        None,
        |_, _| panic!("corrupt store proof read"),
        |_| {
            effects.set(1);
            Ok(())
        },
    )
    .unwrap_err();
    assert!(error.contains("device policy agent store:"));
    assert_eq!(effects.get(), 0);
    assert_eq!(std::fs::read(path).unwrap(), b"broken");
}
