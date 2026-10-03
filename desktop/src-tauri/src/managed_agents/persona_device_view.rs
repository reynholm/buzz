//! Read-only owner/relay policy context and flattened definition projection.
use super::{
    agent_events::managed_agent_content_from_event,
    definition_home::*,
    device_home_sync,
    retention::{get_retained_events_by_kind, scoped_retention_db_path},
    AgentDefinition, ManagedAgentRecord,
};
use crate::{
    app_state::AppState,
    device_identity::{
        load_existing_device_identity, load_existing_host_proof, DeviceIdentity, HostProof,
    },
    secret_store::SecretStore,
};
use nostr::JsonUtil;
use serde::Serialize;
use std::path::Path;
use tauri::{AppHandle, Manager};

/// Compatible definition JSON with computed home and action permissions.
#[derive(Serialize)]
pub(crate) struct PersonaDeviceView {
    #[serde(flatten)]
    pub definition: AgentDefinition,
    pub home: Option<DefinitionHome>,
    #[serde(rename = "homeError", skip_serializing_if = "Option::is_none")]
    pub home_error: Option<String>,
    pub capabilities: DefinitionCapabilities,
}
/// Captured evidence for one owner, relay and workspace generation.
pub(crate) struct DevicePolicyContext {
    pub scope: device_home_sync::SyncScope,
    pub device: DeviceIdentity,
    pub proof: HostProof,
    pub evidence: Vec<RemoteInstanceEvidence>,
    pub readiness: EvidenceReadiness,
}
impl DevicePolicyContext {
    /// Compute without saving definitions, claiming homes or minting keys.
    pub(crate) fn project(
        &self,
        definition: AgentDefinition,
        instances: &[ManagedAgentRecord],
    ) -> PersonaDeviceView {
        let home = classify_definition_home(
            &definition,
            instances,
            &self.device,
            &self.proof,
            &self.evidence,
        );
        let proven = instances.iter().any(|i| {
            i.persona_id.as_deref() == Some(definition.id.as_str())
                && !i.pubkey.is_empty()
                && i.device_host_binding
                    .as_deref()
                    .is_some_and(|b| self.proof.matches(b))
        });
        let capabilities = definition_capabilities(&definition, &home, self.readiness, proven);
        PersonaDeviceView {
            definition,
            home: Some(home),
            home_error: None,
            capabilities,
        }
    }
}

impl PersonaDeviceView {
    /// Keep definitions visible without inventing an unclaimed home on read failure.
    pub(crate) fn unavailable(definition: AgentDefinition, error: String) -> Self {
        let allowed = definition.share_across_devices == Some(true);
        Self {
            definition,
            home: None,
            home_error: Some(error),
            capabilities: DefinitionCapabilities {
                can_create_instance: allowed,
                can_delete_definition: allowed,
                blocked_reason: (!allowed).then(|| "device_home_sync_failed".into()),
            },
        }
    }
}

/// Read structural policy fields without hydrating private keys or repairing storage.
pub(crate) fn read_policy_records(path: &Path) -> Result<Vec<ManagedAgentRecord>, String> {
    match std::fs::read(path) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_err(|e| format!("device policy agent store: {e}"))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(format!("device policy agent store read: {e}")),
    }
}
/// Load only existing device authority and active retained owner heads.
/// Caller holds the managed store lock so context and projection agree.
pub(crate) fn load_device_policy_context<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
) -> Result<DevicePolicyContext, String> {
    let scope = device_home_sync::capture_scope(state)?;
    let readiness = device_home_sync::readiness_locked(state, &scope)?;
    load_device_policy_context_at(app, scope, readiness)
}
/// Load authority inside the finish barrier without re-locking its sync mutex.
pub(crate) fn load_device_policy_context_at<R: tauri::Runtime>(
    app: &AppHandle<R>,
    scope: device_home_sync::SyncScope,
    readiness: EvidenceReadiness,
) -> Result<DevicePolicyContext, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("device data directory: {e}"))?;
    let device = load_existing_device_identity(&directory.join("device.json"))?;
    let proof = load_existing_host_proof(&SecretStore::keyring(
        crate::build_identity::device_host_service(),
    ))?;
    let path = scoped_retention_db_path(
        &directory.join("agents"),
        &scope.relay_url,
        &scope.owner_pubkey,
    );
    let evidence = read_remote_evidence(&path, &scope.owner_pubkey)?;
    Ok(DevicePolicyContext {
        scope,
        device,
        proof,
        evidence,
        readiness,
    })
}
/// Read retained heads without creating a database or modifying rows.
pub(crate) fn read_remote_evidence(
    path: &Path,
    owner: &str,
) -> Result<Vec<RemoteInstanceEvidence>, String> {
    let conn =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| format!("device home evidence read: {e}"))?;
    // Retention atomically purges only the routed covered head when committing
    // a deletion. Its remaining heads are authoritative, including recreations
    // newer than a tombstone. Do not reinterpret additional raw a-tags here.
    let mut evidence = Vec::new();
    for row in get_retained_events_by_kind(&conn, 30177, owner)? {
        let event = nostr::Event::from_json(&row.raw_event)
            .map_err(|e| format!("device home head: {e}"))?;
        event
            .verify()
            .map_err(|e| format!("device home head signature: {e}"))?;
        if event.pubkey.to_hex() != owner || event.kind.as_u16() != 30177 {
            continue;
        }
        let d = event
            .tags
            .iter()
            .find_map(|t| {
                let t = t.as_slice();
                if t.first().map(String::as_str) == Some("d") {
                    t.get(1).map(String::as_str)
                } else {
                    None
                }
            })
            .ok_or_else(|| "device home head missing coordinate".to_string())?;
        if d != row.d_tag
            || event.content != row.content
            || event.created_at.as_secs() as i64 != row.created_at
        {
            return Err("device home retained head does not match signed event".into());
        }
        nostr::PublicKey::from_hex(d).map_err(|e| format!("device home instance pubkey: {e}"))?;
        let content = managed_agent_content_from_event(&event)?;
        if let Some(persona_id) = content
            .persona_id
            .filter(|id| !id.is_empty() && id.trim() == id)
        {
            evidence.push(RemoteInstanceEvidence {
                pubkey: d.to_string(),
                persona_id,
            });
        }
    }
    Ok(evidence)
}
