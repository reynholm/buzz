//! Reproduce late Codex service residency through real ACP and pool return.
use super::*;

const ADAPTER: &str = r#"
import json, subprocess, sys
host = None
child = None
for line in sys.stdin:
    req = json.loads(line)
    method = req['method']
    if method == 'initialize':
        result = {'protocolVersion': 1, 'agentCapabilities': {}}
    elif method == 'session/new':
        result = {'sessionId': 'helper-session'}
    elif method == 'session/prompt':
        if host is None:
            host = subprocess.Popen([sys.argv[1]], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
            host.stdin.write('echo ready\n')
            host.stdin.flush()
            assert host.stdout.readline().strip() == 'ready'
        text = req['params']['prompt'][0]['text']
        if text == 'work':
            host.stdin.write('sleep 60 & echo $!\n')
            host.stdin.flush()
            child = int(host.stdout.readline())
        elif text == 'argument':
            child = subprocess.Popen(['/bin/bash', '-c', 'read -r line', sys.argv[1]], stdin=subprocess.PIPE)
        elif text == 'stop':
            host.stdin.write('kill %d; wait %d; echo stopped\n' % (child, child))
            host.stdin.flush()
            assert host.stdout.readline().strip() == 'stopped'
        result = {'stopReason': 'end_turn'}
    else:
        continue
    print(json.dumps({'jsonrpc': '2.0', 'id': req['id'], 'result': result}), flush=True)
"#;

async fn helper_worker(index: usize, executable: &std::path::Path) -> OwnedAgent {
    let mut agent = worker(index).await;
    agent.acp = AcpClient::spawn(
        "python3",
        &[
            "-u".into(),
            "-c".into(),
            ADAPTER.into(),
            executable.to_string_lossy().into_owned(),
        ],
        &[],
        false,
    )
    .await
    .unwrap();
    agent.acp.initialize().await.unwrap();
    agent.acp.capture_startup_children();
    agent
        .acp
        .session_new("/tmp", vec![], None, None)
        .await
        .unwrap();
    agent
}

fn helper_executable(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let path = dir
        .path()
        .join("Application  Support")
        .join("codex-code-mode-host");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    // Launch a real executable under this name; no argv-only impersonation.
    std::os::unix::fs::symlink("/bin/bash", &path).unwrap();
    path
}

async fn helper_prompt(agent: &mut OwnedAgent, text: &str) {
    assert_eq!(
        agent
            .acp
            .session_prompt_with_idle_timeout(
                "helper-session",
                text,
                Duration::from_secs(2),
                Duration::from_secs(5)
            )
            .await
            .unwrap(),
        StopReason::EndTurn,
    );
}

fn return_after_turn(pool: &mut AgentPool, mut agent: OwnedAgent, scope: SessionScope) {
    agent.background_context = Some(RuntimeHold {
        scope,
        blocking_pids: HashSet::new(),
        continuation: None,
        collection_pending: false,
    });
    pool.return_agent(agent);
}

#[tokio::test]
async fn six_finished_threads_reuse_five_slots_with_late_codex_hosts() {
    let dir = tempfile::tempdir().unwrap();
    let executable = helper_executable(&dir);
    let mut slots = Vec::new();
    for index in 0..5 {
        slots.push(Some(helper_worker(index, &executable).await));
    }
    let mut pool = AgentPool::from_slots(slots);
    for turn in 0..6 {
        let scope = scope();
        let mut agent = pool.try_claim(Some(&scope)).unwrap_or_else(|| {
            panic!(
                "finished threads exhausted the pool before turn {}",
                turn + 1
            )
        });
        helper_prompt(&mut agent, "idle").await;
        return_after_turn(&mut pool, agent, scope);
    }
    assert!(
        !pool.has_runtime_hold(),
        "service processes must not retain completed turns"
    );
}

#[tokio::test]
async fn work_under_late_codex_host_holds_slot_until_child_exits() {
    let dir = tempfile::tempdir().unwrap();
    let executable = helper_executable(&dir);
    let agent = helper_worker(0, &executable).await;
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    let original = scope();
    let mut agent = pool.try_claim(Some(&original)).unwrap();
    helper_prompt(&mut agent, "work").await;
    return_after_turn(&mut pool, agent, original);
    assert!(
        pool.try_claim(Some(&scope())).is_none(),
        "real child must retain the slot"
    );
    let agent = pool.agents[0].as_mut().unwrap();
    helper_prompt(agent, "stop").await;
    pool.queue_finished_background_work(&mut EventQueue::new(DedupMode::Queue));
    assert!(
        pool.try_claim(Some(&scope())).is_some(),
        "child exit must release the slot while host stays alive"
    );
}

#[tokio::test]
async fn work_mentioning_codex_host_in_arguments_still_holds_slot() {
    let dir = tempfile::tempdir().unwrap();
    let executable = helper_executable(&dir);
    let mut agent = helper_worker(0, &executable).await;
    helper_prompt(&mut agent, "argument").await;
    let blocking = agent.acp.background_child_pids().unwrap();
    assert_eq!(
        blocking.len(),
        1,
        "ignore the host, but retain shell work mentioning its path"
    );
    let mut pool = AgentPool::from_slots(vec![None]);
    return_after_turn(&mut pool, agent, scope());
    assert!(pool.try_claim(Some(&scope())).is_none());
}
