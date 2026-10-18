//! Exercise actual late snapshot shells through prompt completion, return and poll.
use super::*;

const SNAPSHOT: &str = include_str!("../process_tree/codex_bash_snapshot.txt");
const SERVER: &str = r#"
import json, os, pathlib, signal, subprocess, sys, time
root = pathlib.Path.cwd()
child = None
for line in sys.stdin:
    action = line.strip()
    if action == 'start':
        env = dict(os.environ, BASH_ENV=str(root / 'startup'))
        child = subprocess.Popen(['/bin/bash', '-lc', (root / 'snapshot').read_text()],
            env=env, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, text=True)
        deadline = time.monotonic() + 5
        while not (root / 'ready').exists():
            assert time.monotonic() < deadline, 'shell startup did not reach barrier'
            assert child.poll() is None, 'shell exited before barrier'
            time.sleep(.01)
    elif action == 'stop':
        child.stdin.write('\n')
        child.stdin.flush()
        child.wait(timeout=5)
    elif action == 'advance':
        child.stdin.write('\n')
        child.stdin.flush()
        deadline = time.monotonic() + 5
        while not (root / 'changed').exists():
            assert time.monotonic() < deadline, 'snapshot did not exec ordinary work'
            time.sleep(.01)
    elif action == 'stop-child':
        os.kill(int((root / 'child').read_text()), signal.SIGTERM)
    print('ready', flush=True)
"#;

const ADAPTER: &str = r#"
import json, pathlib, subprocess, sys
server = subprocess.Popen([sys.argv[1], 'app-server'], cwd=sys.argv[2], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
def action(value):
    server.stdin.write(value + '\n')
    server.stdin.flush()
    assert server.stdout.readline().strip() == 'ready'
for line in sys.stdin:
    req = json.loads(line)
    method = req['method']
    if method == 'initialize':
        result = {'protocolVersion': 1, 'agentCapabilities': {}}
    elif method == 'session/new':
        if sys.argv[3] == 'setup': action('start')
        result = {'sessionId': 'snapshot-session'}
    elif method == 'session/prompt':
        action(req['params']['prompt'][0]['text'])
        result = {'stopReason': 'end_turn'}
    else:
        continue
    print(json.dumps({'jsonrpc': '2.0', 'id': req['id'], 'result': result}), flush=True)
"#;

async fn snapshot_worker(
    dir: &tempfile::TempDir,
    with_child: bool,
    during_setup: bool,
) -> OwnedAgent {
    configured_worker(dir, with_child, during_setup, SNAPSHOT, "codex").await
}

async fn configured_worker(
    dir: &tempfile::TempDir,
    with_child: bool,
    during_setup: bool,
    script: &str,
    parent: &str,
) -> OwnedAgent {
    use std::os::unix::fs::PermissionsExt;
    let python = std::process::Command::new("python3")
        .args(["-c", "import sys; print(sys.executable)"])
        .output()
        .unwrap();
    assert!(python.status.success());
    let python = String::from_utf8(python.stdout).unwrap();
    let executable = dir.path().join(if parent == "symlink" || parent == "exec" {
        "codex"
    } else {
        parent
    });
    if parent == "symlink" {
        std::os::unix::fs::symlink(python.trim(), &executable).unwrap();
    } else {
        std::fs::copy(python.trim(), &executable).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    std::fs::write(dir.path().join("app-server"), SERVER).unwrap();
    std::fs::write(dir.path().join("snapshot"), script).unwrap();
    let startup = if parent == "exec" {
        "trap 'unset BASH_ENV; exec /bin/bash -c \"printf changed > changed; read -r gate\"' EXIT\nprintf ready > ready\nread -r gate\n"
    } else if with_child {
        "sleep 60 &\nprintf '%s' $! > child\nprintf ready > ready\nread -r gate\n"
    } else {
        "printf ready > ready\nread -r gate\n"
    };
    std::fs::write(dir.path().join("startup"), startup).unwrap();
    let mut agent = worker(0).await;
    agent.acp = AcpClient::spawn(
        "python3",
        &[
            "-u".into(),
            "-c".into(),
            ADAPTER.into(),
            executable.to_string_lossy().into_owned(),
            dir.path().to_string_lossy().into_owned(),
            if during_setup { "setup" } else { "late" }.into(),
        ],
        &[],
        false,
    )
    .await
    .unwrap();
    agent.acp.initialize().await.unwrap();
    agent
        .acp
        .session_new("/tmp", vec![], None, None)
        .await
        .unwrap();
    agent
}

async fn action(agent: &mut OwnedAgent, value: &str) {
    agent
        .acp
        .session_prompt_with_idle_timeout(
            "snapshot-session",
            value,
            Duration::from_secs(10),
            Duration::from_secs(15),
        )
        .await
        .unwrap();
}

fn remember(pool: &mut AgentPool, original: &SessionScope) -> OwnedAgent {
    bind(pool, original, 0);
    let mut agent = pool.try_claim(Some(original)).unwrap();
    agent
        .state
        .sessions
        .insert(original.clone(), "snapshot-session".into());
    agent.remember_background_context(
        &PromptSource::Channel(original.clone()),
        None,
        "snapshot-session",
    );
    agent
}

#[tokio::test]
async fn late_codex_snapshot_exit_does_not_enqueue_background_collection() {
    let dir = tempfile::tempdir().unwrap();
    let agent = snapshot_worker(&dir, false, false).await;
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    let original = scope();
    let mut agent = remember(&mut pool, &original);
    action(&mut agent, "start").await;
    let _ = agent.background_work_running(&PromptSource::Channel(original.clone()));
    pool.return_agent(agent);
    action(pool.agents[0].as_mut().unwrap(), "stop").await;
    let mut queue = EventQueue::new(DedupMode::Queue);
    pool.queue_finished_background_work(&mut queue);
    let invented = queue.has_pending_continuation(&original);
    let retained = pool.has_runtime_hold();
    pool.agents[0].as_mut().unwrap().acp.shutdown().await;
    assert!(
        !invented,
        "provider snapshot exit is not a user work completion"
    );
    assert!(!retained, "snapshot bookkeeping must release the worker");
}

#[tokio::test]
async fn codex_snapshot_child_work_survives_late_and_session_capture() {
    for during_setup in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let agent = snapshot_worker(&dir, true, during_setup).await;
        let mut pool = AgentPool::from_slots(vec![Some(agent)]);
        let original = scope();
        let mut agent = remember(&mut pool, &original);
        if !during_setup {
            action(&mut agent, "start").await;
        }
        // Startup capture must also avoid adopting ordinary descendants of a snapshot.
        agent.acp.capture_startup_children();
        assert!(agent.background_work_running(&PromptSource::Channel(original.clone())));
        pool.return_agent(agent);
        let mut queue = EventQueue::new(DedupMode::Queue);
        pool.queue_finished_background_work(&mut queue);
        assert!(!queue.has_pending_continuation(&original));
        assert!(pool.try_claim(Some(&scope())).is_none());
        action(pool.agents[0].as_mut().unwrap(), "stop-child").await;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        while !queue.has_pending_continuation(&original) {
            pool.queue_finished_background_work(&mut queue);
            assert!(
                tokio::time::Instant::now() < deadline,
                "child exit must collect while snapshot remains alive"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let batch = queue.flush_next().unwrap();
        assert_eq!(batch.scope, original);
        assert_eq!(batch.continuation.unwrap().session_id, "snapshot-session");
        assert!(queue.flush_next().is_none());
        action(pool.agents[0].as_mut().unwrap(), "stop").await;
        pool.agents[0].as_mut().unwrap().acp.shutdown().await;
    }
}

#[tokio::test]
async fn ordinary_snapshot_lookalikes_still_collect_actual_work() {
    let suffix = format!("{SNAPSHOT}\n: ordinary_work\n");
    let changed = SNAPSHOT.replace("declare -f", "declare -f; : ordinary_work");
    for (script, parent) in [
        ("printf '%s' '# Snapshot file'\n", "codex"),
        (suffix.as_str(), "codex"),
        (changed.as_str(), "codex"),
        (SNAPSHOT, "ordinary-worker"),
        (SNAPSHOT, "symlink"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let agent = configured_worker(&dir, false, false, script, parent).await;
        let mut pool = AgentPool::from_slots(vec![Some(agent)]);
        let original = scope();
        let mut agent = remember(&mut pool, &original);
        action(&mut agent, "start").await;
        let running = agent.background_work_running(&PromptSource::Channel(original.clone()));
        pool.return_agent(agent);
        let retained = pool.has_runtime_hold();
        action(pool.agents[0].as_mut().unwrap(), "stop").await;
        let mut queue = EventQueue::new(DedupMode::Queue);
        pool.queue_finished_background_work(&mut queue);
        pool.queue_finished_background_work(&mut queue);
        let batch = queue.flush_next();
        let duplicate = queue.flush_next();
        pool.agents[0].as_mut().unwrap().acp.shutdown().await;
        assert!(
            running && retained,
            "ordinary shell must retain its original worker: {parent}"
        );
        let batch = batch.expect("ordinary shell exit must collect");
        assert_eq!(batch.scope, original);
        assert_eq!(batch.continuation.unwrap().session_id, "snapshot-session");
        assert!(duplicate.is_none());
    }
}

#[tokio::test]
async fn snapshot_pid_exec_into_ordinary_work_is_not_a_cached_resident() {
    let dir = tempfile::tempdir().unwrap();
    let mut agent = configured_worker(&dir, false, false, SNAPSHOT, "exec").await;
    action(&mut agent, "start").await;
    let before = agent.acp.background_child_pids().unwrap();
    agent.acp.capture_startup_children();
    action(&mut agent, "advance").await;
    let after = agent.acp.background_child_pids().unwrap();
    action(&mut agent, "stop").await;
    agent.acp.shutdown().await;
    assert!(before.is_empty(), "snapshot starts as bookkeeping");
    assert_eq!(
        after.len(),
        1,
        "same PID with ordinary argv must become work"
    );
}

#[path = "snapshot_uncertainty_tests.rs"]
mod uncertainty;
