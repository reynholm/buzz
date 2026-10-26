//! Inject only OS census/probe results; exercise production ACP adoption and hold/poll.
use super::*;
use crate::process_tree::ProcessTree;

const SHELL: u32 = 990001;
const WORK: u32 = 990002;

fn census(worker: u32, parent: u32, identities: Option<&str>, child: bool) -> ProcessTree {
    let script = SNAPSHOT.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut rows = format!("{parent} {worker} S /fixture/codex app-server\n{SHELL} {parent} S /bin/bash -lc {script}\n");
    if child {
        rows.push_str(&format!(
            "{WORK} {SHELL} S /usr/bin/python3 ordinary-task.py\n"
        ));
    }
    ProcessTree::from_snapshot_with_identities(&rows, identities)
}

fn empty(worker: u32, parent: u32) -> ProcessTree {
    ProcessTree::from_snapshot_with_identities(
        &format!("{parent} {worker} S /fixture/codex app-server\n"),
        None,
    )
}

fn parent(agent: &OwnedAgent) -> (u32, u32) {
    let worker = agent.acp.process_id().unwrap();
    let tree = crate::process_tree::snapshot().unwrap();
    let server = tree
        .children(worker)
        .iter()
        .find(|child| child.command.ends_with(" app-server"))
        .unwrap()
        .pid;
    (worker, server)
}

#[tokio::test]
async fn partial_snapshot_identity_does_not_invent_collection_but_preserves_observed_work() {
    for prior_work in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let agent = snapshot_worker(&dir, false, false).await;
        let (worker, server) = parent(&agent);
        let mut pool = AgentPool::from_slots(vec![Some(agent)]);
        let original = scope();
        let mut agent = remember(&mut pool, &original);
        if prior_work {
            agent.acp.process_tree_snapshots.push_back(Some(ProcessTree::from_snapshot_with_identities(
                &format!("{server} {worker} S /fixture/codex app-server\n{WORK} {server} S /usr/bin/python3 task.py\n"), None)));
            assert!(agent.background_work_running(&PromptSource::Channel(original.clone())));
        }
        // Exact observed race: parent is identified; Bash disappeared between census/probe.
        agent.acp.process_tree_snapshots.extend([
            Some(census(
                worker,
                server,
                Some(&format!("{server} /fixture/codex\n")),
                false,
            )),
            None,
            Some(empty(worker, server)),
            Some(empty(worker, server)),
        ]);
        let _ = agent.background_work_running(&PromptSource::Channel(original.clone()));
        agent.prepare_background_return();
        pool.return_agent(agent);
        let mut queue = EventQueue::new(DedupMode::Queue);
        pool.queue_finished_background_work(&mut queue);
        pool.queue_finished_background_work(&mut queue);
        let batch = queue.flush_next();
        let duplicate = queue.flush_next();
        let held = pool.has_runtime_hold();
        pool.agents[0].as_mut().unwrap().acp.shutdown().await;
        assert_eq!(
            batch.is_some(),
            prior_work,
            "uncertainty alone cannot create completion evidence"
        );
        if let Some(batch) = batch {
            assert_eq!(batch.scope, original);
            assert_eq!(batch.continuation.unwrap().session_id, "snapshot-session");
        }
        assert!(duplicate.is_none());
        assert_eq!(held, prior_work);
    }
}

#[tokio::test]
async fn uncertain_snapshot_capture_never_adopts_shell_or_real_descendants() {
    for session_capture in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let mut agent = snapshot_worker(&dir, false, false).await;
        let (worker, server) = parent(&agent);
        if session_capture {
            agent
                .acp
                .process_tree_snapshots
                .push_back(Some(empty(worker, server)));
        }
        agent
            .acp
            .process_tree_snapshots
            .push_back(Some(census(worker, server, None, true)));
        if session_capture {
            agent
                .acp
                .session_new("/tmp", vec![], None, None)
                .await
                .unwrap();
        } else {
            agent.acp.capture_startup_children();
        }
        agent.acp.process_tree_snapshots.push_back(Some(census(
            worker,
            server,
            Some(&format!("{server} /fixture/codex\n{SHELL} /bin/bash\n")),
            true,
        )));
        let blocking = agent.acp.background_child_pids().unwrap();
        agent.acp.shutdown().await;
        assert_eq!(
            blocking,
            HashSet::from([WORK]),
            "later confirmation must reveal actual child work"
        );
    }
}

#[tokio::test]
async fn snapshot_uncertainty_is_owned_and_known_wrong_identity_stays_work() {
    let dir = tempfile::tempdir().unwrap();
    let mut agent = snapshot_worker(&dir, false, false).await;
    let (worker, server) = parent(&agent);
    agent.acp.process_tree_snapshots.push_back(Some(census(
        worker,
        server,
        Some(&format!(
            "{server} /fixture/codex\n{SHELL} /usr/bin/python3\n"
        )),
        false,
    )));
    let wrong = agent.acp.background_child_pids().unwrap();
    // A foreign worker's partial probe cannot make this idle worker uncertain.
    let script = SNAPSHOT.split_whitespace().collect::<Vec<_>>().join(" ");
    let foreign = format!("{server} {worker} S /fixture/codex app-server\n880001 1 S /other/codex app-server\n880002 880001 S /bin/bash -lc {script}\n");
    agent
        .acp
        .process_tree_snapshots
        .push_back(Some(ProcessTree::from_snapshot_with_identities(
            &foreign, None,
        )));
    let unrelated = agent.acp.background_child_pids();
    agent.acp.shutdown().await;
    assert_eq!(wrong, HashSet::from([SHELL]));
    assert_eq!(unrelated, Some(HashSet::new()));
}

#[tokio::test]
async fn new_real_work_alongside_uncertain_snapshot_collects_after_its_exit() {
    let dir = tempfile::tempdir().unwrap();
    let agent = snapshot_worker(&dir, false, false).await;
    let (worker, server) = parent(&agent);
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    let original = scope();
    let mut agent = remember(&mut pool, &original);
    agent.acp.process_tree_snapshots.extend([
        Some(census(
            worker,
            server,
            Some(&format!("{server} /fixture/codex\n")),
            true,
        )),
        Some(census(
            worker,
            server,
            Some(&format!("{server} /fixture/codex\n")),
            false,
        )),
        Some(empty(worker, server)),
        Some(empty(worker, server)),
    ]);
    assert!(agent.background_work_running(&PromptSource::Channel(original.clone())));
    let observed = agent
        .background_context
        .as_ref()
        .unwrap()
        .blocking_pids
        .clone();
    agent.prepare_background_return();
    pool.return_agent(agent);
    let mut queue = EventQueue::new(DedupMode::Queue);
    pool.queue_finished_background_work(&mut queue);
    pool.queue_finished_background_work(&mut queue);
    let batch = queue.flush_next();
    let duplicate = queue.flush_next();
    pool.agents[0].as_mut().unwrap().acp.shutdown().await;
    assert_eq!(
        observed,
        HashSet::from([WORK]),
        "only genuine child work may become completion evidence"
    );
    let batch = batch.expect("real child first observed alongside uncertainty must collect");
    assert_eq!(batch.scope, original);
    assert_eq!(batch.continuation.unwrap().session_id, "snapshot-session");
    assert!(duplicate.is_none());
}
