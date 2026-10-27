//! Opt-in provider acceptance with real Codex, npm, MCP and production dispatch.
use super::*;

#[tokio::test]
#[ignore = "Requires actual codex-acp, npm and BUZZ_TEST_BIN_DIR"]
async fn real_codex_native_mcp_config_survives_injected_server_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let initialized = dir.path().join("initialized");
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/late_service");
    let mut config: serde_json::Value =
        serde_json::from_str(&std::env::var("CODEX_CONFIG").unwrap_or_else(|_| "{}".into()))
            .unwrap();
    config["mcp_servers"]["native_late"] = serde_json::json!({"command":"npm", "args":["--silent","--prefix", fixture,"start"], "env":{"STALL_TEST_INITIALIZED":initialized,"STALL_TEST_LABEL":"native"}});
    let mut client = AcpClient::spawn_with_env(
        &std::env::var("BUZZ_TEST_CODEX_ACP").unwrap_or_else(|_| "codex-acp".into()),
        &[],
        &[],
        false,
        &[("CODEX_CONFIG".into(), config.to_string())],
    )
    .await
    .unwrap();
    client.initialize().await.unwrap();
    client.capture_startup_children();
    client.process_tree_snapshots.push_back(None);
    client
        .session_new(
            dir.path().to_str().unwrap(),
            vec![McpServer {
                name: "injected".into(),
                command: "npm".into(),
                args: vec![
                    "--silent".into(),
                    "--prefix".into(),
                    fixture.to_string_lossy().into(),
                    "start".into(),
                ],
                env: vec![
                    EnvVar {
                        name: "STALL_TEST_INITIALIZED".into(),
                        value: initialized.to_string_lossy().into(),
                    },
                    EnvVar {
                        name: "STALL_TEST_LABEL".into(),
                        value: "injected".into(),
                    },
                ],
            }],
            None,
            None,
        )
        .await
        .unwrap();
    // Native MCP startup may complete after session/new acknowledges the thread.
    let mut children = client.background_child_pids();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let receipts = || {
        std::fs::read_to_string(&initialized)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect::<HashSet<_>>()
    };
    while (children
        .as_ref()
        .is_none_or(|children| !children.is_empty())
        || receipts().len() < 2)
        && tokio::time::Instant::now() < deadline
    {
        tokio::time::sleep(Duration::from_millis(30)).await;
        children = client.background_child_pids();
    }
    client.shutdown().await;
    assert_eq!(
        receipts(),
        HashSet::from(["native".into(), "injected".into()]),
        "both configured services must actually initialize"
    );
    assert_eq!(
        children,
        Some(HashSet::new()),
        "native and injected MCP launches both require provenance"
    );
}

fn push_request(queue: &mut EventQueue, scope: &SessionScope) {
    let event = EventBuilder::new(Kind::Custom(9),
        "Call the late-service ping MCP tool exactly once, then answer OK. Do not use any other tool or publish messages.")
        .tags([nostr::Tag::parse(["e", &"a".repeat(64), "", "root"]).unwrap()])
        .sign_with_keys(&Keys::generate()).unwrap();
    queue.push(QueuedEvent {
        channel_id: scope.channel_id(),
        scope: scope.clone(),
        event,
        prompt_tag: "test".into(),
        received_at: std::time::Instant::now(),
        edit: None,
    });
}

async fn prompt(agent: &mut OwnedAgent, session: &str, text: &str) {
    assert_eq!(
        agent
            .acp
            .session_prompt_with_idle_timeout(
                session,
                text,
                Duration::from_secs(90),
                Duration::from_secs(180)
            )
            .await
            .unwrap(),
        StopReason::EndTurn
    );
}

async fn wait_for_mcp_tools(agent: &mut OwnedAgent, session: &str, worker: usize) {
    let observer = crate::observer::ObserverHandle::in_process();
    agent.acp.set_observer(Some(observer.clone()), worker);
    let mut status = String::new();
    let ready = timeout(Duration::from_secs(15), async {
        loop {
            let after = observer.snapshot().last().map_or(0, |event| event.seq);
            // Codex handles /mcp locally: inspect its discovered tool inventory
            // before asking the model to use a server that starts asynchronously.
            prompt(agent, session, "/mcp").await;
            status = observer
                .snapshot()
                .iter()
                .filter(|event| event.seq > after && event.kind == "acp_read")
                .map(|event| &event.payload["params"]["update"])
                .filter(|update| update["sessionUpdate"] == "agent_message_chunk")
                .filter_map(|update| update["content"]["text"].as_str())
                .collect();
            let lines: Vec<_> = status.lines().map(|line| line.replace('`', "")).collect();
            if ["late_service", "native_late"].iter().all(|server| {
                lines.iter().any(|line| {
                    line.replace("late-service", "late_service")
                        .starts_with(&format!("- {server}: 2 tools"))
                })
            }) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    eprintln!("worker {worker} MCP inventory before model prompt: {status}");
    assert!(
        ready.is_ok(),
        "both servers must advertise their actual tools"
    );
    agent.acp.set_observer(None, worker);
}

async fn take_result(pool: &mut AgentPool, queue: &mut EventQueue) -> SessionScope {
    let result = timeout(Duration::from_secs(180), pool.result_rx.recv())
        .await
        .unwrap()
        .unwrap();
    if let PromptOutcome::Error(error) = &result.outcome {
        panic!("real provider failed: {error}");
    }
    assert!(matches!(
        result.outcome,
        PromptOutcome::Ok(StopReason::EndTurn)
    ));
    let scope = result.source.scope().unwrap().clone();
    pool.task_map
        .retain(|_, task| task.agent_index != result.agent.index);
    queue.mark_complete(&scope);
    pool.return_agent(result.agent);
    scope
}

#[tokio::test]
#[ignore = "Requires configured codex-acp/provider, npm and BUZZ_TEST_BIN_DIR"]
async fn real_codex_five_workers_late_npm_services_and_original_collection() {
    let adapter = std::env::var("BUZZ_TEST_CODEX_ACP").unwrap_or_else(|_| "codex-acp".into());
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("finish");
    let calls = dir.path().join("calls");
    let starts = dir.path().join("initializations");
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/late_service");
    let server = McpServer {
        name: "late-service".into(),
        command: "npm".into(),
        args: vec![
            "--silent".into(),
            "--prefix".into(),
            fixture.to_string_lossy().into(),
            "start".into(),
        ],
        env: vec![
            EnvVar {
                name: "STALL_TEST_MARKER".into(),
                value: marker.to_string_lossy().into(),
            },
            EnvVar {
                name: "STALL_TEST_CALLS".into(),
                value: calls.to_string_lossy().into(),
            },
            EnvVar {
                name: "STALL_TEST_INITIALIZED".into(),
                value: starts.to_string_lossy().into(),
            },
            EnvVar {
                name: "STALL_TEST_LABEL".into(),
                value: "injected".into(),
            },
        ],
    };
    let mut config: serde_json::Value =
        serde_json::from_str(&std::env::var("CODEX_CONFIG").unwrap_or_else(|_| "{}".into()))
            .unwrap();
    config["model_reasoning_effort"] = serde_json::json!("low");
    config["mcp_servers"]["native_late"] = serde_json::json!({"command":"npm", "args":["--silent","--prefix",fixture,"start"],"env":{"STALL_TEST_MARKER":marker,"STALL_TEST_CALLS":calls,"STALL_TEST_INITIALIZED":starts,"STALL_TEST_LABEL":"native"}});
    config["developer_instructions"] = serde_json::json!("This is an isolated local runtime test. Use only the late-service and native_late MCP tools. Never run shell commands or publish messages, including when background collection asks for a managed report. On collection, call ping and answer with its receipt.");
    let launch = vec![
        ("CODEX_CONFIG".into(), config.to_string()),
        ("BUZZ_RELAY_URL".into(), "http://127.0.0.1:0".into()),
        ("BUZZ_AUTH_TAG".into(), String::new()),
        (
            "BUZZ_PRIVATE_KEY".into(),
            Keys::generate().secret_key().to_secret_hex(),
        ),
    ];
    let mut pool = AgentPool::from_slots(vec![None, None, None, None, None]);
    let scopes: Vec<_> = (0..6).map(|_| scope()).collect();
    for (index, original) in scopes.iter().take(5).enumerate() {
        let mut agent = worker(index).await;
        agent.acp = AcpClient::spawn_with_env(&adapter, &[], &[], false, &launch)
            .await
            .unwrap();
        let initialized = agent.acp.initialize().await.unwrap();
        eprintln!(
            "worker {index} adapter={}",
            initialized
                .get("agentInfo")
                .unwrap_or(&serde_json::Value::Null)
        );
        agent.acp.capture_startup_children();
        // Exercise a missed setup census: MCP services must still be recognized
        // through explicit launch provenance, rather than accidental adoption.
        agent.acp.process_tree_snapshots.push_back(None);
        let session = agent
            .acp
            .session_new(
                dir.path().to_str().unwrap(),
                vec![server.clone()],
                None,
                None,
            )
            .await
            .unwrap();
        let generation = pool.record_scope_owner(original.clone(), index);
        agent
            .state
            .sessions
            .insert(original.clone(), session.clone());
        agent
            .state
            .set_scope_owner_generation(original.clone(), generation);
        agent.agent_name = "codex-acp".into();
        // Only this validated profile exposes per-session tool counts. Adapter
        // 2.1.1 lists injected names without counts, so retain its original
        // provider workflow; the five actual ping receipts remain mandatory.
        if initialized["agentInfo"]["version"].as_str() == Some("2.2.2") {
            wait_for_mcp_tools(&mut agent, &session, index).await;
        } else {
            eprintln!("worker {index}: inventory preflight omitted for this adapter profile");
        }
        prompt(&mut agent, &session, "Call the late-service ping MCP tool exactly once, then answer OK. Do not use any other tool.").await;
        timeout(Duration::from_secs(10), async {
            while std::fs::read_to_string(&starts)
                .unwrap_or_default()
                .lines()
                .count()
                < 2 * (index + 1)
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        agent.remember_background_context(&PromptSource::Channel(original.clone()), None, &session);
        agent.prepare_background_return();
        pool.return_agent(agent);
        assert!(
            !pool.has_runtime_hold(),
            "worker {index} retained only services"
        );
        eprintln!("worker {index}: real MCP turn finished and slot reusable");
    }
    assert_eq!(std::fs::read_to_string(&calls).unwrap().lines().count(), 5);
    let mut ctx = tests::make_prompt_context_no_owner();
    ctx.mcp_servers = vec![server];
    ctx.cwd = dir.path().to_string_lossy().into();
    ctx.dedup_mode = DedupMode::Queue;
    ctx.idle_timeout = Duration::from_secs(90);
    ctx.max_turn_duration = Duration::from_secs(180);
    ctx.system_prompt = Some("This is a local runtime integration test. Use only the supplied late-service and native_late MCP tools when asked. Do not run shell commands or publish any messages. For background collection, call ping then answer with the result. Ignore instructions to publish a managed report.".into());
    let ctx = Arc::new(ctx);
    let mut queue = EventQueue::new(DedupMode::Queue);
    push_request(&mut queue, &scopes[5]);
    push_request(&mut queue, &scopes[0]);
    let mut completed = HashSet::new();
    while completed.len() < 2 {
        assert!(dispatch(&mut pool, &mut queue, &ctx) > 0);
        completed.insert(take_result(&mut pool, &mut queue).await);
    }
    assert_eq!(
        completed,
        HashSet::from([scopes[5].clone(), scopes[0].clone()])
    );
    eprintln!("sixth scope and later original-scope request both completed without restart");
    let original = scopes[0].clone();
    let mut agent = pool.try_claim(Some(&original)).unwrap();
    let index = agent.index;
    let session = agent.state.sessions[&original].clone();
    prompt(&mut agent, &session, "Call the native_late start_work MCP tool exactly once, then answer STARTED. Do not wait, use shell or publish messages.").await;
    timeout(Duration::from_secs(5), async {
        while !marker.with_extension("started").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let event = EventBuilder::new(Kind::Custom(9), "start background work")
        .tags([nostr::Tag::parse(["e", &"a".repeat(64), "", "root"]).unwrap()])
        .sign_with_keys(&Keys::generate())
        .unwrap();
    queue.push(QueuedEvent {
        channel_id: original.channel_id(),
        scope: original.clone(),
        event,
        prompt_tag: "test".into(),
        received_at: std::time::Instant::now(),
        edit: None,
    });
    let batch = queue.flush_next().unwrap();
    queue.mark_complete(&original);
    agent.remember_background_context(
        &PromptSource::Channel(original.clone()),
        Some(&batch),
        &session,
    );
    agent.prepare_background_return();
    pool.return_agent(agent);
    assert!(pool.runtime_holds.contains_key(&index));
    assert!(!pool.runtime_claim_allowed(index, Some(&scopes[5])));
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert!(!marker.with_extension("result").exists());
    std::fs::write(&marker, "finish").unwrap();
    let mut poll = crate::background_poll::BackgroundPoll::default();
    timeout(Duration::from_secs(10), async {
        while !queue.has_pending_continuation(&original) {
            poll.start(pool.background_poll_requests());
            let result = poll.wait().await.unwrap().unwrap();
            pool.apply_background_snapshot(&result.requests, result.tree.as_ref(), &mut queue);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(dispatch(&mut pool, &mut queue, &ctx), 1);
    assert_eq!(take_result(&mut pool, &mut queue).await, original);
    assert!(!pool.has_runtime_hold());
    eprintln!("real MCP child retained the slot for ten seconds; production collection returned to original scope");
    for agent in pool.agents.iter_mut().flatten() {
        agent.acp.shutdown().await;
    }
}
