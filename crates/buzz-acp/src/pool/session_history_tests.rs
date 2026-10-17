use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn scripted(
    events: Vec<Value>,
) -> (
    PromptContext,
    tokio::task::JoinHandle<()>,
    std::sync::Arc<std::sync::Mutex<Vec<Value>>>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut ctx = super::super::tests::make_prompt_context_no_owner();
    ctx.rest_client.base_url = format!("http://{}", listener.local_addr().unwrap());
    let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = requests.clone();
    let server = tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut request = Vec::new();
            let header_end = loop {
                let mut chunk = [0; 8192];
                let n = socket.read(&mut chunk).await.unwrap();
                if n == 0 {
                    return;
                }
                request.extend_from_slice(&chunk[..n]);
                if let Some(at) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    break at + 4;
                }
            };
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|n| n.parse::<usize>().ok())
                })
                .unwrap_or(0);
            while request.len() < header_end + length {
                let mut chunk = [0; 8192];
                let n = socket.read(&mut chunk).await.unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..n]);
            }
            let filters: Vec<Value> = serde_json::from_slice(&request[header_end..]).unwrap();
            let mut response = Vec::new();
            for filter in filters {
                log.lock().unwrap().push(filter.clone());
                let mut page = events
                    .iter()
                    .filter(|event| {
                        if let Some(ids) = filter["ids"].as_array() {
                            return ids.contains(&event["id"]);
                        }
                        if filter["#e"].is_array() && event["id"] == filter["#e"][0] {
                            return false;
                        }
                        if let Some(id) = filter["thread_cursor_id"].as_str() {
                            return event["id"].as_str().unwrap() > id;
                        }
                        true
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                page.sort_by(|a, b| key(a).cmp(&key(b)));
                if filter["depth_limit"].is_null() && filter["ids"].is_null() {
                    page.reverse();
                }
                page.truncate(filter["limit"].as_u64().unwrap() as usize);
                response.extend(page);
            }
            let body = serde_json::to_vec(&response).unwrap();
            // No Content-Length. EOF framing exercises the bounded body reader.
            if socket
                .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n")
                .await
                .is_ok()
            {
                let _ = socket.write_all(&body).await;
            }
        }
    });
    (ctx, server, requests)
}

#[tokio::test]
async fn all_history_same_second_capped_head_tail_and_huge_body_are_truthful() {
    let agent = nostr::Keys::generate();
    let root = format!("{:064x}", 1);
    let mut events = Vec::new();
    for n in 1..=1200 {
        events.push(json!({"kind":9,"id":format!("{n:064x}"),"pubkey":"a".repeat(64),"created_at":1000,"content":format!("event-{n}-{}","x".repeat(if n == 500 {300_000} else {400})),"tags":if n == 2 {json!([["p",agent.public_key().to_hex()]])} else {json!([])}}));
    }
    let (mut ctx, server, requests) = scripted(events).await;
    ctx.agent_keys = agent;
    let (context, mention) = selected(
        &ContextTarget::Thread(root.clone()),
        Uuid::from_u128(42),
        RestartDepth::All,
        &ctx,
    )
    .await;
    assert!(matches!(mention, Some(FirstMention::Found(_))));
    let context = context.unwrap();
    let rendered = crate::queue::format_conversation_context(&context, None);
    assert!(rendered.len() <= HISTORY_BYTES);
    assert!(rendered.contains("event-1-"));
    assert!(rendered.contains("event-2-"));
    assert!(rendered.contains("event-1200-"));
    assert!(!rendered.contains("event-500-"));
    match context {
        ConversationContext::Thread {
            messages,
            total,
            truncated,
            ..
        } => {
            assert!(truncated);
            assert_eq!(total, 1200);
            assert!(messages.len() < total);
            assert_eq!(
                messages.len(),
                messages
                    .iter()
                    .map(|m| &m.event_id)
                    .collect::<HashSet<_>>()
                    .len()
            );
        }
        _ => panic!("thread"),
    }
    let requests = requests.lock().unwrap();
    assert!(requests
        .iter()
        .any(|f| f["thread_cursor_id"].as_str().is_some()));
    assert!(!requests.iter().any(|f| f["thread_cursor_id"] == ""));
    server.abort();
}

#[tokio::test]
async fn bounded_history_reader_rejects_chunked_actual_bytes_and_times_out() {
    for chunked in [true, false] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut ctx = super::super::tests::make_prompt_context_no_owner();
        ctx.rest_client.base_url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 8192];
            assert!(socket.read(&mut request).await.unwrap() > 0);
            if chunked {
                socket.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").await.unwrap();
                let chunk = "x".repeat(1024 * 1024);
                for _ in 0..5 {
                    if socket
                        .write_all(format!("{:x}\r\n{}\r\n", chunk.len(), chunk).as_bytes())
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                let _ = socket.write_all(b"0\r\n\r\n").await;
            } else {
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
        let start = Instant::now();
        let error = ctx
            .rest_client
            .query_raw_bounded(&[json!({"kinds":[9]})], RESPONSE_BYTES)
            .await
            .unwrap_err();
        if chunked {
            assert!(error.to_string().contains("byte budget"));
        } else {
            assert!(error.to_string().contains("deadline"));
        }
        assert!(start.elapsed() < Duration::from_secs(4));
        server.abort();
    }
}

#[tokio::test]
async fn all_fits_and_none_missing_assignment_never_fall_back_to_recent() {
    let root = format!("{:064x}", 0xffff);
    let events = (1..=8).map(|n| json!({"kind":9,"id":if n == 1 {root.clone()} else {format!("{n:064x}")},"pubkey":"a".repeat(64),"created_at":1000,"content":format!("message {n}"),"tags":[]})).collect();
    let (ctx, server, _) = scripted(events).await;
    let (all, _) = selected(
        &ContextTarget::Thread(root.clone()),
        Uuid::from_u128(42),
        RestartDepth::All,
        &ctx,
    )
    .await;
    match all.unwrap() {
        ConversationContext::Thread {
            messages,
            total,
            truncated,
            root_present,
        } => {
            assert_eq!(
                messages[0].event_id, root,
                "canonical root must precede same-second lower IDs"
            );
            assert_eq!(messages.len(), 8);
            assert_eq!(total, 8);
            assert!(!truncated);
            assert!(root_present);
        }
        _ => panic!("thread"),
    }
    let (none, mention) = selected(
        &ContextTarget::Thread(root),
        Uuid::from_u128(42),
        RestartDepth::None,
        &ctx,
    )
    .await;
    assert!(matches!(mention, Some(FirstMention::NotFound)));
    match none.unwrap() {
        ConversationContext::Thread { messages, .. } => assert!(messages.is_empty()),
        _ => panic!("thread"),
    }
    server.abort();
}

#[test]
fn rendered_budget_preserves_assignment_when_root_and_assignment_cannot_both_fit() {
    let root = "a".repeat(64);
    let assignment = "b".repeat(64);
    let mut context = Some(ConversationContext::Thread {
        messages: vec![
            ContextMessage {
                event_id: root.clone(),
                pubkey: "human".into(),
                timestamp: "now".into(),
                content: "root".repeat(50_000),
            },
            ContextMessage {
                event_id: assignment.clone(),
                pubkey: "human".into(),
                timestamp: "now".into(),
                content: "assignment".repeat(10_000),
            },
        ],
        total: 2,
        root_present: true,
        truncated: false,
    });
    enforce_render_budget(
        &mut context,
        None,
        &HashSet::from([root, assignment.clone()]),
    );
    match context.unwrap() {
        ConversationContext::Thread {
            messages,
            root_present,
            truncated,
            ..
        } => {
            assert!(truncated);
            assert!(!root_present);
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].event_id, assignment);
        }
        _ => panic!("thread"),
    }
}

#[tokio::test]
async fn all_history_request_budget_reserves_tail_without_overfetching_candidates() {
    let root = format!("{:064x}", 1);
    let events = (1..=11_000).map(|n| json!({"kind":9,"id":format!("{n:064x}"),"pubkey":"a".repeat(64),"created_at":1000,"content":format!("bounded message {n}"),"tags":[]})).collect();
    let (ctx, server, requests) = scripted(events).await;
    let (context, mention) = selected(
        &ContextTarget::Thread(root),
        Uuid::from_u128(42),
        RestartDepth::All,
        &ctx,
    )
    .await;
    assert!(matches!(mention, Some(FirstMention::NotFound)));
    let context = context.unwrap();
    assert!(crate::queue::format_conversation_context(&context, None).len() <= HISTORY_BYTES);
    match context {
        ConversationContext::Thread {
            messages,
            total,
            truncated,
            ..
        } => {
            assert!(truncated);
            assert!(total < 11_000);
            assert!(
                messages
                    .iter()
                    .any(|m| m.event_id == format!("{:064x}", 11_000)),
                "latest tail is reserved before the scan reaches its cap"
            );
        }
        _ => panic!("thread"),
    }
    let filters = requests.lock().unwrap();
    assert!(
        filters.len() <= MAX_PAGES + 1,
        "root filter shares a request"
    );
    let requested = filters
        .iter()
        .map(|f| f["limit"].as_u64().unwrap() as usize)
        .sum::<usize>();
    assert!(
        requested <= MAX_CANDIDATES,
        "even requested raw candidates must stay inside the shared budget: {requested}"
    );
    server.abort();
}

#[tokio::test]
async fn all_history_many_small_messages_fit_the_actual_rendered_budget() {
    let root = format!("{:064x}", 1);
    let events = (1..=2000).map(|n| json!({"kind":9,"id":format!("{n:064x}"),"pubkey":"a".repeat(64),"created_at":1000,"content":"small","tags":[]})).collect::<Vec<_>>();
    let expected = ConversationContext::Thread {
        messages: events
            .iter()
            .map(|event| json_to_context_message(event).unwrap())
            .collect(),
        total: events.len(),
        root_present: true,
        truncated: false,
    };
    assert!(crate::queue::format_conversation_context(&expected, None).len() < HISTORY_BYTES);
    let (ctx, server, _) = scripted(events).await;
    let (actual, _) = selected(
        &ContextTarget::Thread(root),
        Uuid::from_u128(42),
        RestartDepth::All,
        &ctx,
    )
    .await;
    assert_eq!(
        crate::queue::format_conversation_context(&actual.unwrap(), None),
        crate::queue::format_conversation_context(&expected, None),
        "a fitting rendered history must retain every message"
    );
    server.abort();
}

#[test]
fn supplied_assignment_in_trigger_survives_history_delta_filtering() {
    let root = "a".repeat(64);
    let target = ContextTarget::Thread(root.clone());
    let context = ConversationContext::Thread {
        messages: vec![],
        total: 1,
        root_present: false,
        truncated: false,
    };
    assert!(crate::queue::format_conversation_context(&context, None).len() < HISTORY_BYTES);
    let batch = HashSet::from([root.clone()]);
    for mention in [
        FirstMention::InRoot,
        FirstMention::Found(json!({"id":root})),
    ] {
        assert!(
            !matches!(
                supplied_mention(Some(mention), &target, Some(&context), &batch),
                Some(FirstMention::NotFound)
            ),
            "the trigger still supplies its assignment body"
        );
    }
    assert!(matches!(
        supplied_mention(
            Some(FirstMention::InRoot),
            &target,
            Some(&context),
            &HashSet::new()
        ),
        Some(FirstMention::NotFound)
    ));
}
