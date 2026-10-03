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

fn spawned_result(record: &super::super::ManagedAgentRecord) -> AgentSpawnResult {
    // Controlled child exits immediately; lifecycle boundaries remain injectable
    // without leaving a real agent/process behind when a RED assertion fails.
    #[cfg(unix)]
    let child = {
        use std::os::unix::process::CommandExt;
        std::process::Command::new("/usr/bin/true")
            .process_group(0)
            .spawn()
            .unwrap()
    };
    #[cfg(windows)]
    let child = std::process::Command::new("cmd")
        .args(["/C", "exit", "0"])
        .spawn()
        .unwrap();
    let key =
        super::super::ManagedAgentRuntimeKey::new(&record.pubkey, "wss://relay.example").unwrap();
    let process = ManagedAgentProcess {
        child,
        log_path: Default::default(),
        spawn_config: super::super::spawn_snapshot::prospective_spawn_config_snapshot(
            record,
            &[],
            &[],
            &key.relay_url,
            &Default::default(),
            false,
        ),
        setup_mode: false,
        adapter_availability: None,
        start_nonce: "isolated-restore-test".into(),
        #[cfg(windows)]
        job: None,
    };
    (
        record.pubkey.clone(),
        SpawnOutcome::Spawned(key, Box::new(process)),
    )
}

#[test]
fn post_spawn_authority_error_settles_all_owned_children() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let base = super::super::managed_agents_base_dir(app.handle()).unwrap();
    let (raw, _) = mixed_records();
    write(&base, &raw);
    let before = std::fs::read(base.join("managed-agents.json")).unwrap();
    let cleaned = RefCell::new(Vec::new());
    let results = vec![spawned_result(&raw[1]), spawned_result(&raw[3])];
    let expected: Vec<_> = results
        .iter()
        .filter_map(|(_, outcome)| match outcome {
            SpawnOutcome::Spawned(_, child) => Some(child.child.id()),
            _ => None,
        })
        .collect();
    let error = child_ownership::complete_restore_spawn_results_with(
        app.handle(),
        results,
        |_, _| Err("injected post-spawn authority error".into()),
        |_| panic!("failed authority reached hydration"),
        |_| panic!("failed authority reached persistence"),
        |process| {
            cleaned.borrow_mut().push(process.child.id());
            process.child.wait().unwrap();
            Ok(())
        },
    )
    .err()
    .unwrap();
    assert!(error.contains("post-spawn authority error"));
    assert_eq!(
        *cleaned.borrow(),
        expected,
        "completion discarded owned children"
    );
    assert_eq!(
        before,
        std::fs::read(base.join("managed-agents.json")).unwrap()
    );
    let state = app.state::<AppState>();
    assert!(state.managed_agent_processes.lock().unwrap().is_empty());
    assert!(state
        .managed_agent_restore_cleanup
        .0
        .lock()
        .unwrap()
        .is_empty());
}

#[test]
fn fresh_authority_rejection_cleans_only_new_rejected_child() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let base = super::super::managed_agents_base_dir(app.handle()).unwrap();
    let (raw, allowed) = mixed_records();
    write(&base, &raw);
    let state = app.state::<AppState>();
    let mut tracked = raw[3].clone();
    tracked.pubkey = nostr::Keys::generate().public_key().to_hex();
    let (_, SpawnOutcome::Spawned(tracked_key, tracked_child)) = spawned_result(&tracked) else {
        unreachable!()
    };
    let tracked_pid = tracked_child.child.id();
    state.managed_agent_processes.lock().unwrap().insert(
        tracked_key.clone(),
        super::super::ManagedAgentPairRuntime::starting(*tracked_child),
    );
    let rejected = spawned_result(&raw[1]);
    let SpawnOutcome::Spawned(_, rejected_child) = &rejected.1 else {
        unreachable!()
    };
    let rejected_pid = rejected_child.child.id();
    let cleaned = RefCell::new(Vec::new());
    let secrets = Secrets::default();
    let items = child_ownership::complete_restore_spawn_results_with(
        app.handle(),
        vec![
            rejected,
            spawned_result(&raw[3]),
            (tracked.pubkey.clone(), SpawnOutcome::Skipped),
        ],
        |_, _| Ok(context(EvidenceReadiness::Ready)),
        |rs| hydrate_keys_with(&secrets, rs),
        |rs| persist_agent_keys_with(&secrets, rs),
        |process| {
            cleaned.borrow_mut().push(process.child.id());
            process.child.wait().unwrap();
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(*cleaned.borrow(), vec![rejected_pid]);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].0, allowed);
    assert_preserved(&base, &raw, &secrets);
    let mut runtimes = state.managed_agent_processes.lock().unwrap();
    assert_eq!(runtimes.len(), 2);
    assert_eq!(runtimes.get(&tracked_key).unwrap().child.id(), tracked_pid);
    assert!(runtimes.keys().any(|key| key.pubkey == allowed));
    for runtime in runtimes.values_mut() {
        runtime.child.wait().unwrap();
    }
}

#[test]
fn failed_child_cleanup_propagates_and_retains_retry_ownership() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let base = super::super::managed_agents_base_dir(app.handle()).unwrap();
    let (raw, _) = mixed_records();
    write(&base, &raw);
    let result = child_ownership::complete_restore_spawn_results_with(
        app.handle(),
        vec![spawned_result(&raw[1])],
        |_, _| Ok(context(EvidenceReadiness::Ready)),
        |rs| assert!(rs.is_empty(), "rejected child reached hydration"),
        |_| {},
        |_| Err("injected termination failure".into()),
    );
    let state = app.state::<AppState>();
    let owned = state.managed_agent_restore_cleanup.0.lock().unwrap().len();
    let retry_failure = child_ownership::retry_restore_cleanup_with(app.handle(), |_| {
        Err("injected retry failure".into())
    });
    let retained_after_retry = state.managed_agent_restore_cleanup.0.lock().unwrap().len();
    let retry = child_ownership::retry_restore_cleanup(app.handle());
    assert!(result.is_err(), "failed cleanup reported success");
    assert!(result
        .err()
        .unwrap()
        .contains("injected termination failure"));
    assert_eq!(owned, 1, "unconfirmed child lost reachable ownership");
    assert!(retry_failure
        .unwrap_err()
        .contains("injected retry failure"));
    assert_eq!(retained_after_retry, 1, "failed retry discarded ownership");
    retry.unwrap();
    assert!(state
        .managed_agent_restore_cleanup
        .0
        .lock()
        .unwrap()
        .is_empty());
}

#[test]
fn receipt_failure_settles_unregistered_child_and_preserves_error() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let base = super::super::managed_agents_base_dir(app.handle()).unwrap();
    let (raw, allowed) = mixed_records();
    write(&base, &raw);
    std::fs::write(base.join("agent-pids"), b"block receipt directory").unwrap();
    let cleaned = RefCell::new(0);
    let secrets = Secrets::default();
    child_ownership::complete_restore_spawn_results_with(
        app.handle(),
        vec![spawned_result(&raw[3])],
        |_, _| panic!("shared target requested authority"),
        |rs| hydrate_keys_with(&secrets, rs),
        |rs| persist_agent_keys_with(&secrets, rs),
        |process| {
            *cleaned.borrow_mut() += 1;
            process.child.wait().unwrap();
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(*cleaned.borrow(), 1, "receipt failure discarded the child");
    let state = app.state::<AppState>();
    assert!(state.managed_agent_processes.lock().unwrap().is_empty());
    assert!(state
        .managed_agent_restore_cleanup
        .0
        .lock()
        .unwrap()
        .is_empty());
    assert_preserved(&base, &raw, &secrets);
    let after = read_policy_records(&base.join("managed-agents.json")).unwrap();
    assert!(after
        .iter()
        .find(|r| r.pubkey == allowed)
        .unwrap()
        .last_error
        .as_ref()
        .unwrap()
        .contains("agent-pids"));
}

#[test]
fn new_child_collision_never_replaces_previously_tracked_child() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let base = super::super::managed_agents_base_dir(app.handle()).unwrap();
    let (raw, _) = mixed_records();
    write(&base, &raw);
    let (_, SpawnOutcome::Spawned(key, tracked)) = spawned_result(&raw[3]) else {
        unreachable!()
    };
    let tracked_pid = tracked.child.id();
    let state = app.state::<AppState>();
    state.managed_agent_processes.lock().unwrap().insert(
        key.clone(),
        super::super::ManagedAgentPairRuntime::starting(*tracked),
    );
    let new = spawned_result(&raw[3]);
    let SpawnOutcome::Spawned(_, child) = &new.1 else {
        unreachable!()
    };
    let new_pid = child.child.id();
    let cleaned = RefCell::new(Vec::new());
    let secrets = Secrets::default();
    let items = child_ownership::complete_restore_spawn_results_with(
        app.handle(),
        vec![new],
        |_, _| panic!("shared target requested authority"),
        |rs| hydrate_keys_with(&secrets, rs),
        |rs| persist_agent_keys_with(&secrets, rs),
        |process| {
            cleaned.borrow_mut().push(process.child.id());
            process.child.wait().unwrap();
            Ok(())
        },
    )
    .unwrap();
    assert!(items.is_empty());
    assert_eq!(*cleaned.borrow(), vec![new_pid]);
    let mut runtimes = state.managed_agent_processes.lock().unwrap();
    assert_eq!(runtimes.len(), 1);
    let tracked = runtimes.get_mut(&key).unwrap();
    assert_eq!(tracked.child.id(), tracked_pid);
    tracked.child.wait().unwrap();
    assert_preserved(&base, &raw, &secrets);
}

#[cfg(unix)]
#[test]
fn restore_child_exit_confirmation_is_bounded_and_tree_is_reaped() {
    use std::os::unix::process::CommandExt;
    let (raw, _) = mixed_records();
    let (_, SpawnOutcome::Spawned(_, mut process)) = spawned_result(&raw[3]) else {
        unreachable!()
    };
    process.child.wait().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("nested-pid");
    process.child = std::process::Command::new("/bin/sh")
        .args(["-c", "trap 'kill \"$nested\" 2>/dev/null; wait \"$nested\"; exit 0' TERM; /bin/sleep 60 & nested=$!; printf '%s' \"$nested\" > \"$1\"; wait \"$nested\"", "restore-tree-test"])
        .arg(&pid_file).process_group(0).spawn().unwrap();
    let ready_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let nested_pid = loop {
        if let Some(pid) = std::fs::read_to_string(&pid_file)
            .ok()
            .and_then(|text| text.parse::<u32>().ok())
        {
            break Some(pid);
        }
        if std::time::Instant::now() >= ready_deadline {
            break None;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    let start = std::time::Instant::now();
    let unconfirmed =
        child_ownership::wait_for_restore_child_exit(&mut process, std::time::Duration::ZERO);
    let cleanup = child_ownership::terminate_restore_child(&mut process);
    let elapsed = start.elapsed();
    let reaped_before_release = !super::super::process_is_running(process.child.id());
    let reaped = process.child.try_wait().unwrap().is_some();
    // Always settle this controlled tree before assertions, including mutations
    // that falsely report cleanup success without confirming the leader's exit.
    if cleanup.is_err() || !reaped {
        let _ = super::super::terminate_process(process.child.id());
        let _ = process.child.kill();
        let _ = process.child.wait();
    }
    assert!(unconfirmed.unwrap_err().contains("timed out reaping"));
    cleanup.unwrap();
    assert!(elapsed < std::time::Duration::from_secs(3));
    assert!(
        reaped_before_release,
        "production released ownership before reaping leader"
    );
    assert!(reaped, "leader was not reaped before ownership release");
    let nested_pid = nested_pid.expect("isolated nested child readiness");
    assert!(
        !super::super::process_is_running(nested_pid),
        "descendant survived group cleanup"
    );
}
