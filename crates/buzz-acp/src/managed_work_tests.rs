use super::*;
use crate::{
    config::DedupMode,
    queue::{EventQueue, QueuedEvent, RecoveryPolicy},
    scope::SessionScope,
};
use nostr::{EventBuilder, Keys, Kind, Tag};

pub(crate) fn confirmed_journal(
    rest: &RestClient,
    work: &LogicalWork,
    content: &str,
) -> (Vec<u8>, String) {
    let parent = parent(work).unwrap_or_default();
    let channel = work.scope.channel_id().to_string();
    let key = work.delivery_key.to_string();
    let signer = rest.keys.public_key().to_hex();
    let relay = rest.base_url.trim_end_matches('/');
    let mut tags = vec![
        Tag::parse(["h", &channel]).unwrap(),
        Tag::parse(["buzz-delivery", &key]).unwrap(),
    ];
    if !parent.is_empty() {
        tags.push(Tag::parse(["e", &parent, "", "reply"]).unwrap());
    }
    let report = EventBuilder::new(Kind::Custom(9), content)
        .tags(tags)
        .sign_with_keys(&rest.keys)
        .unwrap();
    let event = report.id.to_hex();
    let namespace = hex::encode(Sha256::digest(
        serde_json::to_vec(&(relay, &signer, &key)).unwrap(),
    ));
    let record = serde_json::json!({"relay":relay,"signer":signer,"key":key,"channel":channel,
        "parent":parent,"event":report,"attempts":1,"confirmed_at":nostr::Timestamp::now().as_secs(),"last_error":null});
    (
        serde_json::to_vec(&serde_json::json!({"version":2,"entries":{namespace:record}})).unwrap(),
        event,
    )
}

fn fixture(dir: &Path) -> (RestClient, LogicalWork) {
    let scope = SessionScope::Conversation {
        channel_id: uuid::Uuid::new_v4(),
    };
    let keys = Keys::generate();
    let rest = RestClient {
        http: reqwest::Client::new(),
        base_url: "http://relay.example/path".into(),
        keys: keys.clone(),
        auth_tag_json: None,
    };
    let mut queue = EventQueue::new(DedupMode::Queue)
        .with_kimi_recovery(RecoveryPolicy::default(), dir.to_str().unwrap());
    queue.push(QueuedEvent {
        channel_id: scope.channel_id(),
        scope,
        event: EventBuilder::new(Kind::Custom(9), "original")
            .sign_with_keys(&keys)
            .unwrap(),
        prompt_tag: "mention".into(),
        edit: None,
        received_at: std::time::Instant::now(),
    });
    let mut work = queue.flush_next().unwrap().work.unwrap();
    work.journal_dir = dir.to_owned();
    (rest, work)
}

#[tokio::test]
async fn kimi_receipt_uses_the_cli_normalized_namespace() {
    let dir = tempfile::tempdir().unwrap();
    let (mut rest, work) = fixture(dir.path());
    let (journal, event) = confirmed_journal(&rest, &work, "first report");
    std::fs::write(dir.path().join("journal.json"), journal).unwrap();
    rest.base_url = "HTTP://RELAY.EXAMPLE:80/path/".into();
    assert_eq!(confirmed_report(&rest, &work).await.unwrap(), Some(event));
}

#[tokio::test]
async fn kimi_receipt_rejects_changed_route_and_corrupted_signed_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let (rest, mut work) = fixture(dir.path());
    let (journal, _) = confirmed_journal(&rest, &work, "first report");
    std::fs::write(dir.path().join("journal.json"), &journal).unwrap();
    let saved = work.thread_tags.clone();
    work.thread_tags.root_event_id = Some("b".repeat(64));
    assert!(confirmed_report(&rest, &work).await.is_err());
    work.thread_tags = saved;
    let mut value: serde_json::Value = serde_json::from_slice(&journal).unwrap();
    for record in value["entries"].as_object_mut().unwrap().values_mut() {
        record["event"]["content"] = "changed without a signature".into();
    }
    std::fs::write(
        dir.path().join("journal.json"),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
    assert!(confirmed_report(&rest, &work).await.is_err());
}
