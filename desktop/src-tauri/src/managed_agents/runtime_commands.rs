use std::sync::atomic::Ordering;

use tauri::{AppHandle, Emitter, Manager};

use super::{
    agent_readiness, append_log_marker, current_instance_id, find_managed_agent_mut,
    load_global_agent_config, load_managed_agents, load_personas, managed_agent_runtime_log_path,
    process_is_running, record_agent_command, resolve_effective_agent_env, save_managed_agents,
    spawn_agent_child, terminate_process, terminate_untracked_pair_runtime,
    write_agent_runtime_receipt, AgentReadiness, BackendKind, ManagedAgentPairRuntime,
    ManagedAgentRuntimeKey, ManagedAgentRuntimeLifecycle, ManagedAgentRuntimeReceipt,
    ManagedAgentRuntimeStatus,
};
use crate::app_state::AppState;

const STATUS_EVENT: &str = "managed-agent-runtime-status";

fn status_for<R: tauri::Runtime>(
    app: &AppHandle<R>,
    record: &super::ManagedAgentRecord,
    key: &ManagedAgentRuntimeKey,
    runtime: Option<&ManagedAgentPairRuntime>,
    requested_relay_url: Option<String>,
) -> ManagedAgentRuntimeStatus {
    let personas = load_personas(app).unwrap_or_default();
    let global = load_global_agent_config(app).unwrap_or_default();
    status_for_with(
        app,
        record,
        key,
        runtime,
        requested_relay_url,
        StatusInputs {
            personas: &personas,
            global: &global,
        },
    )
}

/// Preloaded per-call-site inputs for [`status_for_with`], so multi-row
/// callers (list, reconcile) hit disk once instead of once per row.
struct StatusInputs<'a> {
    personas: &'a [super::AgentDefinition],
    global: &'a super::GlobalAgentConfig,
}

fn status_for_with<R: tauri::Runtime>(
    app: &AppHandle<R>,
    record: &super::ManagedAgentRecord,
    key: &ManagedAgentRuntimeKey,
    runtime: Option<&ManagedAgentPairRuntime>,
    requested_relay_url: Option<String>,
    inputs: StatusInputs<'_>,
) -> ManagedAgentRuntimeStatus {
    let StatusInputs { personas, global } = inputs;
    let command = record_agent_command(record, personas);
    let metadata = super::known_acp_runtime(&command);
    let effective = resolve_effective_agent_env(record, personas, metadata, global);
    let local_setup = matches!(agent_readiness(&effective), AgentReadiness::Ready);
    ManagedAgentRuntimeStatus {
        pubkey: key.pubkey.clone(),
        relay_url: key.relay_url.clone(),
        requested_relay_url,
        local_setup,
        lifecycle: runtime
            .map(|runtime| runtime.lifecycle.clone())
            .unwrap_or(ManagedAgentRuntimeLifecycle::Stopped),
        pid: runtime.map(|runtime| runtime.child.id()),
        error: runtime.and_then(|runtime| runtime.error.clone()),
        log_path: managed_agent_runtime_log_path(app, key)
            .ok()
            .map(|path| path.display().to_string()),
    }
}

fn emit_status<R: tauri::Runtime>(app: &AppHandle<R>, status: &ManagedAgentRuntimeStatus) {
    let _ = app.emit(STATUS_EVENT, status);
}

fn observer_lifecycle_key(
    outer_pubkey: &str,
    payload: &super::ManagedAgentRuntimeLifecycleObserverPayload,
) -> Result<ManagedAgentRuntimeKey, String> {
    if !outer_pubkey.eq_ignore_ascii_case(&payload.pubkey) {
        return Err("observer signer does not match lifecycle payload pubkey".into());
    }
    if matches!(
        payload.lifecycle,
        ManagedAgentRuntimeLifecycle::Starting | ManagedAgentRuntimeLifecycle::Stopped
    ) {
        return Err("observer cannot author starting or stopped lifecycle".into());
    }
    if payload.lifecycle == ManagedAgentRuntimeLifecycle::Failed && payload.error.is_none() {
        return Err("failed lifecycle requires an error".into());
    }
    if payload.lifecycle != ManagedAgentRuntimeLifecycle::Failed && payload.error.is_some() {
        return Err("lifecycle error is only valid for failed".into());
    }
    ManagedAgentRuntimeKey::new(payload.pubkey.clone(), &payload.relay_url)
}

#[tauri::command]
pub fn put_managed_agent_runtime_lifecycle(
    outer_pubkey: String,
    payload: super::ManagedAgentRuntimeLifecycleObserverPayload,
    app: AppHandle,
) -> Result<ManagedAgentRuntimeStatus, String> {
    let key = observer_lifecycle_key(&outer_pubkey, &payload)?;
    let state = app.state::<AppState>();
    let records = load_managed_agents(&app)?;
    let record = records
        .iter()
        .find(|record| record.pubkey.eq_ignore_ascii_case(&key.pubkey))
        .ok_or_else(|| format!("agent {} not found", key.pubkey))?;
    let mut runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|e| e.to_string())?;
    let runtime = runtimes
        .get_mut(&key)
        .ok_or_else(|| "lifecycle frame does not match a tracked runtime pair".to_string())?;
    if runtime.start_nonce != payload.start_nonce {
        return Err("lifecycle frame does not match the current harness generation".into());
    }
    if runtime
        .child
        .try_wait()
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Err("lifecycle frame arrived after process exit".into());
    }
    runtime.lifecycle = payload.lifecycle;
    runtime.error = payload.error;
    let status = status_for(&app, record, &key, Some(runtime), None);
    emit_status(&app, &status);
    Ok(status)
}

// Keep disk, process, and mutex work off the main thread so opening members cannot stall the UI.
#[tauri::command]
pub async fn list_managed_agent_runtimes(
    app: AppHandle,
) -> Result<Vec<ManagedAgentRuntimeStatus>, String> {
    tokio::task::spawn_blocking(move || {
        // This command is polled whenever the members sidebar opens and refetched
        // on every status event — load the per-row status inputs once, outside
        // the locks, instead of hitting disk per row while holding them.
        let personas = load_personas(&app).unwrap_or_default();
        let global = load_global_agent_config(&app).unwrap_or_default();
        let state = app.state::<AppState>();
        let _transition = state
            .managed_agent_runtime_transition
            .lock()
            .map_err(|e| e.to_string())?;
        let _store = state
            .managed_agents_store_lock
            .lock()
            .map_err(|e| e.to_string())?;
        let mut records = load_managed_agents(&app)?;
        let mut runtimes = state
            .managed_agent_processes
            .lock()
            .map_err(|e| e.to_string())?;
        let exited_keys: Vec<_> = runtimes
            .iter_mut()
            .filter_map(|(key, runtime)| match runtime.child.try_wait() {
                Ok(Some(_)) | Err(_) => Some(key.clone()),
                Ok(None) => None,
            })
            .collect();
        let records_changed = !exited_keys.is_empty();
        let mut statuses = Vec::new();
        for key in exited_keys {
            runtimes.remove(&key);
            super::remove_agent_runtime_receipt(&app, &key);
            state.clear_agent_session_cache(&key);
            if let Some(record) = records
                .iter_mut()
                .find(|record| record.pubkey.eq_ignore_ascii_case(&key.pubkey))
            {
                record.updated_at = crate::util::now_iso();
                record.last_stopped_at = Some(record.updated_at.clone());
                let status = status_for_with(
                    &app,
                    record,
                    &key,
                    None,
                    None,
                    StatusInputs {
                        personas: &personas,
                        global: &global,
                    },
                );
                emit_status(&app, &status);
                statuses.push(status);
            }
        }
        statuses.extend(runtimes.iter().filter_map(|(key, runtime)| {
            let record = records
                .iter()
                .find(|record| record.pubkey.eq_ignore_ascii_case(&key.pubkey))?;
            Some(status_for_with(
                &app,
                record,
                key,
                Some(runtime),
                None,
                StatusInputs {
                    personas: &personas,
                    global: &global,
                },
            ))
        }));
        drop(runtimes);
        // Records are only mutated above when a runtime exited — skip the store
        // rewrite on the common nothing-changed poll.
        if records_changed {
            save_managed_agents(&app, &records)?;
        }
        Ok(statuses)
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

pub(crate) fn start_managed_agent_runtime_pair_lazy(
    pubkey: String,
    relay_url: String,
    admission: &super::AdmissionSnapshot,
    app: AppHandle,
) -> Result<ManagedAgentRuntimeStatus, String> {
    start_pair(pubkey, relay_url, true, None, admission, app)
}

pub(crate) fn start_managed_agent_pair_scoped<R: tauri::Runtime>(
    pubkey: String,
    relay_url: String,
    admission: &super::AdmissionSnapshot,
    app: AppHandle<R>,
    expected: &super::device_runtime::RuntimeFence,
) -> Result<ManagedAgentRuntimeStatus, String> {
    start_pair_scoped(
        pubkey,
        relay_url,
        true,
        None,
        PairStartScope {
            expected: Some(expected),
            restart: false,
        },
        admission,
        app,
    )
}

#[tauri::command]
pub fn start_managed_agent_runtime(
    pubkey: String,
    relay_url: String,
    app: AppHandle,
) -> Result<ManagedAgentRuntimeStatus, String> {
    let admission = super::AdmissionSnapshot::capture(&app.state::<AppState>());
    start_managed_agent_runtime_pair_lazy(pubkey, relay_url, &admission, app)
}

fn start_pair<R: tauri::Runtime>(
    pubkey: String,
    relay_url: String,
    lazy: bool,
    expected_updated_at: Option<&str>,
    admission: &super::AdmissionSnapshot,
    app: AppHandle<R>,
) -> Result<ManagedAgentRuntimeStatus, String> {
    start_pair_scoped(
        pubkey,
        relay_url,
        lazy,
        expected_updated_at,
        PairStartScope {
            expected: None,
            restart: false,
        },
        admission,
        app,
    )
}

struct PairStartScope<'a> {
    expected: Option<&'a super::device_runtime::RuntimeFence>,
    restart: bool,
}

fn start_pair_scoped<R: tauri::Runtime>(
    pubkey: String,
    relay_url: String,
    lazy: bool,
    expected_updated_at: Option<&str>,
    scope: PairStartScope<'_>,
    admission: &super::AdmissionSnapshot,
    app: AppHandle<R>,
) -> Result<ManagedAgentRuntimeStatus, String> {
    let state = app.state::<AppState>();
    let transition = state
        .managed_agent_runtime_transition
        .lock()
        .map_err(|e| e.to_string())?;
    if state.shutdown_started.load(Ordering::Acquire) {
        return Err("desktop shutdown has started".into());
    }
    let admitted = transition.admit(admission, &relay_url)?;
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    if let Some(expected) = scope.expected {
        super::device_runtime::assert_runtime_fence(&state, expected)?;
    }
    super::device_runtime::start_pair_phase_with(
        &app,
        &state,
        &pubkey,
        scope.expected.map(|e| &e.scope),
        super::persona_device_view::load_device_policy_context,
        |mut record, _, _| {
            super::storage::hydrate_keys(std::slice::from_mut(&mut record));
            let record = &mut record;
            if record.backend != BackendKind::Local {
                return Err("managed runtime pairs require a local agent".into());
            }
            if expected_updated_at.is_some_and(|expected| record.updated_at != expected) {
                return Err(
                    "managed agent changed while runtime reconciliation was in flight".into(),
                );
            }
            let key = ManagedAgentRuntimeKey::new(pubkey.clone(), &relay_url)?;
            let mut runtimes = state
                .managed_agent_processes
                .lock()
                .map_err(|e| e.to_string())?;
            if !scope.restart
                && runtimes
                    .get_mut(&key)
                    .is_some_and(|runtime| runtime.child.try_wait().ok().flatten().is_none())
            {
                let status = status_for(&app, record, &key, runtimes.get(&key), None);
                return Ok(status);
            }
            if let Some(mut previous) = runtimes.remove(&key) {
                if let Err(error) = terminate_process(previous.child.id())
                    .and_then(|()| previous.child.wait().map(|_| ()).map_err(|e| e.to_string()))
                {
                    runtimes.insert(key.clone(), previous);
                    return Err(error);
                }
                super::remove_agent_runtime_receipt(&app, &key);
                state.clear_agent_session_cache(&key);
            }
            terminate_untracked_pair_runtime(&app, &key)?;

            let owner = state
                .keys
                .lock()
                .ok()
                .map(|keys| keys.public_key().to_hex());
            let mut process = spawn_agent_child(
                &app,
                record,
                &key.relay_url,
                &admitted,
                lazy,
                owner.as_deref(),
                None,
            )?;
            let now = crate::util::now_iso();
            let receipt = ManagedAgentRuntimeReceipt {
                key: key.clone(),
                pid: process.child.id(),
                desktop_instance_id: current_instance_id(&app),
                started_at: now.clone(),
            };
            if let Err(error) = write_agent_runtime_receipt(&app, &receipt) {
                let _ = terminate_process(process.child.id());
                let _ = process.child.wait();
                return Err(error);
            }
            record.runtime_pid = None;
            record.updated_at = now.clone();
            record.last_started_at = Some(now);
            record.last_stopped_at = None;
            record.last_error = None;
            runtimes.insert(key.clone(), ManagedAgentPairRuntime::starting(process));
            let status = status_for(&app, record, &key, runtimes.get(&key), None);
            drop(runtimes);
            super::device_runtime::save_runtime_record(&app, record)?;
            emit_status(&app, &status);
            Ok(status)
        },
    )
}

#[tauri::command]
pub fn stop_managed_agent_runtime(
    pubkey: String,
    relay_url: String,
    app: AppHandle,
) -> Result<ManagedAgentRuntimeStatus, String> {
    stop_pair(pubkey, relay_url, app)
}

fn stop_pair<R: tauri::Runtime>(
    pubkey: String,
    relay_url: String,
    app: AppHandle<R>,
) -> Result<ManagedAgentRuntimeStatus, String> {
    let state = app.state::<AppState>();
    let _transition = state
        .managed_agent_runtime_transition
        .lock()
        .map_err(|e| e.to_string())?;
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let mut records = load_managed_agents(&app)?;
    let record = find_managed_agent_mut(&mut records, &pubkey)?;
    let key = ManagedAgentRuntimeKey::new(pubkey.clone(), &relay_url)?;
    let mut runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|e| e.to_string())?;
    if let Some(mut runtime) = runtimes.remove(&key) {
        let stop_result = if process_is_running(runtime.child.id()) {
            terminate_process(runtime.child.id())
        } else {
            Ok(())
        }
        .and_then(|()| runtime.child.wait().map_err(|e| e.to_string()));
        match stop_result {
            Ok(status) => {
                record.last_exit_code = status.code();
                let _ = append_log_marker(&runtime.log_path, "=== stopped pair runtime ===");
            }
            Err(error) => {
                // Keep failed teardown visible/manageable instead of
                // orphaning it: the child stays tracked and the receipt
                // stays on disk until a stop actually succeeds.
                runtimes.insert(key, runtime);
                return Err(error);
            }
        }
    } else {
        // No runtime is tracked at this key, but a valid prior-session
        // receipt may still point at a live child (e.g. the crash-recovery
        // window for a non-auto-start agent). Terminate that orphan before
        // erasing its receipt — otherwise this "stop" leaves the harness
        // running yet deletes the one artifact sweeps and
        // terminate_untracked_pair_runtime use to find it, and a follow-up
        // start would spawn a duplicate harness for the same pair. On
        // failure the receipt stays on disk (terminate_untracked_pair_runtime
        // only removes it after the child exits), mirroring the tracked
        // path's keep-until-success invariant.
        terminate_untracked_pair_runtime(&app, &key)?;
    }
    super::remove_agent_runtime_receipt(&app, &key);
    state.clear_agent_session_cache(&key);
    record.runtime_pid = None;
    record.updated_at = crate::util::now_iso();
    record.last_stopped_at = Some(record.updated_at.clone());
    let status = status_for(&app, record, &key, None, None);
    drop(runtimes);
    save_managed_agents(&app, &records)?;
    emit_status(&app, &status);
    Ok(status)
}

#[tauri::command]
pub fn restart_managed_agent_runtime(
    pubkey: String,
    relay_url: String,
    app: AppHandle,
) -> Result<ManagedAgentRuntimeStatus, String> {
    // Keep device authorization and teardown in the same transition: a refused
    // restart must leave the existing child running.
    restart_pair(pubkey, relay_url, app, || Ok(()))
}

fn restart_pair<R: tauri::Runtime>(
    pubkey: String,
    relay_url: String,
    app: AppHandle<R>,
    stop: impl FnOnce() -> Result<(), String>,
) -> Result<ManagedAgentRuntimeStatus, String> {
    let admission = super::AdmissionSnapshot::capture(&app.state::<AppState>());
    stop()?;
    start_pair_scoped(
        pubkey,
        relay_url,
        true,
        None,
        PairStartScope {
            expected: None,
            restart: true,
        },
        &admission,
        app,
    )
}

/// Probe whether this agent can operate on `requested_relay_url`.
///
/// Runs a bounded authenticated query with the agent's own keys (NIP-42 +
/// NIP-OA auth tag). Auth success is the spawn-eligibility signal: NIP-29
/// membership (kind 39002) cannot exist before the agent's harness first
/// connects to a relay, so gating on membership *presence* could never
/// bootstrap a pair on a newly configured community — it only rediscovered
/// pairs that had already run. A rejected or timed-out probe surfaces as a
/// Failed status row instead of a silent skip.
async fn probe_agent_relay_access(
    state: &AppState,
    record: super::ManagedAgentRecord,
    requested_relay_url: String,
) -> Result<(super::ManagedAgentRecord, ManagedAgentRuntimeKey, String), String> {
    let key = ManagedAgentRuntimeKey::new(record.pubkey.clone(), &requested_relay_url)?;
    let keys = nostr::Keys::parse(record.private_key_nsec.trim())
        .map_err(|error| format!("invalid managed-agent key: {error}"))?;
    let api_base = crate::relay::relay_http_base_url(&key.relay_url);
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        crate::relay::query_relay_at_with_keys(
            state,
            &api_base,
            &[serde_json::json!({"kinds": [39002], "#p": [record.pubkey]})],
            &keys,
            record.auth_tag.as_deref(),
        ),
    )
    .await
    .map_err(|_| "relay access probe timed out".to_string())??;
    Ok((record, key, requested_relay_url))
}

/// Build the `Failed` status row for a probe failure whose requested relay URL
/// cannot even form a pair key (so there is no canonical `relay_url` to key on).
/// The raw requested URL stands in for both the identity and the requested
/// field so the batch still degrades this one community to a visible row
/// instead of aborting every other community's row.
fn unkeyable_failed_status(
    record: &super::ManagedAgentRecord,
    requested: String,
    error: String,
    personas: &[super::AgentDefinition],
    global: &super::GlobalAgentConfig,
) -> ManagedAgentRuntimeStatus {
    let command = record_agent_command(record, personas);
    let metadata = super::known_acp_runtime(&command);
    let effective = resolve_effective_agent_env(record, personas, metadata, global);
    ManagedAgentRuntimeStatus {
        pubkey: record.pubkey.clone(),
        relay_url: requested.clone(),
        requested_relay_url: Some(requested),
        local_setup: matches!(agent_readiness(&effective), AgentReadiness::Ready),
        lifecycle: ManagedAgentRuntimeLifecycle::Failed,
        pid: None,
        error: Some(error),
        log_path: None,
    }
}

/// Spawn a lazy harness pair for every eligible (agent, community) pair.
///
/// Eligibility is deliberately gated on `start_on_app_launch`: auto-start is
/// the *proactive fan-out* policy — "keep this agent warm in every community" —
/// not a correctness prerequisite. A manual-start agent still works on demand
/// everywhere: attaching it to a channel ensures its pair, an @mention wakes a
/// pair, the members sidebar and Settings controls start pairs, and restore
/// preserves running pairs across relaunch. Fanning out warm-socket pairs for
/// agents the user chose *not* to auto-start would contradict that choice, so
/// reconcile leaves them alone until something explicitly asks for them.
#[tauri::command]
pub async fn reconcile_managed_agent_runtimes(
    communities: Vec<super::ManagedAgentCommunityTarget>,
    app: AppHandle,
) -> Result<Vec<ManagedAgentRuntimeStatus>, String> {
    let admission = super::AdmissionSnapshot::capture(&app.state::<AppState>());
    let runtime_fence = super::device_runtime::capture_runtime_fence(&app.state::<AppState>())?;
    let jobs = auto_start_jobs_with(
        Some(&runtime_fence),
        &app,
        &communities,
        super::persona_device_view::load_device_policy_context,
        super::storage::hydrate_keys,
    )?;
    let probes = probe_auto_start_jobs(jobs, |record, requested| {
        let state = app.state::<AppState>();
        probe_auto_start_job_with(
            &app,
            RuntimeProbeInput {
                record,
                requested,
                fence: &runtime_fence,
            },
            super::persona_device_view::load_device_policy_context,
            super::storage::hydrate_keys,
            move |r, requested| async move { probe_agent_relay_access(&state, r, requested).await },
        )
    })
    .await;

    // start_pair does blocking work (std mutexes, process spawn, receipt
    // writes, and up-to-2s exit polling in terminate_untracked_pair_runtime),
    // so run the post-probe start loop off the async workers, matching the
    // restart flows.
    tokio::task::spawn_blocking(move || {
        let personas = load_personas(&app).unwrap_or_default();
        let global = load_global_agent_config(&app).unwrap_or_default();
        let mut rows = Vec::new();
        for probe in probes {
            match probe {
                Ok((record, key, requested)) => {
                    match start_pair_scoped(
                        record.pubkey.clone(),
                        key.relay_url.clone(),
                        true,
                        Some(&record.updated_at),
                        PairStartScope {
                            expected: Some(&runtime_fence),
                            restart: false,
                        },
                        &admission,
                        app.clone(),
                    ) {
                        Ok(mut status) => {
                            status.requested_relay_url = Some(requested);
                            rows.push(status);
                        }
                        // Removed mid-reconcile: nothing to start and nothing to report.
                        Err(error) if error == super::RELAY_REMOVED_ERROR => {}
                        Err(error) => {
                            let mut status = status_for_with(
                                &app,
                                &record,
                                &key,
                                None,
                                Some(requested),
                                StatusInputs {
                                    personas: &personas,
                                    global: &global,
                                },
                            );
                            status.lifecycle = ManagedAgentRuntimeLifecycle::Failed;
                            status.error = Some(error);
                            rows.push(status);
                        }
                    }
                }
                Err((record, requested, error)) => {
                    // Per-community degradation: a relay URL that cannot even
                    // form a pair key gets a Failed row (with the raw
                    // requested URL) like any other probe failure, instead of
                    // aborting every other community's row.
                    let status =
                        match ManagedAgentRuntimeKey::new(record.pubkey.clone(), &requested) {
                            Ok(key) => {
                                let mut status = status_for_with(
                                    &app,
                                    &record,
                                    &key,
                                    None,
                                    Some(requested),
                                    StatusInputs {
                                        personas: &personas,
                                        global: &global,
                                    },
                                );
                                status.lifecycle = ManagedAgentRuntimeLifecycle::Failed;
                                status.error = Some(error);
                                status
                            }
                            Err(_) => unkeyable_failed_status(
                                &record, requested, error, &personas, &global,
                            ),
                        };
                    rows.push(status);
                }
            }
        }
        rows
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_managed_agent_runtimes_returns_a_future() {
        fn assert_async_command<F, Fut>(_command: F)
        where
            F: Fn(AppHandle) -> Fut,
            Fut: std::future::Future<Output = Result<Vec<ManagedAgentRuntimeStatus>, String>>,
        {
        }

        assert_async_command(list_managed_agent_runtimes);
    }

    fn payload(
        relay_url: &str,
        lifecycle: ManagedAgentRuntimeLifecycle,
        error: Option<&str>,
    ) -> super::super::ManagedAgentRuntimeLifecycleObserverPayload {
        super::super::ManagedAgentRuntimeLifecycleObserverPayload {
            pubkey: "aa".repeat(32),
            relay_url: relay_url.into(),
            start_nonce: "test-generation".into(),
            lifecycle,
            error: error.map(str::to_owned),
        }
    }

    fn record_with_relay(relay_url: &str) -> super::super::ManagedAgentRecord {
        serde_json::from_str(&format!(
            r#"{{
                "pubkey": "{}",
                "name": "pin-test",
                "relay_url": "{relay_url}",
                "acp_command": "buzz-acp",
                "agent_command": "goose",
                "agent_args": [],
                "mcp_command": "",
                "turn_timeout_seconds": 320,
                "system_prompt": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:00Z"
            }}"#,
            "aa".repeat(32)
        ))
        .unwrap()
    }

    #[test]
    fn legacy_relay_pin_is_ignored_for_fan_out() {
        // Zero-touch cutover (#2122): a record carrying a creation-era
        // `relay_url` pin must fan out exactly like an unpinned one — the
        // stored field is parsed but never consulted. See
        // `effective_agent_relay_url`.
        let unpinned = record_with_relay("");
        let pinned = record_with_relay("wss://one.example");
        for record in [&unpinned, &pinned] {
            assert_eq!(
                crate::relay::effective_agent_relay_url(&record.relay_url, "wss://two.example"),
                "wss://two.example"
            );
        }
    }

    #[test]
    fn unkeyable_relay_degrades_to_failed_row() {
        // A requested URL that cannot form a pair key must still yield a
        // Failed row keyed by the raw requested string, so one bad community
        // never aborts the rest of the reconcile batch.
        let record = record_with_relay("");
        let status = unkeyable_failed_status(
            &record,
            "not a url".to_string(),
            "relay access probe timed out".to_string(),
            &[],
            &super::super::GlobalAgentConfig::default(),
        );
        assert!(matches!(
            status.lifecycle,
            ManagedAgentRuntimeLifecycle::Failed
        ));
        assert_eq!(status.relay_url, "not a url");
        assert_eq!(status.requested_relay_url.as_deref(), Some("not a url"));
        assert_eq!(status.pubkey, record.pubkey);
        assert_eq!(
            status.error.as_deref(),
            Some("relay access probe timed out")
        );
        assert!(status.pid.is_none());
    }

    #[test]
    fn runtime_key_rejects_non_hex_pubkeys() {
        assert!(ManagedAgentRuntimeKey::new("../not-a-key", "wss://relay.example").is_err());
        assert!(ManagedAgentRuntimeKey::new("gg".repeat(32), "wss://relay.example").is_err());
    }

    #[test]
    fn runtime_key_canonicalizes_hex_pubkeys() {
        let key = ManagedAgentRuntimeKey::new("AA".repeat(32), "wss://relay.example").unwrap();
        assert_eq!(key.pubkey, "aa".repeat(32));
    }

    #[test]
    fn observer_lifecycle_key_preserves_exact_canonical_pair() {
        let first = payload(
            "WSS://Relay.Example:443/",
            ManagedAgentRuntimeLifecycle::Ready,
            None,
        );
        let key = observer_lifecycle_key(&first.pubkey, &first).unwrap();
        assert_eq!(key.pubkey, first.pubkey);
        assert_eq!(key.relay_url, "wss://relay.example");

        let other = payload(
            "wss://other.example",
            ManagedAgentRuntimeLifecycle::Ready,
            None,
        );
        assert_ne!(key, observer_lifecycle_key(&other.pubkey, &other).unwrap());
    }

    #[test]
    fn observer_lifecycle_rejects_cross_agent_and_desktop_states() {
        let ready = payload(
            "wss://relay.example",
            ManagedAgentRuntimeLifecycle::Ready,
            None,
        );
        assert!(observer_lifecycle_key(&"bb".repeat(32), &ready).is_err());

        let stopped = payload(
            "wss://relay.example",
            ManagedAgentRuntimeLifecycle::Stopped,
            None,
        );
        assert!(observer_lifecycle_key(&stopped.pubkey, &stopped).is_err());
    }

    #[test]
    fn observer_lifecycle_enforces_failed_error_contract() {
        let failed = payload(
            "wss://relay.example",
            ManagedAgentRuntimeLifecycle::Failed,
            None,
        );
        assert!(observer_lifecycle_key(&failed.pubkey, &failed).is_err());

        let ready_with_error = payload(
            "wss://relay.example",
            ManagedAgentRuntimeLifecycle::Ready,
            Some("unexpected"),
        );
        assert!(observer_lifecycle_key(&ready_with_error.pubkey, &ready_with_error).is_err());
    }
}

/// Build the actual reconcile job set before key hydration or relay probes.
fn auto_start_jobs_with<R: tauri::Runtime>(
    expected: Option<&super::device_runtime::RuntimeFence>,
    app: &AppHandle<R>,
    communities: &[super::ManagedAgentCommunityTarget],
    context_provider: impl FnOnce(
        &AppHandle<R>,
        &AppState,
    )
        -> Result<super::persona_device_view::DevicePolicyContext, String>,
    hydrate: impl FnOnce(&mut [super::ManagedAgentRecord]),
) -> Result<Vec<(super::ManagedAgentRecord, String)>, String> {
    let state = app.state::<AppState>();
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let fence = super::device_runtime::capture_runtime_fence(&state)?;
    if let Some(expected) = expected {
        super::device_runtime::assert_runtime_fence(&state, expected)?;
    }
    let records =
        super::persona_device_view::read_policy_records(&super::managed_agents_store_path(app)?)?;
    let context = if super::restore::needs_auto_start_authority(&records) {
        super::restore::auto_start_context_result(&records, context_provider(app, &state))?
    } else {
        None
    };
    if let Some(context) = &context {
        super::device_creation::assert_creation_scope(&fence.scope, &context.scope)?;
    }
    super::device_runtime::assert_runtime_fence(&state, &fence)?;
    let mut candidates = super::restore::select_auto_start_candidates(&records, context.as_ref())?;
    hydrate(&mut candidates);
    Ok(communities
        .iter()
        .flat_map(|c| {
            candidates
                .iter()
                .map(move |r| (r.clone(), c.relay_url.clone()))
        })
        .collect())
}

struct RuntimeProbeInput<'a> {
    record: super::ManagedAgentRecord,
    requested: String,
    fence: &'a super::device_runtime::RuntimeFence,
}
/// Actual per-job reconcile probe adapter; scope and target policy bracket the await.
async fn probe_auto_start_job_with<R, F, Fut>(
    app: &AppHandle<R>,
    input: RuntimeProbeInput<'_>,
    mut context: impl FnMut(
        &AppHandle<R>,
        &AppState,
    ) -> Result<super::persona_device_view::DevicePolicyContext, String>,
    hydrate: impl FnOnce(&mut [super::ManagedAgentRecord]),
    probe: F,
) -> Result<(super::ManagedAgentRecord, ManagedAgentRuntimeKey, String), String>
where
    R: tauri::Runtime,
    F: FnOnce(super::ManagedAgentRecord, String) -> Fut,
    Fut: std::future::Future<
        Output = Result<(super::ManagedAgentRecord, ManagedAgentRuntimeKey, String), String>,
    >,
{
    let state = app.state::<AppState>();
    let fresh = {
        let _store = state
            .managed_agents_store_lock
            .lock()
            .map_err(|e| e.to_string())?;
        super::device_runtime::assert_runtime_fence(&state, input.fence)?;
        super::device_runtime::runtime_phase_locked_with(
            app,
            &state,
            &input.record.pubkey,
            Some(&input.fence.scope),
            &mut context,
            |mut record, _, _| {
                hydrate(std::slice::from_mut(&mut record));
                Ok(record)
            },
        )?
    };
    let result = probe(fresh, input.requested).await?;
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    super::device_runtime::assert_runtime_fence(&state, input.fence)?;
    super::device_runtime::runtime_phase_locked_with(
        app,
        &state,
        &input.record.pubkey,
        Some(&input.fence.scope),
        context,
        |_, _, _| Ok(()),
    )?;
    Ok(result)
}

type AutoStartProbe = Result<
    (super::ManagedAgentRecord, ManagedAgentRuntimeKey, String),
    (super::ManagedAgentRecord, String, String),
>;
async fn probe_auto_start_jobs<F, Fut>(
    jobs: Vec<(super::ManagedAgentRecord, String)>,
    probe: F,
) -> Vec<AutoStartProbe>
where
    F: Fn(super::ManagedAgentRecord, String) -> Fut,
    Fut: std::future::Future<
        Output = Result<(super::ManagedAgentRecord, ManagedAgentRuntimeKey, String), String>,
    >,
{
    use futures_util::{stream, StreamExt};
    stream::iter(jobs)
        .map(|(record, requested)| {
            let fallback = (record.clone(), requested.clone());
            let work = probe(record, requested);
            async move { work.await.map_err(|e| (fallback.0, fallback.1, e)) }
        })
        .buffer_unordered(6)
        .collect()
        .await
}

#[cfg(test)]
mod device_home_job_tests {
    use super::*;
    use crate::managed_agents::{
        definition_home::EvidenceReadiness,
        device_home_migration::tests::{app, context, records, write},
    };
    #[tokio::test]
    async fn copied_and_deferred_auto_start_jobs_have_zero_hydration_and_probes() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let base = super::super::managed_agents_base_dir(app.handle()).unwrap();
        let (mut rs, _) = records();
        rs[1].start_on_app_launch = true;
        let communities = vec![super::super::ManagedAgentCommunityTarget {
            relay_url: "wss://other-scope".into(),
        }];
        for copied in [false, true] {
            rs[1].device_host_binding = copied.then(|| "foreign-marker".into());
            write(&base, &rs);
            let jobs = auto_start_jobs_with(
                None,
                app.handle(),
                &communities,
                |_, state| {
                    let mut c = context(EvidenceReadiness::Pending);
                    c.scope = super::super::device_home_sync::capture_scope(state)?;
                    Ok(c)
                },
                |selected| assert!(selected.is_empty(), "blocked records hydrated keys"),
            )
            .unwrap();
            let probes =
                probe_auto_start_jobs(jobs, |_, _| async { panic!("blocked record probed relay") })
                    .await;
            assert!(probes.is_empty());
        }
    }
    #[tokio::test]
    async fn proven_fresh_jobs_retry_after_migration_and_shared_jobs_need_no_proof() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let base = super::super::managed_agents_base_dir(app.handle()).unwrap();
        let (mut rs, _) = records();
        rs[1].start_on_app_launch = true;
        let mut c = context(EvidenceReadiness::Ready);
        c.scope = super::super::device_home_sync::capture_scope(&app.state::<AppState>()).unwrap();
        rs[1].device_host_binding = Some(c.proof.binding().into());
        write(&base, &rs);
        let communities = vec![super::super::ManagedAgentCommunityTarget {
            relay_url: "wss://other-scope".into(),
        }];
        let jobs =
            auto_start_jobs_with(None, app.handle(), &communities, |_, _| Ok(c), |_| {}).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].1, "wss://other-scope");
        let count = std::cell::Cell::new(0);
        let results = probe_auto_start_jobs(jobs, |r, url| {
            count.set(count.get() + 1);
            async move {
                let key = ManagedAgentRuntimeKey::new(r.pubkey.clone(), &url)?;
                Ok((r, key, url))
            }
        })
        .await;
        assert_eq!(count.get(), 1);
        assert_eq!(results.len(), 1);
        rs[0].share_across_devices = Some(true);
        rs[1].device_host_binding = Some("foreign-marker".into());
        write(&base, &rs);
        let jobs = auto_start_jobs_with(
            None,
            app.handle(),
            &communities,
            |_, _| panic!("shared-only queried proof"),
            |_| {},
        )
        .unwrap();
        assert_eq!(jobs.len(), 1);
    }
    #[test]
    fn auto_start_structural_or_context_errors_propagate_before_keys() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let base = super::super::managed_agents_base_dir(app.handle()).unwrap();
        let (mut rs, _) = records();
        rs[1].start_on_app_launch = true;
        write(&base, &rs);
        assert!(auto_start_jobs_with(
            None,
            app.handle(),
            &[],
            |_, _| Err("proof locked".into()),
            |_| panic!("proof error hydrated keys")
        )
        .is_err());
        std::fs::write(base.join("managed-agents.json"), b"broken").unwrap();
        assert!(auto_start_jobs_with(
            None,
            app.handle(),
            &[],
            |_, _| panic!("broken structural store read context"),
            |_| panic!("broken store hydrated keys")
        )
        .is_err());
    }
    #[test]
    fn shared_jobs_ignore_unselected_private_authority() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let base = super::super::managed_agents_base_dir(app.handle()).unwrap();
        let (mut rs, _) = records();
        rs[1].start_on_app_launch = false;
        let mut shared = crate::managed_agents::device_home_migration::tests::definition();
        shared.id = "shared-one".into();
        shared.share_across_devices = Some(true);
        let mut instance = shared.clone().into_agent_record();
        instance.pubkey = nostr::Keys::generate().public_key().to_hex();
        instance.persona_id = Some(shared.id.clone());
        instance.start_on_app_launch = true;
        rs.push(shared.into_agent_record());
        rs.push(instance);
        write(&base, &rs);
        let communities = vec![super::super::ManagedAgentCommunityTarget {
            relay_url: "wss://test".into(),
        }];
        let jobs = auto_start_jobs_with(
            None,
            app.handle(),
            &communities,
            |_, _| panic!("unselected private row requested proof for shared jobs"),
            |selected| {
                assert_eq!(selected.len(), 1);
                assert_eq!(selected[0].persona_id.as_deref(), Some("shared-one"));
            },
        )
        .unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].0.persona_id.as_deref(), Some("shared-one"));
    }
}
#[cfg(test)]
mod mixed_runtime_tests {
    use super::*;
    use crate::managed_agents::device_home_migration::tests::{app, definition, records, write};
    #[test]
    fn mixed_selected_reconcile_keeps_shared_when_private_authority_is_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let base = super::super::managed_agents_base_dir(app.handle()).unwrap();
        let (mut raw, _) = records();
        raw[1].start_on_app_launch = true;
        let mut d = definition();
        d.id = "shared".into();
        d.share_across_devices = Some(true);
        let mut i = d.clone().into_agent_record();
        i.persona_id = Some(d.id.clone());
        i.pubkey = nostr::Keys::generate().public_key().to_hex();
        i.start_on_app_launch = true;
        let key = i.pubkey.clone();
        raw.push(d.into_agent_record());
        raw.push(i);
        write(&base, &raw);
        let jobs = auto_start_jobs_with(
            None,
            app.handle(),
            &[super::super::ManagedAgentCommunityTarget {
                relay_url: "wss://test".into(),
            }],
            |_, _| Err("injected proof unavailable".into()),
            |selected| {
                assert_eq!(selected.len(), 1);
                assert_eq!(selected[0].pubkey, key);
            },
        )
        .unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].0.pubkey, key);
    }
}

#[cfg(test)]
mod reconcile_callback_tests {
    use super::*;
    use crate::managed_agents::{
        definition_home::EvidenceReadiness,
        device_home_migration::tests::{app, context, records, write},
    };
    #[tokio::test]
    async fn restore_and_reconcile_skip_foreign_pairs_before_probe() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<AppState>();
        let (mut raw, _) = records();
        raw[1].device_host_binding = Some("copied".into());
        write(
            &super::super::managed_agents_base_dir(app.handle()).unwrap(),
            &raw,
        );
        let fence = super::super::device_runtime::capture_runtime_fence(&state).unwrap();
        let probes = std::cell::Cell::new(0);
        let result = probe_auto_start_job_with(
            app.handle(),
            RuntimeProbeInput {
                record: raw[1].clone(),
                requested: "wss://test".into(),
                fence: &fence,
            },
            |_, state| {
                let mut c = context(EvidenceReadiness::Ready);
                c.scope = super::super::device_home_sync::capture_scope(state)?;
                Ok(c)
            },
            |_| panic!("foreign job hydrated keys"),
            |_, _| async {
                probes.set(1);
                Err("transport was reached".into())
            },
        )
        .await;
        assert!(result.is_err());
        assert_eq!(probes.get(), 0);
    }
    #[tokio::test]
    async fn reconcile_shared_probe_result_cannot_start_after_scope_switch() {
        for change in 0..4 {
            let dir = tempfile::tempdir().unwrap();
            let app = app(dir.path());
            let state = app.state::<AppState>();
            let (mut raw, _) = records();
            raw[0].share_across_devices = Some(true);
            write(
                &super::super::managed_agents_base_dir(app.handle()).unwrap(),
                &raw,
            );
            let fence = super::super::device_runtime::capture_runtime_fence(&state).unwrap();
            let result = probe_auto_start_job_with(
                app.handle(),
                RuntimeProbeInput {
                    record: raw[1].clone(),
                    requested: "wss://test".into(),
                    fence: &fence,
                },
                |_, _| panic!("shared requires no proof"),
                |_| {},
                |r, requested| async {
                    match change {
                        0 => {
                            state
                                .workspace_apply_generation
                                .fetch_add(1, Ordering::AcqRel);
                        }
                        1 => {
                            *state.keys.lock().unwrap() = nostr::Keys::generate();
                        }
                        2 => {
                            *state.relay_url_override.lock().unwrap() =
                                Some("wss://switched".into());
                        }
                        _ => {
                            super::super::device_home_sync::begin_session(&state)?;
                        }
                    }
                    let key = ManagedAgentRuntimeKey::new(r.pubkey.clone(), &requested)?;
                    Ok((r, key, requested))
                },
            )
            .await;
            assert!(
                result.is_err(),
                "stale probe result reached final start loop"
            );
        }
    }

    #[tokio::test]
    async fn reconcile_probe_reloads_policy_after_transport() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<AppState>();
        let (mut raw, _) = records();
        raw[0].share_across_devices = Some(true);
        let base = super::super::managed_agents_base_dir(app.handle()).unwrap();
        write(&base, &raw);
        let fence = super::super::device_runtime::capture_runtime_fence(&state).unwrap();
        let result = probe_auto_start_job_with(
            app.handle(),
            RuntimeProbeInput {
                record: raw[1].clone(),
                requested: "wss://test".into(),
                fence: &fence,
            },
            |_, state| {
                let mut c = context(EvidenceReadiness::Ready);
                c.scope = super::super::device_home_sync::capture_scope(state)?;
                Ok(c)
            },
            |_| {},
            |r, requested| async {
                raw[0].share_across_devices = Some(false);
                raw[1].device_host_binding = Some("copied".into());
                write(&base, &raw);
                let key = ManagedAgentRuntimeKey::new(r.pubkey.clone(), &requested)?;
                Ok((r, key, requested))
            },
        )
        .await;
        assert!(result.is_err(), "changed policy reached final start loop");
    }
}
#[cfg(test)]
mod candidate_scope_tests {
    use super::*;
    use crate::managed_agents::device_home_migration::tests::{app, records, write};
    #[test]
    fn reconcile_authority_scope_switch_refuses_before_key_effects() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<AppState>();
        let (mut raw, _) = records();
        let mut c = super::super::device_home_migration::tests::context(
            super::super::definition_home::EvidenceReadiness::Ready,
        );
        c.scope = super::super::device_home_sync::capture_scope(&state).unwrap();
        raw[1].device_host_binding = Some(c.proof.binding().into());
        raw[1].start_on_app_launch = true;
        write(
            &super::super::managed_agents_base_dir(app.handle()).unwrap(),
            &raw,
        );
        let keys = std::cell::Cell::new(0);
        let result = auto_start_jobs_with(
            None,
            app.handle(),
            &[],
            |_, state| {
                state
                    .workspace_apply_generation
                    .fetch_add(1, Ordering::AcqRel);
                Ok(c)
            },
            |_| keys.set(1),
        );
        assert!(result.is_err(), "reconcile accepted stale authority");
        assert_eq!(keys.get(), 0);
    }
}
#[cfg(all(test, not(target_os = "windows")))]
#[path = "runtime_commands_admission_tests.rs"]
mod admission_tests;
