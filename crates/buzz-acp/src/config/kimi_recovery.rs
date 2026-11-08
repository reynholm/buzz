//! Operator configuration for the opt-in Kimi logical-work profile.
use super::ConfigError;
use crate::queue::RecoveryPolicy;
use std::time::Duration;

/// Visibility policy for permission lifecycle telemetry. Compatibility is only
/// selected automatically for an exact, verified adapter identity and version.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum KimiPermissionVisibility {
    #[default]
    Auto,
    Observable,
    Unknown,
}

#[derive(Debug, Clone, clap::Args)]
pub struct KimiRecoveryArgs {
    /// Permission telemetry policy: exact verified compatibility, strict, or unknown.
    #[arg(
        long = "kimi-permission-visibility",
        env = "BUZZ_ACP_KIMI_PERMISSION_VISIBILITY",
        value_enum,
        default_value = "auto"
    )]
    pub permission_visibility: KimiPermissionVisibility,
    /// Deadline for a human Kimi permission decision, capped by the turn budget.
    #[arg(
        long = "kimi-approval-decision-timeout",
        env = "BUZZ_ACP_KIMI_APPROVAL_DECISION_TIMEOUT",
        default_value_t = 120
    )]
    pub approval_decision_timeout: u64,
    /// Kimi retries after the initial attempt (0..16).
    #[arg(
        long = "kimi-recovery-retries",
        env = "BUZZ_ACP_KIMI_RECOVERY_RETRIES",
        default_value_t = 4
    )]
    pub retries: u32,
    /// Base Kimi retry delays in seconds, before independent +/-20% jitter.
    #[arg(
        long = "kimi-recovery-delays",
        env = "BUZZ_ACP_KIMI_RECOVERY_DELAYS",
        value_delimiter = ',',
        default_value = "5,15,45,120"
    )]
    pub delays: Vec<u64>,
    /// Total Kimi work budget in seconds, including attempts and backoff.
    #[arg(
        long = "kimi-recovery-budget",
        env = "BUZZ_ACP_KIMI_RECOVERY_BUDGET",
        default_value_t = 1800
    )]
    pub budget: u64,
    /// Kimi progress deadline after a written permission decision, in seconds.
    #[arg(
        long = "kimi-approval-progress-timeout",
        env = "BUZZ_ACP_KIMI_APPROVAL_PROGRESS_TIMEOUT",
        default_value_t = 120
    )]
    pub approval_progress_timeout: u64,
}

impl KimiRecoveryArgs {
    /// Validate operator input before constructing any work deadlines.
    pub fn policy(&self) -> Result<RecoveryPolicy, ConfigError> {
        let policy = RecoveryPolicy {
            max_retries: self.retries,
            permission_visibility: self.permission_visibility,
            delays: self
                .delays
                .iter()
                .copied()
                .map(Duration::from_secs)
                .collect(),
            total_budget: Duration::from_secs(self.budget),
            approval_progress_timeout: Duration::from_secs(self.approval_progress_timeout),
            approval_decision_timeout: Duration::from_secs(self.approval_decision_timeout),
        };
        policy
            .validate()
            .map_err(|message| ConfigError::ConfigFile(message.into()))?;
        Ok(policy)
    }
}
