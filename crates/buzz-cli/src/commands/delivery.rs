//! Durable final-report publication. The lock protects the entire bounded snapshot.
use super::messages;
use crate::{client::BuzzClient, error::CliError, validate};
use buzz_core::managed_delivery::{same_report, RecoveryRequest, ReportStamp, RECEIPT_PREFIX};
use fs2::FileExt;
use nostr::{Event, EventBuilder, Kind, Tag};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::time::Instant;

const MAX_RECORDS: usize = 1024;
const MAX_RECORD_BYTES: usize = 512 * 1024;
const MAX_JOURNAL_BYTES: usize = 16 * 1024 * 1024;
const MAX_ATTEMPTS: u8 = 8;
const RETENTION_SECS: u64 = 7 * 24 * 60 * 60;

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    version: u8,
    entries: BTreeMap<String, Record>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    relay: String,
    signer: String,
    key: String,
    channel: String,
    parent: String,
    event: Event,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recovery_candidate: Option<ReportStamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    canonical_event: Option<ReportStamp>,
    attempts: u8,
    confirmed_at: Option<u64>,
    last_error: Option<String>,
}

fn error(detail: impl std::fmt::Display) -> CliError {
    CliError::Other(format!("managed delivery journal: {detail}"))
}

fn now() -> Result<u64, CliError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(error)?
        .as_secs())
}

fn relay_namespace(client: &BuzzClient) -> Result<String, CliError> {
    let mut url =
        url::Url::parse(&crate::client::normalize_relay_url(client.relay_url())).map_err(error)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(CliError::Usage(
            "managed delivery requires an HTTP(S) relay URL without credentials, query or fragment"
                .into(),
        ));
    }
    let path = url.path().trim_end_matches('/').to_owned();
    url.set_path(&path);
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

fn namespace(relay: &str, signer: &str, key: &str) -> Result<String, CliError> {
    let bytes = serde_json::to_vec(&(relay, signer, key)).map_err(error)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn private_file(path: &Path) -> Result<File, CliError> {
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(error)
}

fn sync_directory(path: &Path) -> Result<(), CliError> {
    File::open(path)
        .and_then(|dir| dir.sync_all())
        .map_err(error)
}

fn durable_directory(dir: &Path) -> Result<(), CliError> {
    let mut missing = Vec::new();
    let mut ancestor = dir;
    while !ancestor.is_dir() {
        if missing.len() >= 128 {
            return Err(error("journal directory nesting exceeds 128 levels"));
        }
        missing.push(ancestor);
        ancestor = ancestor
            .parent()
            .ok_or_else(|| error("journal directory has no existing ancestor"))?;
    }
    for directory in missing.into_iter().rev() {
        let builder = fs::DirBuilder::new();
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = builder;
            builder.mode(0o700);
            builder
        };
        match builder.create(directory) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && directory.is_dir() => {}
            Err(e) => return Err(error(e)),
        }
    }
    // A concurrent creator may expose the whole chain before syncing any link.
    // Every publisher must persist every ancestor link before it can send.
    for (depth, parent) in dir.ancestors().skip(1).enumerate() {
        if depth >= 128 {
            return Err(error("journal directory nesting exceeds 128 levels"));
        }
        sync_directory(parent)?;
    }
    Ok(())
}

async fn lock(dir: &Path) -> Result<File, CliError> {
    durable_directory(dir)?;
    let file = private_file(&dir.join("journal.lock"))?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(file),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(25)).await
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                return Err(CliError::ManagedDeliveryPending(
                    "journal is busy; retry the same managed key and original route".into(),
                ))
            }
            Err(e) => return Err(error(format!("could not acquire journal lock: {e}"))),
        }
    }
}

fn read(dir: &Path) -> Result<Journal, CliError> {
    let file = match File::open(dir.join("journal.json")) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Journal {
                version: 2,
                ..Default::default()
            })
        }
        Err(e) => return Err(error(e)),
    };
    if file.metadata().map_err(error)?.len() > MAX_JOURNAL_BYTES as u64 {
        return Err(error("journal exceeds 16 MiB"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_JOURNAL_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(error)?;
    if bytes.len() > MAX_JOURNAL_BYTES {
        return Err(error("journal exceeds 16 MiB"));
    }
    let mut journal: Journal = serde_json::from_slice(&bytes).map_err(error)?;
    if !matches!(journal.version, 1 | 2) || journal.entries.len() > MAX_RECORDS {
        return Err(error("unsupported version or record limit exceeded"));
    }
    for (id, record) in &journal.entries {
        if serde_json::to_vec(record).map_err(error)?.len() > MAX_RECORD_BYTES {
            return Err(error("record exceeds 512 KiB"));
        }
        record.event.verify().map_err(error)?;
        for extra in [
            record.recovery_candidate.as_ref(),
            record.canonical_event.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            let restored = extra.restore(&record.event).map_err(error)?;
            if !same_report(&record.event, &restored) {
                return Err(error(
                    "recovery/canonical report changed the original payload or route",
                ));
            }
        }
        let markers = buzz_core::nip10::parse_thread_markers_from_parts(
            record.event.tags.iter().map(|t| t.as_slice()),
        );
        let tag = |name: &str, value: &str| {
            record
                .event
                .tags
                .iter()
                .filter(|t| t.as_slice().first().map(String::as_str) == Some(name))
                .map(|t| t.as_slice())
                .collect::<Vec<_>>()
                == vec![&[name.to_owned(), value.to_owned()][..]]
        };
        if record.attempts > MAX_ATTEMPTS
            || record.event.kind.as_u16() != 9
            || record.event.pubkey.to_hex() != record.signer
            || namespace(&record.relay, &record.signer, &record.key)? != *id
            || uuid::Uuid::parse_str(&record.key)
                .map_err(error)?
                .to_string()
                != record.key
            || uuid::Uuid::parse_str(&record.channel)
                .map_err(error)?
                .to_string()
                != record.channel
            || if record.parent.is_empty() {
                record.event.tags.iter().any(|tag| tag.as_slice()[0] == "e")
            } else {
                markers.reply.as_deref() != Some(&record.parent)
            }
            || !tag("h", &record.channel)
            || !tag("buzz-delivery", &record.key)
        {
            return Err(error("record identity, signature or route is invalid"));
        }
        validate::validate_content_size(&record.event.content)?;
        if !record.parent.is_empty() {
            validate::validate_hex64(&record.parent)?;
        }
    }
    journal.version = 2;
    Ok(journal)
}

fn write(dir: &Path, journal: &Journal) -> Result<(), CliError> {
    let bytes = serde_json::to_vec(journal).map_err(error)?;
    if journal.entries.len() > MAX_RECORDS || bytes.len() > MAX_JOURNAL_BYTES {
        return Err(error(
            "journal capacity reached; pending records are retained",
        ));
    }
    if journal
        .entries
        .values()
        .any(|r| serde_json::to_vec(r).map_or(true, |b| b.len() > MAX_RECORD_BYTES))
    {
        return Err(error("record exceeds 512 KiB"));
    }
    // A fixed temporary name is safe under the one lock, including crash residue.
    let temporary = dir.join("journal.pending");
    let mut file = private_file(&temporary)?;
    file.set_len(0).map_err(error)?;
    file.write_all(&bytes).map_err(error)?;
    file.sync_all().map_err(error)?;
    fs::rename(temporary, dir.join("journal.json")).map_err(error)?;
    sync_directory(dir)
}

async fn check_membership(client: &BuzzClient, channel: &str) -> Result<(), CliError> {
    let raw = client
        .query(&serde_json::json!({"kinds":[39002],"#d":[channel],"limit":1}))
        .await?;
    let events: Vec<Event> = serde_json::from_str(&raw).map_err(error)?;
    let event = events
        .first()
        .ok_or_else(|| error("current channel membership is unavailable"))?;
    event.verify().map_err(error)?;
    if events.len() != 1
        || event.kind.as_u16() != 39002
        || !event
            .tags
            .iter()
            .any(|tag| tag.as_slice() == ["d", channel])
    {
        return Err(error(
            "membership snapshot does not identify the original channel",
        ));
    }
    let signer = client.keys().public_key().to_hex();
    if !event.tags.iter().any(|tag| {
        let parts = tag.as_slice();
        parts.first().map(String::as_str) == Some("p") && parts.get(1) == Some(&signer)
    }) {
        return Err(CliError::Auth("original channel membership was revoked; delivery is retained and will not be rerouted".into()));
    }
    Ok(())
}

fn acknowledgment(
    raw: &str,
    original: &Event,
    recovery: bool,
) -> Result<(serde_json::Value, Event), CliError> {
    let response: serde_json::Value = serde_json::from_str(raw).map_err(error)?;
    if response["accepted"] != true {
        return Err(error("relay did not accept the managed report"));
    }
    let message = response["message"].as_str().unwrap_or("");
    let chosen = if let Some(encoded) = message.strip_prefix(RECEIPT_PREFIX) {
        let event: Event = serde_json::from_str(encoded).map_err(error)?;
        event.verify().map_err(error)?;
        if !same_report(original, &event) {
            return Err(error(
                "canonical receipt changed the original signed report",
            ));
        }
        event
    } else if !recovery {
        original.clone()
    } else {
        return Err(error(
            "relay does not support atomic recovery; signed evidence retained",
        ));
    };
    if response["event_id"] != chosen.id.to_hex() {
        return Err(error("relay did not acknowledge the canonical event ID"));
    }
    Ok((
        serde_json::json!({"event_id":chosen.id.to_hex(),"accepted":true,"message":if recovery {"managed report recovered"} else {"managed report acknowledged"},"mention_pubkeys":messages::event_mention_pubkeys(&chosen)}),
        chosen,
    ))
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn deliver(
    client: &BuzzClient,
    channel: &str,
    parent: &str,
    content: &str,
    key: &str,
    directory: Option<&Path>,
    mentions: Vec<String>,
) -> Result<serde_json::Value, CliError> {
    let key = uuid::Uuid::parse_str(key)
        .map_err(|_| CliError::Usage("--delivery-key must be a UUID".into()))?
        .to_string();
    let channel = validate::parse_uuid(channel)?.to_string();
    let parent = if parent.is_empty() {
        String::new()
    } else {
        validate::parse_event_id(parent)?.to_hex()
    };
    let dir: PathBuf = directory
        .map(Path::to_owned)
        .unwrap_or_else(|| PathBuf::from(".buzz-deliveries"));
    let dir = if dir.is_absolute() {
        dir
    } else {
        std::env::current_dir().map_err(error)?.join(dir)
    };
    let relay = relay_namespace(client)?;
    let signer = client.keys().public_key().to_hex();
    let id = namespace(&relay, &signer, &key)?;
    let _lock = lock(&dir).await?;
    let mut journal = read(&dir)?;
    let timestamp = now()?;
    journal.entries.retain(|_, record| {
        record
            .confirmed_at
            .is_none_or(|confirmed| timestamp.saturating_sub(confirmed) < RETENTION_SECS)
    });
    if let Some(record) = journal.entries.get(&id) {
        if record.channel != channel || record.parent != parent {
            return Err(CliError::Conflict(
                "delivery key is already bound to another original route".into(),
            ));
        }
    }
    check_membership(client, &channel).await?;
    if !journal.entries.contains_key(&id) {
        if journal.entries.len() >= MAX_RECORDS {
            return Err(error(
                "1024-record capacity reached; pending records are retained",
            ));
        }
        if !parent.is_empty() {
            let parent_event = messages::fetch_event(client, &parent).await?;
            if messages::channel_id_from_event(&parent, &parent_event)?.to_string() != channel {
                return Err(CliError::Conflict(
                    "reply parent belongs to another channel".into(),
                ));
            }
        }
        let event = messages::prepare_message(
            client,
            messages::SendMessageParams {
                channel_id: channel.clone(),
                content: content.into(),
                kind: Some(9),
                reply_to: (!parent.is_empty()).then(|| parent.clone()),
                broadcast: false,
                files: vec![],
                mentions,
            },
            Some(&key),
        )
        .await?;
        journal.entries.insert(
            id.clone(),
            Record {
                relay,
                signer,
                key: key.clone(),
                channel,
                parent,
                event,
                recovery_candidate: None,
                canonical_event: None,
                attempts: 0,
                confirmed_at: None,
                last_error: None,
            },
        );
        write(&dir, &journal)?;
    }
    let record = journal
        .entries
        .get_mut(&id)
        .ok_or_else(|| error("record disappeared"))?;
    if record.confirmed_at.is_some() {
        let chosen = match record.canonical_event.as_ref() {
            Some(stamp) => stamp.restore(&record.event).map_err(error)?,
            None => record.event.clone(),
        };
        return Ok(
            serde_json::json!({"event_id":chosen.id.to_hex(),"accepted":true,"message":"managed delivery already confirmed","mention_pubkeys":messages::event_mention_pubkeys(&chosen)}),
        );
    }
    if record.attempts >= MAX_ATTEMPTS {
        return Err(error(format!("terminal: delivery {key} exhausted {MAX_ATTEMPTS} managed submissions; signed evidence retained in {}", dir.display())));
    }
    let original = record.event.clone();
    let recovery_now = now()?;
    let recovery = recovery_now.saturating_sub(original.created_at.as_secs()) > 600;
    let submission = if recovery {
        if record
            .recovery_candidate
            .as_ref()
            .is_none_or(|e| recovery_now.saturating_sub(e.created_at) > 600)
        {
            let signed = EventBuilder::new(original.kind, &original.content)
                .tags(original.tags.clone())
                .allow_self_tagging()
                .sign_with_keys(client.keys())
                .map_err(error)?;
            record.recovery_candidate = Some(ReportStamp::from(&signed));
        }
        let candidate = record
            .recovery_candidate
            .clone()
            .ok_or_else(|| error("recovery candidate disappeared"))?;
        let request = RecoveryRequest {
            original: original.clone(),
            candidate,
        };
        let body = serde_json::to_string(&request).map_err(error)?;
        if body.len() > buzz_core::managed_delivery::MAX_RECOVERY_BYTES {
            return Err(error(
                "recovery payload exceeds protocol limit; original retained",
            ));
        }
        client.sign_event(
            EventBuilder::new(
                Kind::Custom(buzz_core::kind::KIND_MANAGED_DELIVERY_RECOVERY as u16),
                body,
            )
            .tag(Tag::parse(["h", &record.channel]).map_err(error)?),
        )?
    } else {
        original.clone()
    };
    if serde_json::to_vec(&submission).map_err(error)?.len() > 1024 * 1024 {
        return Err(error(
            "recovery envelope exceeds transport limit; original retained",
        ));
    }
    record.attempts += 1;
    write(&dir, &journal)?;
    let result = client
        .submit_event(submission)
        .await
        .and_then(|raw| acknowledgment(&raw, &original, recovery));
    let record = journal
        .entries
        .get_mut(&id)
        .ok_or_else(|| error("record disappeared"))?;
    match result {
        Ok((output, chosen)) => {
            record.canonical_event =
                (chosen.id != record.event.id).then(|| ReportStamp::from(&chosen));
            record.confirmed_at = Some(now()?);
            record.last_error = None;
            write(&dir, &journal)?;
            Ok(output)
        }
        Err(e) => {
            record.last_error = Some(e.to_string().chars().take(2048).collect());
            let state = if record.attempts == MAX_ATTEMPTS {
                "terminal"
            } else {
                "unconfirmed; retry this same key and route"
            };
            write(&dir, &journal)?;
            let detail = format!(
                "{state}: delivery {key}: {e}; signed event retained in {}",
                dir.display()
            );
            if matches!(
                e,
                CliError::Relay {
                    status: 401 | 403,
                    ..
                } | CliError::Auth(_)
            ) {
                Err(e)
            } else if state == "terminal" {
                Err(error(detail))
            } else {
                Err(CliError::ManagedDeliveryPending(detail))
            }
        }
    }
}
