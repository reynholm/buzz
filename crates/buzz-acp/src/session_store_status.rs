//! Restart follow-up ownership shares the reset record's atomic lock.
use super::*;
use buzz_core::session_control::{ControlOrigin, SessionCommand};

pub(crate) const STATUS_TEXT_BYTES: usize = 12 * 1024;
pub(crate) const STATUS_EVENT_BYTES: usize = 20 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RestartStatusPhase {
    WaitingRetirement,
    Pending,
    Started,
    Prepared,
    Completed,
    Failed,
    Unconfirmed,
    Cancelled,
}

/// One bounded follow-up per current restart; old command receipts retain dedup.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RestartStatus {
    pub token: Uuid,
    pub origin: ControlOrigin,
    pub scope: SessionScope,
    pub command: nostr::Event,
    pub owner: Uuid,
    pub deadline: u64,
    pub phase: RestartStatusPhase,
    /// Prepared signed bytes are immutable, including after lost acknowledgment.
    pub prepared: Option<nostr::Event>,
    pub failure_notice: bool,
    pub notice: Option<nostr::Event>,
    pub notice_confirmed: bool,
}

impl RestartStatus {
    pub(crate) fn active(&self) -> bool {
        matches!(
            self.phase,
            RestartStatusPhase::Pending
                | RestartStatusPhase::Started
                | RestartStatusPhase::Prepared
        )
    }
    pub(crate) fn valid(&self) -> bool {
        self.origin.validate().is_ok()
            && matches!(
                SessionCommand::parse(self.command.kind.as_u16(), &self.command.content),
                Some(SessionCommand::Restart(_))
            )
            && self.command.tags.iter().any(|tag| {
                let parts = tag.as_slice();
                parts.first().is_some_and(|part| part == "h")
                    && parts
                        .get(1)
                        .is_some_and(|part| part == &self.scope.channel_id().to_string())
            })
            && self
                .scope
                .root_event_id()
                .is_none_or(|root| crate::queue::reply_thread(&self.command, None) == root)
            && self.command.id.to_hex() == self.origin.command_event_id
            && self.command.verify().is_ok()
            && self
                .scope
                .root_event_id()
                .is_none_or(buzz_core::session_control::is_canonical_id)
            && serde_json::to_vec(&self.command).is_ok_and(|v| v.len() <= STATUS_EVENT_BYTES)
            && self.notice.as_ref().is_none_or(|event| {
                event.kind.as_u16() == 9
                    && event.verify().is_ok()
                    && event.content.len() <= STATUS_TEXT_BYTES
                    && serde_json::to_vec(event).is_ok_and(|v| v.len() <= STATUS_EVENT_BYTES)
            })
            && self.prepared.as_ref().is_none_or(|event| {
                event.kind.as_u16() == 9
                    && event.verify().is_ok()
                    && event.content.len() <= STATUS_TEXT_BYTES
                    && serde_json::to_vec(event).is_ok_and(|v| v.len() <= STATUS_EVENT_BYTES)
            })
    }
}

impl SessionStore {
    pub(crate) fn request_restart_status(
        &self,
        scope: &SessionScope,
        intent: &RestartIntent,
        status: RestartStatus,
    ) -> Result<Vec<String>, StoreError> {
        if status.scope != *scope
            || status.token != intent.token
            || status.origin != intent.origin
            || status.owner != self.instance
            || status.phase != RestartStatusPhase::WaitingRetirement
            || status.deadline != 0
            || status.prepared.is_some()
            || status.notice.is_some()
            || !status.valid()
        {
            return Err(StoreError::InvalidControl);
        }
        self.controls_request_restart_status(scope, intent, status)
    }

    /// Reconcile a proven-dead owner without authorizing any further effect.
    /// An empty scheduler never proves another live controller is abandoned.
    pub(super) fn recover_status(&self, saved: &RestartStatus) -> Result<(), StoreError> {
        if saved.owner == self.instance
            || self.instance_liveness(saved.owner) != InstanceLiveness::Dead
        {
            return Ok(());
        }
        if matches!(
            saved.phase,
            RestartStatusPhase::Completed
                | RestartStatusPhase::Failed
                | RestartStatusPhase::Cancelled
        ) {
            return Ok(());
        }
        let mut next = saved.clone();
        next.owner = self.instance;
        // A failed cancellation write and a crash are indistinguishable from
        // an abandoned Pending/Prepared turn. Dead-owner recovery never grants
        // another model attempt or send, even when signed bytes were prepared.
        next.phase = RestartStatusPhase::Unconfirmed;
        self.status_transition(saved, &next, false)?;
        Ok(())
    }

    pub(crate) fn status_transition(
        &self,
        expected: &RestartStatus,
        next: &RestartStatus,
        consume: bool,
    ) -> Result<(), StoreError> {
        if !next.valid()
            || expected.token != next.token
            || expected.origin != next.origin
            || expected.scope != next.scope
            || next.owner != self.instance
        {
            return Err(StoreError::InvalidControl);
        }
        if expected
            .prepared
            .as_ref()
            .is_some_and(|event| next.prepared.as_ref() != Some(event))
            || expected
                .notice
                .as_ref()
                .is_some_and(|event| next.notice.as_ref() != Some(event))
            || matches!(
                expected.phase,
                RestartStatusPhase::Completed
                    | RestartStatusPhase::Failed
                    | RestartStatusPhase::Cancelled
            )
            || (expected.phase != RestartStatusPhase::Pending
                && next.phase == RestartStatusPhase::Pending)
            || (consume
                && (next.phase != RestartStatusPhase::Completed
                    || next.failure_notice
                    || next.prepared.is_none()))
            || (next.phase == RestartStatusPhase::Completed && !consume)
            || (next.notice_confirmed && next.notice.is_none())
        {
            return Err(StoreError::Conflict);
        }
        self.update_with_quarantine(
            &self.record_path(&expected.scope),
            |current| {
                let mut record = current.cloned().ok_or(StoreError::Conflict)?;
                if record.restart_status.as_ref() != Some(expected)
                    || record.pending_restart.as_ref().map(|intent| intent.token)
                        != Some(expected.token)
                    || (expected.owner != self.instance
                        && self.instance_liveness(expected.owner) != InstanceLiveness::Dead)
                {
                    return Err(StoreError::Conflict);
                }
                record.restart_status = Some(next.clone());
                if consume {
                    record.pending_restart = None;
                }
                Ok(record)
            },
            false,
        )
    }
}
