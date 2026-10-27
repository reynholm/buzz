//! Script only discovery results; exercise real remembered context, return and poll.
use super::*;

async fn ordinary_return(
    snapshots: Vec<Option<HashSet<u32>>>,
) -> (AgentPool, EventQueue, SessionScope) {
    let scope = scope();
    let mut pool = AgentPool::from_slots(vec![Some(worker(0).await)]);
    bind(&mut pool, &scope, 0);
    let mut agent = pool.try_claim(Some(&scope)).unwrap();
    let session = agent.state.sessions[&scope].clone();
    let source = PromptSource::Channel(scope.clone());
    agent.remember_background_context(&source, None, &session);
    agent.acp.background_child_snapshots = snapshots.into();
    // This is the prompt-completion observation before result delivery.
    let _ = agent.background_work_running(&source);
    agent.prepare_background_return();
    pool.return_agent(agent);
    (pool, EventQueue::new(DedupMode::Queue), scope)
}

#[tokio::test]
async fn background_unknown_then_empty_releases_hold_without_inventing_collection() {
    let (mut pool, mut queue, scope) =
        ordinary_return(vec![None, None, Some(HashSet::new())]).await;
    assert!(
        pool.has_runtime_hold(),
        "Unknown must conservatively protect the original worker"
    );
    pool.queue_finished_background_work(&mut queue);
    let invented = queue.has_pending_continuation(&scope);
    let held = pool.has_runtime_hold();
    pool.agents[0].as_mut().unwrap().acp.shutdown().await;
    assert!(
        !invented,
        "no child was ever observed, so no result collection may be invented"
    );
    assert!(
        !held,
        "resolved empty discovery must release an uncertainty-only hold"
    );
}

#[tokio::test]
async fn background_observed_then_unknown_then_empty_collects_exactly_once() {
    let (mut pool, mut queue, scope) = ordinary_return(vec![
        Some(HashSet::from([12345])),
        None,
        Some(HashSet::new()),
        Some(HashSet::new()),
    ])
    .await;
    assert!(pool.has_runtime_hold());
    pool.queue_finished_background_work(&mut queue);
    assert!(queue.has_pending_continuation(&scope));
    pool.queue_finished_background_work(&mut queue);
    let batch = queue.flush_next().unwrap();
    assert!(batch.continuation.is_some());
    assert!(
        queue.flush_next().is_none(),
        "one observed completion must queue exactly once"
    );
    assert!(
        pool.has_runtime_hold(),
        "pending collection keeps original session protected"
    );
    pool.agents[0].as_mut().unwrap().acp.shutdown().await;
}

#[tokio::test]
async fn background_observed_exit_during_return_survives_later_unknown() {
    let (mut pool, mut queue, scope) = ordinary_return(vec![
        Some(HashSet::from([12345])),
        Some(HashSet::new()),
        None,
        Some(HashSet::new()),
    ])
    .await;
    pool.queue_finished_background_work(&mut queue);
    assert!(!queue.has_pending_continuation(&scope));
    pool.queue_finished_background_work(&mut queue);
    let collected = queue.has_pending_continuation(&scope);
    pool.agents[0].as_mut().unwrap().acp.shutdown().await;
    assert!(
        collected,
        "return must retain previously observed completion evidence across Unknown"
    );
}
