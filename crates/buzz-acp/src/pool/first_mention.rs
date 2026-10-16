//! Cold start: find the first message in a thread that mentions the agent,
//! so a new provider session always sees why it was called in, even when
//! that message is older than the recent-message window.
//!
//! The scan walks the thread forward from its start through the relay's
//! thread pagination (`depth_limit` filter with a composite
//! `(created_at, id)` cursor), so it makes progress across replies that share
//! one second and stops at the first match. It is bounded; an incomplete scan
//! never reports a mention as found.

use super::{json_to_context_message, ConversationContext, PromptContext, CONTEXT_FETCH_TIMEOUT};
use buzz_core::kind::{KIND_STREAM_MESSAGE, KIND_STREAM_MESSAGE_V2};
use tokio::time::timeout;
use uuid::Uuid;

/// Replies per page of the forward thread scan.
pub(crate) const FIRST_MENTION_PAGE: usize = 500;
/// Pages scanned before giving up.
pub(crate) const FIRST_MENTION_MAX_PAGES: usize = 20;
/// Any real thread depth; the filter field selects forward thread paging.
const THREAD_DEPTH_LIMIT: u32 = 1_000_000;

#[derive(Debug)]
pub(crate) enum FirstMention {
    /// The first reply that mentions the agent.
    Found(serde_json::Value),
    /// The thread root mentions the agent; it is always in context.
    InRoot,
    /// The whole thread was scanned and nothing mentions the agent.
    NotMentioned,
    /// The scan failed or ran out of budget before reaching a mention.
    NotFound,
}

/// Scan `root_event_id`'s thread from the start for the first mention of
/// `agent`. `query` posts raw filters to the relay's `/query`.
pub(crate) async fn find_first_mention<Query, QueryFut>(
    root_event_id: &str,
    channel_id: Uuid,
    agent: nostr::PublicKey,
    query: Query,
) -> FirstMention
where
    Query: Fn(Vec<serde_json::Value>) -> QueryFut,
    QueryFut: std::future::Future<Output = Result<serde_json::Value, crate::relay::RelayError>>,
{
    let agent_hex = agent.to_hex();
    let channel = channel_id.to_string();
    let mut cursor: Option<(u64, String)> = None;
    for page in 0..FIRST_MENTION_MAX_PAGES {
        let mut filter = serde_json::json!({
            "kinds": [KIND_STREAM_MESSAGE, KIND_STREAM_MESSAGE_V2],
            "#e": [root_event_id],
            "#h": [channel],
            "depth_limit": THREAD_DEPTH_LIMIT,
            "limit": FIRST_MENTION_PAGE,
        });
        if let Some((created_at, id)) = &cursor {
            filter["thread_cursor"] = serde_json::json!(created_at);
            filter["thread_cursor_id"] = serde_json::json!(id);
        }
        let mut request = vec![filter];
        if page == 0 {
            request.push(serde_json::json!({
                "ids": [root_event_id],
                "#h": [channel],
                "limit": 1,
            }));
        }
        let events = match timeout(CONTEXT_FETCH_TIMEOUT, query(request)).await {
            Ok(Ok(serde_json::Value::Array(events))) => events,
            _ => {
                tracing::warn!(
                    channel_id = %channel_id,
                    root = root_event_id,
                    "first-mention scan failed; cold start proceeds without it"
                );
                return FirstMention::NotFound;
            }
        };
        let (roots, mut replies): (Vec<_>, Vec<_>) = events
            .into_iter()
            .partition(|event| event["id"].as_str() == Some(root_event_id));
        if roots.iter().any(|root| mentions(root, &agent_hex)) {
            return FirstMention::InRoot;
        }
        replies.sort_by(|a, b| {
            (a["created_at"].as_u64(), a["id"].as_str())
                .cmp(&(b["created_at"].as_u64(), b["id"].as_str()))
        });
        if let Some(mention) = replies
            .iter()
            .find(|event| is_message(event) && mentions(event, &agent_hex))
        {
            return FirstMention::Found(mention.clone());
        }
        let Some(last) = replies.last().filter(|_| replies.len() >= FIRST_MENTION_PAGE) else {
            return FirstMention::NotMentioned;
        };
        let (Some(created_at), Some(id)) = (last["created_at"].as_u64(), last["id"].as_str())
        else {
            return FirstMention::NotFound;
        };
        cursor = Some((created_at, id.to_owned()));
    }
    tracing::warn!(
        channel_id = %channel_id,
        root = root_event_id,
        pages = FIRST_MENTION_MAX_PAGES,
        "first mention not found within the scan budget"
    );
    FirstMention::NotFound
}

/// Find the first mention of the agent in `root`'s thread and pin it into
/// the fetched `context`.
pub(super) async fn cold_start_pin(
    root: &str,
    channel_id: Uuid,
    context: &mut ConversationContext,
    ctx: &PromptContext,
) -> FirstMention {
    let agent = ctx.agent_keys.public_key();
    let rest = &ctx.rest_client;
    let mention = find_first_mention(root, channel_id, agent, |filters| async move {
        rest.query_raw(&filters).await
    })
    .await;
    if let FirstMention::Found(event) = &mention {
        pin_first_mention(context, event, ctx.context_message_limit, &agent.to_hex());
    }
    mention
}

fn is_message(event: &serde_json::Value) -> bool {
    event["kind"].as_u64().is_some_and(|kind| {
        kind == u64::from(KIND_STREAM_MESSAGE) || kind == u64::from(KIND_STREAM_MESSAGE_V2)
    })
}

fn mentions(event: &serde_json::Value, agent_hex: &str) -> bool {
    event["tags"].as_array().is_some_and(|tags| {
        tags.iter().any(|tag| {
            tag[0].as_str() == Some("p")
                && tag[1]
                    .as_str()
                    .is_some_and(|pubkey| pubkey.eq_ignore_ascii_case(agent_hex))
        })
    })
}

/// Put the first mention into a thread window that lacks it, right after the
/// root. The window keeps at most `limit` replies: the oldest reply not
/// written by the agent makes room, so the agent's own pinned reply stays.
pub(crate) fn pin_first_mention(
    context: &mut ConversationContext,
    mention: &serde_json::Value,
    limit: u32,
    agent_hex: &str,
) {
    let ConversationContext::Thread {
        messages,
        root_present,
        ..
    } = context
    else {
        return;
    };
    let Some(id) = mention["id"].as_str() else {
        return;
    };
    if messages.iter().any(|message| message.event_id == id) {
        return;
    }
    let Some(message) = json_to_context_message(mention) else {
        return;
    };
    let first_reply = usize::from(*root_present);
    if messages.len() - first_reply >= limit as usize {
        let victim = messages[first_reply..]
            .iter()
            .position(|message| !message.pubkey.eq_ignore_ascii_case(agent_hex));
        match victim {
            Some(offset) => {
                messages.remove(first_reply + offset);
            }
            None => return,
        }
    }
    messages.insert(first_reply, message);
}

#[cfg(test)]
#[path = "first_mention_tests.rs"]
mod tests;
