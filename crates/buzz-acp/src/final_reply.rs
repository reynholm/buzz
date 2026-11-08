//! Bounded main-stream final segment. ACP completion authorizes extraction, not
//! a claim that the user's task is complete.
use serde_json::Value;
const MAX_FINAL_BYTES: usize = 64 * 1024;

#[derive(Default)]
pub(crate) struct FinalReply {
    session: String,
    text: String,
    blocked: Option<&'static str>,
    failed: bool,
}

impl FinalReply {
    pub(crate) fn begin(&mut self, session: &str) {
        *self = Self {
            session: session.into(),
            ..Self::default()
        };
    }
    pub(crate) fn observe(&mut self, msg: &Value) {
        if msg["params"]["sessionId"].as_str() != Some(self.session.as_str()) {
            return;
        }
        let update = &msg["params"]["update"];
        let meta = &update["_meta"];
        // Children use the same ACP discriminators. They cannot end a main
        // segment, even when their text or tools are interleaved with it.
        if meta.get("parentToolCallId").is_some_and(|v| !v.is_null()) {
            return;
        }
        if let Some(agent) = meta.get("agent").filter(|v| !v.is_null()) {
            if agent["depth"].as_u64().is_some_and(|d| d > 0)
                || agent["agentName"].as_str().is_some_and(|n| n != "/root")
                || agent["kind"].as_str().is_some_and(|k| k != "root")
            {
                return;
            }
            if agent["depth"] != 0 || agent["agentName"] != "/root" || agent["kind"] != "root" {
                self.text.clear();
                self.blocked = Some("ambiguous_main_provenance");
                return;
            }
        }
        if meta.get("source").is_some_and(|v| !v.is_null()) {
            return;
        }
        if let Some(lifecycle) = meta.get("executionLifecycle") {
            if lifecycle["phase"] == "ended"
                && lifecycle.get("outcome").is_some_and(|v| v != "success")
            {
                self.failed = true;
            }
        }
        match update["sessionUpdate"].as_str() {
            Some("agent_thought_chunk" | "tool_call" | "tool_call_update") => {
                self.text.clear();
                // A later valid main segment can resolve phase ambiguity; an
                // overflow remains terminal for this turn.
                if self.blocked != Some("final_text_exceeds_bound") {
                    self.blocked = None;
                }
            }
            Some("agent_message_chunk") => {
                if update["content"]["type"] != "text" {
                    self.text.clear();
                    self.blocked = Some("nontext_terminal_content");
                    return;
                }
                let Some(text) = update["content"]["text"].as_str() else {
                    self.text.clear();
                    self.blocked = Some("malformed_terminal_text");
                    return;
                };
                if text.is_empty() {
                    return;
                }
                if self.text.len().saturating_add(text.len()) > MAX_FINAL_BYTES {
                    self.text.clear();
                    self.blocked = Some("final_text_exceeds_bound");
                    return;
                }
                if self.blocked.is_none() {
                    self.text.push_str(text);
                }
            }
            _ => {}
        }
    }
    pub(crate) fn take(&mut self, success: bool) -> Result<String, &'static str> {
        let text = std::mem::take(&mut self.text);
        if !success || self.failed {
            return Err("turn_not_successful");
        }
        if let Some(reason) = self.blocked {
            return Err(reason);
        }
        if text.trim().is_empty() {
            return Err("no_terminal_main_text");
        }
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(kind: &str, text: &str, meta: Value) -> Value {
        serde_json::json!({"params":{"sessionId":"main","update":{"sessionUpdate":kind,"content":{"type":"text","text":text},"_meta":meta}}})
    }
    #[test]
    fn runtime_final_segments_main_stream_and_preserves_empty_lifecycle() {
        for (updates, expected) in [
            (
                vec![event("agent_message_chunk", "Which option?", Value::Null)],
                Ok("Which option?"),
            ),
            (
                vec![
                    event("agent_message_chunk", "commentary", Value::Null),
                    event("tool_call", "", Value::Null),
                    event("agent_message_chunk", "Final", Value::Null),
                    event(
                        "agent_message_chunk",
                        "",
                        serde_json::json!({"executionLifecycle":{"v":1,"kind":"request","phase":"ended","outcome":"success"}}),
                    ),
                    event("agent_message_chunk", "", serde_json::json!({"usage":{}})),
                ],
                Ok("Final"),
            ),
            (
                vec![
                    event("agent_message_chunk", "before", Value::Null),
                    event("agent_thought_chunk", "private", Value::Null),
                    event("agent_message_chunk", "after", Value::Null),
                ],
                Ok("after"),
            ),
            (
                vec![
                    event("agent_message_chunk", "before", Value::Null),
                    event("agent_thought_chunk", "private", Value::Null),
                ],
                Err("no_terminal_main_text"),
            ),
            (
                vec![
                    event("agent_message_chunk", "before", Value::Null),
                    event("tool_call", "", Value::Null),
                ],
                Err("no_terminal_main_text"),
            ),
            (
                vec![
                    event("agent_message_chunk", "Final", Value::Null),
                    event(
                        "agent_message_chunk",
                        "child",
                        serde_json::json!({"parentToolCallId":"child"}),
                    ),
                    event(
                        "tool_call",
                        "",
                        serde_json::json!({"agent":{"agentName":"/root/child","depth":1,"kind":"subagent"}}),
                    ),
                ],
                Ok("Final"),
            ),
            (
                vec![event(
                    "agent_message_chunk",
                    "synthetic",
                    serde_json::json!({"source":"slash_command"}),
                )],
                Err("no_terminal_main_text"),
            ),
            (
                vec![event(
                    "agent_message_chunk",
                    "ambiguous",
                    serde_json::json!({"agent":{"depth":0}}),
                )],
                Err("ambiguous_main_provenance"),
            ),
        ] {
            let mut capture = FinalReply::default();
            capture.begin("main");
            for update in updates {
                capture.observe(&update);
            }
            assert_eq!(capture.take(true).as_deref().map_err(|e| *e), expected);
        }
    }
    #[test]
    fn runtime_final_respects_kind9_byte_limit_and_resets_next_prompt() {
        let mut capture = FinalReply::default();
        capture.begin("main");
        capture.observe(&event(
            "agent_message_chunk",
            &"x".repeat(65_537),
            Value::Null,
        ));
        assert_eq!(capture.take(true).err(), Some("final_text_exceeds_bound"));
        capture.begin("main");
        capture.observe(&event("agent_message_chunk", "Next answer", Value::Null));
        assert_eq!(capture.take(true).unwrap(), "Next answer");
    }
}
