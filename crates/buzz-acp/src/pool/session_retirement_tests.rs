use super::*;
use crate::pool::background_work::RuntimeHold;
use crate::pool::{PromptResult, PromptSource, TaskMeta};
use std::collections::HashSet;

#[tokio::test]
async fn retirement_targets_include_checked_out_copies_and_block_idle_claims() {
    let fixture = Fixture::new();
    let a = scope();
    let agent = fixture.agent(&[]).await;
    let mut pool = AgentPool::from_slots(vec![Some(agent), None]);
    let generation = pool.record_scope_owner(a.clone(), 0);
    let cached = pool.agents[0].as_mut().unwrap();
    cached.state.sessions.insert(a.clone(), "cached-a".into());
    cached
        .state
        .scope_owner_generations
        .insert(a.clone(), generation);
    pool.mark_retire_on_return(0);
    assert!(
        pool.try_claim(Some(&a)).is_none(),
        "affinity cannot reclaim a reserved worker"
    );
    assert!(!pool.any_idle());
    pool.record_scope_owner(a.clone(), 1);
    assert_eq!(pool.retirement_targets(&a), vec![0, 1]);
    assert!(
        pool.try_claim(None).is_none(),
        "admitted idle target cannot be claimed"
    );
    let mut agent = pool
        .take_retirement_target(0, &[a].into_iter().collect(), true)
        .unwrap();
    agent.acp.shutdown().await;
}

#[tokio::test]
async fn retirement_foreign_background_hold_waits_but_own_hold_does_not() {
    let fixture = Fixture::new();
    let a = scope();
    let b = SessionScope::Thread {
        channel_id: a.channel_id(),
        root_event_id: "b".repeat(64),
    };
    let agent = fixture.agent(&[]).await;
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    pool.record_scope_owner(a.clone(), 0);
    pool.set_runtime_hold(
        0,
        RuntimeHold {
            scope: b.clone(),
            blocking_pids: HashSet::from([123]),
            continuation: None,
            collection_pending: false,
        },
    );
    pool.mark_retire_on_return(0);
    assert!(pool
        .take_retirement_target(0, &HashSet::from([a]), false)
        .is_none());
    let mut agent = pool
        .take_retirement_target(0, &HashSet::from([b]), false)
        .unwrap();
    agent.acp.shutdown().await;
}

#[tokio::test]
async fn retirement_background_deadline_cannot_take_foreground_worker() {
    let fixture = Fixture::new();
    let a = scope();
    let agent = fixture.agent(&[]).await;
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    pool.mark_retire_on_return(0);
    let handle = pool.join_set.spawn(std::future::pending::<()>());
    pool.task_map.insert(
        handle.id(),
        TaskMeta {
            agent_index: 0,
            channel_id: Some(a.channel_id()),
            scope: Some(a.clone()),
            turn_id: "foreign-foreground".into(),
            recoverable_batch: None,
            control_tx: None,
            steer_tx: None,
            control_dispatch: None,
            successful_steer_deliveries: Default::default(),
        },
    );
    assert!(pool
        .take_retirement_target(0, &HashSet::from([a.clone()]), true)
        .is_none());
    pool.task_map.clear();
    handle.abort();
    let mut agent = pool
        .take_retirement_target(0, &HashSet::from([a]), true)
        .unwrap();
    agent.acp.shutdown().await;
}

#[tokio::test]
async fn retirement_background_release_avoids_unclaimable_collection() {
    let fixture = Fixture::new();
    let a = scope();
    let agent = fixture.agent(&[]).await;
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    let generation = pool.record_scope_owner(a.clone(), 0);
    let cached = pool.agents[0].as_mut().unwrap();
    cached
        .state
        .sessions
        .insert(a.clone(), "background-a".into());
    cached
        .state
        .scope_owner_generations
        .insert(a.clone(), generation);
    pool.set_runtime_hold(
        0,
        RuntimeHold {
            scope: a.clone(),
            blocking_pids: HashSet::from([123]),
            continuation: Some(crate::queue::RuntimeContinuation {
                delivery_key: Uuid::new_v4(),
                ordinary_preempted: false,
                worker_index: 0,
                session_id: "background-a".into(),
                generation,
                thread_tags: crate::queue::ThreadTags::default(),
                received_at: std::time::Instant::now(),
            }),
            collection_pending: false,
        },
    );
    pool.mark_retire_on_return(0);
    let mut queue = crate::EventQueue::new(crate::DedupMode::Queue);
    pool.queue_finished_background_work(&mut queue);
    assert!(!pool.has_runtime_hold());
    assert!(!queue.has_undispatched_work());
    let mut agent = pool
        .take_retirement_target(0, &HashSet::new(), false)
        .unwrap();
    agent.acp.shutdown().await;
}

#[tokio::test]
async fn retirement_return_boundary_bypasses_crash_budget_for_fatal_old_outcome() {
    let fixture = Fixture::new();
    let a = scope();
    let agent = fixture.agent(&[]).await;
    let mut pool = AgentPool::from_slots(vec![None]);
    pool.mark_retire_on_return(0);
    let handle = pool.join_set.spawn(std::future::pending::<()>());
    pool.task_map.insert(
        handle.id(),
        TaskMeta {
            agent_index: 0,
            channel_id: Some(a.channel_id()),
            scope: Some(a.clone()),
            turn_id: "old".into(),
            recoverable_batch: None,
            control_tx: None,
            steer_tx: None,
            control_dispatch: None,
            successful_steer_deliveries: Default::default(),
        },
    );
    let result = PromptResult {
        agent,
        source: PromptSource::Channel(a.clone()),
        turn_id: "old".into(),
        outcome: PromptOutcome::AgentExited,
        batch: None,
    };
    let config = crate::error_outcome_emission_tests::test_config();
    let mut queue = crate::EventQueue::new(crate::DedupMode::Queue);
    let mut heartbeat = false;
    let mut circuits = vec![crate::SlotCircuit {
        crash_times: vec![],
        open_until: None,
        respawn_in_flight: false,
    }];
    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let mut tasks = tokio::task::JoinSet::new();
    let mut activity = tokio::time::Instant::now();
    assert!(matches!(
        crate::handle_prompt_result(
            &mut pool,
            &mut queue,
            &config,
            result,
            &mut heartbeat,
            &HashSet::new(),
            &mut circuits,
            &tx,
            &mut tasks,
            None,
            None,
            &mut activity
        ),
        crate::LoopAction::Continue
    ));
    assert!(circuits[0].crash_times.is_empty());
    assert!(tasks.is_empty());
    assert!(pool.try_claim(None).is_none());
    let mut agent = pool
        .take_retirement_target(0, &HashSet::from([a]), false)
        .unwrap();
    agent.acp.shutdown().await;
    handle.abort();
}
