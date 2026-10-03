use super::*;
use crate::managed_agents::{
    definition_home::EvidenceReadiness,
    device_home_migration::{
        migrate_device_homes_before_sync_with, migrate_device_homes_locked_with,
        tests::{app, context, records, write},
    },
    device_home_sync,
    persona_device_view::{read_policy_records, read_remote_evidence},
    retention::active_retention_scope,
};

#[tokio::test]
async fn fresh_workspace_preparation_initializes_scope_without_authorizing_absence() {
    for proven in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<AppState>();
        let base = crate::managed_agents::managed_agents_base_dir(app.handle()).unwrap();
        let scope = active_retention_scope(app.handle(), &state).unwrap();
        std::fs::remove_file(&scope.db_path).unwrap();
        assert!(!base.join("retention.db").exists());
        assert!(!scope.db_path.exists());
        let (mut rs, key) = records();
        let mut c = context(EvidenceReadiness::Pending);
        c.scope = device_home_sync::capture_scope(&state).unwrap();
        if proven {
            rs[1].device_host_binding = Some(c.proof.binding().into());
            rs[0].origin_device_id = Some("old-public-id".into());
        }
        write(&base, &rs);
        let expected_device = c.device.device_id.clone();
        prepare_workspace_event_sync_with(app.handle(), &scope, || {
            migrate_device_homes_before_sync_with(
                app.handle(),
                |_, state| {
                    c.evidence = read_remote_evidence(&scope.db_path, &c.scope.owner_pubkey)?;
                    c.readiness = device_home_sync::readiness_locked(state, &c.scope)?;
                    assert_eq!(c.readiness, EvidenceReadiness::Pending);
                    Ok(c)
                },
                |_| panic!("Pending workspace cannot claim an unbound legacy key"),
            )
        })
        .unwrap();
        assert!(scope.db_path.is_file());
        let after = read_policy_records(&base.join("managed-agents.json")).unwrap();
        assert_eq!(after[1].device_host_binding.is_some(), proven);
        assert_eq!(
            after[0].origin_device_id.as_deref(),
            proven.then_some(expected_device.as_str())
        );
        let current = device_home_sync::capture_scope(&state).unwrap();
        assert_eq!(
            device_home_sync::readiness_locked(&state, &current).unwrap(),
            EvidenceReadiness::Pending
        );
        if !proven {
            let session = device_home_sync::begin_session(&state).unwrap();
            device_home_sync::hydrate_history(
                &state,
                &session.token,
                |_| async { Ok(vec![]) },
                |_| async { Ok(()) },
            )
            .await
            .unwrap();
            let mut ready = context(EvidenceReadiness::Ready);
            device_home_sync::finish_session_with(&state, &session.token, |verified| {
                ready.scope = verified.clone();
                ready.evidence = read_remote_evidence(&scope.db_path, &verified.owner_pubkey)?;
                migrate_device_homes_locked_with(app.handle(), &ready, |_| Ok(Some(key.clone())))
            })
            .unwrap();
            assert!(
                read_policy_records(&base.join("managed-agents.json")).unwrap()[1]
                    .device_host_binding
                    .is_some()
            );
            assert_eq!(
                device_home_sync::readiness_locked(&state, &current).unwrap(),
                EvidenceReadiness::Ready
            );
        }
    }
}

#[test]
fn workspace_preparation_database_open_and_schema_errors_are_fatal() {
    for directory in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<AppState>();
        let base = crate::managed_agents::managed_agents_base_dir(app.handle()).unwrap();
        let scope = active_retention_scope(app.handle(), &state).unwrap();
        std::fs::remove_file(&scope.db_path).unwrap();
        if directory {
            std::fs::create_dir(&scope.db_path).unwrap();
        } else {
            std::fs::write(&scope.db_path, b"not a SQLite database").unwrap();
        }
        let (rs, _) = records();
        write(&base, &rs);
        let before = std::fs::read(base.join("managed-agents.json")).unwrap();
        let error = prepare_workspace_event_sync_with(app.handle(), &scope, || {
            panic!("failed retention initialization reached authority lookup")
        })
        .unwrap_err();
        assert!(
            error.contains("retention") || error.contains("WAL"),
            "{error}"
        );
        assert_eq!(
            before,
            std::fs::read(base.join("managed-agents.json")).unwrap()
        );
    }
}
#[test]
fn recovery_precedes_migration_and_replays_original_scope_after_switch() {
    use crate::managed_agents::{
        device_home_operations::{commit_claim_in_dir, recover_home_operations_locked_with},
        retention::{get_pending_sync, open_retention_db, scoped_retention_db_path},
    };
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<AppState>();
    let base = crate::managed_agents::managed_agents_base_dir(app.handle()).unwrap();
    let scope = active_retention_scope(app.handle(), &state).unwrap();
    let (mut rs, _) = records();
    let i = rs.pop().unwrap();
    rs[0].origin_released = Some(true);
    write(&base, &rs);
    let owner = nostr::Keys::generate();
    let mut c = context(EvidenceReadiness::Ready);
    c.scope.owner_pubkey = owner.public_key().to_hex();
    c.scope.relay_url = "wss://original".into();
    assert!(commit_claim_in_dir(
        dir.path(),
        &c,
        "one",
        i,
        &owner,
        |raw| crate::managed_agents::atomic_write_json_restricted(
            &base.join("managed-agents.json"),
            &serde_json::to_vec_pretty(raw).unwrap()
        ),
        |_| Err("retention failure".into())
    )
    .is_err());
    prepare_workspace_event_sync_with_recovery(
        app.handle(),
        &scope,
        || {
            recover_home_operations_locked_with(app.handle(), |binding| {
                Ok(c.proof.matches(binding))
            })
        },
        || {
            assert_eq!(
                std::fs::read(dir.path().join("device-home-operations.json")).unwrap(),
                b"[]"
            );
            Ok(())
        },
    )
    .unwrap();
    let original = scoped_retention_db_path(&base, &c.scope.relay_url, &c.scope.owner_pubkey);
    let conn = open_retention_db(&original).unwrap();
    assert_eq!(get_pending_sync(&conn).unwrap().len(), 2);
    let current = open_retention_db(&scope.db_path).unwrap();
    assert!(get_pending_sync(&current).unwrap().is_empty());
}
#[test]
fn recovery_failure_blocks_migration_without_rewriting_intent() {
    use std::cell::Cell;
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let state = app.state::<AppState>();
    let scope = active_retention_scope(app.handle(), &state).unwrap();
    let called = Cell::new(false);
    assert!(prepare_workspace_event_sync_with_recovery(
        app.handle(),
        &scope,
        || Err("proof unavailable".into()),
        || {
            called.set(true);
            Ok(())
        }
    )
    .is_err());
    assert!(!called.get());
}
