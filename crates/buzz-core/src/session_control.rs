//! Pure session-control grammar and bounded public reaction/action metadata.
//!
//! These values describe agent self-reports; they never grant permissions.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Name of the supporting kind-7 reaction tag.
pub const SESSION_RESULT_TAG: &str = "buzz-session-result";
/// Name of the optional manual-transition actor tag.
pub const SESSION_ACTION_TAG: &str = "buzz-session-action";
/// Maximum signed command age in seconds, including the boundary.
pub const CONTROL_MAX_AGE_SECS: u64 = 900;
/// Maximum accepted future clock skew in seconds.
pub const CONTROL_FUTURE_SKEW_SECS: u64 = 30;

/// Cold-start history requested by an explicit restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestartDepth {
    /// Only the next trigger and its assignment.
    None,
    /// Root, assignment and the configured recent window.
    Recent,
    /// The current target thread (conversation for DMs), within history budgets.
    All,
}

impl RestartDepth {
    /// Stable reason suffix.
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Recent => "recent",
            Self::All => "all",
        }
    }
}

/// Exact kind-9 session command, after trimming external whitespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionCommand {
    /// Compact an existing session.
    Compact,
    /// Fence the current session and request a cold start.
    Restart(RestartDepth),
}

impl SessionCommand {
    /// Parse only the five accepted spellings. Targeting and authorization are
    /// separate gates and must be checked by the caller.
    pub fn parse(kind: u16, content: &str) -> Option<Self> {
        if kind != 9 {
            return None;
        }
        match content.trim() {
            "!compact" => Some(Self::Compact),
            "!restart" | "!rotate" => Some(Self::Restart(RestartDepth::Recent)),
            "!restart none" => Some(Self::Restart(RestartDepth::None)),
            "!restart all" => Some(Self::Restart(RestartDepth::All)),
            _ => None,
        }
    }

    /// Canonical command text; recent restarts use `!restart`.
    pub fn as_content(self) -> &'static str {
        match self {
            Self::Compact => "!compact",
            Self::Restart(RestartDepth::None) => "!restart none",
            Self::Restart(RestartDepth::Recent) => "!restart",
            Self::Restart(RestartDepth::All) => "!restart all",
        }
    }
}

/// Signed command identity and effective author from the existing authorization gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlOrigin {
    /// Canonical signed event ID (64 lowercase hex characters).
    pub command_event_id: String,
    /// Effective author's public key (64 lowercase hex characters).
    pub actor_pubkey: String,
}

impl ControlOrigin {
    /// Reject malformed or noncanonical identities.
    pub fn validate(&self) -> Result<(), crate::session_status::StatusError> {
        if is_canonical_id(&self.command_event_id) && is_canonical_id(&self.actor_pubkey) {
            Ok(())
        } else {
            Err(crate::session_status::StatusError::InvalidOrigin)
        }
    }
}

/// Closed public failure vocabulary; adapter errors and paths never go on wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlFailure {
    /// No existing resumable session.
    NoSession,
    /// Runtime has no supported compact operation.
    Unsupported,
    /// Conversation scope could not be verified.
    ScopeUnavailable,
    /// Queue, receipt capacity or target is occupied.
    Busy,
    /// Target generation changed.
    StaleTarget,
    /// Accepted request expired before execution.
    StaleRequest,
    /// Durable storage failed.
    StoreUnavailable,
    /// Existing session could not resume.
    ResumeFailed,
    /// Adapter reported compact failure.
    CompactFailed,
    /// Operation was cancelled.
    Cancelled,
    /// Deadline elapsed.
    Timeout,
    /// Completion could not be confirmed.
    Unconfirmed,
    /// A restart superseded the pending compact.
    Superseded,
    /// Internal failure without public diagnostics.
    Internal,
}

impl ControlFailure {
    /// Parse exactly the safe public vocabulary.
    pub fn from_wire(wire: &str) -> Option<Self> {
        Some(match wire {
            "no_session" => Self::NoSession,
            "unsupported" => Self::Unsupported,
            "scope_unavailable" => Self::ScopeUnavailable,
            "busy" => Self::Busy,
            "stale_target" => Self::StaleTarget,
            "stale_request" => Self::StaleRequest,
            "store_unavailable" => Self::StoreUnavailable,
            "resume_failed" => Self::ResumeFailed,
            "compact_failed" => Self::CompactFailed,
            "cancelled" => Self::Cancelled,
            "timeout" => Self::Timeout,
            "unconfirmed" => Self::Unconfirmed,
            "superseded" => Self::Superseded,
            "internal" => Self::Internal,
            _ => return None,
        })
    }

    /// Safe code for public metadata.
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::NoSession => "no_session",
            Self::Unsupported => "unsupported",
            Self::ScopeUnavailable => "scope_unavailable",
            Self::Busy => "busy",
            Self::StaleTarget => "stale_target",
            Self::StaleRequest => "stale_request",
            Self::StoreUnavailable => "store_unavailable",
            Self::ResumeFailed => "resume_failed",
            Self::CompactFailed => "compact_failed",
            Self::Cancelled => "cancelled",
            Self::Timeout => "timeout",
            Self::Unconfirmed => "unconfirmed",
            Self::Superseded => "superseded",
            Self::Internal => "internal",
        }
    }
}

/// Terminal operation outcome; pending presentation is deliberately separate.
pub type ControlOutcome = Result<(), ControlFailure>;

/// Intermediate or terminal presentation of a command result on wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlResult {
    /// Accepted restart waiting for foreign work to stop.
    Pending,
    /// Confirmed terminal outcome.
    Terminal(ControlOutcome),
}

impl ControlResult {
    /// Standard kind-7 reaction content matching this presentation.
    pub fn reaction(self) -> &'static str {
        match self {
            Self::Pending => "⏳",
            Self::Terminal(Ok(())) => "✅",
            Self::Terminal(Err(_)) => "❌",
        }
    }

    /// Combine reports for the same command and reporting agent. Terminal
    /// results override pending in either order; conflicting terminals are unconfirmed.
    pub fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Pending, value) | (value, Self::Pending) => value,
            (a, b) if a == b => a,
            _ => Self::Terminal(Err(ControlFailure::Unconfirmed)),
        }
    }
}

/// Optional structured result attached to a standard reaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionResult {
    /// Canonical signed command event ID.
    pub command_id: String,
    /// Presentation consistent with the reaction content.
    pub result: ControlResult,
}

impl SessionResult {
    /// Encode the exact five-field v1 supporting tag.
    pub fn event_tag(&self) -> Option<Vec<String>> {
        if !is_canonical_id(&self.command_id) {
            return None;
        }
        let (outcome, code) = match self.result {
            ControlResult::Pending => ("pending", ""),
            ControlResult::Terminal(Ok(())) => ("ok", ""),
            ControlResult::Terminal(Err(code)) => ("error", code.as_wire()),
        };
        Some(vec![
            SESSION_RESULT_TAG.into(),
            "1".into(),
            self.command_id.clone(),
            outcome.into(),
            code.into(),
        ])
    }

    /// Strictly parse all matching tags. Identical duplicates are allowed;
    /// malformed/conflicting duplicates or inconsistent reaction content reject enrichment.
    pub fn from_tags(content: &str, tags: &[Vec<String>]) -> Option<Self> {
        let mut parsed = None;
        for tag in tags
            .iter()
            .filter(|t| t.first().is_some_and(|s| s == SESSION_RESULT_TAG))
        {
            if tag.len() != 5 || tag[1] != "1" || !is_canonical_id(&tag[2]) {
                return None;
            }
            let result = match (tag[3].as_str(), tag[4].as_str()) {
                ("pending", "") => ControlResult::Pending,
                ("ok", "") => ControlResult::Terminal(Ok(())),
                ("error", code) => ControlResult::Terminal(Err(ControlFailure::from_wire(code)?)),
                _ => return None,
            };
            if result.reaction() != content {
                return None;
            }
            let next = Self {
                command_id: tag[2].clone(),
                result,
            };
            if parsed.as_ref().is_some_and(|prior| *prior != next) {
                return None;
            }
            parsed = Some(next);
        }
        parsed
    }
}

/// Actor enrichment for one validated manual status transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlAction {
    /// Display transition ID matching the supporting info tag.
    pub transition_id: Uuid,
    /// Agent self-report; never permission evidence.
    pub origin: ControlOrigin,
}

impl ControlAction {
    /// Encode the exact five-field v1 actor tag after validating both IDs.
    pub fn event_tag(&self) -> Option<Vec<String>> {
        self.origin.validate().ok()?;
        Some(vec![
            SESSION_ACTION_TAG.into(),
            "1".into(),
            self.transition_id.to_string(),
            self.origin.command_event_id.clone(),
            self.origin.actor_pubkey.clone(),
        ])
    }

    /// Read actor metadata only beside valid matching primary/info tags for a
    /// manual transition. Any malformed/conflicting duplicate removes enrichment.
    pub fn from_tags(tags: &[Vec<String>]) -> Option<Self> {
        let primary = unique_tag(tags, crate::session_status::SESSION_TAG)?;
        let info = unique_tag(tags, crate::session_status::SESSION_INFO_TAG)?;
        if primary.len() != 4
            || !matches!(
                primary[1].as_str(),
                "new" | "resumed" | "compacted" | "restarted"
            )
        {
            return None;
        }
        let reason = crate::session_status::StatusReason::from_wire(&primary[2]).ok()?;
        if !reason.is_manual()
            || (!primary[3].is_empty()
                && primary[3]
                    .parse::<u8>()
                    .ok()
                    .filter(|p| *p <= 100)
                    .map(|p| p.to_string())
                    != Some(primary[3].clone()))
        {
            return None;
        }
        if info.len() != 7
            || info[1] != "1"
            || !matches!(info[3].as_str(), "thread" | "conversation")
        {
            return None;
        }
        let transition_id = canonical_uuid(&info[2])?;
        let count = |wire: &str| -> Option<Option<u64>> {
            if wire.is_empty() {
                Some(None)
            } else {
                let n = wire.parse::<u64>().ok()?;
                (n.to_string() == wire).then_some(Some(n))
            }
        };
        let read = count(&info[4])?;
        let total = count(&info[5])?;
        if read.zip(total).is_some_and(|(r, t)| r > t) {
            return None;
        }
        match info[6].as_str() {
            "exact" if read.is_some() && total.is_some() => {}
            "lower_bound" => {}
            "unknown" if total.is_none() => {}
            _ => return None,
        }
        let action = unique_tag(tags, SESSION_ACTION_TAG)?;
        if action.len() != 5 || action[1] != "1" || canonical_uuid(&action[2])? != transition_id {
            return None;
        }
        let origin = ControlOrigin {
            command_event_id: action[3].clone(),
            actor_pubkey: action[4].clone(),
        };
        origin.validate().ok()?;
        Some(Self {
            transition_id,
            origin,
        })
    }
}

fn unique_tag<'a>(tags: &'a [Vec<String>], name: &str) -> Option<&'a Vec<String>> {
    let mut matching = tags.iter().filter(|t| t.first().is_some_and(|s| s == name));
    let first = matching.next()?;
    matching.all(|t| t == first).then_some(first)
}

fn canonical_uuid(wire: &str) -> Option<Uuid> {
    let id = Uuid::parse_str(wire).ok()?;
    (id.to_string() == wire).then_some(id)
}

/// Whether an event ID or public-key encoding is canonical lowercase hex.
pub fn is_canonical_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Freshness gate for signed controls; ordinary messages do not use this rule.
pub fn control_is_fresh(created_at: u64, now: u64) -> bool {
    created_at <= now.saturating_add(CONTROL_FUTURE_SKEW_SECS)
        && now <= created_at.saturating_add(CONTROL_MAX_AGE_SECS)
}
