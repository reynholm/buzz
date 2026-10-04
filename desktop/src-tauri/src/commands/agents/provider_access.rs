//! Upgrade reconciliation for provider-backed managed-agent access.

use tauri::AppHandle;

use crate::{
    app_state::AppState,
    managed_agents::{device_runtime, BackendKind, ManagedAgentRecord},
    util::now_iso,
};

pub(super) fn needs_reconciliation_with_policy(
    record: &ManagedAgentRecord,
    owner_only_access: bool,
) -> bool {
    (owner_only_access || record.provider_policy_pending)
        && record.backend != BackendKind::Local
        && record.backend_agent_id.is_some()
}

#[derive(Debug)]
struct ProviderAccessTarget {
    pubkey: String,
    agent_json: Result<serde_json::Value, String>,
}

fn collect_targets_with(
    records: Vec<ManagedAgentRecord>,
    owner_only_access: bool,
    mut build_payload: impl FnMut(&ManagedAgentRecord) -> Result<serde_json::Value, String>,
) -> Vec<ProviderAccessTarget> {
    records
        .into_iter()
        .filter(|record| needs_reconciliation_with_policy(record, owner_only_access))
        .map(|record| match record.backend {
            BackendKind::Provider { .. } => ProviderAccessTarget {
                agent_json: build_payload(&record),
                pubkey: record.pubkey,
            },
            BackendKind::Local => {
                unreachable!("provider access reconciliation selected a local agent")
            }
        })
        .collect()
}

fn collect_runtime_targets_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    state: &AppState,
    owner_only: bool,
    context: impl FnOnce(
        &tauri::AppHandle<R>,
        &AppState,
    ) -> Result<
        crate::managed_agents::persona_device_view::DevicePolicyContext,
        String,
    >,
    build: impl FnMut(&ManagedAgentRecord) -> Result<serde_json::Value, String>,
) -> Result<Vec<ProviderAccessTarget>, String> {
    let raw = crate::managed_agents::persona_device_view::read_policy_records(
        &crate::managed_agents::managed_agents_store_path(app)?,
    )?;
    let definition_rows: Vec<_> = raw
        .iter()
        .filter(|r| r.pubkey.is_empty())
        .cloned()
        .collect();
    let definitions = crate::managed_agents::persona_definitions_for_policy(&definition_rows);
    let mut selected: Vec<_> = raw
        .into_iter()
        .filter(|r| needs_reconciliation_with_policy(r, owner_only))
        .collect();
    let needs_authority = selected.iter().any(|r| {
        r.persona_id.as_deref().is_some_and(|id| {
            definitions
                .iter()
                .any(|d| d.id == id && d.share_across_devices != Some(true))
        })
    });
    let fence = device_runtime::capture_runtime_fence(state)?;
    let scope = &fence.scope;
    let context = if needs_authority {
        match context(app, state) {
            Ok(context) => {
                crate::managed_agents::device_creation::assert_creation_scope(
                    scope,
                    &context.scope,
                )?;
                Some(context)
            }
            Err(error) => {
                eprintln!("buzz-desktop: provider access private authority unavailable: {error}");
                None
            }
        }
    } else {
        None
    };
    selected.retain(|r| {
        match device_runtime::runtime_start_refusal(r, &definitions, context.as_ref()) {
            None => true,
            Some(error) => {
                eprintln!(
                    "buzz-desktop: provider access skipped for {}: {error}",
                    r.pubkey
                );
                false
            }
        }
    });
    device_runtime::assert_runtime_fence(state, &fence)?;
    Ok(collect_targets_with(selected, owner_only, build))
}

/// Redeploy existing provider agents whose access policy requires enforcement.
///
/// Owner-only builds refresh every existing deployment before each community UI
/// load. All builds also retry records whose saved policy has not yet been
/// acknowledged by a successful provider deployment. Workspace apply fails
/// closed if any selected provider rejects the current policy.
pub(crate) async fn reconcile_on_workspace_apply(
    app: &AppHandle,
    state: &AppState,
) -> Result<(), String> {
    let owner_only_access = crate::managed_agents::owner_only_access_build();
    let (targets, fence) = {
        let _store_guard = state
            .managed_agents_store_lock
            .lock()
            .map_err(|error| error.to_string())?;
        let fence = device_runtime::capture_runtime_fence(state)?;
        let targets = collect_runtime_targets_with(
            app,
            state,
            owner_only_access,
            crate::managed_agents::persona_device_view::load_device_policy_context,
            |record| super::build_deploy_payload(app, state, record),
        )?;
        (targets, fence)
    };

    for target in targets {
        let ProviderAccessTarget { pubkey, agent_json } = target;
        match agent_json {
            Ok(_) => {}
            Err(error) => {
                persist_failure(app, state, &pubkey, &error, &fence)?;
                return Err(format!(
                    "provider access reconciliation failed for agent {pubkey}: {error}"
                ));
            }
        };
        if let Err(error) = super::provider_deploy::deploy_to_provider_scoped(
            app,
            state,
            &pubkey,
            super::provider_deploy::ProviderStartScope {
                relay: Some(&fence.scope.relay_url),
                owner: Some(&fence.scope.owner_pubkey),
                replay_floor: None,
                fence: Some(&fence),
            },
        )
        .await
        {
            return Err(format!(
                "provider access reconciliation failed for agent {pubkey}: {error}"
            ));
        }
    }

    Ok(())
}

fn persist_failure(
    app: &AppHandle,
    state: &AppState,
    pubkey: &str,
    error: &str,
    fence: &device_runtime::RuntimeFence,
) -> Result<(), String> {
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|lock_error| lock_error.to_string())?;
    persist_failure_with(
        app,
        state,
        pubkey,
        error,
        fence,
        crate::managed_agents::persona_device_view::load_device_policy_context,
    )
}

fn persist_failure_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    state: &AppState,
    pubkey: &str,
    error: &str,
    fence: &device_runtime::RuntimeFence,
    context: impl FnOnce(
        &tauri::AppHandle<R>,
        &AppState,
    ) -> Result<
        crate::managed_agents::persona_device_view::DevicePolicyContext,
        String,
    >,
) -> Result<(), String> {
    device_runtime::assert_runtime_fence(state, fence)?;
    device_runtime::provider_phase_with(
        app,
        state,
        pubkey,
        Some(&fence.scope),
        context,
        |mut record, _, _| {
            record.last_error = Some(error.to_string());
            record.updated_at = now_iso();
            crate::managed_agents::storage::save_restore_records_with(
                app,
                &[record],
                &std::collections::HashSet::new(),
                crate::managed_agents::storage::persist_agent_keys,
            )
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(backend: BackendKind, backend_agent_id: Option<&str>) -> ManagedAgentRecord {
        let mut record: ManagedAgentRecord = serde_json::from_value(serde_json::json!({
            "pubkey": "agent", "name": "Agent", "relay_url": "", "acp_command": "",
            "agent_command": "", "agent_args": [], "mcp_command": "",
            "turn_timeout_seconds": 0, "system_prompt": null, "created_at": "",
            "updated_at": "", "last_started_at": null, "last_stopped_at": null,
            "last_exit_code": null, "last_error": null
        }))
        .unwrap();
        record.backend = backend;
        record.backend_agent_id = backend_agent_id.map(str::to_string);
        record
    }

    #[test]
    fn upgrade_collects_existing_provider_and_builds_projected_payload() {
        let records = vec![
            record(
                BackendKind::Provider {
                    id: "provider".into(),
                    config: serde_json::json!({"region": "test"}),
                },
                Some("existing"),
            ),
            record(
                BackendKind::Provider {
                    id: "not-deployed".into(),
                    config: serde_json::json!({}),
                },
                None,
            ),
            record(BackendKind::Local, Some("stale")),
        ];

        let targets = collect_targets_with(records, true, |r| {
            Ok(serde_json::json!({"respond_to": "owner-only", "backend": r.backend}))
        });

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].pubkey, "agent");
        assert_eq!(
            targets[0].agent_json.as_ref().unwrap()["backend"]["id"],
            "provider"
        );
        assert_eq!(
            targets[0].agent_json.as_ref().unwrap()["backend"]["config"]["region"],
            "test"
        );
        assert_eq!(
            targets[0].agent_json.as_ref().unwrap()["respond_to"],
            "owner-only"
        );
    }

    #[test]
    fn unmarked_build_collects_only_pending_targets() {
        let mut pending = record(
            BackendKind::Provider {
                id: "pending-provider".into(),
                config: serde_json::json!({}),
            },
            Some("existing-pending"),
        );
        pending.pubkey = "pending-agent".into();
        pending.provider_policy_pending = true;
        let ordinary = record(
            BackendKind::Provider {
                id: "ordinary-provider".into(),
                config: serde_json::json!({}),
            },
            Some("existing-ordinary"),
        );

        let targets = collect_targets_with(vec![ordinary, pending], false, |record| {
            Ok(serde_json::json!({"pubkey": record.pubkey}))
        });

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].pubkey, "pending-agent");
        assert_eq!(
            targets[0].agent_json.as_ref().unwrap()["pubkey"],
            "pending-agent"
        );
    }

    #[test]
    fn pending_policy_requires_an_existing_provider_deployment() {
        let mut undeployed = record(
            BackendKind::Provider {
                id: "provider".into(),
                config: serde_json::json!({}),
            },
            None,
        );
        undeployed.provider_policy_pending = true;
        let mut local = record(BackendKind::Local, Some("stale-provider-id"));
        local.provider_policy_pending = true;

        assert!(collect_targets_with(vec![undeployed, local], false, |_| {
            Ok(serde_json::Value::Null)
        })
        .is_empty());
    }
}

#[cfg(test)]
mod runtime_tests {
    use super::*;
    use crate::managed_agents::device_home_migration::tests::{app, records, write};
    use tauri::Manager;

    #[test]
    fn provider_access_authority_scope_switch_refuses_before_payload_effects() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<AppState>();
        let (mut raw, _) = records();
        let mut c = crate::managed_agents::device_home_migration::tests::context(
            crate::managed_agents::definition_home::EvidenceReadiness::Ready,
        );
        c.scope = crate::managed_agents::device_home_sync::capture_scope(&state).unwrap();
        raw[1].device_host_binding = Some(c.proof.binding().into());
        raw[1].provider_policy_pending = true;
        raw[1].backend_agent_id = Some("deployed".into());
        raw[1].backend = BackendKind::Provider {
            id: "isolated".into(),
            config: serde_json::json!({}),
        };
        write(
            &crate::managed_agents::managed_agents_base_dir(app.handle()).unwrap(),
            &raw,
        );
        let payloads = std::cell::Cell::new(0);
        let result = collect_runtime_targets_with(
            app.handle(),
            &state,
            false,
            |_, state| {
                crate::managed_agents::device_home_sync::begin_session(state)?;
                Ok(c)
            },
            |_| {
                payloads.set(1);
                Ok(serde_json::json!({}))
            },
        );
        assert!(
            result.is_err(),
            "provider access accepted stale subscription authority"
        );
        assert_eq!(payloads.get(), 0);
    }

    #[test]
    fn provider_access_failure_writeback_is_scoped_and_preserves_pending() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<AppState>();
        let (mut raw, _) = records();
        raw[0].share_across_devices = Some(true);
        raw[1].provider_policy_pending = true;
        write(
            &crate::managed_agents::managed_agents_base_dir(app.handle()).unwrap(),
            &raw,
        );
        let path = crate::managed_agents::managed_agents_store_path(app.handle()).unwrap();
        let fence = device_runtime::capture_runtime_fence(&state).unwrap();
        persist_failure_with(
            app.handle(),
            &state,
            &raw[1].pubkey,
            "provider unavailable",
            &fence,
            |_, _| panic!("shared failure requires no proof"),
        )
        .unwrap();
        let persisted =
            crate::managed_agents::persona_device_view::read_policy_records(&path).unwrap();
        let target = persisted
            .iter()
            .find(|r| r.pubkey == raw[1].pubkey)
            .unwrap();
        assert!(target.provider_policy_pending);
        assert_eq!(target.last_error.as_deref(), Some("provider unavailable"));
        let before = std::fs::read(&path).unwrap();
        state
            .workspace_apply_generation
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        assert!(persist_failure_with(
            app.handle(),
            &state,
            &raw[1].pubkey,
            "stale failure",
            &fence,
            |_, _| panic!("stale failure resolved proof")
        )
        .is_err());
        assert_eq!(std::fs::read(path).unwrap(), before);
    }

    #[test]
    fn provider_access_mixed_batch_preserves_denied_pending_without_keys() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<AppState>();
        let (mut raw, _) = records();
        raw[1].device_host_binding = Some("copied".into());
        raw[1].backend = BackendKind::Provider {
            id: "isolated".into(),
            config: serde_json::json!({}),
        };
        raw[1].backend_agent_id = Some("already-deployed".into());
        raw[1].provider_policy_pending = true;
        let mut shared_def = raw[0].clone();
        shared_def.slug = Some("shared".into());
        shared_def.share_across_devices = Some(true);
        let mut shared = raw[1].clone();
        shared.pubkey = nostr::Keys::generate().public_key().to_hex();
        shared.persona_id = Some("shared".into());
        raw.extend([shared_def, shared.clone()]);
        write(
            &crate::managed_agents::managed_agents_base_dir(app.handle()).unwrap(),
            &raw,
        );
        let path = crate::managed_agents::managed_agents_store_path(app.handle()).unwrap();
        let before = std::fs::read(&path).unwrap();
        let mut built = Vec::new();
        let selected = collect_runtime_targets_with(
            app.handle(),
            &state,
            false,
            |_, _| Err("inaccessible isolated marker".into()),
            |r| {
                built.push(r.pubkey.clone());
                Ok(serde_json::json!({}))
            },
        )
        .unwrap();
        assert_eq!(
            built,
            vec![shared.pubkey.clone()],
            "copied private reached payload/key effects"
        );
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].pubkey, shared.pubkey);
        assert_eq!(
            std::fs::read(path).unwrap(),
            before,
            "denied pending or copied secrets changed"
        );
    }
}
