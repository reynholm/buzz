# Fork patches

Generated from `scripts/fork/patches.json`; regenerate with
`python3 scripts/fork/sync.py render`.

Base: `desktop-v0.5.26` (`2b4b138dc5cf2d9cc1a0ceb21d9063ff56fe8bf4`).

## `docs/superpowers/plans/2026-10-03-buzz-fork-maintenance.md`

Approved fork maintenance implementation plan

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`

## `docs/superpowers/plans/2026-10-03-device-bound-agents.md`

Approved device ownership implementation plan

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`

## `docs/superpowers/specs/2026-10-03-device-bound-agents-design.md`

Approved device ownership and fork acceptance contract

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`

## `FORK_PATCHES.md`

Reviewable generated patch inventory

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`

## `scripts/fork/sync.py`

Validate lost patch paths, symbols and production invocation seams

New module: `true`.

- Required symbol: `def validate_patches(`
- Required symbol: `def main(`
- Invocation: `scripts/fork/sync.py` → `validate_patches`; exact call `errors = validate_patches(args.repo, manifest)`; behavior test `test_cli_rejects_unlisted_path`; verify `python3 -m unittest discover -s scripts/fork/tests -v`
- Verify: `python3 scripts/fork/sync.py validate`

## `scripts/fork/patches.json`

Authoritative patch inventory

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`

## `scripts/fork/tests/test_patch_registry.py`

Mutation coverage for the production registry validator

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`

## `docs/fork/MAINTENANCE.md`

Owner update policy and reproducible baseline evidence

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`

## `desktop/src-tauri/src/commands/agent_config_tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/agents.rs`

Mechanical struct-literal compatibility repair: new device fields default to None; enforce/test creation policy, private defaults, atomic claims or pre-migration recovery; Task6: Carry original create/manual runtime fence through local/provider tails, guard summaries and persist only authorized target records; Task7: Guard exact instance deletion before assignment, process, key, store and journal effects; preserve unrelated raw rows; pin create profile continuation to original runtime fence; Task8 durable original-scope home deletion/label publication and retry

New module: `false`.

- Invocation: `desktop/src-tauri/src/commands/agents.rs` → `deploy_to_provider_scoped`; exact call `provider_deploy::deploy_to_provider_scoped(
                &app,
                &state,
                &pubkey,
                provider_deploy::ProviderStartScope {
                    relay: Some(&create_scope.relay_url),`; behavior test `managed_agents::device_runtime_tests::create_postcommit_scope_is_pinned_before_preflight_and_after_await`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::device_runtime_tests::create_postcommit_scope_is_pinned_before_preflight_and_after_await -- --exact`
- Invocation: `desktop/src-tauri/src/commands/agents.rs` → `deploy_to_provider_scoped`; exact call `provider_deploy::deploy_to_provider_scoped(
                &app,
                &state,
                &pubkey,
                provider_deploy::ProviderStartScope {
                    relay: expected_relay_url.as_deref(),`; behavior test `managed_agents::device_runtime_tests::owning_provider_refuses_deploy_effects`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::device_runtime_tests::owning_provider_refuses_deploy_effects -- --exact`
- Invocation: `desktop/src-tauri/src/commands/agents.rs` → `start_local_agent_with_preflight`; exact call `start_local_agent_with_preflight(
            &app,
            &state,
            &pubkey,
            true,
            runtime_start::LocalStartScope {
                relay: Some(&create_scope.relay_url),
                owner: Some(&create_scope.owner_pubkey),
                replay_floor: None,
                fence: Some(&create_fence),
            },
        )`; behavior test `managed_agents::device_runtime_tests::create_postcommit_scope_is_pinned_before_preflight_and_after_await`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::device_runtime_tests::create_postcommit_scope_is_pinned_before_preflight_and_after_await -- --exact`
- Invocation: `desktop/src-tauri/src/commands/agents.rs` → `publish_agent_profile_with_about`; exact call `profile::publish_agent_profile_with_about(
        &app,
        &state,
        &agent.pubkey,
        &create_fence,`; behavior test `create_import_profile_invocation_never_redirects_after_owner_relay_switch`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib create_import_profile_invocation_never_redirects_after_owner_relay_switch`
- Invocation: `desktop/src-tauri/src/commands/agents.rs` → `delete_managed_agent_phase_with`; exact call `delete_managed_agent_phase_with(
                &app,
                &state,
                &pubkey,`; behavior test `direct_delete_device_guard_precedes_assignment_key_process_store_and_journal_effects`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib direct_delete_device_guard_precedes_assignment_key_process_store_and_journal_effects`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/agents_tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None; Task7: Isolated owning device authority and original publication regressions

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/personas/create.rs`

Mechanical struct-literal compatibility repair: new device fields default to None; enforce/test creation policy, private defaults, atomic claims or pre-migration recovery

New module: `false`.

- Invocation: `desktop/src-tauri/src/commands/personas/create.rs` → `definition_from_request`; exact call `let persona = definition_from_request(input, &device, &now_iso())?;`; behavior test `commands::personas::create::tests::dialog_draft_and_catalog_default_private_with_own_origin`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace commands::personas::create::tests::dialog_draft_and_catalog_default_private_with_own_origin -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/personas/delete_cascade_tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None; Task7: Isolated owning device authority and original publication regressions

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/personas/inbound/catalog_reconcile_tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/personas/inbound/inbound_tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/personas/pending.rs`

Mechanical struct-literal compatibility repair: new device fields default to None; Read-only catalog projection for list without creating retained databases; Task7: Generic retention projection used by native isolated definition-edit transaction

New module: `false`.

- Required symbol: `fn project_persona_sharing_read_only`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/personas/sharing.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/personas/snapshot/fidelity_tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/personas/snapshot/import.rs`

Mechanical struct-literal compatibility repair: new device fields default to None; enforce/test creation policy, private defaults, atomic claims or pre-migration recovery; Task7: Carry original import runtime fence through profile and each memory signing/dispatch continuation

New module: `false`.

- Required symbol: `materialize_import_avatar_scoped`
- Required symbol: `import_upload_authority`
- Required symbol: `decode_snapshot_for_import_readonly`
- Invocation: `desktop/src-tauri/src/commands/personas/snapshot/import.rs` → `materialize_import_avatar_scoped`; exact call `let effective_avatar = materialize_import_avatar_scoped(`; behavior test `commands::personas::snapshot::import::import_avatar_tests::avatar_import_refuses_changed_backend_scope_before_upload_and_after_await`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace commands::personas::snapshot::import::import_avatar_tests::avatar_import_refuses_changed_backend_scope_before_upload_and_after_await -- --exact`
- Invocation: `desktop/src-tauri/src/commands/personas/snapshot/import.rs` → `import_upload_authority`; exact call `let authority = import_upload_authority(&state, &context.scope)?;`; behavior test `commands::personas::snapshot::import::import_avatar_tests::avatar_upload_uses_bound_relay_and_signer_after_backend_switch`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace commands::personas::snapshot::import::import_avatar_tests::avatar_upload_uses_bound_relay_and_signer_after_backend_switch -- --exact`
- Invocation: `desktop/src-tauri/src/commands/personas/snapshot/import.rs` → `decode_snapshot_for_import_readonly`; exact call `decode_snapshot_for_import_readonly(
            &input.file_bytes,
            owner_keys.as_ref(),
            &crate::managed_agents::managed_agents_store_path(&app)?,
            &context.proof,
            crate::managed_agents::storage::resolve_agent_key_readonly,`; behavior test `commands::personas::snapshot::tests::locked_import::confirm_readonly_shared_unbound_endpoint_unlocks_under_another_owner`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace commands::personas::snapshot::tests::locked_import::confirm_readonly_shared_unbound_endpoint_unlocks_under_another_owner -- --exact`
- Invocation: `desktop/src-tauri/src/commands/personas/snapshot/import.rs` → `resolve_key`; exact call `let keys = resolve_key(record)?.ok_or_else(|| LOCKED_CARD_REFUSAL.to_string())?;`; behavior test `commands::personas::snapshot::tests::locked_import::confirm_readonly_proven_local_endpoint_reads_only_exact_recipient`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace commands::personas::snapshot::tests::locked_import::confirm_readonly_proven_local_endpoint_reads_only_exact_recipient -- --exact`
- Invocation: `desktop/src-tauri/src/commands/personas/snapshot/import.rs` → `decode_snapshot_for_import`; exact call `decode_snapshot_for_import(file_bytes, owner_keys, &[recipient])`; behavior test `commands::personas::snapshot::tests::locked_import::confirm_readonly_definitionless_legacy_endpoint_unlocks`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace commands::personas::snapshot::tests::locked_import::confirm_readonly_definitionless_legacy_endpoint_unlocks -- --exact`
- Invocation: `desktop/src-tauri/src/commands/personas/snapshot/import.rs` → `read_policy_records`; exact call `crate::managed_agents::persona_device_view::read_policy_records(store_path)?;`; behavior test `commands::personas::snapshot::tests::locked_import::confirm_readonly_structural_and_key_errors_fail_closed`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace commands::personas::snapshot::tests::locked_import::confirm_readonly_structural_and_key_errors_fail_closed -- --exact`
- Invocation: `desktop/src-tauri/src/commands/personas/snapshot/import.rs` → `publish_persona_profile`; exact call `crate::commands::agents::publish_persona_profile(
        &app,
        &state,
        &record.pubkey,
        &import_fence,`; behavior test `create_import_profile_invocation_never_redirects_after_owner_relay_switch`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib create_import_profile_invocation_never_redirects_after_owner_relay_switch`
- Invocation: `desktop/src-tauri/src/commands/personas/snapshot/import.rs` → `publish_snapshot_memory_entry_with`; exact call `crate::commands::agents::snapshot_publication::publish_snapshot_memory_entry_with(
                &app,
                &state,
                &record.pubkey,
                &import_fence,`; behavior test `original_snapshot_memory_invocation_never_signs_or_sends_into_switched_scope`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib original_snapshot_memory_invocation_never_signs_or_sends_into_switched_scope`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/personas/snapshot/tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None; enforce/test creation policy, private defaults, atomic claims or pre-migration recovery

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/personas/update/name_propagation_tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/team_snapshot.rs`

Mechanical struct-literal compatibility repair: new device fields default to None; enforce/test creation policy, private defaults, atomic claims or pre-migration recovery; Task7: Carry original team import runtime fence through member profile and memory publication

New module: `false`.

- Invocation: `desktop/src-tauri/src/commands/personas/snapshot/import.rs` → `definition_from_snapshot`; exact call `crate::commands::team_snapshot::definition_from_snapshot(`; behavior test `commands::personas::snapshot::tests::imported_snapshot_constructor_defaults_private_with_own_origin`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace commands::personas::snapshot::tests::imported_snapshot_constructor_defaults_private_with_own_origin -- --exact`
- Invocation: `desktop/src-tauri/src/commands/team_snapshot.rs` → `publish_agent_profile_with_about`; exact call `crate::commands::agents::publish_agent_profile_with_about(
            &app,
            &state,
            &m.pubkey,
            &import_fence,`; behavior test `create_import_profile_invocation_never_redirects_after_owner_relay_switch`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib create_import_profile_invocation_never_redirects_after_owner_relay_switch`
- Invocation: `desktop/src-tauri/src/commands/team_snapshot.rs` → `publish_snapshot_memory_entry`; exact call `crate::commands::agents::snapshot_publication::publish_snapshot_memory_entry(
                    &app,
                    &state,
                    &m.pubkey,
                    &import_fence,`; behavior test `original_snapshot_memory_invocation_never_signs_or_sends_into_switched_scope`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib original_snapshot_memory_invocation_never_signs_or_sends_into_switched_scope`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/team_snapshot/tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None; enforce/test creation policy, private defaults, atomic claims or pre-migration recovery

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/teams/adopt/apply.rs`

Mechanical struct-literal compatibility repair: new device fields default to None; enforce/test creation policy, private defaults, atomic claims or pre-migration recovery

New module: `false`.

- Invocation: `desktop/src-tauri/src/commands/teams/adopt/apply.rs` → `plan_add_on_device`; exact call `let plan = plan_add_on_device(`; behavior test `commands::teams::adopt::tests::catalog_team_copy_defaults_private_on_adopting_device`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace commands::teams::adopt::tests::catalog_team_copy_defaults_private_on_adopting_device -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/teams/adopt/tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None; enforce/test creation policy, private defaults, atomic claims or pre-migration recovery

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/teams/pending/tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/teams/sharing/tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/event_sync_team_catalog_tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/agent_events.rs`

Preserve explicit instance-event allowlist and repair legacy fixture

New module: `false`.

- Required symbol: `pub fn agent_event_content`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/agent_snapshot.rs`

Document intentional omission of device metadata from portable snapshots

New module: `false`.

- Required symbol: `pub fn build_snapshot`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/agent_snapshot_envelope.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/agent_snapshot_tests.rs`

Legacy snapshot fixture repair and metadata-independent portable bytes coverage

New module: `false`.

- Required symbol: `fn device_metadata_does_not_change_portable_snapshot_bytes`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/config_bridge/effort_tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/config_bridge/reader_tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/device_policy_types_tests.rs`

Behavior coverage for legacy bytes, unified policy projections, IPC and portable export exclusions

New module: `true`.

- Required symbol: `fn legacy_policy_bytes_and_hash_stay_identical`
- Required symbol: `fn policy_roundtrip_survives_unified_projection`
- Required symbol: `fn portable_exports_exclude_device_metadata`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/discovery/tests.rs`

Keep existing discovery tests and module topology; move explicit device-compatible resolution fixtures to tests/fixtures.rs to restore the inherited size ratchet

New module: `false`.

- Required symbol: `mod fixtures;`
- Required symbol: `use fixtures::{persona_with_runtime, record_with};`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/effective_config/tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/global_config/tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/mod.rs`

Register the device policy compatibility test module; Register home policy, sync and projection modules; Register device-home migration production module; enforce/test creation policy, private defaults, atomic claims or pre-migration recovery; Task6: Register runtime authorization module and owning test module; Task7: Register device metadata merge and local authoring authority modules and isolated owning regressions

New module: `false`.

- Required symbol: `mod device_policy_types_tests;`
- Required symbol: `pub(crate) mod definition_home;`
- Required symbol: `pub(crate) mod device_home_sync;`
- Required symbol: `pub(crate) mod persona_device_view;`
- Required symbol: `pub(crate) mod device_home_migration;`
- Required symbol: `pub(crate) use restore::child_ownership::{retry_restore_cleanup, RestoreCleanup};`
- Required symbol: `mod device_runtime;`
- Required symbol: `mod device_runtime_tests;`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/nest/render_tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/parallelism.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/persona_events.rs`

Append optional device metadata to persona wire content and exclude it from drift hash

New module: `false`.

- Required symbol: `pub fn persona_content_hash`
- Required symbol: `pub fn persona_event_content`
- Required symbol: `pub fn persona_from_event`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/persona_events/tests.rs`

Legacy persona literal repair and independent device-field hash invariance coverage

New module: `false`.

- Required symbol: `fn each_device_metadata_field_is_excluded_from_content_hash`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/personas.rs`

Mechanical struct-literal compatibility repair: new device fields default to None; Use existing pure built-in merge for policy list visibility without saving; enforce/test creation policy, private defaults, atomic claims or pre-migration recovery

New module: `false`.

- Required symbol: `fn persona_definitions_for_policy`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/personas/tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/readiness.rs`

Keep readiness production code and tests unchanged; extract explicit env-resolution record fixture under the existing cfg(test) module to restore the inherited size ratchet

New module: `false`.

- Required symbol: `mod fixtures;`
- Required symbol: `fixtures::record_with_env(env_vars)`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/runtime/test_fixtures.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/runtime/tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/spawn_snapshot/tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/team_catalog/tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/team_snapshot.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/teams_tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None; Task7: Isolated owning device authority and original publication regressions

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/types.rs`

Optional public device metadata and local binding records; preserve unified projections

New module: `false`.

- Required symbol: `pub share_across_devices: Option<bool>`
- Required symbol: `pub origin_device_id: Option<String>`
- Required symbol: `pub origin_device_label: Option<String>`
- Required symbol: `pub origin_released: Option<bool>`
- Required symbol: `pub device_host_binding: Option<String>`
- Required symbol: `pub fn into_agent_record`
- Required symbol: `pub fn to_definition_view`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/types/requests.rs`

Creation-only camelCase device sharing IPC input

New module: `false`.

- Required symbol: `pub share_across_devices: Option<bool>`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/types/tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/mesh_llm/recovery.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/migration_avatar_tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `docs/nips/NIP-AP.md`

Document optional device execution metadata and old-client enforcement limitations

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/Cargo.toml`

Safe hostname default using the already locked gethostname dependency

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/Cargo.lock`

Direct Desktop gethostname dependency without version changes

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/device_identity.rs`

Atomic interprocess device identity and separate verified local keychain proof

New module: `true`.

- Required symbol: `pub fn load_or_create_device_identity`
- Required symbol: `pub(crate) fn load_or_create_host_proof`
- Required symbol: `fn load_existing_device_identity`
- Required symbol: `fn load_existing_host_proof`
- Required symbol: `fn initialize_device_authority`
- Invocation: `desktop/src-tauri/src/device_identity.rs` → `validate_identity`; exact call `validate_identity(&identity)?;`; behavior test `malformed_identity_is_preserved_and_reported`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml device_identity::tests::malformed_identity_is_preserved_and_reported -- --exact`
- Invocation: `desktop/src-tauri/src/device_identity.rs` → `get_or_create_verified`; exact call `store.get_or_create_verified("host", || uuid::Uuid::new_v4().to_string())?`; behavior test `unavailable_keychain_does_not_rebind`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml device_identity::tests::unavailable_keychain_does_not_rebind -- --exact`
- Invocation: `desktop/src-tauri/src/device_identity.rs` → `lock`; exact call `lock.lock()`; behavior test `concurrent_identity_initialization_has_one_uuid`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml device_identity::tests::concurrent_identity_initialization_has_one_uuid -- --exact`
- Invocation: `desktop/src-tauri/src/device_identity.rs` → `read_existing_verified`; exact call `store.read_existing_verified("host")?`; behavior test `device_identity::tests::existing_proof_is_fresh_read_only_and_missing_fails`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml device_identity::tests::existing_proof_is_fresh_read_only_and_missing_fails -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/device_identity/tests.rs`

Independent-process UUID initialization and injected secret load/store/verification failures

New module: `true`.

- Required symbol: `fn concurrent_identity_initialization_has_one_uuid`
- Required symbol: `fn deleted_device_json_keeps_host_proof`
- Required symbol: `fn unavailable_keychain_does_not_rebind`
- Required symbol: `fn demo_marker_does_not_touch_production`
- Required symbol: `fn concurrent_host_proof_initialization_generates_once`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/device_identity.rs`

Expose public device metadata using app data directory and hostname; storage behavior covered below IPC routing, isolated native UI routing remains later acceptance; Task8 durable original-scope home deletion/label publication and retry

New module: `true`.

- Required symbol: `pub fn get_device_identity`
- Required symbol: `load_or_create_device_identity(&directory.join("device.json"), label)`
- Required symbol: `pub fn set_device_label`
- Required symbol: `pub struct DeviceLabelResult`
- Required symbol: `DeviceLabelPublication`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/mod.rs`

Register device identity IPC command

New module: `false`.

- Required symbol: `pub use device_identity::*;`
- Required symbol: `pub use device_home_sync::*;`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/lib.rs`

Register device identity module and public metadata IPC command; Task6: Recovery-mode boot invokes authorized snapshot backfill only through the guarded boot seam

New module: `false`.

- Required symbol: `mod device_identity;`
- Required symbol: `commands::get_device_identity,`
- Required symbol: `commands::begin_device_home_sync,`
- Required symbol: `commands::hydrate_device_home_history,`
- Required symbol: `commands::finish_device_home_sync,`
- Required symbol: `commands::invalidate_device_home_sync,`
- Required symbol: `device_identity::initialize_device_authority(&app_handle)`
- Required symbol: `commands::set_device_label,`
- Invocation: `desktop/src-tauri/src/lib.rs` → `run_boot_backfill_with`; exact call `managed_agents::restore::run_boot_backfill_with(recovery_mode, || {
                backfill_persona_snapshots(&app_handle)
            }) {
                eprintln!("buzz-desktop: persona-snapshot backfill failed: {e}");
            }
`; behavior test `managed_agents::restore::runtime_backfill_tests::recovery_boot_has_no_backfill_effect`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::restore::runtime_backfill_tests::recovery_boot_has_no_backfill_effect -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/build_identity.rs`

Separate host proof service for production and named demo builds

New module: `false`.

- Required symbol: `pub(crate) fn device_host_service`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/secret_store.rs`

Fresh-read get-or-create under existing blob lock with raw-byte durable verification

New module: `false`.

- Required symbol: `pub fn get_or_create_verified`
- Required symbol: `fn mutate_blob_verified`
- Required symbol: `fn read_existing_verified`
- Invocation: `desktop/src-tauri/src/secret_store.rs` → `acquire_blob_lock`; exact call `let _lock = acquire_blob_lock(&self.service)?;`; behavior test `concurrent_host_proof_initialization_generates_once`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml device_identity::tests::concurrent_host_proof_initialization_generates_once -- --exact`
- Invocation: `desktop/src-tauri/src/secret_store.rs` → `read_blob_raw`; exact call `let raw = self.read_blob_raw()?;`; behavior test `verified_value_uses_fresh_storage_and_generates_once`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml device_identity::tests::verified_value_uses_fresh_storage_and_generates_once -- --exact`
- Invocation: `desktop/src-tauri/src/secret_store.rs` → `read_blob_raw`; exact call `if verify && self.read_blob_raw()?.as_deref() != Some(json.as_bytes())`; behavior test `unavailable_keychain_does_not_rebind`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml device_identity::tests::unavailable_keychain_does_not_rebind -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/definition_home.rs`

Pure proof-first home classification and readiness-dependent capabilities

New module: `true`.

- Required symbol: `enum HomeKind`
- Required symbol: `fn classify_definition_home`
- Required symbol: `fn definition_capabilities`
- Invocation: `desktop/src-tauri/src/managed_agents/definition_home.rs` → `matches`; exact call `proof.matches(b)`; behavior test `managed_agents::definition_home::tests::home_matrix`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::definition_home::tests::home_matrix -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/definition_home/tests.rs`

Literal home matrix, canonical links, flattened wire compatibility and signed scoped retained evidence

New module: `true`.

- Required symbol: `fn home_matrix`
- Required symbol: `fn flattened_device_view_is_compatible_and_contains_no_host_authority`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/device_home_sync.rs`

Backend token/scope hydration, exhaustive authenticated paging and live-apply completion barrier; Run deferred migration with prospective readiness under hydrated/drained/token/scope barrier before installing Ready; Task6: Expose store-locked runtime subscription generation and advance it on session replacement/invalidation/reset

New module: `true`.

- Required symbol: `fn begin_session`
- Required symbol: `fn finish_session`
- Required symbol: `async fn hydrate_history`
- Required symbol: `struct DeviceHomeHistory`
- Required symbol: `fn finish_session_with`
- Required symbol: `fn runtime_generation_locked`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_sync.rs` → `active`; exact call `let s = active(sync, scope, token)?;`; behavior test `managed_agents::device_home_sync::tests::stale_sync_session_cannot_ready_new_scope`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::device_home_sync::tests::stale_sync_session_cannot_ready_new_scope -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_sync.rs` → `ordered_heads`; exact call `ordered_heads(collected)`; behavior test `managed_agents::device_home_sync::tests::exhaustive_history_applies_catalog_last_and_coalesces`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::device_home_sync::tests::exhaustive_history_applies_catalog_last_and_coalesces -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_sync.rs` → `begin_apply`; exact call `begin_apply(state,Some(token))?`; behavior test `managed_agents::device_home_sync::tests::finish_waits_for_live_apply_and_latches_error_after_ready`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::device_home_sync::tests::finish_waits_for_live_apply_and_latches_error_after_ready -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_sync.rs` → `migrate`; exact call `migrate(scope)`; behavior test `managed_agents::device_home_sync::tests::deferred_migration_failure_never_installs_ready`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::device_home_sync::tests::deferred_migration_failure_never_installs_ready -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/device_home_sync/tests.rs`

Real signed pagination/dense-boundary/coalescing tests and stale/live-error state tests; Deferred migration error latch and token/hydration/drain regressions

New module: `true`.

- Required symbol: `async fn paging_is_inclusive_exhaustive_and_deduped_before_catalog_apply`
- Required symbol: `async fn finish_waits_for_live_apply_and_latches_error_after_ready`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/persona_device_view.rs`

Read-only structural snapshots, fresh host authority, authoritative remaining signed retained heads and explicit unavailable home; Explicit prospective-readiness context loader avoids recursive sync locking

New module: `true`.

- Required symbol: `struct PersonaDeviceView`
- Required symbol: `fn read_policy_records`
- Required symbol: `fn load_device_policy_context`
- Required symbol: `fn read_remote_evidence`
- Required symbol: `fn load_device_policy_context_at`
- Invocation: `desktop/src-tauri/src/managed_agents/persona_device_view.rs` → `definition_capabilities`; exact call `definition_capabilities(&definition, &home, self.readiness, proven)`; behavior test `managed_agents::definition_home::tests::home_matrix`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::definition_home::tests::home_matrix -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/persona_device_view.rs` → `get_retained_events_by_kind`; exact call `get_retained_events_by_kind(&conn, 30177, owner)?`; behavior test `commands::personas::inbound::device_sync_tests::signed_multi_coordinate_deletion_keeps_unselected_remote_head_and_newer_recreation`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml commands::personas::inbound::device_sync_tests::signed_multi_coordinate_deletion_keeps_unselected_remote_head_and_newer_recreation -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/persona_device_view.rs` → `load_device_policy_context_at`; exact call `load_device_policy_context_at(app, scope, readiness)`; behavior test `commands::device_home_sync::migration_tests::finish_adapter_migrates_before_ready_under_actual_barrier`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml commands::device_home_sync::migration_tests::finish_adapter_migrates_before_ready_under_actual_barrier -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/device_home_sync.rs`

Native IPC adapters, captured-key history transport and reset-before-open/error-aware finalization; Async IPC preserves external unit result, serializes finish with workspace apply, migrates before Ready and retries fresh restore against returned verified scope; top-level Wry entry wiring is compile coverage with native acceptance outstanding

New module: `true`.

- Required symbol: `fn begin_device_home_sync`
- Required symbol: `async fn hydrate_device_home_history`
- Required symbol: `fn finish_device_home_sync`
- Required symbol: `fn invalidate_device_home_sync`
- Required symbol: `async fn finish_device_home_sync`
- Required symbol: `async fn retry_device_home_restore_with`
- Invocation: `desktop/src-tauri/src/commands/device_home_sync.rs` → `reset`; exact call `device_home_sync::reset(state)?;`; behavior test `commands::device_home_sync::tests::begin_retention_error_revokes_previous_readiness`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml commands::device_home_sync::tests::begin_retention_error_revokes_previous_readiness -- --exact`
- Invocation: `desktop/src-tauri/src/commands/device_home_sync.rs` → `finish_session_with`; exact call `device_home_sync::finish_session_with(state, session_token, migrate)?;`; behavior test `commands::device_home_sync::migration_tests::finish_adapter_key_error_latches_failed_and_preserves_store`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml commands::device_home_sync::migration_tests::finish_adapter_key_error_latches_failed_and_preserves_store -- --exact`
- Invocation: `desktop/src-tauri/src/commands/device_home_sync.rs` → `capture_scope`; exact call `device_home_sync::capture_scope(state)? != *expected`; behavior test `commands::device_home_sync::migration_tests::returned_completion_scope_fences_deferred_restore`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml commands::device_home_sync::migration_tests::returned_completion_scope_fences_deferred_restore -- --exact`
- Invocation: `desktop/src-tauri/src/commands/device_home_sync.rs` → `restore_managed_agents_on_launch`; exact call `crate::managed_agents::restore_managed_agents_on_launch(&app, &state.shutdown_started)`; behavior test `commands::device_home_sync::migration_tests::returned_completion_scope_fences_deferred_restore`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml commands::device_home_sync::migration_tests::returned_completion_scope_fences_deferred_restore -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/personas/device_view_tests.rs`

Isolated actual list projection for mixed/shared/private context failures without writes

New module: `true`.

- Required symbol: `fn list_context_errors_are_explicit_and_preserve_only_shared_capabilities`
- Required symbol: `list_projects_retained_catalog_sharing_without_writing_or_requiring_host_proof`
- Required symbol: `identity_recovery_preserves_visible_list_without_scope_or_signing`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/personas/inbound/device_sync_tests.rs`

Signed hydration through actual blocking inbound dispatcher with isolated app paths

New module: `true`.

- Required symbol: `async fn hydration_applies_signed_catalog_through_production_dispatcher_and_propagates_failure`
- Required symbol: `signed_multi_coordinate_deletion_keeps_unselected_remote_head_and_newer_recreation`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/personas/mod.rs`

Read-only flattened list views with shared capability fast path on explicit unavailable context; Task7: Atomically authorize local definition deletion and cascade targets before effects; restrict housekeeping and persistence to selected targets; Task8 durable original-scope home deletion/label publication and retry

New module: `false`.

- Required symbol: `fn list_personas_inner`
- Invocation: `desktop/src-tauri/src/commands/personas/mod.rs` → `persona_definitions_for_policy`; exact call `persona_definitions_for_policy(&records)`; behavior test `commands::personas::device_view_tests::list_context_errors_are_explicit_and_preserve_only_shared_capabilities`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml commands::personas::device_view_tests::list_context_errors_are_explicit_and_preserve_only_shared_capabilities -- --exact`
- Invocation: `desktop/src-tauri/src/commands/personas/mod.rs` → `project_persona_sharing_read_only`; exact call `pending::project_persona_sharing_read_only(
            &retention_path,
            &scope.owner_pubkey,
            &mut personas,
        )?;`; behavior test `commands::personas::device_view_tests::list_projects_retained_catalog_sharing_without_writing_or_requiring_host_proof`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml commands::personas::device_view_tests::list_projects_retained_catalog_sharing_without_writing_or_requiring_host_proof -- --exact`
- Invocation: `desktop/src-tauri/src/commands/personas/mod.rs` → `unavailable`; exact call `PersonaDeviceView::unavailable(definition, error.clone())`; behavior test `commands::personas::device_view_tests::list_context_errors_are_explicit_and_preserve_only_shared_capabilities`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml commands::personas::device_view_tests::list_context_errors_are_explicit_and_preserve_only_shared_capabilities -- --exact`
- Invocation: `desktop/src-tauri/src/commands/personas/mod.rs` → `capture_scope`; exact call `device_home_sync::capture_scope(&state).and_then(|scope| {`; behavior test `commands::personas::device_view_tests::identity_recovery_preserves_visible_list_without_scope_or_signing`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml commands::personas::device_view_tests::identity_recovery_preserves_visible_list_without_scope_or_signing -- --exact`
- Invocation: `desktop/src-tauri/src/commands/personas/mod.rs` → `delete_persona_phase_with`; exact call `delete_persona_phase_with(
                &app,
                &state,
                &id,`; behavior test `local_definition_delete_device_guard_precedes_every_cascade_effect`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib local_definition_delete_device_guard_precedes_every_cascade_effect`
- Invocation: `desktop/src-tauri/src/commands/personas/mod.rs` → `complete_cascade_home_operations`; exact call `complete_cascade_home_operations(&app, &home_operations)?;`; behavior test `definition_cascade_retry_keeps_instance_tombstone_archive_without_release_resurrection`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace definition_cascade_retry_keeps_instance_tombstone_archive_without_release_resurrection`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/app_state.rs`

In-memory scoped backend device home sync state; retain failed restore child cleanup handles separately from authorized runtime pairs

New module: `false`.

- Required symbol: `device_home_sync:`
- Required symbol: `managed_agent_restore_cleanup: crate::managed_agents::RestoreCleanup`
- Required symbol: `managed_agent_restore_cleanup: Default::default()`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/workspace.rs`

Invalidate backend home evidence on workspace apply; Migrate proven homes before scoped event sync without waiting for frontend history; top-level Wry entry wiring is compile coverage with native acceptance outstanding; initialize captured scoped retention schema before read-only policy, propagating open/schema errors while retaining Pending; enforce/test creation policy, private defaults, atomic claims or pre-migration recovery; Task8 durable original-scope home deletion/label publication and retry

New module: `false`.

- Required symbol: `device_home_sync::reset(&state)?;`
- Required symbol: `fn prepare_workspace_event_sync`
- Required symbol: `fn prepare_workspace_event_sync_with`
- Required symbol: `recover`
- Invocation: `desktop/src-tauri/src/commands/workspace.rs` → `prepare_workspace_event_sync`; exact call `prepare_workspace_event_sync(&restore_app, &scope)?;`; behavior test `commands::workspace::device_home_preparation_tests::fresh_workspace_preparation_initializes_scope_without_authorizing_absence`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml commands::workspace::device_home_preparation_tests::fresh_workspace_preparation_initializes_scope_without_authorizing_absence -- --exact`
- Invocation: `desktop/src-tauri/src/commands/workspace.rs` → `open_retention_db`; exact call `crate::managed_agents::retention::open_retention_db(&scope.db_path)?;`; behavior test `commands::workspace::device_home_preparation_tests::fresh_workspace_preparation_initializes_scope_without_authorizing_absence`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml commands::workspace::device_home_preparation_tests::fresh_workspace_preparation_initializes_scope_without_authorizing_absence -- --exact`
- Invocation: `desktop/src-tauri/src/commands/workspace.rs` → `migrate_device_homes_before_sync`; exact call `crate::managed_agents::device_home_migration::migrate_device_homes_before_sync(app)`; behavior test `commands::workspace::device_home_preparation_tests::fresh_workspace_preparation_initializes_scope_without_authorizing_absence`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml commands::workspace::device_home_preparation_tests::fresh_workspace_preparation_initializes_scope_without_authorizing_absence -- --exact`
- Invocation: `desktop/src-tauri/src/commands/workspace.rs` → `recover`; exact call `recover()?;
    migrate()`; behavior test `commands::workspace::device_home_preparation_tests::recovery_failure_blocks_migration_without_rewriting_intent`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace commands::workspace::device_home_preparation_tests::recovery_failure_blocks_migration_without_rewriting_intent -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/personas/inbound.rs`

Token-scoped live apply leases fence async reconciliation and record failures; Task6: Accept legitimate remote owner heads without local author proof; independently skip copied runtime refresh and secret effects while preserving hydration retry semantics; Task7: Merge optional device metadata only after accepted owner head; preserve proven local lineage; reject wrong author; apply accepted tombstones as receiver operations

New module: `false`.

- Required symbol: `session_token: Option<String>`
- Required symbol: `lease.complete(&result)?;`
- Required symbol: `fn reconcile_inbound_tombstone_with_refresh`
- Required symbol: `enum InboundRuntimeRefresh`
- Invocation: `desktop/src-tauri/src/commands/personas/inbound.rs` → `reconcile_inbound_tombstone_with_refresh`; exact call `reconcile_inbound_tombstone_with_refresh(
            &event,
            &arrival_relay_url,
            &app,
            &state,
            refresh,
        )?`; behavior test `commands::personas::inbound::device_sync_tests::signed_multi_coordinate_deletion_keeps_unselected_remote_head_and_newer_recreation`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml commands::personas::inbound::device_sync_tests::signed_multi_coordinate_deletion_keeps_unselected_remote_head_and_newer_recreation -- --exact`
- Invocation: `desktop/src-tauri/src/commands/personas/inbound.rs` → `inbound_refresh_phase_with`; exact call `crate::managed_agents::device_runtime::inbound_refresh_phase_with(
                    &app,
                    &state,
                    &d_tag,
                    None,
                    crate::managed_agents::persona_device_view::load_device_policy_context,`; behavior test `managed_agents::device_runtime_tests::owning_inbound_refuses_restart_effects`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::device_runtime_tests::owning_inbound_refuses_restart_effects -- --exact`
- Invocation: `desktop/src-tauri/src/commands/personas/inbound.rs` → `start_local_agent_pairs_scoped`; exact call `super::super::agents::runtime_start::start_local_agent_pairs_scoped(
                &app,
                &state,
                &pubkey,
                &relay_urls,
                Some(&fence),`; behavior test `managed_agents::device_runtime_tests::owning_inbound_refuses_restart_effects`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::device_runtime_tests::owning_inbound_refuses_restart_effects -- --exact`
- Invocation: `desktop/src-tauri/src/commands/personas/inbound.rs` → `deploy_to_provider_scoped`; exact call `super::super::agents::provider_deploy::deploy_to_provider_scoped(&app, &state, &pubkey,
                super::super::agents::provider_deploy::ProviderStartScope { relay: Some(&fence.scope.relay_url), owner: Some(&fence.scope.owner_pubkey), replay_floor: None, fence: Some(&fence) })
            .await
            .map_err(|error| {
                format!(
                    "Inbound agent access was saved, but its provider deployment failed to refresh with the new policy: {error}"`; behavior test `managed_agents::device_runtime_tests::owning_inbound_refuses_restart_effects`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::device_runtime_tests::owning_inbound_refuses_restart_effects -- --exact`
- Invocation: `desktop/src-tauri/src/commands/personas/inbound.rs` → `merge_inbound_device_metadata`; exact call `crate::managed_agents::device_inbound::merge_inbound_device_metadata(
                local,
                &mut inbound,
                proven,
            );`; behavior test `commands::personas::inbound::device_metadata_tests`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib commands::personas::inbound::device_metadata_tests`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src/features/agents/lib/usePersonaSync.ts`

Live-first backend hydration, rejection-aware drain, bounded fresh-session retry and connection invalidation

New module: `false`.

- Required symbol: `export function startPersonaSync`
- Required symbol: `hydrateDeviceHomeHistory(sessionRun.token)`
- Required symbol: `subscribeToConnectionState`
- Required symbol: `subscribeToReconnects`
- Required symbol: `sessionRun.controller.signal`
- Required symbol: `liveConfirmed && !degraded && !failed`
- Required symbol: `const queueRestart`
- Required symbol: `clearTimeout(restartDelay.timer);`
- Invocation: `desktop/src/features/agents/lib/usePersonaSync.ts` → `finishDeviceHomeSync`; exact call `await finishDeviceHomeSync(sessionRun.token);`; behavior test `backend sync waits for buffered live applies before finish and carries its token`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test --test-name-pattern="backend sync waits for buffered" src/features/agents/lib/usePersonaSync.test.mjs`
- Invocation: `desktop/src/features/agents/lib/usePersonaSync.ts` → `invalidateDeviceHomeSync`; exact call `invalidateDeviceHomeSync(run.token)`; behavior test `connection loss invalidates readiness and reconnect starts a fresh complete session`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test --test-name-pattern="connection loss invalidates" src/features/agents/lib/usePersonaSync.test.mjs`
- Invocation: `desktop/src/features/agents/lib/usePersonaSync.ts` → `abort`; exact call `run.controller.abort();`; behavior test `terminal CLOSED after Ready immediately invalidates and replacement exhaustively hydrates`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test --test-name-pattern="terminal CLOSED after Ready" src/features/agents/lib/usePersonaSyncRelayHealth.test.mjs`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/lib/usePersonaSync.test.mjs`

Preserved coalescing/catalog/gap/degraded/retry behavior plus IPC token, completion, cancellation and reconnect tests; Task9 deferred apply rejection plus exact token disposal invalidation

New module: `false`.

- Required symbol: `backend sync waits for buffered live applies`
- Required symbol: `late previous hydration cannot finalize a replacement session`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/shared/api/tauriPersonas.ts`

Backend sync IPC client and optional live reconciliation token; Task9 maps public snake_case policy/origin and actual camelCase nullable computed projection without invented authority; create sends false default, edit omits device metadata

New module: `false`.

- Required symbol: `export async function beginDeviceHomeSync`
- Required symbol: `export async function hydrateDeviceHomeHistory`
- Required symbol: `sessionToken?: string`
- Required symbol: `shareAcrossDevices: input.shareAcrossDevices ?? false`
- Required symbol: `home?: DefinitionHome | null`
- Required symbol: `invokeTauri<DeviceHomeHistoryResult>`
- Invocation: `desktop/src/shared/api/tauriPersonas.ts` → `invokeTauri`; exact call `await invokeTauri("reconcile_inbound_persona_event", {
    eventJson,
    arrivalRelayUrl,
    sessionToken,
  });`; behavior test `backend sync waits for buffered live applies before finish and carries its token`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test --test-name-pattern="backend sync waits for buffered" src/features/agents/lib/usePersonaSync.test.mjs`
- Invocation: `desktop/src/shared/api/tauriPersonas.ts` → `fromRawPersona`; exact call `return (await invokeTauri<RawPersona[]>("list_personas")).map(fromRawPersona);`; behavior test `raw_home_maps_pubkeys_and_capabilities`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/shared/api/tauriPersonas.test.mjs`
- Invocation: `desktop/src/shared/api/tauriPersonas.ts` → `updatePersonaPayload`; exact call `input: updatePersonaPayload(input),`; behavior test `edit_payload_does_not_change_policy`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/shared/api/tauriPersonas.test.mjs`
- Invocation: `desktop/src/shared/api/tauriPersonas.ts` → `invokeTauri`; exact call `await invokeTauri("invalidate_device_home_sync", { sessionToken });`; behavior test `deferred apply rejection remains failed and disposal invalidates exactly its backend token`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/agents/lib/usePersonaSync.test.mjs`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/shared/api/relayClientShared.ts`

Optional sustained live subscription health while preserving initial readiness API

New module: `false`.

- Required symbol: `type LiveSubscriptionHealth`
- Required symbol: `onHealth?: (health: LiveSubscriptionHealth) => void;`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/shared/api/relayClientSession.ts`

Backward-compatible health observer for live readiness timeout and one-shot removal cleanup; delegate unchanged live setup to relayLiveSubscription.ts with explicit session-owned dependencies to restore the inherited size ratchet

New module: `false`.

- Required symbol: `type LiveSubscriptionHealth`
- Required symbol: `return subscribeLiveSession(`
- Invocation: `desktop/src/shared/api/relayClientSession.ts` → `subscribe`; exact call `return this.subscribe(
      filter,
      onEvent,
      onReady,
      readinessTimeoutMs,
      signal,
      undefined,
      onHealth,
    );`; behavior test `unconfirmed timeout cannot hydrate or finish and confirmed retry starts a fresh session`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test --test-name-pattern="unconfirmed timeout" src/features/agents/lib/usePersonaSyncRelayHealth.test.mjs`
- Invocation: `desktop/src/shared/api/relayClientSession.ts` → `subscribeLiveSession`; exact call `return subscribeLiveSession(`; behavior test `unconfirmed timeout cannot hydrate or finish and confirmed retry starts a fresh session`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test --test-name-pattern="unconfirmed timeout" src/features/agents/lib/usePersonaSyncRelayHealth.test.mjs`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/shared/api/relayClosedRecovery.ts`

Persistent CLOSED health and ownership guard stop detached retries; shared quota gate survives owner retirement

New module: `false`.

- Required symbol: `subscription.onHealth?.("closed")`
- Required symbol: `subscription.onHealth?.("eose")`
- Required symbol: `if (subscriptions.get(subId) !== subscription) return;`
- Invocation: `desktop/src/shared/api/relayClosedRecovery.ts` → `onHealth`; exact call `subscription.onHealth?.("closed");`; behavior test `retryable CLOSED after Ready retires the degraded subscription before fresh recovery`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test --test-name-pattern="retryable CLOSED after Ready" src/features/agents/lib/usePersonaSyncRelayHealth.test.mjs`
- Invocation: `desktop/src/shared/api/relayClosedRecovery.ts` → `activateRateLimit`; exact call `if (closedClass === "rate-limited") activateRateLimit(hintSeconds);`; behavior test `rate-limited CLOSED retires readiness and preserves shared admission cooldown`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test --test-name-pattern="rate-limited CLOSED retires readiness" src/features/agents/lib/usePersonaSyncRelayHealth.test.mjs`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/lib/usePersonaSyncRelayHealth.test.mjs`

Actual hook plus real RelayClient CLOSED/EOSE/timeout recovery and immediate timer disposal tests

New module: `true`.

- Required symbol: `terminal CLOSED before confirmation`
- Required symbol: `terminal CLOSED after Ready`
- Required symbol: `unconfirmed timeout`
- Required symbol: `disposal after retryable CLOSED`
- Required symbol: `rate-limited CLOSED retires readiness`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src-tauri/src/managed_agents/device_home_migration.rs`

Atomic legacy binding/origin snapshot, read-only key verification, prewrite scope fence, shared bypass and durable signed-head retry; Task7: Delegate common publication authority to canonical exact target checks

New module: `true`.

- Required symbol: `fn migrate_device_homes_locked`
- Required symbol: `fn migrate_device_homes_in_dir`
- Required symbol: `fn may_publish_local_instance`
- Required symbol: `fn publication_allowed`
- Required symbol: `fn needs_private_authority`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_migration.rs` → `resolve`; exact call `resolve(&records[i])?`; behavior test `managed_agents::device_home_migration::tests::legacy_claim_requires_available_key`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::device_home_migration::tests::legacy_claim_requires_available_key -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_migration.rs` → `authorize_instance_authority`; exact call `super::device_authority::authorize_instance_authority(
        record,
        definition,
        context,
        super::device_authority::InstanceAuthorityAction::PublishHead,
    )`; behavior test `managed_agents::device_authority_tests::direct_update_or_delete_copied_instance_has_no_side_effects`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::device_authority_tests::direct_update_or_delete_copied_instance_has_no_side_effects`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_migration.rs` → `atomic_write_json_restricted`; exact call `atomic_write_json_restricted(&path, &bytes)?;`; behavior test `managed_agents::device_home_migration::tests::legacy_claim_requires_available_key`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::device_home_migration::tests::legacy_claim_requires_available_key -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_migration.rs` → `migrate_device_homes_locked_with`; exact call `migrate_device_homes_locked_with(app, &context, resolve)`; behavior test `managed_agents::device_home_migration::tests::workspace_hook_defers_legacy_but_reclaims_proven_origin_before_sync`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::device_home_migration::tests::workspace_hook_defers_legacy_but_reclaims_proven_origin_before_sync -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_migration.rs` → `queue_device_home_events`; exact call `queue_device_home_events(&base, &keys, &db, context)?;`; behavior test `managed_agents::device_home_migration::tests::workspace_hook_defers_legacy_but_reclaims_proven_origin_before_sync`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::device_home_migration::tests::workspace_hook_defers_legacy_but_reclaims_proven_origin_before_sync -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/device_home_migration/tests.rs`

Native temp-app migration/retention copy, mixed proof, recovery, idempotence, atomic failure and shared-only regressions

New module: `true`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/event_sync.rs`

Suppress copied-only30175 publication, preserve proven-local-wins and shared-only best effort after successful structural read; private/unknown scopes fail closed

New module: `false`.

- Required symbol: `fn migrate_personas_in_dir_with_context`
- Required symbol: `fn identity_event_sync_leg`
- Invocation: `desktop/src-tauri/src/event_sync.rs` → `publication_allowed`; exact call `crate::managed_agents::device_home_migration::publication_allowed(
                    i,
                    Some(record),
                    context,
                )`; behavior test `managed_agents::device_home_migration::tests::copy_suppresses_both_outbound_kinds_and_restore_candidates`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::device_home_migration::tests::copy_suppresses_both_outbound_kinds_and_restore_candidates -- --exact`
- Invocation: `desktop/src-tauri/src/event_sync.rs` → `any`; exact call `linked_publication.iter().any(|allowed| *allowed)`; behavior test `managed_agents::device_home_migration::tests::proven_home_publishes_definition_among_foreign_copies`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::device_home_migration::tests::proven_home_publishes_definition_among_foreign_copies -- --exact`
- Invocation: `desktop/src-tauri/src/event_sync.rs` → `identity_event_sync_leg`; exact call `identity_event_sync_leg(records, |private| {`; behavior test `event_sync::home_publication_adapter_tests::private_and_unknown_event_sync_errors_propagate`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml event_sync::home_publication_adapter_tests::private_and_unknown_event_sync_errors_propagate -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/reconcile.rs`

Guard each30177 instance publication; proof/context failure propagates in private scopes, shared-only best effort preserved; Task7: Guard canonical instance boot heads, publish explicit shared rows even when mixed private proof fails, and verify captured retention owner/relay

New module: `false`.

- Required symbol: `fn reconcile_agents_in_dir_with_context`
- Invocation: `desktop/src-tauri/src/managed_agents/reconcile.rs` → `publication_allowed`; exact call `super::device_home_migration::publication_allowed(
            record,
            definition.as_ref(),
            context,
        )`; behavior test `managed_agents::device_home_migration::tests::copy_suppresses_both_outbound_kinds_and_restore_candidates`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::device_home_migration::tests::copy_suppresses_both_outbound_kinds_and_restore_candidates -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/reconcile.rs` → `identity_event_sync_leg`; exact call `crate::event_sync::identity_event_sync_leg(records, |private| {`; behavior test `event_sync::home_publication_adapter_tests::private_and_unknown_event_sync_errors_propagate`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml event_sync::home_publication_adapter_tests::private_and_unknown_event_sync_errors_propagate -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/storage.rs`

Read-only migration key resolver propagates backend/malformed-secret errors and verifies pubkey; expose existing hydration boundary for eligible-only auto-start work; protected restore merge reloads current raw unified store and preserves excluded keys/definitions/concurrent rows, injectable existing KeyStore hydration/persistence

New module: `false`.

- Required symbol: `fn resolve_agent_key_readonly`
- Required symbol: `fn resolve_agent_key_readonly_with`
- Required symbol: `fn save_restore_records_with`
- Required symbol: `pub(crate) trait KeyStore`
- Required symbol: `pub fn write_agent_runtime_receipt<R: tauri::Runtime>`
- Invocation: `desktop/src-tauri/src/managed_agents/storage.rs` → `load_all_readonly`; exact call `.load_all_readonly()?`; behavior test `managed_agents::storage::migration_key_tests::migration_key_resolver_validates_read_only_secrets_and_errors`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::storage::migration_key_tests::migration_key_resolver_validates_read_only_secrets_and_errors -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/storage.rs` → `load_agent_store`; exact call `let mut raw = load_agent_store(app)?;`; behavior test `managed_agents::restore::device_home_restore_tests::phase_c_fresh_authority_blocks_changed_target_and_preserves_concurrent_rows`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::phase_c_fresh_authority_blocks_changed_target_and_preserves_concurrent_rows -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/storage.rs` → `atomic_write_json_restricted`; exact call `atomic_write_json_restricted(&managed_agents_store_path(app)?, &bytes)`; behavior test `managed_agents::restore::device_home_restore_tests::mixed_restore_phase_a_save_and_phase_c_writeback_preserve_foreign_inline_copy`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::mixed_restore_phase_a_save_and_phase_c_writeback_preserve_foreign_inline_copy -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/restore.rs`

Select proven/shared/standalone auto-start candidates before key hydration or lifecycle work, retaining existing live-pair duplicate guards; top-level Wry entry wiring is compile coverage with native acceptance outstanding; authority lookup excludes private records not selected for auto-start; PhaseA and fresh PhaseC protected raw-store merge preserves excluded rows/definitions, keys restricted to actual authorized targets, baseline safe disabled housekeeping remains structural-only; mesh preflight error uses same protected writeback; retain spawned-child ownership across fresh authority failures and delegate bounded settlement/retry to child_ownership; Task6: Guard boot backfill before secrets/metadata, preserve mixed shared jobs, locked final restore spawn and carry original fence through mesh/writeback/child ownership

New module: `false`.

- Required symbol: `fn select_auto_start_candidates`
- Required symbol: `fn needs_auto_start_authority`
- Required symbol: `fn prepare_restore_phase_a_with`
- Required symbol: `fn complete_restore_phase_c_with`
- Required symbol: `fn authorized_restore_updates`
- Required symbol: `fn persist_restore_error_with`
- Required symbol: `child_ownership::complete_restore_spawn_results_with`
- Required symbol: `child_ownership::retry_restore_cleanup`
- Required symbol: `fn backfill_persona_snapshots_with`
- Required symbol: `fn run_boot_backfill_with`
- Invocation: `desktop/src-tauri/src/managed_agents/restore.rs` → `auto_start_allowed`; exact call `super::device_home_migration::auto_start_allowed(record, &views, context)?`; behavior test `managed_agents::runtime_commands::device_home_job_tests::copied_and_deferred_auto_start_jobs_have_zero_hydration_and_probes`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::runtime_commands::device_home_job_tests::copied_and_deferred_auto_start_jobs_have_zero_hydration_and_probes -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore.rs` → `select_auto_start_candidates`; exact call `select_auto_start_candidates(&policy_records, context.as_ref())?;`; behavior test `managed_agents::restore::device_home_restore_tests::mixed_restore_phase_a_save_and_phase_c_writeback_preserve_foreign_inline_copy`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::mixed_restore_phase_a_save_and_phase_c_writeback_preserve_foreign_inline_copy -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore.rs` → `save_restore_records_with`; exact call `super::storage::save_restore_records_with(app, &records, &eligible, persist)?;`; behavior test `managed_agents::restore::device_home_restore_tests::mixed_restore_phase_a_save_and_phase_c_writeback_preserve_foreign_inline_copy`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::mixed_restore_phase_a_save_and_phase_c_writeback_preserve_foreign_inline_copy -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore.rs` → `save_restore_records_with`; exact call `super::storage::save_restore_records_with(app, &records, &key_targets, persist)?;`; behavior test `managed_agents::restore::device_home_restore_tests::mixed_restore_phase_c_reload_and_save_never_import_excluded_copy`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::mixed_restore_phase_c_reload_and_save_never_import_excluded_copy -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore.rs` → `authorized_restore_updates`; exact call `authorized_restore_updates(&policy_records, context.as_ref())?;`; behavior test `managed_agents::restore::device_home_restore_tests::disabled_safe_housekeeping_persists_without_key_operations`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::disabled_safe_housekeeping_persists_without_key_operations -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore.rs` → `authorized_restore_updates`; exact call `authorized_restore_updates(&raw, context.as_ref())?;`; behavior test `managed_agents::restore::device_home_restore_tests::phase_c_fresh_authority_blocks_changed_target_and_preserves_concurrent_rows`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::phase_c_fresh_authority_blocks_changed_target_and_preserves_concurrent_rows -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore.rs` → `complete_restore_phase_c_with`; exact call `complete_restore_phase_c_with(
        expected,
        app,
        &[pubkey.to_string()].into_iter().collect(),`; behavior test `managed_agents::restore::device_home_restore_tests::mesh_preflight_error_writeback_preserves_excluded_inline_copy`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --features mesh-llm managed_agents::restore::device_home_restore_tests::mesh_preflight_error_writeback_preserves_excluded_inline_copy -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore.rs` → `complete_restore_spawn_results_with`; exact call `child_ownership::complete_restore_spawn_results_with(
        (app, Some(&restore_fence)),
        spawn_results,`; behavior test `managed_agents::restore::device_home_restore_tests::post_spawn_authority_error_settles_all_owned_children`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::post_spawn_authority_error_settles_all_owned_children -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore.rs` → `retry_restore_cleanup`; exact call `child_ownership::retry_restore_cleanup(app)?;`; behavior test `managed_agents::restore::device_home_restore_tests::failed_child_cleanup_propagates_and_retains_retry_ownership`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::failed_child_cleanup_propagates_and_retains_retry_ownership -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore.rs` → `backfill_persona_snapshots_with`; exact call `backfill_persona_snapshots_with(
        app,
        super::persona_device_view::load_device_policy_context,
        super::storage::hydrate_keys,
        super::storage::persist_agent_keys,
    )`; behavior test `managed_agents::restore::runtime_backfill_tests::boot_backfill_skips_copied_private_and_preserves_shared_metadata_path`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::restore::runtime_backfill_tests::boot_backfill_skips_copied_private_and_preserves_shared_metadata_path -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore.rs` → `restore_spawn_phase_with`; exact call `super::device_runtime::restore_spawn_phase_with(
                            app,
                            &state,
                            &record.pubkey,
                            Some(&restore_fence_ref.scope),
                            super::persona_device_view::load_device_policy_context,`; behavior test `managed_agents::device_runtime_tests::owning_restore_refuses_final_receipt_effects`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::device_runtime_tests::owning_restore_refuses_final_receipt_effects -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore.rs` → `runtime_preflight_with`; exact call `super::device_runtime::runtime_preflight_with(
                app,
                &state,
                &record.pubkey,
                Some(&restore_fence),
                super::persona_device_view::load_device_policy_context,`; behavior test `managed_agents::device_runtime_tests::shared_preflight_refuses_subscription_replacement_after_await`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::device_runtime_tests::shared_preflight_refuses_subscription_replacement_after_await -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore.rs` → `prepare_restore_phase_a_with`; exact call `let agents_to_start = prepare_restore_phase_a_with(
        Some(&restore_fence),
        app,
        shutdown_started,
        super::persona_device_view::load_device_policy_context,
        super::storage::hydrate_keys,`; behavior test `managed_agents::restore::device_home_restore_tests::restore_phase_a_authority_scope_switch_refuses_before_key_effects`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::restore::device_home_restore_tests::restore_phase_a_authority_scope_switch_refuses_before_key_effects -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/runtime_commands.rs`

Guard actual auto-start job selection before key hydration and bounded relay probes; preserve shared and standalone baseline; shared-only selected jobs avoid proof lookup even when unrelated disabled private rows exist; Task6: Atomic authorized restart/terminate, target-only persistence, mixed private/shared jobs, fresh scoped probes and fenced pair start

New module: `false`.

- Required symbol: `fn auto_start_jobs_with`
- Required symbol: `async fn probe_auto_start_jobs`
- Required symbol: `fn start_pair`
- Required symbol: `fn probe_auto_start_job_with`
- Required symbol: `fn start_managed_agent_pair_scoped`
- Invocation: `desktop/src-tauri/src/managed_agents/runtime_commands.rs` → `select_auto_start_candidates`; exact call `super::restore::select_auto_start_candidates(&records, context.as_ref())?;`; behavior test `managed_agents::runtime_commands::device_home_job_tests::copied_and_deferred_auto_start_jobs_have_zero_hydration_and_probes`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::runtime_commands::device_home_job_tests::copied_and_deferred_auto_start_jobs_have_zero_hydration_and_probes -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/runtime_commands.rs` → `needs_auto_start_authority`; exact call `super::restore::needs_auto_start_authority(&records)`; behavior test `managed_agents::runtime_commands::device_home_job_tests::shared_jobs_ignore_unselected_private_authority`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::runtime_commands::device_home_job_tests::shared_jobs_ignore_unselected_private_authority -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/runtime_commands.rs` → `start_pair_phase_with`; exact call `super::device_runtime::start_pair_phase_with(
        &app,
        &state,
        &pubkey,
        expected_scope.map(|e| &e.scope),
        super::persona_device_view::load_device_policy_context,`; behavior test `managed_agents::device_runtime_tests::owning_manual_start_refuses_receipt_effects`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::device_runtime_tests::owning_manual_start_refuses_receipt_effects -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/runtime_commands.rs` → `probe_auto_start_job_with`; exact call `probe_auto_start_job_with(
            &app,
            RuntimeProbeInput {
                record,
                requested,
                fence: &runtime_fence,`; behavior test `managed_agents::runtime_commands::reconcile_callback_tests::restore_and_reconcile_skip_foreign_pairs_before_probe`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::runtime_commands::reconcile_callback_tests::restore_and_reconcile_skip_foreign_pairs_before_probe -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/runtime_commands.rs` → `start_pair`; exact call `start_pair(pubkey, relay_url, true, None, None, false, app)
}

pub(crate) fn start_managed_agent_pair_scoped(
    pubkey: String,
    relay_url: String,`; behavior test `managed_agents::device_runtime_tests::shared_lifecycle_matches_baseline`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::device_runtime_tests::shared_lifecycle_matches_baseline -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/runtime_commands.rs` → `auto_start_jobs_with`; exact call `let jobs = auto_start_jobs_with(
        Some(&runtime_fence),
        &app,
        &communities,
        super::persona_device_view::load_device_policy_context,
        super::storage::hydrate_keys,`; behavior test `managed_agents::runtime_commands::candidate_scope_tests::reconcile_authority_scope_switch_refuses_before_key_effects`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::runtime_commands::candidate_scope_tests::reconcile_authority_scope_switch_refuses_before_key_effects -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/migration.rs`

Document owner-dependent device-home migration belongs after fold/detach in workspace apply, never preidentity boot

New module: `false`.

- Required symbol: `Device-home migration deliberately waits for owner/workspace hydration`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/reconcile/tests.rs`

Existing linked-instance slim event/idempotence fixture injects matching host proof through guarded production reconcile engine; Task7: Canonical linked definition fixture preserves slimming reconciliation regression assertions

New module: `false`.

- Required symbol: `fn slimming_republish_wave_is_one_time`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/workspace_device_home_tests.rs`

Task4 fix1 regression tests bind production owning orchestration with temporary app storage and injected existing authority/process/KeyStore boundaries; enforce/test creation policy, private defaults, atomic claims or pre-migration recovery

New module: `true`.

- Required symbol: `fn fresh_workspace_preparation_initializes_scope_without_authorizing_absence`
- Required symbol: `fn workspace_preparation_database_open_and_schema_errors_are_fatal`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml device_home_preparation_tests`

## `desktop/src-tauri/src/managed_agents/restore/device_home_tests.rs`

Task4 fix1 regression tests bind production owning orchestration with temporary app storage and injected existing authority/process/KeyStore boundaries; Task6: Compatibility adaptation for fenced restore completion/error interfaces; historical child cleanup and row-preservation behavior retained

New module: `true`.

- Required symbol: `fn mixed_restore_phase_a_save_and_phase_c_writeback_preserve_foreign_inline_copy`
- Required symbol: `fn mixed_restore_phase_c_reload_and_save_never_import_excluded_copy`
- Required symbol: `fn disabled_safe_housekeeping_persists_without_key_operations`
- Required symbol: `fn phase_c_fresh_authority_blocks_changed_target_and_preserves_concurrent_rows`
- Required symbol: `fn mesh_preflight_error_writeback_preserves_excluded_inline_copy`
- Required symbol: `fn proven_disabled_housekeeping_uses_captured_context_without_key_operations`
- Required symbol: `fn post_spawn_authority_error_settles_all_owned_children`
- Required symbol: `fn fresh_authority_rejection_cleans_only_new_rejected_child`
- Required symbol: `fn failed_child_cleanup_propagates_and_retains_retry_ownership`
- Required symbol: `fn receipt_failure_settles_unregistered_child_and_preserves_error`
- Required symbol: `fn new_child_collision_never_replaces_previously_tracked_child`
- Required symbol: `fn restore_child_exit_confirmation_is_bounded_and_tree_is_reaped`
- Required symbol: `fn confirmed_exited_owner_allows_replacement_receipt_and_reconcile`
- Required symbol: `fn owner_inspection_error_preserves_handle_settles_incoming_and_propagates`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml device_home_restore_tests`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --features mesh-llm device_home_restore_tests`

## `desktop/src-tauri/src/managed_agents/restore/child_ownership.rs`

Own spawned restore children through authorized registration or target-verified bounded terminate/reap; retain failed cleanup handles for restore retry and shutdown; preserve live/uninspectable pair owners while permitting confirmed-exited replacements; Task6: Carry original RuntimeFence to restore PhaseC while retaining exact child settlement and durable cleanup retry

New module: `true`.

- Required symbol: `fn complete_restore_spawn_results_with`
- Required symbol: `fn settle_restore_child`
- Required symbol: `fn retry_restore_cleanup`
- Required symbol: `fn wait_for_restore_child_exit`
- Required symbol: `struct RestoreCleanup`
- Required symbol: `fn complete_restore_spawn_results_with_inspection`
- Invocation: `desktop/src-tauri/src/managed_agents/restore/child_ownership.rs` → `settle_restore_child`; exact call `settle_restore_child(app, key, process, &mut cleanup)`; behavior test `managed_agents::restore::device_home_restore_tests::post_spawn_authority_error_settles_all_owned_children`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::post_spawn_authority_error_settles_all_owned_children -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore/child_ownership.rs` → `retry_restore_cleanup_with`; exact call `retry_restore_cleanup_with(app, terminate_restore_child)`; behavior test `managed_agents::restore::device_home_restore_tests::failed_child_cleanup_propagates_and_retains_retry_ownership`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::failed_child_cleanup_propagates_and_retains_retry_ownership -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore/child_ownership.rs` → `terminate_process`; exact call `super::super::terminate_process(process.child.id())?;`; behavior test `managed_agents::restore::device_home_restore_tests::restore_child_exit_confirmation_is_bounded_and_tree_is_reaped`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::restore_child_exit_confirmation_is_bounded_and_tree_is_reaped -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore/child_ownership.rs` → `wait_for_restore_child_exit`; exact call `wait_for_restore_child_exit(process, std::time::Duration::from_secs(1))`; behavior test `managed_agents::restore::device_home_restore_tests::restore_child_exit_confirmation_is_bounded_and_tree_is_reaped`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::restore_child_exit_confirmation_is_bounded_and_tree_is_reaped -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore/child_ownership.rs` → `write_agent_runtime_receipt`; exact call `super::super::write_agent_runtime_receipt(app, &receipt)`; behavior test `managed_agents::restore::device_home_restore_tests::receipt_failure_settles_unregistered_child_and_preserves_error`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::receipt_failure_settles_unregistered_child_and_preserves_error -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore/child_ownership.rs` → `complete_restore_spawn_results_with_inspection`; exact call `complete_restore_spawn_results_with_inspection(`; behavior test `managed_agents::restore::device_home_restore_tests::new_child_collision_never_replaces_previously_tracked_child`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::new_child_collision_never_replaces_previously_tracked_child -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore/child_ownership.rs` → `try_wait`; exact call `process
                .child
                .try_wait()`; behavior test `managed_agents::restore::device_home_restore_tests::confirmed_exited_owner_allows_replacement_receipt_and_reconcile`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::confirmed_exited_owner_allows_replacement_receipt_and_reconcile -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore/child_ownership.rs` → `inspect_exit`; exact call `inspect_exit(&mut existing.process)`; behavior test `managed_agents::restore::device_home_restore_tests::owner_inspection_error_preserves_handle_settles_incoming_and_propagates`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::owner_inspection_error_preserves_handle_settles_incoming_and_propagates -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace --features mesh-llm`

## `desktop/src-tauri/src/shutdown.rs`

Retry independently owned failed restore cleanup under shutdown transition before structural/key reads; preserve cleanup errors while existing tracked-agent shutdown continues

New module: `false`.

- Required symbol: `let restore_cleanup_error = managed_agents::retry_restore_cleanup(app).err();`
- Required symbol: `match restore_cleanup_error`
- Invocation: `desktop/src-tauri/src/shutdown.rs` → `retry_restore_cleanup`; exact call `managed_agents::retry_restore_cleanup(app).err();`; behavior test `managed_agents::restore::device_home_restore_tests::failed_child_cleanup_propagates_and_retains_retry_ownership`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::failed_child_cleanup_propagates_and_retains_retry_ownership -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/discovery/tests/fixtures.rs`

Explicit unchanged discovery persona/record constructors; preserve original field values including device None defaults

New module: `true`.

- Required symbol: `pub(super) fn persona_with_runtime`
- Required symbol: `pub(super) fn record_with`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::discovery::tests`

## `desktop/src-tauri/src/managed_agents/readiness/tests/fixtures.rs`

Explicit unchanged readiness env-resolution record fixture; preserve test-pubkey, test-agent, buzz-acp, buzz-agent, timeout320 and every other original field

New module: `true`.

- Required symbol: `pub(super) fn record_with_env`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::readiness::tests::resolve_effective_agent_env_user_env_wins_over_structured_fields -- --exact`

## `desktop/src/shared/api/relayLiveSubscription.ts`

Own unchanged live subscription registration/readiness/cancellation lifetime, health callbacks, priority and session-fenced removal; transport pacing and quota remain in the session

New module: `true`.

- Required symbol: `export async function subscribeLiveSession`
- Required symbol: `onHealth?.("removed")`
- Required symbol: `subscription.onHealth?.("timeout")`
- Required symbol: `epoch === session.currentEpoch()`
- Invocation: `desktop/src/shared/api/relayLiveSubscription.ts` → `sendRawWithReconnectRetry`; exact call `session.sendRawWithReconnectRetry(`; behavior test `cold setup drains at most one live REQ per 250ms, prioritizes visible channel and preserves every filter`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/shared/api/relayClientBurstDrain.test.mjs`
- Invocation: `desktop/src/shared/api/relayLiveSubscription.ts` → `closeSubscription`; exact call `if (epoch === session.currentEpoch())
      await session.closeSubscription(subId);`; behavior test `workspace switch during in-flight setup cannot close or reset the new socket`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/shared/api/relayClientLiveCancellation.test.mjs`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src-tauri/src/migration/backfill.rs`

Device creation policy, default-private sources, atomic claim and durable recovery regression coverage

New module: `false`.

- Invocation: `desktop/src-tauri/src/migration/backfill.rs` → `public_device_at`; exact call `crate::managed_agents::device_creation::public_device_at(&identity_path)?;`; behavior test `migration::backfill::tests::manufactured_definition_defaults_private_and_identity_errors_preserve_store`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace migration::backfill::tests::manufactured_definition_defaults_private_and_identity_errors_preserve_store -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/migration/backfill_tests.rs`

Device creation policy, default-private sources, atomic claim and durable recovery regression coverage

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/managed_agents/device_creation.rs`

Device creation policy, default-private sources, atomic claim and durable recovery regression coverage

New module: `true`.

- Required symbol: `creation_phase_locked`
- Required symbol: `authorize_definition_action`
- Required symbol: `creation_phase`
- Required symbol: `stamp_new_definition`
- Invocation: `desktop/src-tauri/src/commands/agents.rs` → `creation_phase_locked`; exact call `crate::managed_agents::device_creation::creation_phase_locked(
            &app,
            &state,
            requested_persona_id.as_deref(),
            Some(&create_scope),
            crate::managed_agents::persona_device_view::load_device_policy_context,
            |_| {`; behavior test `managed_agents::device_creation::tests::remote_create_has_no_mint_publish_or_save`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace managed_agents::device_creation::tests::remote_create_has_no_mint_publish_or_save -- --exact`
- Invocation: `desktop/src-tauri/src/commands/agents.rs` → `creation_phase_locked`; exact call `crate::managed_agents::device_creation::creation_phase_locked(
            &app,
            &state,
            requested_persona_id.as_deref(),
            Some(&create_scope),
            crate::managed_agents::persona_device_view::load_device_policy_context,
            |context| {`; behavior test `managed_agents::device_creation::tests::native_creation_adapter_rechecks_real_readiness_and_scope_after_await`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace managed_agents::device_creation::tests::native_creation_adapter_rechecks_real_readiness_and_scope_after_await -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/device_creation.rs` → `authorize_definition_action`; exact call `authorize_definition_action(&context, d, records, DefinitionAction::CreateInstance)?;`; behavior test `managed_agents::device_creation::tests::remote_create_has_no_mint_publish_or_save`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace managed_agents::device_creation::tests::remote_create_has_no_mint_publish_or_save -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/device_creation.rs` → `creation_phase`; exact call `creation_phase(
        &scope,`; behavior test `managed_agents::device_creation::tests::native_creation_adapter_refuses_before_all_effects_and_preserves_structural_bytes`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace managed_agents::device_creation::tests::native_creation_adapter_refuses_before_all_effects_and_preserves_structural_bytes -- --exact`
- Invocation: `desktop/src-tauri/src/commands/personas/create.rs` → `stamp_new_definition`; exact call `crate::managed_agents::device_creation::stamp_new_definition(
        &mut persona,
        requested_share,
        device,
    );`; behavior test `commands::personas::create::tests::dialog_draft_and_catalog_default_private_with_own_origin`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace commands::personas::create::tests::dialog_draft_and_catalog_default_private_with_own_origin -- --exact`
- Invocation: `desktop/src-tauri/src/commands/team_snapshot.rs` → `stamp_new_definition`; exact call `crate::managed_agents::device_creation::stamp_new_definition(&mut definition, None, device);`; behavior test `commands::team_snapshot::tests::imported_team_definitions_default_private_with_own_origin`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace commands::team_snapshot::tests::imported_team_definitions_default_private_with_own_origin -- --exact`
- Invocation: `desktop/src-tauri/src/commands/teams/adopt/apply.rs` → `stamp_new_definition`; exact call `crate::managed_agents::device_creation::stamp_new_definition(d, None, device);`; behavior test `commands::teams::adopt::tests::catalog_team_copy_defaults_private_on_adopting_device`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace commands::teams::adopt::tests::catalog_team_copy_defaults_private_on_adopting_device -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/personas.rs` → `stamp_new_definition`; exact call `super::device_creation::stamp_new_definition(definition, None, &device);`; behavior test `managed_agents::device_creation::tests::builtin_materialization_defaults_private_on_current_device`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace managed_agents::device_creation::tests::builtin_materialization_defaults_private_on_current_device -- --exact`
- Invocation: `desktop/src-tauri/src/migration/backfill.rs` → `stamp_new_definition`; exact call `crate::managed_agents::device_creation::stamp_new_definition(
            &mut persona_view,
            None,
            &device,
        );`; behavior test `migration::backfill::tests::manufactured_definition_defaults_private_and_identity_errors_preserve_store`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace migration::backfill::tests::manufactured_definition_defaults_private_and_identity_errors_preserve_store -- --exact`
- Invocation: `desktop/src-tauri/src/commands/personas/snapshot/import.rs` → `assert_creation_scope`; exact call `crate::managed_agents::device_creation::assert_creation_scope(`; behavior test `managed_agents::device_creation::tests::create_scope_is_pinned`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace managed_agents::device_creation::tests::create_scope_is_pinned -- --exact`
- Invocation: `desktop/src-tauri/src/commands/team_snapshot.rs` → `assert_creation_scope`; exact call `crate::managed_agents::device_creation::assert_creation_scope(`; behavior test `managed_agents::device_creation::tests::create_scope_is_pinned`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace managed_agents::device_creation::tests::create_scope_is_pinned -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/managed_agents/device_creation/tests.rs`

Device creation policy, default-private sources, atomic claim and durable recovery regression coverage

New module: `true`.

- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/managed_agents/device_home_operations.rs`

Device creation policy, default-private sources, atomic claim and durable recovery regression coverage; Task8 durable original-scope home deletion/label publication and retry

New module: `true`.

- Required symbol: `commit_home_claim_locked`
- Required symbol: `save_journal`
- Required symbol: `enqueue_home_events`
- Required symbol: `recover_in_dir`
- Required symbol: `load_existing_host_proof`
- Required symbol: `recover_home_operations_locked`
- Required symbol: `commit_new_pairs_locked`
- Invocation: `desktop/src-tauri/src/commands/agents.rs` → `commit_home_claim_locked`; exact call `crate::managed_agents::device_home_operations::commit_home_claim_locked(`; behavior test `managed_agents::device_home_operations::tests::released_definition_is_claimed_without_restart`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace managed_agents::device_home_operations::tests::released_definition_is_claimed_without_restart -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_operations.rs` → `save_journal`; exact call `save_journal(dir, &operations)?;
    save(&raw)?;`; behavior test `managed_agents::device_home_operations::tests::failed_claim_save_never_publishes_uncommitted_intent`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace managed_agents::device_home_operations::tests::failed_claim_save_never_publishes_uncommitted_intent -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_operations.rs` → `enqueue_home_events`; exact call `enqueue_home_events(&mut conn, operation)`; behavior test `managed_agents::device_home_operations::tests::claim_transaction_failure_retains_intent_and_never_half_enqueues`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace managed_agents::device_home_operations::tests::claim_transaction_failure_retains_intent_and_never_half_enqueues -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_operations.rs` → `recover_in_dir`; exact call `recover_in_dir(&dir, verify_binding, |operation| {`; behavior test `commands::workspace::device_home_preparation_tests::recovery_precedes_migration_and_replays_original_scope_after_switch`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace commands::workspace::device_home_preparation_tests::recovery_precedes_migration_and_replays_original_scope_after_switch -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_operations.rs` → `load_existing_host_proof`; exact call `crate::device_identity::load_existing_host_proof(`; behavior test `managed_agents::device_home_operations::tests::copied_or_unavailable_proof_never_recovers_claim_and_preserves_intent_bytes`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace managed_agents::device_home_operations::tests::copied_or_unavailable_proof_never_recovers_claim_and_preserves_intent_bytes -- --exact`
- Invocation: `desktop/src-tauri/src/commands/workspace.rs` → `recover_home_operations_locked`; exact call `crate::managed_agents::device_home_operations::recover_home_operations_locked(app)`; behavior test `commands::workspace::device_home_preparation_tests::recovery_precedes_migration_and_replays_original_scope_after_switch`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace commands::workspace::device_home_preparation_tests::recovery_precedes_migration_and_replays_original_scope_after_switch -- --exact`
- Invocation: `desktop/src-tauri/src/commands/personas/snapshot/import.rs` → `commit_new_pairs_locked`; exact call `crate::managed_agents::device_home_operations::commit_new_pairs_locked(`; behavior test `managed_agents::device_home_operations::tests::new_import_pair_preserves_foreign_records_and_persists_only_new_keys`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace managed_agents::device_home_operations::tests::new_import_pair_preserves_foreign_records_and_persists_only_new_keys -- --exact`
- Invocation: `desktop/src-tauri/src/commands/team_snapshot.rs` → `commit_new_pairs_locked`; exact call `crate::managed_agents::device_home_operations::commit_new_pairs_locked(`; behavior test `managed_agents::device_home_operations::tests::new_import_pair_preserves_foreign_records_and_persists_only_new_keys`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace managed_agents::device_home_operations::tests::new_import_pair_preserves_foreign_records_and_persists_only_new_keys -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_operations.rs` → `enqueue_home_events_in_transaction`; exact call `enqueue_home_events_in_transaction(&tx, operation)?;`; behavior test `intent_save_and_each_sql_enqueue_failure_leave_no_partial_batch`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace intent_save_and_each_sql_enqueue_failure_leave_no_partial_batch`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_operations.rs` → `apply_delete`; exact call `delete::apply_delete(&mut raw, op, Some(&mut verify_binding))?`; behavior test `new_instance_cancels_obsolete_release`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace new_instance_cancels_obsolete_release`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_operations.rs` → `prepare_label_retries`; exact call `label::prepare_label_retries(`; behavior test `premetadata_label_worklist_resolves_when_original_database_becomes_known`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace premetadata_label_worklist_resolves_when_original_database_becomes_known`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/managed_agents/device_home_operations/tests.rs`

Device creation policy, default-private sources, atomic claim and durable recovery regression coverage

New module: `true`.

- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/commands/personas/snapshot/import_avatar_tests.rs`

Task5 scope refusal before avatar upload and after await; bound relay/signing authority exercised through actual upload on isolated loopback fixture

New module: `false`.

- Required symbol: `avatar_import_refuses_changed_backend_scope_before_upload_and_after_await`
- Required symbol: `avatar_upload_uses_bound_relay_and_signer_after_backend_switch`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/media.rs`

Task5 snapshot avatar upload consumes captured authority without mutable workspace rereads; ordinary media uploads preserve existing authority capture

New module: `false`.

- Required symbol: `UploadAuthority`
- Required symbol: `do_upload_with_authority`
- Invocation: `desktop/src-tauri/src/commands/personas/snapshot/import.rs` → `upload_image_bytes`; exact call `crate::commands::media::upload_image_bytes(avatar_bytes, &state, &authority)`; behavior test `commands::personas::snapshot::import::import_avatar_tests::avatar_upload_uses_bound_relay_and_signer_after_backend_switch`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace commands::personas::snapshot::import::import_avatar_tests::avatar_upload_uses_bound_relay_and_signer_after_backend_switch -- --exact`
- Invocation: `desktop/src-tauri/src/commands/media.rs` → `do_upload_with_authority`; exact call `do_upload_with_authority(body, &mime, state, None, None, authority).await`; behavior test `commands::personas::snapshot::import::import_avatar_tests::avatar_upload_uses_bound_relay_and_signer_after_backend_switch`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace commands::personas::snapshot::import::import_avatar_tests::avatar_upload_uses_bound_relay_and_signer_after_backend_switch -- --exact`
- Invocation: `desktop/src-tauri/src/commands/media.rs` → `sign_blossom_upload_auth`; exact call `sign_blossom_upload_auth(&authority.keys, &sha256, expiry_secs, base_url)?;`; behavior test `commands::personas::snapshot::import::import_avatar_tests::avatar_upload_uses_bound_relay_and_signer_after_backend_switch`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace commands::personas::snapshot::import::import_avatar_tests::avatar_upload_uses_bound_relay_and_signer_after_backend_switch -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/personas/snapshot/tests_locked.rs`

R1 owning confirm readonly-selection regression: shared/legacy/local exact recipient unlock, foreign private refusal, no unrelated reads/writes, structural/key failures and owner/plain fast paths

New module: `false`.

- Required symbol: `confirm_readonly_shared_unbound_endpoint_unlocks_under_another_owner`
- Required symbol: `confirm_readonly_definitionless_legacy_endpoint_unlocks`
- Required symbol: `confirm_readonly_foreign_private_endpoint_never_reads_keys`
- Required symbol: `confirm_readonly_proven_local_endpoint_reads_only_exact_recipient`
- Required symbol: `confirm_readonly_owner_endpoint_skips_all_agent_secret_lookups`
- Required symbol: `confirm_readonly_plain_snapshot_skips_all_agent_secret_lookups`
- Required symbol: `confirm_readonly_structural_and_key_errors_fail_closed`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/device_runtime.rs`

Exact-target device binding authorization, canonical definition structural reads, proof-free shared/legacy effects, scoped native effect adapters and asynchronous runtime fences

New module: `true`.

- Required symbol: `fn authorize_instance_start`
- Required symbol: `fn runtime_phase_locked_with`
- Required symbol: `struct RuntimeFence`
- Required symbol: `fn runtime_preflight_with`
- Required symbol: `fn save_runtime_record`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace --features mesh-llm`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/managed_agents/device_runtime_tests.rs`

Runtime spies, original creation/preflight scope, copied keys/auth, mixed sibling binding, canonical read failures and baseline positive lifecycle tests

New module: `true`.

- Required symbol: `fn copied_record_never_reaches_spawn`
- Required symbol: `fn runtime_rechecks_scope_after_authority_before_effect`
- Required symbol: `fn shared_and_legacy_owning_adapters_need_no_host_proof`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace --features mesh-llm`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/managed_agents/runtime.rs`

Authorize fresh exact target before logs, receipt reuse and process spawn; use fresh signer/scope and fail closed persona reads

New module: `false`.

- Required symbol: `fn spawn_agent_child`
- Required symbol: `fn start_managed_agent_process`
- Invocation: `desktop/src-tauri/src/managed_agents/runtime.rs` → `spawn_child_phase_with`; exact call `super::device_runtime::spawn_child_phase_with(
        app,
        &state,
        &record.pubkey,
        None,
        super::persona_device_view::load_device_policy_context,`; behavior test `managed_agents::device_runtime_tests::owning_spawn_rejects_copied_key_and_auth`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::device_runtime_tests::owning_spawn_rejects_copied_key_and_auth -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/runtime.rs` → `runtime_phase_locked_with`; exact call `super::device_runtime::runtime_phase_locked_with(
        app,
        &state,
        &record.pubkey,
        None,
        super::persona_device_view::load_device_policy_context,`; behavior test `managed_agents::device_runtime_tests::start_refuses_before_terminating_existing_receipt`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::device_runtime_tests::start_refuses_before_terminating_existing_receipt -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace --features mesh-llm`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/commands/agents/runtime_start.rs`

Cohesive extraction of local preflight/pair orchestration preserving pair interface and original creation scope

New module: `true`.

- Required symbol: `fn start_local_agent_pairs_with_preflight`
- Required symbol: `struct LocalStartScope`
- Invocation: `desktop/src-tauri/src/commands/agents/runtime_start.rs` → `runtime_preflight_with`; exact call `device_runtime::runtime_preflight_with(
        app,
        state,
        pubkey,
        expected,
        crate::managed_agents::persona_device_view::load_device_policy_context,
        |r, d, _| preflight(app, r, d, false),`; behavior test `managed_agents::device_runtime_tests::preflight_rechecks_current_binding_after_await`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::device_runtime_tests::preflight_rechecks_current_binding_after_await -- --exact`
- Invocation: `desktop/src-tauri/src/commands/agents/runtime_start.rs` → `runtime_preflight_with`; exact call `device_runtime::runtime_preflight_with(
        app,
        state,
        pubkey,
        expected,
        crate::managed_agents::persona_device_view::load_device_policy_context,
        |r, d, _| preflight(app, r, d, fresh),`; behavior test `managed_agents::device_runtime_tests::create_postcommit_scope_is_pinned_before_preflight_and_after_await`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::device_runtime_tests::create_postcommit_scope_is_pinned_before_preflight_and_after_await -- --exact`
- Invocation: `desktop/src-tauri/src/commands/agents/runtime_start.rs` → `start_managed_agent_pair_scoped`; exact call `crate::managed_agents::runtime_commands::start_managed_agent_pair_scoped(
            pubkey.to_string(),
            relay.clone(),
            app.clone(),
            &fence,
        ) {`; behavior test `managed_agents::device_runtime_tests::owning_manual_start_refuses_receipt_effects`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::device_runtime_tests::owning_manual_start_refuses_receipt_effects -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace --features mesh-llm`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/commands/agents/provider_deploy.rs`

Serialized synchronous provider invocation under authorized store lock, fresh payload identity, original runtime fence and target-only result writeback

New module: `false`.

- Required symbol: `struct ProviderStartScope`
- Required symbol: `fn deploy_to_provider_scoped`
- Invocation: `desktop/src-tauri/src/commands/agents/provider_deploy.rs` → `provider_phase_with`; exact call `device_runtime::provider_phase_with(
            &invoke_app,
            &state,
            &target,
            Some(&invoke_fence.scope),
            load_device_policy_context,`; behavior test `managed_agents::device_runtime_tests::owning_provider_refuses_deploy_effects`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::device_runtime_tests::owning_provider_refuses_deploy_effects -- --exact`
- Invocation: `desktop/src-tauri/src/commands/agents/provider_deploy.rs` → `provider_phase_with`; exact call `device_runtime::provider_phase_with(
        app,
        state,
        pubkey,
        Some(&fence.scope),
        load_device_policy_context,`; behavior test `managed_agents::device_runtime_tests::owning_provider_refuses_deploy_effects`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::device_runtime_tests::owning_provider_refuses_deploy_effects -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace --features mesh-llm`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/commands/agents/provider_access.rs`

Structural provider reconciliation selection before payload/key effects, mixed private skips without clearing pending retries and scoped metadata-only failures

New module: `false`.

- Required symbol: `fn collect_runtime_targets_with`
- Required symbol: `fn persist_failure_with`
- Invocation: `desktop/src-tauri/src/commands/agents/provider_access.rs` → `collect_runtime_targets_with`; exact call `collect_runtime_targets_with(
            app,
            state,
            owner_only_access,
            crate::managed_agents::persona_device_view::load_device_policy_context,
            |record| super::build_deploy_payload(app, state, record),`; behavior test `commands::agents::provider_access::runtime_tests::provider_access_mixed_batch_preserves_denied_pending_without_keys`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib commands::agents::provider_access::runtime_tests::provider_access_mixed_batch_preserves_denied_pending_without_keys -- --exact`
- Invocation: `desktop/src-tauri/src/commands/agents/provider_access.rs` → `deploy_to_provider_scoped`; exact call `super::provider_deploy::deploy_to_provider_scoped(
            app,
            state,
            &pubkey,
            super::provider_deploy::ProviderStartScope {
                relay: Some(&fence.scope.relay_url),`; behavior test `managed_agents::device_runtime_tests::owning_provider_refuses_deploy_effects`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::device_runtime_tests::owning_provider_refuses_deploy_effects -- --exact`
- Invocation: `desktop/src-tauri/src/commands/agents/provider_access.rs` → `persist_failure_with`; exact call `persist_failure_with(
        app,
        state,
        pubkey,
        error,
        fence,`; behavior test `commands::agents::provider_access::runtime_tests::provider_access_failure_writeback_is_scoped_and_preserves_pending`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib commands::agents::provider_access::runtime_tests::provider_access_failure_writeback_is_scoped_and_preserves_pending -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace --features mesh-llm`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/commands/agents_deploy.rs`

Build fresh authorized provider payload before selected key hydration using captured owner/relay

New module: `false`.

- Required symbol: `fn build_deploy_payload_with`
- Invocation: `desktop/src-tauri/src/commands/agents_deploy.rs` → `provider_phase_with`; exact call `crate::managed_agents::device_runtime::provider_phase_with(
        app,
        state,
        &record.pubkey,
        None,
        context,`; behavior test `commands::agents::deploy::device_payload_tests::provider_redeploy_obeys_home_before_payload_key_lookup`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib commands::agents::deploy::device_payload_tests::provider_redeploy_obeys_home_before_payload_key_lookup -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace --features mesh-llm`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/commands/agent_models_update.rs`

Fence and authorize model update before stop/key/store effects and carry original runtime scope through restarts; Task7: Authorize exact target updates and rename profile publication before instance effects

New module: `false`.

- Required symbol: `fn model_update_phase_with`
- Invocation: `desktop/src-tauri/src/commands/agent_models_update.rs` → `model_update_phase_with`; exact call `model_update_phase_with(
            &app,
            &state,
            &input.pubkey.clone(),
            Some(&runtime_fence),
            crate::managed_agents::persona_device_view::load_device_policy_context,`; behavior test `commands::agent_models::update::device_runtime_guard_tests::model_update_phase_with_refuses_before_process_key_and_store_effects`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib commands::agent_models::update::device_runtime_guard_tests::model_update_phase_with_refuses_before_process_key_and_store_effects -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace --features mesh-llm`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/commands/global_agent_config.rs`

Authorize and fence config-triggered agent restart before target effects and continuation writes

New module: `false`.

- Required symbol: `fn global_restart_phase_with`
- Invocation: `desktop/src-tauri/src/commands/global_agent_config.rs` → `global_restart_phase_with`; exact call `global_restart_phase_with(
            &app_for_stop,
            &state,
            &pubkey_owned,
            Some(&stop_fence),
            crate::managed_agents::persona_device_view::load_device_policy_context,`; behavior test `commands::global_agent_config::device_runtime_guard_tests::global_restart_phase_with_refuses_before_process_key_and_store_effects`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib commands::global_agent_config::device_runtime_guard_tests::global_restart_phase_with_refuses_before_process_key_and_store_effects -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace --features mesh-llm`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/commands/agent_discovery.rs`

Delegate installer-triggered runtime restart to cohesive scoped module

New module: `false`.

- Required symbol: `mod runtime_restart;`
- Invocation: `desktop/src-tauri/src/commands/agent_discovery.rs` → `restart_setup_mode_agents_after_install`; exact call `restart_setup_mode_agents_after_install(&app, &runtime_id, &runtime_fence).await;

    Ok(InstallRuntimeResult {
        success: true,
        steps: install_result.steps,
        restarted_count,`; behavior test `commands::agent_discovery::runtime_restart::device_runtime_guard_tests::discovery_restart_phase_with_refuses_before_process_key_and_store_effects`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib commands::agent_discovery::runtime_restart::device_runtime_guard_tests::discovery_restart_phase_with_refuses_before_process_key_and_store_effects -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace --features mesh-llm`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/commands/agent_discovery/runtime_restart.rs`

Cohesive extraction of installer restart collection/effects with original scope and target authorization

New module: `true`.

- Required symbol: `fn discovery_restart_phase_with`
- Invocation: `desktop/src-tauri/src/commands/agent_discovery/runtime_restart.rs` → `discovery_restart_phase_with`; exact call `discovery_restart_phase_with(
            &app_for_stop,
            &state,
            &pubkey_owned,
            Some(&stop_fence),
            crate::managed_agents::persona_device_view::load_device_policy_context,`; behavior test `commands::agent_discovery::runtime_restart::device_runtime_guard_tests::discovery_restart_phase_with_refuses_before_process_key_and_store_effects`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib commands::agent_discovery::runtime_restart::device_runtime_guard_tests::discovery_restart_phase_with_refuses_before_process_key_and_store_effects -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace --features mesh-llm`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/commands/agent_models.rs`

Task7: Use guarded profile publication adapter for model updates

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/agents_pending.rs`

Task7: Authorize native instance head before signing and require pre-removal deletion authority for kind5/archive enqueue in captured scope; Task8 durable original-scope home deletion/label publication and retry

New module: `false`.

- Invocation: `desktop/src-tauri/src/commands/agents_pending.rs` → `instance_phase_locked_with`; exact call `crate::managed_agents::device_authority::instance_phase_locked_with(
        app,
        state,
        &record.pubkey,
        None,
        crate::managed_agents::device_authority::InstanceAuthorityAction::PublishHead,`; behavior test `copied_common_head_and_deletion_preparation_have_zero_kind0_30177_5_9035_effects`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib copied_common_head_and_deletion_preparation_have_zero_kind0_30177_5_9035_effects`
- Invocation: `desktop/src-tauri/src/commands/agents_pending.rs` → `validate_deletion_authority`; exact call `validate_deletion_authority(state, permit, InstanceAuthorityAction::Tombstone)?;`; behavior test `prepared_shared_deletion_can_enqueue_after_removal_but_never_in_changed_scope`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib prepared_shared_deletion_can_enqueue_after_removal_but_never_in_changed_scope`
- Invocation: `desktop/src-tauri/src/commands/agents_pending.rs` → `complete_home_delete_locked`; exact call `crate::managed_agents::device_home_operations::delete::complete_home_delete_locked(`; behavior test `prepared_shared_deletion_can_enqueue_after_removal_but_never_in_changed_scope`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace prepared_shared_deletion_can_enqueue_after_removal_but_never_in_changed_scope`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/agents_profile.rs`

Task7: Guard common profile and startup publication before key/media/signing; retain captured original operation scope and canonical exact target

New module: `false`.

- Invocation: `desktop/src-tauri/src/commands/agents_profile.rs` → `original_publication_with`; exact call `crate::managed_agents::device_authority::original_publication_with(
        app,
        state,
        pubkey,
        fence,`; behavior test `create_import_profile_invocation_never_redirects_after_owner_relay_switch`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib create_import_profile_invocation_never_redirects_after_owner_relay_switch`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/personas/update.rs`

Task7: Allow definition edit while skipping unauthorized linked instance rename, key resolution, head and profile publication

New module: `false`.

- Invocation: `desktop/src-tauri/src/commands/personas/update.rs` → `authorize_instance_authority`; exact call `crate::managed_agents::device_authority::authorize_instance_authority(
                                record,`; behavior test `remote_definition_rename_does_not_publish_copied_instance`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib remote_definition_rename_does_not_publish_copied_instance`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/egress_guard_tests.rs`

Task7: Maintain exact events URL and egress guard inventory for extracted snapshot publication adapter

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/teams.rs`

Task7: Authorize all requested definition removals before team cascade effects and avoid unrelated secret hydration

New module: `false`.

- Invocation: `desktop/src-tauri/src/managed_agents/teams.rs` → `authorize_definition_deletion`; exact call `super::device_authority::authorize_definition_deletion(definition, &raw, &c)?;`; behavior test `remote_definition_team_cascade_refuses_before_any_store_or_directory_effect`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib remote_definition_team_cascade_refuses_before_any_store_or_directory_effect`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/agents/snapshot_publication.rs`

Task7: Common original-operation snapshot memory authority, plaintext egress guard before encryption, captured owner/relay signing and dispatch

New module: `true`.

- Required symbol: `fn publish_snapshot_memory_entry_with`
- Invocation: `desktop/src-tauri/src/commands/agents/snapshot_publication.rs` → `original_publication_with`; exact call `original_publication_with(
        app,
        state,
        pubkey,
        fence,`; behavior test `original_snapshot_memory_invocation_never_signs_or_sends_into_switched_scope`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib original_snapshot_memory_invocation_never_signs_or_sends_into_switched_scope`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/agents_profile/device_publication_tests.rs`

Task7: Isolated native device authoring, metadata or original publication regression tests

New module: `true`.

- Required symbol: `create_import_profile_invocation_never_redirects_after_owner_relay_switch`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/personas/inbound/device_metadata_tests.rs`

Task7: Isolated native device authoring, metadata or original publication regression tests

New module: `true`.

- Required symbol: `accepted_release_updates_remote_card`
- Required symbol: `stale_tombstone_cannot_remove_newer_home_and_valid_owner_removal_is_receiver_only`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/personas/update/device_edit_tests.rs`

Task7: Isolated native device authoring, metadata or original publication regression tests

New module: `true`.

- Required symbol: `remote_definition_rename_does_not_publish_copied_instance`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/device_authority.rs`

Task7: Exact canonical instance and atomic definition cascade authority; captured deletion permit and original publication adapter; selected raw deletion persistence

New module: `true`.

- Required symbol: `fn authorize_instance_authority`
- Required symbol: `fn original_publication_with`
- Required symbol: `struct DeletionAuthority`
- Required symbol: `fn selected_deletion_records_with`
- Required symbol: `fn save_deletion_snapshot`
- Invocation: `desktop/src-tauri/src/managed_agents/device_authority.rs` → `authorize_instance_authority`; exact call `authorize_instance_authority(&record, definition, &c, action)?;`; behavior test `direct_update_or_delete_copied_instance_has_no_side_effects`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib direct_update_or_delete_copied_instance_has_no_side_effects`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/device_authority_tests.rs`

Task7: Isolated native device authoring, metadata or original publication regression tests

New module: `true`.

- Required symbol: `allowed_shared_deletion_housekeeping_preserves_copied_siblings_and_canonical_definition`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/device_inbound.rs`

Task7: Optional inbound metadata merge preserves old-writer fields and proven local home lineage while accepting explicit sharing

New module: `true`.

- Required symbol: `fn merge_inbound_device_metadata`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/device_inbound_tests.rs`

Task7: Isolated native device authoring, metadata or original publication regression tests

New module: `true`.

- Required symbol: `merge_inbound_device_metadata`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/agents_delete.rs`

Task8: durable secret-free device home deletion, label retries and owning production-adapter coverage

New module: `true`.

- Invocation: `desktop/src-tauri/src/commands/agents.rs` → `commit_prepared_agent_delete_with`; exact call `commit_prepared_agent_delete_with(`; behavior test `prepared_last_instance_command_queues_release_and_stop_error_preserves_record`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace prepared_last_instance_command_queues_release_and_stop_error_preserves_record`
- Invocation: `desktop/src-tauri/src/commands/agents_delete.rs` → `commit_home_delete_snapshot_locked`; exact call `crate::managed_agents::device_home_operations::delete::commit_home_delete_snapshot_locked(`; behavior test `prepared_private_command_failure_replays_original_scope_and_preserves_copied_sibling`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace prepared_private_command_failure_replays_original_scope_and_preserves_copied_sibling`
- Invocation: `desktop/src-tauri/src/commands/agents_delete.rs` → `tombstone_managed_agent_pending`; exact call `tombstone_managed_agent_pending(app, state, permit, &replay)`; behavior test `prepared_private_command_failure_replays_original_scope_and_preserves_copied_sibling`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace prepared_private_command_failure_replays_original_scope_and_preserves_copied_sibling`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/commands/agents_delete/tests.rs`

Task8: durable secret-free device home deletion, label retries and owning production-adapter coverage

New module: `true`.

- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/commands/personas/home_delete_tests.rs`

Task8: durable secret-free device home deletion, label retries and owning production-adapter coverage

New module: `true`.

- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/managed_agents/device_home_operations/delete.rs`

Task8: durable secret-free device home deletion, label retries and owning production-adapter coverage

New module: `true`.

- Invocation: `desktop/src-tauri/src/commands/agents.rs` → `prepare_home_delete_locked`; exact call `crate::managed_agents::device_home_operations::delete::prepare_home_delete_locked(`; behavior test `copied_instance_never_creates_home_operation`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace copied_instance_never_creates_home_operation`
- Invocation: `desktop/src-tauri/src/commands/personas/mod.rs` → `prepare_home_delete_authorized_locked`; exact call `crate::managed_agents::device_home_operations::delete::prepare_home_delete_authorized_locked(`; behavior test `definition_cascade_retry_keeps_instance_tombstone_archive_without_release_resurrection`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace definition_cascade_retry_keeps_instance_tombstone_archive_without_release_resurrection`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/managed_agents/device_home_operations/label.rs`

Task8: durable secret-free device home deletion, label retries and owning production-adapter coverage

New module: `true`.

- Invocation: `desktop/src-tauri/src/commands/device_identity.rs` → `set_device_label_locked`; exact call `crate::managed_agents::device_home_operations::label::set_device_label_locked(`; behavior test `native_label_adapter_reports_queued_until_existing_event_sync_acknowledges`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace native_label_adapter_reports_queued_until_existing_event_sync_acknowledges`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/managed_agents/device_home_operations/durable_tests.rs`

Task8: durable secret-free device home deletion, label retries and owning production-adapter coverage

New module: `true`.

- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/managed_agents/retention/known_scopes.rs`

Task8: durable secret-free device home deletion, label retries and owning production-adapter coverage

New module: `true`.

- Invocation: `desktop/src-tauri/src/commands/workspace.rs` → `remember_retention_scope`; exact call `crate::managed_agents::retention::remember_retention_scope(`; behavior test `premetadata_label_worklist_resolves_when_original_database_becomes_known`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace premetadata_label_worklist_resolves_when_original_database_becomes_known`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/managed_agents/retention.rs`

Task8: durable secret-free device home deletion, label retries and owning production-adapter coverage; Task8 durable original-scope home deletion/label publication and retry

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `just desktop-tauri-clippy`

## `desktop/src/shared/api/deviceTypes.ts`

Authoritative definition home/capabilities, public device metadata, honest label publication and backend sync result types

New module: `true`.

- Required symbol: `export type DefinitionHome`
- Required symbol: `export type DeviceLabelResult`
- Required symbol: `export type DeviceHomeHistoryResult`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/shared/api/personaTypes.ts`

Optional backend policy/origin and nullable/absent computed projection; creation-only false-default device sharing

New module: `false`.

- Required symbol: `shareAcrossDevices?: boolean`
- Required symbol: `home?: DefinitionHome | null`
- Required symbol: `capabilities?: DefinitionCapabilities`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/shared/api/types.ts`

Expose authoritative device and definition capability types through existing shared API imports

New module: `false`.

- Required symbol: `} from "./deviceTypes";`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/shared/api/tauriPersonas.test.mjs`

Actual list/create/edit/sync native IPC coverage for policy default, immutable metadata and computed projection

New module: `false`.

- Required symbol: `raw_home_maps_pubkeys_and_capabilities`
- Required symbol: `create_payload_default_false`
- Required symbol: `edit_payload_does_not_change_policy`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/shared/api/tauriDevice.ts`

Typed device IPC maps snake_case identity and preserves queued/complete publication and backend failures

New module: `true`.

- Required symbol: `export async function getDeviceIdentity`
- Required symbol: `export async function setDeviceLabel`
- Invocation: `desktop/src/shared/api/tauriDevice.ts` → `fromRawDeviceIdentity`; exact call `identity: fromRawDeviceIdentity(result.identity),`; behavior test `device IPC maps snake_case identity and preserves queued or complete publication`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/shared/api/tauriDevice.test.mjs`
- Invocation: `desktop/src/shared/api/tauriDevice.ts` → `invokeTauri`; exact call `const result = await invokeTauri<{
    identity: RawDeviceIdentity;
    publication: DeviceLabelResult["publication"];
  }>("set_device_label", { label });`; behavior test `device IPC propagates validation and durable-publication failures`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/shared/api/tauriDevice.test.mjs`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/shared/api/tauriDevice.test.mjs`

Actual device invoke payload, queued/complete response mapping and propagated validation/enqueue failure coverage

New module: `true`.

- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/lib/definitionCapabilities.ts`

Fresh backend list/action getter and fail-closed required projection; retain exact refusal code and reported label

New module: `true`.

- Required symbol: `export class DefinitionCapabilityError`
- Required symbol: `export function requireDefinitionCapability`
- Required symbol: `export async function getDefinitionForAction`
- Invocation: `desktop/src/features/agents/lib/definitionCapabilities.ts` → `listPersonas`; exact call `(await listPersonas()).find((item) => item.id === id)`; behavior test `getter re-reads each action and propagates missing definition or failed list`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/agents/lib/definitionCapabilities.test.mjs`
- Invocation: `desktop/src/features/agents/lib/definitionCapabilities.ts` → `requireDefinitionCapability`; exact call `requireDefinitionCapability(persona, action);`; behavior test `getter refresh after raw create observes backend refusal before any mutation`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/agents/lib/definitionCapabilities.test.mjs`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/lib/definitionCapabilities.test.mjs`

Owning real getter/list IPC tests for missing/stale/raw projection, exact ID, action-specific refusal and shared backend fast path

New module: `true`.

- Required symbol: `missing_projection_never_authorizes_start`
- Required symbol: `getter refresh after raw create observes backend refusal`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/useKnownAgentPubkeys.test.mjs`

Bounded React/query render settlement preserves all three exact-key provenance and failed-cached-read assertions

New module: `false`.

- Required symbol: `waitFor(() => assert.deepEqual(result.current, [false, true, true]))`
- Required symbol: `waitFor(() => assert.deepEqual(result.current, [false, false, false]))`
- Verify: `just desktop-test desktop-typecheck`
