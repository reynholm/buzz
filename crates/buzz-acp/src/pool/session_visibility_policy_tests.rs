//! Scope-provenance regressions through the real prompt/status path.
use super::*;
use crate::scope::SessionPolicy;

fn metadata(ctx: &PromptContext, channel_id: Uuid, kind: Option<&str>) {
    let mut cache = ctx.channel_info.cache.write().unwrap();
    cache.remove(&channel_id);
    if let Some(kind) = kind {
        cache.insert(
            channel_id,
            super::super::super::PromptChannelInfo {
                name: "test".into(),
                channel_type: kind.into(),
                description: None,
                project: None,
            },
        );
    }
}

async fn fixture(h: &Harness, policy: SessionPolicy) -> (PromptContext, Relay) {
    let mut ctx = h.ctx();
    ctx.session_policy = policy;
    let server = relay(
        RelayScript {
            events: vec![],
            fail_scan: false,
            fail_history: false,
        },
        &mut ctx,
        Uuid::from_u128(42),
    )
    .await;
    (ctx, server)
}

#[tokio::test]
async fn conversation_status_requires_channel_policy_or_verified_dm() {
    for (policy, kind, eligible) in [
        (SessionPolicy::Thread, None, false),
        (SessionPolicy::Thread, Some("unknown"), false),
        (SessionPolicy::Thread, Some("stream"), false),
        (SessionPolicy::Thread, Some("dm"), true),
        (SessionPolicy::Channel, None, true),
    ] {
        let h = Harness::new();
        let (ctx, server) = fixture(&h, policy).await;
        let scope = SessionScope::Conversation {
            channel_id: Uuid::from_u128(42),
        };
        metadata(&ctx, scope.channel_id(), kind);
        let ctx = Arc::new(ctx);
        h.control(json!({"snapshot": h.snapshot_path(&scope), "usage": [["SELF", 64, 100]]}));
        // A reply tag cannot tell us which policy admitted this Conversation.
        let event = mention(
            &Keys::generate(),
            scope.channel_id(),
            Some(&"a".repeat(64)),
            "reply",
        );
        let agent = h.agent(0, &[]).await;
        let mut agent = run_turn(agent, &ctx, batch(&scope, event)).await;
        assert_eq!(h.seen()[0].is_some(), eligible, "{policy:?}, {kind:?}");
        assert_eq!(
            h.snapshot(&scope).is_some(),
            eligible,
            "{policy:?}, {kind:?}"
        );
        assert!(
            agent.state.sessions.contains_key(&scope),
            "execution scope is unchanged"
        );
        agent.acp.shutdown().await;
        server.server.abort();
    }
}

#[tokio::test]
async fn uncertain_conversation_cannot_outlive_metadata_recovery_and_failed_thread_status() {
    let h = Harness::new();
    let (ctx, server) = fixture(&h, SessionPolicy::Thread).await;
    let scope = SessionScope::Conversation {
        channel_id: Uuid::from_u128(42),
    };
    metadata(&ctx, scope.channel_id(), None);
    let ctx = Arc::new(ctx);
    h.control(json!({"snapshot": h.snapshot_path(&scope), "usage": [["SELF", 99, 100]]}));
    let agent = h.agent(0, &[]).await;
    let event = mention(
        &Keys::generate(),
        scope.channel_id(),
        None,
        "uncertain admission",
    );
    let agent = run_turn(agent, &ctx, batch(&scope, event)).await;
    // Recover metadata; the previously admitted execution scope stays retained.
    metadata(&ctx, scope.channel_id(), Some("stream"));
    let event = mention(
        &Keys::generate(),
        scope.channel_id(),
        None,
        "retained scope",
    );
    let agent = run_turn(agent, &ctx, batch(&scope, event)).await;
    assert!(agent.state.sessions.contains_key(&scope));
    assert!(
        h.snapshot(&scope).is_none(),
        "uncertain Conversation must never be a fallback"
    );

    // A different root still receives its own Thread status under Thread policy.
    let (agent, good_thread, _) = first_turn(&h, &ctx, agent, "verified thread").await;
    assert_eq!(percent(&h.snapshot(&good_thread).unwrap()), Some(64));
    let root = mention(
        &Keys::generate(),
        scope.channel_id(),
        None,
        "failed status thread",
    );
    let thread = thread_of(scope.channel_id(), &root);
    // Force this optional scope's write to fail while provider execution succeeds.
    std::fs::create_dir(h.snapshot_path(&thread)).unwrap();
    h.control(json!({"usage": [["new-1", 98, 100], ["SELF", 77, 100]]}));
    let mut agent = run_turn(agent, &ctx, batch(&thread, root)).await;
    assert!(agent.state.sessions.contains_key(&thread));
    assert!(!agent.state.status.contains_key(&thread));
    assert!(
        h.snapshot(&scope).is_none(),
        "no Conversation candidate for the CLI to borrow"
    );
    assert_eq!(percent(&h.snapshot(&good_thread).unwrap()), Some(64));
    agent.acp.shutdown().await;
    server.server.abort();
}

#[tokio::test]
async fn retained_conversation_revokes_generation_before_rebind_or_late_updates() {
    for (kind, clear_acp, clear_worker) in [
        (None, false, false),
        (Some("stream"), true, false),
        (Some("stream"), false, true),
    ] {
        let h = Harness::new();
        let (ctx, server) = fixture(&h, SessionPolicy::Channel).await;
        let scope = SessionScope::Conversation {
            channel_id: Uuid::from_u128(42),
        };
        metadata(&ctx, scope.channel_id(), kind);
        let mut ctx = Arc::new(ctx);
        h.control(json!({"snapshot": h.snapshot_path(&scope), "usage": [["SELF", 64, 100]]}));
        let event = mention(&Keys::generate(), scope.channel_id(), None, "first");
        let agent = h.agent(0, &[]).await;
        let mut agent = run_turn(agent, &ctx, batch(&scope, event)).await;
        let original = h.snapshot(&scope).unwrap();
        let old_bytes = std::fs::read(h.snapshot_path(&scope)).unwrap();
        let old_binding = agent.state.status[&scope].clone();
        let sid = agent.state.sessions[&scope].clone();
        if clear_acp {
            agent.acp.clear_session_status(&sid);
        }
        if clear_worker {
            agent.state.status.remove(&scope);
        }
        // Simulate an already retained Conversation encountering Thread policy.
        // The next prompt must revoke it even if ACP still has a routing entry.
        Arc::get_mut(&mut ctx).unwrap().session_policy = SessionPolicy::Thread;
        h.control(json!({"snapshot": h.snapshot_path(&scope), "usage": [["SELF", 99, 100]]}));
        let event = mention(&Keys::generate(), scope.channel_id(), None, "retained");
        let agent = run_turn(agent, &ctx, batch(&scope, event)).await;
        assert_eq!(
            agent.state.sessions[&scope], sid,
            "provider session must stay retained"
        );
        assert!(
            h.seen().last().unwrap().is_none(),
            "revoke before provider prompt"
        );
        assert!(!agent.state.status.contains_key(&scope));
        assert!(!agent.acp.has_session_status(&sid));
        assert!(
            !lock_is_held(&h.generation_lock(&original)),
            "release the generation guard"
        );
        assert!(h
            .status
            .bind_existing(&session_visibility_scope(&scope), &sid)
            .is_none());
        // A surviving file is untrusted after revocation; late callbacks cannot rewrite it.
        std::fs::write(h.snapshot_path(&scope), &old_bytes).unwrap();
        old_binding.record_occupancy(100, 100);
        assert_eq!(std::fs::read(h.snapshot_path(&scope)).unwrap(), old_bytes);
        assert!(!old_binding.is_live());
        // Even verified-DM recovery cannot resurrect a retired transition.
        metadata(&ctx, scope.channel_id(), Some("dm"));
        h.control(json!({"usage": [["SELF", 88, 100]]}));
        let event = mention(&Keys::generate(), scope.channel_id(), None, "recovered");
        let mut agent = run_turn(agent, &ctx, batch(&scope, event)).await;
        assert_eq!(agent.state.sessions[&scope], sid);
        assert!(!agent.acp.has_session_status(&sid));
        assert!(!lock_is_held(&h.generation_lock(&original)));
        assert_eq!(std::fs::read(h.snapshot_path(&scope)).unwrap(), old_bytes);
        agent.acp.shutdown().await;
        server.server.abort();
    }
}

#[test]
fn configured_policy_reaches_prompt_status_context_without_base_prompt() {
    for policy in [SessionPolicy::Channel, SessionPolicy::Thread] {
        let mut config = crate::build_mcp_servers_tests::test_config();
        config.session_policy = policy;
        config.no_base_prompt = true;
        let rest = RestClient {
            http: reqwest::Client::new(),
            base_url: "http://127.0.0.1:0".into(),
            keys: config.keys.clone(),
            auth_tag_json: None,
        };
        let runtime = crate::runtime::AgentRuntime::prepare(config).unwrap();
        let ctx = runtime
            .prompt_context(
                rest,
                HashMap::new(),
                crate::runtime::SessionMode::Conversation,
            )
            .unwrap();
        assert_eq!(ctx.session_policy, policy);
        assert!(ctx.base_prompt.is_none());
    }
}
