//! First successful automatic reply carries the live generation's display tags.
use super::*;
use buzz_core::session_control::ControlAction;
use buzz_core::session_status::{
    canonical_identity, parse_snapshot, scope_file_name, ContextOccupancy, SessionState,
    StatusReason, SESSION_INFO_TAG, SESSION_TAG,
};

#[tokio::test]
async fn first_signed_status_carries_selected_history_actor_and_current_or_unknown_occupancy() {
    for (command, depth, read) in [
        ("!restart none", "none", "1"),
        ("!restart", "recent", "2"),
        ("!restart all", "all", "2"),
    ] {
        for usage in [Some("[400,1000]"), None] {
            let fixture = Fixture::new();
            let human = Keys::generate();
            let agent = Keys::generate();
            let channel = Uuid::new_v4();
            let root = event(&human, channel, None, "Inspect the task", Some(&agent), 1);
            let root_id = root.id.to_hex();
            let progress = event(
                &human,
                channel,
                Some(&root_id),
                "Implementation done; check next",
                None,
                2,
            );
            let (mut ctx, relay) = setup(vec![root, progress], channel).await;
            ctx.agent_keys = agent;
            let scope = SessionScope::Thread {
                channel_id: channel,
                root_event_id: root_id,
            };
            let status_scope = crate::pool::session_visibility::status_scope(&scope).unwrap();
            let runtime = crate::session_status::SessionStatusRuntime::open(
                &fixture.dir.path().join("visibility"),
                canonical_identity("wss://relay.example", &ctx.agent_keys.public_key().to_hex())
                    .unwrap(),
            )
            .unwrap();
            // A previous session's known capacity must never leak into this reply.
            let old = runtime
                .begin(
                    status_scope.clone(),
                    "old-session",
                    SessionState::New,
                    StatusReason::FirstInThread,
                )
                .unwrap();
            old.record_occupancy(95, 100);
            ctx.session_status = Some(runtime.clone());
            let admitted = admitted(&fixture, &mut ctx, &human, &scope, command).await;
            ctx.session_controls.plan_retirement(&scope, []);
            let mut env = vec![("STATUS_TEXT", "The task is understood; checks are next.")];
            if let Some(usage) = usage {
                env.push(("STATUS_USAGE", usage));
            }
            let mut result = dispatch(&fixture, Arc::new(ctx), &env).await;
            let posts = relay.posts.lock().unwrap().clone();
            assert_eq!(
                posts.len(),
                1,
                "the first reply must already contain metadata"
            );
            posts[0].verify().unwrap();
            let tags: Vec<Vec<String>> = posts[0]
                .tags
                .iter()
                .map(|tag| tag.as_slice().to_vec())
                .collect();
            let primary = tags.iter().find(|tag| tag[0] == SESSION_TAG).unwrap();
            assert_eq!(
                primary,
                &vec![
                    SESSION_TAG.to_owned(),
                    "restarted".to_owned(),
                    format!("user_restart:{depth}"),
                    if usage.is_some() { "40" } else { "" }.to_owned()
                ]
            );
            let info = tags.iter().find(|tag| tag[0] == SESSION_INFO_TAG).unwrap();
            assert_eq!(
                &info[3..],
                &[
                    "thread",
                    read,
                    "2",
                    if depth == "all" {
                        "exact"
                    } else {
                        "lower_bound"
                    }
                ]
            );
            let envelope = parse_snapshot(
                &std::fs::read(runtime.dir().join(scope_file_name(&status_scope).unwrap()))
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(
                envelope.status.occupancy,
                usage.map(|_| ContextOccupancy::new(400, 1000).unwrap())
            );
            assert_eq!(
                envelope.status.history.unwrap().read,
                Some(read.parse().unwrap())
            );
            assert_eq!(envelope.origin.as_ref(), Some(&admitted.origin));
            for expected in envelope.event_tags().unwrap() {
                assert!(tags.contains(&expected));
            }
            let action = ControlAction::from_tags(&tags).unwrap();
            assert_eq!(action.transition_id, envelope.status.transition_id);
            assert_eq!(action.origin, admitted.origin);
            assert!(old.event_tags().is_none());
            assert_eq!(
                fixture
                    .record(&scope)
                    .restart_status
                    .unwrap()
                    .prepared
                    .unwrap()
                    .id,
                posts[0].id
            );
            result.agent.acp.shutdown().await;
        }
    }
}
