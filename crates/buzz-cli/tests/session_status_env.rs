//! The `buzz` binary takes its session-status directory from
//! `BUZZ_SESSION_STATUS_DIR`, the variable the agent harness sets for its
//! agents and managed MCP server.
//!
//! Runs the real binary against a local mock relay; the status directory is
//! laid out and locked the way the harness does it.

use std::fs::{self, File, OpenOptions};
use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::{extract::State, routing::post, Json, Router};
use buzz_core::session_status::{
    canonical_identity, generation_lock_path, scope_file_name, SessionState, SessionStatus,
    StatusEnvelope, StatusReason, StatusScope, GENERATIONS_DIR, INSTANCE_LOCK_FILE,
};
use fs2::FileExt;
use nostr::{Event, Keys};
use uuid::Uuid;

type Captured = Arc<Mutex<Vec<Event>>>;

async fn publish(
    State(captured): State<Captured>,
    body: axum::body::Bytes,
) -> Json<serde_json::Value> {
    let event: Event = serde_json::from_slice(&body).unwrap();
    event.verify().unwrap();
    let id = event.id.to_hex();
    captured.lock().unwrap().push(event);
    Json(serde_json::json!({"accepted": true, "event_id": id, "message": "saved"}))
}

async fn relay() -> (String, Captured) {
    let captured = Captured::default();
    let app = Router::new()
        .route("/query", post(|| async { Json(serde_json::json!([])) }))
        .route("/events", post(publish))
        .with_state(captured.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, captured)
}

fn held_lock(path: &Path) -> File {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    file.try_lock_exclusive().unwrap();
    file
}

async fn send(
    url: &str,
    captured: &Captured,
    keys: &Keys,
    channel: &str,
    status_dir: Option<&Path>,
) -> Event {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_buzz"));
    command
        .env_clear()
        .env("BUZZ_RELAY_URL", url)
        .env("BUZZ_PRIVATE_KEY", keys.secret_key().to_secret_hex())
        .args([
            "messages",
            "send",
            "--channel",
            channel,
            "--content",
            "hello",
        ]);
    if let Some(dir) = status_dir {
        command.env("BUZZ_SESSION_STATUS_DIR", dir);
    }
    let output = tokio::task::spawn_blocking(move || command.output().unwrap())
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "buzz failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let id = stdout["event_id"].as_str().unwrap();
    captured
        .lock()
        .unwrap()
        .iter()
        .find(|e| e.id.to_hex() == id)
        .cloned()
        .expect("relay received the event")
}

fn status_tags(event: &Event) -> Vec<Vec<String>> {
    event
        .tags
        .iter()
        .map(|t| t.as_slice().to_vec())
        .filter(|t| {
            t.first()
                .is_some_and(|name| name.starts_with("buzz-session"))
        })
        .collect()
}

#[tokio::test]
async fn binary_reads_the_status_directory_from_the_environment() {
    let (url, captured) = relay().await;
    let keys = Keys::generate();
    let channel = Uuid::new_v4();

    // Harness-shaped instance directory with live locks.
    let root = tempfile::tempdir().unwrap();
    let instance_id = Uuid::new_v4();
    let staging = root.path().join(format!(".staging-{instance_id}"));
    fs::create_dir(&staging).unwrap();
    let _instance = held_lock(&staging.join(INSTANCE_LOCK_FILE));
    fs::create_dir(staging.join(GENERATIONS_DIR)).unwrap();
    let dir = root.path().join(instance_id.to_string());
    fs::rename(&staging, &dir).unwrap();
    let generation = Uuid::new_v4();
    let _generation = held_lock(&dir.join(generation_lock_path(generation)));
    let scope = StatusScope {
        channel_id: channel,
        thread_root: None,
    };
    let transition = Uuid::new_v4();
    let envelope = StatusEnvelope::new(
        canonical_identity(&url, &keys.public_key().to_hex()).unwrap(),
        instance_id,
        scope.clone(),
        generation,
        SessionStatus {
            state: SessionState::New,
            reason: StatusReason::FirstInThread,
            transition_id: transition,
            occupancy: None,
            history: None,
        },
    );
    fs::write(
        dir.join(scope_file_name(&scope).unwrap()),
        envelope.to_snapshot_bytes().unwrap(),
    )
    .unwrap();

    let channel = channel.to_string();
    let with_env = send(&url, &captured, &keys, &channel, Some(&dir)).await;
    assert_eq!(
        status_tags(&with_env),
        vec![
            vec![
                "buzz-session".to_owned(),
                "new".into(),
                "first_in_thread".into(),
                String::new()
            ],
            vec![
                "buzz-session-info".to_owned(),
                "1".into(),
                transition.to_string(),
                "conversation".into(),
                String::new(),
                String::new(),
                "unknown".into(),
            ],
        ]
    );

    let without_env = send(&url, &captured, &keys, &channel, None).await;
    assert!(status_tags(&without_env).is_empty());
}
