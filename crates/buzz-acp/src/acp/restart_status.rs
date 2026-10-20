//! Bounded answer capture for one controller-owned, text-only prompt.
use super::*;

pub(super) struct StatusCapture {
    pub(super) session: String,
    text: String,
    pub(super) invalid: bool,
}
impl AcpClient {
    pub(crate) fn begin_status_capture(&mut self, session: &str) {
        self.status_capture = Some(StatusCapture {
            session: session.into(),
            text: String::new(),
            invalid: false,
        });
    }
    pub(crate) fn take_status_text(&mut self) -> Option<String> {
        let capture = self.status_capture.take()?;
        (!capture.invalid && !capture.text.trim().is_empty())
            .then(|| capture.text.trim().to_owned())
    }
    pub(super) fn capture_status_update(&mut self, msg: &serde_json::Value) {
        let Some(capture) = &mut self.status_capture else {
            return;
        };
        if msg["params"]["sessionId"].as_str() != Some(capture.session.as_str()) {
            return;
        }
        let update = &msg["params"]["update"];
        match update["sessionUpdate"].as_str() {
            Some("agent_message_chunk") => {
                if let Some(text) = update["content"]["text"].as_str() {
                    if capture.text.len().saturating_add(text.len())
                        > crate::session_store::STATUS_TEXT_BYTES
                    {
                        capture.invalid = true;
                    } else if !capture.invalid {
                        capture.text.push_str(text);
                    }
                }
            }
            Some("tool_call" | "tool_call_update") => capture.invalid = true,
            _ => {}
        }
    }
    pub(super) fn status_capture_invalid(&self) -> bool {
        self.status_capture
            .as_ref()
            .is_some_and(|capture| capture.invalid)
    }
}
