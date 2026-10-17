//! Durable control receipts share the provider record's lock and atomic write.
use super::*;
use buzz_core::session_control::{
    control_is_fresh, is_canonical_id, ControlFailure, ControlOrigin, ControlOutcome, RestartDepth,
    CONTROL_FUTURE_SKEW_SECS, CONTROL_MAX_AGE_SECS,
};

/// Maximum protected command receipts per scope. Active receipts are never evicted.
pub const MAX_CONTROL_RECEIPTS: usize = 128;

/// Persisted restart policy; cleared only after successful foreground delivery.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestartIntent {
    /// CAS token; an older delivery cannot clear a newer restart.
    pub token: Uuid,
    /// Signed command identity and authorized effective author.
    pub origin: ControlOrigin,
    /// Requested next cold-start history depth.
    pub depth: RestartDepth,
}

/// Durable execution marker; Started never authorizes implicit reexecution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlReceiptState {
    /// Persisted before an external effect may begin.
    Started,
    /// Terminal outcome, reusable on redelivery.
    Finished(ControlOutcome),
}

/// One bounded deduplication receipt in a scope record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlReceipt {
    /// Canonical signed event ID.
    pub command_id: String,
    /// Signed time for compact; admission time for atomic restart. Retention
    /// includes the maximum future skew so a fresh restart cannot lose its receipt.
    pub created_at: u64,
    /// Instance which admitted the effect.
    pub owner_instance: Uuid,
    /// Captured live lease, or record revision when no live provider exists.
    pub owner_generation: Uuid,
    /// Current durable state.
    pub state: ControlReceiptState,
}

/// Admission result. Only New permits an initial effect; coalescing an active
/// duplicate and reconciling orphaned Started receipts belong to the controller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlAttempt {
    /// A new durable Started marker was written.
    New,
    /// Prior marker exists; do not execute again.
    PriorStarted,
    /// Prior terminal outcome; do not execute again.
    PriorFinished(ControlOutcome),
}

impl SessionStore {
    /// Atomically fence the old provider and persist restart intent plus Started
    /// receipt. The caller must freshness-check signed created_at immediately
    /// before admission. Receipts here use admission time because RestartIntent
    /// carries no signed timestamp. Duplicate command IDs never reapply a reset.
    pub fn request_restart(
        &self,
        scope: &SessionScope,
        intent: &RestartIntent,
    ) -> Result<(), StoreError> {
        intent
            .origin
            .validate()
            .map_err(|_| StoreError::InvalidControl)?;
        let now = unix_now()?;
        self.update_control(&self.record_path(scope), |current| {
            if current
                .and_then(|r| receipt(r, &intent.origin.command_event_id))
                .is_some()
            {
                return Err(StoreError::AlreadyRecorded);
            }
            let mut record = self.tombstone(current);
            make_room(&mut record.control_receipts, now)?;
            add_started(
                &mut record,
                &intent.origin.command_event_id,
                now,
                self.instance,
            );
            record.reason = Some(SessionReason::UserRestart(intent.depth));
            record.pending_restart = Some(intent.clone());
            Ok(record)
        })
        .or_else(|err| match err {
            StoreError::AlreadyRecorded => Ok(()),
            other => Err(other),
        })
    }

    /// Clear only this exact restart after successful foreground prompt delivery.
    pub fn complete_restart(&self, scope: &SessionScope, token: Uuid) -> Result<(), StoreError> {
        self.update_control(&self.record_path(scope), |current| {
            let mut record = current.cloned().ok_or(StoreError::Conflict)?;
            if record.pending_restart.as_ref().map(|i| i.token) != Some(token) {
                return Err(StoreError::Conflict);
            }
            record.pending_restart = None;
            Ok(record)
        })
    }

    /// Write Started before attempting compact. Return prior state on duplicate
    /// delivery, including after a crash. This API performs no provider operation.
    pub fn begin_control_attempt(
        &self,
        scope: &SessionScope,
        command_id: &str,
        created_at: u64,
    ) -> Result<ControlAttempt, StoreError> {
        if !is_canonical_id(command_id) {
            return Err(StoreError::InvalidControl);
        }
        let now = unix_now()?;
        let mut attempt = ControlAttempt::New;
        let result = self.update_control(&self.record_path(scope), |current| {
            if let Some(prior) = current.and_then(|r| receipt(r, command_id)) {
                attempt = match prior.state {
                    ControlReceiptState::Started => ControlAttempt::PriorStarted,
                    ControlReceiptState::Finished(outcome) => {
                        ControlAttempt::PriorFinished(outcome)
                    }
                };
                return Err(StoreError::AlreadyRecorded);
            }
            if !control_is_fresh(created_at, now) {
                return Err(StoreError::StaleRequest);
            }
            let mut record = current.cloned().unwrap_or_else(|| self.tombstone(None));
            make_room(&mut record.control_receipts, now)?;
            add_started(&mut record, command_id, created_at, self.instance);
            Ok(record)
        });
        match result {
            Ok(()) | Err(StoreError::AlreadyRecorded) => Ok(attempt),
            Err(err) => Err(err),
        }
    }

    /// Persist the owning instance's terminal result. Conflicting terminal writes
    /// are refused, preserving the first result rather than claiming success.
    pub fn finish_control_attempt(
        &self,
        scope: &SessionScope,
        command_id: &str,
        result: &ControlOutcome,
    ) -> Result<(), StoreError> {
        self.update_control(&self.record_path(scope), |current| {
            let mut record = current.cloned().ok_or(StoreError::Conflict)?;
            let prior = record
                .control_receipts
                .iter_mut()
                .find(|r| r.command_id == command_id)
                .ok_or(StoreError::Conflict)?;
            if prior.owner_instance != self.instance {
                return Err(StoreError::Conflict);
            }
            match prior.state {
                ControlReceiptState::Started => {
                    prior.state = ControlReceiptState::Finished(*result)
                }
                ControlReceiptState::Finished(previous) if previous == *result => {
                    return Err(StoreError::AlreadyRecorded)
                }
                _ => return Err(StoreError::Conflict),
            }
            Ok(record)
        })
        .or_else(|err| match err {
            StoreError::AlreadyRecorded => Ok(()),
            other => Err(other),
        })
    }

    /// Terminalize an orphan without reexecuting it. For this instance the caller
    /// must prove that the exact command+generation job has retired (including
    /// panic handling). A foreign owner additionally must be proven dead by its
    /// OS instance lock; empty local scheduler state is never that proof.
    pub(crate) fn reconcile_control_attempt(
        &self,
        scope: &SessionScope,
        command_id: &str,
        instance: Uuid,
        generation: Uuid,
    ) -> Result<(), StoreError> {
        if instance != self.instance && self.instance_alive(instance) {
            return Err(StoreError::Conflict);
        }
        self.update_control(&self.record_path(scope), |current| {
            let mut record = current.cloned().ok_or(StoreError::Conflict)?;
            let prior = record
                .control_receipts
                .iter_mut()
                .find(|r| r.command_id == command_id)
                .ok_or(StoreError::Conflict)?;
            if prior.owner_instance != instance
                || prior.owner_generation != generation
                || prior.state != ControlReceiptState::Started
            {
                return Err(StoreError::Conflict);
            }
            prior.state = ControlReceiptState::Finished(Err(ControlFailure::Unconfirmed));
            Ok(record)
        })
    }

    /// Control persistence failure leaves the previous atomic snapshot intact:
    /// it must neither promise a reset nor erase receipts protecting an effect.
    fn update_control(
        &self,
        path: &Path,
        decide: impl FnOnce(Option<&SessionRecord>) -> Result<SessionRecord, StoreError>,
    ) -> Result<(), StoreError> {
        self.update_with_quarantine(path, decide, false)
    }
}

fn receipt<'a>(record: &'a SessionRecord, command_id: &str) -> Option<&'a ControlReceipt> {
    record
        .control_receipts
        .iter()
        .find(|r| r.command_id == command_id)
}

fn add_started(record: &mut SessionRecord, command_id: &str, created_at: u64, instance: Uuid) {
    record.control_receipts.push(ControlReceipt {
        command_id: command_id.to_owned(),
        created_at,
        owner_instance: instance,
        owner_generation: record
            .live
            .as_ref()
            .map(|l| l.lease)
            .or(record.revision)
            .unwrap_or_else(Uuid::new_v4),
        state: ControlReceiptState::Started,
    });
}

fn make_room(receipts: &mut Vec<ControlReceipt>, now: u64) -> Result<(), StoreError> {
    receipts
        .retain(|r| matches!(r.state, ControlReceiptState::Started) || receipt_protected(r, now));
    if receipts.len() >= MAX_CONTROL_RECEIPTS {
        Err(StoreError::Busy)
    } else {
        Ok(())
    }
}

fn receipt_protected(receipt: &ControlReceipt, now: u64) -> bool {
    now <= receipt
        .created_at
        .saturating_add(CONTROL_MAX_AGE_SECS + CONTROL_FUTURE_SKEW_SECS)
}

fn unix_now() -> Result<u64, StoreError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| StoreError::InvalidControl)
}

/// Validate persisted metadata without dropping legacy provider records.
pub(super) fn valid_controls(record: &SessionRecord) -> bool {
    let mut ids = HashSet::new();
    record.control_receipts.len() <= MAX_CONTROL_RECEIPTS
        && record
            .control_receipts
            .iter()
            .all(|r| is_canonical_id(&r.command_id) && ids.insert(&r.command_id))
        && record
            .pending_restart
            .as_ref()
            .is_none_or(|i| i.origin.validate().is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_receipt_covers_future_skew_until_signed_event_expires() {
        let admission = 1_000;
        let signed_time = admission + CONTROL_FUTURE_SKEW_SECS;
        let mut receipts = vec![ControlReceipt {
            command_id: "a".repeat(64),
            created_at: admission,
            owner_instance: Uuid::new_v4(),
            owner_generation: Uuid::new_v4(),
            state: ControlReceiptState::Finished(Ok(())),
        }];
        let last_fresh = signed_time + CONTROL_MAX_AGE_SECS;
        assert!(control_is_fresh(signed_time, last_fresh));
        make_room(&mut receipts, last_fresh).unwrap();
        assert_eq!(
            receipts.len(),
            1,
            "a still-fresh +30s event retains its restart receipt"
        );
        make_room(&mut receipts, last_fresh + 1).unwrap();
        assert!(receipts.is_empty());
    }
}
