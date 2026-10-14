//! Durable final-report publication. The lock protects the entire bounded snapshot.
use super::messages;
use crate::{client::BuzzClient, error::CliError, validate};
use fs2::FileExt;
use nostr::Event;
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
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(directory) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && directory.is_dir() => {}
            Err(e) => return Err(error(e)),
        }
        if let Some(parent) = directory.parent() {
            sync_directory(parent)?;
        }
    }
    Ok(())
}

async fn lock(dir: &Path) -> Result<File, CliError> {
    durable_directory(dir)?;
    // Another creator can expose this directory before syncing its parent.
    // Sync even an existing directory's link before any publisher can send.
    if let Some(parent) = dir.parent() {
        sync_directory(parent)?;
    }
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
                version: 1,
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
    let journal: Journal = serde_json::from_slice(&bytes).map_err(error)?;
    if journal.version != 1 || journal.entries.len() > MAX_RECORDS {
        return Err(error("unsupported version or record limit exceeded"));
    }
    for (id, record) in &journal.entries {
        if serde_json::to_vec(record).map_err(error)?.len() > MAX_RECORD_BYTES {
            return Err(error("record exceeds 512 KiB"));
        }
        record.event.verify().map_err(error)?;
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
            || markers.reply.as_deref() != Some(&record.parent)
            || !tag("h", &record.channel)
            || !tag("buzz-delivery", &record.key)
        {
            return Err(error("record identity, signature or route is invalid"));
        }
        validate::validate_content_size(&record.event.content)?;
        validate::validate_hex64(&record.parent)?;
    }
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

fn acknowledgment(raw: &str, event: &Event) -> Result<serde_json::Value, CliError> {
    let response: serde_json::Value = serde_json::from_str(raw).map_err(error)?;
    if response["accepted"] != true || response["event_id"] != event.id.to_hex() {
        return Err(error("relay did not acknowledge the saved event ID"));
    }
    Ok(
        serde_json::json!({"event_id":event.id.to_hex(),"accepted":true,"message":response["message"].as_str().unwrap_or(""),"mention_pubkeys":messages::event_mention_pubkeys(event)}),
    )
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
    let parent = validate::parse_event_id(parent)?.to_hex();
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
        let parent_event = messages::fetch_event(client, &parent).await?;
        if messages::channel_id_from_event(&parent, &parent_event)?.to_string() != channel {
            return Err(CliError::Conflict(
                "reply parent belongs to another channel".into(),
            ));
        }
        let event = messages::prepare_message(
            client,
            messages::SendMessageParams {
                channel_id: channel.clone(),
                content: content.into(),
                kind: Some(9),
                reply_to: Some(parent.clone()),
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
        return Ok(
            serde_json::json!({"event_id":record.event.id.to_hex(),"accepted":true,"message":"managed delivery already confirmed","mention_pubkeys":messages::event_mention_pubkeys(&record.event)}),
        );
    }
    if record.attempts >= MAX_ATTEMPTS {
        return Err(error(format!("terminal: delivery {key} exhausted {MAX_ATTEMPTS} managed submissions; signed evidence retained in {}", dir.display())));
    }
    record.attempts += 1;
    let event = record.event.clone();
    write(&dir, &journal)?;
    let result = client
        .submit_event(event.clone())
        .await
        .and_then(|raw| acknowledgment(&raw, &event));
    let record = journal
        .entries
        .get_mut(&id)
        .ok_or_else(|| error("record disappeared"))?;
    match result {
        Ok(output) => {
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
