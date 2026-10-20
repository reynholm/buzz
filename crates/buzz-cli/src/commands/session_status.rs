//! Optional session-status tags for new agent messages.
//!
//! An agent harness advertises its status instance directory through
//! [`SESSION_STATUS_DIR_ENV`]. Before a new kind-9 message is signed, the CLI
//! looks up the snapshot for the message's destination there and, when it is
//! current, adds the shared `buzz-session` / `buzz-session-info` tags.
//!
//! The lookup is best-effort and read-only: any missing, invalid, foreign,
//! ambiguous or stale snapshot yields no tags and never fails the send. It
//! makes no network calls, writes, deletes or renames nothing, never waits on
//! a lock and keeps no acknowledgment state. Liveness uses nonblocking lock
//! probes: a lock this process can take is not held by its producer, so the
//! probe is released at once and the snapshot is ignored.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use buzz_core::session_status::{
    canonical_identity, generation_lock_path, parse_snapshot, scope_file_name, StatusEnvelope,
    StatusIdentity, StatusScope, INSTANCE_LOCK_FILE, MAX_SNAPSHOT_BYTES,
};
use fs2::FileExt;
use nostr::Tag;
use uuid::Uuid;

use crate::client::BuzzClient;

/// Environment variable naming the agent harness's status instance directory.
pub(crate) const SESSION_STATUS_DIR_ENV: &str = "BUZZ_SESSION_STATUS_DIR";

/// The status directory advertised in this process's environment.
pub(crate) fn status_dir_from_env() -> Option<PathBuf> {
    status_dir_from_value(std::env::var_os(SESSION_STATUS_DIR_ENV))
}

/// Accept only a nonempty absolute UTF-8 path, as the harness writes it.
fn status_dir_from_value(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    let path = PathBuf::from(value?.into_string().ok()?);
    path.is_absolute().then_some(path)
}

/// Status tags for a new kind-9 message from `client` in `channel`, replying
/// in the thread rooted at `root` (if any).
///
/// The identity is the client's own relay URL and signing key.
pub(crate) fn message_status_tags(
    client: &BuzzClient,
    channel: Uuid,
    root: Option<&nostr::EventId>,
) -> Vec<Tag> {
    let Some(directory) = client.session_status_dir() else {
        return Vec::new();
    };
    let Ok(identity) = canonical_identity(client.relay_url(), &client.keys().public_key().to_hex())
    else {
        return Vec::new();
    };
    let root = root.map(nostr::EventId::to_hex);
    status_tags(Some(directory), &identity, channel, root.as_deref())
}

/// Status tags of the single live snapshot matching a publication.
///
/// A thread reply may belong to the thread's own session (thread policy) or
/// to the channel conversation (channel policy, DMs). Both candidates are
/// checked; exactly one current snapshot must match, otherwise the scope is
/// ambiguous and no tags are returned. A top-level message only matches the
/// conversation scope, so it never borrows a thread's status.
pub(crate) fn status_tags(
    directory: Option<&Path>,
    identity: &StatusIdentity,
    channel: Uuid,
    root: Option<&str>,
) -> Vec<Tag> {
    lookup(directory, identity, channel, root)
        .and_then(|envelope| envelope.event_tags().ok())
        .and_then(|tags| {
            tags.into_iter()
                .map(Tag::parse)
                .collect::<Result<_, _>>()
                .ok()
        })
        .unwrap_or_default()
}

fn lookup(
    directory: Option<&Path>,
    identity: &StatusIdentity,
    channel: Uuid,
    root: Option<&str>,
) -> Option<StatusEnvelope> {
    let directory = directory.filter(|dir| dir.is_absolute())?;
    let instance_id = instance_id(directory)?;
    if !held_by_producer(&directory.join(INSTANCE_LOCK_FILE)) {
        return None;
    }
    let thread = root.map(|root| StatusScope {
        channel_id: channel,
        thread_root: Some(root.to_owned()),
    });
    let conversation = StatusScope {
        channel_id: channel,
        thread_root: None,
    };
    let mut found = None;
    for scope in thread.iter().chain(std::iter::once(&conversation)) {
        if let Some(envelope) = current_snapshot(directory, instance_id, identity, scope) {
            if found.replace(envelope).is_some() {
                return None;
            }
        }
    }
    found
}

/// The instance UUID named by the directory's final component.
fn instance_id(directory: &Path) -> Option<Uuid> {
    let name = directory.file_name()?.to_str()?;
    let id = Uuid::parse_str(name).ok()?;
    (id.as_hyphenated().to_string() == name).then_some(id)
}

/// The snapshot for `scope`, if it belongs to this identity, scope and
/// instance and its generation lock is still held by the producer.
fn current_snapshot(
    directory: &Path,
    instance_id: Uuid,
    identity: &StatusIdentity,
    scope: &StatusScope,
) -> Option<StatusEnvelope> {
    let path = directory.join(scope_file_name(scope).ok()?);
    let envelope = parse_snapshot(&read_bounded(&path)?).ok()?;
    let matches = envelope.identity == *identity
        && envelope.scope == *scope
        && envelope.instance_id == instance_id;
    (matches && held_by_producer(&directory.join(generation_lock_path(envelope.generation))))
        .then_some(envelope)
}

/// Open a regular file (not a link, FIFO or device) read-only.
fn open_regular(path: &Path) -> Option<File> {
    if !fs::symlink_metadata(path).ok()?.file_type().is_file() {
        return None;
    }
    let file = File::open(path).ok()?;
    file.metadata().ok()?.is_file().then_some(file)
}

/// Read at most one byte more than a snapshot may hold.
fn read_bounded(path: &Path) -> Option<Vec<u8>> {
    let file = open_regular(path)?;
    let mut bytes = Vec::new();
    file.take(MAX_SNAPSHOT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= MAX_SNAPSHOT_BYTES).then_some(bytes)
}

/// Whether another holder keeps `path` exclusively locked.
///
/// Only a nonblocking attempt is made. If it succeeds the producer no longer
/// holds the lock: release it immediately and report "not held".
fn held_by_producer(path: &Path) -> bool {
    let Some(file) = open_regular(path) else {
        return false;
    };
    match file.try_lock_exclusive() {
        Ok(()) => {
            let _ = FileExt::unlock(&file);
            false
        }
        Err(err) => {
            err.kind() == std::io::ErrorKind::WouldBlock
                || err.raw_os_error() == fs2::lock_contended_error().raw_os_error()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn status_dir_value_must_be_a_nonempty_absolute_path() {
        let absolute = std::env::temp_dir().join("instance");
        let cases: Vec<(Option<OsString>, Option<PathBuf>)> = vec![
            (None, None),
            (Some(OsString::new()), None),
            (Some("relative/instance".into()), None),
            (Some("instance".into()), None),
            (Some(absolute.clone().into()), Some(absolute)),
        ];
        for (value, expected) in cases {
            assert_eq!(status_dir_from_value(value.clone()), expected, "{value:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn status_dir_value_must_be_utf8() {
        use std::os::unix::ffi::OsStringExt;
        let value = OsString::from_vec(b"/tmp/\xff".to_vec());
        assert_eq!(status_dir_from_value(Some(value)), None);
    }
}
