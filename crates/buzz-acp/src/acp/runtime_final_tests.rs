//! Exercise capture at the real prompt/read/result seam, not activity logging.
use super::*;
use std::time::Duration;
const PEER: &str = r#"
import json,sys
for line in sys.stdin:
 r=json.loads(line); m=r.get('method'); v={}
 if 'id' not in r: continue
 if m=='initialize': v={'protocolVersion':1,'agentInfo':{'name':'qwen-code','version':'0.26.7'}}
 elif m=='session/new': v={'sessionId':'s'}
 elif m=='session/prompt':
  text=r['params']['prompt'][0]['text']
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
    client.initialize().await.unwrap();
    let version = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(version.path(), "0.26.7-b24.8").unwrap();
    client.set_runtime_build_file(Some(version.path().to_owned()));
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
    std::fs::write(version.path(), "0.26.7-b24.9").unwrap();
    let upgraded = client
        .session_prompt_with_idle_timeout(
            "s",
            "normal",
            Duration::from_secs(2),
            Duration::from_secs(5),
        )
        .await;
    assert!(
        upgraded.is_err(),
        "an unverified in-process upgrade must block the next provider prompt"
    );
    client.shutdown().await;
}

#[tokio::test]
async fn runtime_final_initialize_gate_rejects_kimi_and_unverified_adapter_versions() {
    for (name, version) in [
        ("Kimi Code CLI", "2.5.2"),
        ("qwen-code", "0.26.8"),
        ("other", "0.26.7"),
    ] {
        let peer = PEER.replace(
            "'name':'qwen-code','version':'0.26.7'",
            &format!("'name':'{name}','version':'{version}'"),
        );
        let mut client = AcpClient::spawn("python3", &["-u".into(), "-c".into(), peer], &[], false)
            .await
            .unwrap();
        client.initialize().await.unwrap();
        assert!(
            client.configure_runtime_final(true).is_err(),
            "unsupported runtime profile: {name} {version}"
        );
        client.shutdown().await;
    }
}
