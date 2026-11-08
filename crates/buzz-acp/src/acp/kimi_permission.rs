//! Request-bound Kimi permission diagnostics and post-response progress bounds.
use super::*;
use crate::config::{KimiPermissionVisibility, PermissionMode};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::time::Instant;
mod controls;
pub(crate) use controls::{owner_decision, PermissionDecision, PermissionNotice};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Visibility {
    Observable,
    CompatibilityUnobservable,
    Unknown,
}

impl Visibility {
    fn label(self) -> &'static str {
        match self {
            Self::Observable => "observable",
            Self::CompatibilityUnobservable => "compatibility_unobservable",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone)]
pub(super) struct PermissionRecord {
    generation: uuid::Uuid,
    visibility: Visibility,
    last_match: Option<Instant>,
    wait_started: Instant,
    pub decision_token: uuid::Uuid,
    pub id: serde_json::Value,
    pub session: String,
    pub prompt: u64,
    pub tool: String,
    pub description: &'static str,
    pub received: Instant,
    pub deadline: Instant,
}

struct ObservedTool {
    last_match: Instant,
    acknowledged: bool,
}

pub(super) struct PermissionState {
    policy: PermissionMode,
    visibility_policy: KimiPermissionVisibility,
    verified_main_only: bool,
    generation: Option<uuid::Uuid>,
    pub(super) stop_recorded: bool,
    observed_tools: HashMap<String, ObservedTool>,
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
            visibility_policy: Default::default(),
            verified_main_only: false,
            generation: None,
            stop_recorded: false,
            observed_tools: HashMap::new(),
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
    pub fn begin(
        &mut self,
        session: &str,
        prompt: u64,
        idle_timeout: Duration,
        generation: uuid::Uuid,
    ) {
        self.generation = Some(generation);
        self.stop_recorded = false;
        self.observed_tools.clear();
        self.idle_timeout = idle_timeout;
        self.session = Some(session.into());
        self.prompt = Some(prompt);
        self.seen.clear();
        self.read_only_tools.clear();
        self.pending = None;
        self.progress.clear();
    }

    pub fn progress_deadline(&self) -> Option<Instant> {
        self.progress
            .values()
            .filter(|record| record.visibility != Visibility::CompatibilityUnobservable)
            .map(|record| record.deadline)
            .min()
    }
}

impl AcpClient {
    pub(super) fn select_kimi_visibility(&mut self, result: &serde_json::Value) {
        self.kimi_permissions.verified_main_only = result
            .pointer("/agentInfo/name")
            .and_then(serde_json::Value::as_str)
            == Some("Kimi Code CLI")
            && result
                .pointer("/agentInfo/version")
                .and_then(serde_json::Value::as_str)
                == Some("2.5.2");
        self.log_kimi_visibility();
    }

    fn configure_kimi_visibility(&mut self, policy: KimiPermissionVisibility) {
        if self.kimi_permissions.visibility_policy != policy {
            self.kimi_permissions.visibility_policy = policy;
            self.log_kimi_visibility();
        }
    }

    fn log_kimi_visibility(&self) {
        let profile = match self.kimi_permissions.visibility_policy {
            KimiPermissionVisibility::Auto if self.kimi_permissions.verified_main_only => {
                "kimi_2_5_2_main_only"
            }
            KimiPermissionVisibility::Observable => "observable_override",
            _ => "unknown",
        };
        tracing::info!(target: "acp::permission", process_generation=%self.process_instance,
            profile, policy=?self.kimi_permissions.visibility_policy, "Kimi permission visibility selected");
        self.observe("permission_visibility_profile", serde_json::json!({
            "profile":profile, "selection":format!("{:?}", self.kimi_permissions.visibility_policy),
            "processGeneration":self.process_instance,
        }));
    }

    fn permission_effect_key(&self, tool: &str) -> String {
        // Work outlives prompts and processes, whereas ACP tool IDs may be reused.
        // Hash a framed tuple to retain bounded identity without retaining payloads.
        hex::encode(Sha256::digest(
            serde_json::to_vec(&(
                self.process_instance,
                &self.kimi_permissions.session,
                self.kimi_permissions.prompt,
                tool,
            ))
            .unwrap_or_default(),
        ))
    }

    pub(super) fn kimi_permission_timeout(&self) -> AcpError {
        let Some(record) = self
            .kimi_permissions
            .progress
            .values()
            .filter(|record| record.visibility != Visibility::CompatibilityUnobservable)
            .min_by_key(|record| record.deadline)
        else {
            return AcpError::PermissionTelemetryTimeout;
        };
        tracing::warn!(target: "acp::permission", process_generation=%record.generation,
            initiator="watchdog", visibility=record.visibility.label(),
            wait_ms=record.wait_started.elapsed().as_millis() as u64,
            last_matching_event_age_ms=?record.last_match.map(|last| last.elapsed().as_millis() as u64),
            reason=if record.visibility == Visibility::Observable { "observable_tool_timeout" } else { "telemetry_unavailable" },
            "Kimi permission watchdog expired");
        self.permission_event(record, "watchdog_expired");
        if record.visibility == Visibility::Observable {
            AcpError::PermissionProgressTimeout
        } else {
            AcpError::PermissionTelemetryTimeout
        }
    }

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
        let reason = if state == "watchdog_expired" {
            match record.visibility {
                Visibility::Observable => Some("observable_tool_timeout"),
                _ => Some("telemetry_unavailable"),
            }
        } else {
            None
        };
        let last_match_age = record
            .last_match
            .map(|last| last.elapsed().as_millis().min(u64::MAX as u128) as u64);
        tracing::debug!(target: "acp::permission", state, visibility=record.visibility.label(),
            process_generation=%record.generation, prompt=record.prompt,
            wait_ms=record.wait_started.elapsed().as_millis() as u64,
            last_matching_event_age_ms=?last_match_age, reason, "Kimi permission lifecycle");
        self.observe("permission_lifecycle",serde_json::json!({
            "state":state,"requestId":record.id,"sessionId":record.session,"promptId":record.prompt,
            "requestToken":record.decision_token,"policy":self.kimi_permissions.policy.as_wire_str(),
            "processGeneration":record.generation,"toolCallId":record.tool,
            "visibility":record.visibility.label(), "waitElapsedMs":record.wait_started.elapsed().as_millis().min(u64::MAX as u128) as u64,
            "lastMatchingEventAgeMs":last_match_age,
            "initiator":if state == "watchdog_expired" { Some("watchdog") } else if state == "cancelled" { Some("user") } else { None },
            "reason":reason,"description":record.description,
            "elapsedMs":record.received.elapsed().as_millis().min(u64::MAX as u128) as u64,
        }));
    }

    pub(super) fn finish_kimi_permissions(&mut self, state: &str) {
        if let Some(record) = self.kimi_permissions.pending.take() {
            self.permission_event(&record, state);
        }
        let records = std::mem::take(&mut self.kimi_permissions.progress);
        for record in records.values() {
            self.permission_event(record, state);
        }
        self.kimi_permissions.observed_tools.clear();
        self.kimi_permissions.read_only_tools.clear();
        self.kimi_permissions.generation = None;
        self.kimi_permissions.session = None;
        self.kimi_permissions.prompt = None;
    }

    pub(super) fn observe_kimi_permission_progress(&mut self, msg: &serde_json::Value) {
        if !self.kimi
            || self.kimi_permissions.generation != Some(self.process_instance)
            || self.last_prompt_id != self.kimi_permissions.prompt
            || self.last_prompt_id.is_none()
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
        let Some(tool) = update["toolCallId"]
            .as_str()
            .filter(|tool| !tool.is_empty() && tool.len() <= 256)
        else {
            return;
        };
        if matches!(update["kind"].as_str(), Some("read" | "search"))
            && self.kimi_permissions.read_only_tools.len() < 256
            && tool.len() <= 256
        {
            self.kimi_permissions.read_only_tools.insert(tool.into());
        }
        // The verified adapter maps tool.progress to a non-empty title update
        // without status. A create alone proves visibility, not execution.
        let verified_progress = self.kimi_permissions.verified_main_only
            && update["sessionUpdate"] == "tool_call_update"
            && update["status"].is_null()
            && update["title"]
                .as_str()
                .is_some_and(|title| !title.is_empty());
        if !matches!(
            update["status"].as_str(),
            Some("pending" | "in_progress" | "completed" | "failed")
        ) && !(update["sessionUpdate"] == "tool_call" && update["status"].is_null())
            && !verified_progress
        {
            return;
        }
        let acknowledged = verified_progress
            || matches!(
                update["status"].as_str(),
                Some("in_progress" | "completed" | "failed")
            );
        let now = Instant::now();
        if self.kimi_permissions.observed_tools.len() < 256
            || self.kimi_permissions.observed_tools.contains_key(tool)
        {
            self.kimi_permissions
                .observed_tools
                .entry(tool.into())
                .and_modify(|observed| {
                    observed.last_match = now;
                    observed.acknowledged |= acknowledged;
                })
                .or_insert(ObservedTool {
                    last_match: now,
                    acknowledged,
                });
        }
        if let Some(work) = &self.kimi_permissions.work {
            let effect_key = self.permission_effect_key(tool);
            match update["status"].as_str() {
                Some("in_progress") if !self.kimi_permissions.read_only_tools.contains(tool) => {
                    work.effect_started(&effect_key)
                }
                // A failed tool may already have produced a partial external effect.
                Some("completed") => work.effect_settled(&effect_key),
                _ => {}
            }
        }
        let progressed: Vec<String> = self
            .kimi_permissions
            .progress
            .iter()
            .filter(|(_, record)| {
                record.tool == tool
                    && record.generation == self.process_instance
                    && Some(record.prompt) == self.last_prompt_id
            })
            .map(|(key, _)| key.clone())
            .collect();
        for key in progressed {
            if let Some(mut record) = self.kimi_permissions.progress.remove(&key) {
                record.visibility = Visibility::Observable;
                record.last_match = Some(now);
                record.deadline = now + self.kimi_permissions.progress_timeout;
                self.permission_event(&record, "progress_observed");
                // Lifecycle visibility does not promise periodic heartbeats.
                // After execution acknowledgement, ordinary turn/work bounds own
                // liveness. Only a successful terminal status settles the effect.
                if !acknowledged {
                    self.kimi_permissions.progress.insert(key, record);
                }
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
            || self.kimi_permissions.generation != Some(self.process_instance)
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
        let observed = self.kimi_permissions.observed_tools.get(tool);
        let last_match = observed.map(|observed| observed.last_match);
        let acknowledged = observed.is_some_and(|observed| observed.acknowledged);
        let visibility = if last_match.is_some()
            || self.kimi_permissions.visibility_policy == KimiPermissionVisibility::Observable
        {
            Visibility::Observable
        } else if self.kimi_permissions.visibility_policy == KimiPermissionVisibility::Auto
            && self.kimi_permissions.verified_main_only
        {
            Visibility::CompatibilityUnobservable
        } else {
            Visibility::Unknown
        };
        let record = PermissionRecord {
            generation: self.process_instance,
            visibility,
            last_match,
            wait_started: now,
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
                work.effect_started(&self.permission_effect_key(&record.tool));
            }
        }
        self.write_kimi_permission(&record, &response).await?;
        self.permission_responded = true;
        self.pending_permission_id = None;
        self.kimi_permissions.pending = None;
        let mut record = record;
        record.wait_started = Instant::now();
        record.deadline = record.wait_started + self.kimi_permissions.progress_timeout;
        if selected.is_some_and(|option| option["kind"] == "allow_once") && !acknowledged {
            self.kimi_permissions.progress.insert(key, record);
        }
        Ok(())
    }
}
