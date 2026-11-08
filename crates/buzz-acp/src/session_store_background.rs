//! Recover background collection only from current authorized, dead-owner records.
use super::*;
use crate::provider_background::Record;

impl SessionStore {
    pub(crate) fn commit_background(
        &self,
        scope: &SessionScope,
        lease: Uuid,
        background: Record,
    ) -> Result<(), StoreError> {
        self.update(&self.record_path(scope), |current| {
            let mut record = owned_record(current, lease)?;
            if background.cancellation.is_some() {
                record.background_cancellation = background.cancellation.clone();
            }
            if let Some(live) = record.live.as_mut() {
                live.provider_background = Some(Box::new(background));
            }
            Ok(record)
        })
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
                if !background.outstanding()
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
