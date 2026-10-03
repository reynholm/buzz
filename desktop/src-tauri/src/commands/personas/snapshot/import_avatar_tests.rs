use super::{materialize_import_avatar, materialize_import_avatar_scoped};
use std::cell::Cell;

#[tokio::test]
async fn inline_avatar_is_uploaded_and_replaced_with_hosted_url() {
    let uploaded = Cell::new(false);
    let result = materialize_import_avatar(
        Some("data:image/png;base64,iVBORw0KGgo="),
        Some("https://sender.invalid/avatar.png"),
        |bytes| {
            uploaded.set(true);
            async move {
                assert_eq!(bytes, b"\x89PNG\r\n\x1a\n");
                Ok("https://relay.example/media/avatar.png".to_string())
            }
        },
    )
    .await
    .unwrap();

    assert!(uploaded.get());
    assert_eq!(
        result.as_deref(),
        Some("https://relay.example/media/avatar.png")
    );
}

#[tokio::test]
async fn hosted_avatar_skips_upload() {
    let result =
        materialize_import_avatar(None, Some("https://sender.example/avatar.png"), |_| async {
            panic!("hosted avatars must not be uploaded")
        })
        .await
        .unwrap();

    assert_eq!(result.as_deref(), Some("https://sender.example/avatar.png"));
}

#[tokio::test]
async fn relay_sized_inline_avatar_becomes_bounded_signed_profile() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use image::ImageEncoder;
    use nostr::JsonUtil;

    let mut pixels = vec![0_u8; 512 * 512 * 4];
    let mut seed = 0x1234_5678_u32;
    for byte in &mut pixels {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        *byte = seed as u8;
    }
    let mut source = Vec::new();
    image::codecs::png::PngEncoder::new(&mut source)
        .write_image(&pixels, 512, 512, image::ExtendedColorType::Rgba8)
        .unwrap();
    assert!(source.len() > 256 * 1024);
    let data_url = format!("data:image/png;base64,{}", STANDARD.encode(&source));
    assert!(data_url.len() > 256 * 1024);

    let avatar = materialize_import_avatar(Some(&data_url), None, |bytes| async move {
        let mime = crate::commands::media::detect_and_validate_mime(&bytes)?;
        assert_eq!(mime, "image/png");
        let sanitized = crate::commands::media::sanitize_image_for_upload(bytes, &mime)?;
        image::load_from_memory(&sanitized).map_err(|error| error.to_string())?;
        Ok("https://relay.example/media/avatar.png".to_string())
    })
    .await
    .unwrap()
    .unwrap();

    let event =
        crate::events::build_profile(Some("Imported agent"), None, Some(&avatar), None, None)
            .unwrap()
            .sign_with_keys(&nostr::Keys::generate())
            .unwrap();
    assert!(event.content.len() < 64 * 1024);
    assert!(!event.content.contains("data:image/"));
    assert!(event
        .content
        .contains("https://relay.example/media/avatar.png"));
    assert!(event.as_json().len() < 256 * 1024);
}

#[tokio::test]
async fn upload_failure_aborts_avatar_materialization() {
    let result = materialize_import_avatar(
        Some("data:image/png;base64,iVBORw0KGgo="),
        None,
        |_| async { Err("relay upload failed".to_string()) },
    )
    .await;

    assert_eq!(result.unwrap_err(), "relay upload failed");
}

#[tokio::test]
async fn malformed_inline_avatar_fails_before_upload() {
    let result =
        materialize_import_avatar(Some("data:image/png;base64,not-base64!"), None, |_| async {
            panic!("malformed avatars must not be uploaded")
        })
        .await;

    assert_eq!(result.unwrap_err(), "Snapshot avatar data is malformed.");
}

#[tokio::test]
async fn avatar_import_refuses_changed_backend_scope_before_upload_and_after_await() {
    use crate::managed_agents::{
        device_creation::assert_creation_scope, device_home_migration::tests::app,
        device_home_sync::capture_scope,
    };
    use tauri::Manager;
    for before_upload in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let app = app(dir.path());
        let state = app.state::<crate::app_state::AppState>();
        let captured = capture_scope(&state).unwrap();
        if before_upload {
            *state.relay_url_override.lock().unwrap() = Some("wss://changed".into());
        }
        let uploads = Cell::new(0);
        let created = Cell::new(0);
        let result = materialize_import_avatar_scoped(
            Some("data:image/png;base64,iVBORw0KGgo="),
            None,
            || assert_creation_scope(&capture_scope(&state)?, &captured),
            |_| async {
                uploads.set(uploads.get() + 1);
                tokio::task::yield_now().await;
                *state.keys.lock().unwrap() = nostr::Keys::generate();
                Ok("https://original-relay.example/avatar.png".into())
            },
        )
        .await;
        if result.is_ok() {
            created.set(1);
        }
        assert!(result.is_err(), "scope switch must abort import");
        assert_eq!(uploads.get(), if before_upload { 0 } else { 1 });
        assert_eq!(created.get(), 0);
    }
}

#[tokio::test]
async fn avatar_upload_uses_bound_relay_and_signer_after_backend_switch() {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use image::ImageEncoder;
    use nostr::JsonUtil;
    use std::io::{Read, Write};
    use tauri::Manager;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut chunk = [0; 4096];
        loop {
            let n = stream.read(&mut chunk).unwrap();
            assert_ne!(n, 0);
            request.extend_from_slice(&chunk[..n]);
            if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..end]);
                let size: usize = headers
                    .lines()
                    .find_map(|line| {
                        line.to_lowercase()
                            .strip_prefix("content-length: ")
                            .map(str::to_string)
                    })
                    .unwrap()
                    .parse()
                    .unwrap();
                if request.len() >= end + 4 + size {
                    break;
                }
            }
        }
        let response = r#"{"url":"https://original.example/avatar.png","sha256":"hash","size":1,"type":"image/png","uploaded":1}"#;
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
        String::from_utf8_lossy(&request).into_owned()
    });
    let dir = tempfile::tempdir().unwrap();
    let app = crate::managed_agents::device_home_migration::tests::app(dir.path());
    let state = app.state::<crate::app_state::AppState>();
    *state.relay_url_override.lock().unwrap() = Some(format!("http://{address}"));
    let scope = crate::managed_agents::device_home_sync::capture_scope(&state).unwrap();
    let authority = super::import_upload_authority(&state, &scope).unwrap();
    *state.relay_url_override.lock().unwrap() = Some(format!("http://{address}/changed"));
    *state.keys.lock().unwrap() = nostr::Keys::generate();
    let mut image = Vec::new();
    image::codecs::png::PngEncoder::new(&mut image)
        .write_image(&[255, 0, 0, 255], 1, 1, image::ExtendedColorType::Rgba8)
        .unwrap();
    crate::commands::media::upload_image_bytes(image, &state, &authority)
        .await
        .unwrap();
    let request = server.join().unwrap();
    assert!(request.starts_with("PUT /upload HTTP/1.1"));
    let auth = request
        .lines()
        .find_map(|line| line.strip_prefix("authorization: Nostr "))
        .unwrap();
    let event = nostr::Event::from_json(URL_SAFE_NO_PAD.decode(auth).unwrap()).unwrap();
    event.verify().unwrap();
    assert_eq!(event.pubkey.to_hex(), scope.owner_pubkey);
}
