//! Real authenticated voice reconnect between two relay instances.
use crate::{
    config::DeployControlToken,
    router::{build_health_router, build_router},
    state::AppState,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use buzz_core::tenant::TenantContext;
use futures_util::{SinkExt, StreamExt};
use nostr::{EventBuilder, Keys, Kind, Tag};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
    MaybeTlsStream, WebSocketStream,
};
use tower::ServiceExt;
use uuid::Uuid;

type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn server(state: Arc<AppState>) -> (SocketAddr, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            build_router(state).into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    (address, task)
}

async fn next(client: &mut Client) -> Message {
    tokio::time::timeout(Duration::from_secs(5), client.next())
        .await
        .expect("audio frame deadline")
        .unwrap()
        .unwrap()
}

async fn join(
    address: SocketAddr,
    state: &AppState,
    tenant: &TenantContext,
    channel: Uuid,
    parent: Uuid,
    key: &Keys,
) -> Client {
    let mut request = format!("ws://{address}/huddle/{channel}/audio")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("host", tenant.host().parse().unwrap());
    let (mut client, _) = connect_async(request).await.unwrap();
    let challenge: serde_json::Value =
        serde_json::from_str(next(&mut client).await.to_text().unwrap()).unwrap();
    let event = EventBuilder::new(Kind::Authentication, "")
        .tag(
            Tag::parse([
                "relay",
                &crate::api::bridge::nip42_expected_relay_url(&state.config.relay_url, tenant),
            ])
            .unwrap(),
        )
        .tag(Tag::parse(["challenge", challenge["challenge"].as_str().unwrap()]).unwrap())
        .sign_with_keys(key)
        .unwrap();
    client.send(Message::Text(serde_json::json!({"type":"auth", "event":event, "parent_channel_id":parent,"protocol_version":1}).to_string().into())).await.unwrap();
    loop {
        let frame = next(&mut client).await;
        if let Message::Text(text) = frame {
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_ne!(value["type"], "error", "audio join rejected: {value}");
            if value["type"] == "joined" {
                return client;
            }
        }
    }
}

async fn media(sender: &mut Client, receiver: &mut Client) {
    let payload = vec![0xf8, 0xff, 0xfe];
    sender
        .send(Message::Binary(payload.clone().into()))
        .await
        .unwrap();
    loop {
        if let Message::Binary(frame) = next(receiver).await {
            assert_eq!(&frame[1..], payload.as_slice());
            break;
        }
    }
}

async fn restart_close(client: &mut Client) {
    loop {
        if let Message::Close(close) = next(client).await {
            assert_eq!(u16::from(close.unwrap().code), 1012);
            let _ = client.flush().await;
            break;
        }
    }
}

#[tokio::test]
#[ignore = "requires Postgres and Redis"]
async fn deployment_active_audio_rejoins_the_same_huddle_on_the_other_color() {
    let blue = super::tests::audio_test_state_real_db_with(|config| {
        config.deploy_control_token =
            Some(DeployControlToken::from_secret("a".repeat(64)).unwrap());
        config.redis_url = std::env::var("REDIS_URL").unwrap();
        config.drain_jitter_ms = 0;
    })
    .await
    .expect("isolated Postgres required");
    let mut green = super::tests::audio_test_state_real_db_with(|config| {
        config.deploy_control_token =
            Some(DeployControlToken::from_secret("b".repeat(64)).unwrap());
        config.redis_url = std::env::var("REDIS_URL").unwrap();
        config.drain_jitter_ms = 0;
    })
    .await
    .unwrap();
    Arc::make_mut(&mut green).relay_keypair = blue.relay_keypair.clone();
    let pool = blue.db.pool();
    let (tenant, parent, alice) = super::tests::seed_audio_fixture(pool).await;
    let bob = Keys::generate();
    for key in [&alice, &bob] {
        blue.db
            .ensure_user(tenant.community(), &key.public_key().to_bytes())
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO channel_members (community_id, channel_id, pubkey, role, invited_by) VALUES ($1,$2,$3,'member',$3)")
        .bind(tenant.community().as_uuid()).bind(parent).bind(bob.public_key().to_bytes().to_vec()).execute(pool).await.unwrap();
    let child = blue
        .db
        .create_channel(
            tenant.community(),
            "deployment-voice",
            buzz_core::channel::ChannelType::Stream,
            buzz_core::channel::ChannelVisibility::Private,
            None,
            &alice.public_key().to_bytes(),
            Some(3600),
        )
        .await
        .unwrap();
    let started = EventBuilder::new(
        Kind::Custom(48100),
        serde_json::json!({"ephemeral_channel_id":child.id}).to_string(),
    )
    .tag(Tag::parse(["h", &parent.to_string()]).unwrap())
    .sign_with_keys(&alice)
    .unwrap();
    sqlx::query("INSERT INTO events (community_id,id,pubkey,created_at,kind,tags,content,sig,channel_id) VALUES ($1,$2,$3,NOW(),48100,$4,$5,$6,$7)")
        .bind(tenant.community().as_uuid()).bind(started.id.to_bytes().to_vec()).bind(alice.public_key().to_bytes().to_vec())
        .bind(serde_json::to_value(&started.tags).unwrap()).bind(&started.content).bind(started.sig.serialize().to_vec()).bind(parent).execute(pool).await.unwrap();
    let (blue_address, blue_server) = server(blue.clone()).await;
    let (green_address, green_server) = server(green.clone()).await;
    let mut a = join(blue_address, &blue, &tenant, child.id, parent, &alice).await;
    let mut b = join(blue_address, &blue, &tenant, child.id, parent, &bob).await;
    media(&mut a, &mut b).await;
    media(&mut b, &mut a).await;
    let response = build_health_router(blue.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/_deploy/drain")
                .header("authorization", format!("Bearer {}", "a".repeat(64)))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"operation_id":Uuid::new_v4()}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    tokio::join!(restart_close(&mut a), restart_close(&mut b));
    drop((a, b));
    tokio::time::timeout(Duration::from_secs(5), async {
        while blue.deploy_drain.snapshot().ws_open != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let archived: bool = sqlx::query_scalar(
        "SELECT archived_at IS NOT NULL FROM channels WHERE community_id=$1 AND id=$2",
    )
    .bind(tenant.community().as_uuid())
    .bind(child.id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(
        !archived,
        "deployment restart archived the huddle before clients could reconnect"
    );
    let mut a = join(green_address, &green, &tenant, child.id, parent, &alice).await;
    let mut b = join(green_address, &green, &tenant, child.id, parent, &bob).await;
    media(&mut a, &mut b).await;
    media(&mut b, &mut a).await;
    a.close(None).await.unwrap();
    b.close(None).await.unwrap();
    drop((a, b));
    tokio::time::timeout(Duration::from_secs(5), async {
        while green.deploy_drain.snapshot().ws_open != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let archived: bool = sqlx::query_scalar(
        "SELECT archived_at IS NOT NULL FROM channels WHERE community_id=$1 AND id=$2",
    )
    .bind(tenant.community().as_uuid())
    .bind(child.id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(
        archived,
        "normal last participant leave must still end the huddle"
    );
    for state in [&blue, &green] {
        assert!(
            !state
                .deploy_drain
                .snapshot()
                .blocking_workers
                .iter()
                .any(|name| name.starts_with("unresolved-")),
            "voice teardown left unknown effects"
        );
    }
    blue_server.abort();
    green_server.abort();
    let _ = tokio::join!(blue_server, green_server);
}
