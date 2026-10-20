//! Review regressions bind language admission and pre-first-poll ownership.
use super::*;

async fn admit_nonowner(
    fixture: &Fixture,
    ctx: &mut PromptContext,
    owner: &Keys,
    requester: &Keys,
    scope: &SessionScope,
    command: &str,
) {
    assert_ne!(owner.public_key(), requester.public_key());
    let allowlist = HashSet::from([requester.public_key().to_hex()]);
    ctx.agent_owner_pubkey = Some(owner.public_key());
    ctx.session_controls =
        crate::session_controls::SessionControlRuntime::new(Some(fixture.store.clone()));
    ctx.session_controls.configure_status_authority(
        crate::RespondTo::Allowlist,
        allowlist.clone(),
        Some(owner.public_key().to_hex()),
    );
    let event = event(
        requester,
        scope.channel_id(),
        scope.root_event_id(),
        command,
        Some(&ctx.agent_keys),
        nostr::Timestamp::now().as_secs(),
    );
    let owner_cache = crate::OwnerCache::new(Some(owner.public_key().to_hex()));
    let mut gate = crate::InboundAuthorGate::connect(
        &ctx.rest_client,
        &ctx.agent_keys.public_key().to_hex(),
        "status language test",
    )
    .await;
    let authorized = crate::authorize_normal_listener_event(
        &mut gate,
        crate::relay::BuzzEvent {
            channel_id: scope.channel_id(),
            event,
            connection_generation: 0,
        },
        &crate::RespondTo::Allowlist,
        &allowlist,
        &owner_cache,
        &ctx.channel_info,
        &ctx.rest_client,
    )
    .await
    .unwrap();
    let crate::session_controls::Admission::Control(job) =
        crate::session_controls::admit_authorized(
            authorized,
            ctx,
            nostr::Timestamp::now().as_secs(),
        )
        .await
    else {
        panic!("allowlisted nonowner must be admitted");
    };
    assert_eq!(job.origin.actor_pubkey, requester.public_key().to_hex());
    assert!(ctx.session_controls.restart(job).unwrap());
}

#[tokio::test]
async fn numbered_and_unsupported_latest_prose_replace_old_english_at_every_depth() {
    for command in ["!restart none", "!restart", "!restart all"] {
        for nonowner in [false, true] {
            for (text, language) in [
                ("1. Реализация готова\n2. Остались проверки.", "ru"),
                (
                    "La réalisation est terminée; quelques vérifications restent nécessaires.",
                    "unknown",
                ),
            ] {
                let fixture = Fixture::new();
                let owner = Keys::generate();
                let requester = if nonowner {
                    Keys::generate()
                } else {
                    owner.clone()
                };
                let agent = Keys::generate();
                let channel = Uuid::new_v4();
                let root = event(
                    &owner,
                    channel,
                    None,
                    "Please inspect the task and report progress",
                    Some(&agent),
                    1,
                );
                let root_id = root.id.to_hex();
                let update = event(&requester, channel, Some(&root_id), text, None, 2);
                let non_evidence = event(&requester, channel, Some(&root_id),
                    "```text\nThe task is done and we are ready\n```\n> The task is done\n2026-01-01T12:00:00Z INFO the task is done\n[12:01:02] INFO the task is done\n!restart all\n$ echo the task is done", None, 3);
                let sibling = event(
                    &requester,
                    channel,
                    Some(&"f".repeat(64)),
                    "The task is done and we are ready",
                    None,
                    4,
                );
                let (mut ctx, relay) =
                    setup(vec![root, update, non_evidence, sibling], channel).await;
                ctx.agent_keys = agent;
                let scope = SessionScope::Thread {
                    channel_id: channel,
                    root_event_id: root_id,
                };
                if nonowner {
                    admit_nonowner(&fixture, &mut ctx, &owner, &requester, &scope, command).await;
                } else {
                    admitted(&fixture, &mut ctx, &owner, &scope, command).await;
                }
                ctx.session_controls.plan_retirement(&scope, []);
                let prompts = fixture.dir.path().join("review-language-prompts");
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
                    "{command} nonowner={nonowner}: {prompt}"
                );
                if command == "!restart none" {
                    assert!(!prompt.contains(text));
                }
                assert_eq!(relay.posts.lock().unwrap().len(), 1);
                assert_eq!(
                    fixture.record(&scope).restart_status.unwrap().phase,
                    RestartStatusPhase::Completed
                );
                result.agent.acp.shutdown().await;
            }
        }
    }
}

#[tokio::test]
async fn scheduler_keeps_its_original_fence_before_the_first_status_poll() {
    for action in ["failed_cancel", "restart", "remove", "authority"] {
        let fixture = Fixture::new();
        let human = Keys::generate();
        let scope = scope();
        let (mut ctx, relay) = setup(vec![], scope.channel_id()).await;
        admitted(&fixture, &mut ctx, &human, &scope, "!restart none").await;
        ctx.session_controls.plan_retirement(&scope, []);
        let later =
            super::super::control_tests::authorized_control(&ctx, &human, &scope, "!restart all")
                .await;
        let old_token = ctx.session_controls.restart_intent(&scope).unwrap().token;
        let agent = fixture
            .agent(&[("STATUS_TEXT", "An obsolete answer")])
            .await;
        let mut pool = AgentPool::from_slots(vec![Some(agent)]);
        let ctx = Arc::new(ctx);
        let (tx, mut rx) = mpsc::unbounded_channel();
        crate::status_tasks::dispatch(&mut pool, &ctx, &tx);
        // No await: the actual scheduled future cannot have been polled yet.
        match action {
            "failed_cancel" => {
                fixture.store.fail_writes(true);
                assert_eq!(
                    ctx.session_controls.cancel_status_outcome(&scope),
                    crate::session_controls::status::StatusCancellation::Unconfirmed
                );
                // Persistence recovers before the stale future is first polled.
                fixture.store.fail_writes(false);
            }
            "restart" => {
                assert!(ctx.session_controls.restart(later).unwrap());
            }
            "remove" => ctx
                .session_controls
                .revoke_channel(scope.channel_id())
                .unwrap(),
            "authority" => ctx.session_controls.configure_status_authority(
                crate::RespondTo::Nobody,
                HashSet::new(),
                Some(human.public_key().to_hex()),
            ),
            _ => unreachable!(),
        }
        let mut result = timeout(Duration::from_secs(8), rx.recv())
            .await
            .unwrap()
            .unwrap();
        pool.join_set.join_next().await.unwrap().unwrap();
        assert!(relay.posts.lock().unwrap().is_empty(), "{action}");
        assert!(
            !fixture
                .methods()
                .iter()
                .any(|method| method == "session/prompt" || method == "session/new"),
            "{action}"
        );
        if action == "restart" {
            assert_ne!(
                ctx.session_controls.restart_intent(&scope).unwrap().token,
                old_token
            );
            assert_eq!(
                fixture.record(&scope).restart_status.unwrap().phase,
                RestartStatusPhase::WaitingRetirement
            );
        }
        result.agent.acp.shutdown().await;
    }
}

#[tokio::test]
async fn expiry_scheduler_keeps_its_fence_when_cancellation_persistence_recovers() {
    let fixture = Fixture::new();
    let human = Keys::generate();
    let scope = scope();
    let (mut ctx, relay) = setup(vec![], scope.channel_id()).await;
    admitted(&fixture, &mut ctx, &human, &scope, "!restart none").await;
    ctx.session_controls.plan_retirement(&scope, []);
    let ctx = Arc::new(ctx);
    let mut pool = AgentPool::from_slots(vec![]);
    let (tx, _rx) = mpsc::unbounded_channel();
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(121)).await;
    crate::status_tasks::dispatch(&mut pool, &ctx, &tx);
    fixture.store.fail_writes(true);
    assert_eq!(
        ctx.session_controls.cancel_status_outcome(&scope),
        crate::session_controls::status::StatusCancellation::Unconfirmed
    );
    fixture.store.fail_writes(false);
    tokio::time::resume();
    timeout(Duration::from_secs(8), pool.join_set.join_next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(relay.posts.lock().unwrap().is_empty());
    assert!(fixture
        .record(&scope)
        .restart_status
        .unwrap()
        .notice
        .is_none());
}

#[tokio::test]
async fn uncertain_owner_cancel_has_no_chat_sender_and_fences_held_feedback() {
    for action in ["cancel", "restart", "remove", "authority"] {
        let fixture = Fixture::new();
        let human = Keys::generate();
        let scope = scope();
        let (mut ctx, relay) = setup(vec![], scope.channel_id()).await;
        admitted(&fixture, &mut ctx, &human, &scope, "!restart none").await;
        ctx.session_controls.plan_retirement(&scope, []);
        let job = ctx.session_controls.pending_statuses().pop().unwrap();
        let dispatch = ctx.session_controls.capture(&scope);
        let old_token = job.saved.token;
        let later =
            super::super::control_tests::authorized_control(&ctx, &human, &scope, "!restart all")
                .await;
        let ctx = Arc::new(ctx);
        let held = crate::pool::restart_status::report_expired(job, ctx.clone(), dispatch.clone());
        let mut pool = AgentPool::from_slots(vec![]);
        let mut queue = crate::EventQueue::new(crate::DedupMode::Queue);
        let observer = crate::observer::ObserverHandle::in_process();
        fixture.store.fail_writes(true);
        assert_eq!(
            crate::handle_owner_cancel_for_scope(
                &mut pool,
                &mut queue,
                &ctx.session_controls,
                &scope,
                Some(&observer)
            ),
            crate::session_controls::status::StatusCancellation::Unconfirmed
        );
        assert!(!dispatch.current());
        assert_eq!(
            observer.snapshot().last().unwrap().payload["status"],
            "unconfirmed"
        );
        assert_eq!(
            fixture.record(&scope).restart_status.unwrap().phase,
            RestartStatusPhase::Pending
        );
        fixture.store.fail_writes(false);
        match action {
            "cancel" => {
                ctx.session_controls.cancel_status_outcome(&scope);
            }
            "restart" => {
                assert!(ctx.session_controls.restart(later).unwrap());
            }
            "remove" => ctx
                .session_controls
                .revoke_channel(scope.channel_id())
                .unwrap(),
            "authority" => ctx.session_controls.configure_status_authority(
                crate::RespondTo::Nobody,
                HashSet::new(),
                Some(human.public_key().to_hex()),
            ),
            _ => unreachable!(),
        }
        held.await;
        // Drain a turn too: an accidentally detached owner notice must surface.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(relay.posts.lock().unwrap().is_empty(), "{action}");
        assert!(fixture
            .record(&scope)
            .restart_status
            .unwrap()
            .notice
            .is_none());
        if action == "restart" {
            assert_ne!(
                ctx.session_controls.restart_intent(&scope).unwrap().token,
                old_token
            );
            assert_eq!(
                fixture.record(&scope).restart_status.unwrap().phase,
                RestartStatusPhase::WaitingRetirement
            );
        }
    }
}

#[tokio::test]
async fn owned_failure_feedback_requires_persistence_and_retries_only_its_saved_bytes() {
    for failure in ["persist", "lost", "wrong", "reject"] {
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
        let job = ctx
            .session_controls
            .status_panicked(&scope, job.saved.token)
            .unwrap();
        let dispatch = ctx.session_controls.capture(&scope);
        if failure == "persist" {
            fixture.store.fail_writes(true);
        } else {
            *relay.mode.lock().unwrap() = failure.into();
        }
        crate::pool::restart_status::report_expired(job, Arc::new(ctx), dispatch).await;
        let saved = fixture.record(&scope);
        assert!(saved.pending_restart.is_some());
        let status = saved.restart_status.unwrap();
        let posts = relay.posts.lock().unwrap().clone();
        if failure == "persist" {
            assert!(posts.is_empty());
            assert_eq!(status.phase, RestartStatusPhase::Started);
            assert!(status.notice.is_none());
        } else {
            assert_eq!(posts.len(), 3);
            assert!(posts
                .iter()
                .all(|post| Some(post) == status.notice.as_ref()));
            assert!(posts[0].content.contains("couldn't confirm"));
            assert_eq!(status.phase, RestartStatusPhase::Unconfirmed);
            assert!(!status.notice_confirmed);
            assert!(status.prepared.is_none());
        }
        assert!(!fixture
            .methods()
            .iter()
            .any(|method| method == "session/prompt"));
        fixture.store.fail_writes(false);
    }
}
