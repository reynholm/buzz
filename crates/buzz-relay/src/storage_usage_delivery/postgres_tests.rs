use super::*;
use crate::deploy_drain::DrainController;
use nostr::nips::nip98::{verify_auth_header, HttpMethod};
use tokio::{io::AsyncWriteExt, net::TcpListener};
use tokio_util::sync::CancellationToken;

fn enable_drain(base: &AppState) -> Arc<AppState> {
    let mut state = base.clone();
    Arc::make_mut(&mut state.config).deploy_control_token =
        Some(crate::config::DeployControlToken::from_secret("a".repeat(64)).unwrap());
    state.deploy_drain = DrainController::new();
    Arc::new(state)
}

#[tokio::test]
#[ignore = "requires Postgres"]
async fn admitted_storage_post_holds_drain_and_retry_releases_leader_for_successor() {
    let pool = sqlx::PgPool::connect(&crate::test_support::database_url())
        .await
        .unwrap();
    let base = crate::state::tests::test_state_with_database_pool(pool).await;
    let community = base
        .db
        .ensure_configured_community("storage-drain.test")
        .await
        .unwrap();
    let snapshot = BucketSnapshot {
        per_community: HashMap::from([(
            *community.id.as_uuid(),
            buzz_media::CommunityStorage {
                bytes: 4096,
                objects: 2,
            },
        )]),
        ..Default::default()
    };
    let mut accounting = base
        .db
        .try_lock_storage_accounting()
        .await
        .unwrap()
        .unwrap();
    accounting
        .save_snapshot(&serde_json::to_value(&snapshot).unwrap(), 1, 100, "test")
        .await
        .unwrap();
    drop(accounting);
    let persisted = base
        .db
        .load_storage_accounting_snapshot()
        .await
        .unwrap()
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = url::Url::parse(&format!(
        "http://{}/storage",
        listener.local_addr().unwrap()
    ))
    .unwrap();
    let mut previous_measured_at = None;
    // The first attempt fails after admission; the successor must resend the
    // same durable snapshot and may then attest completion normally.
    for response in [
        b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .as_slice(),
        b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".as_slice(),
    ] {
        let state = enable_drain(&base);
        let drain = state.deploy_drain.clone();
        let cancel = CancellationToken::new();
        let task = tokio::spawn(run_delivery_worker(
            Arc::clone(&state),
            Some(url.clone()),
            cancel.clone(),
        ));
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let (_, body, authorization) = super::tests::read_http_request(&mut socket).await;
        assert_eq!(
            verify_auth_header(
                &authorization,
                &url,
                HttpMethod::POST,
                nostr::Timestamp::now(),
                Some(&body)
            )
            .unwrap(),
            state.relay_keypair.public_key()
        );
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["communities"][0]["storage_bytes"], 4096);
        if let Some(previous) = previous_measured_at.as_ref() {
            assert_eq!(
                &body["measured_at"], previous,
                "successor must resend the retained snapshot"
            );
        }
        previous_measured_at = Some(body["measured_at"].clone());
        assert!(state
            .db
            .try_lock_session_leader(STORAGE_USAGE_DELIVERY_LOCK_KEY)
            .await
            .unwrap()
            .is_none());
        let op = Uuid::new_v4();
        let boot = drain.begin(op).unwrap().boot_id;
        drain.fence_http(op, boot, "test-edge-retired").unwrap();
        assert_eq!(drain.snapshot().background_inflight, 1);
        assert!(!drain.snapshot().safe_to_stop);
        assert!(!task.is_finished());
        socket.write_all(response).await.unwrap();
        let mut runtime = crate::deploy_runtime::RuntimeDependencies::default();
        runtime.own(task, cancel);
        tokio::time::timeout(Duration::from_secs(3), runtime.shutdown())
            .await
            .unwrap()
            .unwrap();
        assert!(drain.snapshot().safe_to_stop, "{:?}", drain.snapshot());
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(successor) = state
                .db
                .try_lock_session_leader(STORAGE_USAGE_DELIVERY_LOCK_KEY)
                .await
                .unwrap()
            {
                drop(successor);
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "joined worker retained its leader session"
            );
            tokio::task::yield_now().await;
        }
        assert_eq!(
            state
                .db
                .load_storage_accounting_snapshot()
                .await
                .unwrap()
                .unwrap()
                .completed_at,
            persisted.completed_at
        );
    }
}
