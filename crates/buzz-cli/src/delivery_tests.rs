//! Production CLI regressions against a signature-checking local mock relay.
use super::*;
use axum::{extract::State, response::IntoResponse, routing::post, Json, Router};
use nostr::{Event, EventBuilder, Kind, Tag};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(super) struct Relay {
    pub(super) keys: Keys,
    pub(super) channel: String,
    pub(super) parent: Event,
    pub(super) member: Arc<Mutex<bool>>,
    pub(super) ack: Arc<Mutex<&'static str>>,
    pub(super) membership_channel: Arc<Mutex<Option<String>>>,
    pub(super) extra_members: Arc<Mutex<Vec<String>>>,
    pub(super) events: Arc<Mutex<Vec<Event>>>,
    pub(super) bodies: Arc<Mutex<Vec<Vec<u8>>>>,
    /// Workspace emoji sets served for kind-30030 palette queries.
    pub(super) palette: Arc<Mutex<Vec<Event>>>,
    pub(super) profiles: Arc<Mutex<Vec<Event>>>,
}

async fn query(
    State(r): State<Relay>,
    Json(filter): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let filter = &filter[0];
    if filter["kinds"][0] == 39002 {
        let membership_channel = r
            .membership_channel
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| filter["#d"][0].as_str().unwrap().to_string());
        let mut tags = if *r.member.lock().unwrap() {
            vec![
                Tag::parse(["d", &membership_channel]).unwrap(),
                Tag::parse(["p", &r.keys.public_key().to_hex()]).unwrap(),
            ]
        } else {
            vec![Tag::parse(["d", &membership_channel]).unwrap()]
        };
        if *r.member.lock().unwrap() {
            tags.extend(
                r.extra_members
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|pubkey| Tag::parse(["p", pubkey]).unwrap()),
            );
        }
        let e = EventBuilder::new(Kind::Custom(39002), "")
            .tags(tags)
            .allow_self_tagging()
            .sign_with_keys(&r.keys)
            .unwrap();
        return Json(serde_json::json!([e]));
    }
    if filter["kinds"][0] == 30030 {
        return Json(serde_json::json!(*r.palette.lock().unwrap()));
    }
    if filter["kinds"][0] == 0 {
        let profiles = r.profiles.lock().unwrap().clone();
        if !profiles.is_empty() {
            return Json(serde_json::json!(profiles));
        }
        let e = EventBuilder::new(Kind::Metadata, r#"{"name":"Callback"}"#)
            .sign_with_keys(&r.keys)
            .unwrap();
        return Json(serde_json::json!([e]));
    }
    if filter["ids"][0] == r.parent.id.to_hex() {
        return Json(serde_json::json!([r.parent]));
    }
    let events = r.events.lock().unwrap();
    Json(serde_json::json!(events
        .iter()
        .filter(|e| filter["ids"]
            .as_array()
            .is_none_or(|ids| ids.iter().any(|id| id == &e.id.to_hex())))
        .collect::<Vec<_>>()))
}

async fn publish(State(r): State<Relay>, body: axum::body::Bytes) -> axum::response::Response {
    let event: Event = serde_json::from_slice(&body).unwrap();
    event.verify().unwrap();
    r.bodies.lock().unwrap().push(body.to_vec());
    let ack = *r.ack.lock().unwrap();
    if ack == "http-error" {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"invalid: rejected by test relay"})),
        )
            .into_response();
    }
    let mut events = r.events.lock().unwrap();
    let duplicate = events.iter().any(|e| e.id == event.id);
    if !duplicate && ack != "rejected" {
        events.push(event.clone());
    }
    if ack == "malformed" {
        return "{broken ack".into_response();
    }
    match ack {
        "wrong-id" => {
            Json(serde_json::json!({"accepted":true,"event_id":"b".repeat(64),"message":"saved"}))
        }
        "rejected" => Json(
            serde_json::json!({"accepted":false,"event_id":event.id.to_hex(),"message":"rejected"}),
        ),
        _ => Json(
            serde_json::json!({"accepted":true,"event_id":event.id.to_hex(),"message":if duplicate {"duplicate:"} else {"saved"}}),
        ),
    }.into_response()
}

/// Blossom upload stub: describes the received blob without storing it.
async fn upload(
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Json<serde_json::Value> {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned()
    };
    let sha256 = header("x-sha-256");
    Json(serde_json::json!({
        "url": format!("https://media.example/{sha256}"),
        "sha256": sha256,
        "size": body.len(),
        "type": header("content-type"),
        "uploaded": 1,
    }))
}

pub(super) async fn fixture() -> (Relay, String, tempfile::TempDir) {
    fixture_with_keys(Keys::generate()).await
}

pub(super) async fn fixture_with_keys(keys: Keys) -> (Relay, String, tempfile::TempDir) {
    fixture_with_parent_author(keys.clone(), keys).await
}

async fn fixture_with_parent_author(
    keys: Keys,
    parent_author: Keys,
) -> (Relay, String, tempfile::TempDir) {
    let channel = "123e4567-e89b-12d3-a456-426614174000".to_string();
    let parent = EventBuilder::new(Kind::Custom(9), "original task")
        .tag(Tag::parse(["h", &channel]).unwrap())
        .sign_with_keys(&parent_author)
        .unwrap();
    let relay = Relay {
        keys,
        channel,
        parent,
        member: Arc::new(Mutex::new(true)),
        ack: Arc::new(Mutex::new("ok")),
        membership_channel: Arc::new(Mutex::new(None)),
        extra_members: Arc::new(Mutex::new(vec![])),
        events: Arc::new(Mutex::new(vec![])),
        bodies: Arc::new(Mutex::new(vec![])),
        palette: Arc::new(Mutex::new(vec![])),
        profiles: Arc::new(Mutex::new(vec![])),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .route("/query", post(query))
        .route("/events", post(publish))
        .route("/upload", axum::routing::put(upload))
        .with_state(relay.clone());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (relay, url, tempfile::tempdir().unwrap())
}

pub(super) async fn deliver(
    r: &Relay,
    url: &str,
    dir: &std::path::Path,
    key: &str,
    content: &str,
) -> i32 {
    deliver_with_status(r, url, dir, key, content, None).await
}

/// [`deliver`] from a CLI configured with a session-status directory.
pub(super) async fn deliver_with_status(
    r: &Relay,
    url: &str,
    dir: &std::path::Path,
    key: &str,
    content: &str,
    status_dir: Option<&std::path::Path>,
) -> i32 {
    let cli = match parse_args([
        "buzz",
        "messages",
        "deliver",
        "--delivery-key",
        key,
        "--journal-dir",
        dir.to_str().unwrap(),
        "--channel",
        &r.channel,
        "--reply-to",
        &r.parent.id.to_hex(),
        "--content",
        content,
    ]) {
        Ok(cli) => cli,
        Err(_) => return 1,
    };
    let Cmd::Messages(command) = cli.command else {
        panic!("expected messages command");
    };
    let client = BuzzClient::new(url.into(), r.keys.clone(), None, None)
        .unwrap()
        .with_session_status_dir(status_dir.map(std::path::Path::to_owned));
    match commands::messages::dispatch(command, &client, &OutputFormat::Json).await {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            error::exit_code(&e)
        }
    }
}

#[tokio::test]
async fn managed_same_key_changed_text_returns_first_signed_delivery() {
    let (r, url, dir) = fixture().await;
    let key = Uuid::new_v4().to_string();
    assert_eq!(deliver(&r, &url, dir.path(), &key, "first").await, 0);
    assert_eq!(deliver(&r, &url, dir.path(), &key, "changed").await, 0);
    let events = r.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].content, "first");
    assert!(events[0]
        .tags
        .iter()
        .any(|tag| tag.as_slice() == ["buzz-delivery", &key]));
    assert_eq!(r.bodies.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn managed_top_level_route_is_immutable_and_does_not_invent_a_thread() {
    let (r, url, dir) = fixture().await;
    let key = Uuid::new_v4().to_string();
    let make = |text: &str| {
        let cli = parse_args([
            "buzz",
            "messages",
            "deliver",
            "--delivery-key",
            &key,
            "--journal-dir",
            dir.path().to_str().unwrap(),
            "--channel",
            &r.channel,
            "--content",
            text,
        ])
        .expect("managed top-level reports need no reply parent");
        let Cmd::Messages(command) = cli.command else {
            panic!("messages command")
        };
        command
    };
    let client = BuzzClient::new(url, r.keys.clone(), None, None).unwrap();
    commands::messages::dispatch(make("first"), &client, &OutputFormat::Json)
        .await
        .unwrap();
    commands::messages::dispatch(make("changed"), &client, &OutputFormat::Json)
        .await
        .unwrap();
    let events = r.events.lock().unwrap().clone();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].content, "first");
    assert!(!events[0].tags.iter().any(|tag| tag.as_slice()[0] == "e"));
    let wrong = deliver(&r, client.relay_url(), dir.path(), &key, "wrong thread").await;
    assert_eq!(wrong, 5);
}

#[tokio::test]
async fn managed_equal_text_distinct_keys_have_distinct_signed_events() {
    let (r, url, dir) = fixture().await;
    for _ in 0..2 {
        assert_eq!(
            deliver(&r, &url, dir.path(), &Uuid::new_v4().to_string(), "same").await,
            0
        );
    }
    let events = r.events.lock().unwrap();
    assert_eq!(events.len(), 2);
    assert_ne!(events[0].id, events[1].id);
}

#[tokio::test]
async fn managed_concurrent_publishers_share_first_event() {
    let (r, url, dir) = fixture().await;
    let key = Uuid::new_v4().to_string();
    let (a, b) = tokio::join!(
        deliver(&r, &url, dir.path(), &key, "first"),
        deliver(&r, &url, dir.path(), &key, "second")
    );
    assert_eq!((a, b), (0, 0));
    assert_eq!(r.events.lock().unwrap().len(), 1);
    assert_eq!(r.bodies.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn managed_wrong_ack_keeps_signed_event_for_new_invocation() {
    let (r, url, dir) = fixture().await;
    let key = Uuid::new_v4().to_string();
    *r.ack.lock().unwrap() = "wrong-id";
    assert_ne!(deliver(&r, &url, dir.path(), &key, "original").await, 0);
    *r.ack.lock().unwrap() = "ok";
    assert_eq!(deliver(&r, &url, dir.path(), &key, "new text").await, 0);
    let bodies = r.bodies.lock().unwrap();
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[0], bodies[1]);
    assert_eq!(r.events.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn managed_revoked_access_refuses_pending_and_confirmed_replay() {
    let (r, url, dir) = fixture().await;
    let key = Uuid::new_v4().to_string();
    assert_eq!(deliver(&r, &url, dir.path(), &key, "first").await, 0);
    *r.member.lock().unwrap() = false;
    assert_eq!(deliver(&r, &url, dir.path(), &key, "changed").await, 3);
    assert_eq!(
        deliver(&r, &url, dir.path(), &Uuid::new_v4().to_string(), "new").await,
        3
    );
    assert_eq!(r.events.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn managed_mentions_progress_and_explicit_edit_remain_independent() {
    let (r, url, dir) = fixture().await;
    let key = Uuid::new_v4().to_string();
    let client = BuzzClient::new(url.clone(), r.keys.clone(), None, None).unwrap();
    commands::messages::cmd_send_message(
        &client,
        commands::messages::SendMessageParams {
            channel_id: r.channel.clone(),
            content: "progress".into(),
            kind: None,
            reply_to: Some(r.parent.id.to_hex()),
            broadcast: false,
            files: vec![],
            mentions: vec![],
        },
    )
    .await
    .unwrap();
    assert_eq!(
        deliver(&r, &url, dir.path(), &key, "@Callback final").await,
        0
    );
    let event = r.events.lock().unwrap()[1].clone();
    assert!(event
        .tags
        .iter()
        .any(|tag| tag.as_slice() == ["p", &r.keys.public_key().to_hex()]));
    commands::messages::cmd_edit_message(&client, &event.id.to_hex(), "correction")
        .await
        .unwrap();
    assert_eq!(
        deliver(&r, &url, dir.path(), &key, "another final").await,
        0
    );
    let events = r.events.lock().unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].content, "progress");
    assert_eq!(events[2].kind.as_u16(), 40003);
}

#[tokio::test]
async fn managed_pending_attempt_budget_is_finite_and_retains_evidence() {
    let (r, url, dir) = fixture().await;
    let key = Uuid::new_v4().to_string();
    *r.ack.lock().unwrap() = "wrong-id";
    for _ in 0..7 {
        assert_eq!(deliver(&r, &url, dir.path(), &key, "first").await, 2);
    }
    assert_eq!(deliver(&r, &url, dir.path(), &key, "changed").await, 4);
    assert_eq!(
        deliver(&r, &url, dir.path(), &key, "changed again").await,
        4
    );
    assert_eq!(r.bodies.lock().unwrap().len(), 8);
    assert_eq!(r.events.lock().unwrap().len(), 1);
    assert!(std::fs::read_to_string(dir.path().join("journal.json"))
        .unwrap()
        .contains("first"));
}

#[tokio::test]
async fn managed_route_conflict_does_not_publish_or_reroute() {
    let (r, url, dir) = fixture().await;
    let key = Uuid::new_v4().to_string();
    assert_eq!(deliver(&r, &url, dir.path(), &key, "first").await, 0);
    let mut changed = r.clone();
    changed.channel = Uuid::new_v4().to_string();
    assert_eq!(
        deliver(&changed, &url, dir.path(), &key, "changed").await,
        5
    );
    assert_eq!(
        deliver(
            &changed,
            &url,
            dir.path(),
            &Uuid::new_v4().to_string(),
            "new"
        )
        .await,
        5
    );
    assert_eq!(r.events.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn managed_corrupt_or_oversized_journal_never_re_signs() {
    let (r, url, dir) = fixture().await;
    let key = Uuid::new_v4().to_string();
    assert_eq!(deliver(&r, &url, dir.path(), &key, "first").await, 0);
    let path = dir.path().join("journal.json");
    let original = std::fs::read(&path).unwrap();
    std::fs::write(&path, b"broken").unwrap();
    assert_eq!(deliver(&r, &url, dir.path(), &key, "second").await, 4);
    assert_eq!(std::fs::read(&path).unwrap(), b"broken");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(16 * 1024 * 1024 + 1).unwrap();
    assert_eq!(deliver(&r, &url, dir.path(), &key, "third").await, 4);
    std::fs::write(&path, original).unwrap();
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["entries"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap()["event"]["content"] = "signature corruption".into();
    std::fs::write(&path, value.to_string()).unwrap();
    assert_eq!(deliver(&r, &url, dir.path(), &key, "fourth").await, 4);
    assert_eq!(r.events.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn managed_confirmed_expiry_is_explicit_boundary_pending_never_expires() {
    let (r, url, dir) = fixture().await;
    let key = Uuid::new_v4().to_string();
    assert_eq!(deliver(&r, &url, dir.path(), &key, "first").await, 0);
    let path = dir.path().join("journal.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["entries"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap()["confirmed_at"] = 1.into();
    std::fs::write(&path, value.to_string()).unwrap();
    assert_eq!(
        deliver(&r, &url, dir.path(), &key, "after retention").await,
        0
    );
    assert_eq!(r.events.lock().unwrap().len(), 2);
    value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["entries"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap()["confirmed_at"] = serde_json::Value::Null;
    std::fs::write(&path, value.to_string()).unwrap();
    assert_eq!(
        deliver(&r, &url, dir.path(), &key, "pending retry").await,
        0
    );
    assert_eq!(r.events.lock().unwrap().len(), 2);
    let bodies = r.bodies.lock().unwrap();
    assert_eq!(bodies[1], bodies[2]);
}

#[tokio::test]
async fn managed_full_pending_journal_rejects_new_keys_without_evicting_or_growing_locks() {
    use sha2::{Digest, Sha256};
    let (r, url, dir) = fixture().await;
    let original_key = Uuid::new_v4().to_string();
    assert_eq!(
        deliver(&r, &url, dir.path(), &original_key, "first").await,
        0
    );
    let path = dir.path().join("journal.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let template = value["entries"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone();
    for _ in 1..1024 {
        let key = Uuid::new_v4().to_string();
        let mut record = template.clone();
        let tags: Vec<Tag> = r.events.lock().unwrap()[0]
            .tags
            .iter()
            .filter(|t| t.as_slice()[0] != "buzz-delivery")
            .cloned()
            .collect();
        let event = EventBuilder::new(Kind::Custom(9), "retained pending")
            .tags(tags)
            .tag(Tag::parse(["buzz-delivery", &key]).unwrap())
            .sign_with_keys(&r.keys)
            .unwrap();
        record["key"] = key.clone().into();
        record["event"] = serde_json::to_value(event).unwrap();
        record["confirmed_at"] = serde_json::Value::Null;
        let namespace = serde_json::to_vec(&(&url, r.keys.public_key().to_hex(), &key)).unwrap();
        let id = hex::encode(Sha256::digest(namespace));
        value["entries"].as_object_mut().unwrap().insert(id, record);
    }
    std::fs::write(&path, value.to_string()).unwrap();
    assert_eq!(
        deliver(
            &r,
            &url,
            dir.path(),
            &Uuid::new_v4().to_string(),
            "overflow"
        )
        .await,
        4
    );
    assert_eq!(
        deliver(&r, &url, dir.path(), &original_key, "first replay").await,
        0
    );
    let after: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(after["entries"].as_object().unwrap().len(), 1024);
    assert_eq!(r.events.lock().unwrap().len(), 1);
    assert!(std::fs::read_dir(dir.path()).unwrap().count() <= 3);
}

#[tokio::test]
async fn managed_membership_snapshot_from_another_channel_cannot_authorize_delivery() {
    let (r, url, dir) = fixture().await;
    *r.membership_channel.lock().unwrap() = Some(Uuid::new_v4().to_string());
    assert_eq!(
        deliver(&r, &url, dir.path(), &Uuid::new_v4().to_string(), "first").await,
        4
    );
    assert!(r.events.lock().unwrap().is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn managed_new_journal_directories_and_files_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let (r, url, dir) = fixture().await;
    let nested = dir.path().join("parent/new journal");
    assert_eq!(
        deliver(
            &r,
            &url,
            &nested,
            &Uuid::new_v4().to_string(),
            "private final report"
        )
        .await,
        0
    );
    assert_eq!(
        std::fs::metadata(&nested).unwrap().permissions().mode() & 0o777,
        0o700
    );
    for file in std::fs::read_dir(&nested).unwrap() {
        assert_eq!(
            file.unwrap().metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[tokio::test(start_paused = true)]
async fn managed_busy_journal_lock_has_bounded_safe_retry_wait() {
    use fs2::FileExt;
    let (r, url, dir) = fixture().await;
    let held = std::fs::File::create(dir.path().join("journal.lock")).unwrap();
    held.lock_exclusive().unwrap();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(31),
        deliver(&r, &url, dir.path(), &Uuid::new_v4().to_string(), "first"),
    )
    .await;
    assert_eq!(result.unwrap(), 2);
    assert!(r.events.lock().unwrap().is_empty());
}

#[tokio::test]
async fn managed_malformed_and_rejected_ack_remain_pending_until_valid_ack() {
    let (r, url, dir) = fixture().await;
    let key = Uuid::new_v4().to_string();
    *r.ack.lock().unwrap() = "rejected";
    assert_eq!(deliver(&r, &url, dir.path(), &key, "first").await, 2);
    assert!(r.events.lock().unwrap().is_empty());
    *r.ack.lock().unwrap() = "malformed";
    assert_eq!(deliver(&r, &url, dir.path(), &key, "changed").await, 2);
    *r.ack.lock().unwrap() = "ok";
    assert_eq!(
        deliver(&r, &url, dir.path(), &key, "changed again").await,
        0
    );
    assert_eq!(r.events.lock().unwrap().len(), 1);
    let bodies = r.bodies.lock().unwrap();
    assert_eq!(bodies.len(), 3);
    assert!(bodies.iter().all(|b| b == &bodies[0]));
}

#[tokio::test]
async fn managed_oversized_record_and_changed_saved_namespace_or_route_fail_closed() {
    let (r, url, dir) = fixture().await;
    let key = Uuid::new_v4().to_string();
    assert_eq!(deliver(&r, &url, dir.path(), &key, "first").await, 0);
    let path = dir.path().join("journal.json");
    let original: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for field in ["last_error", "relay", "parent", "attempts"] {
        let mut value = original.clone();
        let record = value["entries"]
            .as_object_mut()
            .unwrap()
            .values_mut()
            .next()
            .unwrap();
        record[field] = match field {
            "last_error" => "x".repeat(512 * 1024).into(),
            "relay" => "https://other.invalid".into(),
            "parent" => "b".repeat(64).into(),
            _ => 9.into(),
        };
        std::fs::write(&path, value.to_string()).unwrap();
        assert_eq!(deliver(&r, &url, dir.path(), &key, "changed").await, 4);
    }
    assert_eq!(r.events.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn managed_key_namespace_separates_signers_and_relays() {
    let (r, url, dir) = fixture().await;
    let key = Uuid::new_v4().to_string();
    assert_eq!(deliver(&r, &url, dir.path(), &key, "first signer").await, 0);
    let mut second_signer = r.clone();
    second_signer.keys = Keys::generate();
    r.extra_members
        .lock()
        .unwrap()
        .push(second_signer.keys.public_key().to_hex());
    assert_eq!(
        deliver(&second_signer, &url, dir.path(), &key, "second signer").await,
        0
    );
    assert_eq!(r.events.lock().unwrap().len(), 2);
    let (other_relay, other_url, _) = fixture_with_keys(r.keys.clone()).await;
    assert_eq!(
        deliver(&other_relay, &other_url, dir.path(), &key, "other relay").await,
        0
    );
    assert_eq!(other_relay.events.lock().unwrap().len(), 1);
    let journal: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("journal.json")).unwrap()).unwrap();
    assert_eq!(journal["entries"].as_object().unwrap().len(), 3);
}

#[tokio::test]
async fn managed_explicit_callback_mention_preserves_unresolved_presentation_name() {
    let (r, url, dir) = fixture().await;
    let client = BuzzClient::new(url, r.keys.clone(), None, None).unwrap();
    let cli = parse_args([
        "buzz",
        "messages",
        "deliver",
        "--delivery-key",
        &Uuid::new_v4().to_string(),
        "--journal-dir",
        dir.path().to_str().unwrap(),
        "--channel",
        &r.channel,
        "--reply-to",
        &r.parent.id.to_hex(),
        "--content",
        "@Unknown callback",
        "--mention",
        &r.keys.public_key().to_hex(),
    ])
    .unwrap();
    let Cmd::Messages(command) = cli.command else {
        panic!("expected messages command");
    };
    commands::messages::dispatch(command, &client, &OutputFormat::Json)
        .await
        .unwrap();
    let events = r.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert!(events[0]
        .tags
        .iter()
        .any(|tag| tag.as_slice() == ["p", &r.keys.public_key().to_hex()]));
}

#[cfg(unix)]
#[tokio::test]
async fn managed_existing_directory_requires_parent_link_sync_before_publication() {
    use std::os::unix::fs::PermissionsExt;
    let (r, url, dir) = fixture().await;
    let parent = dir.path().join("parent");
    let journal = parent.join("journal");
    std::fs::create_dir_all(&journal).unwrap();
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o100)).unwrap();
    let result = deliver(
        &r,
        &url,
        &journal,
        &Uuid::new_v4().to_string(),
        "must not publish without durable directory link",
    )
    .await;
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(result, 4);
    assert!(r.events.lock().unwrap().is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn managed_existing_nested_directory_requires_ancestor_link_sync_before_publication() {
    use std::os::unix::fs::PermissionsExt;
    let (r, url, dir) = fixture().await;
    let ancestor = dir.path().join("ancestor");
    let journal = ancestor.join("parent/journal");
    // A concurrent creator can expose the entire chain before syncing its links.
    std::fs::create_dir_all(&journal).unwrap();
    std::fs::set_permissions(&ancestor, std::fs::Permissions::from_mode(0o100)).unwrap();
    let result = deliver(
        &r,
        &url,
        &journal,
        &Uuid::new_v4().to_string(),
        "must not publish without durable ancestor links",
    )
    .await;
    std::fs::set_permissions(&ancestor, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(result, 4);
    assert!(r.events.lock().unwrap().is_empty());
}

#[tokio::test]
async fn managed_expired_journal_never_treats_legacy_wrapper_ack_as_delivery() {
    let (r, url, dir) = fixture().await;
    let key = Uuid::new_v4().to_string();
    assert_eq!(deliver(&r, &url, dir.path(), &key, "first").await, 0);
    let path = dir.path().join("journal.json");
    let mut journal: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let record = journal["entries"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap();
    let first: Event = serde_json::from_value(record["event"].clone()).unwrap();
    let old = EventBuilder::new(first.kind, &first.content)
        .tags(first.tags.clone())
        .allow_self_tagging()
        .custom_created_at(nostr::Timestamp::from(
            nostr::Timestamp::now().as_secs() - 3600,
        ))
        .sign_with_keys(&r.keys)
        .unwrap();
    record["event"] = serde_json::to_value(&old).unwrap();
    record["confirmed_at"] = serde_json::Value::Null;
    std::fs::write(&path, journal.to_string()).unwrap();
    r.events.lock().unwrap().clear();
    assert_eq!(
        deliver(&r, &url, dir.path(), &key, "changed after restart").await,
        2
    );
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let record = saved["entries"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    let original: Event = serde_json::from_value(record["event"].clone()).unwrap();
    let stamp: buzz_core::managed_delivery::ReportStamp =
        serde_json::from_value(record["recovery_candidate"].clone()).unwrap();
    let candidate = stamp.restore(&original).unwrap();
    assert_eq!(original, old);
    assert!(buzz_core::managed_delivery::same_report(
        &original, &candidate
    ));
    assert!(record["confirmed_at"].is_null());
    assert!(r
        .events
        .lock()
        .unwrap()
        .iter()
        .all(|e| u32::from(e.kind.as_u16()) == buzz_core::kind::KIND_MANAGED_DELIVERY_RECOVERY));
}

async fn runtime_dispatch(client: &BuzzClient, args: &[&str]) -> Result<(), error::CliError> {
    let cli = parse_args(args.iter().copied()).unwrap();
    let Cmd::Messages(command) = cli.command else {
        panic!("messages command")
    };
    commands::messages::dispatch(command, client, &OutputFormat::Json).await
}

#[tokio::test]
async fn runtime_final_cli_replay_uses_saved_bytes_and_filters_legacy_receipts() {
    let (relay, url, dir) = fixture().await;
    let key = uuid::Uuid::new_v4().to_string();
    let client = BuzzClient::new(url.clone(), relay.keys.clone(), None, None).unwrap();
    *relay.ack.lock().unwrap() = "malformed";
    let args = [
        "buzz",
        "messages",
        "deliver",
        "--delivery-key",
        &key,
        "--channel",
        &relay.channel,
        "--reply-to",
        &relay.parent.id.to_hex(),
        "--journal-dir",
        dir.path().to_str().unwrap(),
        "--runtime-purpose",
        "answer",
        "--content",
        "Which option?",
    ];
    assert!(runtime_dispatch(&client, &args).await.is_err());
    let bytes = std::fs::read(dir.path().join("journal.json")).unwrap();
    let journal: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let record = journal["entries"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    assert_eq!(journal["version"], 3);
    assert_eq!(record["runtime"]["purpose"], "answer");
    assert_eq!(record["event"]["content"], "Which option?");
    *relay.ack.lock().unwrap() = "ok";
    let retry = [
        "buzz",
        "messages",
        "delivery-retry",
        "--delivery-key",
        &key,
        "--journal-dir",
        dir.path().to_str().unwrap(),
    ];
    runtime_dispatch(&client, &retry).await.unwrap();
    runtime_dispatch(&client, &retry).await.unwrap();
    let bodies = relay.bodies.lock().unwrap().clone();
    assert_eq!(
        bodies.len(),
        2,
        "confirmed replay must suppress another submission"
    );
    assert_eq!(
        bodies[0], bodies[1],
        "ACK loss retries original signed bytes"
    );
    assert_eq!(relay.events.lock().unwrap().len(), 1);
    // A v2/manual record cannot be selected through the runtime-only retry API.
    let legacy_key = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        deliver(&relay, &url, dir.path(), &legacy_key, "manual").await,
        0
    );
    assert!(runtime_dispatch(
        &client,
        &[
            "buzz",
            "messages",
            "delivery-retry",
            "--delivery-key",
            &legacy_key,
            "--journal-dir",
            dir.path().to_str().unwrap()
        ]
    )
    .await
    .is_err());
}

#[tokio::test]
async fn runtime_final_cli_revocation_and_access_loss_do_not_replay_or_reroute() {
    let (relay, url, dir) = fixture().await;
    let client = BuzzClient::new(url, relay.keys.clone(), None, None).unwrap();
    let key = uuid::Uuid::new_v4().to_string();
    *relay.ack.lock().unwrap() = "malformed";
    assert!(runtime_dispatch(
        &client,
        &[
            "buzz",
            "messages",
            "deliver",
            "--delivery-key",
            &key,
            "--channel",
            &relay.channel,
            "--journal-dir",
            dir.path().to_str().unwrap(),
            "--runtime-purpose",
            "failure_notice",
            "--content",
            "No final text was available."
        ]
    )
    .await
    .is_err());
    let retry = [
        "buzz",
        "messages",
        "delivery-retry",
        "--delivery-key",
        &key,
        "--journal-dir",
        dir.path().to_str().unwrap(),
    ];
    *relay.member.lock().unwrap() = false;
    assert!(runtime_dispatch(&client, &retry).await.is_err());
    runtime_dispatch(
        &client,
        &[
            "buzz",
            "messages",
            "delivery-revoke",
            "--delivery-key",
            &key,
            "--journal-dir",
            dir.path().to_str().unwrap(),
        ],
    )
    .await
    .unwrap();
    *relay.member.lock().unwrap() = true;
    assert!(runtime_dispatch(&client, &retry).await.is_err());
    assert_eq!(relay.bodies.lock().unwrap().len(), 1);
    let journal: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("journal.json")).unwrap()).unwrap();
    let record = journal["entries"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    assert_eq!(record["runtime"]["revoked"], true);
    assert_eq!(
        record["attempts"], 1,
        "revocation preserves actual submission attempts"
    );
}

#[tokio::test]
async fn runtime_final_literal_prose_never_adds_foreign_mention_recipients() {
    use nostr::ToBech32;
    let delegator = Keys::generate();
    let delegator_key = delegator.public_key().to_hex();
    let (relay, url, dir) = fixture_with_parent_author(Keys::generate(), delegator).await;
    relay
        .extra_members
        .lock()
        .unwrap()
        .push(delegator_key.clone());
    let foreign = Keys::generate();
    let foreign_key = foreign.public_key().to_hex();
    relay
        .extra_members
        .lock()
        .unwrap()
        .push(foreign_key.clone());
    relay.profiles.lock().unwrap().push(
        EventBuilder::new(Kind::Metadata, r#"{"name":"Foreign"}"#)
            .sign_with_keys(&foreign)
            .unwrap(),
    );
    let client = BuzzClient::new(url.clone(), relay.keys.clone(), None, None).unwrap();
    let text = format!(
        "Discuss @Foreign and nostr:{} as literal references.",
        foreign.public_key().to_bech32().unwrap()
    );
    for content in [&text, "The example contains @Unlisted without a recipient."] {
        let key = Uuid::new_v4().to_string();
        runtime_dispatch(
            &client,
            &[
                "buzz",
                "messages",
                "deliver",
                "--delivery-key",
                &key,
                "--channel",
                &relay.channel,
                "--reply-to",
                &relay.parent.id.to_hex(),
                "--journal-dir",
                dir.path().to_str().unwrap(),
                "--runtime-purpose",
                "answer",
                "--mention",
                &delegator_key,
                "--content",
                content,
            ],
        )
        .await
        .unwrap();
        let events = relay.events.lock().unwrap();
        let event = events.last().unwrap();
        assert_eq!(event.content, content);
        let recipients: Vec<_> = event
            .tags
            .iter()
            .filter(|tag| tag.as_slice().first().map(String::as_str) == Some("p"))
            .map(|tag| tag.as_slice()[1].clone())
            .collect();
        assert_eq!(
            recipients,
            vec![delegator_key.clone()],
            "only the structured original delegator callback is authorized"
        );
    }
    // The ordinary agent-tools path retains its existing automatic mention behavior.
    assert_eq!(
        deliver(&relay, &url, dir.path(), &Uuid::new_v4().to_string(), &text).await,
        0
    );
    assert!(relay
        .events
        .lock()
        .unwrap()
        .last()
        .unwrap()
        .tags
        .iter()
        .any(|tag| tag.as_slice() == ["p", foreign_key.as_str()]));
}

// Removing the saved scope/namespace checks must revoke unrelated records and
// fail this production CLI dispatch regression.
#[tokio::test]
async fn runtime_final_cli_scope_revocation_preserves_other_scopes_and_namespaces() {
    let (relay, url, dir) = fixture().await;
    let client = BuzzClient::new(url.clone(), relay.keys.clone(), None, None).unwrap();
    let other_keys = Keys::generate();
    relay
        .extra_members
        .lock()
        .unwrap()
        .push(other_keys.public_key().to_hex());
    let other_signer = BuzzClient::new(url, other_keys, None, None).unwrap();
    let (other_relay, other_url, _) = fixture_with_keys(relay.keys.clone()).await;
    let other_client = BuzzClient::new(other_url, relay.keys.clone(), None, None).unwrap();
    *relay.ack.lock().unwrap() = "malformed";
    *other_relay.ack.lock().unwrap() = "malformed";
    let root = relay.parent.id.to_hex();
    let other_parent = EventBuilder::new(Kind::Custom(9), "another request")
        .tag(Tag::parse(["h", &relay.channel]).unwrap())
        .sign_with_keys(&relay.keys)
        .unwrap();
    let other_root = other_parent.id.to_hex();
    relay.events.lock().unwrap().push(other_parent);
    let other_channel = "223e4567-e89b-12d3-a456-426614174000";
    let cases = [
        (
            &client,
            relay.channel.as_str(),
            root.as_str(),
            "conversation",
        ),
        (&client, relay.channel.as_str(), root.as_str(), "thread"),
        (
            &client,
            relay.channel.as_str(),
            other_root.as_str(),
            "thread",
        ),
        (&client, other_channel, "", "conversation"),
        (
            &other_signer,
            relay.channel.as_str(),
            root.as_str(),
            "conversation",
        ),
        (&other_client, relay.channel.as_str(), "", "conversation"),
    ];
    let mut keys = Vec::new();
    for (owner, channel, parent, scope) in cases {
        let key = uuid::Uuid::new_v4().to_string();
        let mut args = vec![
            "buzz",
            "messages",
            "deliver",
            "--channel",
            channel,
            "--delivery-key",
            &key,
            "--journal-dir",
            dir.path().to_str().unwrap(),
            "--runtime-purpose",
            "answer",
            "--runtime-scope",
            scope,
            "--content",
            "saved final",
        ];
        if !parent.is_empty() {
            args.extend(["--reply-to", parent]);
        }
        assert!(runtime_dispatch(owner, &args).await.is_err());
        keys.push(key);
    }
    let before = relay.bodies.lock().unwrap().len();
    // Local durable revocation remains authorized after channel access is lost.
    *relay.member.lock().unwrap() = false;
    let revoke = [
        "buzz",
        "messages",
        "delivery-revoke-scope",
        "--channel",
        &relay.channel,
        "--journal-dir",
        dir.path().to_str().unwrap(),
    ];
    runtime_dispatch(&client, &revoke).await.unwrap();
    let read_records = || {
        let journal: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.path().join("journal.json")).unwrap())
                .unwrap();
        keys.iter()
            .map(|key| {
                journal["entries"]
                    .as_object()
                    .unwrap()
                    .values()
                    .find(|r| r["key"] == *key)
                    .unwrap()
                    .clone()
            })
            .collect::<Vec<_>>()
    };
    let records = read_records();
    for (index, record) in records.iter().enumerate() {
        assert_eq!(
            record["runtime"]["revoked"],
            index == 0,
            "conversation cancel crossed scope or namespace: {index}"
        );
        assert_eq!(record["attempts"], 1);
    }
    let mut thread_revoke = revoke.to_vec();
    thread_revoke.extend(["--thread-root", &root]);
    runtime_dispatch(&client, &thread_revoke).await.unwrap();
    let records = read_records();
    for (index, record) in records.iter().enumerate() {
        assert_eq!(
            record["runtime"]["revoked"],
            index < 2,
            "thread cancel crossed scope or namespace: {index}"
        );
        assert_eq!(record["attempts"], 1);
    }
    assert_eq!(
        relay.bodies.lock().unwrap().len(),
        before,
        "revocation must never submit events"
    );
    *relay.member.lock().unwrap() = true;
    assert!(runtime_dispatch(
        &client,
        &[
            "buzz",
            "messages",
            "delivery-retry",
            "--delivery-key",
            &keys[1],
            "--journal-dir",
            dir.path().to_str().unwrap()
        ]
    )
    .await
    .is_err());
    assert_eq!(relay.bodies.lock().unwrap().len(), before);
}
