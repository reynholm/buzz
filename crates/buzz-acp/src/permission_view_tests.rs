//! Consent must refer to the complete request that survives owner telemetry.
use super::*;
use acp::kimi_permission::{PermissionDecision, PermissionNotice};
use config::PermissionMode;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};

const PEER: &str = r#"
import json,sys
permission=json.loads(sys.argv[1])
for line in sys.stdin:
    req=json.loads(line);method=req.get('method')
    if 'id' not in req:continue
    if method=='initialize':result={'protocolVersion':1,'agentInfo':{'name':'Kimi Code CLI','version':'synthetic-owner-view'}}
    elif method=='session/new':result={'sessionId':'view-session'}
    elif method=='session/prompt':
        print(json.dumps(permission),flush=True)
        reply=json.loads(sys.stdin.readline())
        result={'stopReason':'end_turn'}
    else:result={}
    print(json.dumps({'jsonrpc':'2.0','id':req['id'],'result':result}),flush=True)
"#;

async fn permission_view_case(label: &str, tool: Value, complete: bool) {
    let observer = observer::ObserverHandle::in_process();
    let request = json!({
        "jsonrpc":"2.0", "id":0, "method":"session/request_permission",
        "params":{"sessionId":"view-session", "toolCall":tool,
            "options":[{"optionId":"allow-view-once","kind":"allow_once","name":"Allow once"}]}
    });
    let notices = Arc::new(AtomicUsize::new(0));
    let count = notices.clone();
    let (votes, decisions) = mpsc::channel(1);
    let app = axum::Router::new().route(
        "/events",
        axum::routing::post(move |axum::Json(event): axum::Json<nostr::Event>| {
            let count = count.clone();
            let votes = votes.clone();
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
                let _ = votes
                    .send(PermissionDecision {
                        token: Uuid::parse_str(token).unwrap(),
                        allow: true,
                    })
                    .await;
                axum::Json(json!({"accepted":true}))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let agent = nostr::Keys::generate();
    let owner = nostr::Keys::generate();
    let channel = Uuid::new_v4();
    let mut client = AcpClient::spawn(
        "python3",
        &["-u".into(), "-c".into(), PEER.into(), request.to_string()],
        &[],
        false,
    )
    .await
    .unwrap();
    client.set_observer(Some(observer.clone()), 0);
    client.set_observer_context(observer::context_for_turn(
        Some(channel),
        Some("view-session".into()),
        "view-turn".into(),
        chrono::Utc::now().to_rfc3339(),
    ));
    client.initialize().await.unwrap();
    client
        .session_new("/tmp", vec![], None, None)
        .await
        .unwrap();
    client.configure_kimi_policy(PermissionMode::Default);
    client.install_permission_decisions(decisions);
    client.configure_kimi_work(
        None,
        Some(PermissionNotice {
            rest: relay::RestClient {
                http: reqwest::Client::builder().no_proxy().build().unwrap(),
                base_url: format!("http://{address}"),
                keys: agent.clone(),
                auth_tag_json: None,
            },
            channel,
            tags: ThreadTags::default(),
        }),
    );
    let outcome = client
        .session_prompt_with_idle_timeout(
            "view-session",
            "Inspect the complete action",
            Duration::from_secs(3),
            Duration::from_secs(5),
        )
        .await;
    client.shutdown().await;
    server.abort();

    // Use the production queue, trimmer, signer and NIP-44 publisher. An
    // in-process snapshot alone still contains the oversized original.
    let (publisher, mut published) = RelayEventPublisher::test_pair();
    let mut queue = ObserverPublishQueue::default();
    let snapshot = observer.snapshot();
    let request_seq = snapshot
        .iter()
        .find(|event| {
            event.kind == "acp_read" && event.payload["method"] == "session/request_permission"
        })
        .unwrap()
        .seq;
    for event in snapshot {
        queue.ingest(event);
    }
    let mut owner_events = Vec::new();
    while let Some(frame) = queue.next_frame() {
        publish_relay_observer_event(
            &publisher,
            &agent,
            &agent.public_key().to_hex(),
            &owner.public_key().to_hex(),
            &owner.public_key(),
            frame,
        )
        .await;
        let event = tokio::time::timeout(Duration::from_secs(2), published.recv())
            .await
            .unwrap()
            .unwrap();
        event.verify().unwrap();
        let frame: Value = decrypt_observer_payload(&owner, &event).unwrap();
        if let Some(events) = frame.pointer("/payload/events").and_then(Value::as_array) {
            owner_events.extend(events.iter().cloned());
        } else {
            owner_events.push(frame);
        }
    }
    let viewed = owner_events
        .iter()
        .find(|event| event["kind"] == "acp_read" && event["seq"] == request_seq)
        .unwrap();
    let granted = owner_events.iter().any(|event| {
        event["kind"] == "acp_write"
            && event["payload"]["result"]["outcome"]["optionId"] == "allow-view-once"
    });
    if let Ok(directory) = std::env::var("BUZZ_TEST_PERMISSION_VIEW_ARTIFACT_DIR") {
        std::fs::write(
            std::path::Path::new(&directory).join(format!("R3_{label}.json")),
            serde_json::to_vec_pretty(&owner_events).unwrap(),
        )
        .unwrap();
    }
    if complete {
        assert_eq!(
            viewed["payload"], request,
            "{label}: approved request changed in transport"
        );
        assert!(outcome.is_ok(), "{label}: {outcome:?}");
        assert!(
            granted,
            "{label}: complete view did not allow the current request"
        );
        assert_eq!(notices.load(Ordering::SeqCst), 1);
    } else {
        assert_ne!(
            viewed["payload"], request,
            "fixture must reach actual trimming"
        );
        assert!(
            matches!(outcome, Err(acp::AcpError::PermissionViewUnavailable)),
            "{label}: unavailable full owner view allowed outcome {outcome:?}"
        );
        assert!(!granted, "{label}: truncated request was approved");
        assert_eq!(
            notices.load(Ordering::SeqCst),
            0,
            "{label}: offered an unsafe vote"
        );
        assert!(owner_events
            .iter()
            .any(|event| event["kind"] == "permission_lifecycle"
                && event["payload"]["state"] == "decision_view_unavailable"));
    }
}

#[tokio::test]
async fn kimi_manual_permission_requires_unabridged_published_owner_view() {
    let middle = "OWNER_MUST_SEE_THIS_ACTION";
    for (label, title, field, text, complete) in [
        (
            "complete",
            "Bash",
            "command",
            format!("{} {middle} {}", "a".repeat(15_000), "z".repeat(15_000)),
            true,
        ),
        (
            "bash",
            "Bash",
            "command",
            format!("{} {middle} {}", "a".repeat(50_000), "z".repeat(50_000)),
            false,
        ),
        (
            "write",
            "Write",
            "content",
            format!("{} {middle} {}", "a".repeat(50_000), "z".repeat(50_000)),
            false,
        ),
        (
            "patch",
            "Patch",
            "patch",
            format!("{} {middle} {}", "a".repeat(50_000), "z".repeat(50_000)),
            false,
        ),
        (
            "escaped",
            "Bash",
            "command",
            format!("{} {middle} {}", "\0".repeat(6_000), "\0".repeat(6_000)),
            false,
        ),
    ] {
        permission_view_case(
            label,
            json!({
                "toolCallId":"view-tool", "title":title, "rawInput":{field:text},
            }),
            complete,
        )
        .await;
    }
    let fields: serde_json::Map<String, Value> = (0..24)
        .map(|i| (format!("part-{i}"), Value::String("x".repeat(3_000))))
        .collect();
    permission_view_case(
        "aggregate",
        json!({
            "toolCallId":"view-tool", "title":"Write", "rawInput":fields,
        }),
        false,
    )
    .await;
}
