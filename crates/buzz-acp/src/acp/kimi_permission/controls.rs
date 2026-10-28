//! Existing thread transport for explicit, owner-authenticated permission votes.
use super::*;
use crate::{
    config::PermissionMode,
    queue::{LogicalWork, ThreadTags},
    relay::RestClient,
};
use tokio::sync::mpsc;

/// A vote is valid only for the random token issued to one pending request.
#[derive(Clone, Debug)]
pub(crate) struct PermissionDecision {
    pub token: uuid::Uuid,
    pub allow: bool,
}

/// A scoped visible request uses the same relay credentials as its work.
#[derive(Clone)]
pub(crate) struct PermissionNotice {
    pub rest: RestClient,
    pub channel: uuid::Uuid,
    pub tags: ThreadTags,
}

/// Parse only command-first, signed owner messages addressed to this agent.
/// An optional display mention follows the command, never precedes its intent.
pub(crate) fn owner_decision(
    event: &nostr::Event,
    agent: &str,
    owner: Option<&str>,
) -> Option<PermissionDecision> {
    if event.kind.as_u16() != 9
        || owner.is_none_or(|owner| event.pubkey.to_hex() != owner)
        || event.verify().is_err()
        || !crate::event_mentions_agent(event, agent)
    {
        return None;
    }
    let tokens: Vec<&str> = event.content.split_whitespace().collect();
    if tokens.len() < 2
        || (tokens.len() > 2 && !tokens[2].starts_with('@') && !tokens[2].starts_with("nostr:npub"))
    {
        return None;
    }
    let allow = match tokens[0] {
        "!approve" => true,
        "!deny" => false,
        _ => return None,
    };
    Some(PermissionDecision {
        token: uuid::Uuid::parse_str(tokens[1]).ok()?,
        allow,
    })
}

impl AcpClient {
    pub(crate) fn is_kimi(&self) -> bool {
        self.kimi
    }

    pub(crate) fn logical_work(&self) -> Option<&LogicalWork> {
        self.kimi_permissions.work.as_ref()
    }

    /// Harness policy is explicit; advertised adapter modes are a separate fact.
    pub(crate) fn configure_kimi_policy(&mut self, policy: PermissionMode) {
        self.kimi_permissions.policy = policy;
    }

    /// A fresh prompt mailbox cannot receive replies to a retired generation.
    pub(crate) fn install_permission_decisions(&mut self, rx: mpsc::Receiver<PermissionDecision>) {
        self.kimi_permissions.decisions = Some(rx);
    }

    /// Bind cancellation, remaining budget and the user-visible reply route.
    pub(crate) fn configure_kimi_work(
        &mut self,
        work: Option<&LogicalWork>,
        notice: Option<PermissionNotice>,
    ) {
        self.kimi_permissions.work = work.cloned();
        self.kimi_permissions.notice = notice;
        if let Some(work) = work {
            self.kimi_permissions.progress_timeout = work.policy.approval_progress_timeout;
            self.kimi_permissions.decision_timeout = work.policy.approval_decision_timeout;
        }
    }

    pub(super) fn kimi_work_cancelled(&self) -> bool {
        self.kimi_permissions
            .work
            .as_ref()
            .is_some_and(LogicalWork::cancelled)
    }

    pub(super) async fn permission_allow_decision(
        &mut self,
        record: &PermissionRecord,
        msg: &serde_json::Value,
    ) -> Result<bool, AcpError> {
        if self.kimi_work_cancelled() {
            return Err(AcpError::PermissionRequired);
        }
        match self.kimi_permissions.policy {
            PermissionMode::BypassPermissions => return Ok(true),
            PermissionMode::DontAsk | PermissionMode::Plan => return Ok(false),
            PermissionMode::AcceptEdits
                if msg
                    .pointer("/params/toolCall/kind")
                    .and_then(serde_json::Value::as_str)
                    == Some("edit") =>
            {
                return Ok(true)
            }
            _ => {}
        }
        if self.observer.is_none() {
            self.permission_event(record, "decision_view_unavailable");
            return Err(AcpError::PermissionViewUnavailable);
        }
        let Some(mut decisions) = self.kimi_permissions.decisions.take() else {
            self.permission_event(record, "decision_path_unavailable");
            return Err(AcpError::PermissionRequired);
        };
        let now = Instant::now();
        let mut deadline = (now + self.kimi_permissions.decision_timeout)
            .min(
                self.current_hard_deadline
                    .unwrap_or(now + self.kimi_permissions.decision_timeout),
            )
            .min(now + self.kimi_permissions.idle_timeout);
        if let Some(work) = &self.kimi_permissions.work {
            deadline = deadline.min(Instant::from_std(work.deadline));
        }
        if let Some(notice) = self.kimi_permissions.notice.clone() {
            let text=format!("Permission required: {}. Inspect the full request in Activity → Raw ACP (session/request_permission), matching request {} and tool {}. To allow, send `!approve {}`; to deny, send `!deny {}`. Put the agent mention after the command, in this same thread. This request expires in {}s.",record.description,record.id,record.tool,record.decision_token,record.decision_token,deadline.saturating_duration_since(now).as_secs());
            let event = crate::pool::build_failure_notice_event(
                &notice.rest.keys,
                notice.channel,
                &notice.tags,
                &text,
            )
            .ok_or(AcpError::PermissionRequired)?;
            let published = tokio::time::timeout_at(
                deadline.min(now + Duration::from_secs(5)),
                notice.rest.submit_event(&event),
            )
            .await;
            if !matches!(published,Ok(Ok(ref response)) if response.get("accepted").and_then(serde_json::Value::as_bool)!=Some(false))
            {
                self.permission_event(record, "decision_notice_failed");
                return Err(AcpError::PermissionRequired);
            }
        }
        self.permission_event(record, "waiting_approval");
        let decision = loop {
            if Instant::now() >= deadline {
                break Err(AcpError::PermissionDecisionTimeout);
            }
            if self.kimi_work_cancelled() {
                break Err(AcpError::PermissionRequired);
            }
            match tokio::time::timeout_at(deadline, decisions.recv()).await {
                Ok(Some(decision)) if decision.token == record.decision_token => {
                    break Ok(decision.allow)
                }
                Ok(Some(_)) => self.permission_event(record, "stale_decision_ignored"),
                Ok(None) => break Err(AcpError::PermissionRequired),
                Err(_) => break Err(AcpError::PermissionDecisionTimeout),
            }
        };
        self.kimi_permissions.decisions = Some(decisions);
        if self.kimi_work_cancelled() {
            return Err(AcpError::PermissionRequired);
        }
        decision
    }
}
