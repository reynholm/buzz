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

/// Delivery protocol profile, independent of provider names and product versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReplyProfile {
    #[serde(rename = "runtime_final_v1", alias = "b24_0_26_7_8")]
    RuntimeFinalV1,
}

/// Persisted instruction identity, separate from session-store ownership CAS.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReplyContract {
    pub mode: ReplyDelivery,
    pub profile: ReplyProfile,
    pub revision: u32,
}

impl ReplyContract {
    /// Select the protocol; each new worker must negotiate its support over ACP.
    pub fn runtime_final() -> Self {
        Self {
            mode: ReplyDelivery::RuntimeFinal,
            profile: ReplyProfile::RuntimeFinalV1,
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
    /// Optional protocol profile; the legacy B24 spelling remains an alias.
    #[arg(long, env = "BUZZ_ACP_REPLY_PROFILE")]
    pub reply_profile: Option<String>,
    /// Legacy option accepted for compatibility; VERSION files do not select support.
    #[arg(long, env = "BUZZ_ACP_REPLY_BUILD_FILE")]
    pub reply_build_file: Option<std::path::PathBuf>,
}

impl ReplyDeliveryArgs {
    pub fn contract(&self) -> Result<Option<ReplyContract>, ConfigError> {
        match (self.reply_delivery, self.reply_profile.as_deref()) {
            (ReplyDelivery::AgentTools, None) => Ok(None),
            (ReplyDelivery::RuntimeFinal, None | Some("runtime_final_v1" | "b24_0_26_7_8")) => {
                Ok(Some(ReplyContract::runtime_final()))
            }
            _ => Err(ConfigError::ConfigFile("runtime_final accepts only the runtime_final_v1 protocol profile; adapter support must be negotiated over ACP".into())),
        }
    }
}
