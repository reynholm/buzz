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
