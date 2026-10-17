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
//! In the info tag, `unknown` with a non-empty `<read>` means that the total
//! is unavailable (for example because the cold-start fetch was incomplete).

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;
use uuid::Uuid;

/// Snapshot format version written and accepted by this crate.
pub const STATUS_FORMAT_VERSION: u8 = 1;
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
    fn wire(&self) -> (String, String, &'static str) {
        let Some(read) = self.read else {
            return (String::new(), String::new(), "unknown");
        };
        match (self.total, self.exact) {
            (Some(total), true) => (read.to_string(), total.to_string(), "exact"),
            (Some(total), false) => (read.to_string(), total.to_string(), "lower_bound"),
            (None, _) => (read.to_string(), String::new(), "unknown"),
        }
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
    /// Parse a wire reason; anything outside the vocabulary is rejected.
    pub fn from_wire(wire: &str) -> Result<Self, StatusError> {
        let reason = match wire {
            "first_in_thread" => Self::FirstInThread,
            "idle_sleep" => Self::IdleSleep,
            "harness_restart" => Self::HarnessRestart,
            "worker_crash" => Self::WorkerCrash,
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
                "1".to_owned(),
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
    /// Format version; must equal [`STATUS_FORMAT_VERSION`].
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
        }
    }

    /// Check version, canonical identity, scope and status.
    pub fn validate(&self) -> Result<(), StatusError> {
        if self.version != STATUS_FORMAT_VERSION {
            return Err(StatusError::UnsupportedVersion);
        }
        self.identity.validate()?;
        self.scope.validate()?;
        self.status.validate()
    }

    /// Wire tags for this envelope's own scope.
    pub fn event_tags(&self) -> Result<Vec<Vec<String>>, StatusError> {
        self.status.event_tags(&self.scope)
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
mod tests {
    use super::*;
    use serde_json::json;

    fn agent() -> String {
        nostr::Keys::generate().public_key().to_hex()
    }

    fn root() -> String {
        "ab".repeat(32)
    }

    fn status() -> SessionStatus {
        SessionStatus {
            state: SessionState::Resumed,
            reason: StatusReason::IdleSleep,
            transition_id: Uuid::new_v4(),
            occupancy: ContextOccupancy::new(64, 100),
            history: None,
        }
    }

    fn envelope() -> StatusEnvelope {
        StatusEnvelope::new(
            canonical_identity("ws://relay.example", &agent()).unwrap(),
            Uuid::new_v4(),
            StatusScope {
                channel_id: Uuid::new_v4(),
                thread_root: Some(root()),
            },
            Uuid::new_v4(),
            status(),
        )
    }

    // -- occupancy ---------------------------------------------------------

    #[test]
    fn percent_table() {
        let cases: &[(u64, u64, Option<u8>)] = &[
            (64, 100, Some(64)),
            (80, 100, Some(80)),
            (0, 100, Some(0)),
            (79, 100, Some(79)),
            (1, 3, Some(33)),
            (2, 3, Some(66)),
            (5, 0, None),
            (0, 0, None),
            (101, 100, Some(100)),
            (u64::MAX, 1, Some(100)),
            (u64::MAX, u64::MAX, Some(100)),
            (u64::MAX - 1, u64::MAX, Some(99)),
            (u64::MAX / 2, u64::MAX, Some(49)),
        ];
        for &(used, size, expected) in cases {
            let occupancy = ContextOccupancy { used, size };
            assert_eq!(occupancy.percent(), expected, "used={used} size={size}");
        }
    }

    #[test]
    fn occupancy_constructor_rejects_zero_size() {
        assert_eq!(ContextOccupancy::new(1, 0), None);
        assert_eq!(
            ContextOccupancy::new(1, 2),
            Some(ContextOccupancy { used: 1, size: 2 })
        );
    }

    #[test]
    fn occupancy_from_json_rejects_unusable_inputs() {
        let ok = ContextOccupancy::from_json(Some(&json!(64)), Some(&json!(100)));
        assert_eq!(ok, ContextOccupancy::new(64, 100));
        let integral_float = ContextOccupancy::from_json(Some(&json!(64.0)), Some(&json!(100.0)));
        assert_eq!(integral_float, ContextOccupancy::new(64, 100));
        let bad: &[(Option<serde_json::Value>, Option<serde_json::Value>)] = &[
            (None, Some(json!(100))),
            (Some(json!(1)), None),
            (Some(json!(-1)), Some(json!(100))),
            (Some(json!(1)), Some(json!(-100))),
            (Some(json!(1.5)), Some(json!(100))),
            (Some(json!(1)), Some(json!(100.25))),
            (Some(json!("64")), Some(json!(100))),
            (Some(json!(null)), Some(json!(100))),
            (Some(json!(1)), Some(json!(0))),
            (Some(json!(1e30)), Some(json!(100))),
        ];
        for (used, size) in bad {
            assert_eq!(
                ContextOccupancy::from_json(used.as_ref(), size.as_ref()),
                None,
                "used={used:?} size={size:?}"
            );
        }
    }

    // -- tags --------------------------------------------------------------

    #[test]
    fn primary_tag_carries_percent_and_empty_when_unavailable() {
        let scope = StatusScope {
            channel_id: Uuid::new_v4(),
            thread_root: None,
        };
        let mut s = status();
        let tags = s.event_tags(&scope).unwrap();
        assert_eq!(tags[0], ["buzz-session", "resumed", "idle_sleep", "64"]);

        s.occupancy = ContextOccupancy::new(0, 100);
        assert_eq!(s.event_tags(&scope).unwrap()[0][3], "0");

        s.occupancy = None;
        let tags = s.event_tags(&scope).unwrap();
        assert_eq!(tags[0], ["buzz-session", "resumed", "idle_sleep", ""]);

        // A directly constructed zero-size occupancy is rejected, never shown as 0%.
        s.occupancy = Some(ContextOccupancy { used: 3, size: 0 });
        assert_eq!(s.event_tags(&scope), Err(StatusError::InvalidOccupancy));
    }

    #[test]
    fn info_tag_encodes_each_history_mode() {
        let thread = StatusScope {
            channel_id: Uuid::new_v4(),
            thread_root: Some(root()),
        };
        let conversation = StatusScope {
            channel_id: thread.channel_id,
            thread_root: None,
        };
        let mut s = status();
        let id = s.transition_id.to_string();
        let info = |s: &SessionStatus, scope: &StatusScope| s.event_tags(scope).unwrap()[1].clone();

        s.history = Some(HistoryCoverage {
            read: Some(12),
            total: Some(12),
            exact: true,
            incomplete: false,
        });
        assert_eq!(
            info(&s, &thread),
            ["buzz-session-info", "1", &id, "thread", "12", "12", "exact"]
        );

        s.history = Some(HistoryCoverage {
            read: Some(12),
            total: Some(40),
            exact: false,
            incomplete: false,
        });
        assert_eq!(
            info(&s, &conversation),
            [
                "buzz-session-info",
                "1",
                &id,
                "conversation",
                "12",
                "40",
                "lower_bound"
            ]
        );

        s.history = Some(HistoryCoverage {
            read: Some(12),
            total: None,
            exact: false,
            incomplete: true,
        });
        assert_eq!(
            info(&s, &thread),
            ["buzz-session-info", "1", &id, "thread", "12", "", "unknown"]
        );

        // No identities at all: no counts, never zeros.
        s.history = Some(HistoryCoverage {
            read: None,
            total: Some(7),
            exact: false,
            incomplete: false,
        });
        assert_eq!(
            info(&s, &thread),
            ["buzz-session-info", "1", &id, "thread", "", "", "unknown"]
        );

        s.history = None;
        assert_eq!(
            info(&s, &thread),
            ["buzz-session-info", "1", &id, "thread", "", "", "unknown"]
        );
    }

    #[test]
    fn inconsistent_history_is_rejected() {
        let bad = [
            HistoryCoverage {
                read: Some(5),
                total: Some(4),
                exact: false,
                incomplete: false,
            },
            HistoryCoverage {
                read: Some(5),
                total: Some(5),
                exact: true,
                incomplete: true,
            },
            HistoryCoverage {
                read: Some(5),
                total: None,
                exact: true,
                incomplete: false,
            },
            HistoryCoverage {
                read: None,
                total: Some(5),
                exact: true,
                incomplete: false,
            },
        ];
        let scope = StatusScope {
            channel_id: Uuid::new_v4(),
            thread_root: None,
        };
        for history in bad {
            let mut s = status();
            s.history = Some(history);
            assert_eq!(
                s.event_tags(&scope),
                Err(StatusError::InvalidHistory),
                "{history:?}"
            );
        }
    }

    #[test]
    fn tags_leak_no_identity_path_or_provider_id() {
        let private = "/home/someone/private-dir";
        let relay = format!("ws://relay.example/{}?token={private}", "base");
        let agent = agent();
        let env = StatusEnvelope::new(
            canonical_identity(&relay, &agent).unwrap(),
            Uuid::new_v4(),
            StatusScope {
                channel_id: Uuid::new_v4(),
                thread_root: Some(root()),
            },
            Uuid::new_v4(),
            status(),
        );
        let text = env.event_tags().unwrap().concat().join("|");
        for secret in [
            private,
            "private-dir",
            "relay.example",
            &agent,
            &env.scope.channel_id.to_string(),
            &env.scope.thread_root.clone().unwrap(),
            &env.instance_id.to_string(),
            &env.generation.to_string(),
        ] {
            assert!(!text.contains(secret), "tags leaked {secret}");
        }
        // A path smuggled through the reason code is rejected, not emitted.
        let mut s = status();
        s.reason = StatusReason::ResumeFailed(format!("..{private}"));
        assert_eq!(s.event_tags(&env.scope), Err(StatusError::InvalidReason));
    }

    // -- reasons -----------------------------------------------------------

    #[test]
    fn reason_vocabulary_round_trips() {
        for wire in [
            "first_in_thread",
            "idle_sleep",
            "harness_restart",
            "worker_crash",
            "discarded",
            "live_elsewhere",
            "resume_unsupported",
            "config_changed",
            "context_overflow",
            "max_turns",
            "resume_failed:not_found",
            "resume_failed:rpc_-32603",
            "resume_failed:timeout",
            "resume_failed:protocol",
        ] {
            let reason = StatusReason::from_wire(wire).expect(wire);
            assert_eq!(reason.as_wire(), wire);
        }
    }

    #[test]
    fn unknown_or_unsafe_reasons_are_rejected() {
        let long = format!("resume_failed:{}", "a".repeat(83));
        for wire in [
            "",
            "bogus",
            "First_In_Thread",
            "resume_failed:",
            "resume_failed:Has Space",
            "resume_failed:a/b",
            "resume_failed:C0DE",
            "resume_failed:caf\u{e9}",
            "resume_failed:line\nbreak",
            long.as_str(),
        ] {
            assert_eq!(
                StatusReason::from_wire(wire),
                Err(StatusError::InvalidReason),
                "{wire:?}"
            );
        }
        let longest = format!("resume_failed:{}", "a".repeat(82));
        assert_eq!(longest.len(), MAX_REASON_BYTES);
        assert!(StatusReason::from_wire(&longest).is_ok());
    }

    #[test]
    fn state_vocabulary_is_closed_over_four_values() {
        let states = [
            (SessionState::New, "new"),
            (SessionState::Resumed, "resumed"),
            (SessionState::Compacted, "compacted"),
            (SessionState::Restarted, "restarted"),
        ];
        for (state, wire) in states {
            assert_eq!(state.as_wire(), wire);
            assert_eq!(serde_json::to_value(state).unwrap(), json!(wire));
            assert_eq!(
                serde_json::from_value::<SessionState>(json!(wire)).unwrap(),
                state
            );
        }
        assert!(serde_json::from_value::<SessionState>(json!("paused")).is_err());
    }

    // -- identity ----------------------------------------------------------

    #[test]
    fn identity_matches_same_spelling_and_transport_counterparts() {
        let a = agent();
        let base = canonical_identity("wss://relay.example", &a).unwrap();
        for same in [
            "wss://relay.example",
            "wss://relay.example/",
            "WSS://Relay.Example:443/",
            "https://relay.example",
            "https://relay.example/",
            "  https://RELAY.example:443  ",
        ] {
            assert_eq!(canonical_identity(same, &a).unwrap(), base, "{same}");
        }
        let ws = canonical_identity("ws://relay.example", &a).unwrap();
        assert_eq!(
            canonical_identity("http://relay.example:80/", &a).unwrap(),
            ws
        );
        assert_ne!(ws, base, "plain and secure transports stay distinct");
        assert_eq!(base.relay, "wss://relay.example");
        assert_eq!(base.agent, a);
        // Idempotent.
        assert_eq!(canonical_identity(&base.relay, &base.agent).unwrap(), base);
        // Uppercase key hex normalizes.
        assert_eq!(
            canonical_identity("wss://relay.example", &a.to_uppercase()).unwrap(),
            base
        );
    }

    #[test]
    fn identity_differs_on_host_agent_port_path_and_query() {
        let a = agent();
        let id = |relay: &str, agent: &str| canonical_identity(relay, agent).unwrap();
        let base = id("ws://relay.example", &a);
        for other in [
            "ws://other.example",
            "ws://relay.example:8080",
            "ws://relay.example/tenant",
            "ws://relay.example/tenant/",
            "ws://relay.example/?community=a",
            "wss://relay.example",
        ] {
            assert_ne!(id(other, &a), base, "{other}");
        }
        assert_ne!(
            id("ws://relay.example/tenant", &a),
            id("ws://relay.example/tenant/", &a)
        );
        assert_ne!(
            id("ws://relay.example/?community=a", &a),
            id("ws://relay.example/?community=b", &a)
        );
        assert_ne!(
            id("ws://relay.example", &a),
            id("ws://relay.example", &agent())
        );

        // Each loopback spelling is its own host.
        let spellings = ["ws://localhost", "ws://127.0.0.1", "ws://[::1]"];
        for (i, x) in spellings.iter().enumerate() {
            for y in &spellings[i + 1..] {
                assert_ne!(id(x, &a), id(y, &a), "{x} vs {y}");
            }
        }
        assert_eq!(id("ws://LOCALHOST:80", &a), id("ws://localhost", &a));
    }

    #[test]
    fn invalid_identity_inputs_are_rejected() {
        let a = agent();
        for relay in [
            "",
            "not a url",
            "ftp://relay.example",
            "file:///tmp/x",
            "ws://",
            "wss://user@relay.example",
            "wss://user:pw@relay.example",
            "wss://relay.example/#frag",
            "mailto:x@y.z",
        ] {
            assert_eq!(
                canonical_identity(relay, &a),
                Err(StatusError::InvalidRelay),
                "{relay:?}"
            );
        }
        for key in ["", "xyz", "npub1abc", &"0".repeat(63), &"g".repeat(64)] {
            assert_eq!(
                canonical_identity("ws://relay.example", key),
                Err(StatusError::InvalidAgent),
                "{key:?}"
            );
        }
    }

    // -- scope and names ---------------------------------------------------

    #[test]
    fn scope_file_names_are_distinct_and_path_safe() {
        let channel = Uuid::new_v4();
        let conversation = StatusScope {
            channel_id: channel,
            thread_root: None,
        };
        let thread = StatusScope {
            channel_id: channel,
            thread_root: Some(root()),
        };
        let other_root = StatusScope {
            channel_id: channel,
            thread_root: Some("cd".repeat(32)),
        };
        let names: Vec<String> = [&conversation, &thread, &other_root]
            .iter()
            .map(|s| scope_file_name(s).unwrap())
            .collect();
        assert_eq!(names[0], format!("{channel}.conversation.json"));
        assert_eq!(names[1], format!("{channel}.thread-{}.json", root()));
        assert_ne!(names[1], names[2]);
        for name in &names {
            assert!(name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-'));
            assert!(!name.contains(".."));
        }
    }

    #[test]
    fn invalid_thread_roots_never_become_file_names() {
        for root in [
            "",
            "ab",
            "../../etc/passwd",
            &"AB".repeat(32),
            &"zz".repeat(32),
            &format!("{}/", "a".repeat(63)),
            &"a".repeat(65),
        ] {
            let scope = StatusScope {
                channel_id: Uuid::new_v4(),
                thread_root: Some(root.to_owned()),
            };
            assert_eq!(
                scope_file_name(&scope),
                Err(StatusError::InvalidThreadRoot),
                "{root:?}"
            );
        }
    }

    #[test]
    fn instance_layout_names() {
        let generation = Uuid::new_v4();
        assert_eq!(INSTANCE_LOCK_FILE, "instance.lock");
        assert_eq!(
            generation_lock_path(generation),
            format!("generations/{generation}.lock")
        );
    }

    // -- snapshot ----------------------------------------------------------

    #[test]
    fn snapshot_round_trips() {
        let mut env = envelope();
        env.status.history = Some(HistoryCoverage {
            read: Some(3),
            total: Some(9),
            exact: false,
            incomplete: false,
        });
        let bytes = env.to_snapshot_bytes().unwrap();
        assert!(bytes.len() <= MAX_SNAPSHOT_BYTES);
        assert_eq!(parse_snapshot(&bytes).unwrap(), env);
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["version"], 1);
        assert_eq!(value["status"]["reason"], "idle_sleep");
        assert_eq!(value["status"]["state"], "resumed");
    }

    #[test]
    fn snapshot_rejects_unknown_versions_and_bad_shapes() {
        let good: serde_json::Value =
            serde_json::from_slice(&envelope().to_snapshot_bytes().unwrap()).unwrap();
        let parse = |v: &serde_json::Value| parse_snapshot(&serde_json::to_vec(v).unwrap());

        for version in [json!(0), json!(2), json!(255), json!(1000)] {
            let mut v = good.clone();
            v["version"] = version;
            assert_eq!(parse(&v), Err(StatusError::UnsupportedVersion));
        }
        let mut v = good.clone();
        v.as_object_mut().unwrap().remove("version");
        assert_eq!(parse(&v), Err(StatusError::Malformed));

        let mut v = good.clone();
        v["extra"] = json!(1);
        assert_eq!(parse(&v), Err(StatusError::Malformed));

        let mut v = good.clone();
        v["status"]["state"] = json!("paused");
        assert_eq!(parse(&v), Err(StatusError::Malformed));

        let mut v = good.clone();
        v["status"]["reason"] = json!("resume_failed:/etc/passwd");
        assert_eq!(parse(&v), Err(StatusError::Malformed));

        let mut v = good.clone();
        v["scope"]["thread_root"] = json!("../x");
        assert_eq!(parse(&v), Err(StatusError::InvalidThreadRoot));

        let mut v = good.clone();
        v["status"]["occupancy"] = json!({"used": 1, "size": 0});
        assert_eq!(parse(&v), Err(StatusError::InvalidOccupancy));

        let mut v = good.clone();
        v["status"]["history"] =
            json!({"read": 9, "total": 3, "exact": false, "incomplete": false});
        assert_eq!(parse(&v), Err(StatusError::InvalidHistory));

        // Non-canonical identity (uppercase scheme/host, http scheme).
        let mut v = good.clone();
        v["identity"]["relay"] = json!("http://relay.example");
        assert_eq!(parse(&v), Err(StatusError::InvalidRelay));

        assert_eq!(parse_snapshot(b"not json"), Err(StatusError::Malformed));
        assert_eq!(parse_snapshot(b""), Err(StatusError::Malformed));
        assert_eq!(parse_snapshot(b"[1]"), Err(StatusError::Malformed));
    }

    #[test]
    fn snapshot_size_limit_is_enforced_on_both_sides() {
        let env = envelope();
        let bytes = env.to_snapshot_bytes().unwrap();

        // Pad inside valid JSON so only the size limit can reject it.
        let mut padded = bytes.clone();
        padded.truncate(padded.len() - 1);
        padded.extend_from_slice(b",\"pad\":\"");
        padded.resize(MAX_SNAPSHOT_BYTES + 1, b'x');
        assert_eq!(parse_snapshot(&padded), Err(StatusError::TooLarge));
        assert_eq!(
            parse_snapshot(&vec![b' '; MAX_SNAPSHOT_BYTES + 1]),
            Err(StatusError::TooLarge)
        );

        // Exactly at the limit (trailing whitespace) is still accepted.
        let mut at_limit = bytes;
        at_limit.resize(MAX_SNAPSHOT_BYTES, b' ');
        assert_eq!(parse_snapshot(&at_limit), Ok(env));

        // The writer side refuses oversized documents too.
        let mut huge = envelope();
        huge.identity.relay = format!("ws://relay.example/{}", "p".repeat(MAX_SNAPSHOT_BYTES));
        assert_eq!(huge.to_snapshot_bytes(), Err(StatusError::TooLarge));
    }
}
