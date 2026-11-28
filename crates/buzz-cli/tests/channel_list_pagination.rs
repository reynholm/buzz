use std::{
    process::Command,
    sync::{Arc, Mutex},
};

use axum::{extract::State, routing::post, Json, Router};
use serde_json::{json, Value};
use tokio::net::TcpListener;

const MEMBERSHIP_TIME: u64 = 42;
const PAGE_SIZE: usize = 500;

fn event_id(index: usize) -> String {
    format!("{index:064x}")
}

fn channel_id(index: usize) -> String {
    format!("11111111-1111-4111-8111-{index:012x}")
}

#[derive(Clone)]
struct Relay {
    memberships: usize,
    open_channels: Vec<usize>,
    duplicate_membership: Option<(usize, usize)>,
    filters: Arc<Mutex<Vec<Value>>>,
}

impl Relay {
    fn new(memberships: usize, open_channels: Vec<usize>) -> Self {
        Self {
            memberships,
            open_channels,
            duplicate_membership: None,
            filters: Arc::default(),
        }
    }

    fn query(&self, filter: Value) -> Vec<Value> {
        self.filters.lock().unwrap().push(filter.clone());
        let limit = filter["limit"].as_u64().unwrap() as usize;
        assert!(limit <= PAGE_SIZE);
        match filter["kinds"][0].as_u64().unwrap() {
            39002 => {
                // Every membership has the same timestamp, so the cursor must
                // advance by event ID within that second.
                let offset = filter["before_id"].as_str().map_or(0, |id| {
                    assert_eq!(filter["until"], MEMBERSHIP_TIME);
                    usize::from_str_radix(id, 16).unwrap() + 1
                });
                (offset..self.memberships.min(offset + limit))
                    .map(|index| {
                        let channel = match self.duplicate_membership {
                            Some((position, original)) if index == position => original,
                            _ => index,
                        };
                        json!({
                            "id": event_id(index),
                            "created_at": MEMBERSHIP_TIME,
                            "tags": [["d", channel_id(channel)]],
                        })
                    })
                    .collect()
            }
            39000 => {
                let mut channels: Vec<_> = filter["#d"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| {
                        let (_, index) = id.as_str().unwrap().rsplit_once('-').unwrap();
                        usize::from_str_radix(index, 16).unwrap()
                    })
                    .collect();
                // The relay orders metadata newest first, independent of the
                // membership cursor or order of IDs in the filter.
                channels.sort_unstable_by(|a, b| b.cmp(a));
                channels.dedup();
                channels
                    .into_iter()
                    .take(limit)
                    .map(|index| {
                        let visibility = if self.open_channels.contains(&index) {
                            "public"
                        } else {
                            "private"
                        };
                        json!({
                            "id": event_id(index),
                            "created_at": index,
                            "tags": [["d", channel_id(index)], ["name", "channel"], [visibility]],
                        })
                    })
                    .collect()
            }
            kind => panic!("unexpected query kind {kind}"),
        }
    }
}

async fn list_open_memberships(relay: Relay, limit: &str) -> (Value, Vec<Value>) {
    let filters = relay.filters.clone();
    let app = Router::new()
        .route(
            "/query",
            post(
                |State(relay): State<Relay>, Json(filters): Json<Vec<Value>>| async move {
                    let [filter]: [Value; 1] = filters.try_into().unwrap();
                    Json(relay.query(filter))
                },
            ),
        )
        .with_state(relay);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let key = nostr::Keys::generate().secret_key().to_secret_hex();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_buzz"));
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("BUZZ_") {
            cmd.env_remove(name);
        }
    }
    cmd.args([
        "--relay",
        &url,
        "--private-key",
        &key,
        "channels",
        "list",
        "--member",
        "--visibility",
        "open",
        "--limit",
        limit,
    ]);
    let output = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let channels = serde_json::from_slice(&output.stdout).unwrap();
    let filters = filters.lock().unwrap().clone();
    (channels, filters)
}

#[tokio::test(flavor = "multi_thread")]
async fn filtered_member_list_stops_after_the_first_matching_page() {
    let (channels, filters) = list_open_memberships(Relay::new(501, vec![0, 1]), "1").await;
    assert_eq!(channels.as_array().unwrap().len(), 1);
    assert_eq!(channels[0]["channel_id"], channel_id(1));
    assert_eq!(channels[0]["visibility"], "public");
    assert_eq!(channels[0]["created_at"], 1);
    assert_eq!(
        filters.len(),
        2,
        "one membership query and one metadata query"
    );
    assert_eq!(filters[0]["kinds"], json!([39002]));
    assert_eq!(filters[1]["kinds"], json!([39000]));
}

#[tokio::test(flavor = "multi_thread")]
async fn filtered_member_list_pages_past_ten_thousand_same_second_memberships() {
    let mut relay = Relay::new(11_001, vec![10_000, 11_000]);
    // Encountering an already returned channel on a later page must not
    // satisfy the requested count.
    relay.duplicate_membership = Some((10_500, 10_000));
    let (channels, filters) = list_open_memberships(relay, "2").await;
    let ids: Vec<_> = channels
        .as_array()
        .unwrap()
        .iter()
        .map(|channel| channel["channel_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [channel_id(10_000), channel_id(11_000)]);
    assert_eq!(filters.len(), 46);
    for (index, pair) in filters.chunks_exact(2).enumerate() {
        assert_eq!(pair[0]["kinds"], json!([39002]));
        assert_eq!(pair[1]["kinds"], json!([39000]));
        if index > 0 {
            assert_eq!(pair[0]["before_id"], event_id(index * PAGE_SIZE - 1));
            assert_eq!(pair[0]["until"], MEMBERSHIP_TIME);
        }
    }
}
