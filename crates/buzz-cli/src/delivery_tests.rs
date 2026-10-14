//! Production CLI regressions against a signature-checking local mock relay.
use super::*;
use axum::{extract::State, response::IntoResponse, routing::post, Json, Router};
use nostr::{Event, EventBuilder, Kind, Tag};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct Relay {
    keys: Keys,
    channel: String,
    parent: Event,
    member: Arc<Mutex<bool>>,
    ack: Arc<Mutex<&'static str>>,
    membership_channel: Arc<Mutex<Option<String>>>,
    extra_members: Arc<Mutex<Vec<String>>>,
    events: Arc<Mutex<Vec<Event>>>,
    bodies: Arc<Mutex<Vec<Vec<u8>>>>,
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
    if filter["kinds"][0] == 0 {
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
    let mut events = r.events.lock().unwrap();
    let duplicate = events.iter().any(|e| e.id == event.id);
    let ack = *r.ack.lock().unwrap();
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

async fn fixture() -> (Relay, String, tempfile::TempDir) {
    fixture_with_keys(Keys::generate()).await
}

async fn fixture_with_keys(keys: Keys) -> (Relay, String, tempfile::TempDir) {
    let channel = "123e4567-e89b-12d3-a456-426614174000".to_string();
    let parent = EventBuilder::new(Kind::Custom(9), "original task")
        .tag(Tag::parse(["h", &channel]).unwrap())
        .sign_with_keys(&keys)
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
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .route("/query", post(query))
        .route("/events", post(publish))
        .with_state(relay.clone());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (relay, url, tempfile::tempdir().unwrap())
}

async fn deliver(r: &Relay, url: &str, dir: &std::path::Path, key: &str, content: &str) -> i32 {
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
    let client = BuzzClient::new(url.into(), r.keys.clone(), None, None).unwrap();
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
