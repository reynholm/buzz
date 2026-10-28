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
            host.stdin.write('python3 -c \'import time; print("work-ready", flush=True); time.sleep(60)\' & echo $!\n')
            host.stdin.flush()
            # Ack from the child after exec: a freshly forked shell can still
            # have the helper executable identity before becoming real work.
            started = {host.stdout.readline().strip(), host.stdout.readline().strip()}
            assert 'work-ready' in started
            child = int(next(value for value in started if value != 'work-ready'))
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
    // Linux confirms the kernel executable path, so a symlink to bash would
    // correctly remain ordinary work. Copy the executable for that fixture.
    #[cfg(target_os = "linux")]
    std::fs::copy("/bin/bash", &path).unwrap();
    #[cfg(target_os = "macos")]
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
    agent.prepare_background_return();
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

#[cfg(target_os = "linux")]
#[tokio::test]
async fn symlink_named_codex_host_does_not_hide_ordinary_work() {
    let dir = tempfile::tempdir().unwrap();
    let executable = dir.path().join("codex-code-mode-host");
    std::os::unix::fs::symlink("/bin/bash", &executable).unwrap();
    let mut agent = helper_worker(0, &executable).await;
    helper_prompt(&mut agent, "idle").await;
    assert_eq!(agent.acp.background_child_pids().unwrap().len(), 1);
    let mut pool = AgentPool::from_slots(vec![None]);
    return_after_turn(&mut pool, agent, scope());
    assert!(pool.try_claim(Some(&scope())).is_none());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn unlinked_codex_host_does_not_retain_finished_turn() {
    let dir = tempfile::tempdir().unwrap();
    let executable = helper_executable(&dir);
    let mut agent = helper_worker(0, &executable).await;
    helper_prompt(&mut agent, "idle").await;
    std::fs::remove_file(&executable).unwrap();
    let mut pool = AgentPool::from_slots(vec![None]);
    return_after_turn(&mut pool, agent, scope());
    assert!(pool.try_claim(Some(&scope())).is_some());
}

#[tokio::test]
async fn child_of_codex_host_queues_original_collection_before_releasing_slot() {
    let dir = tempfile::tempdir().unwrap();
    let executable = helper_executable(&dir);
    let agent = helper_worker(0, &executable).await;
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    let original = scope();
    bind(&mut pool, &original, 0);
    let mut agent = pool.try_claim(Some(&original)).unwrap();
    agent
        .state
        .sessions
        .insert(original.clone(), "helper-session".into());
    helper_prompt(&mut agent, "work").await;
    let generation = agent.state.scope_owner_generations[&original];
    let delivery_key = Uuid::new_v4();
    agent.background_context = Some(RuntimeHold {
        scope: original.clone(),
        blocking_pids: HashSet::new(),
        collection_pending: false,
        continuation: Some(crate::queue::RuntimeContinuation {
            work: None,
            crash_recovery: false,
            background_ready: true,
            delivery_key,
            ordinary_preempted: false,
            worker_index: 0,
            session_id: "helper-session".into(),
            generation,
            thread_tags: ThreadTags {
                root_event_id: Some("a".repeat(64)),
                parent_event_id: Some("b".repeat(64)),
                ..Default::default()
            },
            received_at: std::time::Instant::now(),
        }),
    });
    agent.prepare_background_return();
    pool.return_agent(agent);
    let mut queue = EventQueue::new(DedupMode::Queue);
    pool.queue_finished_background_work(&mut queue);
    assert!(!queue.has_pending_continuation(&original));
    assert!(pool.try_claim(Some(&scope())).is_none());
    helper_prompt(pool.agents[0].as_mut().unwrap(), "stop").await;
    pool.queue_finished_background_work(&mut queue);
    let batch = queue.flush_next().expect("child exit queues collection");
    assert_eq!(batch.scope, original);
    let continuation = batch.continuation.unwrap();
    assert_eq!(continuation.delivery_key, delivery_key);
    assert_eq!(continuation.session_id, "helper-session");
    assert_eq!(continuation.generation, generation);
    assert_eq!(
        continuation.thread_tags.parent_event_id,
        Some("b".repeat(64))
    );
    assert!(pool.try_claim(Some(&scope())).is_none());
    let mut agent = pool.try_claim(Some(&original)).unwrap();
    agent.finish_background_collection();
    queue.mark_complete(&original);
    agent.prepare_background_return();
    pool.return_agent(agent);
    assert!(pool.try_claim(Some(&scope())).is_some());
}
