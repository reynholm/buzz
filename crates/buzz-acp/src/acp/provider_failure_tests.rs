use super::*;
use std::time::Duration;

// Match codex-acp's negotiated failure response. Without the capability it
// emits ordinary text followed by end_turn, which cannot authorize a retry.
pub(crate) const ADAPTER: &str = r#"
import json, os, sys
typed = False
attempts = 0
for line in sys.stdin:
    req = json.loads(line)
    method = req.get('method')
    if 'id' not in req:
        continue
    result = {}
    if method == 'initialize':
        caps = req['params']['clientCapabilities']
        air = caps.get('_meta', {}).get('jetbrains', {}).get('air', {})
        typed = air.get('version') == 1 and 'sessionFailure' in air.get('capabilities', [])
        result = {'protocolVersion': 2, 'agentInfo': {'name': '@agentclientprotocol/codex-acp', 'version': '2'}}
    elif method == 'session/new':
        result = {'sessionId': 'provider-session'}
    elif method == 'session/prompt':
        text = req['params']['prompt'][0]['text']
        if os.environ.get('PROMPTS_LOG'):
            with open(os.environ['PROMPTS_LOG'], 'a') as out:
                out.write(json.dumps(req['params']) + '\n')
        if os.environ.get('FAIL_FIRST') == '1':
            text = 'network'
        result = {'stopReason': 'cancelled' if text == 'cancel' else 'end_turn'}
        attempts += 1
        if text == 'network' and attempts > 1:
            text = 'success'
        if typed and text not in ('success', 'legacy'):
            failure = {'id': 'turn:error', 'revision': 1, 'category': 'access' if text == 'auth' else 'service',
                       'severity': 'warning' if text == 'warning' else 'error',
                       'title': 'stream disconnected before completion', 'actions': ['login'] if text == 'auth' else ['retry']}
            result['_meta'] = {'jetbrains': {'air': {'version': 1, 'sessionFailure': failure}}}
            if text == 'malformed':
                del failure['severity']
            if text == 'future':
                result['_meta']['jetbrains']['air']['version'] = 2
        if not typed and text == 'network':
            print(json.dumps({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'provider-session','update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'stream disconnected before completion'}}}}), flush=True)
    print(json.dumps({'jsonrpc':'2.0', 'id':req['id'], 'result':result}), flush=True)
"#;

async fn adapter() -> AcpClient {
    let mut client = AcpClient::spawn(
        "python3",
        &["-u".into(), "-c".into(), ADAPTER.into()],
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
    client
}

async fn prompt(client: &mut AcpClient, text: &str) -> Result<StopReason, AcpError> {
    client
        .session_prompt_with_idle_timeout(
            "provider-session",
            text,
            Duration::from_secs(2),
            Duration::from_secs(5),
        )
        .await
}

#[tokio::test]
async fn provider_failure_is_an_application_error_and_same_pipe_can_retry() {
    let mut client = adapter().await;
    let first = prompt(&mut client, "network").await;
    let second = prompt(&mut client, "network").await;
    client.shutdown().await;
    assert!(
        matches!(first, Err(AcpError::AgentError { code: -32603, ref message }) if message.contains("stream disconnected")),
        "a provider failure must reach the queue retry path: {first:?}"
    );
    assert_eq!(second.unwrap(), StopReason::EndTurn);
}

#[tokio::test]
async fn provider_warning_and_legacy_success_do_not_fail_a_turn() {
    let mut client = adapter().await;
    for text in ["warning", "legacy", "success"] {
        assert_eq!(
            prompt(&mut client, text).await.unwrap(),
            StopReason::EndTurn
        );
    }
    client.shutdown().await;
}

#[tokio::test]
async fn provider_auth_failure_uses_the_existing_auth_error_class() {
    let mut client = adapter().await;
    let result = prompt(&mut client, "auth").await;
    client.shutdown().await;
    let error = result.unwrap_err();
    assert!(matches!(error, AcpError::AgentError { code: -32000, .. }));
    assert!(
        crate::is_auth_error(&error),
        "authentication failures must not retry"
    );
}

#[tokio::test]
async fn provider_failure_does_not_override_explicit_cancellation() {
    let mut client = adapter().await;
    let result = prompt(&mut client, "cancel").await;
    client.shutdown().await;
    assert_eq!(result.unwrap(), StopReason::Cancelled);
}

#[tokio::test]
async fn provider_failure_metadata_cannot_silently_claim_success_when_invalid() {
    let mut client = adapter().await;
    for text in ["malformed", "future"] {
        assert!(matches!(
            prompt(&mut client, text).await,
            Err(AcpError::Protocol(_))
        ));
    }
    client.shutdown().await;
}
