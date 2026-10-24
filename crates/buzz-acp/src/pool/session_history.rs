//! Selected restart history. Request/decode/retained-render budgets are separate.
use super::{first_mention::FirstMention, json_to_context_message, ContextTarget, PromptContext};
use crate::queue::{ContextMessage, ConversationContext, PromptProfileLookup};
use buzz_core::session_control::RestartDepth;
use serde_json::{json, Value};
use std::collections::{HashSet, VecDeque};
use tokio::time::{Duration, Instant};
use uuid::Uuid;

pub(crate) const HISTORY_BYTES: usize = 256 * 1024;
const RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const PAGE: usize = 500;
const MAX_PAGES: usize = 20;
const MAX_CANDIDATES: usize = 10_000;

struct Reader<'a> {
    ctx: &'a PromptContext,
    deadline: Instant,
    requests: usize,
    candidates: usize,
    failed: bool,
}
impl Reader<'_> {
    async fn query(&mut self, filters: &[Value]) -> Option<Vec<Value>> {
        if self.requests >= MAX_PAGES || self.candidates >= MAX_CANDIDATES {
            self.failed = true;
            return None;
        }
        let requested = filters.iter().fold(0usize, |total, filter| {
            total.saturating_add(
                filter["limit"]
                    .as_u64()
                    .and_then(|limit| usize::try_from(limit).ok())
                    .unwrap_or(MAX_CANDIDATES),
            )
        });
        if requested > MAX_CANDIDATES.saturating_sub(self.candidates) {
            self.failed = true;
            return None;
        }
        let mut response = None;
        for attempt in 0..2 {
            if self.requests >= MAX_PAGES || Instant::now() >= self.deadline {
                break;
            }
            self.requests += 1;
            let started = Instant::now();
            let deadline = self.deadline.min(started + Duration::from_secs(5));
            match self
                .ctx
                .rest_client
                .query_raw_bounded_until(filters, RESPONSE_BYTES, deadline)
                .await
            {
                Ok(events) => {
                    response = Some(events);
                    break;
                }
                Err(error) => {
                    tracing::warn!(%error, attempt, elapsed_ms = started.elapsed().as_millis(),
                        "selected history request failed");
                }
            }
            if attempt == 0 {
                tokio::time::sleep_until(
                    self.deadline
                        .min(Instant::now() + Duration::from_millis(250)),
                )
                .await;
            }
        }
        let Some(response) = response else {
            self.failed = true;
            return None;
        };
        let Value::Array(mut events) = response else {
            self.failed = true;
            return None;
        };
        // A relay ignoring its requested limit cannot bypass the raw-candidate bound.
        let room = MAX_CANDIDATES - self.candidates;
        if events.len() > requested || events.len() > room {
            self.failed = true;
            return None;
        }
        self.candidates += events.len();
        events.shrink_to_fit();
        Some(events)
    }
}

fn key(event: &Value) -> Option<(u64, &str)> {
    Some((event["created_at"].as_u64()?, event["id"].as_str()?))
}
fn mentions(event: &Value, agent: &str) -> bool {
    event["tags"]
        .as_array()
        .is_some_and(|tags| tags.iter().any(|tag| tag[0] == "p" && tag[1] == agent))
}
fn root_message_id_missing(seen: &HashSet<String>, root: Option<&str>, present: bool) -> bool {
    present && root.is_some_and(|id| !seen.contains(id))
}
fn cost(message: &ContextMessage) -> usize {
    // Charge the message body once, rather than repeating the section wrapper
    // per message. Five bytes cover a separator and index growth through 10000.
    let singleton = ConversationContext::Dm {
        messages: vec![message.clone()],
        total: MAX_CANDIDATES,
        truncated: true,
    };
    let empty = ConversationContext::Dm {
        messages: Vec::new(),
        total: MAX_CANDIDATES,
        truncated: true,
    };
    let rendered = crate::queue::format_conversation_context(&singleton, None);
    let empty = crate::queue::format_conversation_context(&empty, None);
    // The included attribute grows from 0 to 1, so both wrappers have equal size.
    rendered.len().saturating_sub(empty.len()) + 5
}
// Working retention can overestimate each message by at most five bytes. Keep
// that bounded slack so every history fitting the actual renderer survives the
// scan; final rendering still enforces the strict 256 KiB limit.
const RETAINED_HEAD_BYTES: usize = HISTORY_BYTES + MAX_CANDIDATES * 5;
fn retain_tail(tail: &mut VecDeque<ContextMessage>, message: ContextMessage, bytes: &mut usize) {
    let size = cost(&message);
    if size > HISTORY_BYTES / 2 {
        return;
    }
    while *bytes + size > HISTORY_BYTES / 2 {
        if let Some(old) = tail.pop_front() {
            *bytes -= cost(&old);
        } else {
            break;
        }
    }
    *bytes += size;
    tail.push_back(message);
}

/// Selected depths follow the current target thread even for a top-level
/// trigger under channel policy; DMs always use the whole conversation.
pub(super) fn target(batch: &super::FlushBatch, is_dm: bool) -> ContextTarget {
    if is_dm {
        return ContextTarget::Dm;
    }
    match super::resolve_context_target(batch, false) {
        ContextTarget::None => batch.events.last().map_or(ContextTarget::None, |event| {
            ContextTarget::Thread(event.reply_thread())
        }),
        target => target,
    }
}

/// None still searches for the original assignment when context_message_limit=0.
/// All walks only the target thread, or the DM conversation, never the channel.
pub(super) async fn selected(
    target: &ContextTarget,
    channel: Uuid,
    depth: RestartDepth,
    ctx: &PromptContext,
) -> (Option<ConversationContext>, Option<FirstMention>) {
    let (context, mention, _, _) =
        selected_with_language(target, channel, depth, ctx, &HashSet::new()).await;
    (context, mention)
}

pub(super) async fn selected_with_language(
    target: &ContextTarget,
    channel: Uuid,
    depth: RestartDepth,
    ctx: &PromptContext,
    humans: &HashSet<String>,
) -> (
    Option<ConversationContext>,
    Option<FirstMention>,
    &'static str,
    bool,
) {
    let mut language = super::status_language::Evidence::default();
    if *target == ContextTarget::None {
        return (None, Some(FirstMention::NotFound), "unknown", false);
    }
    let mut reader = Reader {
        ctx,
        deadline: Instant::now() + Duration::from_secs(10),
        requests: 0,
        candidates: 0,
        failed: false,
    };
    let agent = ctx.agent_keys.public_key().to_hex();
    let root = match target {
        ContextTarget::Thread(root) => Some(root.as_str()),
        _ => None,
    };
    let mut base = json!({"kinds":[9,40002],"#h":[channel.to_string()],"limit":PAGE});
    if let Some(root) = root {
        base["#e"] = json!([root]);
    }
    let mut head = Vec::new();
    let mut tail = VecDeque::new();
    let mut head_bytes = 0;
    let mut tail_bytes = 0;
    let mut root_message = None;
    let mut assignment: Option<(Value, ContextMessage)> = None;
    let mut seen = HashSet::new();
    let mut incomplete = false;
    let mut complete = false;
    // Reserve the newest tail before the forward scan can exhaust the deadline.
    // Root lookup shares this request and all global budgets.
    // Preserve the bounded newest page for language evidence as well: many
    // agent replies may hide the last human message outside the rendered window.
    // Recent retains only its configured window from this same acquisition.
    let mut newest = base.clone();
    let mut initial = vec![newest.clone()];
    if let Some(root) = root {
        initial.push(json!({"kinds":[9,40002],"#h":[channel.to_string()],"ids":[root],"limit":1}));
    }
    if root.is_some() && depth == RestartDepth::Recent && ctx.context_message_limit > 0 {
        newest["authors"] = json!([agent]);
        newest["limit"] = json!(1);
        initial.push(newest);
    }
    let initial = reader.query(&initial).await;
    let initial_count = initial.as_ref().map_or(0, |events| {
        events
            .iter()
            .filter_map(json_to_context_message)
            .map(|message| message.event_id)
            .collect::<HashSet<_>>()
            .len()
    });
    let recent = initial
        .as_ref()
        .filter(|_| depth == RestartDepth::Recent && ctx.context_message_limit > 0)
        .and_then(|events| {
            if let Some(root) = root {
                super::parse_nostr_thread_response_with_meta(
                    json!(events),
                    root,
                    ctx.context_message_limit,
                    &ctx.agent_keys.public_key(),
                    false,
                    ctx.context_message_limit,
                    &HashSet::new(),
                )
                .map(|parsed| parsed.context)
            } else {
                let mut events = events.clone();
                events.sort_by(|a, b| key(b).cmp(&key(a)));
                events.truncate(ctx.context_message_limit as usize);
                super::parse_nostr_dm_response(json!(events), ctx.context_message_limit)
            }
        });
    if let Some(events) = &initial {
        let mut events = events.iter().collect::<Vec<_>>();
        events.sort_by(|a, b| key(a).cmp(&key(b)));
        for event in events {
            language.observe(event, humans, channel, target);
            let Some(message) = json_to_context_message(event) else {
                continue;
            };
            if Some(message.event_id.as_str()) == root {
                if mentions(event, &agent) {
                    assignment = Some((event.clone(), message.clone()));
                }
                root_message = Some(message.clone());
            }
            if depth == RestartDepth::All {
                retain_tail(&mut tail, message, &mut tail_bytes);
            }
        }
    } else {
        incomplete = true;
    }
    // Threads have a forward composite cursor. DMs use the ordinary descending
    // composite cursor; only exhaustion proves the earliest assignment there.
    let mut cursor: Option<(u64, String)> = None;
    for page in 0..MAX_PAGES {
        if depth == RestartDepth::Recent && root.is_some() && assignment.is_some() {
            complete = true;
            break;
        }
        let mut filter = base.clone();
        if root.is_some() {
            // Start small: the original assignment usually precedes the recent
            // window. Later pages retain the full bounded scan capacity.
            if depth == RestartDepth::Recent && page == 0 {
                filter["limit"] = json!(64);
            }
            filter["depth_limit"] = json!(1_000_000);
            if let Some((at, id)) = &cursor {
                filter["thread_cursor"] = json!(at);
                filter["thread_cursor_id"] = json!(id);
            }
        } else if let Some((at, id)) = &cursor {
            filter["until"] = json!(at);
            filter["before_id"] = json!(id);
        }
        let page_limit = filter["limit"].as_u64().unwrap_or(PAGE as u64) as usize;
        let events = if page == 0 && root.is_none() {
            initial.clone()
        } else {
            reader.query(&[filter]).await
        };
        let Some(mut events) = events else {
            incomplete = true;
            break;
        };
        if root.is_none() {
            // Ordinary relay pages use time DESC, id ASC. Reverse only time:
            // equal-second continuation requires id > before_id.
            events.sort_by(|a, b| {
                b["created_at"]
                    .as_u64()
                    .cmp(&a["created_at"].as_u64())
                    .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
            });
        } else {
            events.sort_by(|a, b| key(a).cmp(&key(b)));
        }
        let next = events
            .last()
            .and_then(key)
            .map(|(at, id)| (at, id.to_owned()));
        let short = events.len() < page_limit;
        for event in events {
            language.observe(&event, humans, channel, target);
            let Some(message) = json_to_context_message(&event) else {
                continue;
            };
            if message.event_id.is_empty() || !seen.insert(message.event_id.clone()) {
                continue;
            }
            // DM pagination order differs from chronological assignment order.
            // Keep the minimum time/id independently, including ties on a page.
            let replaces_assignment = assignment
                .as_ref()
                .is_none_or(|(prior, _)| root.is_none() && key(&event) < key(prior));
            if mentions(&event, &agent) && replaces_assignment {
                assignment = Some((event.clone(), message.clone()));
            }
            if depth == RestartDepth::All {
                let size = cost(&message);
                if root.is_some() {
                    if head_bytes + size <= RETAINED_HEAD_BYTES {
                        head_bytes += size;
                        head.push(message);
                    } else {
                        incomplete = true;
                    }
                } else {
                    // Descending DM pages: keep the newest prefix and roll the
                    // other half toward the oldest messages without accumulation.
                    if head_bytes + size <= RETAINED_HEAD_BYTES {
                        head_bytes += size;
                        head.push(message.clone());
                    }
                    retain_tail(&mut tail, message, &mut tail_bytes);
                }
            }
        }
        if depth != RestartDepth::All && root.is_some() && assignment.is_some() {
            complete = true;
            break;
        }
        if short {
            complete = true;
            break;
        }
        if next.is_none() || next == cursor {
            incomplete = true;
            break;
        }
        cursor = next;
    }
    if !complete {
        incomplete = true;
    }
    if root.is_none() && !complete {
        assignment = None;
    }
    if assignment
        .as_ref()
        .is_some_and(|(_, message)| cost(message) >= HISTORY_BYTES)
    {
        assignment = None;
        incomplete = true;
        complete = false;
    }
    let mention = assignment
        .as_ref()
        .map(|(event, message)| {
            if Some(message.event_id.as_str()) == root {
                FirstMention::InRoot
            } else {
                FirstMention::Found(event.clone())
            }
        })
        .unwrap_or(if complete && depth != RestartDepth::None {
            FirstMention::NotMentioned
        } else {
            FirstMention::NotFound
        });
    let root_observed = root_message.is_some();
    let mut messages = Vec::new();
    // Assignment wins over history when an individual body is too large.
    if let Some((_, assignment)) = assignment {
        if cost(&assignment) < HISTORY_BYTES {
            messages.push(assignment);
        } else {
            incomplete = true;
        }
    }
    if depth == RestartDepth::All {
        if let Some(root) = root_message {
            if cost(&root) < HISTORY_BYTES {
                messages.push(root);
            } else {
                incomplete = true;
            }
        }
        messages.extend(head);
        messages.extend(tail);
    }
    let mut recent_total = 0;
    let mut recent_truncated = false;
    if let Some(
        ConversationContext::Thread {
            messages: tail,
            total,
            truncated,
            ..
        }
        | ConversationContext::Dm {
            messages: tail,
            total,
            truncated,
        },
    ) = recent
    {
        messages.extend(tail);
        recent_total = total;
        recent_truncated = truncated;
    }
    let mut retained = HashSet::new();
    messages.retain(|message| retained.insert(message.event_id.clone()));
    // Timestamp RFC3339 strings sort chronologically for relay integer dates.
    messages.sort_by(|a, b| (&a.timestamp, &a.event_id).cmp(&(&b.timestamp, &b.event_id)));
    if let Some(position) =
        root.and_then(|root| messages.iter().position(|message| message.event_id == root))
    {
        let root = messages.remove(position);
        messages.insert(0, root);
    }
    let root_present =
        root.is_some_and(|root| messages.iter().any(|message| message.event_id == root));
    let total = (seen.len() + usize::from(root_message_id_missing(&seen, root, root_observed)))
        .max(messages.len())
        .max(initial_count)
        .max(recent_total);
    // A recent window is a lower bound; assignment discovery may stop before
    // proving exhaustion of nested replies outside that window.
    let truncated =
        incomplete || depth == RestartDepth::Recent || recent_truncated || total > messages.len();
    let context = if root.is_some() {
        ConversationContext::Thread {
            messages,
            total,
            root_present,
            truncated,
        }
    } else {
        ConversationContext::Dm {
            messages,
            total,
            truncated,
        }
    };
    let mut pins = HashSet::new();
    match &mention {
        FirstMention::Found(event) => {
            if let Some(id) = event["id"].as_str() {
                pins.insert(id.to_owned());
            }
        }
        FirstMention::InRoot => {
            if let Some(root) = root {
                pins.insert(root.to_owned());
            }
        }
        _ => {}
    }
    if let Some(root) = root {
        pins.insert(root.to_owned());
    }
    let mut context = Some(context);
    enforce_render_budget(&mut context, None, &pins);
    (context, Some(mention), language.language(), reader.failed)
}

/// Reconcile assignment discovery with the bodies actually supplied after
/// profile expansion and delta filtering. An assignment in the trigger batch
/// remains available even when removed from the history section.
pub(super) fn supplied_mention(
    mention: Option<FirstMention>,
    target: &ContextTarget,
    context: Option<&ConversationContext>,
    batch_ids: &HashSet<String>,
) -> Option<FirstMention> {
    let id = match mention.as_ref() {
        Some(FirstMention::Found(event)) => event["id"].as_str(),
        Some(FirstMention::InRoot) => match target {
            ContextTarget::Thread(root) => Some(root.as_str()),
            _ => None,
        },
        _ => return mention,
    };
    let supplied = id.is_some_and(|id| {
        batch_ids.contains(id)
            || context.is_some_and(|context| {
                let messages = match context {
                    ConversationContext::Thread { messages, .. }
                    | ConversationContext::Dm { messages, .. } => messages,
                };
                messages.iter().any(|message| message.event_id == id)
            })
    });
    if supplied {
        mention
    } else {
        Some(FirstMention::NotFound)
    }
}

/// Apply the final rendered UTF-8 budget after actor profiles and delta filtering.
pub(super) fn enforce_render_budget(
    context: &mut Option<ConversationContext>,
    profiles: Option<&PromptProfileLookup>,
    pins: &HashSet<String>,
) {
    let Some(context) = context else {
        return;
    };
    let root_id = match context {
        ConversationContext::Thread {
            root_present: true,
            messages,
            ..
        } => messages.first().map(|message| message.event_id.clone()),
        _ => None,
    };
    while crate::queue::format_conversation_context(context, profiles).len() > HISTORY_BYTES {
        let messages = match context {
            ConversationContext::Thread {
                messages,
                truncated,
                ..
            }
            | ConversationContext::Dm {
                messages,
                truncated,
                ..
            } => {
                *truncated = true;
                messages
            }
        };
        if messages.is_empty() {
            break;
        }
        // Preserve bounded head+tail; never truncate a body and count it supplied.
        let victim = (0..messages.len()).min_by_key(|index| {
            (
                if root_id.as_deref() == Some(messages[*index].event_id.as_str()) {
                    1
                } else if pins.contains(&messages[*index].event_id) {
                    2
                } else {
                    0
                },
                index.abs_diff(messages.len() / 2),
            )
        });
        if let Some(victim) = victim {
            messages.remove(victim);
        }
    }
    if let ConversationContext::Thread {
        root_present,
        messages,
        ..
    } = context
    {
        *root_present =
            root_id.is_some_and(|id| messages.iter().any(|message| message.event_id == id));
    }
}

#[cfg(test)]
#[path = "session_history_tests.rs"]
mod tests;
