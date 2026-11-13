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

use super::{normalize_description, pending, trim_optional, trim_required};

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
/// (agent keys, relay url, display name, avatar url, kind:0 about, auth tag,
/// exact instance pubkey, original device runtime fence).
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

pub(crate) type PersonaAvatarTransfers = Vec<(nostr::Keys, String, Option<String>, Option<String>)>;

/// Only settings used by the target avatar plan participate in its fence.
/// Runtime churn and unrelated definitions cannot invalidate the transfer.
fn avatar_settings_snapshot(
    persona: &AgentDefinition,
    records: &[ManagedAgentRecord],
) -> Result<(serde_json::Value, serde_json::Value), String> {
    let mut linked: Vec<_> = records
        .iter()
        .filter(|r| r.persona_id.as_deref() == Some(&persona.id))
        .map(|r| {
            serde_json::json!({
                "pubkey":r.pubkey, "private_key_nsec":r.private_key_nsec,
                "relay_url":r.relay_url, "auth_tag":r.auth_tag, "avatar_url":r.avatar_url,
                "agent_command_override":r.agent_command_override, "device_host_binding":r.device_host_binding,
            })
        })
        .collect();
    linked.sort_by(|a, b| a["pubkey"].as_str().cmp(&b["pubkey"].as_str()));
    Ok((
        serde_json::to_value(persona).map_err(|e| e.to_string())?,
        serde_json::Value::Array(linked),
    ))
}

/// Select exact authorized targets before looking up any private key. Copied
/// foreign instances remain outside both transfer planning and settings writes.
fn avatar_targets_with_keys<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    persona: &AgentDefinition,
    fence: &crate::managed_agents::device_runtime::RuntimeFence,
) -> Result<Vec<ManagedAgentRecord>, String> {
    let records = crate::managed_agents::persona_device_view::read_policy_records(
        &crate::managed_agents::managed_agents_store_path(app)?,
    )?;
    let context = if persona.share_across_devices == Some(true) {
        None
    } else {
        crate::managed_agents::persona_device_view::load_device_policy_context(app, state)
            .ok()
            .filter(|context| context.scope == fence.scope)
    };
    let selected = avatar_targets_with(
        persona,
        &records,
        context.as_ref(),
        crate::managed_agents::storage::resolve_agent_key_readonly,
    )?;
    crate::managed_agents::device_runtime::assert_runtime_fence(state, fence)?;
    Ok(selected)
}

fn avatar_targets_with(
    persona: &AgentDefinition,
    records: &[ManagedAgentRecord],
    context: Option<&crate::managed_agents::persona_device_view::DevicePolicyContext>,
    resolve: impl Fn(&ManagedAgentRecord) -> Result<Option<nostr::Keys>, String>,
) -> Result<Vec<ManagedAgentRecord>, String> {
    use nostr::ToBech32;
    let mut selected: Vec<_> = records
        .iter()
        .filter(|record| {
            if record.pubkey.is_empty() || record.persona_id.as_deref() != Some(persona.id.as_str())
            {
                return false;
            }
            if persona.share_across_devices == Some(true) {
                return true;
            }
            let Some(context) = context else {
                return false;
            };
            crate::managed_agents::device_authority::authorize_instance_authority(
                record,
                Some(persona),
                context,
                crate::managed_agents::device_authority::InstanceAuthorityAction::Update,
            )
            .is_ok()
        })
        .cloned()
        .collect();
    for record in &mut selected {
        let keys = resolve(record)?
            .ok_or_else(|| format!("Cannot change the avatar: signing keys are unavailable for {} ({}). Restore their keys, or remove or unlink those instances, then retry. Changes were not saved.", record.name, record.pubkey))?;
        record.private_key_nsec = keys.secret_key().to_bech32().map_err(|e| e.to_string())?;
    }
    Ok(selected)
}

/// Build the complete plan before any transfer or local write. A missing signer
/// must leave the old desired avatar in place so key recovery can retry it.
fn plan_persona_avatar_transfers(
    persona: &AgentDefinition,
    records: &[ManagedAgentRecord],
    workspace_relay: &str,
) -> Result<PersonaAvatarTransfers, String> {
    let linked: Vec<_> = records
        .iter()
        .filter(|r| r.persona_id.as_deref() == Some(&persona.id))
        .collect();
    let unavailable: Vec<_> = linked
        .iter()
        .filter(|r| nostr::Keys::parse(&r.private_key_nsec).is_err())
        .map(|r| format!("{} ({})", r.name, r.pubkey))
        .collect();
    if !unavailable.is_empty() {
        return Err(format!("Cannot change the avatar: signing keys are unavailable for {}. Restore their keys, or remove or unlink those instances, then retry. Changes were not saved.", unavailable.join(", ")));
    }
    linked
        .into_iter()
        .map(|record| {
            let keys = nostr::Keys::parse(&record.private_key_nsec).map_err(|e| e.to_string())?;
            let mut updated = record.clone();
            let update = prepare_linked_profile_update(&mut updated, persona, false, true, false);
            Ok((
                keys,
                crate::relay::effective_agent_relay_url(&record.relay_url, workspace_relay),
                update.profile_avatar,
                record.auth_tag.clone(),
            ))
        })
        .collect()
}

/// Transfer explicit persona selections before local writes. Empty plans (an
/// unchanged avatar) do not acquire upload proof with the owner's authority.
pub(crate) async fn transfer_persona_avatars_before_save<R: tauri::Runtime>(
    app: &AppHandle<R>,
    fence: &crate::managed_agents::device_runtime::RuntimeFence,
    state: &AppState,
    owner: &nostr::Keys,
    transfers: &PersonaAvatarTransfers,
) -> Result<(), String> {
    for (keys, relay, avatar, tag) in transfers {
        crate::commands::agents::publish_profile_with(
            app,
            state,
            &keys.public_key().to_hex(),
            fence,
            relay,
            crate::managed_agents::persona_device_view::load_device_policy_context,
            |relay| async move {
                crate::relay::profile_avatar::transfer_created_avatar(
                    state,
                    &relay,
                    owner,
                    keys,
                    avatar.as_deref(),
                    tag.as_deref(),
                )
                .await
            },
        )
        .await
        .map_err(|e| {
            format!("Could not transfer the persona avatar; changes were not saved: {e}")
        })?;
    }
    Ok(())
}

#[tauri::command]
pub async fn update_persona(
    input: UpdatePersonaRequest,
    app: AppHandle,
) -> Result<UpdatePersonaResult, String> {
    let (persona, ()) = update_persona_with(input, app, |app, _state, persona, scope| {
        if let Some(scope) = scope {
            if let Err(e) = pending::retain_persona_pending_at(scope, persona) {
                eprintln!("buzz-desktop: persona-retain: {e}");
            }
            crate::commands::refresh_team_catalog_heads_for_persona_in_scope(
                app,
                scope,
                &persona.id,
            );
        }
        // F2: immediately refresh any shared 30178 heads that include this
        // persona as a member. Best-effort inside retain so a hiccup cannot
        // fail the persona edit itself.
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
pub(crate) async fn update_persona_with<T: tauri::Runtime, R: Send + 'static>(
    input: UpdatePersonaRequest,
    app: AppHandle<T>,
    retain: impl FnOnce(
            &AppHandle<T>,
            &AppState,
            &AgentDefinition,
            Option<&crate::managed_agents::retention::RetentionScope>,
        ) -> Result<R, String>
        + Send
        + 'static,
) -> Result<(AgentDefinition, R), String> {
    use tauri::Manager;

    // Capture explicit owner authority before any await. Transfer changed avatar
    // bytes before saving, so a failed transfer leaves the old local avatar and
    // a normal retry still recognizes the change.
    let state = app.state::<AppState>();
    let (retention_scope, edit_fence) = {
        let _workspace = state.workspace_apply_lock.lock().await;
        let _store = state
            .managed_agents_store_lock
            .lock()
            .map_err(|e| e.to_string())?;
        (
            crate::managed_agents::retention::active_retention_scope(&app, &state).ok(),
            crate::managed_agents::device_runtime::capture_runtime_fence(&state)?,
        )
    };
    let persona_id = input.id.clone();
    let mut owner_keys = None;
    let workspace_relay = retention_scope
        .as_ref()
        .map(|scope| scope.relay_url.clone())
        .unwrap_or_else(|| crate::relay::relay_ws_url_with_override(&state));
    let (avatar_snapshot, transfers) = {
        let _guard = state
            .managed_agents_store_lock
            .lock()
            .map_err(|e| e.to_string())?;
        crate::managed_agents::device_runtime::assert_runtime_fence(&state, &edit_fence)?;
        let raw = crate::managed_agents::persona_device_view::read_policy_records(
            &crate::managed_agents::managed_agents_store_path(&app)?,
        )?;
        let personas = crate::managed_agents::persona_definitions_for_policy(&raw);
        let persona = personas
            .iter()
            .find(|p| p.id == input.id)
            .ok_or_else(|| format!("agent {} not found", input.id))?;
        let avatar = trim_optional(input.avatar_url.clone());
        let mut transfers = Vec::new();
        let snapshot = if persona.avatar_url != avatar {
            let records = avatar_targets_with_keys(&app, &state, persona, &edit_fence)?;
            let mut changed = persona.clone();
            changed.avatar_url = avatar;
            changed.acp_command = trim_optional(input.acp_command.clone());
            transfers = plan_persona_avatar_transfers(&changed, &records, &workspace_relay)?;
            owner_keys = Some(
                retention_scope
                    .as_ref()
                    .ok_or("Signing keys unavailable for avatar transfer")?
                    .owner_keys
                    .clone(),
            );
            Some(avatar_settings_snapshot(persona, &records)?)
        } else {
            None
        };
        (snapshot, transfers)
    };
    if let Some(owner) = &owner_keys {
        transfer_persona_avatars_before_save(&app, &edit_fence, &state, owner, &transfers).await?;
    }

    // Phase 1: synchronous save (persona record + linked agent avatar updates)
    let (result, retained, profile_sync_params) = tokio::task::spawn_blocking({
        let app = app.clone();
        move || -> Result<(AgentDefinition, R, ProfileSyncParams), String> {
            let state = app.state::<AppState>();
            update_persona_locked_with(
                input,
                &app,
                &state,
                |app, state, persona| retain(app, state, persona, retention_scope.as_ref()),
                crate::managed_agents::persona_device_view::load_device_policy_context,
                crate::managed_agents::storage::resolve_agent_key_readonly,
                crate::commands::agents::retain_managed_agent_pending,
                || try_regenerate_nest(&app),
                |personas| {
                    if let Some(expected) = avatar_snapshot {
                        let current = personas.iter().find(|p| p.id == persona_id)
                            .ok_or("Agent changed during avatar transfer; retry the edit")?;
                        crate::managed_agents::device_runtime::assert_runtime_fence(&state, &edit_fence)
                            .map_err(|_| "Agent settings or workspace changed during avatar transfer; retry the edit")?;
                        let records = avatar_targets_with_keys(&app, &state, current, &edit_fence)?;
                        if avatar_settings_snapshot(current, &records)? != expected {
                            return Err("Agent settings or workspace changed during avatar transfer; retry the edit".into());
                        }
                    } else {
                        crate::managed_agents::device_runtime::assert_runtime_fence(&state, &edit_fence)?;
                    }
                    Ok(())
                },
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
    before_save: impl FnOnce(&[AgentDefinition]) -> Result<(), String>,
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
    let raw = crate::managed_agents::persona_device_view::read_policy_records(
        &crate::managed_agents::managed_agents_store_path(app)?,
    )?;
    before_save(&crate::managed_agents::persona_definitions_for_policy(&raw))?;
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
