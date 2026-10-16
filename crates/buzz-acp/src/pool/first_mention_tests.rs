use super::*;
use nostr::{EventBuilder, Keys, Kind, Tag, Timestamp};
use std::sync::Mutex;

const CHANNEL: Uuid = Uuid::from_u128(42);

struct Thread {
    root: serde_json::Value,
    /// Ascending by `(created_at, id)`, as the relay pages them.
    replies: Vec<serde_json::Value>,
}

fn event(
    keys: &Keys,
    created_at: u64,
    root: Option<&str>,
    mention: Option<&Keys>,
    text: &str,
) -> serde_json::Value {
    let mut tags = vec![Tag::parse(["h", CHANNEL.to_string().as_str()]).unwrap()];
    if let Some(root) = root {
        tags.push(Tag::parse(["e", root, "", "reply"]).unwrap());
    }
    if let Some(agent) = mention {
        tags.push(Tag::parse(["p", agent.public_key().to_hex().as_str()]).unwrap());
    }
    let event = EventBuilder::new(Kind::Custom(9), text)
        .tags(tags)
        .custom_created_at(Timestamp::from(created_at))
        .sign_with_keys(keys)
        .unwrap();
    serde_json::to_value(event).unwrap()
}

/// `count` replies; reply `mention_at` mentions `agent`. With `same_second`
/// every reply shares one `created_at`.
fn thread(agent: &Keys, count: usize, mention_at: Option<usize>, same_second: bool) -> Thread {
    thread_with_root(agent, count, mention_at, same_second, false)
}

fn thread_with_root(
    agent: &Keys,
    count: usize,
    mention_at: Option<usize>,
    same_second: bool,
    root_mentions: bool,
) -> Thread {
    let human = Keys::generate();
    let root = event(&human, 1_000, None, root_mentions.then_some(agent), "root");
    let root_id = root["id"].as_str().unwrap().to_owned();
    let mut replies: Vec<_> = (0..count)
        .map(|n| {
            let at = if same_second { 2_000 } else { 2_000 + n as u64 };
            let mention = (mention_at == Some(n)).then_some(agent);
            event(&human, at, Some(&root_id), mention, &format!("reply {n}"))
        })
        .collect();
    replies.sort_by(|a, b| {
        (a["created_at"].as_u64(), a["id"].as_str())
            .cmp(&(b["created_at"].as_u64(), b["id"].as_str()))
    });
    Thread { root, replies }
}

impl Thread {
    fn root_id(&self) -> String {
        self.root["id"].as_str().unwrap().to_owned()
    }

    /// Relay stand-in: the bridge's forward thread pagination for the thread
    /// filter, plus the root for an `ids` filter in the same request.
    fn answer(&self, filters: &[serde_json::Value]) -> serde_json::Value {
        let mut events = Vec::new();
        for filter in filters {
            if filter.get("ids").is_some() {
                events.push(self.root.clone());
            } else if let serde_json::Value::Array(page) = self.page(filter) {
                events.extend(page);
            }
        }
        serde_json::Value::Array(events)
    }

    fn page(&self, filter: &serde_json::Value) -> serde_json::Value {
        let limit = filter["limit"].as_u64().unwrap() as usize;
        let cursor = filter["thread_cursor"].as_u64().map(|at| {
            (
                at,
                filter["thread_cursor_id"].as_str().unwrap_or("").to_owned(),
            )
        });
        let page: Vec<_> = self
            .replies
            .iter()
            .filter(|reply| match &cursor {
                None => true,
                Some((at, id)) => {
                    (
                        reply["created_at"].as_u64().unwrap(),
                        reply["id"].as_str().unwrap().to_owned(),
                    ) > (*at, id.clone())
                }
            })
            .take(limit)
            .cloned()
            .collect();
        serde_json::Value::Array(page)
    }

    fn mentioned_id(&self, n: usize) -> String {
        self.replies
            .iter()
            .find(|reply| reply["content"] == format!("reply {n}"))
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned()
    }
}

/// Returns the thread filter of every request.
async fn search(thread: &Thread, agent: &Keys) -> (FirstMention, Vec<serde_json::Value>) {
    let filters = Mutex::new(Vec::new());
    let found = find_first_mention(&thread.root_id(), CHANNEL, agent.public_key(), |request| {
        let thread_filter = request
            .iter()
            .find(|filter| filter.get("ids").is_none())
            .cloned()
            .unwrap();
        filters.lock().unwrap().push(thread_filter);
        let answer = thread.answer(&request);
        async move { Ok(answer) }
    })
    .await;
    (found, filters.into_inner().unwrap())
}

fn found_id(found: &FirstMention) -> String {
    match found {
        FirstMention::Found(event) => event["id"].as_str().unwrap().to_owned(),
        other => panic!("expected a mention, got {other:?}"),
    }
}

#[tokio::test]
async fn first_mention_found_on_second_page() {
    let agent = Keys::generate();
    let thread = thread(&agent, 600, Some(520), false);
    let (found, filters) = search(&thread, &agent).await;
    assert_eq!(found_id(&found), thread.mentioned_id(520));
    assert_eq!(filters.len(), 2);
    assert_eq!(filters[0]["#e"][0], thread.root_id());
    assert_eq!(filters[0]["#h"][0], CHANNEL.to_string());
    assert!(filters[0]["depth_limit"].is_u64());
    assert_eq!(filters[0]["limit"], FIRST_MENTION_PAGE);
    assert_eq!(
        filters[1]["thread_cursor"],
        thread.replies[499]["created_at"]
    );
    assert_eq!(filters[1]["thread_cursor_id"], thread.replies[499]["id"]);
}

#[tokio::test]
async fn same_second_page_boundary_advances() {
    // 600 replies in one second: a timestamp-only cursor would repeat page one.
    let agent = Keys::generate();
    let thread = thread(&agent, 600, None, true);
    let (found, filters) = search(&thread, &agent).await;
    assert!(matches!(found, FirstMention::NotMentioned));
    assert_eq!(filters.len(), 2);
    assert_eq!(
        filters[1]["thread_cursor"],
        thread.replies[499]["created_at"]
    );
    assert_eq!(filters[1]["thread_cursor_id"], thread.replies[499]["id"]);
}

#[tokio::test]
async fn same_second_mention_is_found() {
    let agent = Keys::generate();
    let thread = thread(&agent, 600, Some(550), true);
    let (found, _) = search(&thread, &agent).await;
    assert_eq!(found_id(&found), thread.mentioned_id(550));
}

#[tokio::test]
async fn budget_exhausted_reports_not_found() {
    let agent = Keys::generate();
    let pages = FIRST_MENTION_MAX_PAGES + 1;
    let thread = thread(&agent, pages * FIRST_MENTION_PAGE, None, false);
    let (found, filters) = search(&thread, &agent).await;
    assert!(matches!(found, FirstMention::NotFound));
    assert_eq!(filters.len(), FIRST_MENTION_MAX_PAGES);
}

#[tokio::test]
async fn root_mention_needs_no_pin() {
    let agent = Keys::generate();
    let thread = thread_with_root(&agent, 30, Some(3), false, true);
    let (found, filters) = search(&thread, &agent).await;
    assert!(matches!(found, FirstMention::InRoot));
    assert_eq!(filters.len(), 1);
}

#[tokio::test]
async fn short_page_without_mention_is_not_mentioned() {
    let agent = Keys::generate();
    let thread = thread(&agent, 30, None, false);
    let (found, filters) = search(&thread, &agent).await;
    assert!(matches!(found, FirstMention::NotMentioned));
    assert_eq!(filters.len(), 1);
}

fn window(thread: &Thread, newest: usize) -> ConversationContext {
    let start = thread.replies.len() - newest;
    let mut messages = vec![json_to_context_message(&thread.root).unwrap()];
    messages.extend(
        thread.replies[start..]
            .iter()
            .map(|reply| json_to_context_message(reply).unwrap()),
    );
    ConversationContext::Thread {
        messages,
        total: thread.replies.len() + 1,
        root_present: true,
        truncated: start > 0,
    }
}

fn ids(context: &ConversationContext) -> Vec<String> {
    match context {
        ConversationContext::Thread { messages, .. } => {
            messages.iter().map(|m| m.event_id.clone()).collect()
        }
        ConversationContext::Dm { .. } => unreachable!(),
    }
}

#[test]
fn thread_context_pins_first_mention_outside_window() {
    let agent = Keys::generate();
    let thread = thread(&agent, 30, Some(2), false);
    let mut context = window(&thread, 12);
    let mention = thread.replies[2].clone();
    pin_first_mention(&mut context, &mention, 12, &agent.public_key().to_hex());
    let ids = ids(&context);
    assert_eq!(ids.len(), 13, "root + 12 replies");
    assert_eq!(ids[0], thread.root_id());
    assert_eq!(
        ids[1],
        thread.mentioned_id(2),
        "pinned right after the root"
    );
    assert!(!ids.contains(&thread.replies[18]["id"].as_str().unwrap().to_owned()));
    let ConversationContext::Thread {
        truncated, total, ..
    } = context
    else {
        unreachable!()
    };
    assert!(truncated);
    assert_eq!(total, 31);
}

#[test]
fn first_mention_inside_window_is_not_duplicated() {
    let agent = Keys::generate();
    let thread = thread(&agent, 30, Some(25), false);
    let mut context = window(&thread, 12);
    let before = ids(&context);
    pin_first_mention(
        &mut context,
        &thread.replies[25].clone(),
        12,
        &agent.public_key().to_hex(),
    );
    assert_eq!(ids(&context), before);
}

#[test]
fn first_mention_equal_to_root_is_not_pinned() {
    let agent = Keys::generate();
    let thread = thread(&agent, 30, None, false);
    let mut context = window(&thread, 12);
    let before = ids(&context);
    pin_first_mention(
        &mut context,
        &thread.root.clone(),
        12,
        &agent.public_key().to_hex(),
    );
    assert_eq!(ids(&context), before);
}
