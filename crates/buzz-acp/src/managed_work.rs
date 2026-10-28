//! Kimi work uses the existing CLI journal and managed publication contract.
use crate::{queue::LogicalWork, relay::RestClient};
use buzz_core::managed_delivery::{delivery_key, ReportStamp};
use nostr::Event;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read, path::Path, process::Stdio, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const MAX_JOURNAL_BYTES: u64 = 16 * 1024 * 1024;

#[cfg(test)]
#[path = "managed_work_tests.rs"]
pub(crate) mod tests;

#[derive(Deserialize)]
struct Journal {
    version: u8,
    entries: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct Record {
    relay: String,
    signer: String,
    key: String,
    channel: String,
    parent: String,
    event: Event,
    confirmed_at: Option<u64>,
    canonical_event: Option<ReportStamp>,
}

struct Receipt {
    event: Option<String>,
    content_hash: [u8; 32],
}

fn parent(work: &LogicalWork) -> Option<String> {
    let tags = work.reply_tags();
    tags.root_event_id.or(tags.parent_event_id)
}

/// Inspect only a bounded, atomically replaced snapshot. The CLI owns writes
/// and locking; this reader never edits, reconstructs or confirms a receipt.
pub(crate) async fn confirmed_report(
    rest: &RestClient,
    work: &LogicalWork,
) -> Result<Option<String>, &'static str> {
    Ok(inspect_receipt(rest, work)
        .await?
        .and_then(|receipt| receipt.event))
}

pub(crate) async fn terminal_content_matches(
    rest: &RestClient,
    work: &LogicalWork,
    text: &str,
) -> Result<bool, &'static str> {
    let receipt = inspect_receipt(rest, work)
        .await?
        .filter(|receipt| receipt.event.is_some())
        .ok_or("managed receipt is not confirmed")?;
    let expected: [u8; 32] = Sha256::digest(text.as_bytes()).into();
    Ok(receipt.content_hash == expected)
}

async fn inspect_receipt(
    rest: &RestClient,
    work: &LogicalWork,
) -> Result<Option<Receipt>, &'static str> {
    let path = work.journal_dir.join("journal.json");
    let relay = journal_relay(&rest.base_url)?;
    let signer = rest.keys.public_key().to_hex();
    let work = work.clone();
    tokio::time::timeout(
        Duration::from_secs(2),
        tokio::task::spawn_blocking(move || read_receipt(&path, &relay, &signer, &work)),
    )
    .await
    .map_err(|_| "journal read deadline expired")?
    .map_err(|_| "journal reader failed")?
}

/// Match the CLI's managed-delivery namespace, including URL normalization.
fn journal_relay(base: &str) -> Result<String, &'static str> {
    let mut url = url::Url::parse(&crate::relay::relay_ws_to_http(base))
        .map_err(|_| "invalid journal relay")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("invalid journal relay");
    }
    let path = url.path().trim_end_matches('/').to_owned();
    url.set_path(&path);
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

fn read_receipt(
    path: &Path,
    relay: &str,
    signer: &str,
    work: &LogicalWork,
) -> Result<Option<Receipt>, &'static str> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("journal is unreadable"),
    };
    if file
        .metadata()
        .map_err(|_| "journal metadata is unreadable")?
        .len()
        > MAX_JOURNAL_BYTES
    {
        return Err("journal exceeds its size bound");
    }
    let mut bytes = Vec::new();
    file.take(MAX_JOURNAL_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "journal read failed")?;
    decode_receipt(&bytes, relay, signer, work)
}

fn decode_receipt(
    bytes: &[u8],
    relay: &str,
    signer: &str,
    work: &LogicalWork,
) -> Result<Option<Receipt>, &'static str> {
    if bytes.len() as u64 > MAX_JOURNAL_BYTES {
        return Err("journal exceeds its size bound");
    }
    let journal: Journal =
        serde_json::from_slice(bytes).map_err(|_| "journal format is invalid")?;
    if !matches!(journal.version, 1 | 2) || journal.entries.len() > 1024 {
        return Err("journal version or entry bound is invalid");
    }
    let key = work.delivery_key.to_string();
    let namespace = serde_json::to_vec(&(relay, signer, &key)).map_err(|_| "namespace failed")?;
    let id = hex::encode(Sha256::digest(namespace));
    let Some(value) = journal.entries.get(&id) else {
        return Ok(None);
    };
    if serde_json::to_vec(value)
        .map_err(|_| "record format is invalid")?
        .len()
        > 512 * 1024
    {
        return Err("record exceeds its size bound");
    }
    let record: Record =
        serde_json::from_value(value.clone()).map_err(|_| "record format is invalid")?;
    let tags = buzz_core::nip10::parse_thread_markers_from_parts(
        record.event.tags.iter().map(|tag| tag.as_slice()),
    );
    let expected_parent = parent(work).unwrap_or_default();
    let channel = work.scope.channel_id().to_string();
    let channel_tags: Vec<_> = record
        .event
        .tags
        .iter()
        .filter(|tag| tag.as_slice()[0] == "h")
        .collect();
    if record.relay != relay
        || record.signer != signer
        || record.key != key
        || record.channel != channel
        || record.parent != expected_parent
        || record.event.pubkey.to_hex() != signer
        || record.event.verify().is_err()
        || delivery_key(&record.event) != Some(work.delivery_key)
        || channel_tags.len() != 1
        || channel_tags[0].as_slice() != ["h", &channel]
        || if expected_parent.is_empty() {
            record.event.tags.iter().any(|tag| tag.as_slice()[0] == "e")
        } else {
            tags.reply.as_deref() != Some(expected_parent.as_str())
        }
    {
        return Err("record signature, namespace or route is invalid");
    }
    let event = match record.canonical_event {
        Some(stamp) => stamp
            .restore(&record.event)
            .map_err(|_| "canonical receipt is invalid")?,
        None => record.event,
    };
    Ok(Some(Receipt {
        event: record.confirmed_at.map(|_| event.id.to_hex()),
        content_hash: Sha256::digest(event.content.as_bytes()).into(),
    }))
}

/// Prefer the CLI shipped beside this harness, including candidate canaries.
fn delivery_command() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.parent()
                .map(|dir| dir.join(if cfg!(windows) { "buzz.exe" } else { "buzz" }))
        })
        .filter(|path| path.is_file())
        .unwrap_or_else(|| "buzz".into())
}

/// A terminal notification is another invocation of the SAME managed key,
/// never a bare send. A pending/confirmed first report therefore wins.
pub(crate) async fn publish_terminal(
    rest: &RestClient,
    work: &LogicalWork,
    content: &str,
) -> Result<String, &'static str> {
    publish_terminal_using(&delivery_command(), rest, work, content).await
}

pub(crate) async fn publish_terminal_using(
    program: &Path,
    rest: &RestClient,
    work: &LogicalWork,
    content: &str,
) -> Result<String, &'static str> {
    if work.publication_revoked() {
        return Err("work was cancelled");
    }
    let mut command = tokio::process::Command::new(program);
    command
        .env("BUZZ_RELAY_URL", &rest.base_url)
        .env("BUZZ_PRIVATE_KEY", rest.keys.secret_key().to_secret_hex())
        .env_remove("BUZZ_AUTH_TAG")
        .args([
            "messages",
            "deliver",
            "--delivery-key",
            &work.delivery_key.to_string(),
            "--channel",
            &work.scope.channel_id().to_string(),
            "--journal-dir",
        ])
        .arg(&work.journal_dir)
        .args(["--content", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    if let Some(tag) = &rest.auth_tag_json {
        command.env("BUZZ_AUTH_TAG", tag);
    }
    if let Some(parent) = parent(work) {
        command.args(["--reply-to", &parent]);
    }
    let mut child = command.spawn().map_err(|_| "managed CLI could not start")?;
    let publishing = tokio::time::timeout(Duration::from_secs(30), async {
        let mut input = child.stdin.take().ok_or("managed CLI stdin unavailable")?;
        input
            .write_all(content.as_bytes())
            .await
            .map_err(|_| "managed CLI input failed")?;
        drop(input);
        let mut output = Vec::new();
        child
            .stdout
            .take()
            .ok_or("managed CLI output unavailable")?
            .take(65537)
            .read_to_end(&mut output)
            .await
            .map_err(|_| "managed CLI output failed")?;
        if output.len() > 65536 {
            return Err("managed CLI response exceeded its bound");
        }
        if !child
            .wait()
            .await
            .map_err(|_| "managed CLI wait failed")?
            .success()
        {
            return Err("managed delivery remains unconfirmed");
        }
        let value: serde_json::Value =
            serde_json::from_slice(&output).map_err(|_| "managed CLI response is invalid")?;
        if value["accepted"] != true {
            return Err("managed delivery was not confirmed");
        }
        value["event_id"]
            .as_str()
            .filter(|id| id.len() == 64 && id.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .map(str::to_owned)
            .ok_or("managed CLI receipt is invalid")
    });
    let result = tokio::select! {
        result = publishing => result.unwrap_or(Err("managed delivery time budget expired")),
        _ = work.publication_revoked_signal() => Err("publication was explicitly revoked"),
    };
    if result.is_err() {
        let _ = tokio::time::timeout(Duration::from_secs(2), child.kill()).await;
    }
    result
}
