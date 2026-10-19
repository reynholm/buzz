//! Claude's standard usage size is provisional until the SDK reports a window.
//!
//! The installed adapter supports filtered SDK result notifications. Pair a
//! validated result window with its following final standard usage update;
//! never use aggregate SDK usage as the context-occupancy numerator.
use super::{AcpClient, StandardAdapterKind};
use serde_json::Value;
use uuid::Uuid;

pub(super) struct State {
    generation: Uuid,
    enabled: bool,
    saw_usage: bool,
    pending_window: Option<u64>,
    confirmed_window: Option<u64>,
    awaiting_final: bool,
}

impl State {
    pub(super) fn new(generation: Uuid) -> Self {
        Self {
            generation,
            enabled: true,
            saw_usage: false,
            pending_window: None,
            confirmed_window: None,
            awaiting_final: false,
        }
    }
}

impl AcpClient {
    /// Start a new numerator measurement while retaining capacity established
    /// for this same provider-session generation. A tool-published reply may
    /// precede the next provider result, so confirmed capacity must survive.
    pub(super) fn begin_claude_context_turn(&mut self, session: &str) {
        if self.standard_adapter != Some(StandardAdapterKind::Claude) {
            return;
        }
        if let Some(state) = self.claude_context.get_mut(session) {
            state.enabled = true;
            state.saw_usage = false;
            state.pending_window = None;
            state.awaiting_final = false;
        }
        if let Some(binding) = self.session_status.get(session) {
            // The old numerator belongs to the previous turn. The next usage
            // notification can immediately use the retained confirmed window.
            binding.clear_occupancy();
        }
    }

    pub(super) fn reset_claude_context(&mut self, session: &str, enabled: Option<bool>) {
        if self.standard_adapter != Some(StandardAdapterKind::Claude) {
            return;
        }
        if let Some(state) = self.claude_context.get_mut(session) {
            if let Some(enabled) = enabled {
                state.enabled = enabled;
            }
            state.saw_usage = false;
            state.pending_window = None;
            state.confirmed_window = None;
            state.awaiting_final = false;
        }
        if let Some(binding) = self.session_status.get(session) {
            binding.clear_occupancy();
        }
    }

    /// This notification can contain result text. Consume it before tracing or
    /// observer publication, including malformed or unsolicited SDK messages.
    pub(super) fn consume_claude_context_frame(&mut self, frame: &Value) -> bool {
        if self.standard_adapter != Some(StandardAdapterKind::Claude)
            || frame.get("method").and_then(Value::as_str) != Some("_claude/sdkMessage")
            || frame.get("id").is_some()
        {
            return false;
        }
        let Some(session) = frame.pointer("/params/sessionId").and_then(Value::as_str) else {
            return true;
        };
        let Some(binding) = self.session_status.get(session).filter(|b| b.is_live()) else {
            return true;
        };
        let Some(state) = self.claude_context.get_mut(session) else {
            return true;
        };
        if state.generation != binding.generation_id() {
            return true;
        }
        // The adapter emits this SDK frame before processing the result, then
        // emits the result's final standard usage update. A preceding streaming
        // reading proves the current result can have a context numerator.
        let saw_usage = std::mem::take(&mut state.saw_usage);
        state.pending_window = None;
        state.confirmed_window = None;
        state.awaiting_final = true;
        if state.enabled && saw_usage {
            state.pending_window = authoritative_window(&frame["params"]["message"]);
        }
        binding.clear_occupancy();
        true
    }

    pub(super) fn record_claude_context_usage(&mut self, frame: &Value) {
        let Some(session) = frame.pointer("/params/sessionId").and_then(Value::as_str) else {
            return;
        };
        let Some(binding) = self.session_status.get(session).filter(|b| b.is_live()) else {
            return;
        };
        let Some(state) = self.claude_context.get_mut(session) else {
            return;
        };
        if state.generation != binding.generation_id() || !state.enabled {
            return;
        }
        let used = frame.pointer("/params/update/used").and_then(Value::as_u64);
        let final_update = std::mem::take(&mut state.awaiting_final);
        let window = if final_update {
            state.confirmed_window = state.pending_window.take().filter(|_| used.is_some());
            state.confirmed_window
        } else {
            state.confirmed_window
        };
        state.saw_usage = used.is_some() && !final_update;
        match used.zip(window) {
            Some((used, size)) => binding.record_occupancy(used, size),
            None => binding.clear_occupancy(),
        }
    }
}

/// With result-only filtering there is no safe way to select a primary model
/// from a mixed-model tally. Accept a window only when every reported model
/// agrees. Aliases and subagents cannot cause a first-entry or prefix guess.
fn authoritative_window(result: &Value) -> Option<u64> {
    if result.get("type")?.as_str()? != "result"
        || result.get("subtype")?.as_str()? != "success"
        || result.get("is_error")?.as_bool()?
        || result.get("num_turns")?.as_u64()? == 0
    {
        return None;
    }
    let models = result.get("modelUsage")?.as_object()?;
    if models.is_empty() || models.len() > 64 {
        return None;
    }
    let mut window = None;
    for model in models.values() {
        let size = model
            .get("contextWindow")?
            .as_u64()
            .filter(|size| *size > 0)?;
        if window.is_some_and(|previous| previous != size) {
            return None;
        }
        window = Some(size);
    }
    window
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_successful_unambiguous_provider_windows_are_authoritative() {
        let valid = json!({"type":"result", "subtype":"success", "is_error":false,
            "num_turns":1, "modelUsage":{"alias":{"contextWindow":1_000_000}}});
        assert_eq!(authoritative_window(&valid), Some(1_000_000));
        for bad in [
            Value::Null,
            json!(0),
            json!(-1),
            json!(1.5),
            json!("1000000"),
        ] {
            let mut frame = valid.clone();
            frame["modelUsage"]["alias"]["contextWindow"] = bad;
            assert_eq!(authoritative_window(&frame), None);
        }
        let mut frame = valid.clone();
        frame["modelUsage"]["subagent"] = json!({"contextWindow":200_000});
        assert_eq!(authoritative_window(&frame), None);
        frame["modelUsage"]["subagent"]["contextWindow"] = json!(1_000_000);
        assert_eq!(authoritative_window(&frame), Some(1_000_000));
        for (key, value) in [
            ("num_turns", json!(0)),
            ("is_error", json!(true)),
            ("subtype", json!("error_during_execution")),
            ("modelUsage", json!({})),
        ] {
            let mut frame = valid.clone();
            frame[key] = value;
            assert_eq!(authoritative_window(&frame), None);
        }
    }
}
