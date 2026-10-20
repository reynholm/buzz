//! Deliberately small language inference: Russian, English, or unknown.
//! Only already bounded, signed conversation evidence from a trusted human is
//! used. Code, quotes, operational controls and log-shaped lines carry no hint.
use super::{ContextTarget, PromptContext};
use serde_json::Value;
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Default)]
pub(super) struct Evidence {
    newest: Option<((u64, String), &'static str)>,
}
impl Evidence {
    pub(super) fn observe(
        &mut self,
        event: &Value,
        humans: &HashSet<String>,
        channel: Uuid,
        target: &ContextTarget,
    ) {
        if !event["pubkey"]
            .as_str()
            .is_some_and(|author| humans.contains(author))
        {
            return;
        }
        let Some(content) = event["content"].as_str() else {
            return;
        };
        let Some(language) = infer(content) else {
            return;
        };
        let key = (
            event["created_at"].as_u64().unwrap_or_default(),
            event["id"].as_str().unwrap_or_default().to_owned(),
        );
        if self.newest.as_ref().is_some_and(|(prior, _)| key <= *prior) {
            return;
        }
        let Ok(signed) = serde_json::from_value::<nostr::Event>(event.clone()) else {
            return;
        };
        if signed.verify().is_err() {
            return;
        }
        if !signed.tags.iter().any(|tag| {
            let parts = tag.as_slice();
            parts.first().is_some_and(|part| part == "h")
                && parts
                    .get(1)
                    .is_some_and(|part| part == &channel.to_string())
        }) {
            return;
        }
        if let ContextTarget::Thread(root) = target {
            if signed.id.to_hex() != *root
                && crate::queue::parse_thread_tags(&signed)
                    .root_event_id
                    .as_ref()
                    != Some(root)
            {
                return;
            }
        }
        if !matches!(signed.kind.as_u16(), 9 | 40002)
            || crate::session_controls::reserved_raw(event)
        {
            return;
        }
        if self.newest.as_ref().is_none_or(|(prior, _)| key > *prior) {
            self.newest = Some((key, language));
        }
    }
    pub(super) fn language(&self) -> &'static str {
        self.newest
            .as_ref()
            .map_or("unknown", |(_, language)| *language)
    }
}

// None means no prose evidence; substantive unsupported prose explicitly
// replaces older certainty with unknown.
pub(super) fn infer(text: &str) -> Option<&'static str> {
    let mut fenced = false;
    let mut ru = 0;
    let mut en = 0;
    let mut letters = 0;
    for line in text.lines().take(128) {
        let line = line.trim();
        if line.starts_with("```") || line.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced
            || log_shaped(line)
            || line.starts_with(['>', '!', '$', '{'])
            || ["[\"", "[{", "http://", "https://"]
                .iter()
                .any(|prefix| line.starts_with(*prefix))
        {
            continue;
        }
        let mut inline = false;
        let filtered: String = line
            .chars()
            .take(2048)
            .filter(|character| {
                if *character == '`' {
                    inline = !inline;
                    return false;
                }
                !inline
            })
            .collect();
        letters += filtered.chars().filter(|c| c.is_alphabetic()).count();
        ru += filtered
            .chars()
            .filter(|c| matches!(c, 'А'..='я' | 'Ё' | 'ё'))
            .count();
        let lower = filtered.to_ascii_lowercase();
        en += lower
            .split(|c: char| !c.is_ascii_alphabetic())
            .filter(|word| {
                matches!(
                    *word,
                    "the"
                        | "this"
                        | "that"
                        | "please"
                        | "is"
                        | "are"
                        | "we"
                        | "you"
                        | "and"
                        | "have"
                        | "has"
                        | "need"
                        | "done"
                        | "with"
                        | "what"
                        | "task"
                        | "can"
                        | "continue"
                        | "status"
                )
            })
            .count();
    }
    if ru >= 4 {
        Some("ru")
    } else if en >= 2 {
        Some("en")
    } else if letters >= 4 {
        Some("unknown")
    } else {
        None
    }
}

// Match timestamp/severity prefixes, not numbered or date-led progress prose.
fn log_shaped(line: &str) -> bool {
    fn severity(token: &str) -> bool {
        matches!(
            token.trim_matches(['[', ']']),
            "ERROR" | "WARN" | "INFO" | "DEBUG" | "TRACE" | "FATAL"
        )
    }
    fn clock(token: &str) -> bool {
        let bytes = token.trim_matches(['[', ']']).as_bytes();
        bytes.len() >= 8
            && bytes[2] == b':'
            && bytes[5] == b':'
            && bytes[..8]
                .iter()
                .enumerate()
                .all(|(i, b)| matches!(i, 2 | 5) || b.is_ascii_digit())
    }
    let mut words = line.split_whitespace();
    let first = words.next().unwrap_or_default().trim_matches(['[', ']']);
    if severity(first) || clock(first) {
        return true;
    }
    let bytes = first.as_bytes();
    let date = bytes.len() >= 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[..10]
            .iter()
            .enumerate()
            .all(|(i, b)| matches!(i, 4 | 7) || b.is_ascii_digit());
    date && ((bytes.get(10) == Some(&b'T') && first.get(11..).is_some_and(clock))
        || words
            .next()
            .is_some_and(|next| severity(next) || clock(next)))
}

/// Owner and authenticated requester are a conversation-author trust policy,
/// not a proof that a key belongs to a human. Verified agent attestations remove
/// those authors; absent profiles never become a claimed human credential.
pub(super) async fn trusted_authors(requester: &str, ctx: &PromptContext) -> HashSet<String> {
    let mut authors = HashSet::from([requester.to_owned()]);
    if let Some(owner) = &ctx.agent_owner_pubkey {
        authors.insert(owner.to_hex());
    }
    authors.remove(&ctx.agent_keys.public_key().to_hex());
    if authors.is_empty() {
        return authors;
    }
    let filter =
        serde_json::json!({"kinds":[0],"authors":authors.iter().collect::<Vec<_>>(),"limit":2});
    if let Ok(Ok(Value::Array(events))) = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        ctx.rest_client.query_raw_bounded(&[filter], 128 * 1024),
    )
    .await
    {
        for value in events.into_iter().take(2) {
            let Ok(event) = serde_json::from_value::<nostr::Event>(value) else {
                continue;
            };
            if event.kind.as_u16() != 0 || event.verify().is_err() {
                continue;
            }
            if event.tags.iter().any(|tag| {
                serde_json::to_string(tag).ok().is_some_and(|json| {
                    buzz_sdk::nip_oa::verify_auth_tag(&json, &event.pubkey).is_ok()
                })
            }) {
                authors.remove(&event.pubkey.to_hex());
            }
        }
    }
    authors
}
