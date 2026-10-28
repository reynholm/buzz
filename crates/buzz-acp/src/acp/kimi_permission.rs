//! Request-bound Kimi permission diagnostics and post-response progress bounds.
use super::*;
use crate::config::PermissionMode;
use std::time::Duration;
use tokio::time::Instant;
mod controls;
pub(crate) use controls::{owner_decision, PermissionDecision, PermissionNotice};

#[derive(Clone)]
pub(super) struct PermissionRecord {
    pub decision_token: uuid::Uuid,
    pub id: serde_json::Value,
    pub session: String,
    pub prompt: u64,
    pub tool: String,
    pub description: &'static str,
    pub received: Instant,
    pub deadline: Instant,
}

pub(super) struct PermissionState {
    policy: PermissionMode,
    decision_timeout: Duration,
    idle_timeout: Duration,
    decisions: Option<tokio::sync::mpsc::Receiver<PermissionDecision>>,
    notice: Option<PermissionNotice>,
    work: Option<crate::queue::LogicalWork>,
    session: Option<String>,
    prompt: Option<u64>,
    seen: HashSet<String>,
    read_only_tools: HashSet<String>,
    pending: Option<PermissionRecord>,
    progress: HashMap<String, PermissionRecord>,
    pub progress_timeout: Duration,
}

impl Default for PermissionState {
    fn default() -> Self {
        Self {
            policy: PermissionMode::BypassPermissions,
            decision_timeout: Duration::from_secs(120),
            idle_timeout: Duration::from_secs(1500),
            decisions: None,
            notice: None,
            work: None,
            session: None,
            prompt: None,
            seen: HashSet::new(),
            read_only_tools: HashSet::new(),
            pending: None,
            progress: HashMap::new(),
            progress_timeout: Duration::from_secs(120),
        }
    }
}

fn id_key(id: &serde_json::Value) -> Option<String> {
    if id.is_i64() || id.is_u64() {
        return Some(id.to_string());
    }
    let text = id.as_str()?;
    (text.len() <= 256 && !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_graphic()))
        .then(|| id.to_string())
}

fn safe_title(title: Option<&str>) -> &'static str {
    match title {
        Some("Bash") => "shell command",
        Some("Read") => "read file",
        Some("Grep") => "search text",
        Some("Glob") => "find files",
        Some("Edit") => "edit file",
        Some("Write") => "write file",
        _ => "tool action",
    }
}

impl PermissionState {
    pub fn begin(&mut self, session: &str, prompt: u64, idle_timeout: Duration) {
        self.idle_timeout = idle_timeout;
        self.session = Some(session.into());
        self.prompt = Some(prompt);
        self.seen.clear();
        self.read_only_tools.clear();
        self.pending = None;
        self.progress.clear();
    }

    pub fn progress_deadline(&self) -> Option<Instant> {
        self.progress.values().map(|record| record.deadline).min()
    }
}

impl AcpClient {
    async fn write_kimi_permission(
        &mut self,
        record: &PermissionRecord,
        response: &serde_json::Value,
    ) -> Result<(), AcpError> {
        let now = Instant::now();
        let mut deadline = self
            .current_hard_deadline
            .unwrap_or(now + Duration::from_secs(30))
            .min(now + Duration::from_secs(30))
            .min(record.received + self.kimi_permissions.idle_timeout);
        if let Some(work) = &self.kimi_permissions.work {
            deadline = deadline.min(Instant::from_std(work.deadline));
        }
        let effective = deadline.saturating_duration_since(now);
        let written = tokio::time::timeout_at(deadline, self.write_ndjson(response))
            .await
            .map_err(|_| AcpError::WriteTimeout(effective))
            .and_then(|result| result);
        if written.is_ok() {
            self.permission_event(record, "response_written");
        } else {
            self.permission_event(record, "response_write_failed");
        }
        self.kimi_permissions.pending = None;
        self.pending_permission_id = None;
        written
    }

    pub(crate) fn configure_kimi_progress(&mut self, timeout: Duration) {
        self.kimi_permissions.progress_timeout = timeout;
    }

    fn permission_event(&self, record: &PermissionRecord, state: &str) {
        if let Some(work) = &self.kimi_permissions.work {
            match state {
                "received" | "waiting_approval" => {
                    work.transition("waiting_approval", "permission_request")
                }
                "progress_observed" => work.transition("running", "permission_progress"),
                _ => {}
            }
        }
        self.observe("permission_lifecycle",serde_json::json!({
            "state":state,"requestId":record.id,"sessionId":record.session,"promptId":record.prompt,
            "requestToken":record.decision_token,"policy":self.kimi_permissions.policy.as_wire_str(),
            "processGeneration":self.process_instance,"toolCallId":record.tool,"description":record.description,
            "elapsedMs":record.received.elapsed().as_millis().min(u64::MAX as u128) as u64,
        }));
    }

    pub(super) fn finish_kimi_permissions(&mut self, state: &str) {
        if let Some(record) = self.kimi_permissions.pending.take() {
            self.permission_event(&record, state);
        }
        let records = std::mem::take(&mut self.kimi_permissions.progress);
        for record in records.values() {
            if state == "prompt_completed" {
                self.permission_event(record, "progress_observed");
            }
            self.permission_event(record, state);
        }
        self.kimi_permissions.session = None;
        self.kimi_permissions.prompt = None;
    }

    pub(super) fn observe_kimi_permission_progress(&mut self, msg: &serde_json::Value) {
        if !self.kimi
            || msg.get("method").and_then(serde_json::Value::as_str) != Some("session/update")
            || msg
                .pointer("/params/sessionId")
                .and_then(serde_json::Value::as_str)
                != self.kimi_permissions.session.as_deref()
        {
            return;
        }
        let update = &msg["params"]["update"];
        if !matches!(
            update["sessionUpdate"].as_str(),
            Some("tool_call" | "tool_call_update")
        ) {
            return;
        }
        let Some(tool) = update["toolCallId"].as_str() else {
            return;
        };
        if matches!(update["kind"].as_str(), Some("read" | "search"))
            && self.kimi_permissions.read_only_tools.len() < 256
            && tool.len() <= 256
        {
            self.kimi_permissions.read_only_tools.insert(tool.into());
        }
        if !matches!(
            update["status"].as_str(),
            Some("in_progress" | "completed" | "failed")
        ) {
            return;
        }
        if let Some(work) = &self.kimi_permissions.work {
            match update["status"].as_str() {
                Some("in_progress") if !self.kimi_permissions.read_only_tools.contains(tool) => {
                    work.effect_started(tool)
                }
                // A failed tool may already have produced a partial external effect.
                Some("completed") => work.effect_settled(tool),
                _ => {}
            }
        }
        let progressed: Vec<String> = self
            .kimi_permissions
            .progress
            .iter()
            .filter(|(_, record)| record.tool == tool)
            .map(|(key, _)| key.clone())
            .collect();
        for key in progressed {
            if let Some(record) = self.kimi_permissions.progress.remove(&key) {
                self.permission_event(&record, "progress_observed");
            }
        }
    }

    pub(super) async fn handle_kimi_permission_request(
        &mut self,
        msg: &serde_json::Value,
    ) -> Result<(), AcpError> {
        let id = msg
            .get("id")
            .ok_or_else(|| AcpError::Protocol("permission request missing id".into()))?;
        let key = id_key(id)
            .ok_or_else(|| AcpError::Protocol("permission request has invalid id".into()))?;
        let session = msg
            .pointer("/params/sessionId")
            .and_then(serde_json::Value::as_str);
        if session != self.kimi_permissions.session.as_deref()
            || self.last_prompt_id != self.kimi_permissions.prompt
            || self.last_prompt_id.is_none()
        {
            self.write_ndjson(&permission_response_cancelled(id))
                .await?;
            self.observe("permission_lifecycle",serde_json::json!({"state":"refused_stale_session","requestId":id,"processGeneration":self.process_instance}));
            return Ok(());
        }
        if self.kimi_permissions.seen.len() >= 1024
            || !self.kimi_permissions.seen.insert(key.clone())
        {
            self.write_ndjson(&permission_response_cancelled(id))
                .await?;
            self.observe("permission_lifecycle",serde_json::json!({"state":"refused_duplicate","requestId":id,"processGeneration":self.process_instance}));
            return Err(AcpError::PermissionConflict);
        }
        let tool = msg
            .pointer("/params/toolCall/toolCallId")
            .and_then(serde_json::Value::as_str)
            .filter(|tool| !tool.is_empty() && tool.len() <= 256)
            .ok_or_else(|| {
                AcpError::Protocol("permission request missing bounded tool call id".into())
            })?;
        let now = Instant::now();
        let record = PermissionRecord {
            decision_token: uuid::Uuid::new_v4(),
            id: id.clone(),
            session: session.unwrap_or_default().into(),
            prompt: self.last_prompt_id.unwrap_or_default(),
            tool: tool.into(),
            description: safe_title(
                msg.pointer("/params/toolCall/title")
                    .and_then(serde_json::Value::as_str),
            ),
            received: now,
            deadline: now + self.kimi_permissions.progress_timeout,
        };
        self.permission_event(&record, "received");
        self.kimi_permissions.pending = Some(record.clone());
        self.pending_permission_id = Some(id.clone());
        self.permission_responded = false;
        let options = msg
            .pointer("/params/options")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| AcpError::Protocol("permission request missing options".into()))?;
        let allow = match self.permission_allow_decision(&record, msg).await {
            Ok(allow) => allow,
            Err(error) => {
                self.permission_event(
                    &record,
                    if self.kimi_work_cancelled() {
                        "decision_cancelled"
                    } else {
                        "decision_unavailable"
                    },
                );
                let _ = self
                    .write_kimi_permission(&record, &permission_response_cancelled(id))
                    .await;
                return Err(error);
            }
        };
        let selected = if allow {
            options.iter().find(|option| option["kind"] == "allow_once")
        } else {
            None
        }
        .or_else(|| {
            options
                .iter()
                .find(|option| option["kind"] == "reject_once")
        });
        let response = if let Some(option) = selected {
            let option_id = option["optionId"].as_str().ok_or_else(|| {
                AcpError::Protocol("permission option missing offered optionId".into())
            })?;
            self.permission_event(
                &record,
                if option["kind"] == "allow_once" {
                    "decision_allow_once"
                } else {
                    "decision_reject_once"
                },
            );
            permission_response_selected(id, option_id)
        } else {
            self.permission_event(&record, "decision_cancelled");
            permission_response_cancelled(id)
        };
        if self.kimi_work_cancelled() {
            let _ = self
                .write_kimi_permission(&record, &permission_response_cancelled(id))
                .await;
            return Err(AcpError::PermissionRequired);
        }
        if selected.is_some_and(|option| option["kind"] == "allow_once") {
            let read_only = matches!(
                msg.pointer("/params/toolCall/kind")
                    .and_then(serde_json::Value::as_str),
                Some("read" | "search")
            );
            if read_only && self.kimi_permissions.read_only_tools.len() < 256 {
                self.kimi_permissions
                    .read_only_tools
                    .insert(record.tool.clone());
            } else if let Some(work) = &self.kimi_permissions.work {
                work.effect_started(&record.tool);
            }
        }
        self.write_kimi_permission(&record, &response).await?;
        self.permission_responded = true;
        self.pending_permission_id = None;
        self.kimi_permissions.pending = None;
        let mut record = record;
        record.deadline = Instant::now() + self.kimi_permissions.progress_timeout;
        self.kimi_permissions.progress.insert(key, record);
        Ok(())
    }
}
