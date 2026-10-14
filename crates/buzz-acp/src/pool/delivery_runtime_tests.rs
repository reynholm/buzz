//! Opt-in production runtime checks: real bundled ACP agent, MCP shell and CLI,
//! with deterministic local model and signature-checking mock relay readback.
use super::*;
use crate::queue::{EventQueue, RuntimeContinuation};
use nostr::{Event, EventBuilder, Kind, Tag};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn request(socket: &mut tokio::net::TcpStream) -> (String, serde_json::Value, Vec<u8>) {
    let mut bytes = Vec::new();
    let mut buf = [0; 8192];
    let end = loop {
        let n = socket.read(&mut buf).await.unwrap();
        assert_ne!(n, 0);
        bytes.extend_from_slice(&buf[..n]);
        assert!(bytes.len() < 2_000_000);
        if let Some(end) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = String::from_utf8_lossy(&bytes[..end]);
    let route = headers
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();
    let length: usize = headers
        .lines()
        .find_map(|line| {
            line.to_lowercase()
                .strip_prefix("content-length:")
                .map(|s| s.trim().parse().unwrap())
        })
        .unwrap();
    assert!(length < 1_900_000);
    while bytes.len() < end + length {
        let n = socket.read(&mut buf).await.unwrap();
        assert_ne!(n, 0);
        bytes.extend_from_slice(&buf[..n]);
    }
    let body = bytes[end..end + length].to_vec();
    (route, serde_json::from_slice(&body).unwrap(), body)
}

async fn response(socket: &mut tokio::net::TcpStream, status: &str, body: serde_json::Value) {
    let body = body.to_string();
    let response = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    let _ = socket.write_all(response.as_bytes()).await;
}

struct MockRelay {
    url: String,
    keys: nostr::Keys,
    channel: Uuid,
    parent: Event,
    events: Arc<std::sync::Mutex<Vec<Event>>>,
    bodies: Arc<std::sync::Mutex<Vec<Vec<u8>>>>,
    losses: Arc<AtomicUsize>,
    hold: Arc<AtomicBool>,
    accepted: Arc<tokio::sync::Notify>,
    task: tokio::task::JoinHandle<()>,
}

impl MockRelay {
    async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let keys = nostr::Keys::generate();
        let channel = Uuid::new_v4();
        let parent = EventBuilder::new(Kind::Custom(9), "original task")
            .tag(Tag::parse(["h", &channel.to_string()]).unwrap())
            .sign_with_keys(&keys)
            .unwrap();
        let membership = EventBuilder::new(Kind::Custom(39002), "")
            .tags([
                Tag::parse(["d", &channel.to_string()]).unwrap(),
                Tag::parse(["p", &keys.public_key().to_hex()]).unwrap(),
            ])
            .allow_self_tagging()
            .sign_with_keys(&keys)
            .unwrap();
        let events = Arc::new(std::sync::Mutex::new(Vec::<Event>::new()));
        let bodies = Arc::new(std::sync::Mutex::new(Vec::new()));
        let losses = Arc::new(AtomicUsize::new(0));
        let hold = Arc::new(AtomicBool::new(false));
        let accepted = Arc::new(tokio::sync::Notify::new());
        let (events_task, bodies_task, losses_task, hold_task, accepted_task, parent_task) = (
            events.clone(),
            bodies.clone(),
            losses.clone(),
            hold.clone(),
            accepted.clone(),
            parent.clone(),
        );
        let task = tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let (route, value, body) = request(&mut socket).await;
                if route == "/query" {
                    let filter = &value[0];
                    let reply = if filter["kinds"][0] == 39002 {
                        serde_json::json!([membership])
                    } else if filter["ids"][0] == parent_task.id.to_hex() {
                        serde_json::json!([parent_task])
                    } else {
                        serde_json::json!(*events_task.lock().unwrap())
                    };
                    response(&mut socket, "200 OK", reply).await;
                } else if route == "/events" {
                    let event: Event = serde_json::from_value(value).unwrap();
                    event.verify().unwrap();
                    assert!(event
                        .tags
                        .iter()
                        .any(|tag| tag.as_slice() == ["h", &channel.to_string()]));
                    let duplicate = {
                        let mut events = events_task.lock().unwrap();
                        let duplicate = events.iter().any(|e| e.id == event.id);
                        if !duplicate {
                            events.push(event.clone());
                        }
                        duplicate
                    };
                    bodies_task.lock().unwrap().push(body);
                    accepted_task.notify_one();
                    while hold_task.load(Ordering::SeqCst) {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                    if losses_task
                        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                        .is_ok()
                    {
                        // Stored event, successful HTTP header, then lost response body.
                        let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4096\r\nConnection: close\r\n\r\n{").await;
                    } else {
                        response(&mut socket, "200 OK", serde_json::json!({"accepted":true,"event_id":event.id.to_hex(),"message":if duplicate {"duplicate:"} else {"saved"}})).await;
                    }
                } else {
                    panic!("unexpected mock relay route {route}");
                }
            }
        });
        Self {
            url,
            keys,
            channel,
            parent,
            events,
            bodies,
            losses,
            hold,
            accepted,
            task,
        }
    }

    fn command(
        &self,
        binary: &std::path::Path,
        dir: &std::path::Path,
        key: Uuid,
        content: &str,
    ) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(binary);
        command
            .env_remove("BUZZ_AUTH_TAG")
            .arg("--relay")
            .arg(&self.url)
            .arg("--private-key")
            .arg(self.keys.secret_key().to_secret_hex())
            .args([
                "messages",
                "deliver",
                "--delivery-key",
                &key.to_string(),
                "--channel",
                &self.channel.to_string(),
                "--reply-to",
                &self.parent.id.to_hex(),
                "--journal-dir",
            ])
            .arg(dir)
            .args(["--content", content]);
        command.kill_on_drop(true);
        command
    }

    async fn readback(&self) -> Vec<Event> {
        let events: Vec<Event> = reqwest::Client::new()
            .post(format!("{}/query", self.url))
            .json(&serde_json::json!([{"kinds":[9],"#h":[self.channel.to_string()]}]))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        for event in &events {
            event.verify().unwrap();
        }
        eprintln!(
            "mock-relay verified readback: {}",
            serde_json::to_string(&events).unwrap()
        );
        events
    }
}

impl Drop for MockRelay {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn binaries() -> PathBuf {
    PathBuf::from(
        std::env::var("BUZZ_TEST_BIN_DIR").expect("set BUZZ_TEST_BIN_DIR to built binaries"),
    )
}

#[tokio::test]
#[ignore = "requires rebuilt buzz CLI, buzz-agent and buzz-dev-mcp; BUZZ_TEST_BIN_DIR"]
async fn managed_runtime_body_loss_and_process_restart_replay_saved_bytes() {
    let relay = MockRelay::start().await;
    let dir = tempfile::tempdir().unwrap();
    let key = Uuid::new_v4();
    relay.losses.store(3, Ordering::SeqCst);
    let first = relay
        .command(&binaries().join("buzz"), dir.path(), key, "first")
        .output()
        .await
        .unwrap();
    assert_eq!(
        first.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(String::from_utf8_lossy(&first.stderr).contains("managed_delivery_pending"));
    let retry = relay
        .command(
            &binaries().join("buzz"),
            dir.path(),
            key,
            "changed on restart",
        )
        .output()
        .await
        .unwrap();
    assert!(
        retry.status.success(),
        "{}",
        String::from_utf8_lossy(&retry.stderr)
    );
    let events = relay.readback().await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].content, "first");
    let bodies = relay.bodies.lock().unwrap();
    assert_eq!(bodies.len(), 4);
    assert!(bodies.iter().all(|body| body == &bodies[0]));
}

#[tokio::test]
#[ignore = "requires rebuilt buzz CLI; BUZZ_TEST_BIN_DIR"]
async fn managed_runtime_killed_after_relay_acceptance_replays_on_new_process() {
    let relay = MockRelay::start().await;
    let dir = tempfile::tempdir().unwrap();
    let key = Uuid::new_v4();
    relay.hold.store(true, Ordering::SeqCst);
    let mut first = relay
        .command(
            &binaries().join("buzz"),
            dir.path(),
            key,
            "saved before crash",
        )
        .spawn()
        .unwrap();
    timeout(Duration::from_secs(10), relay.accepted.notified())
        .await
        .unwrap();
    let journal = std::fs::read_to_string(dir.path().join("journal.json")).unwrap();
    assert!(journal.contains(&relay.events.lock().unwrap()[0].id.to_hex()));
    assert!(journal.contains("\"confirmed_at\":null"));
    first.kill().await.unwrap();
    first.wait().await.unwrap();
    relay.hold.store(false, Ordering::SeqCst);
    let retry = relay
        .command(
            &binaries().join("buzz"),
            dir.path(),
            key,
            "changed after crash",
        )
        .output()
        .await
        .unwrap();
    assert!(
        retry.status.success(),
        "{}",
        String::from_utf8_lossy(&retry.stderr)
    );
    assert_eq!(relay.readback().await.len(), 1);
    let bodies = relay.bodies.lock().unwrap();
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[0], bodies[1]);
}

#[tokio::test]
#[ignore = "requires rebuilt buzz CLI; BUZZ_TEST_BIN_DIR"]
async fn managed_runtime_concurrent_cli_processes_share_event_and_lock() {
    let relay = MockRelay::start().await;
    let dir = tempfile::tempdir().unwrap();
    let key = Uuid::new_v4();
    let mut a = relay.command(
        &binaries().join("buzz"),
        dir.path(),
        key,
        "first concurrent",
    );
    let mut b = relay.command(
        &binaries().join("buzz"),
        dir.path(),
        key,
        "second concurrent",
    );
    let (a, b) = tokio::join!(a.output(), b.output());
    assert!(a.unwrap().status.success());
    assert!(b.unwrap().status.success());
    assert_eq!(relay.readback().await.len(), 1);
    assert_eq!(relay.bodies.lock().unwrap().len(), 1);
}

#[tokio::test]
#[ignore = "requires rebuilt buzz-agent and buzz-dev-mcp embedding managed CLI; BUZZ_TEST_BIN_DIR"]
async fn managed_runtime_bundled_agent_publication_then_application_error_retries_one_event() {
    let relay = MockRelay::start().await;
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path().join("owner's workspace");
    std::fs::create_dir(&workspace).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let model_url = format!("http://{}", listener.local_addr().unwrap());
    let model = tokio::spawn(async move {
        let mut commands = Vec::new();
        for round in 0..4 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let (_, value, _) = request(&mut socket).await;
            if round == 1 {
                assert!(
                    value["messages"].to_string().contains("accepted"),
                    "publication tool must have completed before model error"
                );
                response(&mut socket, "400 Bad Request", serde_json::json!({"error":{"message":"application failure after final report publication","type":"invalid_request_error","code":"test_application_error"}})).await;
                continue;
            }
            let (message, reason) = if round == 0 || round == 2 {
                let name = value["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find_map(|tool| {
                        tool["function"]["name"]
                            .as_str()
                            .filter(|name| name.ends_with("__shell"))
                    })
                    .unwrap();
                let prompt = value["messages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .rev()
                    .filter_map(|m| m["content"].as_str())
                    .find(|text| text.contains("buzz messages deliver --delivery-key"))
                    .unwrap();
                let line = prompt
                    .lines()
                    .find(|line| line.starts_with("buzz messages deliver --delivery-key"))
                    .unwrap();
                commands.push(line.to_string());
                let text = if round == 0 {
                    "first final report"
                } else {
                    "changed final report after retry"
                };
                let command = format!(
                    "env -u BUZZ_AUTH_TAG {}",
                    line.replace("<FINAL_REPORT>", &format!("'{text}'"))
                );
                (
                    serde_json::json!({"role":"assistant","content":null,"tool_calls":[{"id":format!("publish-{round}"),"type":"function","function":{"name":name,"arguments":serde_json::json!({"command":command}).to_string()}}]}),
                    "tool_calls",
                )
            } else {
                (
                    serde_json::json!({"role":"assistant","content":"done"}),
                    "stop",
                )
            };
            response(&mut socket, "200 OK", serde_json::json!({"id":"managed-test","object":"chat.completion","model":"probe","choices":[{"index":0,"message":message,"finish_reason":reason}]})).await;
        }
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0], commands[1]);
    });
    let mut config = crate::build_mcp_servers_tests::test_config();
    config.keys = relay.keys.clone();
    config.relay_url = relay.url.clone();
    config.mcp_command = binaries()
        .join("buzz-dev-mcp")
        .to_string_lossy()
        .into_owned();
    let env = vec![
        ("BUZZ_AGENT_PROVIDER".into(), "openai".into()),
        ("OPENAI_COMPAT_BASE_URL".into(), model_url),
        ("OPENAI_COMPAT_API_KEY".into(), "test".into()),
        ("OPENAI_COMPAT_MODEL".into(), "probe".into()),
    ];
    let mut agent = OwnedAgent {
        index: 0,
        acp: AcpClient::spawn(
            binaries().join("buzz-agent").to_str().unwrap(),
            &[],
            &env,
            false,
        )
        .await
        .unwrap(),
        state: SessionState::default(),
        model_capabilities: None,
        desired_model: None,
        model_overridden: false,
        desired_model_request_id: None,
        desired_model_pending_ack: false,
        startup_effort: None,
        agent_name: "delivery-test".into(),
        goose_system_prompt_supported: None,
        protocol_version: 2,
        background_context: None,
    };
    agent.acp.initialize().await.unwrap();
    let session = agent
        .acp
        .session_new_full(
            workspace.to_str().unwrap(),
            crate::build_mcp_servers(&config),
            None,
            None,
        )
        .await
        .unwrap();
    let scope = SessionScope::Conversation {
        channel_id: relay.channel,
    };
    agent
        .state
        .sessions
        .insert(scope.clone(), session.session_id.clone());
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    let generation = pool.record_scope_owner(scope.clone(), 0);
    pool.agents[0]
        .as_mut()
        .unwrap()
        .state
        .set_scope_owner_generation(scope.clone(), generation);
    let continuation = RuntimeContinuation {
        delivery_key: Uuid::new_v4(),
        worker_index: 0,
        session_id: session.session_id,
        generation,
        thread_tags: ThreadTags {
            root_event_id: Some(relay.parent.id.to_hex()),
            parent_event_id: Some(relay.parent.id.to_hex()),
            ..Default::default()
        },
        received_at: std::time::Instant::now(),
    };
    let mut queue = EventQueue::new(DedupMode::Queue);
    queue.enqueue_continuation(scope.clone(), continuation);
    let mut ctx = tests::make_prompt_context_no_owner();
    ctx.cwd = workspace.to_string_lossy().into_owned();
    ctx.agent_keys = relay.keys.clone();
    ctx.memory_enabled = false;
    ctx.context_message_limit = 0;
    ctx.rest_client.base_url = relay.url.clone();
    ctx.channel_info = ChannelInfoResolver::new(HashMap::new(), ctx.rest_client.clone());
    ctx.channel_info.cache.write().unwrap().insert(
        relay.channel,
        PromptChannelInfo {
            name: "reports".into(),
            channel_type: "stream".into(),
            description: None,
            project: None,
        },
    );
    ctx.channel_info.projects.write().unwrap().insert(
        relay.channel,
        CachedProjectInfo {
            value: None,
            fetched_at: std::time::Instant::now(),
        },
    );
    let ctx = Arc::new(ctx);
    assert_eq!(
        background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
        1
    );
    let result = timeout(Duration::from_secs(30), pool.rx_and_join_set().0.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        result.outcome,
        PromptOutcome::Error(AcpError::AgentError { .. })
    ));
    background_work_tests::apply_result(&mut pool, &mut queue, result, None, &HashSet::new());
    assert_eq!(relay.readback().await.len(), 1);
    tokio::time::sleep_until(queue.next_retry_deadline().unwrap().into()).await;
    assert_eq!(
        background_work_tests::dispatch(&mut pool, &mut queue, &ctx),
        1
    );
    let result = timeout(Duration::from_secs(30), pool.rx_and_join_set().0.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        result.outcome,
        PromptOutcome::Ok(StopReason::EndTurn)
    ));
    background_work_tests::apply_result(&mut pool, &mut queue, result, None, &HashSet::new());
    model.await.unwrap();
    let events = relay.readback().await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].content, "first final report");
    events[0].verify().unwrap();
    assert_eq!(relay.bodies.lock().unwrap().len(), 1);
    let journal = std::fs::read_to_string(workspace.join(".buzz-deliveries/journal.json")).unwrap();
    assert!(journal.contains(&events[0].id.to_hex()));
    for agent in pool.agents.iter_mut().flatten() {
        agent.acp.shutdown().await;
    }
}
