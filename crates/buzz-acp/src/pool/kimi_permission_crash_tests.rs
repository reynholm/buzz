//! Manual permission crashes through real ACP pipes and production dispatch.
use super::*;
use crate::{
    acp::{kimi_permission::PermissionDecision, TerminationReport},
    config::{DedupMode, PermissionMode},
    observer::{ObserverEvent, ObserverHandle},
    queue::{EventQueue, QueuedEvent, RecoveryPolicy},
};
use std::collections::HashSet;
use tokio::time::timeout;

const ADAPTER: &str = r#"
import json, os, pathlib, select, sys, time
directory = pathlib.Path(sys.argv[1])
for line in sys.stdin:
    req = json.loads(line)
    with (directory / 'requests.jsonl').open('a') as out:
        out.write(json.dumps(req) + '\n')
    method = req.get('method')
    if 'id' not in req: continue
    if method == 'initialize':
        result = {'protocolVersion': 1, 'agentInfo': {'name': 'Kimi Code CLI', 'version': 'synthetic-manual-crash'}}
    elif method == 'session/new': result = {'sessionId': 'permission-session'}
    elif method == 'session/prompt':
        session = req['params']['sessionId']
        print(json.dumps({'jsonrpc': '2.0', 'id': 'permission-one', 'method': 'session/request_permission', 'params': {'sessionId': session, 'toolCall': {'toolCallId': 'pending-execute', 'title': 'Bash', 'kind': 'execute'}, 'options': [{'kind': 'allow_once', 'optionId': 'allow'}, {'kind': 'reject_once', 'optionId': 'deny'}]}}), flush=True)
        while True:
            if (directory / 'crash').exists() and not (directory / 'crashed').exists():
                (directory / 'crashed').write_text('before decision')
                if (directory / 'receipt-path').exists():
                    state = pathlib.Path((directory / 'receipt-path').read_text())
                    temporary = state.with_suffix('.next')
                    temporary.write_text(json.dumps({'state': (directory / 'crash').read_text()}))
                    temporary.replace(state)
                    if (directory / 'decision-deadline').exists():
                        time.sleep(.5)
                sys.exit(17)
            if select.select([sys.stdin], [], [], .01)[0]:
                response = json.loads(sys.stdin.readline())
                with (directory / 'responses.jsonl').open('a') as out:
                    out.write(json.dumps(response) + '\n')
                assert response['id'] == 'permission-one'
                assert response['result']['outcome']['optionId'] == 'allow'
                break
        print(json.dumps({'jsonrpc': '2.0', 'method': 'session/update', 'params': {'sessionId': session, 'update': {'sessionUpdate': 'tool_call_update', 'toolCallId': 'pending-execute', 'status': 'completed'}}}), flush=True)
        result = {'stopReason': 'end_turn'}
    else: result = {}
    print(json.dumps({'jsonrpc': '2.0', 'id': req['id'], 'result': result}), flush=True)
"#;

fn args(fixture: &Fixture) -> Vec<String> {
    vec![
        "-u".into(),
        "-c".into(),
        ADAPTER.into(),
        fixture.dir.path().display().to_string(),
    ]
}

fn worker(fixture: &Fixture, acp: AcpClient) -> OwnedAgent {
    OwnedAgent {
        index: 0,
        acp,
        state: SessionState::with_store(Some(fixture.store.clone())),
        model_capabilities: None,
        desired_model: None,
        model_overridden: false,
        desired_model_request_id: None,
        desired_model_pending_ack: false,
        startup_effort: None,
        agent_name: "Kimi Code CLI".into(),
        goose_system_prompt_supported: None,
        protocol_version: 1,
        background_context: None,
    }
}

async fn lifecycle(
    observed: &mut tokio::sync::broadcast::Receiver<ObserverEvent>,
    state: &str,
) -> ObserverEvent {
    timeout(Duration::from_secs(3), async {
        loop {
            let event = observed.recv().await.unwrap();
            if event.kind == "permission_lifecycle" && event.payload["state"] == state {
                return event;
            }
        }
    })
    .await
    .unwrap()
}

fn token(event: &ObserverEvent) -> Uuid {
    Uuid::parse_str(event.payload["requestToken"].as_str().unwrap()).unwrap()
}

#[tokio::test]
async fn kimi_manual_permission_child_exit_recovers_same_work_and_rejects_retired_vote() {
    manual_permission_exit(None, false, true).await;
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn resource_failure_during_manual_permission_never_replays_work() {
    for failure in ["oom", "guard_failed"] {
        for deadline in [false, true] {
            for clean in [false, true] {
                manual_permission_exit(Some(failure), deadline, clean).await;
            }
        }
    }
}

async fn manual_permission_exit(failure: Option<&str>, deadline: bool, clean: bool) {
    let fixture = Fixture::new();
    let scope = scope();
    let (mut ctx, server) = relay_context(scope.channel_id(), vec![]).await;
    ctx.permission_mode = PermissionMode::Default;
    let observer = ObserverHandle::in_process();
    let mut observed = observer.subscribe();
    let mut acp = AcpClient::spawn("python3", &args(&fixture), &[], false)
        .await
        .unwrap();
    acp.set_observer(Some(observer.clone()), 0);
    acp.initialize().await.unwrap();
    #[cfg(target_os = "linux")]
    let group = failure.map(|_| {
        let group = acp.inject_unverified_resource_scope();
        std::fs::write(group.join("memory.events"), "oom_kill 0\n").unwrap();
        if clean {
            std::fs::remove_dir(group.join("cgroup.kill")).unwrap();
            std::fs::write(group.join("cgroup.kill"), "").unwrap();
            std::fs::write(group.join("cgroup.events"), "populated 0\n").unwrap();
        }
        std::fs::write(
            fixture.dir.path().join("receipt-path"),
            group.parent().unwrap().join("state.json").to_str().unwrap(),
        )
        .unwrap();
        group
    });
    let process = acp.process_instance;
    let mut pool = AgentPool::from_slots(vec![Some(worker(&fixture, acp))]);
    let mut queue = EventQueue::new(DedupMode::Queue).with_kimi_recovery(
        RecoveryPolicy {
            delays: vec![Duration::from_nanos(1)],
            approval_decision_timeout: if deadline {
                Duration::from_millis(200)
            } else {
                Duration::from_secs(120)
            },
            ..Default::default()
        },
        fixture.dir.path().to_str().unwrap(),
    );
    let event = EventBuilder::new(Kind::Custom(9), "original manual work")
        .tag(Tag::parse(["e", scope.root_event_id().unwrap(), "", "reply"]).unwrap())
        .sign_with_keys(&Keys::generate())
        .unwrap();
    let event_id = event.id.to_hex();
    queue.push(QueuedEvent {
        channel_id: scope.channel_id(),
        scope: scope.clone(),
        event,
        prompt_tag: "mention".into(),
        received_at: std::time::Instant::now(),
        edit: None,
    });
    let ctx = Arc::new(ctx);
    let dispatch = |pool: &mut AgentPool, queue: &mut EventQueue| {
        crate::dispatch_pending(pool, queue, &ctx, &mut tokio::time::Instant::now(), None).len()
    };
    assert_eq!(dispatch(&mut pool, &mut queue), 1);
    let pending = lifecycle(&mut observed, "waiting_approval").await;
    let task = pool.task_map().values().next().unwrap();
    let original = task
        .recoverable_batch
        .as_ref()
        .unwrap()
        .work
        .clone()
        .unwrap();
    let route = task.recoverable_batch.as_ref().unwrap().reply_tags();
    let delivery_key = original.delivery_key;
    let journal = original.journal_dir.clone();
    if deadline {
        std::fs::write(fixture.dir.path().join("decision-deadline"), "wait").unwrap();
    }
    std::fs::write(
        fixture.dir.path().join("crash"),
        failure.unwrap_or("exit before vote"),
    )
    .unwrap();
    let result = timeout(Duration::from_secs(2), pool.result_rx.recv())
        .await
        .expect("child exit must interrupt the 120-second permission decision wait")
        .unwrap();
    assert!(
        matches!(
            (&result.outcome, failure),
            (PromptOutcome::AgentExited, None)
                | (
                    PromptOutcome::Error(AcpError::ResourceLimit { .. }),
                    Some("oom")
                )
                | (
                    PromptOutcome::Error(AcpError::ResourceGuard(_)),
                    Some("guard_failed")
                )
        ),
        "manual wait lost exit cause: failure={failure:?}, deadline={deadline}"
    );
    let returned_work = result.batch.as_ref().unwrap().work.as_ref().unwrap();
    assert_eq!(returned_work.delivery_key, delivery_key);
    assert_eq!(returned_work.journal_dir, journal);
    assert_eq!(
        returned_work.reply_tags().root_event_id,
        route.root_event_id
    );
    assert!(!original.uncertain_effects(), "no action was approved");
    if !deadline {
        assert!(
            !pool.permission_decision(
                &scope,
                &route,
                PermissionDecision {
                    token: token(&pending),
                    allow: true
                }
            ),
            "a dead process cannot receive a late vote"
        );
    }
    assert!(!observer
        .snapshot()
        .iter()
        .any(|event| event.kind == "acp_write"
            && event.payload.pointer("/result/outcome/optionId").is_some()));

    let mut config = crate::build_mcp_servers_tests::test_config();
    config.agent_command = "python3".into();
    config.agent_args = args(&fixture);
    let mut circuits = vec![crate::SlotCircuit {
        crash_times: vec![],
        open_until: Some(std::time::Instant::now()),
        respawn_in_flight: false,
    }];
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let mut tasks = tokio::task::JoinSet::new();
    crate::handle_prompt_result(
        &mut pool,
        &mut queue,
        &config,
        result,
        &mut false,
        &HashSet::new(),
        &mut circuits,
        &tx,
        &mut tasks,
        None,
        None,
        &mut tokio::time::Instant::now(),
    );
    assert!(original.awaiting_retirement());
    assert_eq!(original.needs_human(), failure.is_some());
    assert_eq!(
        dispatch(&mut pool, &mut queue),
        0,
        "retry requires checked retirement"
    );
    let mut returned = timeout(Duration::from_secs(10), rx.recv())
        .await
        .unwrap()
        .unwrap();
    if failure.is_some() {
        assert_eq!(queue.queued_event_count(&scope), 0);
        assert!(!queue.has_undispatched_work());
        assert!(original.needs_human());
        assert_eq!(original.original_events[0].event.id.to_hex(), event_id);
        assert_eq!(original.reply_tags().root_event_id, route.root_event_id);
        let requests = std::fs::read_to_string(fixture.dir.path().join("requests.jsonl")).unwrap();
        assert_eq!(
            requests
                .lines()
                .filter(|line| line.contains("session/prompt"))
                .count(),
            1
        );
        if !clean {
            let retirement = returned.retirement.as_mut().unwrap();
            assert_eq!(retirement.report, TerminationReport::Unverified);
            assert!(returned.result.is_err(), "unverified worker was replaced");
            let mut retained = retirement.retained.take().unwrap();
            assert_eq!(
                retained.acp.logical_work().unwrap().delivery_key,
                delivery_key
            );
            assert_eq!(retained.acp.logical_work().unwrap().journal_dir, journal);
            #[cfg(target_os = "linux")]
            {
                let group = group.as_ref().unwrap();
                std::fs::remove_dir(group.join("cgroup.kill")).unwrap();
                std::fs::write(group.join("cgroup.kill"), "").unwrap();
                std::fs::write(group.join("cgroup.events"), "populated 0\n").unwrap();
            }
            assert_eq!(retained.acp.terminate().await, TerminationReport::Clean);
        } else {
            assert_eq!(
                returned.retirement.as_ref().unwrap().report,
                TerminationReport::Clean
            );
            assert!(!crate::worker_retirement::finish(
                &mut pool,
                &ctx.session_controls,
                &mut HashMap::new(),
                &mut returned
            ));
            let (acp, protocol, name) = returned.result.unwrap();
            assert_ne!(acp.process_instance, process);
            assert_eq!(original.delivery_key, delivery_key);
            assert_eq!(original.journal_dir, journal);
            let mut replacement = worker(&fixture, acp);
            replacement.protocol_version = protocol;
            replacement.agent_name = name;
            pool.return_agent(replacement);
            assert_eq!(pool.live_count(), 1);
        }
        assert_eq!(
            dispatch(&mut pool, &mut queue),
            0,
            "terminal work replayed after retirement"
        );
        tasks.shutdown().await;
        pool.join_set.shutdown().await;
        for agent in pool.agents.iter_mut().flatten() {
            agent.acp.shutdown().await;
        }
        server.abort();
        return;
    }
    assert_eq!(
        returned.retirement.as_ref().unwrap().report,
        TerminationReport::Clean
    );
    assert!(!crate::worker_retirement::finish(
        &mut pool,
        &ctx.session_controls,
        &mut HashMap::new(),
        &mut returned
    ));
    let (mut acp, protocol, name) = returned.result.unwrap();
    assert_ne!(acp.process_instance, process);
    acp.set_observer(Some(observer.clone()), 0);
    let mut replacement = worker(&fixture, acp);
    replacement.protocol_version = protocol;
    replacement.agent_name = name;
    pool.return_agent(replacement);
    assert_eq!(dispatch(&mut pool, &mut queue), 1);
    let retried = lifecycle(&mut observed, "waiting_approval").await;
    assert_ne!(token(&retried), token(&pending));
    assert!(pool.permission_decision(
        &scope,
        &route,
        PermissionDecision {
            token: token(&pending),
            allow: true
        }
    ));
    lifecycle(&mut observed, "stale_decision_ignored").await;
    assert!(
        pool.result_rx.try_recv().is_err(),
        "a retired token cannot approve the replacement request"
    );
    assert!(pool.permission_decision(
        &scope,
        &route,
        PermissionDecision {
            token: token(&retried),
            allow: true
        }
    ));
    let result = timeout(Duration::from_secs(3), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(result.outcome, PromptOutcome::Ok(StopReason::EndTurn)),
        "current approval must complete recovered work"
    );
    let recovered = result.agent.acp.logical_work().unwrap();
    assert_eq!(recovered.delivery_key, original.delivery_key);
    assert_eq!(recovered.deadline, original.deadline);
    assert_eq!(recovered.original_events[0].event.id.to_hex(), event_id);
    assert_eq!(
        recovered.reply_tags().root_event_id,
        original.reply_tags().root_event_id
    );
    let requests: Vec<serde_json::Value> =
        std::fs::read_to_string(fixture.dir.path().join("requests.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .filter(|request: &serde_json::Value| request["method"] == "session/prompt")
            .collect();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|request| request
        .to_string()
        .contains(&original.delivery_key.to_string())
        && request.to_string().contains(&event_id)));
    crate::pool::background_work_tests::apply_result(
        &mut pool,
        &mut queue,
        result,
        None,
        &HashSet::new(),
    );
    tasks.shutdown().await;
    pool.agents[0].take().unwrap().acp.shutdown().await;
    server.abort();
}
