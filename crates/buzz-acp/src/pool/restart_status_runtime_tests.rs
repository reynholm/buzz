//! Tests bind authorized admission, checked retirement, the scheduler, real ACP
//! framing, durable ownership and signature-checking HTTP publication.
use super::*;
use crate::session_store::RestartStatusPhase;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::time::timeout;

struct Relay {
    posts: Arc<Mutex<Vec<nostr::Event>>>,
    mode: Arc<Mutex<String>>,
    on_post: Arc<Mutex<Option<Arc<SessionStore>>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Relay {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn setup(events: Vec<nostr::Event>, channel: Uuid) -> (PromptContext, Relay) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let posts = Arc::new(Mutex::new(Vec::new()));
    let mode = Arc::new(Mutex::new(String::new()));
    let on_post: Arc<Mutex<Option<Arc<SessionStore>>>> = Arc::new(Mutex::new(None));
    let failure_hook = on_post.clone();
    let captured = posts.clone();
    let behavior = mode.clone();
    let task = tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let events = events.clone();
            let captured = captured.clone();
            let behavior = behavior.clone();
            let failure_hook = failure_hook.clone();
            tokio::spawn(async move {
                let mut request = Vec::new();
                let mut block = [0u8; 8192];
                let header = loop {
                    let n = socket.read(&mut block).await.unwrap_or_default();
                    if n == 0 {
                        return;
                    }
                    request.extend_from_slice(&block[..n]);
                    if request.len() > 128 * 1024 {
                        return;
                    }
                    if let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                        break end + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&request[..header]).into_owned();
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|n| n.trim().parse::<usize>().ok())
                    })
                    .unwrap_or_default();
                if length > 128 * 1024 {
                    return;
                }
                while request.len() < header + length {
                    let n = socket.read(&mut block).await.unwrap_or_default();
                    if n == 0 {
                        return;
                    }
                    request.extend_from_slice(&block[..n]);
                }
                let body: serde_json::Value =
                    serde_json::from_slice(&request[header..header + length]).unwrap_or_default();
                let response = if headers.starts_with("POST /events ") {
                    let event: nostr::Event = serde_json::from_value(body).unwrap();
                    event.verify().unwrap();
                    let id = event.id.to_hex();
                    captured.lock().unwrap().push(event);
                    if let Some(store) = failure_hook.lock().unwrap().as_ref() {
                        store.fail_writes(true);
                    }
                    let mode = behavior.lock().unwrap().clone();
                    if mode == "lost" {
                        return;
                    }
                    if mode == "reject" {
                        serde_json::json!({"accepted":false,"event_id":id})
                    } else if mode == "wrong" {
                        serde_json::json!({"accepted":true,"event_id":"0".repeat(64)})
                    } else {
                        serde_json::json!({"accepted":true,"event_id":id})
                    }
                } else if headers.starts_with("POST /query ") {
                    let filters = body["filters"]
                        .as_array()
                        .cloned()
                        .or_else(|| body.as_array().cloned())
                        .unwrap_or_default();
                    let selected: Vec<_> = events
                        .iter()
                        .filter(|event| {
                            filters.iter().any(|filter| {
                                if filter["kinds"].as_array().is_some_and(|kinds| {
                                    !kinds.iter().any(|kind| {
                                        kind.as_u64() == Some(event.kind.as_u16() as u64)
                                    })
                                }) {
                                    return false;
                                }
                                if filter["ids"].as_array().is_some_and(|ids| {
                                    !ids.iter()
                                        .any(|id| id.as_str() == Some(event.id.to_hex().as_str()))
                                }) {
                                    return false;
                                }
                                for (name, tag) in [("#h", "h"), ("#e", "e")] {
                                    if filter[name].as_array().is_some_and(|values| {
                                        !event.tags.iter().any(|t| {
                                            t.as_slice().first().is_some_and(|v| v == tag)
                                                && t.as_slice().get(1).is_some_and(|v| {
                                                    values.iter().any(|expected| {
                                                        expected.as_str() == Some(v.as_str())
                                                    })
                                                })
                                        })
                                    }) {
                                        return false;
                                    }
                                }
                                true
                            })
                        })
                        .cloned()
                        .collect();
                    serde_json::to_value(selected).unwrap()
                } else {
                    serde_json::json!({})
                };
                let response = response.to_string();
                let wire = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response);
                let _ = socket.write_all(wire.as_bytes()).await;
            });
        }
    });
    let mut ctx = ctx();
    ctx.context_message_limit = 12;
    ctx.session_policy = crate::scope::SessionPolicy::Thread;
    ctx.rest_client.base_url = url.clone();
    ctx.channel_info = ChannelInfoResolver::new(
        HashMap::from([(
            channel,
            crate::relay::ChannelInfo {
                name: "status".into(),
                channel_type: "stream".into(),
                description: None,
            },
        )]),
        RestClient {
            http: reqwest::Client::new(),
            base_url: url,
            keys: ctx.agent_keys.clone(),
            auth_tag_json: None,
        },
    );
    (
        ctx,
        Relay {
            posts,
            mode,
            on_post,
            task,
        },
    )
}
fn event(
    keys: &Keys,
    channel: Uuid,
    root: Option<&str>,
    text: &str,
    agent: Option<&Keys>,
    at: u64,
) -> nostr::Event {
    let mut tags = vec![Tag::parse(["h", channel.to_string().as_str()]).unwrap()];
    if let Some(root) = root {
        tags.push(Tag::parse(["e", root, "", "reply"]).unwrap());
    }
    if let Some(agent) = agent {
        tags.push(Tag::parse(["p", agent.public_key().to_hex().as_str()]).unwrap());
    }
    EventBuilder::new(Kind::Custom(9), text)
        .tags(tags)
        .custom_created_at(nostr::Timestamp::from(at))
        .sign_with_keys(keys)
        .unwrap()
}
async fn admitted(
    fixture: &Fixture,
    ctx: &mut PromptContext,
    human: &Keys,
    scope: &SessionScope,
    command: &str,
) -> crate::session_controls::ControlJob {
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    ctx.session_controls.configure_status_authority(
        crate::RespondTo::OwnerOnly,
        HashSet::new(),
        Some(human.public_key().to_hex()),
    );
    ctx.agent_owner_pubkey = Some(human.public_key());
    let job = super::control_tests::authorized_control(ctx, human, scope, command).await;
    assert!(ctx.session_controls.restart(job.clone()).unwrap());
    job
}
async fn dispatch(
    fixture: &Fixture,
    ctx: Arc<PromptContext>,
    env: &[(&str, &str)],
) -> crate::pool::restart_status::StatusTaskResult {
    let agent = fixture.agent(env).await;
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    let (tx, mut rx) = mpsc::unbounded_channel();
    crate::status_tasks::dispatch(&mut pool, &ctx, &tx);
    let result = timeout(Duration::from_secs(12), rx.recv())
        .await
        .unwrap()
        .unwrap();
    pool.join_set.join_next().await;
    result
}

#[tokio::test]
async fn checked_restart_depths_publish_one_signed_status_and_resume_without_bootstrap() {
    for (command, depth) in [
        ("!restart none", "none"),
        ("!restart", "recent"),
        ("!restart all", "all"),
    ] {
        let fixture = Fixture::new();
        let human = Keys::generate();
        let agent = Keys::generate();
        let channel = Uuid::new_v4();
        let root = event(
            &human,
            channel,
            None,
            "Please inspect the task and report progress",
            Some(&agent),
            1,
        );
        let root_id = root.id.to_hex();
        let progress = event(
            &human,
            channel,
            Some(&root_id),
            "The implementation is done; validation remains open",
            None,
            2,
        );
        let (mut ctx, relay) = setup(vec![root, progress], channel).await;
        ctx.agent_keys = agent;
        ctx.initial_message = Some("DO-NOT-RUN-BOOTSTRAP".into());
        let scope = SessionScope::Thread {
            channel_id: channel,
            root_event_id: root_id.clone(),
        };
        let job = admitted(&fixture, &mut ctx, &human, &scope, command).await;
        assert!(ctx.session_controls.pending_statuses().is_empty());
        ctx.session_controls.plan_retirement(&scope, [7]);
        assert!(ctx.session_controls.pending_statuses().is_empty());
        ctx.session_controls
            .retirement_report(7, crate::acp::TerminationReport::Clean);
        assert_eq!(ctx.session_controls.pending_statuses().len(), 1);
        let prompts = fixture.dir.path().join("status-prompts");
        let checks = fixture.dir.path().join("store");
        let ctx = Arc::new(ctx);
        let mut result = dispatch(
            &fixture,
            ctx.clone(),
            &[
                (
                    "STATUS_TEXT",
                    "The task is understood; validation is the next step.",
                ),
                ("PROMPTS", prompts.to_str().unwrap()),
                ("CHECK_STATUS_STARTED_ROOT", checks.to_str().unwrap()),
            ],
        )
        .await;
        let posts = relay.posts.lock().unwrap().clone();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].pubkey, ctx.agent_keys.public_key());
        assert_eq!(
            posts[0].content,
            "The task is understood; validation is the next step."
        );
        let tags = crate::queue::parse_thread_tags(&posts[0]);
        assert_eq!(tags.root_event_id, Some(root_id.clone()));
        assert!(tags.mentioned_pubkeys.is_empty());
        let record = fixture.record(&scope);
        assert_eq!(
            record.restart_status.unwrap().phase,
            RestartStatusPhase::Completed
        );
        assert!(record.pending_restart.is_none());
        assert!(!ctx.session_controls.restart(job).unwrap());
        assert!(ctx.session_controls.pending_statuses().is_empty());
        let prompt = std::fs::read_to_string(&prompts).unwrap();
        assert!(!prompt.contains("DO-NOT-RUN-BOOTSTRAP"));
        assert!(prompt.contains(&format!("user_restart:{depth}")));
        assert!(!prompt.contains("!restart"));
        assert!(prompt.contains("Please inspect the task"));
        if depth == "none" {
            assert!(!prompt.contains("implementation is done"));
        } else {
            assert!(prompt.contains("implementation is done"));
        }
        let sid = result.agent.state.sessions[&scope].clone();
        result.agent.state.control_dispatch.take().unwrap().finish();
        let later = event(
            &human,
            channel,
            Some(&root_id),
            "Continue with the actual validation task",
            Some(&ctx.agent_keys),
            3,
        );
        let mut resumed = run_turn(result.agent, &ctx, batch(&scope, later)).await;
        assert_eq!(resumed.state.sessions[&scope], sid);
        assert_eq!(
            fixture
                .methods()
                .iter()
                .filter(|method| *method == "session/new")
                .count(),
            1
        );
        resumed.acp.shutdown().await;
    }
}

#[tokio::test]
async fn failure_outputs_never_become_status_and_keep_restart_result_successful() {
    for extra in [
        vec![("STATUS_STOP", "refusal")],
        vec![("STATUS_STOP", "max_tokens")],
        vec![("FAIL_PROMPT", "1")],
        vec![("STATUS_TOOL", "1")],
        vec![("STATUS_PERMISSION", "1")],
        vec![("STATUS_WRONG_SESSION", "1")],
        vec![("STATUS_THOUGHT", "1"), ("STATUS_TEXT", "")],
    ] {
        let fixture = Fixture::new();
        let human = Keys::generate();
        let scope = scope();
        let (mut ctx, relay) = setup(vec![], scope.channel_id()).await;
        let job = admitted(&fixture, &mut ctx, &human, &scope, "!restart none").await;
        ctx.session_controls.plan_retirement(&scope, []);
        let mut env = vec![("STATUS_TEXT", "partial output must not be posted")];
        env.extend(extra);
        let mut result = dispatch(&fixture, Arc::new(ctx), &env).await;
        let posts = relay.posts.lock().unwrap().clone();
        assert_eq!(posts.len(), 1);
        assert!(posts[0].content.contains("couldn't confirm"));
        assert!(!posts[0].content.contains("partial output"));
        let record = fixture.record(&scope);
        assert_eq!(
            record
                .control_receipts
                .iter()
                .find(|receipt| receipt.command_id == job.origin.command_event_id)
                .unwrap()
                .state,
            crate::session_store::ControlReceiptState::Finished(Ok(()))
        );
        assert_eq!(
            record.restart_status.unwrap().phase,
            RestartStatusPhase::Failed
        );
        assert!(record.pending_restart.is_some());
        assert_eq!(
            fixture
                .methods()
                .iter()
                .filter(|method| *method == "session/prompt")
                .count(),
            1
        );
        if env.iter().any(|(name, _)| *name == "STATUS_PERMISSION") {
            timeout(Duration::from_secs(2), async {
                while !fixture.methods().contains(&"permission-denied".into()) {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
        }
        result.agent.acp.shutdown().await;
    }
}

#[tokio::test]
async fn selected_history_keeps_only_a_language_hint_from_omitted_russian_conversation() {
    for command in ["!restart none", "!restart", "!restart all"] {
        for language in ["ru", "en", "unknown"] {
            let fixture = Fixture::new();
            let human = Keys::generate();
            let agent = Keys::generate();
            let channel = Uuid::new_v4();
            let root = event(
                &human,
                channel,
                None,
                if language == "unknown" {
                    "???"
                } else {
                    "Please inspect the task and report progress"
                },
                Some(&agent),
                1,
            );
            let root_id = root.id.to_hex();
            let text = if language == "ru" {
                "Проверка завершена, осталось собрать итоговый результат"
            } else if language == "en" {
                "The validation is done and the next step is clear"
            } else {
                "???"
            };
            let progress = event(&human, channel, Some(&root_id), text, None, 2);
            let code=event(&human,channel,Some(&root_id),"```text\nThe task is done and we are ready\n```\n> The task is done\n2026-01-01 INFO the task is done\n!restart all",None,3);
            let sibling = event(
                &human,
                channel,
                Some(&"f".repeat(64)),
                "The task is done and we are ready",
                None,
                4,
            );
            let events = vec![root, progress, code, sibling];
            let (mut ctx, _relay) = setup(events, channel).await;
            ctx.agent_keys = agent;
            let scope = SessionScope::Thread {
                channel_id: channel,
                root_event_id: root_id,
            };
            admitted(&fixture, &mut ctx, &human, &scope, command).await;
            // Requester-only language policy works when no configured owner key
            // is available to the prompt layer.
            ctx.agent_owner_pubkey = None;
            ctx.session_controls.plan_retirement(&scope, []);
            let prompts = fixture.dir.path().join("language-prompts");
            let mut result = dispatch(
                &fixture,
                Arc::new(ctx),
                &[
                    ("STATUS_TEXT", "A useful status response"),
                    ("PROMPTS", prompts.to_str().unwrap()),
                ],
            )
            .await;
            let wire = std::fs::read_to_string(prompts).unwrap();
            let params: serde_json::Value =
                serde_json::from_str(wire.lines().next().unwrap()).unwrap();
            let prompt = params["prompt"][0]["text"].as_str().unwrap();
            assert!(
                prompt.contains(&format!("conversation-language=\"{language}\"")),
                "{prompt}"
            );
            let request = prompt
                .split_once("<restart-status ")
                .unwrap()
                .1
                .split_once("</restart-status>")
                .unwrap()
                .0;
            assert!(request.contains("Use the same language as the human conversation"));
            assert_eq!(
                request.contains("Write this status in Russian."),
                language == "ru",
                "{command} language={language}: {request}"
            );
            assert_eq!(
                request.contains("Write this status in English."),
                language == "en",
                "{command} language={language}: {request}"
            );
            if command == "!restart none" && language == "ru" {
                assert!(!prompt.contains(text));
            }
            result.agent.acp.shutdown().await;
        }
    }
}

#[tokio::test]
async fn lost_or_wrong_ack_retries_immutable_bytes_without_replaying_the_model() {
    for mode in ["lost", "wrong", "reject"] {
        let fixture = Fixture::new();
        let human = Keys::generate();
        let scope = scope();
        let (mut ctx, relay) = setup(vec![], scope.channel_id()).await;
        admitted(&fixture, &mut ctx, &human, &scope, "!restart none").await;
        ctx.session_controls.plan_retirement(&scope, []);
        *relay.mode.lock().unwrap() = mode.into();
        let controls = ctx.session_controls.clone();
        let mut result = dispatch(
            &fixture,
            Arc::new(ctx),
            &[("STATUS_TEXT", "Actual generated answer")],
        )
        .await;
        let posts = relay.posts.lock().unwrap().clone();
        assert_eq!(posts.len(), 6);
        assert!(posts[..3].iter().all(|event| event == &posts[0]));
        assert!(posts[3..].iter().all(|event| event == &posts[3]));
        assert_ne!(posts[0].id, posts[3].id);
        let saved = fixture.record(&scope).restart_status.unwrap();
        assert_eq!(saved.phase, RestartStatusPhase::Unconfirmed);
        assert_eq!(saved.prepared, Some(posts[0].clone()));
        assert_eq!(saved.notice, Some(posts[3].clone()));
        assert!(!saved.notice_confirmed);
        assert!(!controls.status_pending(&scope));
        assert!(controls.pending_statuses().is_empty());
        assert_eq!(
            fixture
                .methods()
                .iter()
                .filter(|method| *method == "session/prompt")
                .count(),
            1
        );
        result.agent.acp.shutdown().await;
    }
}

#[tokio::test]
async fn pending_status_never_merges_or_discards_new_human_work_and_sibling_is_available() {
    let fixture = Fixture::new();
    let human = Keys::generate();
    let scope = scope();
    let (mut ctx, _relay) = setup(vec![], scope.channel_id()).await;
    admitted(&fixture, &mut ctx, &human, &scope, "!restart none").await;
    ctx.session_controls.plan_retirement(&scope, []);
    let ctx = Arc::new(ctx);
    let agent = fixture.agent(&[]).await;
    let mut pool = AgentPool::from_slots(vec![Some(agent)]);
    let mut queue = crate::EventQueue::new(crate::DedupMode::Queue);
    let later = event(
        &human,
        scope.channel_id(),
        scope.root_event_id(),
        "Preserve this actual task instruction",
        Some(&ctx.agent_keys),
        nostr::Timestamp::now().as_secs(),
    );
    queue.push(crate::QueuedEvent {
        scope: scope.clone(),
        channel_id: scope.channel_id(),
        event: later.clone(),
        received_at: std::time::Instant::now(),
        prompt_tag: "test".into(),
        edit: None,
    });
    let mut activity = tokio::time::Instant::now();
    assert!(crate::dispatch_pending(&mut pool, &mut queue, &ctx, &mut activity, None).is_empty());
    assert!(queue.has_undispatched_work());
    let sibling = SessionScope::Thread {
        channel_id: scope.channel_id(),
        root_event_id: "c".repeat(64),
    };
    let sibling_event = event(
        &human,
        sibling.channel_id(),
        sibling.root_event_id(),
        "Execute this sibling task",
        Some(&ctx.agent_keys),
        nostr::Timestamp::now().as_secs(),
    );
    queue.push(crate::QueuedEvent {
        scope: sibling.clone(),
        channel_id: sibling.channel_id(),
        event: sibling_event,
        received_at: std::time::Instant::now(),
        prompt_tag: "test".into(),
        edit: None,
    });
    let dispatched = crate::dispatch_pending(&mut pool, &mut queue, &ctx, &mut activity, None);
    assert_eq!(dispatched.len(), 1);
    assert_eq!(dispatched[0].0, sibling);
    let mut completed = timeout(Duration::from_secs(5), pool.rx_and_join_set().0.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(completed.outcome, PromptOutcome::Ok(_)));
    assert!(ctx.session_controls.status_pending(&scope));
    assert!(ctx.session_controls.cancel_status(&scope));
    let batch = queue.flush_next().unwrap();
    assert_eq!(batch.events.len(), 1);
    assert_eq!(batch.events[0].event.id, later.id);
    assert!(batch.cancelled_events.is_empty());
    assert_eq!(
        fixture.record(&scope).restart_status.unwrap().phase,
        RestartStatusPhase::Cancelled
    );
    completed.agent.acp.shutdown().await;
    pool.join_set.join_next().await;
}

#[tokio::test]
async fn failed_retirement_store_failure_and_startup_failure_never_fabricate_status_success() {
    for failure in ["unverified", "store", "startup"] {
        let fixture = Fixture::new();
        let human = Keys::generate();
        let scope = scope();
        let (mut ctx, relay) = setup(vec![], scope.channel_id()).await;
        admitted(&fixture, &mut ctx, &human, &scope, "!restart none").await;
        ctx.session_controls.plan_retirement(&scope, [7]);
        if failure == "unverified" {
            ctx.session_controls
                .retirement_report(7, crate::acp::TerminationReport::Unverified);
            assert!(ctx.session_controls.pending_statuses().is_empty());
            assert!(relay.posts.lock().unwrap().is_empty());
            continue;
        }
        ctx.session_controls
            .retirement_report(7, crate::acp::TerminationReport::Clean);
        if failure == "store" {
            fixture.store.fail_writes(true);
        }
        let env = if failure == "startup" {
            vec![("FAIL_NEW", "1")]
        } else {
            vec![]
        };
        let mut result = dispatch(&fixture, Arc::new(ctx), &env).await;
        assert_eq!(
            fixture
                .methods()
                .iter()
                .filter(|method| *method == "session/prompt")
                .count(),
            0
        );
        let saved = fixture.record(&scope).restart_status.unwrap();
        assert_ne!(saved.phase, RestartStatusPhase::Completed);
        assert!(relay
            .posts
            .lock()
            .unwrap()
            .iter()
            .all(|post| post.content.contains("couldn't confirm")));
        fixture.store.fail_writes(false);
        result.agent.acp.shutdown().await;
    }
}

#[tokio::test]
async fn late_model_results_are_fenced_by_new_restart_cancel_removal_shutdown_and_authority() {
    for action in ["restart", "cancel", "remove", "shutdown", "authority"] {
        let fixture = Fixture::new();
        let human = Keys::generate();
        let scope = scope();
        let (mut ctx, relay) = setup(vec![], scope.channel_id()).await;
        admitted(&fixture, &mut ctx, &human, &scope, "!restart none").await;
        ctx.session_controls.plan_retirement(&scope, []);
        let controls = ctx.session_controls.clone();
        let old_token = controls.restart_intent(&scope).unwrap().token;
        let ctx = Arc::new(ctx);
        let log = fixture.log();
        let env = [
            ("PROMPT_DELAY", "0.2"),
            ("STATUS_TEXT", "This obsolete status must be suppressed"),
        ];
        let work = dispatch(&fixture, ctx.clone(), &env);
        let interrupt = async {
            timeout(Duration::from_secs(5), async {
                while !std::fs::read_to_string(&log)
                    .unwrap_or_default()
                    .contains("session/prompt")
                {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            match action {
                "restart" => {
                    let later = super::control_tests::authorized_control(
                        &ctx,
                        &human,
                        &scope,
                        "!restart all",
                    )
                    .await;
                    assert!(controls.restart(later).unwrap());
                }
                "cancel" => {
                    assert!(controls.cancel_status(&scope));
                }
                "remove" => {
                    controls.revoke_channel(scope.channel_id()).unwrap();
                }
                "shutdown" => {
                    assert!(controls.shutdown_statuses().contains(&scope));
                }
                "authority" => controls.configure_status_authority(
                    crate::RespondTo::Nobody,
                    HashSet::new(),
                    Some(human.public_key().to_hex()),
                ),
                _ => unreachable!(),
            }
        };
        let (mut result, ()) = tokio::join!(work, interrupt);
        assert!(relay.posts.lock().unwrap().is_empty());
        assert_eq!(
            fixture
                .methods()
                .iter()
                .filter(|method| *method == "session/prompt")
                .count(),
            1
        );
        if action == "restart" {
            let current = controls.restart_intent(&scope).unwrap();
            assert_ne!(current.token, old_token);
            assert_eq!(current.depth, buzz_core::session_control::RestartDepth::All);
            assert!(!controls.cancel_status_for_token(&scope, old_token));
            assert_eq!(
                controls.restart_intent(&scope).unwrap().token,
                current.token
            );
        }
        result.agent.acp.shutdown().await;
    }
}

#[tokio::test]
async fn pending_observer_cancel_and_expiry_do_not_require_a_worker_or_drop_human_input() {
    let fixture = Fixture::new();
    let human = Keys::generate();
    let scope = scope();
    let (mut ctx, relay) = setup(vec![], scope.channel_id()).await;
    admitted(&fixture, &mut ctx, &human, &scope, "!restart none").await;
    ctx.session_controls.plan_retirement(&scope, []);
    let mut pool = AgentPool::from_slots(vec![]);
    let mut queue = crate::EventQueue::new(crate::DedupMode::Queue);
    crate::handle_cancel_turn_control_with_status(
        &serde_json::json!({"channelId":scope.channel_id().to_string()}),
        &mut pool,
        &mut queue,
        None,
        Some(&ctx.session_controls),
    );
    assert!(!ctx.session_controls.status_pending(&scope));
    assert_eq!(
        fixture.record(&scope).restart_status.unwrap().phase,
        RestartStatusPhase::Cancelled
    );
    let next = super::control_tests::authorized_control(&ctx, &human, &scope, "!restart all").await;
    assert!(ctx.session_controls.restart(next).unwrap());
    ctx.session_controls.plan_retirement(&scope, []);
    // Use the real pending-job deadline and scheduler with virtual monotonic
    // time. No provider slot exists, so this can only publish failure feedback.
    let ctx = Arc::new(ctx);
    let (tx, _rx) = mpsc::unbounded_channel();
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(121)).await;
    crate::status_tasks::dispatch(&mut pool, &ctx, &tx);
    assert!(!ctx.session_controls.status_pending(&scope));
    tokio::time::resume();
    timeout(Duration::from_secs(8), pool.join_set.join_next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(relay.posts.lock().unwrap().len(), 1);
    assert!(relay.posts.lock().unwrap()[0]
        .content
        .contains("couldn't confirm"));
    assert_eq!(
        fixture
            .methods()
            .iter()
            .filter(|method| *method == "session/prompt")
            .count(),
        0
    );
}

#[tokio::test]
async fn reconstruction_claims_only_dead_owners_and_never_replays_started_model_attempts() {
    for phase in [
        "waiting",
        "pending",
        "started",
        "prepared",
        "completed",
        "cancel_uncertain",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let keys = Keys::generate();
        let agent_hex = keys.public_key().to_hex();
        let store = Arc::new(
            SessionStore::open(dir.path().join("store"), "wss://relay.example", &agent_hex)
                .unwrap(),
        );
        let fixture = Fixture { dir, store };
        let human = Keys::generate();
        let scope = scope();
        let (mut ctx, relay) = setup(vec![], scope.channel_id()).await;
        ctx.agent_keys = keys;
        admitted(&fixture, &mut ctx, &human, &scope, "!restart none").await;
        if phase != "waiting" {
            ctx.session_controls.plan_retirement(&scope, []);
        }
        let mut prepared = None;
        if matches!(phase, "started" | "prepared" | "completed") {
            let mut job = ctx.session_controls.pending_statuses().pop().unwrap();
            ctx.session_controls
                .transition_status(&mut job, RestartStatusPhase::Started, None, false, false)
                .unwrap();
            if phase != "started" {
                let tags = crate::queue::ThreadTags {
                    root_event_id: scope.root_event_id().map(str::to_owned),
                    parent_event_id: scope.root_event_id().map(str::to_owned),
                    ..Default::default()
                };
                let signed = crate::pool::build_failure_notice_event(
                    &ctx.agent_keys,
                    scope.channel_id(),
                    &tags,
                    "Persisted actual model-authored reply",
                )
                .unwrap();
                prepared = Some(signed.clone());
                ctx.session_controls
                    .transition_status(
                        &mut job,
                        RestartStatusPhase::Prepared,
                        Some(signed.clone()),
                        false,
                        false,
                    )
                    .unwrap();
                // The store seam rejects attempts to change prepared bytes.
                let changed = crate::pool::build_failure_notice_event(
                    &ctx.agent_keys,
                    scope.channel_id(),
                    &tags,
                    "A duplicate replacement answer",
                )
                .unwrap();
                assert!(ctx
                    .session_controls
                    .transition_status(
                        &mut job,
                        RestartStatusPhase::Prepared,
                        Some(changed),
                        false,
                        false
                    )
                    .is_err());
                if phase == "completed" {
                    ctx.session_controls
                        .transition_status(
                            &mut job,
                            RestartStatusPhase::Completed,
                            Some(signed),
                            false,
                            true,
                        )
                        .unwrap();
                }
            }
        }
        if phase == "cancel_uncertain" {
            fixture.store.fail_writes(true);
            assert_eq!(
                ctx.session_controls.cancel_status_outcome(&scope),
                crate::session_controls::status::StatusCancellation::Unconfirmed
            );
            fixture.store.fail_writes(false);
            assert!(!ctx.session_controls.status_pending(&scope));
        }
        let live_probe = SessionStore::open(
            fixture.dir.path().join("store"),
            "wss://relay.example",
            &agent_hex,
        )
        .unwrap();
        assert!(
            live_probe
                .sweep_orphan_controls(128)
                .unwrap()
                .reconciled_statuses
                == 0
        );
        drop(live_probe);
        ctx.session_controls = Default::default();
        let Fixture { dir, store } = fixture;
        drop(store);
        let reopened = Arc::new(
            SessionStore::open(dir.path().join("store"), "wss://relay.example", &agent_hex)
                .unwrap(),
        );
        let fixture = Fixture {
            dir,
            store: reopened.clone(),
        };
        ctx.session_controls = crate::session_controls::SessionControlRuntime::new(Some(reopened));
        ctx.session_controls.configure_status_authority(
            crate::RespondTo::OwnerOnly,
            HashSet::new(),
            Some(human.public_key().to_hex()),
        );
        ctx.session_controls.sweep_orphans(128).unwrap();
        assert!(ctx.session_controls.pending_statuses().is_empty());
        assert!(relay.posts.lock().unwrap().is_empty());
        assert_eq!(
            fixture
                .methods()
                .iter()
                .filter(|method| *method == "session/prompt")
                .count(),
            0
        );
        let saved = fixture.record(&scope);
        assert_eq!(
            saved.restart_status.as_ref().unwrap().phase,
            if phase == "completed" {
                RestartStatusPhase::Completed
            } else {
                RestartStatusPhase::Unconfirmed
            }
        );
        assert_eq!(saved.restart_status.unwrap().prepared, prepared);
        assert_eq!(saved.pending_restart.is_some(), phase != "completed");
    }
}

#[path = "restart_status_edge_tests.rs"]
mod edge_tests;

#[path = "restart_status_review_tests.rs"]
mod review_tests;

#[path = "restart_status_unavailable_tests.rs"]
mod unavailable_tests;
