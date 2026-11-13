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

#[tokio::test]
async fn sleep_inhibitor_retains_internal_work_and_collects_once() {
    use crate::process_tree::ProcessTree;

    for inhibitor in [
        "990002 990001 S /usr/bin/caffeinate -is\n",
        "990002 990001 S systemd-inhibit --what=sleep --who=Qwen Code --why=Working --mode=block sleep infinity\n990003 990002 S /usr/bin/sleep infinity\n",
        "990002 990001 S systemd-inhibit --no-ask-password --what=sleep --who=Qwen Code --why=Working --mode=block sleep infinity\n990003 990002 S /usr/bin/sleep infinity\n",
    ] {
        for (work, inhibitor_parent, expected) in [
            ("", "990001", HashSet::new()),
            (
                "990004 990001 S python3 task.py\n",
                "990001",
                HashSet::from([990004]),
            ),
            (
                "990004 990001 S /bin/bash job.sh\n",
                "990004",
                HashSet::from([990004]),
            ),
            (
                "990004 990001 S python3 task.py\n",
                "990004",
                HashSet::from([990004]),
            ),
            (
                "990004 990001 S /bin/sh launcher.sh\n990005 990004 S python3 task.py\n",
                "990005",
                HashSet::from([990004]),
            ),
        ] {
            let real_work = !expected.is_empty();
            // The inhibitor can be the only OS child of an internal subagent.
            // Nested inhibitors remain covered by their working ancestor.
            let expected = if inhibitor_parent == "990001" {
                expected.union(&HashSet::from([990002])).copied().collect()
            } else {
                expected
            };
            let original = scope();
            let mut pool = AgentPool::from_slots(vec![Some(worker(0).await)]);
            bind(&mut pool, &original, 0);
            let mut agent = pool.try_claim(Some(&original)).unwrap();
            let worker = agent.acp.process_id().unwrap();
            let provider = format!("990001 {worker} S node provider.js\n");
            agent.acp.process_tree_snapshots.push_back(Some(
                ProcessTree::from_snapshot_with_identities(&provider, None),
            ));
            agent.acp.capture_startup_children();
            let session = agent.state.sessions[&original].clone();
            let source = PromptSource::Channel(original.clone());
            agent.remember_background_context(&source, None, &session);
            let inhibitor =
                inhibitor.replacen("990002 990001", &format!("990002 {inhibitor_parent}"), 1);
            let after_exec = if real_work {
                "990004 990001 R /usr/bin/shasum -a 256 /dev/zero\n".to_owned()
            } else {
                inhibitor.clone()
            };
            for rows in [
                format!("{provider}{inhibitor}{work}"),
                format!("{provider}{inhibitor}{work}"),
                format!("{provider}{after_exec}"),
                format!("{provider}{after_exec}"),
                provider.clone(),
                provider.clone(),
            ] {
                agent.acp.process_tree_snapshots.push_back(Some(
                    ProcessTree::from_snapshot_with_identities(&rows, None),
                ));
            }
            let running = agent.background_work_running(&source);
            assert_eq!(
                agent.background_context.as_ref().unwrap().blocking_pids,
                expected,
                "{inhibitor}{work}"
            );
            agent.prepare_background_return();
            pool.return_agent(agent);
            let held_before_exit = pool.has_runtime_hold();
            let mut queue = EventQueue::new(DedupMode::Queue);
            pool.queue_finished_background_work(&mut queue);
            pool.queue_finished_background_work(&mut queue);
            assert!(
                !queue.has_pending_continuation(&original),
                "collection must wait for the exec'd work to exit: {inhibitor}{work}"
            );
            assert!(pool.has_runtime_hold());
            assert_eq!(
                pool.runtime_holds[&0].blocking_pids,
                HashSet::from([if real_work { 990004 } else { 990002 }])
            );
            assert!(pool.try_claim(Some(&scope())).is_none());
            pool.queue_finished_background_work(&mut queue);
            pool.queue_finished_background_work(&mut queue);
            let batch = queue.flush_next();
            let duplicate = queue.flush_next();
            let held_after_exit = pool.has_runtime_hold();
            pool.agents[0].as_mut().unwrap().acp.shutdown().await;
            assert!(running, "{inhibitor}");
            assert!(held_before_exit, "{inhibitor}");
            assert!(batch.is_some(), "{inhibitor}");
            assert!(held_after_exit, "{inhibitor}");
            assert!(duplicate.is_none());
            if let Some(batch) = batch {
                assert_eq!(batch.scope, original);
                assert_eq!(batch.continuation.unwrap().session_id, session);
            }
        }
    }
}

/// A model request runs in a thread of the adapter, with only its shared
/// inhibitor visible as an OS child after the foreground prompt returns.
#[cfg(target_os = "macos")]
const INTERNAL_WORK_ADAPTER: &str = r#"
import json, os, subprocess, sys, threading, time
inhibitor = None
def internal_work():
    while not os.path.exists(os.path.join(sys.argv[1], 'finish')):
        time.sleep(.01)
    with open(os.path.join(sys.argv[1], 'result'), 'w') as result:
        result.write('internal work completed')
    inhibitor.terminate()
    inhibitor.wait()
for line in sys.stdin:
    req = json.loads(line)
    with open(os.path.join(sys.argv[1], 'requests'), 'a') as log:
        log.write(line)
    if req['method'] == 'initialize':
        result = {'protocolVersion': 2, 'agentCapabilities': {}}
    elif req['method'] == 'session/new':
        result = {'sessionId': 'helper-session'}
    elif req['method'] == 'session/prompt':
        if inhibitor is None:
            inhibitor = subprocess.Popen(['/usr/bin/caffeinate', '-is'])
            threading.Thread(target=internal_work, daemon=True).start()
        else:
            assert open(os.path.join(sys.argv[1], 'result')).read() == 'internal work completed'
        result = {'stopReason': 'end_turn'}
    else:
        continue
    print(json.dumps({'jsonrpc': '2.0', 'id': req['id'], 'result': result}), flush=True)
"#;

#[cfg(target_os = "macos")]
#[tokio::test]
async fn internal_work_with_only_caffeinate_survives_idle_deadline_and_collects() {
    let dir = tempfile::tempdir().unwrap();
    let acp = AcpClient::spawn(
        "python3",
        &[
            "-u".into(),
            "-c".into(),
            INTERNAL_WORK_ADAPTER.into(),
            dir.path().to_string_lossy().into_owned(),
        ],
        &[],
        false,
    )
    .await
    .unwrap();
    let mut agent = owned_worker(0, acp);
    agent.acp.initialize().await.unwrap();
    agent.acp.capture_startup_children();
    agent
        .acp
        .session_new("/tmp", vec![], None, None)
        .await
        .unwrap();
    let original = scope();
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    bind(&mut pool, &original, 0);
    pool.agents[0]
        .as_mut()
        .unwrap()
        .state
        .sessions
        .insert(original.clone(), "helper-session".into());
    let mut agent = pool.try_claim(Some(&original)).unwrap();
    agent.remember_background_context(
        &PromptSource::Channel(original.clone()),
        None,
        "helper-session",
    );
    helper_prompt(&mut agent, "start").await;
    // No fake process snapshot: the production scanner must see the native
    // inhibitor even though the actual task has no OS process of its own.
    agent.prepare_background_return();
    pool.return_agent(agent);
    let last = tokio::time::Instant::now();
    assert!(
        !crate::idle_pool_sleep_due(
            true,
            last,
            last + Duration::from_secs(901),
            Duration::from_secs(900),
            false,
            false,
            false,
            false,
            pool.has_runtime_hold(),
        ),
        "idle shutdown must not terminate an internal request after end_turn"
    );
    assert!(pool.try_claim(Some(&scope())).is_none());
    assert!(!dir.path().join("result").exists());
    let mut queue = EventQueue::new(DedupMode::Queue);
    let requests = pool.background_poll_requests();
    let tree = crate::process_tree::snapshot().unwrap();
    pool.apply_background_snapshot(&requests, Some(&tree), &mut queue);
    assert!(!queue.has_pending_continuation(&original));
    finish_child(&mut pool, &mut queue, &dir).await;
    let batch = queue.flush_next().expect("collect completed internal work");
    assert_eq!(batch.scope, original);
    assert_eq!(
        batch.continuation.as_ref().unwrap().session_id,
        "helper-session"
    );
    assert!(queue.flush_next().is_none());
    let mut agent = pool.try_claim(Some(&original)).unwrap();
    helper_prompt(&mut agent, "collect").await;
    agent.finish_background_collection();
    agent.prepare_background_return();
    pool.return_agent(agent);
    assert!(crate::idle_pool_sleep_due(
        true,
        last,
        last + Duration::from_secs(901),
        Duration::from_secs(900),
        false,
        false,
        false,
        false,
        pool.has_runtime_hold(),
    ));
    let log = std::fs::read_to_string(dir.path().join("requests")).unwrap();
    let prompts: Vec<serde_json::Value> = log
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .filter(|request| request["method"] == "session/prompt")
        .collect();
    assert_eq!(prompts.len(), 2);
    assert!(prompts
        .iter()
        .all(|request| request["params"]["sessionId"] == "helper-session"));
    pool.agents[0].as_mut().unwrap().acp.shutdown().await;
}
