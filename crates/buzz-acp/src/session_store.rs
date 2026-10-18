//! Durable per-scope records of ACP provider sessions.
//!
//! A provider session outlives the worker process that created it: after an
//! idle pool sleep, a worker crash, or a harness restart, the next turn in the
//! same scope can resume it with ACP `session/resume`. This module stores one
//! small JSON record per scope so that decision survives the process.
//!
//! Every change to a record happens inside a critical section guarded by an
//! exclusive lock on `<record>.lock`, with a bounded wait. Ownership is fenced
//! by a lease: a claim writes a fresh lease, and later writes only succeed
//! while the record still carries it, so a late writer that lost the record
//! (a stale owner, a rotate in between) never overwrites the new owner.
//!
//! Store failures never fail a turn. A failed write poisons the scope for the
//! rest of the process, and, if the failure happened inside the critical
//! section on a record we own, moves that record aside to `.stale` so a later
//! process does not resume a session this one could not update.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::scope::SessionScope;
use buzz_core::session_control::RestartDepth;

#[path = "session_store_controls.rs"]
#[allow(dead_code)]
// Control controller consumes these durable APIs independently of provider resume.
mod controls;
#[allow(unused_imports)] // Durable controller contract, independent of provider acquisition.
pub use controls::{ControlAttempt, ControlReceipt, ControlReceiptState, RestartIntent};

/// Delivered event ids kept per record, oldest dropped first.
pub const MAX_STORED_DELIVERED_IDS: usize = 2048;
/// Records larger than this are treated as unreadable.
pub const MAX_RECORD_BYTES: u64 = 256 * 1024;
/// Longest wait for a record lock before the operation gives up as busy.
pub const STORE_LOCK_WAIT: Duration = Duration::from_secs(2);

const RECORD_VERSION: u32 = 2;
const LOCK_RETRY: Duration = Duration::from_millis(20);
const INSTANCES_DIR: &str = "instances";

/// Why a live session left worker memory while staying resumable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Departure {
    IdleSleep,
    WorkerCrash,
    /// Worker retired for a sibling scope restart.
    WorkerRecycled,
}

/// Why a scope's session is new or resumed, as reported to the agent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "String", from = "String")]
pub enum SessionReason {
    IdleSleep,
    WorkerCrash,
    WorkerRecycled,
    UserRestart(RestartDepth),
    UserCompact,
    AutoCompact,
    HarnessRestart,
    FirstInThread,
    Discarded,
    ConfigChanged,
    LiveElsewhere,
    ResumeUnsupported,
    /// `resume_failed:<code>`.
    ResumeFailed(String),
}

impl SessionReason {
    /// Stable wire string.
    pub fn as_wire(&self) -> String {
        match self {
            Self::IdleSleep => "idle_sleep".into(),
            Self::WorkerCrash => "worker_crash".into(),
            Self::WorkerRecycled => "worker_recycled".into(),
            Self::UserRestart(depth) => format!("user_restart:{}", depth.as_wire()),
            Self::UserCompact => "user_compact".into(),
            Self::AutoCompact => "auto_compact".into(),
            Self::HarnessRestart => "harness_restart".into(),
            Self::FirstInThread => "first_in_thread".into(),
            Self::Discarded => "discarded".into(),
            Self::ConfigChanged => "config_changed".into(),
            Self::LiveElsewhere => "live_elsewhere".into(),
            Self::ResumeUnsupported => "resume_unsupported".into(),
            Self::ResumeFailed(code) => format!("resume_failed:{code}"),
        }
    }
}

impl From<Departure> for SessionReason {
    fn from(departure: Departure) -> Self {
        match departure {
            Departure::IdleSleep => Self::IdleSleep,
            Departure::WorkerCrash => Self::WorkerCrash,
            Departure::WorkerRecycled => Self::WorkerRecycled,
        }
    }
}

impl From<SessionReason> for String {
    fn from(reason: SessionReason) -> Self {
        reason.as_wire()
    }
}

impl From<String> for SessionReason {
    fn from(wire: String) -> Self {
        match wire.as_str() {
            "idle_sleep" => Self::IdleSleep,
            "worker_crash" => Self::WorkerCrash,
            "worker_recycled" => Self::WorkerRecycled,
            "user_restart:none" => Self::UserRestart(RestartDepth::None),
            "user_restart:recent" => Self::UserRestart(RestartDepth::Recent),
            "user_restart:all" => Self::UserRestart(RestartDepth::All),
            "user_compact" => Self::UserCompact,
            "auto_compact" => Self::AutoCompact,
            "harness_restart" => Self::HarnessRestart,
            "first_in_thread" => Self::FirstInThread,
            "discarded" => Self::Discarded,
            "config_changed" => Self::ConfigChanged,
            "live_elsewhere" => Self::LiveElsewhere,
            "resume_unsupported" => Self::ResumeUnsupported,
            other => Self::ResumeFailed(
                other
                    .strip_prefix("resume_failed:")
                    .unwrap_or(other)
                    .to_owned(),
            ),
        }
    }
}

/// A provider session that a later turn may resume.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveSession {
    /// Ownership token, replaced by every claim.
    pub lease: Uuid,
    pub adapter: String,
    pub session_id: String,
    pub cwd: String,
    pub model: Option<String>,
    /// Harness process that owns the session.
    pub instance: Uuid,
    pub worker_index: usize,
    pub departed: Option<Departure>,
    pub turns: u32,
    pub standing_context_sent: bool,
    /// Oldest first, capped at [`MAX_STORED_DELIVERED_IDS`].
    pub delivered_event_ids: Vec<String>,
    pub hydrated_thread_roots: Vec<String>,
}

/// One record per scope. `live: None` is a tombstone: the session was
/// discarded on purpose and must not be resumed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub version: u32,
    /// Hash of the canonical relay identity and agent public key.
    pub namespace: String,
    pub live: Option<LiveSession>,
    pub reason: Option<SessionReason>,
    /// Fresh CAS revision; absent on legacy v2 records.
    #[serde(default)]
    pub revision: Option<Uuid>,
    /// Policy for the next successful foreground delivery.
    #[serde(default)]
    pub pending_restart: Option<RestartIntent>,
    /// Bounded durable control deduplication, empty on legacy v2 records.
    #[serde(default)]
    pub control_receipts: Vec<ControlReceipt>,
}

/// Result of reading a scope's record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadOutcome {
    Missing,
    Found(Box<SessionRecord>),
    /// The record could not be read; it was moved aside to `.corrupt`.
    Unreadable,
}

/// What a claim expects to find on disk, taken from the record its decision
/// was based on. A mismatch means another owner changed the record since.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expect {
    Missing,
    Tombstone { revision: Option<Uuid> },
    Live { session_id: String, lease: Uuid },
}

impl Expect {
    /// The expectation matching what `load` returned.
    pub fn from_outcome(outcome: &LoadOutcome) -> Self {
        match outcome {
            // An unreadable record was moved aside, so the path is empty now.
            LoadOutcome::Missing | LoadOutcome::Unreadable => Self::Missing,
            LoadOutcome::Found(record) => match &record.live {
                None => Self::Tombstone {
                    revision: record.revision,
                },
                Some(live) => Self::Live {
                    session_id: live.session_id.clone(),
                    lease: live.lease,
                },
            },
        }
    }

    fn matches(&self, current: Option<&SessionRecord>) -> bool {
        match (self, current) {
            (Self::Missing, None) => true,
            (Self::Tombstone { revision }, Some(record)) => {
                record.live.is_none() && record.revision == *revision
            }
            (Self::Live { session_id, lease }, Some(record)) => record
                .live
                .as_ref()
                .is_some_and(|live| live.session_id == *session_id && live.lease == *lease),
            _ => false,
        }
    }
}

#[derive(Debug)]
pub enum StoreError {
    /// The record lock stayed taken for longer than [`STORE_LOCK_WAIT`].
    Busy,
    /// The record no longer matches what the caller expected.
    Conflict,
    Io(io::Error),
    Unreadable,
    /// Input is not a canonical control identity.
    InvalidControl,
    /// New signed request is outside the freshness window.
    StaleRequest,
    /// Internal no-write signal for an already persisted control.
    AlreadyRecorded,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => f.write_str("record lock busy"),
            Self::Conflict => f.write_str("record changed by another owner"),
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Unreadable => f.write_str("record unreadable"),
            Self::InvalidControl => f.write_str("invalid control metadata"),
            Self::StaleRequest => f.write_str("stale control request"),
            Self::AlreadyRecorded => f.write_str("control already recorded"),
        }
    }
}

/// Append newly delivered ids, skipping duplicates and dropping the oldest
/// beyond [`MAX_STORED_DELIVERED_IDS`].
pub fn append_delivered(ids: &mut Vec<String>, new: impl IntoIterator<Item = String>) {
    let mut seen: HashSet<String> = ids.iter().cloned().collect();
    for id in new {
        if seen.insert(id.clone()) {
            ids.push(id);
        }
    }
    if ids.len() > MAX_STORED_DELIVERED_IDS {
        let excess = ids.len() - MAX_STORED_DELIVERED_IDS;
        ids.drain(..excess);
    }
}

/// Registry proof distinguishes a live owner from temporarily unavailable evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InstanceLiveness {
    Alive,
    Dead,
    Unknown,
}

/// Per-agent directory of session records.
pub struct SessionStore {
    dir: PathBuf,
    namespace: String,
    instance: Uuid,
    /// Registered on the first write and held for the life of the process;
    /// the OS releases it on exit, which is how other processes tell a dead
    /// instance from a live one. Every record naming this instance is written
    /// after the lock exists.
    instance_lock: Mutex<Option<File>>,
    poisoned: Mutex<HashSet<PathBuf>>,
    poisoned_channels: Mutex<HashSet<Uuid>>,
    #[cfg(test)]
    fail_writes: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    before_instance_lock: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl SessionStore {
    /// Open an isolated namespace under either the default or configured root.
    /// Legacy unscoped records are deliberately not migrated. Nothing touches
    /// the disk until the first write.
    pub fn open(root: PathBuf, relay: &str, agent: &str) -> io::Result<Self> {
        let namespace = store_namespace(relay, agent)?;
        Ok(Self {
            dir: root.join("v2").join(&namespace),
            namespace,
            instance: Uuid::new_v4(),
            instance_lock: Mutex::new(None),
            poisoned: Mutex::new(HashSet::new()),
            poisoned_channels: Mutex::new(HashSet::new()),
            #[cfg(test)]
            fail_writes: std::sync::atomic::AtomicBool::new(false),
            #[cfg(test)]
            before_instance_lock: Mutex::new(None),
        })
    }

    pub fn instance(&self) -> Uuid {
        self.instance
    }

    /// Create and hold this process's instance lock, sweeping lock files of
    /// instances that are no longer running.
    fn register_instance(&self) -> io::Result<()> {
        let mut held = self
            .instance_lock
            .lock()
            .map_err(|_| io::Error::other("instance lock poisoned"))?;
        if held.is_some() {
            return Ok(());
        }
        // Publication and removal share a registry lock. An unlocked UUID
        // pathname can be a registration in progress, not a dead process.
        let _registry = self
            .lock_record(&self.dir.join(INSTANCES_DIR))
            .map_err(|error| io::Error::other(error.to_string()))?;
        let instances = self.dir.join(INSTANCES_DIR);
        fs::create_dir_all(&instances)?;
        for entry in fs::read_dir(&instances)?.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("lock") {
                continue;
            }
            if let Ok(file) = OpenOptions::new().read(true).write(true).open(&path) {
                if file.try_lock_exclusive().is_ok() {
                    let _ = fs::remove_file(&path);
                }
            }
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.instance_lock_path(self.instance))?;
        #[cfg(test)]
        if let Some(hook) = self.before_instance_lock.lock().unwrap().take() {
            hook();
        }
        lock.try_lock_exclusive()?;
        *held = Some(lock);
        Ok(())
    }

    fn instance_lock_path(&self, instance: Uuid) -> PathBuf {
        self.dir
            .join(INSTANCES_DIR)
            .join(format!("{instance}.lock"))
    }

    /// Whether the harness process `instance` may still be running.
    /// Uncertain ownership must block resume admission just like a live owner.
    pub fn instance_alive(&self, instance: Uuid) -> bool {
        self.instance_liveness(instance) != InstanceLiveness::Dead
    }

    pub(crate) fn instance_liveness(&self, instance: Uuid) -> InstanceLiveness {
        if instance == self.instance {
            return InstanceLiveness::Alive;
        }
        let Ok(_registry) = self.lock_record(&self.dir.join(INSTANCES_DIR)) else {
            return InstanceLiveness::Unknown;
        };
        let path = self.instance_lock_path(instance);
        let file = match OpenOptions::new().read(true).write(true).open(&path) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return InstanceLiveness::Dead,
            Err(_) => return InstanceLiveness::Unknown,
        };
        match file.try_lock_exclusive() {
            Ok(()) => {
                let _ = FileExt::unlock(&file);
                let _ = fs::remove_file(&path);
                InstanceLiveness::Dead
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => InstanceLiveness::Alive,
            Err(_) => InstanceLiveness::Unknown,
        }
    }

    /// Read a scope's record. A record that cannot be parsed is moved aside.
    pub fn load(&self, scope: &SessionScope) -> LoadOutcome {
        let path = self.record_path(scope);
        match read_record(&path, &self.namespace) {
            Ok(None) => LoadOutcome::Missing,
            Ok(Some(record)) => LoadOutcome::Found(Box::new(record)),
            Err(()) => {
                if let Ok(_guard) = self.lock_record(&path) {
                    // Re-check under the lock: a writer may have replaced it.
                    if read_record(&path, &self.namespace).is_err() {
                        let _ = fs::rename(&path, with_suffix(&path, "corrupt"));
                        // A newer canonical record supersedes quarantine metadata.
                        // Never resurrect that older intent after corruption.
                        let stale = with_suffix(&path, "stale");
                        let _ = fs::rename(&stale, with_suffix(&stale, "corrupt"));
                    }
                }
                tracing::warn!(target: "session_store", path = %path.display(), "unreadable session record");
                LoadOutcome::Unreadable
            }
        }
    }

    /// Take ownership of a scope's record before calling the provider.
    /// Returns the new lease.
    pub fn claim(
        &self,
        scope: &SessionScope,
        expect: &Expect,
        mut live: LiveSession,
    ) -> Result<Uuid, StoreError> {
        let lease = Uuid::new_v4();
        live.lease = lease;
        self.update(&self.record_path(scope), |current| {
            if !expect.matches(current) {
                return Err(StoreError::Conflict);
            }
            Ok(SessionRecord {
                version: RECORD_VERSION,
                namespace: self.namespace.clone(),
                live: Some(live),
                reason: None,
                revision: Some(Uuid::new_v4()),
                pending_restart: current.and_then(|r| r.pending_restart.clone()),
                control_receipts: current
                    .map(|r| r.control_receipts.clone())
                    .unwrap_or_default(),
            })
        })?;
        Ok(lease)
    }

    /// Change the live session while it still carries `lease`.
    pub fn commit(
        &self,
        scope: &SessionScope,
        lease: Uuid,
        change: impl FnOnce(&mut LiveSession),
    ) -> Result<(), StoreError> {
        self.update(&self.record_path(scope), |current| {
            let mut record = owned_record(current, lease)?;
            if let Some(live) = record.live.as_mut() {
                change(live);
            }
            Ok(record)
        })
    }

    pub fn mark_departed(
        &self,
        scope: &SessionScope,
        lease: Uuid,
        departure: Departure,
    ) -> Result<(), StoreError> {
        self.commit(scope, lease, |live| live.departed = Some(departure))
    }

    /// Invalidate the owned provider after an attempt, retaining pending restart
    /// policy until foreground delivery completes it. Explicit scope/channel
    /// revocation uses the separate unconditional discard methods below.
    pub fn discard(&self, scope: &SessionScope, lease: Uuid) -> Result<(), StoreError> {
        self.update(&self.record_path(scope), |current| {
            let owned = owned_record(current, lease)?;
            let mut record = self.tombstone(Some(&owned));
            record.pending_restart = owned.pending_restart;
            Ok(record)
        })
    }

    /// Tombstone the record whoever owns it (owner commands, channel removal).
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Explicit policy-revoking API retained; production restart uses one atomic request_restart instead"
        )
    )]
    pub fn discard_scope(&self, scope: &SessionScope) -> Result<(), StoreError> {
        self.update(&self.record_path(scope), |current| {
            Ok(self.tombstone(current))
        })
    }

    /// Tombstone every record of a channel. Returns how many were changed.
    pub fn discard_channel(&self, channel_id: uuid::Uuid) -> Result<usize, StoreError> {
        let result = self.discard_channel_records(channel_id);
        if let Err(err) = &result {
            // Enumeration can fail before any record is known. A per-record
            // poison cannot fence these disk-only scopes after pool sleep.
            if let Ok(mut channels) = self.poisoned_channels.lock() {
                channels.insert(channel_id);
            }
            tracing::warn!(target: "session_store", %channel_id,
                "channel invalidation failed, channel will not resume: {err}");
        }
        result
    }

    fn discard_channel_records(&self, channel_id: Uuid) -> Result<usize, StoreError> {
        let dir = self.channel_dir(channel_id);
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(err) => return Err(StoreError::Io(err)),
        };
        let mut changed = 0;
        let mut first_error = None;
        let mut visited = HashSet::new();
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    first_error.get_or_insert(StoreError::Io(err));
                    continue;
                }
            };
            let mut path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("stale") {
                path = path.with_extension("");
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("json")
                || !visited.insert(path.clone())
            {
                continue;
            }
            match self.update(&path, |current| Ok(self.tombstone(current))) {
                Ok(()) => changed += 1,
                Err(err) => {
                    first_error.get_or_insert(err);
                }
            }
        }
        match first_error {
            Some(err) => Err(err),
            None => Ok(changed),
        }
    }

    pub fn is_poisoned(&self, scope: &SessionScope) -> bool {
        self.poisoned_channels
            .lock()
            .map(|channels| channels.contains(&scope.channel_id()))
            .unwrap_or(true)
            || self
                .poisoned
                .lock()
                .map(|set| set.contains(&self.record_path(scope)))
                .unwrap_or(true)
    }

    #[cfg(test)]
    pub fn fail_writes(&self, on: bool) {
        self.fail_writes
            .store(on, std::sync::atomic::Ordering::SeqCst);
    }

    fn poison_path(&self, path: &Path) {
        if let Ok(mut set) = self.poisoned.lock() {
            set.insert(path.to_path_buf());
        }
    }

    fn record_path(&self, scope: &SessionScope) -> PathBuf {
        let key = match scope {
            SessionScope::Conversation { channel_id } => format!("conversation:{channel_id}"),
            SessionScope::Thread {
                channel_id,
                root_event_id,
            } => format!("thread:{channel_id}:{root_event_id}"),
        };
        let digest = hex::encode(Sha256::digest(key.as_bytes()));
        self.channel_dir(scope.channel_id())
            .join(format!("{digest}.json"))
    }

    /// Directory containing this namespace's records for one channel.
    pub(crate) fn channel_dir(&self, channel_id: Uuid) -> PathBuf {
        self.dir.join(channel_id.to_string())
    }

    fn tombstone(&self, current: Option<&SessionRecord>) -> SessionRecord {
        SessionRecord {
            version: RECORD_VERSION,
            namespace: self.namespace.clone(),
            live: None,
            reason: Some(SessionReason::Discarded),
            revision: Some(Uuid::new_v4()),
            pending_restart: None,
            control_receipts: current
                .map(|r| r.control_receipts.clone())
                .unwrap_or_default(),
        }
    }

    fn lock_record(&self, path: &Path) -> Result<File, StoreError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(StoreError::Io)?;
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(with_suffix(path, "lock"))
            .map_err(StoreError::Io)?;
        let deadline = Instant::now() + STORE_LOCK_WAIT;
        loop {
            match lock.try_lock_exclusive() {
                Ok(()) => return Ok(lock),
                Err(_) if Instant::now() < deadline => std::thread::sleep(LOCK_RETRY),
                Err(_) => return Err(StoreError::Busy),
            }
        }
    }

    /// The critical section: lock, read, decide, write. `decide` sees the
    /// record currently on disk and returns the record to write.
    fn update(
        &self,
        path: &Path,
        decide: impl FnOnce(Option<&SessionRecord>) -> Result<SessionRecord, StoreError>,
    ) -> Result<(), StoreError> {
        self.update_with_quarantine(path, decide, true)
    }

    fn update_with_quarantine(
        &self,
        path: &Path,
        decide: impl FnOnce(Option<&SessionRecord>) -> Result<SessionRecord, StoreError>,
        quarantine: bool,
    ) -> Result<(), StoreError> {
        let result = self.update_locked(path, decide, quarantine);
        match &result {
            Ok(())
            | Err(
                StoreError::Conflict
                | StoreError::AlreadyRecorded
                | StoreError::StaleRequest
                | StoreError::InvalidControl,
            ) => {}
            Err(StoreError::Busy) if !quarantine => {}
            Err(err) => {
                tracing::warn!(
                    target: "session_store",
                    path = %path.display(),
                    "session record write failed, scope will not resume: {err}"
                );
                self.poison_path(path);
            }
        }
        result
    }

    fn update_locked(
        &self,
        path: &Path,
        decide: impl FnOnce(Option<&SessionRecord>) -> Result<SessionRecord, StoreError>,
        quarantine: bool,
    ) -> Result<(), StoreError> {
        self.register_instance().map_err(StoreError::Io)?;
        let lock = self.lock_record(path)?;
        let current = read_record(path, &self.namespace).map_err(|()| StoreError::Unreadable)?;
        let mut next = decide(current.as_ref())?;
        next.revision = Some(Uuid::new_v4());
        if let Err(err) = self.write_record(path, &next) {
            // Still inside the critical section, on a record `decide`
            // accepted: move it aside so no later process resumes it.
            if quarantine && current.is_some() {
                let _ = fs::rename(path, with_suffix(path, "stale"));
            }
            drop(lock);
            return Err(StoreError::Io(err));
        }
        drop(lock);
        Ok(())
    }

    fn write_record(&self, path: &Path, record: &SessionRecord) -> io::Result<()> {
        #[cfg(test)]
        if self.fail_writes.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(io::Error::other("injected write failure"));
        }
        let dir = path
            .parent()
            .ok_or_else(|| io::Error::other("record path has no parent"))?;
        let json = serde_json::to_vec(record).map_err(io::Error::other)?;
        if json.len() as u64 > MAX_RECORD_BYTES || !controls::valid_controls(record) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "record exceeds limits",
            ));
        }
        let mut file = tempfile::NamedTempFile::new_in(dir)?;
        file.write_all(&json)?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|err| err.error)?;
        #[cfg(unix)]
        File::open(dir)?.sync_all()?;
        Ok(())
    }
}

impl Drop for SessionStore {
    fn drop(&mut self) {
        let held = self.instance_lock.get_mut().ok().and_then(Option::take);
        if held.is_some() {
            let _ = fs::remove_file(self.instance_lock_path(self.instance));
        }
    }
}

fn store_namespace(relay: &str, agent: &str) -> io::Result<String> {
    let invalid = |message| io::Error::new(io::ErrorKind::InvalidInput, message);
    let relay = url::Url::parse(relay.trim()).map_err(|_| invalid("invalid relay URL"))?;
    if !matches!(relay.scheme(), "ws" | "wss")
        || relay.host().is_none()
        || !relay.username().is_empty()
        || relay.password().is_some()
        || relay.fragment().is_some()
    {
        return Err(invalid("invalid relay identity"));
    }
    // URL serialization normalizes case, default ports and the root slash.
    // Keep distinct hosts (including loopback hosts), paths and queries:
    // the relay host selects the community, unlike runtime process aliases.
    let agent = nostr::PublicKey::from_hex(agent)
        .map_err(|_| invalid("invalid agent public key"))?
        .to_hex();
    let identity = serde_json::to_vec(&("buzz-session-store-v2", relay.as_str(), agent))
        .map_err(io::Error::other)?;
    Ok(hex::encode(Sha256::digest(identity)))
}

/// The current record, if its live session carries `lease`.
fn owned_record(current: Option<&SessionRecord>, lease: Uuid) -> Result<SessionRecord, StoreError> {
    match current {
        Some(record) if record.live.as_ref().is_some_and(|live| live.lease == lease) => {
            Ok(record.clone())
        }
        _ => Err(StoreError::Conflict),
    }
}

/// `Ok(None)` when absent, `Err(())` when present but unusable.
fn read_record(path: &Path, namespace: &str) -> Result<Option<SessionRecord>, ()> {
    if let Some(record) = read_record_file(path, namespace)? {
        return Ok(Some(record));
    }
    // A failed provider write quarantines stale provider state. Its accepted
    // restart policy and receipts must survive that quarantine as a tombstone.
    Ok(read_record_file(&with_suffix(path, "stale"), namespace)?
        .filter(|r| !r.control_receipts.is_empty() || r.pending_restart.is_some())
        .map(|mut record| {
            record.live = None;
            record.reason = Some(SessionReason::ResumeFailed("not_persisted".into()));
            record
        }))
}

fn read_record_file(path: &Path, namespace: &str) -> Result<Option<SessionRecord>, ()> {
    use std::io::Read;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_RECORD_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err(());
    }
    match serde_json::from_slice::<SessionRecord>(&bytes) {
        Ok(record)
            if record.version == RECORD_VERSION
                && record.namespace == namespace
                && controls::valid_controls(&record) =>
        {
            Ok(Some(record))
        }
        _ => Err(()),
    }
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".");
    name.push(suffix);
    PathBuf::from(name)
}

#[cfg(test)]
#[path = "session_store_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "session_store_control_tests.rs"]
mod control_tests;

#[path = "session_store_orphans.rs"]
mod orphan_controls;
