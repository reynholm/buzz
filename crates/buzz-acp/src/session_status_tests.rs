use super::*;
use crate::acp::AcpClient;
use buzz_core::session_status::{
    canonical_identity, generation_lock_path, parse_snapshot, scope_file_name, StatusEnvelope,
    GENERATIONS_DIR, INSTANCE_LOCK_FILE,
};
use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

fn identity() -> StatusIdentity {
    canonical_identity(
        "wss://relay.example",
        &nostr::Keys::generate().public_key().to_hex(),
    )
    .unwrap()
}

fn thread(channel: u128, root: char) -> StatusScope {
    StatusScope {
        channel_id: Uuid::from_u128(channel),
        thread_root: Some(root.to_string().repeat(64)),
    }
}

fn conversation(channel: u128) -> StatusScope {
    StatusScope {
        channel_id: Uuid::from_u128(channel),
        thread_root: None,
    }
}

fn open_runtime() -> (tempfile::TempDir, SessionStatusRuntime) {
    let root = tempfile::tempdir().unwrap();
    let runtime = SessionStatusRuntime::open(root.path(), identity()).unwrap();
    (root, runtime)
}

fn begin(runtime: &SessionStatusRuntime, scope: &StatusScope, sid: &str) -> StatusBinding {
    runtime
        .begin(
            scope.clone(),
            sid,
            SessionState::New,
            StatusReason::FirstInThread,
        )
        .expect("status binding")
}

fn snapshot_path(runtime: &SessionStatusRuntime, scope: &StatusScope) -> PathBuf {
    runtime.dir().join(scope_file_name(scope).unwrap())
}

fn snapshot(runtime: &SessionStatusRuntime, scope: &StatusScope) -> Option<StatusEnvelope> {
    let bytes = fs::read(snapshot_path(runtime, scope)).ok()?;
    Some(parse_snapshot(&bytes).expect("valid snapshot"))
}

fn generation_lock(runtime: &SessionStatusRuntime, envelope: &StatusEnvelope) -> PathBuf {
    runtime
        .dir()
        .join(generation_lock_path(envelope.generation))
}

/// Whether another open file description cannot take the lock right now.
fn lock_is_held(path: &Path) -> bool {
    let Ok(file) = OpenOptions::new().read(true).open(path) else {
        return false;
    };
    match file.try_lock_exclusive() {
        Ok(()) => {
            FileExt::unlock(&file).unwrap();
            false
        }
        Err(_) => true,
    }
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn set_mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

// ---- layout and locks ----

#[test]
fn display_tags_follow_current_generation_and_reject_revoked_clones() {
    let (_root, runtime) = open_runtime();
    let scope = thread(1, 'a');
    let binding = begin(&runtime, &scope, "old");
    let clone = binding.clone();
    binding.record_occupancy(95, 100);
    assert_eq!(
        binding.event_tags().unwrap(),
        snapshot(&runtime, &scope).unwrap().event_tags().unwrap()
    );
    let next = begin(&runtime, &scope, "next");
    assert!(binding.event_tags().is_none());
    assert!(clone.event_tags().is_none());
    assert_eq!(next.event_tags().unwrap()[0][3], "");
    runtime.retire(&scope);
    assert!(next.event_tags().is_none());
}

#[test]
fn open_holds_instance_lock_in_private_layout() {
    let root = tempfile::tempdir().unwrap();
    let runtime = SessionStatusRuntime::open(root.path(), identity()).unwrap();
    let dir = runtime.dir().to_path_buf();
    let instance: Uuid = dir.file_name().unwrap().to_str().unwrap().parse().unwrap();
    let identity_dir = dir.parent().unwrap();
    let name = identity_dir.file_name().unwrap().to_str().unwrap();
    assert_eq!(name.len(), 64);
    assert!(name.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(
        identity_dir.parent().unwrap(),
        root.path().join("status").join("v1")
    );
    assert_eq!(mode(&dir), 0o700);
    assert_eq!(mode(&dir.join(GENERATIONS_DIR)), 0o700);
    assert_eq!(mode(identity_dir), 0o700);
    assert!(lock_is_held(&dir.join(INSTANCE_LOCK_FILE)));
    let binding = begin(&runtime, &thread(1, 'a'), "s1");
    assert_eq!(
        snapshot(&runtime, &thread(1, 'a')).unwrap().instance_id,
        instance
    );
    drop(binding);
    drop(runtime);
    assert!(
        !dir.exists(),
        "a stopped runtime removes its instance directory"
    );
}

#[test]
fn begin_publishes_private_snapshot_under_held_generation_lock() {
    let (_root, runtime) = open_runtime();
    let scope = thread(1, 'a');
    let binding = runtime
        .begin(
            scope.clone(),
            "s1",
            SessionState::Resumed,
            StatusReason::IdleSleep,
        )
        .unwrap();
    let envelope = snapshot(&runtime, &scope).unwrap();
    assert_eq!(envelope.scope, scope);
    assert_eq!(envelope.status.state, SessionState::Resumed);
    assert_eq!(envelope.status.reason, StatusReason::IdleSleep);
    assert_eq!(envelope.status.occupancy, None, "never borrowed, never 0%");
    assert_eq!(envelope.status.history, None);
    assert_eq!(mode(&snapshot_path(&runtime, &scope)), 0o600);
    assert!(lock_is_held(&generation_lock(&runtime, &envelope)));
    assert!(binding.is_live());
    assert_eq!(binding.session_id(), "s1");
}

#[test]
fn invalid_scope_or_reason_publishes_nothing() {
    let (_root, runtime) = open_runtime();
    let upper = StatusScope {
        channel_id: Uuid::from_u128(1),
        thread_root: Some("AB".repeat(32)),
    };
    assert!(runtime
        .begin(upper, "s1", SessionState::New, StatusReason::FirstInThread)
        .is_none());
    assert!(runtime
        .begin(
            thread(1, 'a'),
            "s1",
            SessionState::New,
            StatusReason::ResumeFailed("Raw adapter error: /tmp".into()),
        )
        .is_none());
    let files: Vec<_> = fs::read_dir(runtime.dir())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name())
        .collect();
    assert_eq!(files.len(), 2, "only the lock and generations: {files:?}");
}

#[test]
fn occupancy_and_history_update_the_snapshot() {
    let (_root, runtime) = open_runtime();
    let scope = conversation(2);
    let binding = begin(&runtime, &scope, "s1");
    let transition = snapshot(&runtime, &scope).unwrap().status.transition_id;
    binding.record_occupancy(64, 100);
    let history = HistoryCoverage {
        read: Some(3),
        total: Some(9),
        exact: false,
        incomplete: true,
    };
    binding.set_history(history);
    let status = snapshot(&runtime, &scope).unwrap().status;
    assert_eq!(status.occupancy.and_then(|o| o.percent()), Some(64));
    assert_eq!(status.history, Some(history));
    assert_eq!(status.transition_id, transition);
    binding.record_occupancy(5, 0);
    assert_eq!(
        snapshot(&runtime, &scope)
            .unwrap()
            .status
            .occupancy
            .and_then(|o| o.percent()),
        Some(64),
        "a zero context size is unavailable, not a new value"
    );
}

#[test]
fn successor_generation_revokes_predecessor() {
    let (_root, runtime) = open_runtime();
    let scope = thread(1, 'a');
    let first = begin(&runtime, &scope, "s1");
    first.record_occupancy(64, 100);
    let old = snapshot(&runtime, &scope).unwrap();
    let second = runtime
        .begin(
            scope.clone(),
            "s2",
            SessionState::Resumed,
            StatusReason::WorkerCrash,
        )
        .unwrap();
    let new = snapshot(&runtime, &scope).unwrap();
    assert_ne!(old.generation, new.generation);
    assert_ne!(old.status.transition_id, new.status.transition_id);
    assert_eq!(new.status.occupancy, None, "the successor starts unknown");
    assert!(!lock_is_held(&generation_lock(&runtime, &old)));
    assert!(lock_is_held(&generation_lock(&runtime, &new)));
    assert!(!first.is_live());
    first.record_occupancy(90, 100);
    first.retire();
    assert_eq!(snapshot(&runtime, &scope).unwrap(), new);
    assert!(second.is_live());
}

#[test]
fn retire_releases_lock_and_late_callbacks_never_recreate() {
    let (_root, runtime) = open_runtime();
    let scope = thread(1, 'a');
    let binding = begin(&runtime, &scope, "s1");
    let clone = binding.clone();
    let envelope = snapshot(&runtime, &scope).unwrap();
    runtime.retire(&scope);
    assert!(snapshot(&runtime, &scope).is_none());
    assert!(!lock_is_held(&generation_lock(&runtime, &envelope)));
    clone.record_occupancy(10, 100);
    clone.set_history(HistoryCoverage {
        read: Some(1),
        total: None,
        exact: false,
        incomplete: false,
    });
    assert!(snapshot(&runtime, &scope).is_none());
    assert!(!binding.is_live());
    assert!(runtime.bind_existing(&scope, "s1").is_none());
}

#[test]
fn failed_snapshot_removal_still_revokes_generation() {
    let (_root, runtime) = open_runtime();
    let scope = thread(1, 'a');
    let neighbor = thread(1, 'b');
    let binding = begin(&runtime, &scope, "s1");
    let late = binding.clone();
    let other = begin(&runtime, &neighbor, "s2");
    let envelope = snapshot(&runtime, &scope).unwrap();
    let before = fs::read(snapshot_path(&runtime, &scope)).unwrap();
    let neighbor_before = fs::read(snapshot_path(&runtime, &neighbor)).unwrap();
    let probe = File::open(generation_lock(&runtime, &envelope)).unwrap();
    assert!(probe.try_lock_exclusive().is_err(), "held while current");

    set_mode(runtime.dir(), 0o500);
    binding.retire();
    set_mode(runtime.dir(), 0o700);

    probe
        .try_lock_exclusive()
        .expect("the generation lock is released although removal failed");
    FileExt::unlock(&probe).unwrap();
    assert_eq!(
        fs::read(snapshot_path(&runtime, &scope)).unwrap(),
        before,
        "the remaining snapshot names a released generation"
    );
    late.record_occupancy(99, 100);
    late.set_history(HistoryCoverage {
        read: Some(1),
        total: Some(1),
        exact: true,
        incomplete: false,
    });
    assert_eq!(fs::read(snapshot_path(&runtime, &scope)).unwrap(), before);
    assert!(runtime.bind_existing(&scope, "s1").is_none());
    assert_eq!(
        fs::read(snapshot_path(&runtime, &neighbor)).unwrap(),
        neighbor_before
    );
    let neighbor_lock = generation_lock(&runtime, &snapshot(&runtime, &neighbor).unwrap());
    assert!(lock_is_held(&neighbor_lock), "successors are untouched");
    other.record_occupancy(10, 100);
    assert_eq!(
        snapshot(&runtime, &neighbor)
            .unwrap()
            .status
            .occupancy
            .and_then(|o| o.percent()),
        Some(10)
    );
}

#[test]
fn write_failure_disables_only_that_scope() {
    let (_root, runtime) = open_runtime();
    let failing = thread(1, 'a');
    let healthy = thread(1, 'b');
    let binding = begin(&runtime, &failing, "s1");
    let other = begin(&runtime, &healthy, "s2");
    let envelope = snapshot(&runtime, &failing).unwrap();

    set_mode(runtime.dir(), 0o500);
    binding.record_occupancy(64, 100);
    set_mode(runtime.dir(), 0o700);

    assert!(!binding.is_live());
    assert!(!lock_is_held(&generation_lock(&runtime, &envelope)));
    assert!(
        runtime
            .begin(
                failing.clone(),
                "s3",
                SessionState::New,
                StatusReason::Discarded
            )
            .is_none(),
        "a failed scope stops producing status for this runtime"
    );
    binding.record_occupancy(70, 100);
    if let Some(left) = snapshot(&runtime, &failing) {
        assert_eq!(left.generation, envelope.generation, "never rewritten");
        assert_eq!(left.status.occupancy, None);
    }
    other.record_occupancy(81, 100);
    assert_eq!(
        snapshot(&runtime, &healthy)
            .unwrap()
            .status
            .occupancy
            .and_then(|o| o.percent()),
        Some(81)
    );
}

#[test]
fn retire_waits_for_a_delayed_writer_then_removes_snapshot() {
    let (_root, runtime) = open_runtime();
    let scope = thread(1, 'a');
    let binding = begin(&runtime, &scope, "s1");
    let envelope = snapshot(&runtime, &scope).unwrap();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    runtime.delay_next_write(Box::new(move || {
        entered_tx.send(()).unwrap();
        release_rx.recv().unwrap();
    }));
    let writer = {
        let binding = binding.clone();
        std::thread::spawn(move || binding.record_occupancy(50, 100))
    };
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let retire = {
        let runtime = runtime.clone();
        let scope = scope.clone();
        std::thread::spawn(move || runtime.retire(&scope))
    };
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        !retire.is_finished(),
        "retirement waits for the in-flight write"
    );
    release_tx.send(()).unwrap();
    writer.join().unwrap();
    retire.join().unwrap();
    assert!(snapshot(&runtime, &scope).is_none());
    assert!(!lock_is_held(&generation_lock(&runtime, &envelope)));
    binding.record_occupancy(60, 100);
    assert!(snapshot(&runtime, &scope).is_none());
}

#[test]
fn bind_existing_keeps_transition_for_the_same_session_only() {
    let (_root, runtime) = open_runtime();
    let scope = thread(1, 'a');
    let binding = begin(&runtime, &scope, "s1");
    let again = runtime.bind_existing(&scope, "s1").expect("same session");
    again.record_occupancy(12, 100);
    let status = snapshot(&runtime, &scope).unwrap().status;
    assert_eq!(status.occupancy.and_then(|o| o.percent()), Some(12));
    assert!(runtime.bind_existing(&scope, "other").is_none());
    assert!(runtime.bind_existing(&thread(1, 'b'), "s1").is_none());
    binding.retire();
    assert!(!again.is_live());
}

#[test]
fn retire_channel_leaves_other_channels() {
    let (_root, runtime) = open_runtime();
    let removed = [thread(1, 'a'), conversation(1)];
    let kept = thread(2, 'a');
    let _bindings: Vec<_> = removed
        .iter()
        .chain([&kept])
        .enumerate()
        .map(|(n, scope)| begin(&runtime, scope, &format!("s{n}")))
        .collect();
    runtime.retire_channel(Uuid::from_u128(1));
    for scope in &removed {
        assert!(snapshot(&runtime, scope).is_none());
    }
    assert!(snapshot(&runtime, &kept).is_some());
}

#[test]
fn open_sweeps_dead_instances_only() {
    let root = tempfile::tempdir().unwrap();
    let identity = identity();
    let live = SessionStatusRuntime::open(root.path(), identity.clone()).unwrap();
    let parent = live.dir().parent().unwrap().to_path_buf();
    let dead = parent.join(Uuid::new_v4().to_string());
    fs::create_dir_all(dead.join(GENERATIONS_DIR)).unwrap();
    File::create(dead.join(INSTANCE_LOCK_FILE)).unwrap();
    let next = SessionStatusRuntime::open(root.path(), identity).unwrap();
    assert!(!dead.exists(), "a dead instance directory is swept");
    assert!(live.dir().exists(), "a live instance directory is kept");
    assert_ne!(live.dir(), next.dir());
}

#[test]
fn status_dir_env_is_owned_by_the_harness() {
    let env_of = |extra: &[(String, String)]| {
        let mut cmd = tokio::process::Command::new("true");
        apply_status_dir_env(&mut cmd, extra);
        cmd.as_std()
            .get_envs()
            .find(|(name, _)| *name == SESSION_STATUS_DIR_ENV)
            .map(|(_, value)| value.map(|v| v.to_str().unwrap().to_owned()))
    };
    assert_eq!(
        env_of(&[(SESSION_STATUS_DIR_ENV.into(), "/status/current".into())]),
        Some(Some("/status/current".into()))
    );
    assert_eq!(
        env_of(&[]),
        Some(None),
        "an inherited directory of another harness is removed"
    );
}

// ---- usage notifications through the real AcpClient dispatcher ----

/// Spawn a fake adapter named `name` (which selects the adapter kind) that
/// answers one `session/prompt` after emitting `notifications`.
async fn adapter(
    name: &str,
    notifications: &[serde_json::Value],
) -> (AcpClient, tempfile::TempDir) {
    adapter_with_stop(name, notifications, "end_turn").await
}

async fn adapter_with_stop(
    name: &str,
    notifications: &[serde_json::Value],
    stop: &str,
) -> (AcpClient, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    let mut script = String::from("#!/usr/bin/env bash\nread -r REQ\n");
    script.push_str(r#"ID=$(printf '%s' "$REQ" | sed -E 's/.*"id":([0-9]+).*/\1/')"#);
    script.push('\n');
    for notification in notifications {
        script.push_str(&format!("echo '{notification}'\n"));
    }
    script.push_str(&format!(
        r#"echo '{{"jsonrpc":"2.0","id":'"$ID"',"result":{{"stopReason":"{stop}"}}}}'"#
    ));
    script.push_str("\nsleep 1\n");
    fs::write(&path, script).unwrap();
    set_mode(&path, 0o755);
    let client = AcpClient::spawn(path.to_str().unwrap(), &[], &[], false)
        .await
        .unwrap();
    (client, dir)
}

fn usage(
    session: Option<&str>,
    used: serde_json::Value,
    size: serde_json::Value,
) -> serde_json::Value {
    let mut params = serde_json::json!({
        "update": {
            "sessionUpdate": "usage_update",
            "used": used,
            "size": size,
        }
    });
    if let Some(session) = session {
        params["sessionId"] = serde_json::json!(session);
    }
    serde_json::json!({"jsonrpc": "2.0", "method": "session/update", "params": params})
}

fn with_cost(mut update: serde_json::Value, amount: f64) -> serde_json::Value {
    update["params"]["update"]["cost"] = serde_json::json!({"amount": amount, "currency": "USD"});
    update
}

fn percent(runtime: &SessionStatusRuntime, scope: &StatusScope) -> Option<u8> {
    snapshot(runtime, scope)?.status.occupancy?.percent()
}

async fn interleaved_updates(
    name: &str,
) -> (
    SessionStatusRuntime,
    Option<crate::usage::TurnUsage>,
    tempfile::TempDir,
) {
    use serde_json::json;
    let (root, runtime) = open_runtime();
    let (a, b, retired) = (thread(1, 'a'), thread(1, 'b'), thread(1, 'c'));
    let notifications = [
        with_cost(usage(Some("sess-a"), json!(64), json!(100)), 0.5),
        usage(Some("sess-b"), json!(81), json!(100)),
        usage(Some("sess-a"), json!(70), json!(100)),
        usage(None, json!(99), json!(100)),
        usage(Some("sess-unknown"), json!(99), json!(100)),
        usage(Some("sess-b"), json!(-1), json!(100)),
        usage(Some("sess-b"), json!(5), json!(0)),
        usage(Some("sess-b"), json!("7"), json!(100)),
        usage(Some("sess-b"), json!(1.5), json!(100)),
        usage(Some("sess-c"), json!(50), json!(100)),
    ];
    let (mut client, dir) = adapter(name, &notifications).await;
    client.bind_session_status("sess-a", begin(&runtime, &a, "sess-a"));
    client.bind_session_status("sess-b", begin(&runtime, &b, "sess-b"));
    let late = begin(&runtime, &retired, "sess-c");
    client.bind_session_status("sess-c", late.clone());
    runtime.retire(&retired);
    client.notify_session_spawned("sess-a");
    client
        .session_prompt_with_idle_timeout(
            "sess-a",
            "hello",
            Duration::from_secs(5),
            Duration::from_secs(10),
        )
        .await
        .unwrap();
    let usage = client.take_turn_usage();
    client.shutdown().await;
    drop(dir);
    assert_eq!(
        percent(&runtime, &a),
        (name != "claude-code").then_some(70),
        "{name}: Claude requires an authoritative SDK result"
    );
    assert_eq!(
        percent(&runtime, &b),
        (name != "claude-code").then_some(81),
        "{name}: late A never modifies B"
    );
    assert!(
        snapshot(&runtime, &retired).is_none(),
        "{name}: a retired generation is not recreated"
    );
    let files = fs::read_dir(runtime.dir()).unwrap().count();
    assert_eq!(files, 4, "{name}: lock, generations and two snapshots");
    (runtime, usage, root)
}

#[tokio::test]
async fn claude_usage_updates_route_by_their_own_session_id() {
    let (_runtime, usage, _root) = interleaved_updates("claude-code").await;
    let usage = usage.expect("claude cost is still accounted");
    assert_eq!(usage.turn_cost_usd, Some(0.5));
    assert_eq!(usage.cumulative_cost_usd, Some(0.5));
    assert_eq!(
        usage.turn_input_tokens, None,
        "occupancy is not token usage"
    );
}

#[tokio::test]
async fn codex_usage_updates_route_by_their_own_session_id() {
    let (_runtime, usage, _root) = interleaved_updates("codex-acp").await;
    assert_eq!(
        usage.and_then(|usage| usage.cumulative_cost_usd),
        None,
        "codex cost stays ignored"
    );
}

fn claude_result(session: &str, windows: &[u64]) -> serde_json::Value {
    use serde_json::json;
    let models: serde_json::Map<String, serde_json::Value> = windows
        .iter()
        .enumerate()
        .map(|(i, size)| (format!("alias-{i}"), json!({"contextWindow":size})))
        .collect();
    json!({"jsonrpc":"2.0", "method":"_claude/sdkMessage", "params":{
        "sessionId":session, "message":{"type":"result", "subtype":"success",
        "is_error":false, "num_turns":1, "modelUsage":models,
        "result":"PRIVATE_RAW_RESULT_MARKER"}}})
}

async fn claude_context_case(notifications: &[serde_json::Value]) -> (Option<u8>, Option<u8>) {
    let (root, runtime) = open_runtime();
    let (a, b) = (thread(2, 'a'), thread(2, 'b'));
    let (mut client, dir) = adapter("claude-code", notifications).await;
    let observer = crate::observer::ObserverHandle::in_process();
    client.set_observer(Some(observer.clone()), 0);
    client.bind_session_status("sess-a", begin(&runtime, &a, "sess-a"));
    client.bind_session_status("sess-b", begin(&runtime, &b, "sess-b"));
    client
        .session_prompt_with_idle_timeout(
            "sess-a",
            "hello",
            Duration::from_secs(5),
            Duration::from_secs(10),
        )
        .await
        .unwrap();
    let actual = (percent(&runtime, &a), percent(&runtime, &b));
    let events = serde_json::to_string(&observer.snapshot()).unwrap();
    assert!(
        !events.contains("PRIVATE_RAW_RESULT_MARKER"),
        "SDK result text stays out of observer frames"
    );
    assert!(
        !events.contains("_claude/sdkMessage"),
        "private SDK envelope stays out of observer frames"
    );
    client.shutdown().await;
    drop((dir, root));
    actual
}

#[tokio::test]
async fn claude_context_uses_provider_window_and_the_final_context_numerator() {
    use serde_json::json;
    for (used, capacity, expected) in [
        (92_519, 1_000_000, 9),
        (259_397, 1_000_000, 25),
        (260_000, 1_000_000, 26),
        (92_519, 200_000, 46),
    ] {
        let notifications = [
            usage(Some("sess-a"), json!(used - 1), json!(200_000)),
            claude_result("sess-a", &[capacity]),
            usage(Some("sess-a"), json!(used), json!(200_000)),
        ];
        assert_eq!(
            claude_context_case(&notifications).await,
            (Some(expected), None)
        );
    }
}

#[tokio::test]
async fn claude_context_stays_unknown_without_a_matching_unambiguous_result() {
    use serde_json::json;
    let stream = usage(Some("sess-a"), json!(259_397), json!(200_000));
    for result in [
        claude_result("sess-a", &[200_000, 1_000_000]),
        claude_result("sess-b", &[1_000_000]),
        claude_result("sess-retired", &[1_000_000]),
    ] {
        let notifications = [stream.clone(), result, stream.clone()];
        assert_eq!(claude_context_case(&notifications).await, (None, None));
    }
    assert_eq!(claude_context_case(&[stream]).await, (None, None));
}

#[tokio::test]
async fn claude_context_error_zero_turn_and_malformed_results_cannot_publish_capacity() {
    use serde_json::json;
    for (key, value) in [
        ("is_error", json!(true)),
        ("num_turns", json!(0)),
        ("modelUsage", json!({})),
        ("modelUsage", json!({"model":{"contextWindow":0}})),
        ("modelUsage", json!({"model":{"contextWindow":"1000000"}})),
    ] {
        let mut result = claude_result("sess-a", &[1_000_000]);
        result["params"]["message"][key] = value;
        let stream = usage(Some("sess-a"), json!(259_397), json!(200_000));
        assert_eq!(
            claude_context_case(&[stream.clone(), result, stream]).await,
            (None, None)
        );
    }
}

#[tokio::test]
async fn claude_cancelled_prompt_clears_result_capacity() {
    use serde_json::json;
    let (root, runtime) = open_runtime();
    let scope = thread(3, 'a');
    let notifications = [
        usage(Some("sess-a"), json!(90_000), json!(200_000)),
        claude_result("sess-a", &[1_000_000]),
        usage(Some("sess-a"), json!(92_519), json!(200_000)),
    ];
    let (mut client, dir) = adapter_with_stop("claude-code", &notifications, "cancelled").await;
    client.bind_session_status("sess-a", begin(&runtime, &scope, "sess-a"));
    let stop = client
        .session_prompt_with_idle_timeout(
            "sess-a",
            "hello",
            Duration::from_secs(5),
            Duration::from_secs(10),
        )
        .await
        .unwrap();
    assert_eq!(stop, crate::acp::StopReason::Cancelled);
    assert_eq!(percent(&runtime, &scope), None);
    client.shutdown().await;
    drop((dir, root));
}

/// Pause the provider before the second result. The snapshot read at this
/// boundary is the input a tool-published kind-9 answer would freeze in tags.
async fn claude_second_turn_before_result(updates: &[serde_json::Value]) -> Option<u8> {
    use serde_json::json;
    let (root, runtime) = open_runtime();
    let scope = thread(4, 'a');
    let dir = tempfile::tempdir().unwrap();
    let gate = dir.path().join("continue");
    let path = dir.path().join("claude-code");
    let mut script = String::from("#!/usr/bin/env bash\n");
    let read_request = r#"read -r REQ
ID=$(printf '%s' "$REQ" | sed -E 's/.*"id":([0-9]+).*/\1/')
"#;
    let respond = r#"echo '{"jsonrpc":"2.0","id":'"$ID"',"result":{"stopReason":"end_turn"}}'
"#;
    script.push_str(read_request);
    for frame in [
        usage(Some("sess-a"), json!(90_000), json!(200_000)),
        claude_result("sess-a", &[1_000_000]),
        usage(Some("sess-a"), json!(92_519), json!(200_000)),
    ] {
        script.push_str(&format!("echo '{frame}'\n"));
    }
    script.push_str(respond);
    script.push_str(read_request);
    for frame in updates {
        script.push_str(&format!("echo '{frame}'\n"));
    }
    let ready = json!({"jsonrpc":"2.0", "method":"session/update", "params":{
        "sessionId":"sess-a", "update":{"sessionUpdate":"tool_call",
        "toolCallId":"second-publish", "title":"Publishing", "kind":"other"}}});
    script.push_str(&format!("echo '{ready}'\n"));
    script.push_str("for ((i=0; i<300; i++)); do\n  [[ -f \"$CLAUDE_CONTEXT_TEST_GATE\" ]] && break\n  sleep 0.01\ndone\n");
    script.push_str(respond);
    script.push_str("sleep 1\n");
    fs::write(&path, script).unwrap();
    set_mode(&path, 0o755);
    let mut client = AcpClient::spawn(
        path.to_str().unwrap(),
        &[],
        &[(
            "CLAUDE_CONTEXT_TEST_GATE".into(),
            gate.to_str().unwrap().into(),
        )],
        false,
    )
    .await
    .unwrap();
    let observer = crate::observer::ObserverHandle::in_process();
    client.set_observer(Some(observer.clone()), 0);
    client.bind_session_status("sess-a", begin(&runtime, &scope, "sess-a"));
    client
        .session_prompt_with_idle_timeout(
            "sess-a",
            "first",
            Duration::from_secs(5),
            Duration::from_secs(10),
        )
        .await
        .unwrap();
    assert_eq!(percent(&runtime, &scope), Some(9));
    let check = async {
        let mut at_publication = None;
        for _ in 0..200 {
            if observer.snapshot().iter().any(|event| {
                event
                    .payload
                    .pointer("/params/update/toolCallId")
                    .and_then(serde_json::Value::as_str)
                    == Some("second-publish")
            }) {
                at_publication = Some(percent(&runtime, &scope));
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        fs::write(&gate, b"continue").unwrap();
        at_publication
    };
    let (turn, at_publication) = tokio::join!(
        client.session_prompt_with_idle_timeout(
            "sess-a",
            "second",
            Duration::from_secs(5),
            Duration::from_secs(10)
        ),
        check
    );
    turn.unwrap();
    client.shutdown().await;
    drop((dir, root));
    at_publication.expect("the second tool-publication boundary was observed")
}

#[tokio::test]
async fn claude_confirmed_capacity_survives_an_ordinary_prompt_for_tool_published_answers() {
    use serde_json::json;
    assert_eq!(
        claude_second_turn_before_result(&[usage(Some("sess-a"), json!(259_397), json!(200_000))])
            .await,
        Some(25)
    );
}

#[tokio::test]
async fn claude_model_change_and_ambiguous_result_clear_prior_confirmed_capacity() {
    use serde_json::json;
    let changed = json!({"jsonrpc":"2.0", "method":"session/update", "params":{
        "sessionId":"sess-a", "update":{"sessionUpdate":"current_model_update",
        "currentModelId":"different-alias"}}});
    let stream = usage(Some("sess-a"), json!(259_397), json!(200_000));
    assert_eq!(
        claude_second_turn_before_result(&[changed, stream.clone()]).await,
        None
    );
    assert_eq!(
        claude_second_turn_before_result(&[
            stream.clone(),
            claude_result("sess-a", &[1_000_000, 200_000]),
            stream
        ])
        .await,
        None
    );
}
