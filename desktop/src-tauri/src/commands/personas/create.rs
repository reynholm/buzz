//! The persona creation command surface, split from `mod.rs` (file-size cap)
//! as the sibling of [`super::update`].

use tauri::AppHandle;
use uuid::Uuid;

use crate::{
    app_state::AppState,
    managed_agents::{
        apply_persona_behavior, load_personas, save_personas, try_regenerate_nest,
        validate_agent_definition_text, AgentDefinition, CatalogSource, CreatePersonaRequest,
    },
    util::now_iso,
};

use super::{normalize_description, pending, retain_persona_pending, trim_optional, trim_required};

#[tauri::command]
pub async fn create_persona(
    input: CreatePersonaRequest,
    app: AppHandle,
) -> Result<AgentDefinition, String> {
    use tauri::Manager;
    tokio::task::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let device = crate::managed_agents::device_creation::local_device(&app)?;
        let persona = definition_from_request(input, &device, &now_iso())?;
        let _store_guard = state
            .managed_agents_store_lock
            .lock()
            .map_err(|error| error.to_string())?;
        let mut personas = load_personas(&app)?;
        pending::project_active_persona_sharing(&app, &state, &mut personas);
        personas.push(persona.clone());
        save_personas(&app, &personas)?;
        retain_persona_pending(&app, &state, &persona);
        try_regenerate_nest(&app);
        Ok(persona)
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

/// Local dialog/draft/catalog constructor. Imported catalog policy is never inherited.
pub(crate) fn definition_from_request(
    input: CreatePersonaRequest,
    device: &crate::device_identity::DeviceIdentity,
    now: &str,
) -> Result<AgentDefinition, String> {
    let display_name = trim_required(&input.display_name, "Display name")?;
    // System prompt optional: core memory is auto-injected. Empty is valid.
    // Preserve it byte-for-byte: shared/import review surfaces show this
    // exact string before the ACP harness executes it.
    let system_prompt = input.system_prompt.clone();
    validate_agent_definition_text(&display_name, &system_prompt)?;
    let description = normalize_description(input.description)?;
    let avatar_url = trim_optional(input.avatar_url);
    let acp_command = trim_optional(input.acp_command);
    let runtime = trim_optional(input.runtime);
    let model = trim_optional(input.model);
    let provider = trim_optional(input.provider);
    // Normalized before the store is touched: a coordinate that can't match
    // a publication is worse than no coordinate, because it silently
    // re-enables the duplicate add it exists to prevent.
    let catalog_source = input
        .catalog_source
        .map(CatalogSource::normalized)
        .transpose()?;
    let name_pool: Vec<String> = input
        .name_pool
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    crate::managed_agents::validate_user_env_keys(&input.env_vars)?;
    let mut persona = AgentDefinition {
        share_across_devices: None,
        origin_device_id: None,
        origin_device_label: None,
        origin_released: None,
        id: Uuid::new_v4().to_string(),
        display_name,
        avatar_url,
        description,
        system_prompt,
        acp_command,
        runtime,
        model,
        provider,
        name_pool,
        is_builtin: false,
        is_active: true,
        shared: false,
        source_team: None,
        source_team_persona_slug: None,
        catalog_source,
        // Team-publication provenance is set only by
        // `add_team_from_catalog`, never by an ordinary create.
        team_catalog_source: None,
        env_vars: input.env_vars,
        respond_to: None,
        respond_to_allowlist: Vec::new(),
        parallelism: None,
        session_policy: crate::managed_agents::AcpSessionPolicy::Channel,
        created_at: now.to_string(),
        updated_at: now.to_string(),
    };
    let requested_share = if persona.catalog_source.is_none() {
        input.share_across_devices
    } else {
        None
    };
    crate::managed_agents::device_creation::stamp_new_definition(
        &mut persona,
        requested_share,
        device,
    );
    apply_persona_behavior(&mut persona, input.behavior)?;
    Ok(persona)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dialog_draft_and_catalog_default_private_with_own_origin() {
        let device = crate::managed_agents::device_home_migration::tests::context(
            crate::managed_agents::definition_home::EvidenceReadiness::Ready,
        )
        .device;
        for shared in [None, Some(false), Some(true)] {
            let mut input = serde_json::json!({"displayName":"One","systemPrompt":"Test"});
            if let Some(value) = shared {
                input["shareAcrossDevices"] = serde_json::json!(value);
            }
            let request: CreatePersonaRequest = serde_json::from_value(input).unwrap();
            let d = definition_from_request(request, &device, "now").unwrap();
            assert_eq!(d.share_across_devices, Some(shared.unwrap_or(false)));
            assert_eq!(
                d.origin_device_id.as_deref(),
                Some(device.device_id.as_str())
            );
            assert_eq!(d.origin_released, Some(false));
        }
        let request:CreatePersonaRequest=serde_json::from_value(serde_json::json!({"displayName":"Imported","systemPrompt":"Test","shareAcrossDevices":true,"catalogSource":{"ownerPubkey":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","personaId":"one"}})).unwrap();
        let d = definition_from_request(request, &device, "now").unwrap();
        assert_eq!(d.share_across_devices, Some(false));
    }
}
