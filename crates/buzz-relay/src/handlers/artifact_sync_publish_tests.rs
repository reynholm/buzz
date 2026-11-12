//! Production publisher recovery against a controlled Redis command boundary.
use super::*;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};

// A PUBLISH-only failure leaves unrelated Redis commands available. The test
// chooses when each operational PUBLISH is acknowledged; no retry logic is fake.
async fn command_peer() -> (
    String,
    mpsc::Receiver<oneshot::Sender<()>>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("redis://{}", listener.local_addr().unwrap());
    let (signals, received) = mpsc::channel(8);
    let task = tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            let signals = signals.clone();
            connections.spawn(async move {
                let (read, mut write) = socket.into_split();
                let mut read = BufReader::new(read);
                loop {
                    let mut line = String::new();
                    if read.read_line(&mut line).await.unwrap() == 0 {
                        return;
                    }
                    let count: usize = line.trim().strip_prefix('*').unwrap().parse().unwrap();
                    let mut args = Vec::new();
                    for _ in 0..count {
                        line.clear();
                        read.read_line(&mut line).await.unwrap();
                        let len: usize = line.trim().strip_prefix('$').unwrap().parse().unwrap();
                        let mut bytes = vec![0; len + 2];
                        read.read_exact(&mut bytes).await.unwrap();
                        args.push(String::from_utf8(bytes[..len].to_vec()).unwrap());
                    }
                    let response = if args[0].eq_ignore_ascii_case("publish") {
                        if args[1] == "buzz:control:artifact-sync-loss" {
                            assert_eq!(args[2], "resync");
                            let (ack, wait) = oneshot::channel();
                            signals.send(ack).await.unwrap();
                            wait.await.unwrap();
                            ":1\r\n"
                        } else {
                            "-NOPERM event publication disabled\r\n"
                        }
                        .to_owned()
                    } else if args[0].eq_ignore_ascii_case("ping") {
                        // Pool recycle uses PING <counter>; Redis echoes it.
                        if let Some(value) = args.get(1) {
                            format!("${}\r\n{}\r\n", value.len(), value)
                        } else {
                            "+PONG\r\n".to_owned()
                        }
                    } else {
                        "+OK\r\n".to_owned()
                    };
                    if write.write_all(response.as_bytes()).await.is_err() {
                        return;
                    }
                }
            });
        }
    });
    (url, received, task)
}

#[tokio::test]
async fn loss_during_ack_and_after_retirement_keeps_one_owned_rate_limited_worker() {
    use crate::deploy_drain::WorkKind;
    let (url, mut signals, peer) = command_peer().await;
    let base = crate::state::tests::test_state_with_unreachable_database_url(
        "postgres://test:test@127.0.0.1:1/test",
    )
    .await;
    let mut state = (*base).clone();
    // This fixture has no autonomous services; isolate the accepted-work census.
    state.deploy_drain = crate::deploy_drain::DrainController::new();
    Arc::make_mut(&mut state.config).deploy_control_token =
        Some(crate::config::DeployControlToken::from_secret("a".repeat(64)).unwrap());
    state.pubsub = Arc::new(
        buzz_pubsub::PubSubManager::new(
            &url,
            deadpool_redis::Config::from_url(&url)
                .create_pool(Some(deadpool_redis::Runtime::Tokio1))
                .unwrap(),
        )
        .await
        .unwrap(),
    );
    // Independent command transport lets another completed event publication
    // arrive while the first transport's control acknowledgement is in flight.
    // Both AppState clones still share the real recovery and drain state.
    let mut other = state.clone();
    other.pubsub = Arc::new(
        buzz_pubsub::PubSubManager::new(
            &url,
            deadpool_redis::Config::from_url(&url)
                .create_pool(Some(deadpool_redis::Runtime::Tokio1))
                .unwrap(),
        )
        .await
        .unwrap(),
    );
    let state = Arc::new(state);
    let other = Arc::new(other);
    let drain = state.deploy_drain.clone();
    let parent = drain.try_admit_root(WorkKind::Http).unwrap();
    let tenant = buzz_core::TenantContext::resolved(
        buzz_core::CommunityId::from_uuid(Uuid::new_v4()),
        "example.test",
    );
    let event = super::tests::stored(45010, Uuid::new_v4());
    for _ in 0..2 {
        assert!(
            parent
                .scope(publish_reliably(
                    &state,
                    &tenant,
                    EventTopic::Global,
                    &event
                ))
                .await
        );
    }
    let first = tokio::time::timeout(Duration::from_secs(7), signals.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(drain.snapshot().background_inflight, 1);
    // This loss was not covered when the first control publication began.
    assert!(
        parent
            .scope(publish_reliably(
                &other,
                &tenant,
                EventTopic::Global,
                &event
            ))
            .await
    );
    first.send(()).unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(200), signals.recv())
            .await
            .is_err()
    );
    assert_eq!(drain.snapshot().background_inflight, 1);
    let second = tokio::time::timeout(Duration::from_secs(7), signals.recv())
        .await
        .unwrap()
        .unwrap();
    second.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while drain.snapshot().background_inflight != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    // A new failure immediately after retirement must retain the previous
    // attempt's deadline, even though control publication keeps succeeding.
    assert!(
        parent
            .scope(publish_reliably(
                &state,
                &tenant,
                EventTopic::Global,
                &event
            ))
            .await
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(200), signals.recv())
            .await
            .is_err()
    );
    let op = Uuid::new_v4();
    let boot = drain.begin(op).unwrap().boot_id;
    drain.fence_http(op, boot, "test-edge-retired").unwrap();
    parent.complete();
    assert_eq!(drain.snapshot().background_inflight, 1);
    assert!(!drain.snapshot().safe_to_stop);
    let third = tokio::time::timeout(Duration::from_secs(7), signals.recv())
        .await
        .unwrap()
        .unwrap();
    third.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while !drain.snapshot().safe_to_stop {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(drain.snapshot().background_inflight, 0);
    peer.abort();
}
