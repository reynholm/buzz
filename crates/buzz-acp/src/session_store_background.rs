//! Recover background collection only from current authorized, dead-owner records.
use super::*;
use crate::provider_background::Record;

impl SessionStore {
    pub(crate) fn commit_needs_user(
        &self,
        receipt: crate::authorization_context::NeedsUserReceipt,
        lease: Uuid,
    ) -> Result<(), StoreError> {
        self.update(&self.record_path(&receipt.scope), |current| {
            let mut record = owned_record(current, lease)?;
            if record
                .live
                .as_ref()
                .is_none_or(|live| live.session_id != receipt.identity.session_id)
            {
                return Err(StoreError::Conflict);
            }
            record.needs_user = Some(receipt.clone());
            Ok(record)
        })
    }

    pub(crate) fn commit_background(
        &self,
        scope: &SessionScope,
        lease: Uuid,
        background: Record,
    ) -> Result<(), StoreError> {
        self.update(&self.record_path(scope), |current| {
            let mut record = owned_record(current, lease)?;
            let mut background = background.clone();
            let saved = record
                .live
                .as_ref()
                .and_then(|live| live.provider_background.as_ref())
                .filter(|saved| {
                    saved.identity == background.identity && saved.scope == background.scope
                })
                .cloned();
            if let Some(saved) = &saved {
                background
                    .merge_authorization(saved)
                    .map_err(|_| StoreError::Unreadable)?;
            } else if background.authorization.is_some()
                && !record.authorization_ingress.pending.is_empty()
            {
                // The batch can contain only a suffix of the accepted input.
                // Preserve ingress order before adding any batch-only evidence.
                let batch_sources = std::mem::take(&mut background.events);
                for source in record
                    .authorization_ingress
                    .pending
                    .iter()
                    .chain(&batch_sources)
                {
                    background
                        .accept_authorization_source(source.clone())
                        .map_err(|_| StoreError::Unreadable)?;
                }
            }
            if background.authorization.is_some() {
                if let Some(revoked) = record
                    .authorization_revocation
                    .as_ref()
                    .filter(|revoked| revoked.identity == background.identity)
                {
                    if let Some(journal) = &mut background.authorization {
                        journal.control = Some(crate::authorization_context::Control {
                            cause: revoked.cause.clone(),
                            source_id: revoked.source_id.clone(),
                        });
                    }
                }
                for source in &record.authorization_ingress.pending {
                    background
                        .accept_authorization_source(source.clone())
                        .map_err(|_| StoreError::Unreadable)?;
                }
                if let Some(journal) = &mut background.authorization {
                    journal.ingress_sequence = record.authorization_ingress.sequence;
                    if record.authorization_ingress.incomplete {
                        journal.fenced_reason = Some("source_coverage_incomplete".into());
                    }
                }
                record.authorization_ingress.pending.clear();
                record.authorization_ingress.incomplete = false;
                for source in &background.events {
                    let id = source.event.id.to_hex();
                    if !record.authorization_ingress.seen_ids.contains(&id) {
                        if record.authorization_ingress.seen_ids.len() >= 2048 {
                            record.authorization_ingress.seen_ids.remove(0);
                            if let Some(journal) = &mut background.authorization {
                                journal.initial_coverage = false;
                            }
                        }
                        record.authorization_ingress.seen_ids.push(id);
                    }
                }
            }
            background
                .refresh_authorization()
                .map_err(|_| StoreError::Unreadable)?;
            background.validate().map_err(|_| StoreError::Unreadable)?;
            if background.cancellation.is_some() {
                record.background_cancellation = background.cancellation.clone();
            }
            if let Some(live) = record.live.as_mut() {
                live.provider_background = Some(Box::new(background));
            }
            Ok(record)
        })
    }

    /// Finish an unbound recovery only while the exact provider owner is dead.
    /// The record lock serializes this identity/liveness check with new claims.
    pub(crate) fn finish_dead_provider_background(
        &self,
        expected: &Record,
        reason: &str,
    ) -> Result<(), StoreError> {
        expected.validate().map_err(|_| StoreError::Unreadable)?;
        let result = self.update(&self.record_path(&expected.scope), |current| {
            let mut record = current.ok_or(StoreError::Conflict)?.clone();
            let live = record.live.as_mut().ok_or(StoreError::Conflict)?;
            let background = live
                .provider_background
                .as_mut()
                .ok_or(StoreError::Conflict)?;
            if live.session_id != expected.identity.session_id
                || background.identity != expected.identity
                || background.scope != expected.scope
            {
                return Err(StoreError::Conflict);
            }
            if !background.recoverable() {
                return Err(StoreError::AlreadyRecorded);
            }
            if self.instance_liveness(live.instance) != InstanceLiveness::Dead {
                return Err(StoreError::Conflict);
            }
            background.parent_pending = false;
            background.terminal_reason = Some(reason.into());
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

    pub(crate) fn pending_provider_work(
        &self,
        channels: &[Uuid],
    ) -> Result<Vec<Record>, StoreError> {
        let mut found = Vec::new();
        for channel in channels {
            let entries = match fs::read_dir(self.channel_dir(*channel)) {
                Ok(entries) => entries,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(StoreError::Io(error)),
            };
            for (scanned, entry) in entries.enumerate() {
                if scanned >= 8192 {
                    return Err(StoreError::Busy);
                }
                let entry = entry.map_err(StoreError::Io)?;
                let path = entry.path();
                if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                    continue;
                }
                let record =
                    read_record(&path, &self.namespace).map_err(|_| StoreError::Unreadable)?;
                let Some(live) = record.and_then(|record| record.live) else {
                    continue;
                };
                let Some(background) = live.provider_background else {
                    continue;
                };
                if !background.recoverable()
                    || background.scope.channel_id() != *channel
                    || self.record_path(&background.scope) != path
                    || live.session_id != background.identity.session_id
                    || self.instance_liveness(live.instance) != InstanceLiveness::Dead
                {
                    continue;
                }
                background.validate().map_err(|_| StoreError::Unreadable)?;
                if found.len() >= 128 {
                    return Err(StoreError::Busy);
                }
                found.push(*background);
            }
        }
        Ok(found)
    }
}
