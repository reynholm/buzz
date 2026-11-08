//! Real ACP pipes exercising permission-only subagent and main-tool visibility.
use super::*;
use crate::observer::{ObserverEvent, ObserverHandle};
use std::time::Duration;

const ADAPTER: &str = r#"
import json,sys,time,select
case=json.loads(sys.argv[1]); permission=json.loads(sys.argv[2]); turns=0
if case.get('background'):
    permission['params']['toolCall']['toolCallId']='0:background_tool_fixture'
    permission['params']['toolCall']['title']='Bash'
    permission['params']['toolCall']['content'][0]['content']['text']='Requesting approval to background subagent action'
def emit(value): print(json.dumps(value),flush=True)
def update(value,session='fixture-session'):
    emit({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':session,'update':value}})
def tool(status='pending',name='0:tool_fixture'):
    if status=='progress': return {'sessionUpdate':'tool_call_update','toolCallId':name,'title':'Working'}
    if status=='create': return {'sessionUpdate':'tool_call','toolCallId':name,'kind':'execute'}
    return {'sessionUpdate':'tool_call' if status=='pending' else 'tool_call_update','toolCallId':name,'status':status,'kind':'execute'}
for line in sys.stdin:
    req=json.loads(line); method=req.get('method')
    if 'id' not in req: continue
    if method=='initialize': result={'protocolVersion':1,'agentInfo':{'name':case.get('name','Kimi Code CLI'),'version':case.get('version','2.5.2')}}
    elif method=='session/new': result={'sessionId':'fixture-session'}
    elif method=='session/prompt':
        turns+=1
        first=not case.get('only_first') or turns==1
        if first and case.get('before'): update(tool(case['before'] if isinstance(case['before'],str) else 'pending'))
        emit(permission)
        reply=json.loads(sys.stdin.readline())
        if first and case.get('after'): update(tool(case['after']))
        until=time.monotonic()+.24
        cancelled=False
        while time.monotonic()<until:
            if case.get('wrong_session'): update(tool('completed'),'unrelated-session')
            if case.get('wrong_tool'): update(tool('completed','another-tool'))
            if not case.get('silence') and not case.get('quiet'):
                update({'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'still working'}})
            if select.select([sys.stdin],[],[],.005)[0]:
                control=json.loads(sys.stdin.readline())
                if control.get('method')=='session/cancel':
                    cancelled=True
                    break
        result={'stopReason':'cancelled' if cancelled else 'end_turn'}
    else: result={}
    emit({'jsonrpc':'2.0','id':req['id'],'result':result})
"#;

async fn client(case: serde_json::Value) -> (AcpClient, ObserverHandle) {
    let observer = ObserverHandle::in_process();
    let mut client = AcpClient::spawn(
        "python3",
        &[
            "-u".into(),
            "-c".into(),
            ADAPTER.into(),
            case.to_string(),
            kimi_permission_tests::REQUEST.into(),
        ],
        &[],
        false,
    )
    .await
    .unwrap();
    client.set_observer(Some(observer.clone()), 0);
    client.initialize().await.unwrap();
    client
        .session_new("/tmp", vec![], None, None)
        .await
        .unwrap();
    client.configure_kimi_progress(Duration::from_millis(50));
    (client, observer)
}

async fn run(case: serde_json::Value) -> (Result<StopReason, AcpError>, Vec<ObserverEvent>) {
    let (mut client, observer) = client(case.clone()).await;
    let result = client
        .session_prompt_with_idle_timeout(
            "fixture-session",
            "fixture",
            Duration::from_millis(if case["silence"] == true { 90 } else { 500 }),
            Duration::from_millis(if case["hard"] == true { 140 } else { 1000 }),
        )
        .await;
    if case["cleanup"] == true {
        let drained = client
            .cancel_with_cleanup_grace("fixture-session", Duration::from_millis(500))
            .await;
        assert!(drained.is_ok(), "cleanup: {drained:?}");
    }
    client.shutdown().await;
    (result, observer.snapshot())
}

#[tokio::test]
async fn kimi_visibility_permission_only_foreground_and_background_survive_individual_deadline() {
    // The installed adapter omits both foreground/background subagent updates.
    // Returning the former unconditional progress deadline breaks this case.
    for background in [false, true] {
        let (result, events) = run(serde_json::json!({"background":background})).await;
        assert!(
            matches!(result, Ok(StopReason::EndTurn)),
            "background={background}: {result:?}"
        );
        assert!(events.iter().any(|e| e.kind == "permission_lifecycle"
            && e.payload["visibility"] == "compatibility_unobservable"));
        assert!(
            !events
                .iter()
                .any(|e| e.kind == "permission_lifecycle"
                    && e.payload["state"] == "progress_observed"),
            "completion is not lifecycle evidence"
        );
    }
}

#[tokio::test]
async fn kimi_visibility_observable_stall_is_not_reset_by_text_or_other_tools() {
    let (result, events) =
        run(serde_json::json!({"before":true,"wrong_tool":true,"wrong_session":true})).await;
    assert!(
        matches!(result, Err(AcpError::PermissionProgressTimeout)),
        "{result:?}"
    );
    assert!(events.iter().any(|e| e.kind == "permission_lifecycle"
        && e.payload["state"] == "watchdog_expired"
        && e.payload["visibility"] == "observable"
        && e.payload["reason"] == "observable_tool_timeout"
        && e.payload["lastMatchingEventAgeMs"].is_number()));
}

#[tokio::test]
async fn kimi_visibility_permission_before_create_acknowledges_observable_lifecycle() {
    // Matching execution acknowledgement removes only the acknowledgement wait.
    let (result, _) = run(serde_json::json!({"after":"in_progress"})).await;
    assert!(matches!(result, Ok(StopReason::EndTurn)), "{result:?}");
    let (result, events) = run(serde_json::json!({"after":"completed"})).await;
    assert!(matches!(result, Ok(StopReason::EndTurn)), "{result:?}");
    assert!(events.iter().any(|e| e.kind == "permission_lifecycle"
        && e.payload["state"] == "progress_observed"
        && e.payload["visibility"] == "observable"));
}

#[tokio::test]
async fn kimi_visibility_unknown_versions_bound_telemetry_without_claiming_tool_stall() {
    for version in ["2.5.1", "2.5.3", "2.5.20", "2.5.2+custom", "unknown"] {
        let (result, events) = run(serde_json::json!({"version":version})).await;
        let error = result.unwrap_err().to_string();
        assert!(error.contains("telemetry"), "version={version}: {error}");
        assert!(events.iter().any(|e| e.kind == "permission_lifecycle"
            && e.payload["state"] == "watchdog_expired"
            && e.payload["visibility"] == "unknown"
            && e.payload["reason"] == "telemetry_unavailable"));
    }
}

#[tokio::test]
async fn kimi_visibility_global_idle_and_hard_bounds_remain_active() {
    let (result, events) = run(serde_json::json!({"silence":true,"cleanup":true})).await;
    assert!(
        matches!(result, Err(AcpError::IdleTimeout(_))),
        "{result:?}"
    );
    assert!(events.iter().any(|e| e.kind == "prompt_stop"
        && e.payload["initiator"] == "deadline"
        && e.payload["reason"] == "idle_timeout"));
    assert_eq!(
        events.iter().filter(|e| e.kind == "prompt_stop").count(),
        1,
        "cleanup must retain the deadline initiator"
    );
    let (result, events) = run(serde_json::json!({"hard":true,"cleanup":true})).await;
    assert!(
        matches!(result, Err(AcpError::HardTimeout { .. })),
        "{result:?}"
    );
    assert!(events.iter().any(|e| e.kind == "prompt_stop"
        && e.payload["initiator"] == "deadline"
        && e.payload["reason"] == "hard_timeout"));
    assert_eq!(
        events.iter().filter(|e| e.kind == "prompt_stop").count(),
        1,
        "cleanup must retain the deadline initiator"
    );
}

#[tokio::test]
async fn kimi_visibility_explicit_cancel_drains_without_automatic_continuation() {
    let (mut client, observer) = client(serde_json::json!({})).await;
    // Cancel after the old per-tool bound, while the compatibility turn is alive.
    {
        let prompt = client.session_prompt_with_idle_timeout(
            "fixture-session",
            "fixture",
            Duration::from_secs(1),
            Duration::from_secs(2),
        );
        tokio::pin!(prompt);
        tokio::select! {
            result = &mut prompt => panic!("turn ended before user cancellation: {result:?}"),
            _ = tokio::time::sleep(Duration::from_millis(120)) => {},
        }
    }
    let result = client
        .cancel_with_cleanup("fixture-session", Duration::from_secs(1))
        .await;
    assert!(result.is_ok(), "{result:?}");
    client.shutdown().await;
    let events = observer.snapshot();
    assert_eq!(
        events
            .iter()
            .filter(|e| e.kind == "acp_write" && e.payload["method"] == "session/prompt")
            .count(),
        1
    );
    assert!(events
        .iter()
        .any(|e| e.kind == "permission_lifecycle" && e.payload["state"] == "cancelled"));
    assert!(events.iter().any(|e| e.kind == "prompt_stop"
        && e.payload["initiator"] == "user"
        && e.payload["reason"] == "explicit_cancel"));
}

#[tokio::test]
async fn kimi_visibility_same_tool_in_new_process_cannot_settle_old_unknown_effect() {
    let work = logical_work(Default::default());
    for case in [
        serde_json::json!({}),
        serde_json::json!({"after":"completed"}),
    ] {
        let (mut client, _) = client(case).await;
        client.configure_kimi_work(Some(&work), None);
        // Let both turns finish even under the old implementation; this isolates
        // effect identity from the separate permission-only deadline regression.
        client.configure_kimi_progress(Duration::from_millis(500));
        let result = client
            .session_prompt_with_idle_timeout(
                "fixture-session",
                "fixture",
                Duration::from_secs(1),
                Duration::from_secs(2),
            )
            .await;
        client.shutdown().await;
        assert!(matches!(result, Ok(StopReason::EndTurn)), "{result:?}");
        assert!(
            work.uncertain_effects(),
            "new process reused a tool ID and falsely settled the old effect"
        );
    }
}

#[tokio::test]
async fn kimi_visibility_observability_resets_between_prompts() {
    let (mut client, observer) =
        client(serde_json::json!({"before":true,"after":"completed","only_first":true})).await;
    for _ in 0..2 {
        let result = client
            .session_prompt_with_idle_timeout(
                "fixture-session",
                "fixture",
                Duration::from_secs(1),
                Duration::from_secs(2),
            )
            .await;
        assert!(matches!(result, Ok(StopReason::EndTurn)), "{result:?}");
    }
    client.shutdown().await;
    let completions: Vec<_> = observer
        .snapshot()
        .into_iter()
        .filter(|e| e.kind == "permission_lifecycle" && e.payload["state"] == "prompt_completed")
        .collect();
    assert_eq!(completions.len(), 1);
    assert_eq!(
        completions[0].payload["visibility"],
        "compatibility_unobservable"
    );
}

fn logical_work(policy: crate::queue::RecoveryPolicy) -> crate::queue::LogicalWork {
    use crate::{
        config::DedupMode,
        queue::{EventQueue, QueuedEvent},
        scope::SessionScope,
    };
    use nostr::{EventBuilder, Keys, Kind};
    let scope = SessionScope::Conversation {
        channel_id: uuid::Uuid::new_v4(),
    };
    let mut queue = EventQueue::new(DedupMode::Queue).with_kimi_recovery(policy, "/tmp");
    queue.push(QueuedEvent {
        channel_id: scope.channel_id(),
        scope,
        event: EventBuilder::new(Kind::Custom(9), "fixture")
            .sign_with_keys(&Keys::generate())
            .unwrap(),
        received_at: std::time::Instant::now(),
        prompt_tag: "test".into(),
        edit: None,
    });
    queue.flush_next().unwrap().work.unwrap()
}

#[tokio::test]
async fn kimi_visibility_operator_profile_flows_from_cli_to_runtime_watchdog() {
    use clap::Parser;
    for (policy, expected_profile, expected_error) in [
        ("observable", "observable_override", "no tool progress"),
        ("unknown", "unknown", "telemetry"),
    ] {
        let args = crate::config::CliArgs::try_parse_from([
            "buzz-acp",
            "--private-key",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "--kimi-permission-visibility",
            policy,
        ])
        .unwrap();
        let work = logical_work(args.kimi_recovery.policy().unwrap());
        let (mut client, observer) = client(serde_json::json!({})).await;
        client.configure_kimi_work(Some(&work), None);
        client.configure_kimi_progress(Duration::from_millis(50));
        let result = client
            .session_prompt_with_idle_timeout(
                "fixture-session",
                "fixture",
                Duration::from_secs(1),
                Duration::from_secs(2),
            )
            .await;
        client.shutdown().await;
        let error = result.unwrap_err();
        assert!(
            error.to_string().contains(expected_error),
            "{policy}: {error}"
        );
        assert!(crate::pool::PromptOutcome::Error(error).requires_retirement());
        assert!(observer
            .snapshot()
            .iter()
            .any(|e| e.kind == "permission_visibility_profile"
                && e.payload["profile"] == expected_profile));
    }
    assert!(crate::config::CliArgs::try_parse_from([
        "buzz-acp",
        "--private-key",
        "0000000000000000000000000000000000000000000000000000000000000001",
        "--kimi-permission-visibility",
        "all-future-kimi",
    ])
    .is_err());
}

#[tokio::test]
async fn kimi_visibility_statusless_create_proves_visibility_without_settling_effect() {
    let (result, events) = run(serde_json::json!({"after":"create"})).await;
    assert!(
        matches!(result, Err(AcpError::PermissionProgressTimeout)),
        "{result:?}"
    );
    assert!(events.iter().any(|e| e.kind == "permission_lifecycle"
        && e.payload["state"] == "watchdog_expired"
        && e.payload["visibility"] == "observable"));
}

#[tokio::test]
async fn kimi_visibility_acknowledged_main_tool_can_run_quietly_without_settling_effects() {
    // The verified CLI promises lifecycle, not periodic execution heartbeats.
    let mut failures = Vec::new();
    for case in [
        serde_json::json!({"after":"in_progress","quiet":true}),
        serde_json::json!({"before":"in_progress","quiet":true}),
        serde_json::json!({"before":true,"after":"progress","quiet":true}),
    ] {
        let work = logical_work(Default::default());
        let (mut client, observer) = client(case.clone()).await;
        client.configure_kimi_work(Some(&work), None);
        client.configure_kimi_progress(Duration::from_millis(50));
        let result = client
            .session_prompt_with_idle_timeout(
                "fixture-session",
                "fixture",
                Duration::from_secs(1),
                Duration::from_secs(2),
            )
            .await;
        client.shutdown().await;
        if !matches!(result, Ok(StopReason::EndTurn)) {
            failures.push(format!("{case}: {result:?}"));
        }
        assert!(
            work.uncertain_effects(),
            "acknowledgement must not settle the external effect"
        );
        if observer
            .snapshot()
            .iter()
            .any(|e| e.kind == "permission_lifecycle" && e.payload["state"] == "watchdog_expired")
        {
            failures.push(format!(
                "{case}: acknowledgement was treated as a heartbeat contract"
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
    // A title-only update is a verified progress signal only for the exact CLI.
    let (result, _) = run(serde_json::json!({"version":"2.5.3","after":"progress"})).await;
    assert!(
        matches!(result, Err(AcpError::PermissionTelemetryTimeout)),
        "{result:?}"
    );
}
