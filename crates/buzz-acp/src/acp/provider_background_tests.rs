use super::*;

#[tokio::test]
async fn provider_background_contract_is_advertised_with_an_explicit_version() {
    let script = r#"
import json,sys
for line in sys.stdin:
    r=json.loads(line)
    if r.get('method')=='initialize':
        c=r['params']['clientCapabilities'].get('_meta',{}).get('buzz',{}).get('backgroundTasks',{})
        print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':{'protocolVersion':2,'advertised':c}}),flush=True)
"#;
    let mut client = AcpClient::spawn(
        "python3",
        &["-u".into(), "-c".into(), script.into()],
        &[],
        false,
    )
    .await
    .unwrap();
    let result = client.initialize().await.unwrap();
    client.shutdown().await;
    assert_eq!(result["advertised"]["version"], 1);
}

#[tokio::test]
async fn provider_background_cancel_ack_preserves_racing_prompt_response() {
    let script = r#"
import json,sys
for line in sys.stdin:
    r=json.loads(line)
    if r.get('method')=='_buzz/background_tasks/cancel':
        print(json.dumps({'jsonrpc':'2.0','id':99,'result':{'stopReason':'cancelled'}}),flush=True)
        print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':{'durable':True}}),flush=True)
"#;
    let mut client = AcpClient::spawn(
        "python3",
        &["-u".into(), "-c".into(), script.into()],
        &[],
        false,
    )
    .await
    .unwrap();
    client.last_prompt_id = Some(99);
    let ack = client
        .send_request("_buzz/background_tasks/cancel", serde_json::json!({}))
        .await
        .unwrap();
    assert_eq!(ack["durable"], true);
    let stop = client
        .cancel_with_cleanup_grace("original-session", std::time::Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(stop, StopReason::Cancelled);
    assert!(client.deferred_prompt_response.is_none());
    client.shutdown().await;
}
