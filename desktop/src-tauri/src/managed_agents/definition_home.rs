//! Definition home classification and projected command capabilities.
use super::{AgentDefinition, ManagedAgentRecord};
use crate::device_identity::{DeviceIdentity, HostProof};
use serde::Serialize;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum HomeKind {
    Local,
    Remote,
    Unclaimed,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DefinitionHome {
    pub kind: HomeKind,
    pub label: Option<String>,
    pub remote_instance_pubkeys: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DefinitionCapabilities {
    pub can_create_instance: bool,
    pub can_delete_definition: bool,
    pub blocked_reason: Option<String>,
}
#[derive(Debug, Clone)]
pub(crate) struct RemoteInstanceEvidence {
    pub pubkey: String,
    pub persona_id: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EvidenceReadiness {
    Pending,
    Ready,
    Failed,
}
/// Classify only canonical definition links; a verified local binding wins.
pub(crate) fn classify_definition_home(
    definition: &AgentDefinition,
    instances: &[ManagedAgentRecord],
    device: &DeviceIdentity,
    proof: &HostProof,
    evidence: &[RemoteInstanceEvidence],
) -> DefinitionHome {
    let linked: Vec<_> = instances
        .iter()
        .filter(|i| i.persona_id.as_deref() == Some(definition.id.as_str()) && !i.pubkey.is_empty())
        .collect();
    let proven = linked.iter().any(|i| {
        i.device_host_binding
            .as_deref()
            .is_some_and(|b| proof.matches(b))
    });
    let mut pubkeys: Vec<_> = evidence
        .iter()
        .filter(|e| e.persona_id == definition.id && !e.pubkey.is_empty())
        .filter(|e| {
            !linked.iter().any(|i| {
                i.pubkey == e.pubkey
                    && i.device_host_binding
                        .as_deref()
                        .is_some_and(|b| proof.matches(b))
            })
        })
        .map(|e| e.pubkey.clone())
        .collect();
    pubkeys.sort();
    pubkeys.dedup();
    let foreign_binding = linked.iter().any(|i| {
        i.device_host_binding
            .as_deref()
            .is_some_and(|b| !proof.matches(b))
    });
    let active_origin = definition.origin_released != Some(true);
    let kind = if proven {
        HomeKind::Local
    } else if foreign_binding
        || !pubkeys.is_empty()
        || (active_origin
            && definition
                .origin_device_id
                .as_deref()
                .is_some_and(|id| id != device.device_id))
    {
        HomeKind::Remote
    } else if active_origin
        && definition.origin_device_id.as_deref() == Some(device.device_id.as_str())
    {
        HomeKind::Local
    } else {
        HomeKind::Unclaimed
    };
    let label = match kind {
        HomeKind::Local => Some(device.label.clone()),
        HomeKind::Remote => definition.origin_device_label.clone(),
        HomeKind::Unclaimed => None,
    };
    DefinitionHome {
        kind,
        label,
        remote_instance_pubkeys: pubkeys,
    }
}
/// Project permission, withholding absence-based actions until exhaustive sync.
pub(crate) fn definition_capabilities(
    definition: &AgentDefinition,
    home: &DefinitionHome,
    readiness: EvidenceReadiness,
    locally_proven: bool,
) -> DefinitionCapabilities {
    let blocked_reason = if definition.share_across_devices == Some(true) || locally_proven {
        None
    } else if home.kind == HomeKind::Remote {
        Some("definition_hosted_elsewhere".into())
    } else {
        match readiness {
            EvidenceReadiness::Ready => None,
            EvidenceReadiness::Pending => Some("device_home_sync_pending".into()),
            EvidenceReadiness::Failed => Some("device_home_sync_failed".into()),
        }
    };
    let allowed = blocked_reason.is_none();
    DefinitionCapabilities {
        can_create_instance: allowed,
        can_delete_definition: allowed,
        blocked_reason,
    }
}
#[cfg(test)]
mod tests;
