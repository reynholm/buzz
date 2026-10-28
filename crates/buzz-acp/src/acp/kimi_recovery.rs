//! Positive classification at Kimi error boundaries, before detail is lost.
//!
//! Internal provider log records are not JSON-RPC payloads. Only fields actually
//! present in the received error are consulted; raw messages are never retained.
use serde_json::Value;

/// Safe, actionable classification of a Kimi application failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureClass {
    OAuthConnection,
    OAuthRefreshLock,
    NetworkTimeout,
    Authentication,
    Configuration,
    Unknown,
}

impl FailureClass {
    /// Only positively recognized temporary failures authorize a retry.
    pub fn is_transient(self) -> bool {
        matches!(
            self,
            Self::OAuthConnection | Self::OAuthRefreshLock | Self::NetworkTimeout
        )
    }

    /// Stable diagnostic label containing no provider-supplied text.
    pub fn label(self) -> &'static str {
        match self {
            Self::OAuthConnection => "oauth_connection",
            Self::OAuthRefreshLock => "oauth_refresh_lock",
            Self::NetworkTimeout => "network_timeout",
            Self::Authentication => "authentication",
            Self::Configuration => "configuration",
            Self::Unknown => "unknown",
        }
    }

    fn reason(self) -> &'static str {
        match self {
            Self::OAuthConnection => "temporary OAuth connection failure",
            Self::OAuthRefreshLock => "temporary OAuth refresh lock failure",
            Self::NetworkTimeout => "temporary network timeout",
            Self::Authentication => "authentication failed",
            Self::Configuration => "configuration failed",
            Self::Unknown => "unclassified failure",
        }
    }
}

/// Bounded structural context safe to retain in queue and observer diagnostics.
#[derive(Clone, Debug)]
pub struct KimiFailure {
    pub code: i64,
    pub error_type: Option<String>,
    pub status: Option<u16>,
    pub class: FailureClass,
}

impl std::fmt::Display for KimiFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Kimi {} (RPC {})", self.class.reason(), self.code)
    }
}

fn safe_type(value: &str) -> bool {
    value.len() <= 96
        && (value.ends_with("Error") || value.ends_with("Timeout"))
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

impl KimiFailure {
    /// Read only approved error fields. Never serialize the whole RPC error.
    pub fn from_json(error: &Value) -> Self {
        let names: Vec<&str> = [
            "/data/name",
            "/data/type",
            "/data/error/name",
            "/data/error/type",
            "/name",
            "/type",
        ]
        .iter()
        .filter_map(|pointer| error.pointer(pointer).and_then(Value::as_str))
        .filter(|name| safe_type(name))
        .collect();
        let messages: Vec<&str> = [
            "/message",
            "/data/message",
            "/data/error/message",
            "/data/cause/message",
            "/data",
        ]
        .iter()
        .filter_map(|pointer| error.pointer(pointer).and_then(Value::as_str))
        .collect();
        let statuses: Vec<u16> = [
            "/status",
            "/statusCode",
            "/data/status",
            "/data/statusCode",
            "/data/error/status",
            "/data/error/statusCode",
            "/data/code",
            "/code",
        ]
        .iter()
        .filter_map(|pointer| error.pointer(pointer).and_then(Value::as_u64))
        .filter(|status| (100..=599).contains(status))
        .map(|status| status as u16)
        .collect();
        Self::from_fields(
            error.get("code").and_then(Value::as_i64).unwrap_or(-32000),
            &names,
            &messages,
            &statuses,
            false,
        )
    }

    /// Classify the existing Air v1 contract directly, without inventing RPC
    /// fields. Its access/login instruction overrides a timeout-like title.
    pub(super) fn from_session_failure(title: &str, auth_required: bool) -> Self {
        Self::from_fields(
            if auth_required { -32000 } else { -32603 },
            &[],
            &[title],
            &[],
            auth_required,
        )
    }

    fn from_fields(
        code: i64,
        names: &[&str],
        messages: &[&str],
        statuses: &[u16],
        auth_required: bool,
    ) -> Self {
        let named = |name: &str| {
            names.contains(&name)
                || messages.iter().any(|message| {
                    let message = message.trim();
                    message == name
                        || message
                            .strip_prefix(name)
                            .is_some_and(|tail| tail.starts_with(": "))
                })
        };
        let lower: Vec<String> = messages
            .iter()
            .map(|message| message.to_ascii_lowercase())
            .collect();
        let auth = auth_required
            || statuses.iter().any(|status| matches!(status, 401 | 403))
            || [
                "AuthenticationError",
                "UnauthorizedError",
                "ForbiddenError",
                "CredentialsError",
                "LoginRequiredError",
            ]
            .iter()
            .any(|name| named(name))
            || lower.iter().any(|message| {
                [
                    "unauthorized",
                    "re-authenticate",
                    "reauthenticate",
                    "revoked",
                    "login required",
                    "log in to",
                    "please log in",
                    "authentication required",
                    "not authenticated",
                    "invalid credentials",
                    "invalid api key",
                    "invalid access token",
                    "invalid refresh token",
                ]
                .iter()
                .any(|marker| message.contains(marker))
                    || message
                        .split(|ch: char| !ch.is_ascii_alphanumeric())
                        .any(|word| matches!(word, "401" | "403"))
            });
        let configuration = ["ConfigurationError", "ConfigError", "ModelNotFoundError"]
            .iter()
            .any(|name| named(name))
            || lower.iter().any(|message| {
                message.contains("model not found")
                    || message.contains("unknown model")
                    || message.contains("model does not exist")
            });
        let lock = messages.iter().any(|message| {
            matches!(
                message.trim(),
                "Unable to acquire OAuth refresh lock"
                    | "OAuthError: Unable to acquire OAuth refresh lock"
            )
        });
        let connection = lower.iter().any(|message| {
            [
                "timeout",
                "timed out",
                "connection",
                "connect",
                "temporarily unavailable",
            ]
            .iter()
            .any(|marker| message.contains(marker))
        });
        let class = if auth {
            FailureClass::Authentication
        } else if configuration {
            FailureClass::Configuration
        } else if lock
            && (named("OAuthError")
                || messages
                    .iter()
                    .any(|message| message.trim() == "Unable to acquire OAuth refresh lock"))
        {
            FailureClass::OAuthRefreshLock
        } else if named("OAuthConnectionError") && connection {
            FailureClass::OAuthConnection
        } else if [
            "APITimeoutError",
            "ConnectTimeout",
            "ReadTimeout",
            "TimeoutError",
            "APIConnectionError",
        ]
        .iter()
        .any(|name| named(name))
        {
            FailureClass::NetworkTimeout
        } else {
            FailureClass::Unknown
        };
        let error_type = names.first().map(|name| (*name).to_owned()).or_else(|| {
            [
                "OAuthConnectionError",
                "OAuthError",
                "APITimeoutError",
                "ConnectTimeout",
                "ReadTimeout",
                "TimeoutError",
                "APIConnectionError",
            ]
            .into_iter()
            .find(|name| named(name))
            .map(str::to_owned)
        });
        Self {
            code,
            error_type,
            status: statuses.first().copied(),
            class,
        }
    }
}
