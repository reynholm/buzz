//! Startup reconciliation of durable Started markers whose OS owner is dead.
use super::*;

#[derive(Debug, Default)]
pub(crate) struct OrphanSweep {
    pub finished: Vec<String>,
    pub failures: usize,
    pub more: bool,
    pub revisit: bool,
}

impl OrphanSweep {
    fn failed(&mut self, path: &Path, error: impl std::fmt::Display) {
        self.failures = self.failures.saturating_add(1);
        tracing::warn!(target: "session_store", path = %path.display(), %error,
            "orphan reconciliation deferred for this record; other scopes continue");
    }
}

impl SessionStore {
    /// Reconcile through the same canonical reader/lock as normal store writes.
    /// Partial progress survives failures of other records; failures require retry.
    pub(crate) fn sweep_orphan_controls(&self, limit: usize) -> Result<OrphanSweep, StoreError> {
        let mut progress = OrphanSweep::default();
        let limit = limit.min(128);
        if limit == 0 {
            return Ok(progress);
        }
        let channels = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(progress),
            Err(error) => return Err(StoreError::Io(error)),
        };
        for channel in channels {
            let channel = match channel {
                Ok(channel) => channel,
                Err(error) => {
                    progress.failed(&self.dir, error);
                    continue;
                }
            };
            if channel
                .file_name()
                .to_str()
                .and_then(|name| Uuid::parse_str(name).ok())
                .is_none()
            {
                continue;
            }
            let entries = match fs::read_dir(channel.path()) {
                Ok(entries) => entries,
                Err(error) => {
                    progress.failed(&channel.path(), error);
                    continue;
                }
            };
            for entry in entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        progress.failed(&channel.path(), error);
                        continue;
                    }
                };
                let entry_path = entry.path();
                let Some(path) = canonical_key(&entry_path) else {
                    continue;
                };
                // A canonical file and its stale alias are one key. Skip the
                // alias when canonical exists; stale-only uses the normal reader.
                // No unbounded directory-wide key set is needed. If a concurrent
                // recovery creates canonical during iteration, the receipt CAS
                // and Finished state still prevent duplicate reconciliation.
                if entry_path != path {
                    match path.try_exists() {
                        Ok(true) => continue,
                        Ok(false) => {}
                        Err(error) => {
                            progress.failed(&path, error);
                            continue;
                        }
                    }
                }
                let record = match read_record(&path, &self.namespace) {
                    Ok(Some(record)) => record,
                    Ok(None) => continue,
                    Err(()) => {
                        progress.failed(&path, StoreError::Unreadable);
                        continue;
                    }
                };
                let orphans: Vec<_> = record
                    .control_receipts
                    .iter()
                    .filter(|receipt| {
                        if receipt.state != ControlReceiptState::Started
                            || receipt.owner_instance == self.instance
                        {
                            return false;
                        }
                        match self.instance_liveness(receipt.owner_instance) {
                            InstanceLiveness::Dead => true,
                            InstanceLiveness::Alive | InstanceLiveness::Unknown => {
                                // A live owner can die later; uncertainty can resolve.
                                // Neither is proof this reconciliation is complete.
                                progress.revisit = true;
                                false
                            }
                        }
                    })
                    .take(limit.saturating_sub(progress.finished.len()))
                    .cloned()
                    .collect();
                if orphans.is_empty() {
                    continue;
                }
                let result = self.update_with_quarantine(
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
                            // Recheck the live foreign-owner proof in the locked decision.
                            if self.instance_liveness(receipt.owner_instance)
                                != InstanceLiveness::Dead
                            {
                                return Err(StoreError::Conflict);
                            }
                            receipt.state = ControlReceiptState::Finished(Err(
                                buzz_core::session_control::ControlFailure::Unconfirmed,
                            ));
                        }
                        Ok(record)
                    },
                    false,
                );
                match result {
                    Ok(()) => progress
                        .finished
                        .extend(orphans.into_iter().map(|receipt| receipt.command_id)),
                    Err(error) => progress.failed(&path, error),
                }
                if progress.finished.len() == limit {
                    progress.more = true;
                    return Ok(progress);
                }
            }
        }
        Ok(progress)
    }
}

fn canonical_key(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    let key = name
        .strip_suffix(".json")
        .or_else(|| name.strip_suffix(".json.stale"))?;
    (key.len() == 64 && key.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| path.with_file_name(format!("{key}.json")))
}
