//! Source acceptance and per-attempt freshness admission share the record lock.
use super::*;
use crate::authorization_context::Admission;
use crate::provider_background::SavedEvent;

impl SessionRecord {
    /// Called inside the same lock that replaces or tombstones a live session.
    pub(super) fn preserve_authorization_revocation(
        &mut self,
        previous: Option<&Self>,
    ) -> Result<(), StoreError> {
        let Some(old) = previous
            .and_then(|record| record.live.as_ref())
            .and_then(|live| live.provider_background.as_ref())
        else {
            return Ok(());
        };
        let Some(journal) = &old.authorization else {
            return Ok(());
        };
        if self
            .live
            .as_ref()
            .and_then(|live| live.provider_background.as_ref())
            .is_some_and(|new| new.identity == old.identity)
        {
            return Ok(());
        }
        let control =
            journal
                .control
                .clone()
                .unwrap_or_else(|| crate::authorization_context::Control {
                    cause: self
                        .reason
                        .clone()
                        .map(String::from)
                        .unwrap_or_else(|| "scope_replacement".into()),
                    source_id: self
                        .pending_restart
                        .as_ref()
                        .map(|intent| intent.origin.command_event_id.clone()),
                });
        self.authorization_revocation = Some(crate::authorization_context::Revocation {
            identity: old.identity.clone(),
            revision: if journal.control.is_some() {
                journal.revision
            } else {
                journal
                    .revision
                    .checked_add(1)
                    .ok_or(StoreError::Unreadable)?
            },
            update_id: if journal.control.is_some() {
                journal.update_id
            } else {
                Uuid::new_v4()
            },
            cause: control.cause,
            source_id: control.source_id,
            source_ids: old
                .events
                .iter()
                .map(|source| source.event.id.to_hex())
                .collect(),
        });
        Ok(())
    }
}

impl SessionStore {
    /// Fence this process's authority before its shutdown grace period.
    pub(crate) fn revoke_authorization_for_shutdown(
        &self,
        scope: &SessionScope,
    ) -> Result<(), StoreError> {
        let result = self.update(&self.record_path(scope), |current| {
            let mut record = current.cloned().ok_or(StoreError::AlreadyRecorded)?;
            let live = record.live.as_mut().ok_or(StoreError::AlreadyRecorded)?;
            if live.instance != self.instance {
                return Err(StoreError::Conflict);
            }
            let background = live
                .provider_background
                .as_mut()
                .ok_or(StoreError::AlreadyRecorded)?;
            let journal = background
                .authorization
                .as_mut()
                .ok_or(StoreError::AlreadyRecorded)?;
            journal
                .control
                .get_or_insert(crate::authorization_context::Control {
                    cause: "host_shutdown".into(),
                    source_id: None,
                });
            // Shutdown revokes this connection's authority, not durable child
            // results or the pending parent. A new host must reconcile them.
            background
                .refresh_authorization()
                .map_err(|_| StoreError::Unreadable)?;
            background.validate().map_err(|_| StoreError::Unreadable)?;
            Ok(record)
        });
        match result {
            Err(StoreError::AlreadyRecorded) => Ok(()),
            result => result,
        }
    }
    /// No source is acknowledged by ingress before this durable fence is stored.
    pub(crate) fn accept_authorization_input(
        &self,
        scope: &SessionScope,
        input: SavedEvent,
        control: Option<&str>,
    ) -> Result<bool, StoreError> {
        let result = self.update(&self.record_path(scope), |current| {
            let mut record = current.cloned().unwrap_or_else(|| {
                let mut record = self.tombstone(None);
                record.reason = Some(SessionReason::FirstInThread);
                record
            });
            let id = input.event.id.to_hex();
            let known = record
                .authorization_ingress
                .pending
                .iter()
                .chain(
                    record
                        .live
                        .as_ref()
                        .and_then(|live| live.provider_background.as_ref())
                        .map(|background| background.events.as_slice())
                        .unwrap_or_default(),
                )
                .find(|saved| saved.event.id == input.event.id);
            let conflict = known.is_some_and(|saved| saved != &input);
            let ingress = &mut record.authorization_ingress;
            if input.event.verify().is_ok() && ingress.seen_ids.contains(&id) {
                if !conflict {
                    return Err(StoreError::AlreadyRecorded);
                }
                ingress.incomplete = true;
            }
            ingress.sequence = ingress
                .sequence
                .checked_add(1)
                .ok_or(StoreError::Unreadable)?;
            if input.event.verify().is_err()
                || ingress.pending.len() >= 50
                || serde_json::to_vec(&ingress.pending)
                    .map_err(|_| StoreError::Unreadable)?
                    .len()
                    + serde_json::to_vec(&input)
                        .map_err(|_| StoreError::Unreadable)?
                        .len()
                    > 64 * 1024
            {
                ingress.incomplete = true;
            } else {
                ingress.pending.push(input.clone());
            }
            if ingress.seen_ids.len() >= 2048 {
                ingress.seen_ids.remove(0);
                ingress.incomplete = true;
            }
            ingress.seen_ids.push(id);
            if let Some(background) = record
                .live
                .as_mut()
                .and_then(|live| live.provider_background.as_mut())
                .filter(|background| {
                    background.scope == *scope
                        && background.authorization.is_some()
                        && background.terminal_reason.is_none()
                        && background.cancellation.is_none()
                })
            {
                background
                    .accept_authorization_source(input.clone())
                    .map_err(|_| StoreError::Unreadable)?;
                if let Some(journal) = &mut background.authorization {
                    journal.ingress_sequence = ingress.sequence;
                    if let Some(reason) = control {
                        journal
                            .control
                            .get_or_insert(crate::authorization_context::Control {
                                cause: reason.into(),
                                source_id: Some(input.event.id.to_hex()),
                            });
                    }
                    if ingress.incomplete {
                        journal.fenced_reason = Some("source_coverage_incomplete".into());
                    }
                }
                background
                    .refresh_authorization()
                    .map_err(|_| StoreError::Unreadable)?;
                background.validate().map_err(|_| StoreError::Unreadable)?;
                ingress.pending.clear();
                ingress.incomplete = false;
            }
            Ok(record)
        });
        match result {
            Ok(()) => Ok(true),
            Err(StoreError::AlreadyRecorded) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// A current receipt admits this one attempt's freshness, not its action.
    pub(crate) fn check_authorization_admission(
        &self,
        scope: &SessionScope,
        lease: Uuid,
        request: &Admission,
    ) -> Result<serde_json::Value, StoreError> {
        if !request.valid() {
            return Err(StoreError::Conflict);
        }
        let mut status = "stale";
        let mut replayed = false;
        self.update(&self.record_path(scope), |current| {
            let mut record = owned_record(current, lease)?;
            let live = record.live.as_mut().ok_or(StoreError::Conflict)?;
            let background = live
                .provider_background
                .as_mut()
                .ok_or(StoreError::Conflict)?;
            if background.scope != *scope
                || background.identity != request.identity
                || live.session_id != request.identity.session_id
            {
                return Err(StoreError::Conflict);
            }
            let expired = background.expired();
            let revoked = record
                .authorization_revocation
                .as_ref()
                .is_some_and(|revoked| revoked.identity == request.identity);
            let snapshot = background
                .authorization
                .as_ref()
                .ok_or(StoreError::Conflict)?
                .snapshot(background);
            let journal = background
                .authorization
                .as_mut()
                .ok_or(StoreError::Conflict)?;
            if let Some(saved) = journal.admissions.get(&request.check_id) {
                if saved != request {
                    journal.fenced_reason = Some("check_id_conflict".into());
                } else {
                    replayed = true;
                }
            }
            if background.needs_user.is_some()
                || record
                    .needs_user
                    .as_ref()
                    .is_some_and(|receipt| receipt.identity == request.identity)
            {
                status = "needs_user";
            } else if journal.fenced_reason.is_none()
                && !expired
                && !revoked
                && snapshot["state"] == "active"
                && journal.ingress_sequence == record.authorization_ingress.sequence
                && journal.revision == request.revision
                && journal.receipt_revision == request.revision
                && (replayed || journal.admissions.len() < 128)
            {
                status = "current";
                journal.admissions.insert(request.check_id, request.clone());
            }
            background
                .refresh_authorization()
                .map_err(|_| StoreError::Unreadable)?;
            background.validate().map_err(|_| StoreError::Unreadable)?;
            Ok(record)
        })?;
        Ok(
            serde_json::json!({"identity":request.identity,"revision":request.revision,"check_id":request.check_id,
            "call_id":request.call_id,"fingerprint":request.fingerprint,"status":status,"replayed":replayed,"durable":true}),
        )
    }
}
