use super::*;
use crate::observer::{ObserverEvent, ObserverHandle};
use std::time::Duration;

pub(super) const REQUEST: &str = include_str!("../../test-fixtures/kimi/permission_request.json");
pub(super) const ADAPTER: &str = r#"
import json,sys,time,os
permission=json.loads(sys.argv[1]); mode=sys.argv[2]
for line in sys.stdin:
    req=json.loads(line); method=req.get('method')
    if 'id' not in req: continue
    if method=='initialize': result={'protocolVersion':1,'agentInfo':{'name':'Kimi Code CLI','version':'synthetic-permission-fault'}}
    elif method=='session/new': result={'sessionId':'fixture-session'}
    elif method=='session/prompt':
        if mode=='no_request':
            while True:
                print(json.dumps({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'fixture-session','update':{'sessionUpdate':'session_info_update'}}}),flush=True)
                time.sleep(.001)
        if mode=='wrong_session': permission['params']['sessionId']='another-session'
        if mode=='malformed_option': permission['params']['options']=[{'kind':'reject_once'}]
        if mode=='write_failure': os.close(0)
        print(json.dumps(permission),flush=True)
        if mode=='crash_permission': sys.exit(17)
        if mode=='write_failure':
            while True: time.sleep(1)
        reply=json.loads(sys.stdin.readline())
        if mode in ('no_progress','progress_then_noise'):
            if mode=='progress_then_noise':
                print(json.dumps({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'fixture-session','update':{'sessionUpdate':'tool_call_update','toolCallId':'0:tool_fixture','status':'completed'}}}),flush=True)
            while True:
                print(json.dumps({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'fixture-session','update':{'sessionUpdate':'session_info_update'}}}),flush=True)
                time.sleep(.001)
        if mode=='duplicate':
            permission['params']['toolCall']['toolCallId']='0:tool_different'
            print(json.dumps(permission),flush=True)
            reply=json.loads(sys.stdin.readline())
        result={'stopReason':'end_turn'}
    else: result={}
    print(json.dumps({'jsonrpc':'2.0','id':req['id'],'result':result}),flush=True)
"#;

async fn permission(mode: &str) -> (Result<StopReason, AcpError>, Vec<ObserverEvent>) {
    let observer = ObserverHandle::in_process();
    let mut client = AcpClient::spawn(
        "python3",
        &[
            "-u".into(),
            "-c".into(),
            ADAPTER.into(),
            REQUEST.into(),
            mode.into(),
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
    if mode == "policy_dont_ask" {
        client.configure_kimi_policy(crate::config::PermissionMode::DontAsk);
    } else if mode == "policy_manual_no_listener" {
        client.configure_kimi_policy(crate::config::PermissionMode::Default);
    }
    let result = client
        .session_prompt_with_idle_timeout(
            "fixture-session",
            "safe fixture",
            Duration::from_secs(2),
            Duration::from_millis(300),
        )
        .await;
    client.shutdown().await;
    (result, observer.snapshot())
}

#[tokio::test]
async fn kimi_written_permission_without_tool_progress_has_a_separate_deadline() {
    let (result, events) = permission("no_progress").await;
    assert!(
        matches!(result, Err(AcpError::PermissionProgressTimeout)),
        "{result:?}"
    );
    let states: Vec<_> = events
        .iter()
        .filter(|event| event.kind == "permission_lifecycle")
        .filter_map(|event| event.payload["state"].as_str())
        .collect();
    assert!(states.contains(&"response_written"));
    assert!(!states.contains(&"progress_observed"));
    assert_eq!(states.last(), Some(&"prompt_failed"));
}

#[cfg(unix)]
#[tokio::test]
async fn kimi_eof_at_permission_can_be_checked_retired_from_live_evidence() {
    let mut client = AcpClient::spawn(
        "python3",
        &[
            "-u".into(),
            "-c".into(),
            ADAPTER.into(),
            REQUEST.into(),
            "crash_permission".into(),
        ],
        &[],
        false,
    )
    .await
    .unwrap();
    client.initialize().await.unwrap();
    client
        .session_new("/tmp", vec![], None, None)
        .await
        .unwrap();
    let outcome = client
        .session_prompt_with_idle_timeout(
            "fixture-session",
            "fixture",
            Duration::from_secs(1),
            Duration::from_secs(2),
        )
        .await;
    assert!(
        matches!(outcome, Err(AcpError::AgentExited | AcpError::Io(_))),
        "{outcome:?}"
    );
    assert_eq!(client.terminate().await, TerminationReport::Clean);
}

#[tokio::test]
async fn kimi_real_tool_progress_clears_permission_watchdog_but_not_hard_deadline() {
    let (result, events) = permission("progress_then_noise").await;
    assert!(
        matches!(result, Err(AcpError::HardTimeout { .. })),
        "{result:?}"
    );
    assert!(events
        .iter()
        .any(|event| event.kind == "permission_lifecycle"
            && event.payload["state"] == "progress_observed"));
}

#[tokio::test]
async fn kimi_silence_without_permission_does_not_invent_a_pending_request() {
    let (result, events) = permission("no_request").await;
    assert!(
        matches!(result, Err(AcpError::HardTimeout { .. })),
        "{result:?}"
    );
    assert!(!events
        .iter()
        .any(|event| event.kind == "permission_lifecycle"));
}

#[tokio::test]
async fn kimi_permission_write_failure_has_a_bounded_failed_outcome() {
    let (result, events) = permission("write_failure").await;
    assert!(matches!(result, Err(AcpError::Io(_))), "{result:?}");
    assert!(events
        .iter()
        .any(|event| event.kind == "permission_lifecycle"
            && event.payload["state"] == "response_write_failed"));
    assert!(!events
        .iter()
        .any(|event| event.kind == "permission_lifecycle"
            && event.payload["state"] == "response_written"));
}

#[tokio::test]
async fn kimi_permission_never_fabricates_an_option_id() {
    let (result, events) = permission("malformed_option").await;
    assert!(matches!(result, Err(AcpError::Protocol(_))), "{result:?}");
    assert!(approvals(&events).is_empty());
    assert!(!events.iter().any(|event| event.kind == "acp_write"
        && event.payload.pointer("/result/outcome/optionId").is_some()));
}

#[tokio::test]
async fn kimi_permission_respects_explicit_reject_and_manual_policy() {
    let (result, events) = permission("policy_dont_ask").await;
    assert_eq!(result.unwrap(), StopReason::EndTurn);
    assert!(approvals(&events).is_empty());
    assert!(events.iter().any(|event| event.kind == "acp_write"
        && event
            .payload
            .pointer("/result/outcome/optionId")
            .and_then(serde_json::Value::as_str)
            == Some("reject")));
    let (result, events) = permission("policy_manual_no_listener").await;
    assert!(
        result.is_err(),
        "manual policy without a decision path must fail closed"
    );
    assert!(approvals(&events).is_empty());
}

fn approvals(events: &[ObserverEvent]) -> Vec<&serde_json::Value> {
    events
        .iter()
        .filter(|event| {
            event.kind == "acp_write"
                && event
                    .payload
                    .pointer("/result/outcome/outcome")
                    .and_then(serde_json::Value::as_str)
                    == Some("selected")
                && event
                    .payload
                    .pointer("/result/outcome/optionId")
                    .and_then(serde_json::Value::as_str)
                    == Some("approve_once")
        })
        .map(|event| &event.payload)
        .collect()
}

#[tokio::test]
async fn kimi_permission_from_another_session_is_never_approved() {
    let (_, events) = permission("wrong_session").await;
    assert!(
        approvals(&events).is_empty(),
        "wrong-session permission received a grant"
    );
}

#[tokio::test]
async fn kimi_observed_permission_uses_offered_option_and_same_numeric_request_id() {
    let (result, events) = permission("success").await;
    assert_eq!(result.unwrap(), StopReason::EndTurn);
    let selected = approvals(&events);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0]["id"], 0);
}

#[tokio::test]
async fn kimi_duplicate_permission_id_cannot_authorize_a_different_tool_call() {
    let (_, events) = permission("duplicate").await;
    assert_eq!(
        approvals(&events).len(),
        1,
        "a reused request ID granted another action"
    );
}
