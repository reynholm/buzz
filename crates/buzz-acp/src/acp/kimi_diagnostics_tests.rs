//! Plaintext diagnostics and the owner's encrypted request view are distinct.
use super::*;
use crate::observer::ObserverHandle;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Default)]
struct LogCapture(Arc<Mutex<Vec<u8>>>);

impl Write for LogCapture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

const PEER: &str = r#"
import json,sys
kimi=sys.argv[1]=='1';steer=sys.argv[2]
for line in sys.stdin:
    req=json.loads(line);method=req.get('method')
    if 'id' not in req:continue
    if method=='initialize':
        result={'protocolVersion':1,'agentInfo':{'name':'Kimi Code CLI' if kimi else 'other-adapter','version':'fixture'},'diagnosticSecret':'INITIALIZE_DIAGNOSTIC_SECRET','_meta':{'steering':{'supported':bool(steer)}}}
    elif method=='session/new':result={'sessionId':'diagnostic-session'}
    elif method=='session/prompt':
        params={'sessionId':'diagnostic-session','toolCall':{'toolCallId':'diagnostic-tool','title':'Bash','rawInput':{'command':'curl -H "Authorization: Bearer PERMISSION_DIAGNOSTIC_SECRET" https://example.invalid'},'content':[{'type':'content','content':{'type':'text','text':'PERMISSION_DIAGNOSTIC_SECRET'}}]},'options':[{'optionId':'approve_once','kind':'allow_once','name':'Allow once'}]}
        print(json.dumps({'jsonrpc':'2.0','id':0,'method':'session/request_permission','params':params}),flush=True)
        approved=False;steered=not bool(steer)
        while not (approved and steered):
            reply=json.loads(sys.stdin.readline())
            if reply.get('method')=='_session/steering':
                steered=True
                response={'jsonrpc':'2.0','id':reply['id']}
                if steer=='error':response['error']={'code':-32603,'message':'STEER_ERROR_DIAGNOSTIC_SECRET','data':{'token':'STEER_ERROR_DIAGNOSTIC_SECRET'}}
                elif steer=='outcome':response['result']={'outcome':{'token':'STEER_OUTCOME_DIAGNOSTIC_SECRET'}}
                else:response['result']={'outcome':'injected'}
                print(json.dumps(response),flush=True)
            else:
                assert reply['result']['outcome']['optionId']=='approve_once'
                approved=True
        for update in [
            {'sessionUpdate':'tool_call','toolCallId':'diagnostic-tool','title':'TOOL_TITLE_DIAGNOSTIC_SECRET','kind':'execute'},
            {'sessionUpdate':'tool_call_update','toolCallId':'diagnostic-tool','status':'completed'},
            {'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'MESSAGE_DIAGNOSTIC_SECRET'}},
            {'sessionUpdate':'agent_thought_chunk','content':{'type':'text','text':'THOUGHT_DIAGNOSTIC_SECRET'}},
        ]:
            print(json.dumps({'jsonrpc':'2.0','method':'session/update','params':{'sessionId':'diagnostic-session','update':update}}),flush=True)
        result={'stopReason':'end_turn'}
    else:result={}
    print(json.dumps({'jsonrpc':'2.0','id':req['id'],'result':result}),flush=True)
"#;

async fn diagnostics(kimi: bool, steer: &str) -> (String, String) {
    let capture = LogCapture::default();
    let writer = capture.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .without_time()
        .with_writer(move || writer.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let observer = ObserverHandle::in_process();
    let mut client = AcpClient::spawn(
        "python3",
        &[
            "-u".into(),
            "-c".into(),
            PEER.into(),
            u8::from(kimi).to_string(),
            steer.into(),
        ],
        &[],
        false,
    )
    .await
    .unwrap();
    client.set_observer(Some(observer.clone()), 0);
    client.initialize().await.unwrap();
    client
        .session_new(
            "/tmp",
            vec![McpServer {
                name: "fixture".into(),
                command: "unused".into(),
                args: vec![],
                env: vec![EnvVar {
                    name: "FIXTURE_SECRET".into(),
                    value: "MCP_DIAGNOSTIC_SECRET".into(),
                }],
            }],
            None,
            None,
        )
        .await
        .unwrap();
    let (steer_tx, steer_rx) = tokio::sync::mpsc::channel(1);
    let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
    if !steer.is_empty() {
        client.install_steer_rx(steer_rx);
        steer_tx
            .send(crate::pool::SteerRequest {
                prompt_blocks: vec!["STEER_DIAGNOSTIC_SECRET".into()],
                ack_tx,
            })
            .await
            .unwrap();
    }
    let result = client
        .session_prompt_with_idle_timeout(
            "diagnostic-session",
            "PROMPT_DIAGNOSTIC_SECRET",
            Duration::from_secs(2),
            Duration::from_secs(5),
        )
        .await;
    client.shutdown().await;
    if !steer.is_empty() {
        let ack = ack_rx.await.unwrap();
        // The production main loop also formats this real read-loop ack with Debug.
        tracing::info!(?ack, "observed real ACP steer acknowledgement");
        match steer {
            "error" => assert!(matches!(
                ack,
                crate::pool::SteerAck::Err(crate::pool::SteerError::AgentError {
                    code: -32603,
                    ..
                })
            )),
            "outcome" => assert!(matches!(
                ack,
                crate::pool::SteerAck::Err(crate::pool::SteerError::OutcomeRejected { .. })
            )),
            _ => assert!(matches!(ack, crate::pool::SteerAck::Success { .. })),
        }
    }
    assert_eq!(result.unwrap(), StopReason::EndTurn);
    let text = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    (text, serde_json::to_string(&observer.snapshot()).unwrap())
}

#[tokio::test]
async fn kimi_plaintext_logs_keep_structure_without_copying_private_request_content() {
    let (text, owner_view) = diagnostics(true, "").await;
    for marker in [
        "INITIALIZE_DIAGNOSTIC_SECRET",
        "MCP_DIAGNOSTIC_SECRET",
        "PROMPT_DIAGNOSTIC_SECRET",
        "PERMISSION_DIAGNOSTIC_SECRET",
        "TOOL_TITLE_DIAGNOSTIC_SECRET",
        "MESSAGE_DIAGNOSTIC_SECRET",
        "THOUGHT_DIAGNOSTIC_SECRET",
    ] {
        assert!(
            !text.contains(marker),
            "plaintext diagnostic copied {marker}"
        );
    }
    for identity in [
        "session/prompt",
        "session/request_permission",
        "diagnostic-session",
        "diagnostic-tool",
    ] {
        assert!(
            text.contains(identity),
            "missing safe correlation {identity}"
        );
    }
    assert!(
        owner_view.contains("PERMISSION_DIAGNOSTIC_SECRET"),
        "the owner must still inspect the concrete permission request"
    );
}

#[tokio::test]
async fn other_adapter_plaintext_diagnostics_keep_their_existing_behavior() {
    let (text, owner_view) = diagnostics(false, "").await;
    assert!(text.contains("PERMISSION_DIAGNOSTIC_SECRET"));
    assert!(text.contains("MCP_DIAGNOSTIC_SECRET"));
    assert!(owner_view.contains("PERMISSION_DIAGNOSTIC_SECRET"));
}

#[tokio::test]
async fn kimi_native_steer_keeps_safe_logs_and_ack_variants() {
    let mut violations = vec![];
    for mode in ["injected", "error", "outcome"] {
        let (text, owner_view) = diagnostics(true, mode).await;
        for marker in [
            "STEER_DIAGNOSTIC_SECRET",
            "STEER_ERROR_DIAGNOSTIC_SECRET",
            "STEER_OUTCOME_DIAGNOSTIC_SECRET",
        ] {
            if text.contains(marker) {
                violations.push(format!("{mode}: {marker}"));
            }
        }
        assert!(text.contains("_session/steering"));
        assert!(owner_view.contains("STEER_DIAGNOSTIC_SECRET"));
    }
    assert!(
        violations.is_empty(),
        "steer plaintext copied {violations:?}"
    );
}

#[tokio::test]
async fn kimi_manual_permission_without_request_view_does_not_offer_or_accept_a_vote() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let notices = Arc::new(AtomicUsize::new(0));
    let count = notices.clone();
    let (decisions, rx) = tokio::sync::mpsc::channel(1);
    let app = axum::Router::new().route(
        "/events",
        axum::routing::post(move |axum::Json(event): axum::Json<nostr::Event>| {
            let count = count.clone();
            let decisions = decisions.clone();
            async move {
                event.verify().unwrap();
                count.fetch_add(1, Ordering::SeqCst);
                let token = event
                    .content
                    .split("!approve ")
                    .nth(1)
                    .unwrap()
                    .split('`')
                    .next()
                    .unwrap();
                decisions
                    .send(kimi_permission::PermissionDecision {
                        token: uuid::Uuid::parse_str(token).unwrap(),
                        allow: true,
                    })
                    .await
                    .unwrap();
                axum::Json(serde_json::json!({"accepted":true}))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let mut client = AcpClient::spawn(
        "python3",
        &[
            "-u".into(),
            "-c".into(),
            kimi_permission_tests::ADAPTER.into(),
            kimi_permission_tests::REQUEST.into(),
            "success".into(),
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
    client.configure_kimi_policy(crate::config::PermissionMode::Default);
    client.install_permission_decisions(rx);
    client.configure_kimi_work(
        None,
        Some(kimi_permission::PermissionNotice {
            rest: crate::relay::RestClient {
                http: reqwest::Client::builder().no_proxy().build().unwrap(),
                base_url: format!("http://{address}"),
                keys: nostr::Keys::generate(),
                auth_tag_json: None,
            },
            channel: uuid::Uuid::new_v4(),
            tags: crate::queue::ThreadTags::default(),
        }),
    );
    let result = client
        .session_prompt_with_idle_timeout(
            "fixture-session",
            "safe fixture",
            Duration::from_secs(2),
            Duration::from_secs(5),
        )
        .await;
    client.shutdown().await;
    server.abort();
    assert!(
        matches!(result,Err(ref error) if error.to_string().to_lowercase().contains("activity")),
        "{result:?}"
    );
    assert_eq!(
        notices.load(Ordering::SeqCst),
        0,
        "do not invite blind approval before an exact request view exists"
    );
}
