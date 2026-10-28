//! Synthetic failures over production ACP pipes; not incident/provider traces.
use super::*;
use serde_json::json;
use std::time::Duration;

const SYNTHETIC_ADAPTER: &str = r#"
import json,sys
for line in sys.stdin:
    req=json.loads(line)
    method=req.get('method')
    if 'id' not in req: continue
    if method=='initialize':
        result={'protocolVersion':1,'agentInfo':{'name':'Kimi Code CLI','version':'synthetic'}}
    elif method=='session/new': result={'sessionId':'synthetic-kimi-session'}
    elif method=='session/prompt':
        print(json.dumps({'jsonrpc':'2.0','id':req['id'],'error':json.loads(req['params']['prompt'][0]['text'])}),flush=True)
        continue
    else: result={}
    print(json.dumps({'jsonrpc':'2.0','id':req['id'],'result':result}),flush=True)
"#;

async fn rpc_failure(value: serde_json::Value, kimi: bool) -> AcpError {
    let script = if kimi {
        SYNTHETIC_ADAPTER.to_string()
    } else {
        SYNTHETIC_ADAPTER.replace("Kimi Code CLI", "other-adapter")
    };
    let mut client = AcpClient::spawn("python3", &["-u".into(), "-c".into(), script], &[], false)
        .await
        .unwrap();
    client.initialize().await.unwrap();
    client
        .session_new("/tmp", vec![], None, None)
        .await
        .unwrap();
    let result = client
        .session_prompt_with_idle_timeout(
            "synthetic-kimi-session",
            &value.to_string(),
            Duration::from_secs(2),
            Duration::from_secs(5),
        )
        .await;
    client.shutdown().await;
    result.unwrap_err()
}

#[tokio::test]
async fn kimi_structured_failure_survives_generic_rpc_message_without_secret_logging() {
    let error = rpc_failure(json!({
        "code": -32603, "message": "Internal error",
        "data": {"name":"OAuthConnectionError", "message":"OAuth endpoint connection timed out", "credentials":"must-not-leak"}
    }), true).await;
    assert!(
        error.to_string().contains("temporary OAuth connection"),
        "{error}"
    );
    assert!(format!("{error:?}").contains("OAuthConnectionError"));
    assert!(!format!("{error:?}").contains("must-not-leak"));
}

#[tokio::test]
async fn other_adapter_rpc_error_keeps_its_existing_contract() {
    let error = rpc_failure(
        json!({
            "code": -32603, "message": "Internal error",
            "data": {"name":"OAuthConnectionError", "message":"OAuth endpoint connection timed out"}
        }),
        false,
    )
    .await;
    assert!(
        matches!(error, AcpError::AgentError { code:-32603, ref message } if message=="Internal error")
    );
}

#[tokio::test]
async fn kimi_failure_table_preserves_auth_precedence_and_exact_fallbacks() {
    for (value, expected, auth) in [
        (
            json!({"code":-32603,"message":"OAuthError: Unable to acquire OAuth refresh lock"}),
            "temporary OAuth refresh lock",
            false,
        ),
        (
            json!({"code":-32603,"message":"OAuthConnectionError: OAuth endpoint connection timed out"}),
            "temporary OAuth connection",
            false,
        ),
        (
            json!({"code":-32603,"message":"APITimeoutError: request timed out"}),
            "temporary network timeout",
            false,
        ),
        (
            json!({"code":-32603,"message":"Internal error","data":{"type":"APITimeoutError","message":"request timed out"}}),
            "temporary network timeout",
            false,
        ),
        (
            json!({"code":-32603,"message":"OAuthError: different failure"}),
            "unclassified failure",
            false,
        ),
        (
            json!({"code":-32603,"message":"an unrelated timeout"}),
            "unclassified failure",
            false,
        ),
        (
            json!({"code":-32603,"message":"Internal error","data":{"name":"OAuthConnectionError","status":401,"message":"connection timed out"}}),
            "authentication failed",
            true,
        ),
        (
            json!({"code":-32603,"message":"Internal error","data":{"name":"OAuthConnectionError","statusCode":403,"message":"connection timed out"}}),
            "authentication failed",
            true,
        ),
        (
            json!({"code":-32603,"message":"OAuthConnectionError: Re-authenticate after timeout"}),
            "authentication failed",
            true,
        ),
        (
            json!({"code":-32603,"message":"OAuthConnectionError: unauthorized connection timeout"}),
            "authentication failed",
            true,
        ),
        (
            json!({"code":-32603,"message":"OAuthConnectionError: credentials revoked"}),
            "authentication failed",
            true,
        ),
        (
            json!({"code":-32603,"message":"OAuthConnectionError: login required"}),
            "authentication failed",
            true,
        ),
        (
            json!({"code":-32603,"message":"model not found"}),
            "configuration failed",
            false,
        ),
        (
            json!({"code":-32603,"message":"Internal error","data":{"name":"ConfigurationError","message":"invalid endpoint"}}),
            "configuration failed",
            false,
        ),
    ] {
        let error = rpc_failure(value.clone(), true).await;
        assert!(
            error.to_string().contains(expected),
            "input={value}, error={error}"
        );
        assert_eq!(
            crate::is_auth_error(&error),
            auth,
            "input={value}, error={error}"
        );
    }
}
