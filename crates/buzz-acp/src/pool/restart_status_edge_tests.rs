//! Additional production-path failure and routing boundaries.
use super::*;

#[tokio::test]
async fn overflow_empty_thought_and_rate_limited_output_never_publish_partial_answers() {
    let overflow = "x".repeat(crate::session_store::STATUS_TEXT_BYTES + 1);
    for env in [
        vec![("STATUS_TEXT", overflow.as_str())],
        vec![("STATUS_TEXT", "")],
        vec![("STATUS_THOUGHT", "1")],
        vec![
            ("STATUS_TEXT", "partial"),
            ("STATUS_STOP", "max_turn_requests"),
        ],
        vec![
            ("STATUS_TEXT", "partial"),
            ("FAIL_PROMPT", "1"),
            ("FAIL_PROMPT_CODE", "429"),
        ],
    ] {
        let fixture = Fixture::new();
        let human = Keys::generate();
        let scope = scope();
        let (mut ctx, relay) = setup(vec![], scope.channel_id()).await;
        admitted(&fixture, &mut ctx, &human, &scope, "!restart none").await;
        ctx.session_controls.plan_retirement(&scope, []);
        let mut result = dispatch(&fixture, Arc::new(ctx), &env).await;
        let posts = relay.posts.lock().unwrap().clone();
        assert_eq!(posts.len(), 1);
        assert!(posts[0].content.contains("couldn't confirm"));
        assert!(!posts[0].content.contains("partial"));
        assert_eq!(
            fixture.record(&scope).restart_status.unwrap().phase,
            RestartStatusPhase::Failed
        );
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
async fn dm_and_channel_policy_keep_the_canonical_original_reply_route() {
    for dm in [true, false] {
        let fixture = Fixture::new();
        let human = Keys::generate();
        let agent = Keys::generate();
        let channel = Uuid::new_v4();
        let root = event(
            &human,
            channel,
            None,
            "Please explain the current task",
            Some(&agent),
            1,
        );
        let root_id = root.id.to_hex();
        let (mut ctx, relay) = setup(vec![root], channel).await;
        ctx.agent_keys = agent;
        ctx.session_policy = if dm {
            crate::scope::SessionPolicy::Thread
        } else {
            crate::scope::SessionPolicy::Channel
        };
        let info = crate::relay::ChannelInfo {
            name: "conversation".into(),
            channel_type: if dm { "dm" } else { "stream" }.into(),
            description: None,
        };
        ctx.channel_info =
            ChannelInfoResolver::new(HashMap::from([(channel, info)]), ctx.rest_client.clone());
        let scope = SessionScope::Conversation {
            channel_id: channel,
        };
        admitted(&fixture, &mut ctx, &human, &scope, "!restart none").await;
        ctx.session_controls.plan_retirement(&scope, []);
        let mut result = dispatch(
            &fixture,
            Arc::new(ctx),
            &[("STATUS_TEXT", "The retained assignment is understood")],
        )
        .await;
        let posts = relay.posts.lock().unwrap().clone();
        assert_eq!(posts.len(), 1);
        let route = crate::queue::parse_thread_tags(&posts[0]);
        assert!(route.mentioned_pubkeys.is_empty());
        assert_eq!(
            route.root_event_id,
            if dm { None } else { Some("a".repeat(64)) }
        );
        // The channel-policy control fixture explicitly replies to its own
        // canonical root; a DM ignores thread tags and scopes the conversation.
        assert_ne!(route.root_event_id, Some(root_id));
        result.agent.acp.shutdown().await;
    }
}

#[tokio::test]
async fn status_acquisition_respects_a_shorter_existing_task_deadline() {
    let fixture = Fixture::new();
    let human = Keys::generate();
    let scope = scope();
    let (mut ctx, relay) = setup(vec![], scope.channel_id()).await;
    admitted(&fixture, &mut ctx, &human, &scope, "!restart none").await;
    ctx.session_controls.plan_retirement(&scope, []);
    ctx.max_turn_duration = Duration::from_millis(50);
    let started = std::time::Instant::now();
    let mut result = dispatch(
        &fixture,
        Arc::new(ctx),
        &[("DELAY", "0.5"), ("STATUS_TEXT", "must not run")],
    )
    .await;
    assert!(started.elapsed() < Duration::from_secs(8));
    assert_eq!(
        fixture
            .methods()
            .iter()
            .filter(|method| *method == "session/prompt")
            .count(),
        0
    );
    assert_ne!(
        fixture.record(&scope).restart_status.unwrap().phase,
        RestartStatusPhase::Completed
    );
    assert!(relay
        .posts
        .lock()
        .unwrap()
        .iter()
        .all(|post| post.content.contains("couldn't confirm")));
    result.agent.acp.shutdown().await;
}

#[tokio::test]
async fn a_failed_preparation_write_keeps_the_attempt_record_without_publishing_any_text() {
    let fixture = Fixture::new();
    let human = Keys::generate();
    let scope = scope();
    let (mut ctx, relay) = setup(vec![], scope.channel_id()).await;
    admitted(&fixture, &mut ctx, &human, &scope, "!restart none").await;
    ctx.session_controls.plan_retirement(&scope, []);
    let controls = ctx.session_controls.clone();
    let ctx = Arc::new(ctx);
    let log = fixture.log();
    let env = [
        ("PROMPT_DELAY", "0.2"),
        ("STATUS_TEXT", "A complete but unpersisted answer"),
    ];
    let work = dispatch(&fixture, ctx, &env);
    let fail = async {
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
        fixture.store.fail_writes(true);
    };
    let (mut result, ()) = tokio::join!(work, fail);
    assert!(relay.posts.lock().unwrap().is_empty());
    assert_eq!(
        fixture.record(&scope).restart_status.unwrap().phase,
        RestartStatusPhase::Started
    );
    assert!(!controls.status_pending(&scope));
    fixture.store.fail_writes(false);
    result.agent.acp.shutdown().await;
}

#[tokio::test]
async fn a_verified_agent_requester_cannot_supply_the_human_language_hint() {
    let fixture = Fixture::new();
    let requester = Keys::generate();
    let owner = Keys::generate();
    let agent = Keys::generate();
    let channel = Uuid::new_v4();
    let root = event(
        &requester,
        channel,
        None,
        "Проверка завершена, осталось собрать результат",
        Some(&agent),
        1,
    );
    let root_id = root.id.to_hex();
    let tag = buzz_sdk::nip_oa::compute_auth_tag(&owner, &requester.public_key(), "").unwrap();
    let profile = EventBuilder::new(Kind::Metadata, "{}")
        .tags([buzz_sdk::nip_oa::parse_auth_tag(&tag).unwrap()])
        .sign_with_keys(&requester)
        .unwrap();
    let (mut ctx, _relay) = setup(vec![root, profile], channel).await;
    ctx.agent_keys = agent;
    let scope = SessionScope::Thread {
        channel_id: channel,
        root_event_id: root_id,
    };
    admitted(&fixture, &mut ctx, &requester, &scope, "!restart none").await;
    ctx.agent_owner_pubkey = None;
    ctx.session_controls.plan_retirement(&scope, []);
    let prompts = fixture.dir.path().join("agent-language");
    let mut result = dispatch(
        &fixture,
        Arc::new(ctx),
        &[
            ("STATUS_TEXT", "A short status"),
            ("PROMPTS", prompts.to_str().unwrap()),
        ],
    )
    .await;
    let wire = std::fs::read_to_string(prompts).unwrap();
    let params: serde_json::Value = serde_json::from_str(wire.lines().next().unwrap()).unwrap();
    assert!(params["prompt"][0]["text"]
        .as_str()
        .unwrap()
        .contains("conversation-language=\"unknown\""));
    result.agent.acp.shutdown().await;
}

#[tokio::test]
async fn acknowledgment_without_a_completion_write_retains_prepared_bytes_and_token() {
    let fixture = Fixture::new();
    let human = Keys::generate();
    let scope = scope();
    let (mut ctx, relay) = setup(vec![], scope.channel_id()).await;
    admitted(&fixture, &mut ctx, &human, &scope, "!restart none").await;
    ctx.session_controls.plan_retirement(&scope, []);
    *relay.on_post.lock().unwrap() = Some(fixture.store.clone());
    let mut result = dispatch(
        &fixture,
        Arc::new(ctx),
        &[("STATUS_TEXT", "Actual acknowledged model text")],
    )
    .await;
    let posts = relay.posts.lock().unwrap().clone();
    assert_eq!(posts.len(), 1);
    let saved = fixture.record(&scope);
    assert!(saved.pending_restart.is_some());
    let status = saved.restart_status.unwrap();
    assert_eq!(status.phase, RestartStatusPhase::Prepared);
    assert_eq!(status.prepared, Some(posts[0].clone()));
    assert_eq!(
        fixture
            .methods()
            .iter()
            .filter(|method| *method == "session/prompt")
            .count(),
        1
    );
    fixture.store.fail_writes(false);
    *relay.on_post.lock().unwrap() = None;
    result.agent.acp.shutdown().await;
}

#[tokio::test]
async fn a_known_local_panic_reports_failure_without_restarting_the_model() {
    let fixture = Fixture::new();
    let human = Keys::generate();
    let scope = scope();
    let (mut ctx, relay) = setup(vec![], scope.channel_id()).await;
    admitted(&fixture, &mut ctx, &human, &scope, "!restart none").await;
    ctx.session_controls.plan_retirement(&scope, []);
    let mut job = ctx.session_controls.pending_statuses().pop().unwrap();
    ctx.session_controls
        .transition_status(&mut job, RestartStatusPhase::Started, None, false, false)
        .unwrap();
    let token = job.saved.token;
    let panic = ctx.session_controls.status_panicked(&scope, token).unwrap();
    assert!(!ctx.session_controls.status_pending(&scope));
    let dispatch = ctx.session_controls.capture(&scope);
    crate::pool::restart_status::report_expired(panic, Arc::new(ctx), dispatch).await;
    assert_eq!(
        fixture.record(&scope).restart_status.unwrap().phase,
        RestartStatusPhase::Failed
    );
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
