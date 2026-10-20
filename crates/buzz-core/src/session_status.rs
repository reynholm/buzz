//! Shared session-status contract.
//!
//! The agent harness writes small per-scope status snapshots, the CLI reads
//! them and attaches a pair of tags to outgoing kind-9 messages, and Desktop
//! renders those tags. This module holds the pure pieces all three share:
//! canonical status identity, scope-to-file naming, the snapshot JSON format
//! (version [`STATUS_FORMAT_VERSION`]), validation, and the wire tags.
//!
//! Nothing here touches the filesystem or the network. Status is display
//! metadata: it carries no transcript, prompt, key, provider session ID, lease
//! or local path, and reading it grants no authority.
//!
//! # Instance directory layout
//!
//! The harness owns one directory per running instance (see
//! [`INSTANCE_LOCK_FILE`]). Inside it:
//!
//! - `instance.lock` — held by the harness for as long as the instance lives;
//! - `generations/<generation UUID>.lock` — see [`generation_lock_path`], held
//!   while a snapshot generation is current;
//! - one snapshot file per scope — see [`scope_file_name`].
//!
//! # Wire tags
//!
//! [`SessionStatus::event_tags`] produces two tags:
//!
//! - `["buzz-session", "<state>", "<reason>", "<percent>"]`, where `<percent>`
//!   is the empty string when context occupancy is unavailable (never `0`);
//! - `["buzz-session-info", "1", "<transition UUID>", "thread|conversation",
//!   "<read or empty>", "<total or empty>", "exact|lower_bound|unknown"]`.
//!
//! The info tag's mode carries the completeness of the cold-start history:
//!
//! - `exact`: the history was complete; `<read>` and `<total>` are both set;
//! - `lower_bound`: the history is known to be incomplete, or `<total>` is
//!   present but only a proven lower bound. `<total>` may be empty (readers
//!   then say earlier history may be omitted) and `<read>` may be empty when
//!   event identities are missing;
//! - `unknown`: no completeness claim: there is no total and the history is
//!   not known to be incomplete. `<read>` may still be set.

use std::fmt;

use crate::session_control::{ControlAction, ControlOrigin, RestartDepth};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;
use uuid::Uuid;

/// Snapshot format version written by this crate; readers also accept v1.
pub const STATUS_FORMAT_VERSION: u8 = 2;
/// Maximum accepted or produced snapshot size in bytes (16 KiB).
pub const MAX_SNAPSHOT_BYTES: usize = 16 * 1024;
/// Maximum length of the wire reason in ASCII bytes.
pub const MAX_REASON_BYTES: usize = 96;
/// Name of the file, inside an instance directory, held while it is alive.
pub const INSTANCE_LOCK_FILE: &str = "instance.lock";
/// Name of the instance subdirectory that holds generation lock files.
pub const GENERATIONS_DIR: &str = "generations";
/// Name of the primary status tag.
pub const SESSION_TAG: &str = "buzz-session";
/// Name of the supporting status tag.
pub const SESSION_INFO_TAG: &str = "buzz-session-info";
/// Version field of the supporting tag; independent of [`STATUS_FORMAT_VERSION`].
pub const SESSION_INFO_VERSION: &str = "1";

const RESUME_FAILED_PREFIX: &str = "resume_failed:";
const THREAD_ROOT_HEX_LEN: usize = 64;

/// Errors from status validation, naming and parsing.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum StatusError {
    /// The relay URL is not usable as a status identity.
    #[error("invalid relay URL for status identity")]
    InvalidRelay,
    /// The agent key is not a valid public key.
    #[error("invalid agent public key")]
    InvalidAgent,
    /// A thread root is not 64 lowercase hex characters.
    #[error("thread root must be 64 lowercase hex characters")]
    InvalidThreadRoot,
    /// A reason is not in the known, bounded vocabulary.
    #[error("unknown or unsafe status reason")]
    InvalidReason,
    /// Manual transition origin is malformed or forbidden by the envelope.
    #[error("invalid manual transition origin")]
    InvalidOrigin,
    /// Occupancy has a zero size.
    #[error("context size must be positive")]
    InvalidOccupancy,
    /// History counts are inconsistent.
    #[error("inconsistent history coverage")]
    InvalidHistory,
    /// A snapshot is larger than [`MAX_SNAPSHOT_BYTES`].
    #[error("status snapshot exceeds the size limit")]
    TooLarge,
    /// A snapshot uses a format version this crate does not understand.
    #[error("unsupported status snapshot version")]
    UnsupportedVersion,
    /// A snapshot is not valid JSON of the expected shape.
    #[error("malformed status snapshot")]
    Malformed,
}

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

/// Canonical `(relay, agent)` identity used to match status to a sender.
///
/// Built only by [`canonical_identity`]. This is a status-lookup identity and
/// does not replace the durable session store's own namespace helper.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusIdentity {
    /// Canonical relay URL (`ws`/`wss` form).
    pub relay: String,
    /// Agent public key as lowercase hex.
    pub agent: String,
}

/// Canonicalize a relay URL and agent key into a [`StatusIdentity`].
///
/// URL serialization normalizes host case, default ports and the root slash.
/// `http`/`https` are folded onto `ws`/`wss` because a CLI may reach the same
/// relay over either transport. Everything else is preserved: each loopback
/// spelling stays distinct, as do nondefault ports, paths and queries.
/// Credentials, fragments, other schemes and invalid keys are rejected.
pub fn canonical_identity(relay: &str, agent: &str) -> Result<StatusIdentity, StatusError> {
    let mut url = Url::parse(relay.trim()).map_err(|_| StatusError::InvalidRelay)?;
    if url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(StatusError::InvalidRelay);
    }
    let websocket = match url.scheme() {
        "ws" | "http" => "ws",
        "wss" | "https" => "wss",
        _ => return Err(StatusError::InvalidRelay),
    };
    if url.scheme() != websocket {
        url.set_scheme(websocket)
            .map_err(|()| StatusError::InvalidRelay)?;
    }
    let mut relay = url.to_string();
    if url.path() == "/" && url.query().is_none() {
        relay.pop();
    }
    let agent = nostr::PublicKey::from_hex(agent)
        .map_err(|_| StatusError::InvalidAgent)?
        .to_hex();
    Ok(StatusIdentity { relay, agent })
}

impl StatusIdentity {
    /// Whether this identity is already in canonical form.
    pub fn validate(&self) -> Result<(), StatusError> {
        if canonical_identity(&self.relay, &self.agent)? == *self {
            Ok(())
        } else {
            Err(StatusError::InvalidRelay)
        }
    }
}

// ---------------------------------------------------------------------------
// Scope and file names
// ---------------------------------------------------------------------------

/// The conversation a status belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusScope {
    /// Channel (or DM) the session runs in.
    pub channel_id: Uuid,
    /// Thread root event ID (64 lowercase hex); `None` is conversation scope.
    pub thread_root: Option<String>,
}

impl StatusScope {
    /// Reject a thread root that is not 64 lowercase hex characters.
    pub fn validate(&self) -> Result<(), StatusError> {
        match &self.thread_root {
            None => Ok(()),
            Some(root)
                if root.len() == THREAD_ROOT_HEX_LEN
                    && root
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) =>
            {
                Ok(())
            }
            Some(_) => Err(StatusError::InvalidThreadRoot),
        }
    }

    /// `thread` when a root is present, `conversation` otherwise.
    pub fn kind(&self) -> &'static str {
        if self.thread_root.is_some() {
            "thread"
        } else {
            "conversation"
        }
    }
}

/// File name of a scope's snapshot inside an instance directory.
///
/// The name is built from a validated UUID and validated hex only, so it can
/// never contain a path separator or traversal component.
pub fn scope_file_name(scope: &StatusScope) -> Result<String, StatusError> {
    scope.validate()?;
    let channel = scope.channel_id.as_hyphenated();
    Ok(match &scope.thread_root {
        Some(root) => format!("{channel}.thread-{root}.json"),
        None => format!("{channel}.conversation.json"),
    })
}

/// Relative path (with `/` separators) of a generation's lock file inside an
/// instance directory: `generations/<generation UUID>.lock`.
pub fn generation_lock_path(generation: Uuid) -> String {
    format!("{GENERATIONS_DIR}/{}.lock", generation.as_hyphenated())
}

// ---------------------------------------------------------------------------
// Occupancy and history
// ---------------------------------------------------------------------------

/// Context window usage as reported by the adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextOccupancy {
    /// Tokens currently in context.
    pub used: u64,
    /// Context window size; must be positive.
    pub size: u64,
}

impl ContextOccupancy {
    /// Build an occupancy, or `None` when `size` is zero.
    pub fn new(used: u64, size: u64) -> Option<Self> {
        (size > 0).then_some(Self { used, size })
    }

    /// Build an occupancy from loosely typed adapter values.
    ///
    /// Missing, negative, nonintegral, non-numeric or oversized inputs yield
    /// `None`, as does a zero size.
    pub fn from_json(
        used: Option<&serde_json::Value>,
        size: Option<&serde_json::Value>,
    ) -> Option<Self> {
        Self::new(json_count(used?)?, json_count(size?)?)
    }

    /// `min(100, floor(100 * used / size))`, or `None` when `size` is zero.
    pub fn percent(&self) -> Option<u8> {
        if self.size == 0 {
            return None;
        }
        if self.used >= self.size {
            return Some(100);
        }
        let percent = u128::from(self.used) * 100 / u128::from(self.size);
        Some(u8::try_from(percent).unwrap_or(100))
    }
}

/// Largest integer an `f64` represents exactly.
const MAX_EXACT_F64_INTEGER: f64 = 9_007_199_254_740_992.0;

fn json_count(value: &serde_json::Value) -> Option<u64> {
    let number = value.as_number()?;
    if let Some(count) = number.as_u64() {
        return Some(count);
    }
    let float = number.as_f64()?;
    (float.is_finite() && float >= 0.0 && float.fract() == 0.0 && float <= MAX_EXACT_F64_INTEGER)
        .then_some(float as u64)
}

/// How much visible history a cold-started session was given.
///
/// Counts concern messages supplied in the cold-start prompt only; they do not
/// claim the model understood them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryCoverage {
    /// Unique message events actually supplied; `None` when identities are
    /// missing, in which case no ratio is shown.
    pub read: Option<u64>,
    /// Messages in the conversation: exact when `exact`, otherwise a proven
    /// lower bound; `None` when unavailable.
    pub total: Option<u64>,
    /// Whether `total` is exact. Requires `read`, `total` and complete evidence.
    pub exact: bool,
    /// Whether the cold-start fetch was known to be incomplete.
    pub incomplete: bool,
}

impl HistoryCoverage {
    /// Check count consistency.
    pub fn validate(&self) -> Result<(), StatusError> {
        if let (Some(read), Some(total)) = (self.read, self.total) {
            if total < read {
                return Err(StatusError::InvalidHistory);
            }
        }
        if self.exact && (self.read.is_none() || self.total.is_none() || self.incomplete) {
            return Err(StatusError::InvalidHistory);
        }
        Ok(())
    }

    /// Wire fields `(read, total, mode)` for the supporting tag.
    ///
    /// - `exact`: complete history; read and total are both present;
    /// - `lower_bound`: known incomplete, or a total that is not exact (a
    ///   proven lower bound); total and read may be empty;
    /// - `unknown`: no completeness claim (no total, not incomplete).
    fn wire(&self) -> (String, String, &'static str) {
        let count = |value: Option<u64>| value.map(|v| v.to_string()).unwrap_or_default();
        let mode = if self.exact {
            "exact"
        } else if self.incomplete || self.total.is_some() {
            "lower_bound"
        } else {
            "unknown"
        };
        (count(self.read), count(self.total), mode)
    }
}

// ---------------------------------------------------------------------------
// State, reason, status
// ---------------------------------------------------------------------------

/// Session state vocabulary (closed).
///
/// Only `New` and `Resumed` are produced today; `Compacted` and `Restarted`
/// are reserved and validated so readers and writers agree on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// A fresh provider session.
    New,
    /// A previously stored provider session was resumed.
    Resumed,
    /// The context was compacted.
    Compacted,
    /// The session was restarted.
    Restarted,
}

impl SessionState {
    /// Stable wire string.
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Resumed => "resumed",
            Self::Compacted => "compacted",
            Self::Restarted => "restarted",
        }
    }
}

/// Why a session is in its state (bounded, closed vocabulary).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StatusReason {
    /// First session for this conversation.
    FirstInThread,
    /// Previous worker went idle and slept.
    IdleSleep,
    /// The harness restarted.
    HarnessRestart,
    /// The previous worker crashed.
    WorkerCrash,
    /// A sibling session lost its worker to a manual restart.
    WorkerRecycled,
    /// A manual restart with the requested cold-start history depth.
    UserRestart(RestartDepth),
    /// Manual compaction completed.
    UserCompact,
    /// Automatic compaction completed.
    AutoCompact,
    /// The stored session was discarded.
    Discarded,
    /// The session is live in another worker or process.
    LiveElsewhere,
    /// The adapter cannot resume sessions.
    ResumeUnsupported,
    /// Resuming failed; the payload is a short safe code (`[a-z0-9_-]`).
    ResumeFailed(String),
    /// Agent configuration changed.
    ConfigChanged,
    /// The context window overflowed.
    ContextOverflow,
    /// The turn limit was reached.
    MaxTurns,
}

impl StatusReason {
    /// Whether an actor may be attached to this explicit user transition.
    pub fn is_manual(&self) -> bool {
        matches!(self, Self::UserRestart(_) | Self::UserCompact)
    }

    /// Parse a wire reason; anything outside the vocabulary is rejected.
    pub fn from_wire(wire: &str) -> Result<Self, StatusError> {
        let reason = match wire {
            "first_in_thread" => Self::FirstInThread,
            "idle_sleep" => Self::IdleSleep,
            "harness_restart" => Self::HarnessRestart,
            "worker_crash" => Self::WorkerCrash,
            "worker_recycled" => Self::WorkerRecycled,
            "user_restart:none" => Self::UserRestart(RestartDepth::None),
            "user_restart:recent" => Self::UserRestart(RestartDepth::Recent),
            "user_restart:all" => Self::UserRestart(RestartDepth::All),
            "user_compact" => Self::UserCompact,
            "auto_compact" => Self::AutoCompact,
            "discarded" => Self::Discarded,
            "live_elsewhere" => Self::LiveElsewhere,
            "resume_unsupported" => Self::ResumeUnsupported,
            "config_changed" => Self::ConfigChanged,
            "context_overflow" => Self::ContextOverflow,
            "max_turns" => Self::MaxTurns,
            other => match other.strip_prefix(RESUME_FAILED_PREFIX) {
                Some(code) => Self::ResumeFailed(code.to_owned()),
                None => return Err(StatusError::InvalidReason),
            },
        };
        reason.validate()?;
        Ok(reason)
    }

    /// Stable wire string. Call [`Self::validate`] first for untrusted codes.
    pub fn as_wire(&self) -> String {
        match self {
            Self::FirstInThread => "first_in_thread".into(),
            Self::IdleSleep => "idle_sleep".into(),
            Self::HarnessRestart => "harness_restart".into(),
            Self::WorkerCrash => "worker_crash".into(),
            Self::WorkerRecycled => "worker_recycled".into(),
            Self::UserRestart(depth) => format!("user_restart:{}", depth.as_wire()),
            Self::UserCompact => "user_compact".into(),
            Self::AutoCompact => "auto_compact".into(),
            Self::Discarded => "discarded".into(),
            Self::LiveElsewhere => "live_elsewhere".into(),
            Self::ResumeUnsupported => "resume_unsupported".into(),
            Self::ConfigChanged => "config_changed".into(),
            Self::ContextOverflow => "context_overflow".into(),
            Self::MaxTurns => "max_turns".into(),
            Self::ResumeFailed(code) => format!("{RESUME_FAILED_PREFIX}{code}"),
        }
    }

    /// Check that a `resume_failed` code is a short safe token and that the
    /// wire form fits [`MAX_REASON_BYTES`].
    pub fn validate(&self) -> Result<(), StatusError> {
        if let Self::ResumeFailed(code) = self {
            let safe = !code.is_empty()
                && code.bytes().all(|b| {
                    b.is_ascii_digit() || b.is_ascii_lowercase() || b == b'_' || b == b'-'
                });
            if !safe || self.as_wire().len() > MAX_REASON_BYTES {
                return Err(StatusError::InvalidReason);
            }
        }
        Ok(())
    }
}

impl fmt::Display for StatusReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_wire())
    }
}

impl TryFrom<String> for StatusReason {
    type Error = StatusError;

    fn try_from(wire: String) -> Result<Self, Self::Error> {
        Self::from_wire(&wire)
    }
}

impl From<StatusReason> for String {
    fn from(reason: StatusReason) -> Self {
        reason.as_wire()
    }
}

impl Serialize for StatusReason {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.as_wire())
    }
}

impl<'de> Deserialize<'de> for StatusReason {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = String::deserialize(deserializer)?;
        Self::from_wire(&wire).map_err(serde::de::Error::custom)
    }
}

/// The reportable state of one session transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionStatus {
    /// Current state.
    pub state: SessionState,
    /// Why the session is in that state.
    pub reason: StatusReason,
    /// Random display ID; changes on acquisition/transition, stable across
    /// ordinary turns. Not a provider session ID or capability.
    pub transition_id: Uuid,
    /// Last reported context occupancy; `None` until known.
    pub occupancy: Option<ContextOccupancy>,
    /// Cold-start history coverage, when known.
    pub history: Option<HistoryCoverage>,
}

impl SessionStatus {
    /// Check reason, occupancy and history.
    pub fn validate(&self) -> Result<(), StatusError> {
        self.reason.validate()?;
        if self.occupancy.is_some_and(|o| o.size == 0) {
            return Err(StatusError::InvalidOccupancy);
        }
        if let Some(history) = &self.history {
            history.validate()?;
        }
        Ok(())
    }

    /// Wire tags for an outgoing message in `scope`.
    ///
    /// Returns the primary [`SESSION_TAG`] followed by [`SESSION_INFO_TAG`].
    /// The tags contain only vocabulary strings, a percentage, a transition
    /// UUID and counts: no path, key, provider ID or identity.
    pub fn event_tags(&self, scope: &StatusScope) -> Result<Vec<Vec<String>>, StatusError> {
        self.validate()?;
        let percent = self
            .occupancy
            .and_then(|o| o.percent())
            .map(|p| p.to_string())
            .unwrap_or_default();
        let (read, total, mode) = self
            .history
            .as_ref()
            .map_or((String::new(), String::new(), "unknown"), |h| h.wire());
        Ok(vec![
            vec![
                SESSION_TAG.to_owned(),
                self.state.as_wire().to_owned(),
                self.reason.as_wire(),
                percent,
            ],
            vec![
                SESSION_INFO_TAG.to_owned(),
                SESSION_INFO_VERSION.to_owned(),
                self.transition_id.as_hyphenated().to_string(),
                scope.kind().to_owned(),
                read,
                total,
                mode.to_owned(),
            ],
        ])
    }
}

// ---------------------------------------------------------------------------
// Snapshot envelope
// ---------------------------------------------------------------------------

/// The snapshot document written by the harness and read by the CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusEnvelope {
    /// Format version; v1 and [`STATUS_FORMAT_VERSION`] are accepted.
    pub version: u8,
    /// Who the status belongs to.
    pub identity: StatusIdentity,
    /// Harness instance that wrote the snapshot.
    pub instance_id: Uuid,
    /// Conversation the status belongs to.
    pub scope: StatusScope,
    /// Generation of the scope's status; names its lock file.
    pub generation: Uuid,
    /// The reportable state.
    pub status: SessionStatus,
    /// Optional manual-transition origin; forbidden in legacy schema v1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<ControlOrigin>,
}

impl StatusEnvelope {
    /// Build a current-version envelope.
    pub fn new(
        identity: StatusIdentity,
        instance_id: Uuid,
        scope: StatusScope,
        generation: Uuid,
        status: SessionStatus,
    ) -> Self {
        Self {
            version: STATUS_FORMAT_VERSION,
            identity,
            instance_id,
            scope,
            generation,
            status,
            origin: None,
        }
    }

    /// Check version, canonical identity, scope and status.
    pub fn validate(&self) -> Result<(), StatusError> {
        if !matches!(self.version, 1 | STATUS_FORMAT_VERSION) {
            return Err(StatusError::UnsupportedVersion);
        }
        if let Some(origin) = &self.origin {
            if self.version == 1 || !self.status.reason.is_manual() {
                return Err(StatusError::InvalidOrigin);
            }
            origin.validate()?;
        }
        self.identity.validate()?;
        self.scope.validate()?;
        self.status.validate()
    }

    /// Wire tags for this envelope's own scope.
    pub fn event_tags(&self) -> Result<Vec<Vec<String>>, StatusError> {
        self.validate()?;
        let mut tags = self.status.event_tags(&self.scope)?;
        if let Some(origin) = &self.origin {
            tags.push(
                ControlAction {
                    transition_id: self.status.transition_id,
                    origin: origin.clone(),
                }
                .event_tag()
                .ok_or(StatusError::InvalidOrigin)?,
            );
        }
        Ok(tags)
    }

    /// Serialize after validating, enforcing [`MAX_SNAPSHOT_BYTES`].
    pub fn to_snapshot_bytes(&self) -> Result<Vec<u8>, StatusError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| StatusError::Malformed)?;
        if bytes.len() > MAX_SNAPSHOT_BYTES {
            return Err(StatusError::TooLarge);
        }
        Ok(bytes)
    }
}

/// Parse and validate a snapshot.
///
/// Rejects oversized input, malformed JSON, unknown versions and any envelope
/// that fails [`StatusEnvelope::validate`]. The version is checked before the
/// remaining fields so a future format reports [`StatusError::UnsupportedVersion`].
pub fn parse_snapshot(bytes: &[u8]) -> Result<StatusEnvelope, StatusError> {
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err(StatusError::TooLarge);
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| StatusError::Malformed)?;
    match value.get("version").and_then(serde_json::Value::as_u64) {
        Some(1) if value.get("origin").is_some() => return Err(StatusError::InvalidOrigin),
        Some(1) => {}
        Some(v) if v == u64::from(STATUS_FORMAT_VERSION) => {}
        Some(_) => return Err(StatusError::UnsupportedVersion),
        None => return Err(StatusError::Malformed),
    }
    let envelope: StatusEnvelope =
        serde_json::from_value(value).map_err(|_| StatusError::Malformed)?;
    envelope.validate()?;
    Ok(envelope)
}

#[cfg(test)]
#[path = "session_status_tests.rs"]
mod tests;
