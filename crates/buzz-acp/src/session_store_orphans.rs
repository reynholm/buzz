//! Startup reconciliation of durable Started markers whose OS owner is dead.
use super::*;

impl SessionStore {
    /// Reconcile orphan receipts without reexecuting their external effect.
    /// Each result is safe for a signed reaction to the original command ID.
    pub(crate) fn sweep_orphan_controls(&self, limit: usize) -> Result<Vec<String>, StoreError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let channels = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(StoreError::Io(error)),
        };
        let mut finished = Vec::new();
        for channel in channels {
            let channel = channel.map_err(StoreError::Io)?;
            if channel
                .file_name()
                .to_str()
                .and_then(|name| Uuid::parse_str(name).ok())
                .is_none()
            {
                continue;
            }
            for entry in fs::read_dir(channel.path()).map_err(StoreError::Io)? {
                let path = entry.map_err(StoreError::Io)?.path();
                if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                    continue;
                }
                let Some(record) =
                    read_record(&path, &self.namespace).map_err(|()| StoreError::Unreadable)?
                else {
                    continue;
                };
                let orphans: Vec<_> = record
                    .control_receipts
                    .iter()
                    .filter(|receipt| {
                        receipt.state == ControlReceiptState::Started
                            && receipt.owner_instance != self.instance
                            && !self.instance_alive(receipt.owner_instance)
                    })
                    .take(limit.saturating_sub(finished.len()))
                    .cloned()
                    .collect();
                if orphans.is_empty() {
                    continue;
                }
                self.update_with_quarantine(
                    &path,
                    |current| {
                        let mut record = current.cloned().ok_or(StoreError::Conflict)?;
                        for orphan in &orphans {
                            let Some(receipt) =
                                record.control_receipts.iter_mut().find(|receipt| {
                                    receipt.command_id == orphan.command_id && *receipt == orphan
                                })
                            else {
                                return Err(StoreError::Conflict);
                            };
                            receipt.state = ControlReceiptState::Finished(Err(
                                buzz_core::session_control::ControlFailure::Unconfirmed,
                            ));
                        }
                        Ok(record)
                    },
                    false,
                )?;
                finished.extend(orphans.into_iter().map(|receipt| receipt.command_id));
                if finished.len() == limit {
                    return Ok(finished);
                }
            }
        }
        Ok(finished)
    }
}
