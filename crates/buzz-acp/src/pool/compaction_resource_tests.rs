use super::*;
use std::collections::HashSet;

#[tokio::test]
async fn resource_failure_during_compaction_retires_worker_without_replay() {
    for failure in ["oom", "guard_failed"] {
        let (fixture, ctx, mut pool, human, server) = setup().await;
        let mut agent = pool.try_claim(Some(&scope())).unwrap();
        agent.acp.shutdown().await;
        let (acp, group) = crate::pool::provider_failure_tests::resource_adapter(
            fixture.dir.path(),
            failure,
            "session/prompt",
        )
        .await;
        agent.acp = acp;
        agent.acp.ensure_compaction_route("old-1");
        agent.acp.advertise_compaction("old-1");
        pool.return_agent(agent);
        let request = enqueue(&ctx, &pool, &human, &scope()).await;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        crate::control_tasks::dispatch(&mut pool, &ctx, &tx);
        let result = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(result.outcome.is_err());
        assert!(
            result.transport_failed,
            "resource failure during compaction returned a dead worker as healthy: {failure}"
        );
        let config = crate::build_mcp_servers_tests::test_config();
        let mut circuits = vec![crate::SlotCircuit {
            crash_times: vec![],
            open_until: None,
            respawn_in_flight: false,
        }];
        let (respawn_tx, mut respawn_rx) = tokio::sync::mpsc::channel(8);
        let mut tasks = tokio::task::JoinSet::new();
        crate::control_tasks::finish(
            &mut pool,
            &ctx,
            result,
            &HashSet::new(),
            &config,
            &mut circuits,
            &respawn_tx,
            &mut tasks,
            None,
            &mut tokio::time::Instant::now(),
        );
        assert_eq!(pool.live_count(), 0, "resource-failed worker was reused");
        assert!(!ctx.session_controls.compact_pending(&scope()));
        assert!(matches!(
            fixture
                .record(&scope())
                .control_receipts
                .iter()
                .find(|receipt| receipt.command_id == request.origin.command_event_id)
                .unwrap()
                .state,
            crate::session_store::ControlReceiptState::Finished(Err(_))
        ));
        crate::control_tasks::dispatch(&mut pool, &ctx, &tx);
        assert!(rx.try_recv().is_err(), "failed compaction was replayed");
        let methods =
            std::fs::read_to_string(fixture.dir.path().join("resource-methods.log")).unwrap();
        assert_eq!(
            methods
                .lines()
                .filter(|line| *line == "session/prompt")
                .count(),
            1
        );
        let returned = tokio::time::timeout(Duration::from_secs(5), respawn_rx.recv())
            .await
            .unwrap()
            .unwrap();
        let retirement = returned
            .retirement
            .expect("checked retirement was bypassed");
        assert_eq!(retirement.report, crate::acp::TerminationReport::Unverified);
        assert!(
            returned.result.is_err(),
            "replacement started before verified cleanup"
        );
        let mut retained = retirement
            .retained
            .expect("original worker ownership was discarded");
        std::fs::write(group.join("cgroup.events"), "populated 0\n").unwrap();
        retained.acp.shutdown().await;
        tasks.shutdown().await;
        pool.join_set.shutdown().await;
        server.abort();
    }
}
