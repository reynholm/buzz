//! Real streamed upload and S3 completion after the HTTP admission fence.
use super::*;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use nostr::{EventBuilder, JsonUtil, Keys, Kind, Tag};
use sha2::{Digest, Sha256};
use tokio::sync::Notify;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires isolated Postgres and MinIO"]
async fn deployment_long_upload_finishes_s3_write_after_http_fence() {
    let mut config = crate::config::Config::for_test();
    config.require_relay_membership = false;
    config.audit_enabled = false;
    config.redis_url = "redis://127.0.0.1:1".into();
    config.read_database_url = None;
    config.deploy_control_token =
        Some(crate::config::DeployControlToken::from_secret("a".repeat(64)).unwrap());
    config.media.s3_endpoint =
        std::env::var("BUZZ_TEST_S3_ENDPOINT").expect("explicit isolated S3");
    config.media.s3_access_key = std::env::var("BUZZ_TEST_S3_ACCESS_KEY").unwrap();
    config.media.s3_secret_key = std::env::var("BUZZ_TEST_S3_SECRET_KEY").unwrap();
    config.media.s3_bucket = std::env::var("BUZZ_TEST_S3_BUCKET").unwrap();
    config.media.s3_addressing_style = buzz_media::S3AddressingStyle::Path;
    config.media.upload_records_enabled = false;
    let pool = sqlx::PgPool::connect(&crate::test_support::database_url())
        .await
        .unwrap();
    let (state, audit) =
        crate::state::tests::build_test_state_with_shutdown(config, pool.clone()).await;
    let host = format!("upload-drain-{}.test", Uuid::new_v4().simple());
    let community = state
        .db
        .ensure_configured_community(&host)
        .await
        .unwrap()
        .id;
    let bytes = format!("Synthetic long upload {}\n", Uuid::new_v4()).into_bytes();
    let hash = hex::encode(Sha256::digest(&bytes));
    let key = Keys::generate();
    let event = EventBuilder::new(Kind::from(24242), "Upload blob")
        .tags([
            Tag::parse(["t", "upload"]).unwrap(),
            Tag::parse(["server", &host]).unwrap(),
            Tag::parse([
                "expiration",
                &(nostr::Timestamp::now().as_secs() + 120).to_string(),
            ])
            .unwrap(),
            Tag::parse(["x", &hash]).unwrap(),
        ])
        .sign_with_keys(&key)
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = crate::router::build_router(state.clone());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    let release = Arc::new(Notify::new());
    let delayed = release.clone();
    let sent = bytes.clone();
    let upload = tokio::spawn(async move {
        let stream = futures_util::stream::once(async move {
            delayed.notified().await;
            Ok::<_, std::io::Error>(sent)
        });
        reqwest::Client::new()
            .put(format!("http://{address}/upload"))
            .header("host", host)
            .header("x-sha-256", hash)
            .header(
                "authorization",
                format!("Nostr {}", URL_SAFE_NO_PAD.encode(event.as_json())),
            )
            .body(reqwest::Body::wrap_stream(stream))
            .send()
            .await
            .unwrap()
    });
    tokio::time::timeout(Duration::from_secs(5), async {
        while state.deploy_drain.snapshot().upload_inflight != 1 {
            assert!(!upload.is_finished(), "upload ended before body admission");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let op = Uuid::new_v4();
    let boot = state.deploy_drain.begin(op).unwrap().boot_id;
    state
        .deploy_drain
        .fence_http(op, boot, "direct-probes-stopped")
        .unwrap();
    tokio::time::sleep(Duration::from_secs(36)).await;
    assert_eq!(state.deploy_drain.snapshot().upload_inflight, 1);
    assert!(!state.deploy_drain.snapshot().safe_to_stop);
    release.notify_one();
    let response = tokio::time::timeout(Duration::from_secs(10), upload)
        .await
        .unwrap()
        .unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    assert!(status.is_success(), "{status}: {body}");
    let descriptor: BlobDescriptor = serde_json::from_str(&body).unwrap();
    let object = descriptor.url.rsplit('/').next().unwrap();
    assert_eq!(state.media_storage.get(object).await.unwrap(), bytes);
    assert_eq!(state.deploy_drain.snapshot().upload_inflight, 0);
    assert!(!state
        .deploy_drain
        .snapshot()
        .blocking_workers
        .iter()
        .any(|name| name.contains("unresolved")));
    state.media_storage.delete(object).await.unwrap();
    state
        .media_storage
        .delete(&buzz_media::MediaStorage::sidecar_key(
            community,
            &descriptor.sha256,
        ))
        .await
        .unwrap();
    server.abort();
    let _ = server.await;
    audit.join().await.unwrap();
    sqlx::query("DELETE FROM community_serving_write_leases WHERE community_id=$1")
        .bind(community.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM audit_log WHERE community_id=$1")
        .bind(community.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM communities WHERE id=$1")
        .bind(community.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
}
