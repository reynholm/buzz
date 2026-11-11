// These witnesses drive real WebSocket AUTH dispatch and hold the production
// membership write behind a PostgreSQL row lock after its session permit.

#[tokio::test]
#[ignore = "requires Postgres and Redis — runs in postgres-ci nextest lane"]
async fn root_auth_first_default_join_remains_owned_after_socket_closes() {
    first_default_join_remains_owned_after_socket_closes().await;
}

async fn first_default_join_remains_owned_after_socket_closes() {
    use futures_util::SinkExt;
    use std::{net::SocketAddr, sync::Arc, time::Duration};
    use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest};

    let mut state = (*crate::api::bridge::postgres_tests::bridge_handler_test_state()
        .await
        .expect("isolated PostgreSQL and Redis required"))
    .clone();
    let config = Arc::make_mut(&mut state.config);
    config.require_relay_membership = false;
    config.deploy_control_token =
        Some(crate::config::DeployControlToken::from_secret("a".repeat(64)).unwrap());
    config.drain_jitter_ms = 0;
    // This router fixture starts no autonomous runtime workers.
    state.deploy_drain.seal_worker_inventory(&[]).unwrap();
    let state = Arc::new(state);
    let community = seeded_community(&state).await;
    let host = format!("admission-{}.example", community.as_uuid().simple());
    let tenant = buzz_core::TenantContext::resolved(community, &host);
    let (owner, person) = (Keys::generate(), Keys::generate());
    let channel = default_channels(&state, community, &owner, &["drain-default"]).await[0];
    // The first-member write takes this row lock after
    // acquire_effect(). Observe the real wait, not merely a pre-permit hook.
    let mut blocker = state.db.pool().begin().await.unwrap();
    let blocker_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *blocker)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM channels WHERE community_id = $1 AND id = $2 FOR UPDATE")
        .bind(community.as_uuid())
        .bind(channel)
        .execute(&mut *blocker)
        .await
        .unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server_state = state.clone();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            crate::router::build_router(server_state)
                .into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    let mut request = format!("ws://{address}/").into_client_request().unwrap();
    request.headers_mut().insert("host", host.parse().unwrap());
    let (mut client, _) = connect_async(request).await.expect("root WebSocket");
    let challenge = next_auth_deploy_frame(&mut client).await;
    assert_eq!(challenge[0], "AUTH");
    let event = EventBuilder::auth(
        challenge[1].as_str().unwrap(),
        crate::api::bridge::nip42_expected_relay_url(&state.config.relay_url, &tenant)
            .parse::<RelayUrl>()
            .unwrap(),
    )
    .sign_with_keys(&person)
    .unwrap();
    client
        .send(tokio_tungstenite::tungstenite::Message::Text(
            serde_json::json!(["AUTH", event]).to_string().into(),
        ))
        .await
        .unwrap();
    let reply = next_auth_deploy_frame(&mut client).await;
    assert_eq!(reply[0], "OK", "{reply}");
    assert_eq!(reply[1], event.id.to_hex());
    assert_eq!(reply[2], true, "{reply}");

    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity \
                 WHERE datname = current_database() AND $1 = ANY(pg_blocking_pids(pid)))",
            )
            .bind(blocker_pid)
            .fetch_one(state.db.pool())
            .await
            .unwrap();
            if waiting {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("admitted join must reach its blocked database write");
    assert_eq!(
        state
            .db
            .get_member_role(community, channel, person.public_key().as_bytes())
            .await
            .unwrap(),
        None,
        "the blocked join has not committed"
    );

    // Exercise the actual socket cleanup before draining the process. It
    // must not need to await the detached, already-permitted database effect.
    client.close(None).await.unwrap();
    drop(client);
    let drain = state.deploy_drain.clone();
    tokio::time::timeout(Duration::from_secs(5), async {
        while drain.snapshot().ws_open != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("socket cleanup must finish while its admitted join is blocked");
    let operation = uuid::Uuid::new_v4();
    let boot = drain.begin(operation).unwrap().boot_id;
    drain
        .fence_http(operation, boot, "test-route-retired")
        .unwrap();
    let held = drain.snapshot();
    assert_eq!(held.ws_open, 0);
    assert_eq!(
        held.background_inflight, 1,
        "AUTH must own the complete admitted join: {held:?}"
    );
    assert!(
        !held.safe_to_stop,
        "a blocked accepted join must prevent stop"
    );

    blocker.commit().await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while !drain.snapshot().safe_to_stop {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("join and its announcements must complete without an unattributed-work fault");
    assert_eq!(
        state
            .db
            .get_member_role(community, channel, person.public_key().as_bytes())
            .await
            .unwrap()
            .as_deref(),
        Some("member"),
        "the effect permitted before cancellation is retained"
    );
    let announcements: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE community_id = $1 AND channel_id = $2 \
         AND kind = 40099 AND content::jsonb->>'type' = 'member_joined' \
         AND content::jsonb->>'target' = $3",
    )
    .bind(community.as_uuid())
    .bind(channel)
    .bind(person.public_key().to_hex())
    .fetch_one(state.db.pool())
    .await
    .unwrap();
    assert_eq!(
        announcements, 1,
        "the accepted join finishes its announcement"
    );
    server.abort();
    let _ = server.await;
}

async fn next_auth_deploy_frame(
    client: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> serde_json::Value {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::Message;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            match client
                .next()
                .await
                .expect("root socket ended")
                .expect("root frame")
            {
                Message::Text(text) => return serde_json::from_str(&text).expect("relay JSON"),
                Message::Ping(_) | Message::Pong(_) => {}
                other => panic!("expected AUTH decision, got {other:?}"),
            }
        }
    })
    .await
    .expect("AUTH frame deadline")
}
