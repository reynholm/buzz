//! Exercise capture at the real prompt/read/result seam, not activity logging.
use super::*;
use std::time::Duration;
const PEER: &str = r#"
import json,sys
for line in sys.stdin:
 r=json.loads(line); m=r.get('method'); v={}
 if 'id' not in r: continue
 if m=='initialize':
  assert r['params']['clientCapabilities']['_meta']['buzz']['replyDelivery']['version']==1
  assert r['params']['clientCapabilities']['_meta']['buzz']['replyDelivery']['mode']=='runtime_final'
  v={'protocolVersion':1,'agentInfo':{'name':'qwen-code','version':'0.26.7'},'_meta':{'buzz':{'replyDelivery':{'version':1,'mode':'runtime_final'}}}}
 elif m=='session/new': v={'sessionId':'s'}
 elif m=='session/prompt':
  text=r['params']['prompt'][0]['text']
  delivery=r['params'].get('_meta',{}).get('buzz',{}).get('replyDelivery')
  assert (delivery is None) if text=='tools' else delivery==dict(version=1,mode='runtime_final')
  updates=[{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'Final question?'}}]
  if text=='thought': updates.append({'sessionUpdate':'agent_thought_chunk','content':{'type':'text','text':'private'}})
  updates.insert(0,{'sessionUpdate':'available_commands_update','availableCommands':[{'name':'synthetic','description':'x'*662000}]})
  updates.extend([{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':''},'_meta':{'executionLifecycle':{'v':1,'kind':'request','phase':'ended','outcome':'success'}}}, {'sessionUpdate':'usage_update','used':1,'size':100}])
  for u in updates: print(json.dumps({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'s','update':u}}),flush=True)
  if text=='error':
   print(json.dumps({'jsonrpc':'2.0','id':r['id'],'error':{'code':-32603,'message':'provider failed'}}),flush=True); continue
  v={'stopReason':'cancelled' if text=='cancel' else 'end_turn'}
 print(json.dumps({'jsonrpc':'2.0','id':r['id'],'result':v}),flush=True)
"#;

#[tokio::test]
async fn runtime_final_production_prompt_capture_excludes_failed_and_cancelled_partial_text() {
    let mut client = AcpClient::spawn(
        "python3",
        &["-u".into(), "-c".into(), PEER.into()],
        &[],
        false,
    )
    .await
    .unwrap();
    client.request_runtime_final(true);
    client.initialize().await.unwrap();
    client.configure_runtime_final(true).unwrap();
    client
        .session_new("/tmp", vec![], None, None)
        .await
        .unwrap();
    for (text, expected) in [
        ("normal", Some("Final question?")),
        ("thought", None),
        ("error", None),
        ("cancel", None),
    ] {
        let result = client
            .session_prompt_with_idle_timeout(
                "s",
                text,
                Duration::from_secs(2),
                Duration::from_secs(5),
            )
            .await;
        assert_eq!(
            client
                .take_runtime_final(matches!(result, Ok(StopReason::EndTurn)))
                .ok()
                .as_deref(),
            expected,
            "{text}"
        );
    }
    client.configure_runtime_final(false).unwrap();
    client
        .session_prompt_with_idle_timeout(
            "s",
            "tools",
            Duration::from_secs(2),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
    assert!(client.take_runtime_final(true).is_err());
    client.shutdown().await;
}

#[tokio::test]
async fn runtime_final_initialize_capability_is_independent_of_product_identity() {
    for (name, version) in [
        ("Kimi Code CLI", "2.5.2"),
        ("qwen-code", "99.0.0-b24.99"),
        ("other", "development"),
    ] {
        let peer = PEER.replace(
            "'name':'qwen-code','version':'0.26.7'",
            &format!("'name':'{name}','version':'{version}'"),
        );
        let mut client = AcpClient::spawn("python3", &["-u".into(), "-c".into(), peer], &[], false)
            .await
            .unwrap();
        client.request_runtime_final(true);
        client.initialize().await.unwrap();
        assert!(
            client.configure_runtime_final(true).is_ok(),
            "negotiated delivery must not depend on product identity: {name} {version}"
        );
        client.shutdown().await;
    }
}

#[tokio::test]
async fn runtime_final_product_identity_cannot_substitute_for_capability() {
    for capability in [
        "None",
        "{}",
        "{'version':2,'mode':'runtime_final'}",
        "{'version':1,'mode':'agent_tools'}",
        "{'version':'1','mode':'runtime_final'}",
        "{'version':True,'mode':'runtime_final'}",
        "{'version':1.0,'mode':'runtime_final'}",
        "{'version':1}",
    ] {
        let peer = PEER.replace("{'version':1,'mode':'runtime_final'}", capability);
        let mut client = AcpClient::spawn("python3", &["-u".into(), "-c".into(), peer], &[], false)
            .await
            .unwrap();
        client.request_runtime_final(true);
        client.initialize().await.unwrap();
        assert!(
            client.configure_runtime_final(true).is_err(),
            "invalid capability admitted: {capability}"
        );
        client.shutdown().await;
    }
}

#[tokio::test]
async fn runtime_final_unsolicited_capability_cannot_enable_default_tool_delivery() {
    let peer = PEER
        .replace("assert r['params']['clientCapabilities']['_meta']['buzz']['replyDelivery']['version']==1", "assert 'replyDelivery' not in r['params']['clientCapabilities']['_meta']['buzz']")
        .replace("assert r['params']['clientCapabilities']['_meta']['buzz']['replyDelivery']['mode']=='runtime_final'", "pass");
    let mut client = AcpClient::spawn("python3", &["-u".into(), "-c".into(), peer], &[], false)
        .await
        .unwrap();
    client.initialize().await.unwrap();
    assert!(client.configure_runtime_final(true).is_err());
    client.shutdown().await;
}
