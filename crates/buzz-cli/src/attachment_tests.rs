//! Attachment regressions through the production CLI and HTTP upload boundary.
use crate::client::{build_imeta_tag, BuzzClient};
use crate::commands::messages::{cmd_send_message, SendMessageParams};
use axum::{body::Bytes, extract::State, http::HeaderMap, routing::put, Json, Router};
use nostr::{Event, Keys};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

type Uploads = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

#[derive(Clone, Default)]
struct Store {
    uploads: Uploads,
    events: Arc<Mutex<Vec<Event>>>,
    legacy: bool,
}

async fn upload(
    State(store): State<Store>,
    headers: HeaderMap,
    body: Bytes,
) -> Json<serde_json::Value> {
    let mime = headers["content-type"].to_str().unwrap().to_owned();
    let hash = hex::encode(Sha256::digest(&body));
    assert_eq!(headers["x-sha-256"], hash);
    assert!(headers["authorization"]
        .to_str()
        .unwrap()
        .starts_with("Nostr "));
    store
        .uploads
        .lock()
        .unwrap()
        .push((mime.clone(), body.to_vec()));
    Json(serde_json::json!({
        "url": format!("https://relay.test/media/{hash}.bin"),
        "sha256": hash, "size": body.len(), "type": mime, "uploaded": 0
    }))
}

async fn relay(store: Store) -> (String, tokio::task::JoinHandle<()>) {
    let app = Router::new()
        .route(
            if store.legacy {
                "/media/upload"
            } else {
                "/upload"
            },
            put(upload),
        )
        .route(
            "/events",
            axum::routing::post(
                |State(store): State<Store>, Json(event): Json<Event>| async move {
                    event.verify().unwrap();
                    let id = event.id.to_hex();
                    store.events.lock().unwrap().push(event);
                    Json(serde_json::json!({"event_id": id, "accepted": true, "message": ""}))
                },
            ),
        )
        .with_state(store);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, task)
}

#[tokio::test]
async fn uploads_documents_archives_and_markdown_with_original_names() {
    let store = Store::default();
    let (url, task) = relay(store.clone()).await;
    let client = BuzzClient::new(url, Keys::generate(), None, None).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let fixtures: &[(&str, &[u8], &str)] = &[
        ("report.pdf", b"%PDF-1.4\n%%EOF\n", "application/pdf"),
        (
            "bundle.zip",
            b"PK\x03\x04\x14\x00\x00\x00",
            "application/zip",
        ),
        (
            "notes.md",
            "# Notes\n\nUnicode: 🐝\n".as_bytes(),
            "application/octet-stream",
        ),
        (
            "report.docx",
            b"PK\x03\x04\x14\x00\x00\x00",
            "application/zip",
        ),
        (
            "bundle.7z",
            b"7z\xbc\xaf\x27\x1c\x00\x04",
            "application/x-7z-compressed",
        ),
        ("bundle.rar", b"Rar!\x1a\x07\x00\x00", "application/vnd.rar"),
    ];
    for &(name, body, mime) in fixtures {
        let path = dir.path().join(name);
        std::fs::write(&path, body).unwrap();
        let result = client.upload_file(path.to_str().unwrap()).await;
        assert!(result.is_ok(), "{name}: {result:?}");
        let desc = result.unwrap();
        let tags = build_imeta_tag(&desc);
        assert!(tags.contains(&format!("filename {name}")), "{tags:?}");
        assert!(tags.contains(&format!("size {}", body.len())));
        assert_eq!(
            store.uploads.lock().unwrap().last().unwrap(),
            &(mime.into(), body.to_vec())
        );
    }
    task.abort();
}

#[tokio::test]
async fn legacy_upload_keeps_filename_metadata() {
    let (url, task) = relay(Store {
        legacy: true,
        ..Store::default()
    })
    .await;
    let client = BuzzClient::new(url, Keys::generate(), None, None).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.md");
    std::fs::write(&path, b"# Notes\n").unwrap();
    let desc = client.upload_file(path.to_str().unwrap()).await.unwrap();
    assert!(build_imeta_tag(&desc).contains(&"filename notes.md".into()));
    task.abort();
}

#[tokio::test]
async fn messages_emit_file_links_for_multiple_attachments_and_keep_media_markup() {
    for kind in [9, 45001] {
        let store = Store::default();
        let (url, task) = relay(store.clone()).await;
        let dir = tempfile::tempdir().unwrap();
        let fixtures: &[(&str, &[u8])] = &[
            ("notes [draft].md", b"# Notes\n"),
            ("bundle.zip", b"PK\x03\x04\x14\x00\x00\x00"),
            ("photo.jpg", b"\xff\xd8\xff\xe0\x00\x10JFIF\x00\x01"),
            (
                "clip.mp4",
                b"\x00\x00\x00\x18ftypisom\x00\x00\x00\x00isommp41",
            ),
        ];
        let client = BuzzClient::new(url, Keys::generate(), None, None).unwrap();
        let mut files = Vec::new();
        for &(name, bytes) in fixtures {
            let path = dir.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            files.push(path.to_str().unwrap().to_string());
        }
        cmd_send_message(
            &client,
            SendMessageParams {
                channel_id: "00000000-0000-0000-0000-000000000001".into(),
                content: "Attachments".into(),
                kind: Some(kind),
                reply_to: None,
                broadcast: false,
                files,
                mentions: vec![],
            },
        )
        .await
        .unwrap();
        let events = store.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.kind.as_u16(), kind);
        assert!(
            event
                .content
                .contains("\n[notes \\[draft\\].md](https://relay.test/media/"),
            "{}",
            event.content
        );
        assert!(event
            .content
            .contains("\n[bundle.zip](https://relay.test/media/"));
        assert!(event
            .content
            .contains("\n![image](https://relay.test/media/"));
        assert!(event
            .content
            .contains("\n![video](https://relay.test/media/"));
        assert_eq!(
            event
                .tags
                .iter()
                .filter(|t| t.as_slice()[0] == "imeta")
                .count(),
            4
        );
        drop(events);
        task.abort();
    }
}

#[tokio::test]
async fn upload_sanitizes_filename_for_relay_imeta_validation() {
    let (url, task) = relay(Store::default()).await;
    let client = BuzzClient::new(url, Keys::generate(), None, None).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report\n[draft].md");
    std::fs::write(&path, b"# Notes\n").unwrap();
    let desc = client.upload_file(path.to_str().unwrap()).await.unwrap();
    assert!(build_imeta_tag(&desc).contains(&"filename report[draft].md".into()));
    task.abort();
}

#[tokio::test]
async fn rejects_detected_executables_before_upload() {
    let store = Store::default();
    let (url, task) = relay(store.clone()).await;
    let client = BuzzClient::new(url, Keys::generate(), None, None).unwrap();
    let mut file = tempfile::NamedTempFile::new().unwrap();
    use std::io::Write;
    file.write_all(&[b"\x7fELF".as_slice(), &[0u8; 60]].concat())
        .unwrap();
    assert!(client
        .upload_file(file.path().to_str().unwrap())
        .await
        .is_err());
    assert!(store.uploads.lock().unwrap().is_empty());
    task.abort();
}

#[test]
fn enforces_file_caps_without_widening_supported_media() {
    use crate::attachments::validate_upload;
    for (mime, max) in [
        ("application/pdf", 104_857_600),
        ("application/zip", 104_857_600),
        ("application/octet-stream", 104_857_600),
        ("image/jpeg", 52_428_800),
        ("video/mp4", 524_288_000),
    ] {
        assert!(validate_upload(mime, max).is_ok());
        assert!(validate_upload(mime, max + 1).is_err());
    }
    for mime in [
        "audio/mpeg",
        "video/quicktime",
        "image/tiff",
        "image/svg+xml",
    ] {
        assert!(validate_upload(mime, 12).is_err());
    }
}

#[test]
fn unicode_filenames_fit_relay_byte_limit_and_keep_extensions() {
    use crate::attachments::sanitize_filename;
    for (name, expected, mime) in [
        (
            format!("{}.md", "é".repeat(128)),
            format!("{}.md", "é".repeat(126)),
            "application/octet-stream",
        ),
        (
            format!("{}.png", "🐝".repeat(128)),
            format!("{}.png", "🐝".repeat(62)),
            "image/png",
        ),
        (
            format!("{}.zip", "a".repeat(300)),
            format!("{}.zip", "a".repeat(251)),
            "application/zip",
        ),
    ] {
        let filename = sanitize_filename(&name);
        assert_eq!(filename, expected);
        assert!(filename.len() <= 255);
        let desc = serde_json::from_value(serde_json::json!({
            "url": "https://relay.test/media/blob.bin", "sha256": "ab",
            "size": 12, "type": mime, "uploaded": 0,
            "filename": filename,
        }))
        .unwrap();
        assert!(build_imeta_tag(&desc).contains(&format!("filename {expected}")));
    }
}
