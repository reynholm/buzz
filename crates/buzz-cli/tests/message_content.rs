//! Exercise content validation through the real CLI, including its stdin handling.

use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};

use axum::{extract::State, routing::post, Json, Router};
use nostr::{Event, Keys};
use uuid::Uuid;

#[derive(Clone, Default)]
struct Relay {
    requests: Arc<Mutex<usize>>,
    events: Arc<Mutex<Vec<Event>>>,
}

async fn relay() -> (String, Relay, tokio::task::JoinHandle<()>) {
    let state = Relay::default();
    let app = Router::new()
        .route(
            "/query",
            post(|State(state): State<Relay>| async move {
                *state.requests.lock().unwrap() += 1;
                Json(serde_json::json!([{
                    "tags": [["h", Uuid::nil().to_string()]]
                }]))
            }),
        )
        .route(
            "/events",
            post(
                |State(state): State<Relay>, Json(event): Json<Event>| async move {
                    event.verify().unwrap();
                    *state.requests.lock().unwrap() += 1;
                    let id = event.id;
                    state.events.lock().unwrap().push(event);
                    Json(serde_json::json!({"accepted": true, "event_id": id}))
                },
            ),
        )
        .route(
            "/upload",
            axum::routing::put(|State(state): State<Relay>| async move {
                *state.requests.lock().unwrap() += 1;
                Json(serde_json::json!({
                    "url": "https://media.example/report.txt",
                    "sha256": "a".repeat(64),
                    "size": 5,
                    "type": "application/octet-stream",
                    "uploaded": 1
                }))
            }),
        )
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, state, task)
}

async fn run(url: &str, args: &[&str], stdin: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_buzz"));
    command
        .env_clear()
        .env("BUZZ_RELAY_URL", url)
        .env(
            "BUZZ_PRIVATE_KEY",
            Keys::generate().secret_key().to_secret_hex(),
        )
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let stdin = stdin.map(str::to_owned);
    tokio::task::spawn_blocking(move || {
        let mut child = command.spawn().unwrap();
        if let Some(stdin) = stdin {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(stdin.as_bytes())
                .unwrap();
        }
        child.wait_with_output().unwrap()
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn blank_message_commands_fail_before_any_relay_request() {
    let (url, state, task) = relay().await;
    let channel = Uuid::nil().to_string();
    let event = "a".repeat(64);
    let commands = [
        vec!["messages", "send", "--channel", &channel, "--content"],
        vec![
            "messages",
            "send",
            "--channel",
            &channel,
            "--kind",
            "45001",
            "--content",
        ],
        vec![
            "messages",
            "send",
            "--channel",
            &channel,
            "--kind",
            "45003",
            "--reply-to",
            &event,
            "--content",
        ],
        vec!["messages", "edit", "--event", &event, "--content"],
        vec![
            "messages",
            "send-diff",
            "--channel",
            &channel,
            "--repo",
            "https://example.com/repo",
            "--commit",
            &event,
            "--diff",
        ],
    ];
    for command in commands {
        for (value, stdin) in [
            ("", None),
            (" \t\n\u{2003}", None),
            ("-", None),
            ("-", Some("")),
            ("-", Some(" \t\n\u{00a0}")),
        ] {
            let mut args = command.clone();
            args.push(value);
            let output = run(&url, &args, stdin).await;
            assert_eq!(output.status.code(), Some(1), "{args:?}: {output:?}");
            assert!(output.stdout.is_empty());
            let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
            assert_eq!(error["error"], "user_error");
            assert!(error["message"]
                .as_str()
                .unwrap()
                .starts_with("content is empty"));
        }
    }
    assert_eq!(*state.requests.lock().unwrap(), 0);
    task.abort();
}

#[tokio::test]
async fn message_commands_preserve_stdin_bytes_in_signed_events() {
    let (url, state, task) = relay().await;
    let channel = Uuid::nil().to_string();
    let event = "a".repeat(64);
    let content = "  text with `backticks` and $variables\n\n";
    for args in [
        vec!["messages", "send", "--channel", &channel, "--content", "-"],
        vec!["messages", "edit", "--event", &event, "--content", "-"],
        vec![
            "messages",
            "send-diff",
            "--channel",
            &channel,
            "--repo",
            "https://example.com/repo",
            "--commit",
            &event,
            "--diff",
            "-",
        ],
    ] {
        let output = run(&url, &args, Some(content)).await;
        assert!(output.status.success(), "{args:?}: {output:?}");
        let events = state.events.lock().unwrap();
        assert_eq!(events.last().unwrap().content, content);
    }
    assert_eq!(state.events.lock().unwrap().len(), 3);
    task.abort();
}

#[tokio::test]
async fn attachment_only_message_still_uploads_and_publishes() {
    let (url, state, task) = relay().await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.txt");
    std::fs::write(&path, "hello").unwrap();
    let output = run(
        &url,
        &[
            "messages",
            "send",
            "--channel",
            &Uuid::nil().to_string(),
            "--content",
            "",
            "--file",
            path.to_str().unwrap(),
        ],
        None,
    )
    .await;
    assert!(output.status.success(), "{output:?}");
    let events = state.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert!(events[0].content.contains("report.txt"));
    assert!(events[0]
        .tags
        .iter()
        .any(|tag| tag.as_slice()[0] == "imeta"));
    task.abort();
}
