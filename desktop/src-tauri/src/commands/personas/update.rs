//! The persona edit command surface: `update_persona` (best-effort enqueue)
//! and the `update_persona_with` seam that `update_persona_and_publish` reuses
//! to await relay acceptance for the same save.

use tauri::AppHandle;

use crate::{
    app_state::AppState,
    managed_agents::{
        apply_persona_behavior, effective_agent_command, load_personas, managed_agent_avatar_url,
        save_personas, try_regenerate_nest, validate_agent_definition_text, AgentDefinition,
        ManagedAgentRecord, UpdatePersonaRequest,
    },
    util::now_iso,
};

use super::{normalize_description, retain_persona_pending, trim_optional, trim_required};

#[cfg(test)]
mod device_edit_tests;
#[cfg(test)]
mod name_propagation_tests;

/// Return value of the `update_persona` command. Uses flatten so all
/// `AgentDefinition` fields appear at the top level of the JSON response —
/// backward-compatible with callers that already destructure a raw persona object.
#[derive(Debug, serde::Serialize)]
pub struct UpdatePersonaResult {
    #[serde(flatten)]
    persona: AgentDefinition,
}

/// Propagate a persona definition's display_name rename to linked agent instances.
/// Only instances whose current `name` equals `old_display_name` are updated;
/// pool-named instances (e.g. "Birch", "Compass") keep their individualised name.
/// Updates both `record.name` (relay display name) and `record.display_name`.
/// Returns the pubkeys of the records that were renamed.
fn propagate_persona_name_rename(
    records: &mut [ManagedAgentRecord],
    persona_id: &str,
    old_display_name: &str,
    new_display_name: &str,
) -> Vec<String> {
    let mut renamed = Vec::new();
    for record in records.iter_mut() {
        if record.persona_id.as_deref() != Some(persona_id) {
            continue;
        }
        if record.name != old_display_name {
            continue; // pool-named instance — keep its individualised name
        }
        record.name = new_display_name.to_string();
        record.display_name = Some(new_display_name.to_string());
        renamed.push(record.pubkey.clone());
    }
    renamed
}

#[derive(Debug, PartialEq, Eq)]
struct LinkedProfileUpdate {
    /// Whether this update changed bytes in the managed-agent record.
    record_changed: bool,
    /// Whether this instance needs a complete kind:0 replacement event.
    profile_sync_required: bool,
    /// Avatar to publish with the complete kind:0 replacement event.
    profile_avatar: Option<String>,
}

/// Apply the persisted portion of a persona identity edit to one linked
/// instance and resolve the avatar for the complete kind:0 replacement.
///
/// Description-only edits deliberately leave the record unchanged, but still
/// need a non-empty avatar projection for legacy records whose `avatar_url`
/// has not yet been backfilled. The persona avatar is authoritative there;
/// the effective command icon is the final fallback.
fn prepare_linked_profile_update(
    record: &mut ManagedAgentRecord,
    persona: &AgentDefinition,
    renamed: bool,
    avatar_changed: bool,
    about_changed: bool,
) -> LinkedProfileUpdate {
    let mut record_changed = renamed;
    if avatar_changed {
        let effective_cmd = effective_agent_command(
            record.persona_id.as_deref(),
            std::slice::from_ref(persona),
            record.agent_command_override.as_deref(),
        );
        record.avatar_url = persona
            .avatar_url
            .clone()
            .or_else(|| managed_agent_avatar_url(&effective_cmd));
        record_changed = true;
    }

    let effective_cmd = effective_agent_command(
        record.persona_id.as_deref(),
        std::slice::from_ref(persona),
        record.agent_command_override.as_deref(),
    );
    let profile_avatar = record
        .avatar_url
        .clone()
        .or_else(|| persona.avatar_url.clone())
        .or_else(|| managed_agent_avatar_url(&effective_cmd));

    LinkedProfileUpdate {
        record_changed,
        profile_sync_required: record_changed || about_changed,
        profile_avatar,
    }
}

/// Profile sync params collected under the store lock for async relay publish:
/// (agent keys, relay url, display name, avatar url, kind:0 about, auth tag).
type ProfileSyncParams = Vec<(
    nostr::Keys,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    crate::managed_agents::device_runtime::RuntimeFence,
)>;

#[tauri::command]
pub async fn update_persona(
    input: UpdatePersonaRequest,
    app: AppHandle,
) -> Result<UpdatePersonaResult, String> {
    let (persona, ()) = update_persona_with(input, app, |app, state, persona| {
        retain_persona_pending(app, state, persona);
        // F2: immediately refresh any shared 30178 heads that include this
        // persona as a member. Best-effort inside retain so a hiccup cannot
        // fail the persona edit itself.
        crate::commands::refresh_team_catalog_heads_for_persona(app, state, &persona.id);
        Ok(())
    })
    .await?;
    Ok(UpdatePersonaResult { persona })
}

/// Save an edited persona, hand the saved record to `retain` while the store
/// lock is still held, then sync the relay profiles of linked agent instances.
///
/// `retain` is the only difference between the two update commands:
/// [`update_persona`] enqueues best-effort, while
/// [`sharing::update_persona_and_publish`] prepares a strict publication and
/// returns the event so the caller can await relay acceptance.
pub(super) async fn update_persona_with<R: Send + 'static>(
    input: UpdatePersonaRequest,
    app: AppHandle,
    retain: impl FnOnce(&AppHandle, &AppState, &AgentDefinition) -> Result<R, String> + Send + 'static,
) -> Result<(AgentDefinition, R), String> {
    use tauri::Manager;

    // Phase 1: synchronous save (persona record + linked agent avatar updates)
    let (result, retained, profile_sync_params) = tokio::task::spawn_blocking({
        let app = app.clone();
        move || -> Result<(AgentDefinition, R, ProfileSyncParams), String> {
            let state = app.state::<AppState>();
            update_persona_locked_with(
                input,
                &app,
                &state,
                retain,
                crate::managed_agents::persona_device_view::load_device_policy_context,
                crate::managed_agents::storage::resolve_agent_key_readonly,
                crate::commands::agents::retain_managed_agent_pending,
                || try_regenerate_nest(&app),
            )
        }
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))??;

    // Phase 2: await relay profile sync for linked agents whose avatar,
    // display_name, or effective description (kind:0 about) was just
    // updated. We await (rather than fire-and-forget)
    // so the frontend cache invalidation that follows the mutation settlement
    // sees the fresh relay profile. Best-effort — failures are logged, not surfaced.
    if !profile_sync_params.is_empty() {
        let state = app.state::<AppState>();
        for (agent_keys, relay_url, display_name, avatar_url, about, auth_tag, pubkey, fence) in
            profile_sync_params
        {
            if let Some(e) = crate::commands::agents::publish_agent_profile_with_about(
                &app,
                &state,
                &pubkey,
                &fence,
                &relay_url,
                &agent_keys,
                &display_name,
                avatar_url.as_deref(),
                about.as_deref(),
                auth_tag.as_deref(),
            )
            .await
            {
                eprintln!("buzz-desktop: relay profile sync failed after persona update: {e}");
            }
        }
    }

    Ok((result, retained))
}

/// Definition edit's native transaction. Instance propagation is separately authorized.
#[allow(clippy::too_many_arguments)]
fn update_persona_locked_with<R: tauri::Runtime, T>(
    input: UpdatePersonaRequest,
    app: &AppHandle<R>,
    state: &AppState,
    retain: impl FnOnce(&AppHandle<R>, &AppState, &AgentDefinition) -> Result<T, String>,
    context: impl FnOnce(
        &AppHandle<R>,
        &AppState,
    ) -> Result<
        crate::managed_agents::persona_device_view::DevicePolicyContext,
        String,
    >,
    resolve_keys: impl Fn(&ManagedAgentRecord) -> Result<Option<nostr::Keys>, String>,
    retain_instance: impl Fn(&AppHandle<R>, &AppState, &ManagedAgentRecord),
    refresh: impl FnOnce(),
) -> Result<(AgentDefinition, T, ProfileSyncParams), String> {
    let display_name = trim_required(&input.display_name, "Display name")?;
    let system_prompt = input.system_prompt.clone();
    validate_agent_definition_text(&display_name, &system_prompt)?;
    let description = normalize_description(input.description)?;
    let avatar_url = trim_optional(input.avatar_url);
    let acp_command = trim_optional(input.acp_command);
    let runtime = trim_optional(input.runtime);
    let model = trim_optional(input.model);
    let provider = trim_optional(input.provider);

    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;
    let fence = crate::managed_agents::device_runtime::capture_runtime_fence(state)?;
    let mut personas = load_personas(app)?;
    super::pending::project_active_persona_sharing(app, state, &mut personas);

    let persona = personas
        .iter_mut()
        .find(|record| record.id == input.id)
        .ok_or_else(|| format!("agent {} not found", input.id))?;

    // Track what changed so we can propagate to linked agent records.
    let avatar_changed = persona.avatar_url != avatar_url;
    let name_changed = persona.display_name != display_name;
    let old_display_name = persona.display_name.clone();
    // The kind:0 `about` is the authored description, so a
    // description edit changes what should be published.
    let old_about =
        crate::managed_agents::effective_agent_description(persona.description.as_deref());
    let new_about = crate::managed_agents::effective_agent_description(description.as_deref());
    let about_changed = old_about != new_about;

    persona.display_name = display_name;
    persona.avatar_url = avatar_url;
    persona.description = description;
    persona.system_prompt = system_prompt;
    persona.acp_command = acp_command;
    persona.runtime = runtime;
    persona.model = model;
    persona.provider = provider;
    persona.name_pool = input
        .name_pool
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if let Some(env_vars) = input.env_vars {
        crate::managed_agents::validate_user_env_keys(&env_vars)?;
        persona.env_vars = env_vars;
    }
    apply_persona_behavior(persona, input.behavior)?;
    persona.updated_at = now_iso();

    let result = persona.clone();
    save_personas(app, &personas)?;

    let retained = retain(app, state, &result)?;
    refresh();

    // If the avatar, display_name, or effective description changed,
    // propagate to linked agent records and collect relay profile sync
    // params for the async phase. An about-only change touches no
    // record bytes but still republishes each linked kind:0 profile.
    let sync_params: ProfileSyncParams = if avatar_changed || name_changed || about_changed {
        let mut records = crate::managed_agents::persona_device_view::read_policy_records(
            &crate::managed_agents::managed_agents_store_path(app)?,
        )?;
        // Only authorized targets reach identity propagation or private-key resolution.
        let c = if result.share_across_devices == Some(true) {
            None
        } else {
            context(app, state).ok().filter(|c| c.scope == fence.scope)
        };
        if crate::managed_agents::device_runtime::assert_runtime_fence(state, &fence).is_err() {
            return Ok((result, retained, Vec::new()));
        }
        let authorized: std::collections::HashSet<_> = records
            .iter()
            .filter(|record| {
                record.persona_id.as_deref() == Some(result.id.as_str())
                    && (result.share_across_devices == Some(true)
                        || c.as_ref().is_some_and(|context| {
                            crate::managed_agents::device_authority::authorize_instance_authority(
                                record,
                                Some(&result),
                                context,
                                crate::managed_agents::device_authority::InstanceAuthorityAction::Update,
                            ).is_ok()
                        }))
            })
            .map(|record| record.pubkey.clone())
            .collect();
        records.retain(|r| authorized.contains(&r.pubkey));
        let mut params: ProfileSyncParams = Vec::new();
        let mut agents_modified = false;
        let workspace_relay = fence.scope.relay_url.clone();

        // Propagate the display_name rename to instances that still
        // carry the old definition display_name (pool-named instances
        // keep their individualised name) in one pass; the loop below
        // only decides which records need a relay profile sync.
        let renamed: Vec<String> = if name_changed {
            propagate_persona_name_rename(
                &mut records,
                &result.id,
                &old_display_name,
                &result.display_name,
            )
        } else {
            Vec::new()
        };

        for record in records.iter_mut() {
            if record.persona_id.as_deref() != Some(&result.id) {
                continue;
            }
            let was_renamed = renamed.contains(&record.pubkey);
            let update = prepare_linked_profile_update(
                record,
                &result,
                was_renamed,
                avatar_changed,
                about_changed,
            );

            agents_modified = agents_modified || update.record_changed;
            if update.profile_sync_required {
                if let Ok(Some(agent_keys)) = resolve_keys(record) {
                    let relay_url = crate::relay::effective_agent_relay_url(
                        &record.relay_url,
                        &workspace_relay,
                    );
                    params.push((
                        agent_keys,
                        relay_url,
                        record.name.clone(),
                        update.profile_avatar,
                        new_about.clone(),
                        record.auth_tag.clone(),
                        record.pubkey.clone(),
                        fence.clone(),
                    ));
                }
            }
        }

        if agents_modified {
            crate::managed_agents::storage::save_restore_records_with(
                app,
                &records,
                &std::collections::HashSet::new(),
                |_| {},
            )?;
            // Keep retained kind:30177 identity records in lockstep with
            // the rename (#2423): `record.name` is part of the published
            // identity projection, so skipping this strands the relay on
            // the stale name→pubkey binding until the next boot reconcile.
            // Avatar-only edits are excluded — the avatar is not in the
            // projection, so retaining would be a guaranteed no-op.
            for record in records.iter().filter(|r| renamed.contains(&r.pubkey)) {
                retain_instance(app, state, record);
            }
        }

        params
    } else {
        Vec::new()
    };

    Ok((result, retained, sync_params))
}
