//! Real HTTP coverage of the native FileCard fetch path before the save dialog.
use super::{fetch_blob_bytes, fetch_blob_bytes_with_cap, MAX_DOWNLOAD_BYTES};
use crate::app_state::{build_app_state, AppState};
use axum::{
    body::{Body, Bytes},
    http::Response,
    routing::get,
    Router,
};
use tauri::Manager;

const MIB: usize = 1024 * 1024;

async fn blob_server(size: usize, declared_length: bool) -> (String, tokio::task::JoinHandle<()>) {
    let app = Router::new().route(
        "/media/blob.zip",
        get(move || async move {
            let stream = futures_util::stream::unfold(0, move |offset| async move {
                if offset == size {
                    return None;
                }
                let length = MIB.min(size - offset);
                let mut chunk = vec![b'x'; length];
                if offset == 0 {
                    chunk[..4].copy_from_slice(b"PK\x03\x04");
                }
                Some((Ok::<_, std::io::Error>(Bytes::from(chunk)), offset + length))
            });
            let mut response = Response::builder().header("content-type", "application/zip");
            if declared_length {
                response = response.header("content-length", size);
            }
            response.body(Body::from_stream(stream)).unwrap()
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (base, task)
}

fn app_state(base: &str) -> tauri::App<tauri::test::MockRuntime> {
    let state = build_app_state();
    app_with_state(base, state)
}

fn app_with_state(base: &str, state: AppState) -> tauri::App<tauri::test::MockRuntime> {
    *state.relay_url_override.lock().unwrap() = Some(base.into());
    tauri::test::mock_builder()
        .manage(state)
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap()
}

#[tokio::test]
async fn generic_download_above_fifty_mib_is_byte_exact() {
    for (size, declared_length) in [(60 * MIB, true), (60 * MIB, false), (100 * MIB, true)] {
        let (base, server) = blob_server(size, declared_length).await;
        let app = app_state(&base);
        let state = app.state::<AppState>();
        let result = fetch_blob_bytes(&format!("{base}/media/blob.zip"), &state).await;
        server.abort();
        let bytes = result.unwrap();
        assert_eq!(bytes.len(), size);
        assert_eq!(&bytes[..4], b"PK\x03\x04");
        assert!(bytes[4..].iter().all(|&byte| byte == b'x'));
    }
}

#[tokio::test]
async fn generic_download_rejects_over_hundred_mib_with_or_without_length() {
    for declared_length in [true, false] {
        let (base, server) = blob_server(100 * MIB + 1, declared_length).await;
        let app = app_state(&base);
        let state = app.state::<AppState>();
        let error = fetch_blob_bytes(&format!("{base}/media/blob.zip"), &state)
            .await
            .unwrap_err();
        server.abort();
        assert!(error.contains("file too large"), "{error}");
    }
}

#[tokio::test]
async fn image_fetch_cap_still_rejects_over_fifty_mib() {
    let (base, server) = blob_server(50 * MIB + 1, true).await;
    let app = app_state(&base);
    let state = app.state::<AppState>();
    let error = fetch_blob_bytes_with_cap(
        &format!("{base}/media/blob.zip"),
        &state,
        MAX_DOWNLOAD_BYTES,
        None,
    )
    .await
    .unwrap_err();
    server.abort();
    assert!(error.contains("file too large"), "{error}");
}

#[tokio::test]
#[ignore = "requires a relay fixture above 50 MiB and configured test credentials"]
async fn native_download_from_relay_matches_expected_sha256() {
    use sha2::{Digest, Sha256};
    let url = std::env::var("BUZZ_TEST_ATTACHMENT_URL").unwrap();
    let expected = std::env::var("BUZZ_TEST_ATTACHMENT_SHA256").unwrap();
    let parsed = url::Url::parse(&url).unwrap();
    let mut state = build_app_state();
    // A delegated test identity needs its owner's attestation for membership;
    // ordinary human clients use their own member key. Keep redirects disabled.
    if let Ok(tag) = std::env::var("BUZZ_AUTH_TAG") {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-auth-tag", tag.parse().unwrap());
        state.media_fetch_client = reqwest::Client::builder()
            .default_headers(headers)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
    }
    let app = app_with_state(&parsed.origin().ascii_serialization(), state);
    let state = app.state::<AppState>();
    let bytes = fetch_blob_bytes(&url, &state).await.unwrap();
    assert!(bytes.len() > 50 * MIB);
    assert_eq!(hex::encode(Sha256::digest(&bytes)), expected);
}
