//! Plaintext Kimi diagnostics are structural; owner activity remains separate.
use super::*;
use serde_json::{Map, Value};

fn identity(value: Option<&Value>) -> Option<Value> {
    let value = value?;
    if value.is_i64() || value.is_u64() {
        return Some(value.clone());
    }
    let text = value.as_str()?;
    (!text.is_empty() && text.len() <= 256 && text.bytes().all(|byte| byte.is_ascii_graphic()))
        .then(|| value.clone())
}

fn metadata(msg: &Value) -> Value {
    let mut fields = Map::new();
    fields.insert("payloadOmitted".into(), Value::Bool(true));
    if let Some(id) = identity(msg.get("id")) {
        fields.insert("requestId".into(), id);
    }
    if let Some(method) = msg.get("method").and_then(Value::as_str) {
        let method = match method {
            "initialize"
            | "authenticate"
            | "session/new"
            | "session/load"
            | "session/resume"
            | "session/prompt"
            | "session/cancel"
            | "session/update"
            | "session/request_permission"
            | "session/set_mode"
            | "session/set_model"
            | "session/set_config_option"
            | "_session/steering"
            | "_goose/unstable/session/steer" => method,
            _ => "other",
        };
        fields.insert("method".into(), method.into());
    }
    for (name, pointers) in [
        ("sessionId", &["/params/sessionId", "/result/sessionId"][..]),
        (
            "toolCallId",
            &["/params/toolCall/toolCallId", "/params/update/toolCallId"][..],
        ),
    ] {
        if let Some(id) = pointers
            .iter()
            .find_map(|pointer| identity(msg.pointer(pointer)))
        {
            fields.insert(name.into(), id);
        }
    }
    if let Some(update) = msg
        .pointer("/params/update/sessionUpdate")
        .and_then(Value::as_str)
    {
        let update = match update {
            "agent_message_chunk"
            | "agent_thought_chunk"
            | "tool_call"
            | "tool_call_update"
            | "plan"
            | "available_commands_update"
            | "session_info_update"
            | "usage_update"
            | "config_option_update"
            | "current_model_update"
            | "compaction_update"
            | "keepalive" => update,
            _ => "other",
        };
        fields.insert("update".into(), update.into());
    }
    if let Some(status @ ("pending" | "in_progress" | "completed" | "failed")) =
        msg.pointer("/params/update/status").and_then(Value::as_str)
    {
        fields.insert("status".into(), status.into());
    }
    if let Some(reason) = msg
        .pointer("/result/stopReason")
        .and_then(Value::as_str)
        .and_then(StopReason::from_str)
    {
        fields.insert("stopReason".into(), format!("{reason:?}").into());
    }
    if let Some(error) = msg.get("error") {
        let failure = kimi_recovery::KimiFailure::from_json(error);
        fields.insert(
            "failure".into(),
            serde_json::json!({
                "code":failure.code, "type":failure.error_type,
                "status":failure.status, "class":failure.class.label(),
            }),
        );
    } else if msg
        .pointer("/result/_meta/jetbrains/air/sessionFailure")
        .is_some()
    {
        let class = match provider_failure::check_kimi_response(&msg["result"]) {
            Err(AcpError::KimiFailure(failure)) => failure.class.label(),
            _ => "invalid_provider_failure",
        };
        fields.insert("failureClass".into(), class.into());
    }
    Value::Object(fields)
}

impl AcpClient {
    pub(super) fn trace_kimi_wire(&self, direction: &str, msg: &Value) {
        let metadata = metadata(msg);
        tracing::debug!(target:"acp::wire", process_generation=%self.process_instance,
            "{direction} {metadata}");
    }

    pub(super) fn trace_outgoing(&self, msg: &Value, notification: bool) {
        if self.kimi {
            self.trace_kimi_wire("→", msg);
        } else if notification {
            tracing::debug!(target:"acp::wire", "→ (notification) {}", serde_json::to_string(msg).unwrap_or_default());
        } else {
            tracing::debug!(target:"acp::wire", "→ {}", serde_json::to_string(msg).unwrap_or_default());
        }
    }
}
