//! Real HTTP and ACP boundaries: a slow/recovering relay must not erase history.
use super::*;

#[tokio::test]
async fn restart_recent_keeps_history_across_slow_reads_partial_failure_and_recovery() {
    for mode in ["history-slow", "history-scan-failed", "history-unavailable"] {
        let fixture = Fixture::new();
        let human = Keys::generate();
        let agent = Keys::generate();
        let channel = Uuid::new_v4();
        let root = event(&human, channel, None, "A conversation", None, 1);
        let root_id = root.id.to_hex();
        let assignment = event(
            &human,
            channel,
            Some(&root_id),
            "Implement the requested task",
            Some(&agent),
            2,
        );
        let progress = event(
            &human,
            channel,
            Some(&root_id),
            "Implementation finished; validation remains",
            None,
            3,
        );
        let (mut ctx, relay) = setup(vec![root, assignment, progress], channel).await;
        ctx.agent_keys = agent;
        *relay.mode.lock().unwrap() = mode.into();
        let scope = SessionScope::Thread {
            channel_id: channel,
            root_event_id: root_id.clone(),
        };
        admitted(&fixture, &mut ctx, &human, &scope, "!restart").await;
        ctx.session_controls.plan_retirement(&scope, []);
        let ctx = Arc::new(ctx);
        let prompts = fixture.dir.path().join("prompts.ndjson");
        let mut result = dispatch(
            &fixture,
            ctx.clone(),
            &[
                ("PROMPTS", prompts.to_str().unwrap()),
                ("STATUS_TEXT", "Validation is next."),
            ],
        )
        .await;
        let posts = relay.posts.lock().unwrap().clone();
        assert_eq!(posts.len(), 1);
        posts[0].verify().unwrap();
        assert_eq!(
            crate::queue::parse_thread_tags(&posts[0])
                .root_event_id
                .as_deref(),
            Some(root_id.as_str())
        );
        if mode == "history-unavailable" {
            assert!(
                !prompts.exists(),
                "an unavailable history must never become an empty model status turn"
            );
            assert!(posts[0].content.contains("history could not be loaded"));
            assert_eq!(
                fixture.record(&scope).restart_status.unwrap().phase,
                RestartStatusPhase::Failed
            );
        } else {
            let prompt = std::fs::read_to_string(&prompts).unwrap();
            assert!(
                prompt.contains("Implementation finished; validation remains"),
                "retain the latest message even when assignment discovery fails"
            );
            assert_eq!(posts[0].content, "Validation is next.");
            assert_eq!(
                fixture.record(&scope).restart_status.unwrap().phase,
                RestartStatusPhase::Completed
            );
        }
        result.agent.acp.shutdown().await;
        // A new authorized attempt after recovery must acquire history normally;
        // failed acquisition neither consumes future work nor poisons the session.
        if mode == "history-unavailable" {
            *relay.mode.lock().unwrap() = String::new();
            let mut ctx = Arc::try_unwrap(ctx).ok().unwrap();
            admitted(&fixture, &mut ctx, &human, &scope, "!rotate").await;
            ctx.session_controls.plan_retirement(&scope, []);
            let mut recovered = dispatch(
                &fixture,
                Arc::new(ctx),
                &[
                    ("PROMPTS", prompts.to_str().unwrap()),
                    ("STATUS_TEXT", "History restored; validation is next."),
                ],
            )
            .await;
            assert!(std::fs::read_to_string(&prompts)
                .unwrap()
                .contains("Implementation finished; validation remains"));
            assert_eq!(relay.posts.lock().unwrap().len(), 2);
            assert_eq!(
                fixture.record(&scope).restart_status.unwrap().phase,
                RestartStatusPhase::Completed
            );
            recovered.agent.acp.shutdown().await;
        }
        let queries = relay.queries.lock().unwrap();
        assert!(
            queries
                .iter()
                .filter(|filter| !filter["depth_limit"].is_null())
                .all(|filter| filter["limit"] == 64),
            "assignment discovery must not download 500 bodies per page for recent history"
        );
        let recent_reads = queries
            .iter()
            .filter(|filter| {
                filter["#e"].is_array()
                    && filter["depth_limit"].is_null()
                    && filter["authors"].is_null()
            })
            .count();
        assert_eq!(
            recent_reads,
            if mode == "history-unavailable" { 3 } else { 1 },
            "reuse the acquired newest page instead of fetching recent history again"
        );
    }
}
