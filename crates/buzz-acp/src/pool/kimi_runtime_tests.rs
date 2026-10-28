//! Production dispatch boundaries, using bounded synthetic ACP peers.
use super::*;
use crate::pool::TaskMeta;

#[tokio::test]
async fn kimi_failed_external_action_is_not_replayed_after_transient_provider_error() {
    use crate::{
        config::DedupMode,
        queue::{EventQueue, QueuedEvent, RecoveryPolicy},
    };
    let fixture = Fixture::new();
    let scope = scope();
    let (ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    let marker = fixture.dir.path().join("external-action");
    let error =
        serde_json::json!({"code":-32603,"message":"OAuthConnectionError: connection timed out"})
            .to_string();
    let agent = fixture
        .agent(&[
            ("KIMI_IDENTITY", "1"),
            ("FAIL_PROMPT", "1"),
            ("KIMI_ERROR", &error),
            ("EFFECT_MARKER", marker.to_str().unwrap()),
        ])
        .await;
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    let mut queue = EventQueue::new(DedupMode::Queue)
        .with_kimi_recovery(RecoveryPolicy::default(), "/tmp/work");
    queue.push(QueuedEvent {
        channel_id: scope.channel_id(),
        scope: scope.clone(),
        event: EventBuilder::new(Kind::Custom(9), "uncertain external action")
            .sign_with_keys(&Keys::generate())
            .unwrap(),
        prompt_tag: "mention".into(),
        received_at: std::time::Instant::now(),
        edit: None,
    });
    let ctx = Arc::new(ctx);
    assert_eq!(
        crate::dispatch_pending(
            &mut pool,
            &mut queue,
            &ctx,
            &mut tokio::time::Instant::now(),
            None
        )
        .len(),
        1
    );
    let result = tokio::time::timeout(Duration::from_secs(5), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(&result.outcome, PromptOutcome::Error(AcpError::KimiFailure(failure)) if failure.class.is_transient())
    );
    let work = result.batch.as_ref().unwrap().work.clone().unwrap();
    crate::pool::background_work_tests::apply_result(
        &mut pool,
        &mut queue,
        result,
        None,
        &Default::default(),
    );
    assert!(
        work.needs_human(),
        "failed tool status does not prove absence of partial effects"
    );
    assert!(!queue.has_pending_scope(&scope));
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "performed\n");
    pool.agents[0].take().unwrap().acp.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn kimi_permission_mailbox_requires_current_route_scope_and_control_generation() {
    use crate::{
        config::DedupMode,
        queue::{EventQueue, QueuedEvent, RecoveryPolicy},
    };
    for cancelled in [false, true] {
        let fixture = Fixture::new();
        let scope = scope();
        let (mut ctx, server) = relay_context(scope.channel_id(), vec![]).await;
        ctx.session_policy = crate::scope::SessionPolicy::Thread;
        ctx.session_controls =
            crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
        let mut queue = EventQueue::new(DedupMode::Queue)
            .with_kimi_recovery(RecoveryPolicy::default(), "/tmp/work");
        queue.push(QueuedEvent {
            channel_id: scope.channel_id(),
            scope: scope.clone(),
            event: EventBuilder::new(Kind::Custom(9), "permission route")
                .tag(Tag::parse(["e", scope.root_event_id().unwrap(), "", "reply"]).unwrap())
                .sign_with_keys(&Keys::generate())
                .unwrap(),
            prompt_tag: "mention".into(),
            received_at: std::time::Instant::now(),
            edit: None,
        });
        let batch = queue.flush_next().unwrap();
        let route = batch.reply_tags();
        let mut pool = AgentPool::from_slots(vec![]);
        let handle = pool.join_set.spawn(std::future::pending());
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        pool.task_map.insert(
            handle.id(),
            TaskMeta {
                permission_tx: Some(tx),
                agent_index: 0,
                channel_id: Some(scope.channel_id()),
                scope: Some(scope.clone()),
                turn_id: "permission-bound".into(),
                recoverable_batch: Some(batch),
                control_tx: None,
                steer_tx: None,
                control_dispatch: Some(ctx.session_controls.capture(&scope)),
                successful_steer_deliveries: Default::default(),
            },
        );
        let vote = crate::acp::kimi_permission::PermissionDecision {
            token: Uuid::new_v4(),
            allow: true,
        };
        let wrong_scope = SessionScope::Thread {
            channel_id: scope.channel_id(),
            root_event_id: "b".repeat(64),
        };
        let wrong_route = crate::queue::ThreadTags {
            root_event_id: Some("b".repeat(64)),
            parent_event_id: Some("b".repeat(64)),
            ..Default::default()
        };
        assert!(!pool.permission_decision(&wrong_scope, &route, vote.clone()));
        assert!(!pool.permission_decision(&scope, &wrong_route, vote.clone()));
        assert!(rx.try_recv().is_err());
        assert!(pool.permission_decision(&scope, &route, vote.clone()));
        assert_eq!(rx.recv().await.unwrap().token, vote.token);
        if cancelled {
            assert!(queue.cancel_logical_work(&scope));
        } else {
            let job = super::control_tests::authorized_control(
                &ctx,
                &Keys::generate(),
                &scope,
                "!restart all",
            )
            .await;
            assert_eq!(job.scope, scope);
            assert!(ctx.session_controls.restart(job).unwrap());
        }
        assert!(!pool.permission_decision(&scope, &route, vote));
        assert!(rx.try_recv().is_err());
        pool.join_set.shutdown().await;
        server.abort();
    }
}

#[tokio::test]
async fn kimi_terminal_provider_errors_do_not_enter_the_recovery_queue() {
    use crate::{
        config::DedupMode,
        queue::{EventQueue, QueuedEvent, RecoveryPolicy},
    };
    for failure in [
        serde_json::json!({"code":-32603,"message":"Internal error","data":{"name":"OAuthConnectionError","message":"timeout but credentials revoked","status":401}}),
        serde_json::json!({"code":-32603,"message":"model not found","data":{"name":"ModelNotFoundError"}}),
        serde_json::json!({"code":-32603,"message":"unknown problem","data":{"name":"UnexpectedError"}}),
    ] {
        let fixture = Fixture::new();
        let scope = scope();
        let (ctx, server) = relay_context(scope.channel_id(), vec![]).await;
        let text = failure.to_string();
        let agent = fixture
            .agent(&[
                ("KIMI_IDENTITY", "1"),
                ("FAIL_PROMPT", "1"),
                ("KIMI_ERROR", &text),
            ])
            .await;
        let mut pool = AgentPool::from_slots(vec![Some(agent)]);
        let mut queue = EventQueue::new(DedupMode::Queue)
            .with_kimi_recovery(RecoveryPolicy::default(), "/tmp/work");
        queue.push(QueuedEvent {
            channel_id: scope.channel_id(),
            scope: scope.clone(),
            event: EventBuilder::new(Kind::Custom(9), "terminal request")
                .sign_with_keys(&Keys::generate())
                .unwrap(),
            prompt_tag: "mention".into(),
            received_at: std::time::Instant::now(),
            edit: None,
        });
        let ctx = Arc::new(ctx);
        assert_eq!(
            crate::dispatch_pending(
                &mut pool,
                &mut queue,
                &ctx,
                &mut tokio::time::Instant::now(),
                None
            )
            .len(),
            1
        );
        let result = tokio::time::timeout(Duration::from_secs(5), pool.result_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(&result.outcome, PromptOutcome::Error(AcpError::KimiFailure(failure)) if !failure.class.is_transient())
        );
        crate::pool::background_work_tests::apply_result(
            &mut pool,
            &mut queue,
            result,
            None,
            &Default::default(),
        );
        assert!(!queue.has_pending_scope(&scope));
        assert!(queue.next_retry_deadline().is_none());
        assert!(queue.flush_next().is_none());
        assert_eq!(
            fixture
                .methods()
                .iter()
                .filter(|method| *method == "session/prompt")
                .count(),
            1
        );
        pool.agents[0].take().unwrap().acp.shutdown().await;
        server.abort();
    }
}

#[tokio::test]
async fn kimi_confirmed_report_prevents_any_new_provider_rpc() {
    use crate::{
        config::DedupMode,
        queue::{EventQueue, QueuedEvent, RecoveryPolicy},
    };
    let fixture = Fixture::new();
    let scope = scope();
    let (ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    let agent = fixture.agent(&[("KIMI_IDENTITY", "1")]).await;
    let mut queue = EventQueue::new(DedupMode::Queue).with_kimi_recovery(
        RecoveryPolicy::default(),
        fixture.dir.path().to_str().unwrap(),
    );
    queue.push(QueuedEvent {
        channel_id: scope.channel_id(),
        scope,
        event: EventBuilder::new(Kind::Custom(9), "already reported work")
            .sign_with_keys(&Keys::generate())
            .unwrap(),
        prompt_tag: "mention".into(),
        received_at: std::time::Instant::now(),
        edit: None,
    });
    let mut batch = queue.flush_next().unwrap();
    let work = batch.work.as_mut().unwrap();
    work.journal_dir = fixture.dir.path().to_owned();
    work.attempt = 1;
    let (journal, _) =
        crate::managed_work::tests::confirmed_journal(&ctx.rest_client, work, "first final report");
    std::fs::write(work.journal_dir.join("journal.json"), journal).unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    run_prompt_task(
        agent,
        Some(batch),
        None,
        Arc::new(ctx),
        tx,
        None,
        "receipt-replay".into(),
        Default::default(),
    )
    .await;
    let mut result = rx.recv().await.unwrap();
    assert!(matches!(result.outcome, PromptOutcome::Ok(_)));
    assert_eq!(
        fixture.methods(),
        vec!["initialize"],
        "confirmed report suppresses even session creation"
    );
    assert!(result.batch.is_none());
    result.agent.acp.shutdown().await;
    server.abort();
}

/// Real queue/handler/ACP pipes; the OAuth error payload remains synthetic.
#[tokio::test]
async fn kimi_oauth_failure_retries_original_work_through_production_dispatch() {
    use crate::{
        config::DedupMode,
        queue::{EventQueue, QueuedEvent, RecoveryPolicy},
    };
    let fixture = Fixture::new();
    let scope = scope();
    let (ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    let prompts = fixture.dir.path().join("prompts");
    let error = serde_json::json!({"code":-32603,"message":"Internal error","data":{
        "name":"OAuthError","message":"Unable to acquire OAuth refresh lock"}})
    .to_string();
    let agent = fixture
        .agent(&[
            ("KIMI_IDENTITY", "1"),
            ("FAIL_PROMPT", "1"),
            ("KIMI_ERROR", &error),
            ("PROMPTS", prompts.to_str().unwrap()),
        ])
        .await;
    let process = agent.acp.process_instance;
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    let mut queue = EventQueue::new(DedupMode::Queue).with_kimi_recovery(
        RecoveryPolicy {
            delays: vec![Duration::from_nanos(1)],
            ..Default::default()
        },
        "/tmp/work",
    );
    queue.push(QueuedEvent {
        channel_id: scope.channel_id(),
        scope: scope.clone(),
        event: EventBuilder::new(Kind::Custom(9), "original work")
            .sign_with_keys(&Keys::generate())
            .unwrap(),
        prompt_tag: "mention".into(),
        received_at: std::time::Instant::now(),
        edit: None,
    });
    let ctx = Arc::new(ctx);
    let dispatch = |pool: &mut AgentPool, queue: &mut EventQueue| {
        crate::dispatch_pending(pool, queue, &ctx, &mut tokio::time::Instant::now(), None).len()
    };
    assert_eq!(dispatch(&mut pool, &mut queue), 1);
    let result = tokio::time::timeout(Duration::from_secs(5), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(&result.outcome, PromptOutcome::Error(AcpError::KimiFailure(failure)) if failure.class == crate::acp::kimi_recovery::FailureClass::OAuthRefreshLock)
    );
    let original = result.batch.as_ref().unwrap().work.clone().unwrap();
    crate::pool::background_work_tests::apply_result(
        &mut pool,
        &mut queue,
        result,
        None,
        &Default::default(),
    );
    assert_eq!(dispatch(&mut pool, &mut queue), 1);
    let mut result = tokio::time::timeout(Duration::from_secs(5), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(result.outcome, PromptOutcome::Ok(_)));
    assert_eq!(
        result.agent.acp.process_instance, process,
        "application failure reuses the healthy pipe"
    );
    let retried = result.agent.acp.logical_work().unwrap();
    assert_eq!(retried.delivery_key, original.delivery_key);
    assert_eq!(retried.deadline, original.deadline);
    assert_eq!(retried.attempt, 1);
    assert_eq!(
        retried.reply_tags().root_event_id,
        original.reply_tags().root_event_id
    );
    assert_eq!(
        fixture
            .methods()
            .iter()
            .filter(|method| *method == "session/new")
            .count(),
        1
    );
    let requests: Vec<serde_json::Value> = std::fs::read_to_string(prompts)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(requests.len(), 2);
    for prompt in requests {
        assert!(prompt
            .to_string()
            .contains(&original.delivery_key.to_string()));
    }
    result.agent.acp.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn kimi_explicit_cancel_interrupts_startup_and_returns_no_retry_batch() {
    use crate::{
        config::DedupMode,
        queue::{EventQueue, QueuedEvent, RecoveryPolicy},
    };
    let fixture = Fixture::new();
    let scope = scope();
    let (ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    let agent = fixture.agent(&[("DELAY", "2")]).await;
    let process = agent.acp.process_instance;
    let mut queue = EventQueue::new(DedupMode::Queue)
        .with_kimi_recovery(RecoveryPolicy::default(), "/tmp/work");
    queue.push(QueuedEvent {
        channel_id: scope.channel_id(),
        scope: scope.clone(),
        event: EventBuilder::new(Kind::Custom(9), "cancel startup")
            .sign_with_keys(&Keys::generate())
            .unwrap(),
        prompt_tag: "mention".into(),
        received_at: std::time::Instant::now(),
        edit: None,
    });
    let batch = queue.flush_next().unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let task = tokio::spawn(run_prompt_task(
        agent,
        Some(batch),
        None,
        Arc::new(ctx),
        tx,
        None,
        "cancel-startup".into(),
        Default::default(),
    ));
    tokio::time::timeout(Duration::from_secs(2), async {
        while !fixture
            .methods()
            .iter()
            .any(|method| method == "session/new")
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(queue.cancel_logical_work(&scope));
    let mut result = tokio::time::timeout(Duration::from_millis(500), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.agent.acp.process_instance, process);
    assert!(matches!(
        result.outcome,
        PromptOutcome::Error(AcpError::WorkCancelledPoisoned)
    ));
    assert!(result.batch.is_none());
    result.agent.acp.shutdown().await;
    task.await.unwrap();
    server.abort();
}

#[tokio::test]
async fn logical_deadline_covers_session_creation_and_returns_the_owned_process() {
    use crate::config::DedupMode;
    use crate::queue::{EventQueue, QueuedEvent, RecoveryPolicy};
    let fixture = Fixture::new();
    let scope = scope();
    let (ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    let agent = fixture.agent(&[("DELAY", "1")]).await;
    let process = agent.acp.process_instance;
    let event = EventBuilder::new(Kind::Custom(9), "bounded work")
        .sign_with_keys(&Keys::generate())
        .unwrap();
    let mut queue = EventQueue::new(DedupMode::Queue)
        .with_kimi_recovery(RecoveryPolicy::default(), "/tmp/work");
    queue.push(QueuedEvent {
        channel_id: scope.channel_id(),
        scope,
        event,
        prompt_tag: "mention".into(),
        received_at: std::time::Instant::now(),
        edit: None,
    });
    let mut batch = queue.flush_next().unwrap();
    batch.work.as_mut().unwrap().deadline = std::time::Instant::now() + Duration::from_millis(100);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let task = tokio::spawn(run_prompt_task(
        agent,
        Some(batch),
        None,
        Arc::new(ctx),
        tx,
        None,
        "bounded-startup".into(),
        Default::default(),
    ));
    let returned = tokio::time::timeout(Duration::from_millis(600), rx.recv()).await;
    if returned.is_err() {
        task.abort();
    }
    let mut result = returned
        .expect("logical deadline must also bound startup")
        .unwrap();
    assert_eq!(result.agent.acp.process_instance, process);
    assert!(matches!(
        result.outcome,
        PromptOutcome::Error(AcpError::WorkDeadlinePoisoned)
    ));
    assert!(result.batch.as_ref().unwrap().work.is_some());
    assert!(!fixture
        .methods()
        .iter()
        .any(|method| method == "session/prompt"));
    result.agent.acp.shutdown().await;
    task.await.unwrap();
    server.abort();
}

#[tokio::test]
async fn kimi_initial_error_after_background_launch_preserves_collection_without_foreground_replay()
{
    exercise_initial_background_error(false).await;
}

#[tokio::test]
async fn kimi_terminal_error_after_background_launch_requires_result_reconciliation() {
    exercise_initial_background_error(true).await;
}

async fn exercise_initial_background_error(terminal: bool) {
    use crate::{
        config::DedupMode,
        queue::{EventQueue, QueuedEvent, RecoveryPolicy},
    };
    struct Finish(std::path::PathBuf);
    impl Drop for Finish {
        fn drop(&mut self) {
            let _ = std::fs::write(&self.0, "done");
        }
    }
    let fixture = Fixture::new();
    let finish = Finish(fixture.dir.path().join("finish-background"));
    let scope = scope();
    let (ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    let prompts = fixture.dir.path().join("prompts");
    let message = if terminal {
        "OAuthConnectionError: Re-authenticate after connection timeout"
    } else {
        "OAuthConnectionError: connection timed out"
    };
    let error = serde_json::json!({"code":-32603,"message":message}).to_string();
    let agent = fixture
        .agent(&[
            ("KIMI_IDENTITY", "1"),
            ("FAIL_PROMPT", "1"),
            ("KIMI_ERROR", &error),
            ("BACKGROUND_MARKER", finish.0.to_str().unwrap()),
            ("PROMPTS", prompts.to_str().unwrap()),
        ])
        .await;
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    let mut queue = EventQueue::new(DedupMode::Queue).with_kimi_recovery(
        RecoveryPolicy {
            delays: vec![Duration::from_nanos(1)],
            ..Default::default()
        },
        fixture.dir.path().to_str().unwrap(),
    );
    queue.push(QueuedEvent {
        channel_id: scope.channel_id(),
        scope: scope.clone(),
        event: EventBuilder::new(Kind::Custom(9), "launch background work")
            .sign_with_keys(&Keys::generate())
            .unwrap(),
        prompt_tag: "mention".into(),
        received_at: std::time::Instant::now(),
        edit: None,
    });
    let ctx = Arc::new(ctx);
    let dispatch = |pool: &mut AgentPool, queue: &mut EventQueue| {
        crate::dispatch_pending(pool, queue, &ctx, &mut tokio::time::Instant::now(), None).len()
    };
    assert_eq!(dispatch(&mut pool, &mut queue), 1);
    let result = tokio::time::timeout(Duration::from_secs(5), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(&result.outcome,PromptOutcome::Error(AcpError::KimiFailure(failure)) if failure.class.is_transient() != terminal)
    );
    let original = result.batch.as_ref().unwrap().work.clone().unwrap();
    assert!(
        !original.uncertain_effects(),
        "launch completion was observed"
    );
    crate::pool::background_work_tests::apply_result(
        &mut pool,
        &mut queue,
        result,
        None,
        &Default::default(),
    );
    if terminal {
        let human = original.needs_human();
        drop(finish);
        pool.agents[0].take().unwrap().acp.shutdown().await;
        server.abort();
        assert!(
            human,
            "a terminal provider failure cannot discard a still-live background result"
        );
        assert!(!queue.has_pending_scope(&scope));
        return;
    }
    assert_eq!(
        dispatch(&mut pool, &mut queue),
        0,
        "a live background child must prevent foreground replay"
    );
    assert!(
        pool.runtime_holds[&0].continuation.is_some(),
        "the original collection must survive"
    );
    drop(finish);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            pool.queue_finished_background_work(&mut queue);
            if queue.has_flushable_work() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(dispatch(&mut pool, &mut queue), 1);
    let mut result = tokio::time::timeout(Duration::from_secs(5), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(result.outcome, PromptOutcome::Ok(_)));
    let work = result.agent.acp.logical_work().unwrap();
    assert_eq!(work.delivery_key, original.delivery_key);
    assert_eq!(work.deadline, original.deadline);
    assert_eq!(
        work.reply_tags().root_event_id,
        original.reply_tags().root_event_id
    );
    let frames: Vec<serde_json::Value> = std::fs::read_to_string(prompts)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(frames.len(), 2);
    assert!(frames[1].to_string().contains("background child process"));
    result.agent.acp.shutdown().await;
    server.abort();
}
