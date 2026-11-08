//! Runtime replay APIs share the original signed journal, lock and publisher.
use super::*;

/// Runtime schema is isolated from legacy manual-delivery journals by default.
pub(super) const DIRECTORY: &str = ".buzz-runtime-deliveries";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RuntimeDelivery {
    pub(super) purpose: String,
    profile: String,
    contract_revision: u32,
    pub(super) revoked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) scope: Option<String>,
}

impl RuntimeDelivery {
    pub(super) fn new(purpose: &str, scope: Option<&str>) -> Result<Self, CliError> {
        let value = Self {
            purpose: purpose.into(),
            profile: "b24_0_26_7_8".into(),
            contract_revision: 1,
            revoked: false,
            scope: scope.map(str::to_owned),
        };
        value.validate()?;
        Ok(value)
    }
    pub(super) fn validate(&self) -> Result<(), CliError> {
        if !matches!(self.purpose.as_str(), "answer" | "failure_notice")
            || self.profile != "b24_0_26_7_8"
            || self.contract_revision != 1
            || self
                .scope
                .as_deref()
                .is_some_and(|scope| !matches!(scope, "conversation" | "thread"))
        {
            return Err(error("unsupported runtime delivery metadata"));
        }
        Ok(())
    }
}

fn directory(directory: Option<&Path>) -> Result<PathBuf, CliError> {
    let dir = directory.unwrap_or_else(|| Path::new(DIRECTORY));
    Ok(if dir.is_absolute() {
        dir.to_owned()
    } else {
        std::env::current_dir().map_err(error)?.join(dir)
    })
}

/// Enumerate at most 32 eligible saved publications, without exposing content.
pub(crate) async fn pending(
    client: &BuzzClient,
    directory_arg: Option<&Path>,
) -> Result<serde_json::Value, CliError> {
    let dir = directory(directory_arg)?;
    let _lock = lock(&dir).await?;
    let journal = read(&dir)?;
    let relay = relay_namespace(client)?;
    let signer = client.keys().public_key().to_hex();
    let entries: Vec<_> = journal.entries.values().filter(|record| {
        record.relay == relay && record.signer == signer && record.confirmed_at.is_none()
            && record.attempts < MAX_ATTEMPTS && record.runtime.as_ref().is_some_and(|r| !r.revoked)
    }).take(32).map(|record| serde_json::json!({"delivery_key":record.key,"channel":record.channel,"parent":record.parent,"purpose":record.runtime.as_ref().map(|r| &r.purpose)})).collect();
    Ok(serde_json::json!({"pending":entries}))
}

/// No content/route input exists: only the validated original record can send.
pub(crate) async fn retry(
    client: &BuzzClient,
    key: &str,
    directory_arg: Option<&Path>,
) -> Result<serde_json::Value, CliError> {
    let dir = directory(directory_arg)?;
    let _lock = lock(&dir).await?;
    let mut journal = read(&dir)?;
    let key = uuid::Uuid::parse_str(key).map_err(error)?.to_string();
    let id = namespace(
        &relay_namespace(client)?,
        &client.keys().public_key().to_hex(),
        &key,
    )?;
    let record = journal
        .entries
        .get(&id)
        .filter(|r| r.runtime.is_some())
        .ok_or_else(|| error("saved runtime publication not found"))?;
    if record.runtime.as_ref().is_some_and(|r| r.revoked) {
        return Err(CliError::Auth(
            "runtime publication was explicitly revoked".into(),
        ));
    }
    check_membership(client, &record.channel).await?;
    publish_record(client, &dir, &mut journal, &id).await
}

/// Revocation does not require current membership and never erases evidence.
pub(crate) async fn revoke(
    client: &BuzzClient,
    key: &str,
    directory_arg: Option<&Path>,
) -> Result<serde_json::Value, CliError> {
    let dir = directory(directory_arg)?;
    let _lock = lock(&dir).await?;
    let mut journal = read(&dir)?;
    let key = uuid::Uuid::parse_str(key).map_err(error)?.to_string();
    let id = namespace(
        &relay_namespace(client)?,
        &client.keys().public_key().to_hex(),
        &key,
    )?;
    if let Some(record) = journal.entries.get_mut(&id) {
        let runtime = record
            .runtime
            .as_mut()
            .ok_or_else(|| error("saved publication is not runtime-managed"))?;
        runtime.revoked = true;
        record.last_error = Some("runtime publication explicitly revoked".into());
        write(&dir, &journal)?;
    }
    Ok(serde_json::json!({"delivery_key":key,"revoked":true}))
}

/// Revoke matching saved records even after all execution owners have gone.
/// This operates only on the existing own-signer/relay journal under its lock.
pub(crate) async fn revoke_scope(
    client: &BuzzClient,
    channel: &str,
    thread_root: Option<&str>,
    directory_arg: Option<&Path>,
) -> Result<serde_json::Value, CliError> {
    let channel = validate::parse_uuid(channel)?.to_string();
    let root = thread_root
        .map(validate::parse_event_id)
        .transpose()?
        .map(|id| id.to_hex());
    let dir = directory(directory_arg)?;
    let _lock = lock(&dir).await?;
    let mut journal = read(&dir)?;
    let relay = relay_namespace(client)?;
    let signer = client.keys().public_key().to_hex();
    let mut revoked = 0;
    for record in journal.entries.values_mut() {
        if record.relay != relay
            || record.signer != signer
            || record.channel != channel
            || record.confirmed_at.is_some()
        {
            continue;
        }
        let Some(runtime) = &mut record.runtime else {
            continue;
        };
        let matches = match (runtime.scope.as_deref(), root.as_deref()) {
            (Some("conversation"), None) => true,
            (Some("thread"), Some(root)) => record.parent == root,
            // Scope-less older runtime records retain conservative original
            // channel/reply-route cancellation, never any sending authority.
            (None, None) => true,
            (None, Some(root)) => record.parent == root,
            _ => false,
        };
        if matches && !runtime.revoked {
            runtime.revoked = true;
            record.last_error =
                Some("runtime publication explicitly revoked by scope control".into());
            revoked += 1;
        }
    }
    if revoked > 0 {
        write(&dir, &journal)?;
    }
    Ok(serde_json::json!({"revoked":revoked}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn runtime_final_pending_index_filters_namespace_legacy_and_revoked_records() {
        let (relay, url, dir) = crate::delivery_tests::fixture().await;
        let client = BuzzClient::new(url.clone(), relay.keys.clone(), None, None).unwrap();
        *relay.ack.lock().unwrap() = "malformed";
        let runtime_key = uuid::Uuid::new_v4().to_string();
        for purpose in [None, Some("answer")] {
            let key = if purpose.is_some() {
                runtime_key.clone()
            } else {
                uuid::Uuid::new_v4().to_string()
            };
            assert!(deliver(
                &client,
                &relay.channel,
                "",
                "saved",
                &key,
                Some(dir.path()),
                vec![],
                purpose,
                None,
            )
            .await
            .is_err());
        }
        let listed = pending(&client, Some(dir.path())).await.unwrap();
        assert_eq!(listed["pending"].as_array().unwrap().len(), 1);
        assert_eq!(listed["pending"][0]["delivery_key"], runtime_key);
        assert!(listed["pending"][0].get("content").is_none());
        let other = BuzzClient::new(url, nostr::Keys::generate(), None, None).unwrap();
        assert!(pending(&other, Some(dir.path())).await.unwrap()["pending"]
            .as_array()
            .unwrap()
            .is_empty());
        revoke(&client, &runtime_key, Some(dir.path()))
            .await
            .unwrap();
        assert!(pending(&client, Some(dir.path())).await.unwrap()["pending"]
            .as_array()
            .unwrap()
            .is_empty());
        let mut journal: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.path().join("journal.json")).unwrap())
                .unwrap();
        for record in journal["entries"].as_object_mut().unwrap().values_mut() {
            if record["runtime"].is_object() {
                record["runtime"]["contract_revision"] = serde_json::json!(2);
            }
        }
        std::fs::write(
            dir.path().join("journal.json"),
            serde_json::to_vec(&journal).unwrap(),
        )
        .unwrap();
        assert!(
            pending(&client, Some(dir.path())).await.is_err(),
            "unsupported metadata must fail closed"
        );
    }
}
