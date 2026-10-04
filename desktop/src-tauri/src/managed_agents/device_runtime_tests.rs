use super::{
    definition_home::EvidenceReadiness,
    device_home_migration::tests::{app, context, records, write},
    device_runtime::*,
};
use std::cell::Cell;
use tauri::Manager;

#[test]
fn shared_and_legacy_owning_adapters_need_no_host_proof() {
    for legacy in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<crate::app_state::AppState>();
        let (mut raw, _) = records();
        if legacy {
            raw[1].persona_id = None;
        } else {
            raw[0].share_across_devices = Some(true);
        }
        write(&super::managed_agents_base_dir(app.handle()).unwrap(), &raw);
        let effects = Cell::new(0);
        macro_rules! run {
            ($seam:ident) => {
                $seam(
                    app.handle(),
                    &state,
                    &raw[1].pubkey,
                    None,
                    |_, _| panic!("shared/legacy resolved host authority"),
                    |r, _, scope| {
                        assert_eq!(r.pubkey, raw[1].pubkey);
                        assert_eq!(scope, super::device_home_sync::capture_scope(&state)?);
                        effects.set(effects.get() + 1);
                        Ok(())
                    },
                )
                .unwrap();
            };
        }
        run!(spawn_child_phase_with);
        run!(start_pair_phase_with);
        run!(restore_spawn_phase_with);
        run!(provider_phase_with);
        run!(inbound_refresh_phase_with);
        assert_eq!(effects.get(), 5);
    }
}

#[test]
fn runtime_rechecks_scope_after_authority_before_effect() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    let (mut raw, _) = records();
    let mut c = context(EvidenceReadiness::Pending);
    c.scope = super::device_home_sync::capture_scope(&state).unwrap();
    raw[1].device_host_binding = Some(c.proof.binding().into());
    write(&super::managed_agents_base_dir(app.handle()).unwrap(), &raw);
    let effects = Cell::new(0);
    let result = runtime_phase_locked_with(
        app.handle(),
        &state,
        &raw[1].pubkey,
        None,
        |_, state| {
            state
                .workspace_apply_generation
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            Ok(c)
        },
        |_, _, _| {
            effects.set(1);
            Ok(())
        },
    );
    assert!(
        result.is_err(),
        "scope changed while authority was resolved"
    );
    assert_eq!(effects.get(), 0);
}

#[test]
fn runtime_rechecks_subscription_after_authority_before_effect() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    let (mut raw, _) = records();
    let mut c = context(EvidenceReadiness::Pending);
    c.scope = super::device_home_sync::capture_scope(&state).unwrap();
    raw[1].device_host_binding = Some(c.proof.binding().into());
    write(&super::managed_agents_base_dir(app.handle()).unwrap(), &raw);
    let effects = Cell::new(0);
    let result = runtime_phase_locked_with(
        app.handle(),
        &state,
        &raw[1].pubkey,
        None,
        |_, state| {
            super::device_home_sync::begin_session(state)?;
            Ok(c)
        },
        |_, _, _| {
            effects.set(1);
            Ok(())
        },
    );
    assert!(
        result.is_err(),
        "subscription changed while authority was resolved"
    );
    assert_eq!(effects.get(), 0);
}

#[test]
fn proven_exact_local_runtime_remains_allowed_while_hydration_is_pending() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    let (mut raw, _) = records();
    let mut c = context(EvidenceReadiness::Pending);
    c.scope = super::device_home_sync::capture_scope(&state).unwrap();
    raw[1].device_host_binding = Some(c.proof.binding().into());
    write(&super::managed_agents_base_dir(app.handle()).unwrap(), &raw);
    let effects = Cell::new(0);
    spawn_child_phase_with(
        app.handle(),
        &state,
        &raw[1].pubkey,
        None,
        |_, _| Ok(c),
        |_, _, _| {
            effects.set(1);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(effects.get(), 1);
}

#[test]
fn copied_record_never_reaches_spawn() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    let (mut raw, _) = records();
    raw[1].device_host_binding = Some("copied-marker".into());
    raw[1].auth_tag = Some("valid-auth-does-not-prove-host".into());
    write(&super::managed_agents_base_dir(app.handle()).unwrap(), &raw);
    let effects = Cell::new(0);
    let result = runtime_phase_locked_with(
        app.handle(),
        &state,
        &raw[1].pubkey,
        None,
        |_, state| {
            let mut c = context(EvidenceReadiness::Ready);
            c.scope = super::device_home_sync::capture_scope(state)?;
            Ok(c)
        },
        |_, _, _| {
            effects.set(effects.get() + 1);
            Ok(())
        },
    );
    assert!(result.is_err(), "copied runtime was authorized");
    assert_eq!(effects.get(), 0, "logs/process effects reached");
}

#[test]
fn start_refuses_before_terminating_existing_receipt() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    let (mut raw, _) = records();
    raw[1].device_host_binding = Some("foreign".into());
    write(&super::managed_agents_base_dir(app.handle()).unwrap(), &raw);
    let terminated = Cell::new(false);
    assert!(runtime_phase_locked_with(
        app.handle(),
        &state,
        &raw[1].pubkey,
        None,
        |_, state| {
            let mut c = context(EvidenceReadiness::Ready);
            c.scope = super::device_home_sync::capture_scope(state)?;
            Ok(c)
        },
        |_, _, _| {
            terminated.set(true);
            Ok(())
        }
    )
    .is_err());
    assert!(!terminated.get());
}

#[test]
fn provider_redeploy_obeys_home() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    let (mut raw, _) = records();
    raw[1].backend = super::BackendKind::Provider {
        id: "isolated".into(),
        config: serde_json::json!({}),
    };
    raw[1].device_host_binding = Some("foreign".into());
    write(&super::managed_agents_base_dir(app.handle()).unwrap(), &raw);
    let deployed = Cell::new(false);
    assert!(runtime_phase_locked_with(
        app.handle(),
        &state,
        &raw[1].pubkey,
        None,
        |_, _| Err("injected keychain inaccessible".into()),
        |_, _, _| {
            deployed.set(true);
            Ok(())
        }
    )
    .is_err());
    assert!(!deployed.get());
}

#[test]
fn shared_lifecycle_matches_baseline() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    let (mut raw, _) = records();
    raw[0].share_across_devices = Some(true);
    write(&super::managed_agents_base_dir(app.handle()).unwrap(), &raw);
    runtime_phase_locked_with(
        app.handle(),
        &state,
        &raw[1].pubkey,
        None,
        |_, _| panic!("shared requires no authority"),
        |_, _, _| Ok(()),
    )
    .unwrap();
    let expected = super::device_home_sync::capture_scope(&state).unwrap();
    state
        .workspace_apply_generation
        .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    assert!(
        runtime_phase_locked_with(
            app.handle(),
            &state,
            &raw[1].pubkey,
            Some(&expected),
            |_, _| panic!("shared requires no authority"),
            |_, _, _| Ok(())
        )
        .is_err(),
        "shared scope was redirected"
    );
}

macro_rules! owning_refusal {
    ($test:ident, $seam:ident) => {
        #[test]
        fn $test() {
            let dir = tempfile::tempdir().unwrap();
            let app = app(dir.path());
            let state = app.state::<crate::app_state::AppState>();
            let (mut raw, keys) = records();
            raw[1].device_host_binding = Some("copied-marker".into());
            raw[1].private_key_nsec = nostr::ToBech32::to_bech32(keys.secret_key()).unwrap();
            raw[1].auth_tag = Some("copied-auth".into());
            write(&super::managed_agents_base_dir(app.handle()).unwrap(), &raw);
            let before =
                std::fs::read(super::managed_agents_store_path(app.handle()).unwrap()).unwrap();
            let effects = Cell::new(0);
            let result = $seam(
                app.handle(),
                &state,
                &raw[1].pubkey,
                None,
                |_, state| {
                    let mut c = context(EvidenceReadiness::Ready);
                    c.scope = super::device_home_sync::capture_scope(state)?;
                    Ok(c)
                },
                |_, _, _| {
                    effects.set(effects.get() + 1);
                    Ok(())
                },
            );
            assert!(
                result.is_err(),
                "{} admitted copied record",
                stringify!($seam)
            );
            assert_eq!(effects.get(), 0, "{} reached effects", stringify!($seam));
            assert_eq!(
                before,
                std::fs::read(super::managed_agents_store_path(app.handle()).unwrap()).unwrap()
            );
        }
    };
}
owning_refusal!(
    owning_spawn_rejects_copied_key_and_auth,
    spawn_child_phase_with
);
owning_refusal!(
    owning_manual_start_refuses_receipt_effects,
    start_pair_phase_with
);
owning_refusal!(
    owning_restore_refuses_final_receipt_effects,
    restore_spawn_phase_with
);
owning_refusal!(owning_provider_refuses_deploy_effects, provider_phase_with);
#[test]
fn owning_inbound_refuses_restart_effects() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    let (mut raw, _) = records();
    raw[1].device_host_binding = Some("copied".into());
    write(&super::managed_agents_base_dir(app.handle()).unwrap(), &raw);
    let effects = Cell::new(0);
    let refresh = inbound_refresh_phase_with(
        app.handle(),
        &state,
        &raw[1].pubkey,
        None,
        |_, state| {
            let mut c = context(EvidenceReadiness::Pending);
            c.scope = super::device_home_sync::capture_scope(state)?;
            Ok(c)
        },
        |_, _, _| {
            effects.set(1);
            Ok(())
        },
    )
    .unwrap();
    assert!(refresh.is_none());
    assert_eq!(effects.get(), 0);
}

#[tokio::test]
async fn preflight_rechecks_current_binding_after_await() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    let (mut raw, _) = records();
    raw[0].share_across_devices = Some(true);
    let base = super::managed_agents_base_dir(app.handle()).unwrap();
    write(&base, &raw);
    let pubkey = raw[1].pubkey.clone();
    let effects = Cell::new(0);
    let result = runtime_preflight_with(
        app.handle(),
        &state,
        &pubkey,
        None,
        |_, state| {
            let mut c = context(EvidenceReadiness::Ready);
            c.scope = super::device_home_sync::capture_scope(state)?;
            Ok(c)
        },
        |_, _, _| async {
            raw[0].share_across_devices = Some(false);
            raw[1].device_host_binding = Some("foreign".into());
            write(&base, &raw);
            Ok(())
        },
        |_, _, _| {
            effects.set(1);
            Ok(())
        },
    )
    .await;
    assert!(result.is_err(), "preflight used stale shared policy");
    assert_eq!(effects.get(), 0);
}
#[tokio::test]
async fn create_postcommit_scope_is_pinned_before_preflight_and_after_await() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    let (mut raw, _) = records();
    raw[0].share_across_devices = Some(true);
    write(&super::managed_agents_base_dir(app.handle()).unwrap(), &raw);
    let captured = capture_runtime_fence(&state).unwrap();
    state
        .workspace_apply_generation
        .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    let probes = Cell::new(0);
    let effects = Cell::new(0);
    let result = runtime_preflight_with(
        app.handle(),
        &state,
        &raw[1].pubkey,
        Some(&captured),
        |_, _| panic!("shared reads no proof"),
        |_, _, _| async {
            probes.set(1);
            Ok(())
        },
        |_, _, _| {
            effects.set(1);
            Ok(())
        },
    )
    .await;
    assert!(result.is_err(), "stale creation reached preflight");
    assert_eq!((probes.get(), effects.get()), (0, 0));
}
#[tokio::test]
async fn shared_preflight_refuses_subscription_replacement_after_await() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    let (mut raw, _) = records();
    raw[0].share_across_devices = Some(true);
    write(&super::managed_agents_base_dir(app.handle()).unwrap(), &raw);
    let effects = Cell::new(0);
    let result = runtime_preflight_with(
        app.handle(),
        &state,
        &raw[1].pubkey,
        None,
        |_, _| panic!("shared reads no proof"),
        |_, _, _| async {
            super::device_home_sync::begin_session(&state)?;
            Ok(())
        },
        |_, _, _| {
            effects.set(1);
            Ok(())
        },
    )
    .await;
    assert!(result.is_err(), "subscription replacement reached spawn");
    assert_eq!(effects.get(), 0);
}

#[test]
fn target_foreign_sibling_never_uses_another_instances_local_proof() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    let (mut raw, _) = records();
    let mut c = context(EvidenceReadiness::Ready);
    c.scope = super::device_home_sync::capture_scope(&state).unwrap();
    let mut sibling = raw[1].clone();
    sibling.pubkey = nostr::Keys::generate().public_key().to_hex();
    sibling.device_host_binding = Some(c.proof.binding().into());
    raw[1].device_host_binding = Some("foreign".into());
    raw.push(sibling);
    write(&super::managed_agents_base_dir(app.handle()).unwrap(), &raw);
    assert!(runtime_phase_locked_with(
        app.handle(),
        &state,
        &raw[1].pubkey,
        None,
        |_, _| Ok(c),
        |_, _, _| -> Result<(), String> { panic!("foreign sibling reached runtime") }
    )
    .is_err());
}
#[test]
fn own_origin_without_exact_binding_never_reaches_runtime() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    let (mut raw, _) = records();
    let mut c = context(EvidenceReadiness::Ready);
    c.scope = super::device_home_sync::capture_scope(&state).unwrap();
    raw[0].origin_device_id = Some(c.device.device_id.clone());
    raw[1].device_host_binding = None;
    write(&super::managed_agents_base_dir(app.handle()).unwrap(), &raw);
    assert!(runtime_phase_locked_with(
        app.handle(),
        &state,
        &raw[1].pubkey,
        None,
        |_, _| Ok(c),
        |_, _, _| -> Result<(), String> { panic!("own JSON origin authorized an unbound key") }
    )
    .is_err());
}
#[test]
fn runtime_missing_definition_and_malformed_store_never_resolve_authority() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    let (mut raw, _) = records();
    raw.remove(0);
    raw[0].share_across_devices = Some(true);
    let base = super::managed_agents_base_dir(app.handle()).unwrap();
    write(&base, &raw);
    assert!(runtime_phase_locked_with(
        app.handle(),
        &state,
        &raw[0].pubkey,
        None,
        |_, _| panic!("missing definition read proof"),
        |_, _, _| -> Result<(), String> { panic!("missing definition authorized") }
    )
    .is_err());
    std::fs::write(base.join("managed-agents.json"), b"broken").unwrap();
    assert!(runtime_phase_locked_with(
        app.handle(),
        &state,
        &raw[0].pubkey,
        None,
        |_, _| panic!("malformed store read proof"),
        |_, _, _| -> Result<(), String> { panic!("malformed store authorized") }
    )
    .is_err());
}
#[tokio::test]
async fn private_preflight_refuses_before_any_probe() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    let (mut raw, _) = records();
    raw[1].device_host_binding = Some("copied".into());
    write(&super::managed_agents_base_dir(app.handle()).unwrap(), &raw);
    let probes = Cell::new(0);
    assert!(runtime_preflight_with(
        app.handle(),
        &state,
        &raw[1].pubkey,
        None,
        |_, state| {
            let mut c = context(EvidenceReadiness::Ready);
            c.scope = super::device_home_sync::capture_scope(state)?;
            Ok(c)
        },
        |_, _, _| async {
            probes.set(1);
            Err("probe reached".into())
        },
        |_, _, _| Ok(())
    )
    .await
    .is_err());
    assert_eq!(probes.get(), 0);
}
