//! Explicit delivery opt-in; provider and backend names never select a profile.
use super::ConfigError;

/// Who publishes the final user-facing answer.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    clap::ValueEnum,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ReplyDelivery {
    #[default]
    #[value(name = "agent_tools")]
    AgentTools,
    #[value(name = "runtime_final")]
    RuntimeFinal,
}

/// Verified installation profile; persisted independently of provider model names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReplyProfile {
    #[serde(rename = "b24_0_26_7_8")]
    VerifiedB24,
}

/// Persisted instruction identity, separate from session-store ownership CAS.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReplyContract {
    pub mode: ReplyDelivery,
    pub profile: ReplyProfile,
    pub revision: u32,
}

impl ReplyContract {
    /// The only currently verified runtime profile. Selecting it attests the
    /// operator checked the installed wrapper's fork build, absent from ACP init.
    pub fn b24() -> Self {
        Self {
            mode: ReplyDelivery::RuntimeFinal,
            profile: ReplyProfile::VerifiedB24,
            revision: 1,
        }
    }

    /// Refresh this contract in every ordinary/continuation prompt.
    pub fn instruction(&self) -> &'static str {
        "<reply-delivery revision=\"1\">Buzz publishes your final user-facing prose in the original conversation after a successful turn. Return the final answer as ordinary assistant text. Use messaging tools only for separate progress or attachments. Do not manually publish or duplicate your final answer. A clarification question is a valid answer; delivering it does not complete the user's task.</reply-delivery>"
    }
}

#[derive(Debug, Clone, clap::Args)]
pub struct ReplyDeliveryArgs {
    /// Final publication mode (runtime delivery is explicitly opt-in).
    #[arg(
        long,
        env = "BUZZ_ACP_REPLY_DELIVERY",
        value_enum,
        default_value = "agent_tools"
    )]
    pub reply_delivery: ReplyDelivery,
    /// Verified installed integration profile, required for runtime_final.
    #[arg(long, env = "BUZZ_ACP_REPLY_PROFILE")]
    pub reply_profile: Option<String>,
    /// Active installation VERSION file, checked before new provider execution.
    #[arg(long, env = "BUZZ_ACP_REPLY_BUILD_FILE")]
    pub reply_build_file: Option<std::path::PathBuf>,
}

impl ReplyDeliveryArgs {
    pub fn contract(&self, command: &str) -> Result<Option<ReplyContract>, ConfigError> {
        match (self.reply_delivery, self.reply_profile.as_deref()) {
            (ReplyDelivery::AgentTools, None) => Ok(None),
            (ReplyDelivery::RuntimeFinal, Some("b24_0_26_7_8"))
                if self.reply_build_file.as_ref().is_some_and(|p| p.is_absolute()) && std::path::Path::new(command).file_name().and_then(|v| v.to_str()) == Some("b24-cowork-code-cli") => Ok(Some(ReplyContract::b24())),
            _ => Err(ConfigError::ConfigFile("runtime_final requires --reply-profile b24_0_26_7_8, an absolute --reply-build-file for the active VERSION file, and the verified b24-cowork-code-cli wrapper; Kimi and unverified launches must use agent_tools".into())),
        }
    }
}

/// Read a small regular VERSION file from the configured active installation.
/// New execution fails closed on upgrades; journal-only replay does not call it.
pub(crate) fn verify_build_file(path: Option<&std::path::Path>) -> Result<(), &'static str> {
    use std::io::Read;
    let path = path.ok_or("runtime_build_provenance_missing")?;
    let metadata = std::fs::metadata(path).map_err(|_| "runtime_build_provenance_unavailable")?;
    if !metadata.is_file() || metadata.len() > 64 {
        return Err("runtime_build_provenance_invalid");
    }
    let mut version = String::new();
    std::fs::File::open(path)
        .map_err(|_| "runtime_build_provenance_unavailable")?
        .take(65)
        .read_to_string(&mut version)
        .map_err(|_| "runtime_build_provenance_invalid")?;
    if version.len() > 64 || version.trim() != "0.26.7-b24.8" {
        return Err("runtime_build_not_verified");
    }
    Ok(())
}
