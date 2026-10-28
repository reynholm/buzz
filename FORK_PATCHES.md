# Fork patches

Generated from `scripts/fork/patches.json`; regenerate with
`python3 scripts/fork/sync.py render`.

Base snapshot: `upstream-main` (`2d289f720de0012fa36b5f6faa0e3bdd845ccfcf`). Release floor: `desktop-v0.5.28`.

## `docs/superpowers/plans/2026-10-03-buzz-fork-maintenance.md`

Approved fork maintenance implementation plan

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`

## `docs/superpowers/plans/2026-10-03-device-bound-agents.md`

Approved device ownership implementation plan

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`

## `FORK_PATCHES.md`

Reviewable generated patch inventory

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`

## `scripts/fork/sync.py`

Validate protected patches and prepare idempotent, baseline-guarded release merges and clean blocked reports

New module: `true`.

- Required symbol: `def validate_patches(`
- Required symbol: `def main(`
- Required symbol: `def prepare_update(`
- Required symbol: `def prepare_blocked_report(`
- Required symbol: `def select_update(`
- Required symbol: `def run_clean_target(`
- Required symbol: `def draft_body(`
- Invocation: `scripts/fork/sync.py` → `validate_patches`; exact call `errors = validate_patches(args.repo, manifest)`; behavior test `test_cli_rejects_unlisted_path`; verify `python3 -m unittest discover -s scripts/fork/tests -v`
- Invocation: `scripts/fork/sync.py` → `validate_patches`; exact call `report.missing_seams = validate_patches(worktree, candidate_manifest)`; behavior test `test_merge_success_missing_seam_blocks`; verify `python3 -m unittest discover -s scripts/fork/tests -v`
- Invocation: `scripts/fork/sync.py` → `prepare_update`; exact call `report = prepare_update(args.repo, selection, manifest, base=args.base, clean_target=baseline)`; behavior test `test_cli_prepare_runs_guarded_candidate`; verify `python3 -m unittest discover -s scripts/fork/tests -v`
- Invocation: `scripts/fork/sync.py` → `merge_target`; exact call `merge = merge_target(worktree, report.target_sha)`; behavior test `test_clean_merge_uses_target_registry_and_preserves_patch`; verify `python3 -m unittest discover -s scripts/fork/tests -v`
- Invocation: `scripts/fork/sync.py` → `draft_body`; exact call `body.write(draft_body(report))`; behavior test `test_draft_body_contains_handoff_requirements`; verify `python3 -m unittest discover -s scripts/fork/tests -v`
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

Fork maintenance runbook, all-registry falsification scope and honest local-versus-external acceptance status

New module: `true`.

- Required symbol: `## Linux x86_64 release assets`
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
                state,
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
            state,
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
        state,
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

Keep discovery test topology and device-compatible fixtures; use immutable executable version-probe fixtures with bounded timeout and descendant cleanup.

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
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib device_workflow_tests`

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
- Required symbol: `pub can_start_on_device: bool,`
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

Safe hostname default; patched ZIP 9 extraction, maintained Opus backend, and immutable compatible MeshLLM and NetEq sources shared across consumers.

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/Cargo.lock`

Lock patched native dependencies, including emSign TLS verification and trust stores, the immutable MeshLLM and NetEq compatibility pins, maintained Opus bindings and updated Tauri utilities; retain target-specific Windows updater constraints explicitly; include ctutils >= 0.4.4 for RUSTSEC-2026-0385.

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `cargo deny --manifest-path desktop/src-tauri/Cargo.toml --target aarch64-apple-darwin --features mesh-llm check`

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
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib device_workflow_tests`

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
- Required symbol: `.build(desktop_context())`
- Required symbol: `build_identity::get_fork_build_identity,`
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
- Required symbol: `struct ForkBuildIdentity`
- Required symbol: `fn get_fork_build_identity()`
- Required symbol: `pub fn print_fork_artifact_probe_if_requested()`
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

Native IPC adapters, captured-key history transport and reset-before-open/error-aware finalization; Async IPC preserves external unit result, serializes finish with workspace apply, migrates before Ready and retries fresh restore against returned verified scope; top-level Wry entry wiring is compile coverage with native acceptance outstanding; fetch verified captured-relay archive off locks, fence scope/session, and pass archive to legacy migration before Ready

New module: `true`.

- Required symbol: `fn begin_device_home_sync`
- Required symbol: `async fn hydrate_device_home_history`
- Required symbol: `fn finish_device_home_sync`
- Required symbol: `fn invalidate_device_home_sync`
- Required symbol: `async fn finish_device_home_sync`
- Required symbol: `async fn retry_device_home_restore_with`
- Required symbol: `async fn finish_device_home_sync_after_archive`
- Required symbol: `fn finish_device_home_sync_inner_with_archive`
- Required symbol: `super::identity_archive::fetch_verified_archived_pubkeys_at(state_ref, &target).await`
- Required symbol: `device_home_sync::capture_scope(state)? != expected`
- Invocation: `desktop/src-tauri/src/commands/device_home_sync.rs` → `reset`; exact call `device_home_sync::reset(state)?;`; behavior test `commands::device_home_sync::tests::begin_retention_error_revokes_previous_readiness`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml commands::device_home_sync::tests::begin_retention_error_revokes_previous_readiness -- --exact`
- Invocation: `desktop/src-tauri/src/commands/device_home_sync.rs` → `finish_session_with`; exact call `device_home_sync::finish_session_with(state, session_token, migrate)?;`; behavior test `commands::device_home_sync::migration_tests::finish_adapter_key_error_latches_failed_and_preserves_store`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml commands::device_home_sync::migration_tests::finish_adapter_key_error_latches_failed_and_preserves_store -- --exact`
- Invocation: `desktop/src-tauri/src/commands/device_home_sync.rs` → `capture_scope`; exact call `device_home_sync::capture_scope(state)? != *expected`; behavior test `commands::device_home_sync::migration_tests::returned_completion_scope_fences_deferred_restore`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml commands::device_home_sync::migration_tests::returned_completion_scope_fences_deferred_restore -- --exact`
- Invocation: `desktop/src-tauri/src/commands/device_home_sync.rs` → `restore_managed_agents_on_launch`; exact call `crate::managed_agents::restore_managed_agents_on_launch(
                &self.app,
                &state.shutdown_started,
                self.admission,
                self.sweeps,
            )`; behavior test `commands::device_home_sync::migration_tests::returned_completion_scope_fences_deferred_restore`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml commands::device_home_sync::migration_tests::returned_completion_scope_fences_deferred_restore -- --exact`
- Invocation: `desktop/src-tauri/src/commands/device_home_sync.rs` → `finish_device_home_sync_deferred_restore_with`; exact call `finish_device_home_sync_deferred_restore_with(
        session_token,
        app,`; behavior test `commands::device_home_sync::deferred_restore_tests::remove_and_readd_during_archive_does_not_revive_deferred_restore`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib commands::device_home_sync::deferred_restore_tests::remove_and_readd_during_archive_does_not_revive_deferred_restore -- --exact`
- Invocation: `desktop/src-tauri/src/commands/device_home_sync.rs` → `finish_device_home_sync_deferred_restore_with`; exact call `finish_device_home_sync_deferred_restore_with(
        session_token,
        app,`; behavior test `commands::device_home_sync::deferred_restore_tests::remove_and_readd_while_restore_waits_for_workspace_lock_does_not_revive_it`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib commands::device_home_sync::deferred_restore_tests::remove_and_readd_while_restore_waits_for_workspace_lock_does_not_revive_it -- --exact`
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
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib device_workflow_tests`

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

Token-scoped live apply leases fence async reconciliation and record failures; Task6: Accept legitimate remote owner heads without local author proof; independently skip copied runtime refresh and secret effects while preserving hydration retry semantics; Task7: Merge optional device metadata only after accepted owner head; preserve proven local lineage; reject wrong author; apply accepted tombstones as receiver operations; R1 stop Desktop-owned runtimes before accepting changed ACL, leave stop-failure head retryable, keep provider pending and guarded spawn

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
- Invocation: `desktop/src-tauri/src/commands/personas/inbound.rs` → `start_local_agent_pairs_scoped`; exact call `super::super::agents::runtime_start::start_local_agent_pairs_scoped(
                &app,
                &state,
                &pubkey,
                &relay_urls,
                Some(&fence),`; behavior test `commands::personas::inbound::access_policy_retry_tests`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace`
- Invocation: `desktop/src-tauri/src/commands/personas/inbound.rs` → `deploy_to_provider_scoped`; exact call `super::super::agents::provider_deploy::deploy_to_provider_scoped(&app, &state, &pubkey,
                super::super::agents::provider_deploy::ProviderStartScope { relay: Some(&fence.scope.relay_url), owner: Some(&fence.scope.owner_pubkey), replay_floor: None, fence: Some(&fence) })
            .await
            .map_err(|error| {
                format!(
                    "Inbound agent access was saved, but its provider deployment failed to refresh with the new policy: {error}"`; behavior test `commands::personas::inbound::access_policy_retry_tests`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace`
- Invocation: `desktop/src-tauri/src/commands/personas/inbound.rs` → `merge_inbound_device_metadata`; exact call `crate::managed_agents::device_inbound::merge_inbound_device_metadata(
                local,
                &mut inbound,
                proven,
            );`; behavior test `commands::personas::inbound::device_metadata_tests`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib commands::personas::inbound::device_metadata_tests`
- Invocation: `desktop/src-tauri/src/commands/personas/inbound.rs` → `stop`; exact call `stop(&app, record, &mut runtimes)?;`; behavior test `commands::personas::inbound::access_policy_retry_tests`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib device_workflow_tests`

## `desktop/src/features/agents/lib/usePersonaSync.ts`

Live-first backend hydration, rejection-aware drain, bounded fresh-session retry and connection invalidation; R2 confirmed live deadline, degraded delivery and bounded recovery with late-EOSE hydration; pre-Run begin failure cooldown recovery fenced by generation and disposal

New module: `false`.

- Required symbol: `export function startPersonaSync`
- Required symbol: `hydrateDeviceHomeHistory(sessionRun.token)`
- Required symbol: `subscribeToConnectionState`
- Required symbol: `subscribeToReconnects`
- Required symbol: `sessionRun.controller.signal`
- Required symbol: `liveConfirmed && !degraded && !failed`
- Required symbol: `const queueRestart`
- Required symbol: `clearTimeout(restartDelay.timer);`
- Required symbol: `const cancelBeginRecovery`
- Required symbol: `epoch === generation &&`
- Invocation: `desktop/src/features/agents/lib/usePersonaSync.ts` → `finishDeviceHomeSync`; exact call `await finishDeviceHomeSync(sessionRun.token);`; behavior test `backend sync waits for buffered live applies before finish and carries its token`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test --test-name-pattern="backend sync waits for buffered" src/features/agents/lib/usePersonaSync.test.mjs`
- Invocation: `desktop/src/features/agents/lib/usePersonaSync.ts` → `invalidateDeviceHomeSync`; exact call `invalidateDeviceHomeSync(run.token)`; behavior test `connection loss invalidates readiness and reconnect starts a fresh complete session`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test --test-name-pattern="connection loss invalidates" src/features/agents/lib/usePersonaSync.test.mjs`
- Invocation: `desktop/src/features/agents/lib/usePersonaSync.ts` → `abort`; exact call `run.controller.abort();`; behavior test `terminal CLOSED after Ready immediately invalidates and replacement exhaustively hydrates`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test --test-name-pattern="terminal CLOSED after Ready" src/features/agents/lib/usePersonaSyncRelayHealth.test.mjs`
- Invocation: `desktop/src/features/agents/lib/usePersonaSync.ts` → `cancelBeginRecovery`; exact call `cancelBeginRecovery();`; behavior test `reconnect supersedes a begin recovery timer without a duplicate subscription`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test --test-name-pattern="reconnect supersedes a begin recovery" src/features/agents/lib/usePersonaSyncRelayHealth.test.mjs`
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

Actual hook plus real RelayClient CLOSED/EOSE/timeout recovery and immediate timer disposal tests; pre-Run begin failure recovery, bounded retries, Ready barrier and generation/disposal cancellation

New module: `true`.

- Required symbol: `terminal CLOSED before confirmation`
- Required symbol: `terminal CLOSED after Ready`
- Required symbol: `unconfirmed timeout`
- Required symbol: `disposal after retryable CLOSED`
- Required symbol: `rate-limited CLOSED retires readiness`
- Required symbol: `cooldown begin failure recovers`
- Required symbol: `repeated begin failures have bounded frequency`
- Required symbol: `late begin rejection from a superseded generation`
- Required symbol: `retries completion without a WebSocket reconnect`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src-tauri/src/managed_agents/device_home_migration.rs`

Atomic legacy binding/origin snapshot, read-only key verification, prewrite scope fence, shared bypass and durable signed-head retry; Task7: Delegate common publication authority to canonical exact target checks; verified archived duplicates do not block local key/proof claim; active/copy/unavailable evidence still blocks

New module: `true`.

- Required symbol: `fn migrate_device_homes_locked`
- Required symbol: `fn migrate_device_homes_in_dir`
- Required symbol: `fn may_publish_local_instance`
- Required symbol: `fn publication_allowed`
- Required symbol: `fn needs_private_authority`
- Required symbol: `fn migrate_device_homes_in_dir_with_archive`
- Required symbol: `fn migrate_device_homes_locked_with_archive`
- Required symbol: `!archived.contains(&e.pubkey)`
- Required symbol: `archived.contains(&records[i].pubkey)`
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
        (app, Some(restore_fence)),
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
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib device_workflow_tests`

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
        scope.expected.map(|e| &e.scope),
        super::persona_device_view::load_device_policy_context,`; behavior test `managed_agents::device_runtime_tests::owning_manual_start_refuses_receipt_effects`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::device_runtime_tests::owning_manual_start_refuses_receipt_effects -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/runtime_commands.rs` → `probe_auto_start_job_with`; exact call `probe_auto_start_job_with(
            &app,
            RuntimeProbeInput {
                record,
                requested,
                fence: &runtime_fence,`; behavior test `managed_agents::runtime_commands::reconcile_callback_tests::restore_and_reconcile_skip_foreign_pairs_before_probe`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::runtime_commands::reconcile_callback_tests::restore_and_reconcile_skip_foreign_pairs_before_probe -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/runtime_commands.rs` → `start_pair`; exact call `start_pair(pubkey, relay_url, true, None, admission, app)
}

pub(crate) fn start_managed_agent_pair_scoped<`; behavior test `managed_agents::device_runtime_tests::shared_lifecycle_matches_baseline`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::device_runtime_tests::shared_lifecycle_matches_baseline -- --exact`
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
- Required symbol: `yaml_serde::from_str::<yaml_serde::Value>(frontmatter).ok()?`
- Invocation: `desktop/src-tauri/src/migration.rs` → `from_str`; exact call `yaml_serde::from_str::<yaml_serde::Value>(frontmatter).ok()?`; behavior test `migration::yaml_tests::runtime_rewrite_preserves_unknown_yaml_values_and_markdown`; verify `cargo test --locked --manifest-path desktop/src-tauri/Cargo.toml --lib migration::yaml_tests`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `cargo test --locked --manifest-path desktop/src-tauri/Cargo.toml --lib migration::yaml_tests`

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
            state,
            requested_persona_id.as_deref(),
            Some(&create_scope),
            crate::managed_agents::persona_device_view::load_device_policy_context,
            |_| {`; behavior test `managed_agents::device_creation::tests::remote_create_has_no_mint_publish_or_save`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace managed_agents::device_creation::tests::remote_create_has_no_mint_publish_or_save -- --exact`
- Invocation: `desktop/src-tauri/src/commands/agents.rs` → `creation_phase_locked`; exact call `crate::managed_agents::device_creation::creation_phase_locked(
            &app,
            state,
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

Device creation policy, default-private sources, atomic claim and durable recovery regression coverage; Task8 durable original-scope home deletion/label publication and retry; Preserve canonical persona coordinates for builtin claims and durable journal recovery

New module: `true`.

- Required symbol: `commit_home_claim_locked`
- Required symbol: `save_journal`
- Required symbol: `enqueue_home_events`
- Required symbol: `recover_in_dir`
- Required symbol: `load_existing_host_proof`
- Required symbol: `recover_home_operations_locked`
- Required symbol: `commit_new_pairs_locked`
- Required symbol: `superseded_created_at`
- Invocation: `desktop/src-tauri/src/commands/agents.rs` → `commit_home_claim_locked`; exact call `crate::managed_agents::device_home_operations::commit_home_claim_locked(`; behavior test `managed_agents::device_home_operations::tests::released_definition_is_claimed_without_restart`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace managed_agents::device_home_operations::tests::released_definition_is_claimed_without_restart -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_operations.rs` → `save_journal`; exact call `save_journal(dir, &operations)?;
    save(&raw)?;`; behavior test `managed_agents::device_home_operations::tests::failed_claim_save_never_publishes_uncommitted_intent`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace managed_agents::device_home_operations::tests::failed_claim_save_never_publishes_uncommitted_intent -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_operations.rs` → `enqueue_home_events`; exact call `enqueue_home_events(dir, &mut conn, operation)`; behavior test `managed_agents::device_home_operations::tests::claim_transaction_failure_retains_intent_and_never_half_enqueues`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace managed_agents::device_home_operations::tests::claim_transaction_failure_retains_intent_and_never_half_enqueues -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_operations.rs` → `recover_in_dir`; exact call `recover_in_dir(&dir, verify_binding, |operation| {`; behavior test `commands::workspace::device_home_preparation_tests::recovery_precedes_migration_and_replays_original_scope_after_switch`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace commands::workspace::device_home_preparation_tests::recovery_precedes_migration_and_replays_original_scope_after_switch -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_operations.rs` → `load_existing_host_proof`; exact call `crate::device_identity::load_existing_host_proof(`; behavior test `managed_agents::device_home_operations::tests::copied_or_unavailable_proof_never_recovers_claim_and_preserves_intent_bytes`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace managed_agents::device_home_operations::tests::copied_or_unavailable_proof_never_recovers_claim_and_preserves_intent_bytes -- --exact`
- Invocation: `desktop/src-tauri/src/commands/workspace.rs` → `recover_home_operations_locked`; exact call `crate::managed_agents::device_home_operations::recover_home_operations_locked(app)`; behavior test `commands::workspace::device_home_preparation_tests::recovery_precedes_migration_and_replays_original_scope_after_switch`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace commands::workspace::device_home_preparation_tests::recovery_precedes_migration_and_replays_original_scope_after_switch -- --exact`
- Invocation: `desktop/src-tauri/src/commands/personas/snapshot/import.rs` → `commit_new_pairs_locked`; exact call `crate::managed_agents::device_home_operations::commit_new_pairs_locked(`; behavior test `managed_agents::device_home_operations::tests::new_import_pair_preserves_foreign_records_and_persists_only_new_keys`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace managed_agents::device_home_operations::tests::new_import_pair_preserves_foreign_records_and_persists_only_new_keys -- --exact`
- Invocation: `desktop/src-tauri/src/commands/team_snapshot.rs` → `commit_new_pairs_locked`; exact call `crate::managed_agents::device_home_operations::commit_new_pairs_locked(`; behavior test `managed_agents::device_home_operations::tests::new_import_pair_preserves_foreign_records_and_persists_only_new_keys`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace managed_agents::device_home_operations::tests::new_import_pair_preserves_foreign_records_and_persists_only_new_keys -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_operations.rs` → `enqueue_home_events_in_transaction`; exact call `enqueue_home_events_in_transaction(&tx, operation, &definitions)?;`; behavior test `intent_save_and_each_sql_enqueue_failure_leave_no_partial_batch`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace intent_save_and_each_sql_enqueue_failure_leave_no_partial_batch`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_operations.rs` → `apply_delete`; exact call `delete::apply_delete(&mut raw, op, Some(&mut verify_binding))?`; behavior test `new_instance_cancels_obsolete_release`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace new_instance_cancels_obsolete_release`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_operations.rs` → `prepare_label_retries`; exact call `label::prepare_label_retries(`; behavior test `premetadata_label_worklist_resolves_when_original_database_becomes_known`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace premetadata_label_worklist_resolves_when_original_database_becomes_known`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_operations.rs` → `coalesce_label_worklists`; exact call `label::coalesce_label_worklists(&mut operations, label)?;`; behavior test `managed_agents::device_home_operations::label_tests`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_operations.rs` → `persona_d_tag`; exact call `.map(|d| super::persona_events::persona_d_tag(&d))`; behavior test `builtin_and_source_slug_claims_enqueue_canonical_persona_heads`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib builtin_and_source_slug_claims_enqueue_canonical_persona_heads`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/managed_agents/device_home_operations/tests.rs`

Device creation policy, default-private sources, atomic claim and durable recovery regression coverage; Canonical builtin/source-slug claims, exact signed-byte legacy recovery, and invalid event rejection

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

Authorize fresh exact target before logs, receipt reuse and process spawn; use fresh signer/scope and fail closed persona reads; read-only computed exact-instance can_start_on_device summary with shared/legacy no-authority-read fast path

New module: `false`.

- Required symbol: `fn spawn_agent_child`
- Required symbol: `fn start_managed_agent_process`
- Required symbol: `let policy_context = if needs_context { context() } else { None };`
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
- Invocation: `desktop/src-tauri/src/managed_agents/runtime.rs` → `runtime_start_refusal`; exact call `        super::device_runtime::runtime_start_refusal(record, personas, policy_context.as_ref())
            .is_none();`; behavior test `managed_agents::device_runtime_tests::summary_reports_exact_instance_capability_without_spawning_or_persisting`; verify `CARGO_INCREMENTAL=0 cargo test --manifest-path desktop/src-tauri/Cargo.toml summary_reports_exact_instance_capability --lib`
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
- Invocation: `desktop/src-tauri/src/commands/agents/runtime_start.rs` → `start_local_agent_after_preflight`; exact call `start_local_agent_after_preflight(app, state, pubkey, requested, |model| async move {
        ensure_relay_mesh_for_record(app, model.as_deref(), fresh).await
    })`; behavior test `commands::agents::admission_tests::ordinary_start_removed_and_readded_during_preflight_is_refused`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib commands::agents::admission_tests::ordinary_start_removed_and_readded_during_preflight_is_refused -- --exact`
- Invocation: `desktop/src-tauri/src/commands/agents/runtime_start.rs` → `start_managed_agent_pair_scoped`; exact call `crate::managed_agents::runtime_commands::start_managed_agent_pair_scoped(
            pubkey.to_string(),
            relay.clone(),
            admission,
            app.clone(),
            &fence,
        ) {`; behavior test `managed_agents::device_runtime_tests::owning_manual_start_refuses_receipt_effects`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::device_runtime_tests::owning_manual_start_refuses_receipt_effects -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace --features mesh-llm`
- Verify: `just desktop-tauri-clippy`

## `desktop/src-tauri/src/commands/agents/provider_deploy.rs`

Serialized synchronous provider invocation under authorized store lock, fresh payload identity, original runtime fence and target-only result writeback; R3 external deploy outside global store lock, exact incarnation/effective configuration and scope writeback fence

New module: `false`.

- Required symbol: `struct ProviderStartScope`
- Required symbol: `fn deploy_to_provider_scoped`
- Invocation: `desktop/src-tauri/src/commands/agents/provider_deploy.rs` → `provider_phase_with`; exact call `device_runtime::provider_phase_with(
            app,
            state,
            pubkey,
            Some(&fence.scope),
            &mut context,`; behavior test `commands::agents::provider_deploy`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace`
- Invocation: `desktop/src-tauri/src/commands/agents/provider_deploy.rs` → `provider_phase_with`; exact call `device_runtime::provider_phase_with(
        app,
        state,
        pubkey,
        Some(&fence.scope),
        context,`; behavior test `commands::agents::provider_deploy`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace`
- Invocation: `desktop/src-tauri/src/commands/agents/provider_deploy.rs` → `invoke_provider_with`; exact call `invoke_provider_with(
            &invoke_app,
            &state,
            &target,
            &invoke_fence,`; behavior test `delayed_provider_allows_independent_store_action`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace`
- Invocation: `desktop/src-tauri/src/commands/agents/provider_deploy.rs` → `assert_runtime_fence`; exact call `assert_payload_scope(&payload, Some(&scope.relay_url), Some(&scope.owner_pubkey))?;
                device_runtime::assert_runtime_fence(state, fence)?;`; behavior test `workspace_replacement_during_prepare_refuses_external_deploy`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace`
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

Task8: durable secret-free device home deletion, label retries and owning production-adapter coverage; R5 coalesce all outstanding scope label worklists to latest intent with monotonic signed floors; canonical persona deduplication and superseded timestamp floors preserve builtin and source-slug label retries

New module: `true`.

- Required symbol: `fn coalesce_label_worklists`
- Invocation: `desktop/src-tauri/src/commands/device_identity.rs` → `set_device_label_locked`; exact call `crate::managed_agents::device_home_operations::label::set_device_label_locked(`; behavior test `native_label_adapter_reports_queued_until_existing_event_sync_acknowledges`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace native_label_adapter_reports_queued_until_existing_event_sync_acknowledges`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_operations/label.rs` → `coalesce_label_worklists`; exact call `coalesce_label_worklists(&mut ops, label)?;`; behavior test `managed_agents::device_home_operations::label_tests`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_operations/label.rs` → `coalesce_label_worklists`; exact call `coalesce_label_worklists(&mut ops, &label)?;`; behavior test `managed_agents::device_home_operations::label_tests`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_operations/label.rs` → `persona_d_tag`; exact call `.get(&super::super::persona_events::persona_d_tag(&d))`; behavior test `newer_label_is_signed_after_superseded_unenqueued_event`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib newer_label_is_signed_after_superseded_unenqueued_event`
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
- Required symbol: `canStartOnDevice?: boolean;`
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

Fresh backend list/action getter and fail-closed required projection; retain exact refusal code and reported label; strict backend instance reuse permission and retryable hydration explanation

New module: `true`.

- Required symbol: `export class DefinitionCapabilityError`
- Required symbol: `export function requireDefinitionCapability`
- Required symbol: `export async function getDefinitionForAction`
- Required symbol: `return canStartOnDevice === true;`
- Required symbol: ``${code}: Device history is not ready. Wait for synchronization and retry.``
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

## `desktop/src/features/agents/channelAgents.accessPolicy.test.mjs`

Task10 authoritative device capability integration and owning action coverage

New module: `false`.

- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/channelAgents.ts`

Task10 authoritative device capability integration and owning action coverage

New module: `false`.

- Invocation: `desktop/src/features/agents/channelAgents.ts` → `getDefinitionForAction`; exact call `  if (fresh.personaId)
    await getDefinitionForAction(fresh.personaId, "createInstance");`; behavior test `channel capability refuses direct-attach before every mutation (definition_hosted_elsewhere)`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/agents/lib/deviceActionPaths.test.mjs src/features/agents/channelAgents.accessPolicy.test.mjs src/features/agents/lib/instanceInputForDefinition.test.mjs src/features/onboarding/welcomeGuide.test.mjs`
- Invocation: `desktop/src/features/agents/channelAgents.ts` → `canReuseManagedAgentOnDevice`; exact call `  if (!canReuseManagedAgentOnDevice(fresh.canStartOnDevice))
    throw new DefinitionCapabilityError("instance_not_runnable_on_device");`; behavior test `channel_reuse_never_attaches_copied_instance (direct-attach)`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/agents/lib/deviceActionPaths.test.mjs src/features/agents/channelAgents.accessPolicy.test.mjs src/features/agents/lib/instanceInputForDefinition.test.mjs src/features/onboarding/welcomeGuide.test.mjs`
- Invocation: `desktop/src/features/agents/channelAgents.ts` → `listManagedAgents`; exact call `  const fresh = (await listManagedAgents()).find(
    (agent) =>
      normalizePubkey(agent.pubkey) === normalizePubkey(input.agent.pubkey),
  );`; behavior test `a vanished exact instance cannot attach a cached identity`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/agents/lib/deviceActionPaths.test.mjs src/features/agents/channelAgents.accessPolicy.test.mjs src/features/agents/lib/instanceInputForDefinition.test.mjs src/features/onboarding/welcomeGuide.test.mjs`
- Invocation: `desktop/src/features/agents/channelAgents.ts` → `getDefinitionForAction`; exact call `  const definition = input.personaId
    ? await getDefinitionForAction(input.personaId, "createInstance")
    : undefined;`; behavior test `channel capability refuses direct-provision before every mutation (definition_hosted_elsewhere)`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/agents/lib/deviceActionPaths.test.mjs src/features/agents/channelAgents.accessPolicy.test.mjs src/features/agents/lib/instanceInputForDefinition.test.mjs src/features/onboarding/welcomeGuide.test.mjs`
- Invocation: `desktop/src/features/agents/channelAgents.ts` → `listManagedAgents`; exact call `const managedAgents = context ? await listManagedAgents() : undefined;`; behavior test `channel refresh refuses stale allowed instance before access policy (false)`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/agents/lib/deviceActionPaths.test.mjs src/features/agents/channelAgents.accessPolicy.test.mjs src/features/agents/lib/instanceInputForDefinition.test.mjs src/features/onboarding/welcomeGuide.test.mjs`
- Invocation: `desktop/src/features/agents/channelAgents.ts` → `canReuseManagedAgentOnDevice`; exact call `    const reusable = findReusablePersonaAgent(
      managedAgents ?? [],
      input.personaId,
      context.channelMemberPubkeys,
    );
    if (reusable) {
      if (!canReuseManagedAgentOnDevice(reusable.canStartOnDevice))
        throw new DefinitionCapabilityError("instance_not_runnable_on_device");`; behavior test `channel_reuse_never_attaches_copied_instance (reuse)`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/agents/channelAgents.accessPolicy.test.mjs`
- Invocation: `desktop/src/features/agents/channelAgents.ts` → `canReuseManagedAgentOnDevice`; exact call `    const reusable = findReusableGenericAgent(
      managedAgents ?? [],
      input.runtime.command,
      context.channelMemberPubkeys,
    );
    if (reusable) {
      if (!canReuseManagedAgentOnDevice(reusable.canStartOnDevice))
        throw new DefinitionCapabilityError("instance_not_runnable_on_device");`; behavior test `generic reuse also refuses missing exact-instance authority before access write`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/agents/channelAgents.accessPolicy.test.mjs`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/lib/instanceInputForDefinition.test.mjs`

Task10 authoritative device capability integration and owning action coverage

New module: `false`.

- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/lib/instanceInputForDefinition.ts`

Task10 authoritative device capability integration and owning action coverage

New module: `false`.

- Invocation: `desktop/src/features/agents/lib/instanceInputForDefinition.ts` → `requireDefinitionCapability`; exact call `  requireDefinitionCapability(persona, "createInstance");`; behavior test `builder refuses before avatar upload even with provider intent`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/agents/lib/deviceActionPaths.test.mjs src/features/agents/channelAgents.accessPolicy.test.mjs src/features/agents/lib/instanceInputForDefinition.test.mjs src/features/onboarding/welcomeGuide.test.mjs`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/ui/useManagedAgentActions.ts`

Task10 authoritative device capability integration and owning action coverage

New module: `false`.

- Invocation: `desktop/src/features/agents/ui/useManagedAgentActions.ts` → `getDefinitionForAction`; exact call `      persona = await getDefinitionForAction(persona.id, "createInstance");`; behavior test `managed: all_builder_callers_observe_remote_refusal / pending_history_blocks_every_creation_path (definition_hosted_elsewhere)`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/agents/lib/deviceActionPaths.test.mjs src/features/agents/channelAgents.accessPolicy.test.mjs src/features/agents/lib/instanceInputForDefinition.test.mjs src/features/onboarding/welcomeGuide.test.mjs`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/ui/usePersonaActions.ts`

Task10 authoritative device capability integration and owning action coverage

New module: `false`.

- Invocation: `desktop/src/features/agents/ui/usePersonaActions.ts` → `getDefinitionForAction`; exact call `await getDefinitionForAction(persona.id, "createInstance")`; behavior test `persona: all_builder_callers_observe_remote_refusal / pending_history_blocks_every_creation_path (definition_hosted_elsewhere)`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/agents/lib/deviceActionPaths.test.mjs src/features/agents/channelAgents.accessPolicy.test.mjs src/features/agents/lib/instanceInputForDefinition.test.mjs src/features/onboarding/welcomeGuide.test.mjs`
- Invocation: `desktop/src/features/agents/ui/usePersonaActions.ts` → `getDefinitionForAction`; exact call `      await getDefinitionForAction(persona.id, "deleteDefinition");`; behavior test `persona: definition deletion refuses before team/cascade mutations (definition_hosted_elsewhere)`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/agents/lib/deviceActionPaths.test.mjs src/features/agents/channelAgents.accessPolicy.test.mjs src/features/agents/lib/instanceInputForDefinition.test.mjs src/features/onboarding/welcomeGuide.test.mjs`
- Invocation: `desktop/src/features/agents/ui/usePersonaActions.ts` → `savedDefinitionRecovery`; exact call `savedDefinitionRecovery(savedDefinition.displayName, error)`; behavior test `persona: saved definition refusal closes creation with Start recovery and no duplicate retry`; verify `just desktop-test`
- Invocation: `desktop/src/features/agents/ui/usePersonaActions.ts` → `savedDefinitionRecovery`; exact call `savedDefinitionRecovery(persona.displayName, error)`; behavior test `persona: native refusal after saved definition keeps Start recovery`; verify `just desktop-test`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/ui/useTeamActions.ts`

Task10 authoritative device capability integration and owning action coverage; R6 capability checks only for cascading team definitions

New module: `false`.

- Required symbol: `      actions.setActionErrorMessage(
        result.failures
          .map(({ name, error }) => `${name}: ${error}`)
          .join("\n"),
      );`
- Required symbol: `    if (failures.length) actions.setActionErrorMessage(failures.join("\n"));`
- Invocation: `desktop/src/features/agents/ui/useTeamActions.ts` → `getDefinitionForAction`; exact call `        await getDefinitionForAction(id, "deleteDefinition");`; behavior test `team: definition deletion refuses before team/cascade mutations (definition_hosted_elsewhere)`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/agents/lib/deviceActionPaths.test.mjs src/features/agents/channelAgents.accessPolicy.test.mjs src/features/agents/lib/instanceInputForDefinition.test.mjs src/features/onboarding/welcomeGuide.test.mjs`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/useAgentManagement.ts`

Task10 authoritative device capability integration and owning action coverage

New module: `false`.

- Invocation: `desktop/src/features/agents/useAgentManagement.ts` → `getDefinitionForAction`; exact call `await getDefinitionForAction(persona.id, "createInstance")`; behavior test `management: all_builder_callers_observe_remote_refusal / pending_history_blocks_every_creation_path (definition_hosted_elsewhere)`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/agents/lib/deviceActionPaths.test.mjs src/features/agents/channelAgents.accessPolicy.test.mjs src/features/agents/lib/instanceInputForDefinition.test.mjs src/features/onboarding/welcomeGuide.test.mjs`
- Invocation: `desktop/src/features/agents/useAgentManagement.ts` → `savedDefinitionRecovery`; exact call `savedDefinitionRecovery(savedDefinitionName, cause)`; behavior test `management: saved definition refusal closes creation with Start recovery and no duplicate retry`; verify `just desktop-test`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/onboarding/welcomeGuide.test.mjs`

Task10 authoritative device capability integration and owning action coverage

New module: `false`.

- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/onboarding/welcomeGuide.ts`

Task10 authoritative device capability integration and owning action coverage

New module: `false`.

- Invocation: `desktop/src/features/onboarding/welcomeGuide.ts` → `getDefinitionForAction`; exact call `  persona = await getDefinitionForAction(persona.id, "createInstance");`; behavior test `Welcome builder re-reads its exact definition before producing instance input`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/agents/lib/deviceActionPaths.test.mjs src/features/agents/channelAgents.accessPolicy.test.mjs src/features/agents/lib/instanceInputForDefinition.test.mjs src/features/onboarding/welcomeGuide.test.mjs`
- Invocation: `desktop/src/features/onboarding/welcomeGuide.ts` → `getDefinitionForAction`; exact call `      await getDefinitionForAction(starter.personaId, "createInstance");`; behavior test `onboarding_and_team_skip_with_reason (definition_hosted_elsewhere)`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/agents/lib/deviceActionPaths.test.mjs src/features/agents/channelAgents.accessPolicy.test.mjs src/features/agents/lib/instanceInputForDefinition.test.mjs src/features/onboarding/welcomeGuide.test.mjs`
- Invocation: `desktop/src/features/onboarding/welcomeGuide.ts` → `canReuseManagedAgentOnDevice`; exact call `      if (!canReuseManagedAgentOnDevice(existing.canStartOnDevice))
        throw new DefinitionCapabilityError("instance_not_runnable_on_device");`; behavior test `Welcome refuses copied existing instance before runtime or access repair and membership`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/agents/lib/deviceActionPaths.test.mjs src/features/agents/channelAgents.accessPolicy.test.mjs src/features/agents/lib/instanceInputForDefinition.test.mjs src/features/onboarding/welcomeGuide.test.mjs`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/profile/ui/UserProfilePanel.tsx`

Task10 authoritative device capability integration and owning action coverage; render Start/Delete only from computed backend capabilities

New module: `false`.

- Required symbol: `resolvedPersona.capabilities?.canCreateInstance === true`
- Required symbol: `resolvedPersona.capabilities?.canDeleteDefinition === true`
- Invocation: `desktop/src/features/profile/ui/UserProfilePanel.tsx` → `getDefinitionForAction`; exact call `      personaToStart = await getDefinitionForAction(
        personaToStart.id,
        "createInstance",
      );`; behavior test `profile: all_builder_callers_observe_remote_refusal / pending_history_blocks_every_creation_path (definition_hosted_elsewhere)`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/agents/lib/deviceActionPaths.test.mjs src/features/agents/channelAgents.accessPolicy.test.mjs src/features/agents/lib/instanceInputForDefinition.test.mjs src/features/onboarding/welcomeGuide.test.mjs`
- Invocation: `desktop/src/features/profile/ui/UserProfilePanel.tsx` → `getDefinitionForAction`; exact call `        await getDefinitionForAction(personaToConfirm.id, "deleteDefinition");`; behavior test `profile: definition deletion refuses before team/cascade mutations (definition_hosted_elsewhere)`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/agents/lib/deviceActionPaths.test.mjs src/features/agents/channelAgents.accessPolicy.test.mjs src/features/agents/lib/instanceInputForDefinition.test.mjs src/features/onboarding/welcomeGuide.test.mjs`
- Invocation: `desktop/src/features/profile/ui/UserProfilePanel.tsx` → `getDefinitionForAction`; exact call `        await getDefinitionForAction(resolvedPersona.id, "deleteDefinition");`; behavior test `built-in profile removal checks delete capability before cascade and deactivation`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/agents/lib/deviceActionPaths.test.mjs src/features/agents/channelAgents.accessPolicy.test.mjs src/features/agents/lib/instanceInputForDefinition.test.mjs src/features/onboarding/welcomeGuide.test.mjs`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/shared/api/tauri.ts`

Task10 authoritative device capability integration and owning action coverage

New module: `false`.

- Required symbol: `canStartOnDevice: agent.can_start_on_device,`
- Required symbol: `can_start_on_device?: boolean;`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/lib/deviceActionPaths.test.mjs`

Task10 authoritative device capability integration and owning action coverage

New module: `true`.

- Required symbol: `production builder caller inventory is exact`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/AGENTS.md`

Device sharing creation UI, authoritative home presentation and saved-definition recovery Document scoped session permission, routing, restart warning, original command/result authority and batched actor behavior. Thread-composer agent-first context placement, bounded raw participant roster, disabled-editor availability and captured scope reset.

New module: `false`.

- Required symbol: `## Scoped session controls`
- Verify: `just desktop-test desktop-typecheck`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke session-controls.spec.ts session-status.spec.ts`

## `desktop/src/features/agents/lib/savedDefinitionRecovery.ts`

Device sharing creation UI, authoritative home presentation and saved-definition recovery

New module: `true`.

- Required symbol: `export function savedDefinitionRecovery(`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/ui/AgentDefinitionDialog.tsx`

Device sharing creation UI, authoritative home presentation and saved-definition recovery

New module: `false`.

- Invocation: `desktop/src/features/agents/ui/AgentDefinitionDialog.tsx` → `AgentDeviceSharingField`; exact call `<AgentDeviceSharingField`; behavior test `create_toggle_defaults_off_and_survives_submit`; verify `just desktop-test`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/ui/AgentDeviceSharingField.tsx`

Device sharing creation UI, authoritative home presentation and saved-definition recovery

New module: `true`.

- Required symbol: `export function AgentDeviceSharingField(`
- Required symbol: `checked={value}`
- Required symbol: `onChange(event.target.checked)`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/ui/PersonaActionsMenu.tsx`

Device sharing creation UI, authoritative home presentation and saved-definition recovery

New module: `false`.

- Invocation: `desktop/src/features/agents/ui/PersonaActionsMenu.tsx` → `canDeleteDefinition`; exact call `persona.capabilities?.canDeleteDefinition === true`; behavior test `remote_card_has_no_start_or_delete`; verify `just desktop-test`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/ui/PersonaRemoteRuntime.tsx`

Device sharing creation UI, authoritative home presentation and saved-definition recovery

New module: `true`.

- Required symbol: `export function PersonaRemoteRuntime(`
- Required symbol: `persona.home?.remoteInstancePubkeys`
- Required symbol: `"device_home_sync_pending"`
- Invocation: `desktop/src/features/agents/ui/PersonaRemoteRuntime.tsx` → `useAgentAvailabilityLookup`; exact call `useAgentAvailabilityLookup(pubkeys)`; behavior test `presence_uses_pubkey_not_name`; verify `just desktop-test`
- Invocation: `desktop/src/features/agents/ui/PersonaRemoteRuntime.tsx` → `invalidateQueries`; exact call `client.invalidateQueries({ queryKey: personasQueryKey })`; behavior test `pending_failed_and_missing_projection_hide_actions_but_allow_sync_retry`; verify `just desktop-test`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/ui/RequestedAgentCreateDialogs.tsx`

Device sharing creation UI, authoritative home presentation and saved-definition recovery

New module: `false`.

- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/ui/UnifiedAgentsSection.tsx`

Device sharing creation UI, authoritative home presentation and saved-definition recovery

New module: `false`.

- Invocation: `desktop/src/features/agents/ui/UnifiedAgentsSection.tsx` → `PersonaRemoteRuntime`; exact call `<PersonaRemoteRuntime persona={persona} />`; behavior test `remote_card_has_no_start_or_delete`; verify `just desktop-test`
- Invocation: `desktop/src/features/agents/ui/UnifiedAgentsSection.tsx` → `canCreateInstance`; exact call `persona.capabilities?.canCreateInstance === true`; behavior test `pending_failed_and_missing_projection_hide_actions_but_allow_sync_retry`; verify `just desktop-test`
- Invocation: `desktop/src/features/agents/ui/UnifiedAgentsSection.tsx` → `canStartOnDevice`; exact call `(!agent || agent.canStartOnDevice === true)`; behavior test `copied_local_instance_does_not_restore_start`; verify `just desktop-test`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/ui/UnifiedAgentsSectionCardTarget.test.mjs`

Device sharing creation UI, authoritative home presentation and saved-definition recovery

New module: `false`.

- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/ui/UnifiedAgentsSectionDeviceHome.jsdom-test.mjs`

Device sharing creation UI, authoritative home presentation and saved-definition recovery

New module: `true`.

- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/ui/personaDialogState.test.mjs`

Device sharing creation UI, authoritative home presentation and saved-definition recovery

New module: `false`.

- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/agents/ui/personaDialogState.ts`

Device sharing creation UI, authoritative home presentation and saved-definition recovery

New module: `false`.

- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/settings/ui/AgentsSettingsPanel.tsx`

Device identity settings and honest queued publication UI

New module: `false`.

- Invocation: `desktop/src/features/settings/ui/AgentsSettingsPanel.tsx` → `DeviceIdentitySettingsCard`; exact call `<DeviceIdentitySettingsCard />`; behavior test `label_keyboard_save_reports_queued`; verify `just desktop-test`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/settings/ui/DeviceIdentitySettingsCard.jsdom-test.mjs`

Device identity settings and honest queued publication UI

New module: `true`.

- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/settings/ui/DeviceIdentitySettingsCard.tsx`

Device identity settings and honest queued publication UI

New module: `true`.

- Required symbol: `export function DeviceIdentitySettingsCard(`
- Required symbol: `save.mutate(label.trim())`
- Required symbol: `save.data.publication === "queued"`
- Invocation: `desktop/src/features/settings/ui/DeviceIdentitySettingsCard.tsx` → `getDeviceIdentity`; exact call `queryFn: getDeviceIdentity`; behavior test `identity_read_error_retains_retry_affordance`; verify `just desktop-test`
- Invocation: `desktop/src/features/settings/ui/DeviceIdentitySettingsCard.tsx` → `setDeviceLabel`; exact call `mutationFn: setDeviceLabel`; behavior test `label_keyboard_save_reports_queued`; verify `just desktop-test`
- Verify: `just desktop-test desktop-typecheck`

## `.github/workflows/fork-sync.yml`

Exact clean-upstream baseline dependency, full Python regression and draft-only candidate release workflow

New module: `true`.

- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `scripts/fork/tests/test_sync.py`

Synthetic Git and GH release preparation, conflict reports, recovery and baseline failure regression coverage; R4 isolated fixture Git identity and production identity guard regressions

New module: `true`.

- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `scripts/fork/tests/test_workflow_contract.py`

Actual JSON Actions graph ordering and injected clean-target failure regression coverage

New module: `true`.

- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `.gitignore`

Exact Apple Silicon fork candidate artifact provenance and immutable About identity; private design notes directory stays untracked

New module: `false`.

- Required symbol: `/artifacts/fork/`
- Required symbol: `/docs/local`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `scripts/fork/build-candidate.sh`

Exact Apple Silicon fork candidate artifact provenance and immutable About identity

New module: `true`.

- Required symbol: `assert_source`
- Required symbol: `unset BUZZ_UPDATER_ENDPOINT BUZZ_UPDATER_PUBLIC_KEY BUZZ_BUILD_DEMO_SLUG`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `scripts/fork/verify_artifact.py`

Exact Apple Silicon fork candidate artifact provenance and immutable About identity

New module: `true`.

- Required symbol: `def verify_artifact(`
- Required symbol: `def load_baseline(`
- Required symbol: `def probe_artifact(`
- Required symbol: `def verify_bundle_signature(`
- Invocation: `scripts/fork/verify_artifact.py` → `probe_artifact`; exact call `probe = probe_artifact(binaries / executable_name)`; behavior test `test_identity_matches_candidate`; verify `python3 -m unittest discover -s scripts/fork/tests -v`
- Invocation: `scripts/fork/verify_artifact.py` → `verify_bundle_signature`; exact call `verify_bundle_signature(app)`; behavior test `test_linker_signature_is_rejected_before_native_probe`; verify `python3 -m unittest discover -s scripts/fork/tests -v`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `scripts/fork/tests/test_artifact.py`

Exact Apple Silicon fork candidate artifact provenance and immutable About identity

New module: `true`.

- Required symbol: `test_placeholder_or_wrong_arch_is_rejected`
- Required symbol: `test_bundled_updater_is_rejected`
- Required symbol: `test_identity_matches_candidate`
- Required symbol: `test_actual_build_command_fails_closed_before_artifacts`
- Required symbol: `test_dirty_source_is_rejected_before_build_side_effects`
- Required symbol: `test_invalid_bundle_signature_is_rejected`
- Required symbol: `test_linker_signature_is_rejected_before_native_probe`
- Required symbol: `test_sealed_bundle_passes_and_tampered_resource_is_rejected`
- Required symbol: `test_modified_nested_sidecar_is_rejected`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `desktop/src/shared/api/tauriBuildIdentity.ts`

Exact Apple Silicon fork candidate artifact provenance and immutable About identity

New module: `true`.

- Required symbol: `export async function getForkBuildIdentity()`
- Required symbol: `export function forkBuildLabel(`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/shared/api/tauriBuildIdentity.test.mjs`

Exact Apple Silicon fork candidate artifact provenance and immutable About identity

New module: `true`.

- Required symbol: `About preserves upstream version`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/settings/ui/SettingsViewBuildIdentity.jsdom-test.mjs`

Exact Apple Silicon fork candidate artifact provenance and immutable About identity

New module: `true`.

- Required symbol: `About renders native fork identity beside unchanged upstream version`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/settings/ui/SettingsView.tsx`

Exact Apple Silicon fork candidate artifact provenance and immutable About identity

New module: `false`.

- Required symbol: `data-testid="settings-fork-identity"`
- Invocation: `desktop/src/features/settings/ui/SettingsView.tsx` → `getForkBuildIdentity`; exact call `void getForkBuildIdentity().then((identity) => {`; behavior test `About renders native fork identity beside unchanged upstream version`; verify `just desktop-test desktop-typecheck`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src-tauri/build.rs`

Exact Apple Silicon fork candidate artifact provenance and immutable About identity

New module: `false`.

- Required symbol: `cargo:rerun-if-env-changed={key}`
- Required symbol: `cargo:rustc-env=BUZZ_FORK_SHA={sha}`
- Verify: `just desktop-tauri-test desktop-tauri-check desktop-tauri-clippy desktop-tauri-fmt-check`

## `desktop/src-tauri/src/main.rs`

Exact Apple Silicon fork candidate artifact provenance and immutable About identity

New module: `false`.

- Required symbol: `buzz_lib::print_fork_artifact_probe_if_requested()`
- Invocation: `desktop/src-tauri/src/main.rs` → `print_fork_artifact_probe_if_requested`; exact call `if buzz_lib::print_fork_artifact_probe_if_requested() {`; behavior test `test_probe_uses_early_readonly_command`; verify `just desktop-tauri-test`
- Verify: `just desktop-tauri-test desktop-tauri-check desktop-tauri-clippy desktop-tauri-fmt-check`

## `scripts/fork/promote.py`

Owner-merged exact-tree promotion with immutable artifact/acceptance pins and resumable verified release cleanup

New module: `true`.

- Required symbol: `def verify_promotion(`
- Required symbol: `def publish_promotion(`
- Required symbol: `def verify_owner_merge(`
- Required symbol: `def verify_archive(`
- Required symbol: `from publish_linux import is_linux_asset`
- Invocation: `scripts/fork/promote.py` → `verify_promotion`; exact call `promotion = verify_promotion(repo, candidate_sha, merged_sha, manifest)`; behavior test `test_changed_merge_tree_cannot_promote`; verify `python3 -m unittest discover -s scripts/fork/tests -v`
- Invocation: `scripts/fork/promote.py` → `verify_owner_merge`; exact call `verify_owner_merge(promotion, gh)`; behavior test `test_authoritative_owner_merge_rejection_precedes_all_mutations`; verify `python3 -m unittest discover -s scripts/fork/tests -v`
- Invocation: `scripts/fork/promote.py` → `verify_archive`; exact call `verify_archive(directory / names[0], manifest)`; behavior test `test_rechecks_archived_binary_bytes_even_with_resealed_checksums`; verify `python3 -m unittest discover -s scripts/fork/tests -v`
- Invocation: `scripts/fork/promote.py` → `is_linux_asset`; exact call `existing_names = [item['name'] for item in release['assets'] if not is_linux_asset(item['name'])]`; behavior test `scripts/fork/tests/test_verify_linux.py`; verify `python3 -m unittest discover -s scripts/fork/tests -v`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `scripts/fork/tests/test_promote.py`

Synthetic disposable Git and executable GH fixtures for acceptance/tree/checksum/tag/release/cleanup gates

New module: `true`.

- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `.github/workflows/fork-promote.yml`

Manual owner-dispatched protected-environment promotion of exact tested candidate artifacts

New module: `true`.

- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `desktop/src-tauri/src/managed_agents/device_workflow_tests.rs`

Two-store production-seam device workflows and complete Desktop access projection matrix

New module: `true`.

- Required symbol: `fn private_two_device_single_pubkey(`
- Required symbol: `fn release_then_reclaim(`
- Required symbol: `fn shared_two_device_baseline(`
- Required symbol: `fn device_policy_access_matrix(`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib device_workflow_tests`
- Verify: `python3 scripts/fork/sync.py validate`

## `docs/fork/DEVICE_AGENT_ACCEPTANCE.md`

Exact-candidate owner native/physical acceptance procedure and explicitly pending final handoff gates

New module: `true`.

- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib device_workflow_tests`
- Verify: `python3 scripts/fork/sync.py validate`

## `desktop/src-tauri/src/commands/personas/inbound/access_policy_retry_tests.rs`

R1 ACL proof/stop failure and identical-event retry regressions

New module: `true`.

- Required symbol: `fn effective_access_revocation_stops_owned_child_despite_unavailable_proof`
- Required symbol: `fn effective_access_stop_failure_preserves_old_acl_and_same_head_retry`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace`

## `desktop/src-tauri/src/commands/agents/provider_deploy_tests.rs`

R3 delayed provider, independent store actions, stale target/config/payload/scope regressions

New module: `true`.

- Required symbol: `fn delayed_provider_allows_independent_store_action`
- Required symbol: `fn recreated_same_pubkey_refuses_stale_provider_success_and_failure`
- Required symbol: `fn workspace_replacement_during_prepare_refuses_external_deploy`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace`

## `desktop/src-tauri/src/managed_agents/device_home_operations/label_tests.rs`

R5 A-failed enqueue B-success recovery, cross-scope preservation and timestamp floor regressions

New module: `true`.

- Required symbol: `fn newer_successful_label_survives_older_sql_enqueue_failure_and_recovery`
- Required symbol: `fn newer_label_preserves_other_relay_owner_and_unresolved_scope_worklists`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace`

## `desktop/src/features/profile/ui/UserProfilePanelSections.tsx`

Device agent profile and archived duplicate migration regression protection

New module: `false`.

- Required symbol: `managedAgent.canStartOnDevice === true`
- Required symbol: `<PersonaRemoteRuntime persona={persona} />`
- Verify: `pnpm --dir desktop test`
- Verify: `pnpm --dir desktop typecheck`

## `desktop/src-tauri/src/commands/identity_archive.rs`

Verified relay archive acquisition with total deadline and explicit errors for scoped migration recovery; presentation remains fail-open

New module: `false`.

- Required symbol: `snapshot.kind.as_u16() != 13535`
- Required symbol: `fn fetch_verified_archived_pubkeys_at`
- Required symbol: `tokio::time::timeout(deadline, read_verified_archive_at(state, target))`
- Required symbol: `device_home_archive_timeout`
- Required symbol: `device_home_archive_unavailable`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace`

## `desktop/src-tauri/src/commands/device_home_sync_archive_tests.rs`

Device agent profile and archived duplicate migration regression protection

New module: `true`.

- Required symbol: `archive_transport_failures_and_invalid_kind_never_complete_as_empty_evidence`
- Required symbol: `stalled_archive_headers_or_body_are_bounded_and_recover_same_identity_and_token`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace`

## `.github/scripts/codex-security-review.js`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `.github/scripts/codex-security-review.test.js`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `.github/workflows/_ci-clients.yml`

Pin the Android lane to Temurin JDK 17 for Gradle 8.14.5 and explicitly select the same JAVA_HOME in Flutter before APK and JVM unit builds.

New module: `false`.

- Required symbol: `- name: Set up Android JDK
        if: matrix.lane == 'android'
        uses: actions/setup-java@b6effb05e454b25005698d916606bdc6ffcbf961 # v5
        with:
          distribution: temurin
          java-version: '17'`
- Required symbol: `- name: Select Android JDK for Flutter
        if: matrix.lane == 'android'
        run: flutter config --jdk-dir="$JAVA_HOME"`
- Verify: `node --test scripts/ci-selection.test.mjs scripts/ci-required-gates.test.mjs scripts/ci-apt-retry.test.mjs`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `.github/workflows/_ci-desktop-macos.yml`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `.github/workflows/_ci-relay.yml`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches Build, cache and transfer the CLI executable alongside relay artifacts for managed recovery tests in the PostgreSQL lane. Preserve the MinIO CI image override in every relay integration consumer. Retain the real CLI build and artifact inventory after the upstream producer matrix split.; pin browser E2E shards and their binary producer to Ubuntu 24.04 supported by the locked Playwright release.

New module: `false`.

- Required symbol: `export BUZZ_TEST_BIN_DIR="$PWD/target/ci"`
- Required symbol: `desktop-e2e-integration-shard:
    env:
      COMPOSE_FILE: docker-compose.yml:docker-compose.ci.yml`
- Required symbol: `backend-integration:
    env:
      COMPOSE_FILE: docker-compose.yml:docker-compose.ci.yml`
- Required symbol: `relay-e2e:
    env:
      COMPOSE_FILE: docker-compose.yml:docker-compose.ci.yml`
- Required symbol: `run: cargo build --profile ci -p buzz-relay -p git-credential-nostr -p buzz-cli`
- Required symbol: `- artifact: relay-binaries
            paths: |
              target/ci/buzz-relay
              target/ci/buzz
              target/ci/git-credential-nostr`
- Required symbol: `    runs-on: ubuntu-24.04`
- Required symbol: `desktop-e2e-relay:
    name: Desktop E2E Relay (${{ matrix.artifact }})
    # Match the supported Playwright runner and its binary producer.
    runs-on: ubuntu-24.04`
- Required symbol: `name: Desktop E2E Integration (${{ matrix.shard }}/${{ strategy.job-total }})
    # Match the supported Playwright runner and its binary producer.
    runs-on: ubuntu-24.04`
- Verify: `just ci`

## `.github/workflows/_ci-rust.yml`

Preserve checked ACP retirement and budget cold native Windows compilation plus the full Tauri test suite

New module: `false`.

- Required symbol: `python scripts/check-acp-retirement.py`
- Required symbol: `python3 scripts/check-acp-retirement.py`
- Required symbol: `timeout-minutes: 90`
- Verify: `just ci`

## `.github/workflows/ci.yml`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `.github/workflows/codex-security-review.yml`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `ARCHITECTURE.md`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `CHANGELOG.md`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `Cargo.lock`

Preserve the fork dependency graph with patched rustls, emSign TLS verification and trust stores, Negentropy and ZIP versions and one immutable MeshLLM compatibility source; include ctutils >= 0.4.4 for RUSTSEC-2026-0385. Upgrade the gateway P-256 graph to crypto-bigint 0.7.6 for RUSTSEC-2026-0387.

New module: `false`.

- Verify: `just ci`
- Verify: `cargo deny check`

## `Justfile`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches Select the infra-free demo echo cancellation regression in the required nextest unit lane.

New module: `false`.

- Required symbol: `test(=mesh_boot::tests::demo_echo_keeps_inflight_read_across_ticks_and_still_drains)`
- Verify: `just ci`
- Verify: `just test-unit`

## `bin/.uv-0.12.21.pkg`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `bin/uv`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `bin/uvx`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-acp/README.md`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-acp/src/acp.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches Route standard usage_update occupancy to the status binding of its own session ID; the harness owns BUZZ_SESSION_STATUS_DIR in adapter spawns. Negotiate structured terminal provider failures and propagate them through the existing queue retry and authentication paths. Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `false`.

- Required symbol: `pub async fn session_resume(`
- Required symbol: `self.resume_supported = result`
- Required symbol: `pub(crate) fn bind_session_status(`
- Required symbol: `pub(crate) fn clear_session_status(`
- Required symbol: `DispatchRetired,`
- Required symbol: `"session": {"compaction": {}}`
- Required symbol: `if update_type == "compaction_update"`
- Required symbol: `provider_failure::check_response(result)?;`
- Invocation: `crates/buzz-acp/src/acp.rs` → `record_usage_update`; exact call `crate::session_status::record_usage_update(&self.session_status, msg);`; behavior test `claude_usage_updates_route_by_their_own_session_id`; verify `cargo test -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/acp.rs` → `apply_status_dir_env`; exact call `crate::session_status::apply_status_dir_env(&mut cmd, extra_env);`; behavior test `status_dir_env_is_owned_by_the_harness`; verify `cargo test -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/acp.rs` → `update`; exact call `self.compactions.update(session, update)`; behavior test `compaction_standard_dispatch_terminal_only_duplicate_and_next_request`; verify `cargo test --locked -p buzz-acp --lib compaction_standard_dispatch_terminal_only_duplicate_and_next_request -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/acp.rs` → `compacted`; exact call `binding.compacted(origin);`; behavior test `typed_compaction_changes_only_transition_and_manual_actor`; verify `cargo test --locked -p buzz-acp --lib typed_compaction_changes_only_transition_and_manual_actor -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/acp.rs` → `prepare_resume`; exact call `self.compactions.prepare_resume(session_id)`; behavior test `resumed_pre_ack_compaction_cannot_become_a_manual_completion_or_actor`; verify `cargo test --locked -p buzz-acp --lib resumed_pre_ack_compaction_cannot_become_a_manual_completion_or_actor -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/acp.rs` → `complete_resume`; exact call `.complete_resume(session_id, generation, response.is_ok());`; behavior test `resumed_pre_ack_compaction_cannot_become_a_manual_completion_or_actor`; verify `cargo test --locked -p buzz-acp --lib resumed_pre_ack_compaction_cannot_become_a_manual_completion_or_actor -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/acp.rs` → `bind`; exact call `self.compactions.bind(session_id, binding.generation_id());`; behavior test `resumed_pre_ack_compaction_cannot_become_a_manual_completion_or_actor`; verify `cargo test --locked -p buzz-acp --lib resumed_pre_ack_compaction_cannot_become_a_manual_completion_or_actor -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/acp.rs` → `prepare_new`; exact call `self.compactions.prepare_new();`; behavior test `compact_uses_pre_ack_new_session_advertisement_after_ordinary_completion`; verify `cargo test --locked -p buzz-acp --lib compact_uses_pre_ack_new_session_advertisement_after_ordinary_completion -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/acp.rs` → `complete_new`; exact call `self.compactions.complete_new(generation, returned_session);`; behavior test `failed_or_malformed_new_response_cannot_adopt_pre_ack_advertisement`; verify `cargo test --locked -p buzz-acp --lib failed_or_malformed_new_response_cannot_adopt_pre_ack_advertisement -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/acp.rs` → `check_response`; exact call `provider_failure::check_response(result)?;`; behavior test `provider_failure_is_an_application_error_and_same_pipe_can_retry`; verify `cargo test -p buzz-acp --lib provider_failure -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/acp.rs` → `trace_outgoing`; exact call `self.trace_outgoing(&msg, false);`; behavior test `kimi_plaintext_logs_keep_structure_without_copying_private_request_content`; verify `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`
- Invocation: `crates/buzz-acp/src/acp.rs` → `decode_agent_error`; exact call `return Err(self.decode_agent_error(error));`; behavior test `kimi_structured_failure_survives_generic_rpc_message_without_secret_logging`; verify `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`
- Invocation: `crates/buzz-acp/src/acp.rs` → `handle_kimi_permission_request`; exact call `return self.handle_kimi_permission_request(msg).await;`; behavior test `kimi_permission_from_another_session_is_never_approved`; verify `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`
- Invocation: `crates/buzz-acp/src/acp.rs` → `trace_outgoing`; exact call `self.trace_outgoing(&msg, false);
                            match self.write_ndjson(&msg).await {`; behavior test `kimi_native_steer_keeps_safe_logs_and_ack_variants`; verify `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`
- Verify: `just ci`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/acp/launch.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-acp/src/acp/launch/tests.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-acp/src/acp_frame_writer.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-acp/src/base_prompt.md`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-acp/src/config.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `false`.

- Required symbol: `BUZZ_ACP_SESSION_STORE_DIR`
- Required symbol: `BUZZ_ACP_NO_SESSION_RESUME`
- Verify: `just ci`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/edit_routing.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-acp/src/git.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-acp/src/lib.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches Preserve the heartbeat collection call with a textual drift guard; full live-service-loop behavioral mutation coverage is not claimed. Refresh idle clocks when a non-heartbeat prompt result is handled (port of block/buzz#7173). Enable session status once per harness and retire status on owner rotate and channel removal; forward the status directory to the managed MCP server. Schedule one asynchronous shared process poll, fence its application, and continue capacity dispatch across unavailable scopes. Main-loop select wiring is textually guarded; full relay-loop mutation coverage is not claimed. Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `false`.

- Required symbol: `process_poll.start(pool.background_poll_requests());`
- Required symbol: `fn result_refreshes_idle_clock(source: &PromptSource) -> bool`
- Required symbol: `mod session_status;`
- Required symbol: `name == session_status::SESSION_STATUS_DIR_ENV`
- Required symbol: `ctx.session_controls.compact_pending(&scope) && batch.continuation.is_none()`
- Required symbol: `fn handle_owner_cancel_for_scope`
- Invocation: `crates/buzz-acp/src/lib.rs` → `result_refreshes_idle_clock`; exact call `if result_refreshes_idle_clock(&result.source) {`; behavior test `error_outcome_emission_tests::channel_result_refreshes_idle_clock_wiring`; verify `cargo test -p buzz-acp --lib error_outcome_emission_tests::channel_result_refreshes_idle_clock_wiring -- --exact`
- Invocation: `crates/buzz-acp/src/lib.rs` → `depart_idle_sessions`; exact call `pool.depart_idle_sessions(session_store::Departure::IdleSleep);`; behavior test `pool_sleep_marks_every_idle_session`; verify `cargo test -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `restart`; exact call `match ctx.session_controls.restart(job) {`; behavior test `restart_during_delayed_acquisition_fences_old_dispatch`; verify `cargo test -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `discard_durable_channel`; exact call `pool.discard_durable_channel(ch);`; behavior test `channel_removal_then_readd_starts_new_session`; verify `cargo test -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `open_session_store`; exact call `let session_store = pool::open_session_store(`; behavior test `resume_advertised_uses_session_resume_not_new`; verify `cargo test -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `set_session_store`; exact call `pool.set_session_store(session_store.clone());`; behavior test `rotate_before_first_resume_discards_disk_session`; verify `cargo test -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `enable_session_status`; exact call `runtime.enable_session_status();`; behavior test `every_spawn_path_and_managed_mcp_get_the_current_status_dir`; verify `cargo test -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `retire_scope`; exact call `pool::session_visibility::retire_scope(`; behavior test `rotate_during_a_delayed_turn_retires_before_late_usage`; verify `cargo test -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `retire_channel`; exact call `pool::session_visibility::retire_channel(`; behavior test `channel_removal_retires_every_scope_of_that_channel`; verify `cargo test -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `admit_authorized`; exact call `session_controls::admit_authorized(`; behavior test `controls_use_author_capability_before_subscription_and_reserve_wrong_targets`; verify `cargo test -p buzz-acp --lib controls_use_author_capability_before_subscription_and_reserve_wrong_targets -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `capture`; exact call `agent.state.control_dispatch = Some(ctx.session_controls.capture(&scope));`; behavior test `busy_one_shot_already_consumed_keeps_restart_gate_and_newer_queue`; verify `cargo test -p buzz-acp --lib busy_one_shot_already_consumed_keeps_restart_gate_and_newer_queue -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `retirement_pending`; exact call `if ctx.session_controls.retirement_pending(&scope)
            || ctx.session_controls.status_pending(&scope)
            || (ctx.session_controls.compact_pending(&scope) && batch.continuation.is_none())`; behavior test `busy_one_shot_already_consumed_keeps_restart_gate_and_newer_queue`; verify `cargo test -p buzz-acp --lib busy_one_shot_already_consumed_keeps_restart_gate_and_newer_queue -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `current`; exact call `    if result
        .agent
        .state
        .control_dispatch
        .as_ref()
        .is_some_and(|dispatch| !dispatch.current())`; behavior test `late_cancelled_result_cannot_requeue_old_batch_after_restart`; verify `cargo test -p buzz-acp --lib late_cancelled_result_cannot_requeue_old_batch_after_restart -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `discard_retired_output`; exact call `result.outcome = result.outcome.discard_retired_output();`; behavior test `retired_transport_error_replaces_the_worker_through_result_handling`; verify `cargo test -p buzz-acp --lib retired_transport_error_replaces_the_worker_through_result_handling -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `drive`; exact call `worker_retirement::drive(`; behavior test `checked_retirement_executor_waits_for_verified_kill_and_excludes_refill`; verify `cargo test -p buzz-acp --lib checked_retirement_executor_waits_for_verified_kill_and_excludes_refill -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `finish`; exact call `worker_retirement::finish(`; behavior test `checked_retirement_executor_waits_for_verified_kill_and_excludes_refill`; verify `cargo test -p buzz-acp --lib checked_retirement_executor_waits_for_verified_kill_and_excludes_refill -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `return_for_retirement`; exact call `pool.return_for_retirement(result.agent);`; behavior test `retirement_return_boundary_bypasses_crash_budget_for_fatal_old_outcome`; verify `cargo test -p buzz-acp --lib retirement_return_boundary_bypasses_crash_budget_for_fatal_old_outcome -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `publish_result`; exact call `session_controls::publish::publish_result(&rest, &publication).await`; behavior test `control_result_publication_has_three_total_signed_attempts`; verify `cargo test -p buzz-acp --lib control_result_publication_has_three_total_signed_attempts -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `plan_retirement`; exact call `ctx.session_controls.plan_retirement(&scope, targets);`; behavior test `checked_retirement_requires_every_clean_report_and_retries_after_uncertainty`; verify `cargo test -p buzz-acp --lib checked_retirement_requires_every_clean_report_and_retries_after_uncertainty -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `drive`; exact call `orphan_reconciliation.drive(&ctx.session_controls, publication_capacity);`; behavior test `orphan_reconciliation_retries_failed_writes_with_capped_backoff_without_capacity_spin`; verify `cargo test -p buzz-acp --lib orphan_reconciliation_retries_failed_writes_with_capped_backoff_without_capacity_spin -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `wake`; exact call `_ = orphan_reconciliation.wake() => {`; behavior test `orphan_reconciliation_retries_failed_writes_with_capped_backoff_without_capacity_spin`; verify `cargo test -p buzz-acp --lib orphan_reconciliation_retries_failed_writes_with_capped_backoff_without_capacity_spin -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `join_next`; exact call `Some(result) = result_publications.join_next(), if !result_publications.is_empty() => {`; behavior test `orphan_reconciliation_retries_failed_writes_with_capped_backoff_without_capacity_spin`; verify `cargo test -p buzz-acp --lib orphan_reconciliation_retries_failed_writes_with_capped_backoff_without_capacity_spin -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `lose_retirement_evidence`; exact call `controls.lose_retirement_evidence(i);`; behavior test `retirement_foreground_panic_cannot_attest_replacement_or_forget_original_on_retry`; verify `cargo test -p buzz-acp --lib retirement_foreground_panic_cannot_attest_replacement_or_forget_original_on_retry -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `dispatch`; exact call `control_tasks::dispatch(&mut pool, &ctx, &control_tx);`; behavior test `compact_waits_for_exact_busy_owner_without_cancel_or_fork`; verify `cargo test --locked -p buzz-acp --lib compact_waits_for_exact_busy_owner_without_cancel_or_fork -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `wake_pending`; exact call `control_tasks::wake_pending(&mut queue, &ctx.session_controls);`; behavior test `compact_only_lazy_work_requires_existing_session`; verify `cargo test --locked -p buzz-acp --lib compact_only_lazy_work_requires_existing_session -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `compaction_panicked`; exact call `controls.compaction_panicked(i);`; behavior test `compact_actual_panic_terminalizes_started_without_ordinary_retry`; verify `cargo test --locked -p buzz-acp --lib compact_actual_panic_terminalizes_started_without_ordinary_retry -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `compaction_target`; exact call `pool.compaction_target(&job.scope, &ctx.session_controls, config.session_resume)`; behavior test `compact_resume_disabled_disk_refusal_and_replay_never_wake`; verify `cargo test --locked -p buzz-acp --lib compact_resume_ -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `handle_owner_cancel_for_scope`; exact call `handle_owner_cancel_for_scope(
                                        &mut pool, &mut queue, &ctx.session_controls, &scope, observer.as_ref(),
                                    );`; behavior test `uncertain_owner_cancel_has_no_chat_sender_and_fences_held_feedback`; verify `cargo test -p buzz-acp --lib uncertain_owner_cancel_has_no_chat_sender_and_fences_held_feedback -- --test-threads=1`
- Verify: `just ci`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/pool.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches Expose already-known DM metadata for display-only restart activity routing. Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `false`.

- Required symbol: `pub(crate) fn discard_retired_output(self) -> Self`
- Required symbol: `pub(crate) fn is_known_dm(`
- Invocation: `crates/buzz-acp/src/pool.rs` → `discard_durable`; exact call `self.discard_durable(scope);`; behavior test `invalidate_scope_writes_tombstone`; verify `cargo test -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool.rs` → `depart_durable_all`; exact call `self.depart_durable_all(crate::session_store::Departure::WorkerCrash);`; behavior test `invalidate_all_marks_worker_crash_without_tombstone`; verify `cargo test -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool.rs` → `persist_delivery`; exact call `self.persist_delivery(`; behavior test `delivery_success_persists_ids`; verify `cargo test -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool.rs` → `acquire_session`; exact call `match session_continuity::acquire_session(`; behavior test `resume_advertised_uses_session_resume_not_new`; verify `cargo test -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool.rs` → `persist_turns`; exact call `agent.state.persist_turns(scope);`; behavior test `max_turns_survives_resume`; verify `cargo test -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool.rs` → `insert_session_marker`; exact call `session_continuity::insert_session_marker(`; behavior test `marker_present_once_after_start`; verify `cargo test -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool.rs` → `invalidate_after_error`; exact call `agent.state.invalidate_after_error(&source, &e);`; behavior test `idle_worker_transport_death_keeps_session_resumable`; verify `cargo test -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool.rs` → `publish_session_start`; exact call `agent.publish_session_start(`; behavior test `new_session_status_precedes_the_prompt_and_spans_ordinary_turns`; verify `cargo test -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool.rs` → `rebind_session_status`; exact call `agent.rebind_session_status(status_runtime, scope, &sid);`; behavior test `new_session_status_precedes_the_prompt_and_spans_ordinary_turns`; verify `cargo test -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool.rs` → `history_coverage`; exact call `binding.set_history(session_visibility::history_coverage(`; behavior test `pinned_assignment_outside_the_window_counts_supplied_events_once`; verify `cargo test -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool.rs` → `reconcile_session_status_scope`; exact call `if agent.reconcile_session_status_scope(`; behavior test `retained_conversation_revokes_generation_before_rebind_or_late_updates`; verify `cargo test -p buzz-acp --lib session_visibility::tests::policy -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool.rs` → `selected`; exact call `session_history::selected(&context_target, b.channel_id, depth, &ctx).await`; behavior test `selected_restart_depths_reach_actual_prompt_and_supplied_ids`; verify `cargo test -p buzz-acp --lib selected_restart_depths_reach_actual_prompt_and_supplied_ids -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool.rs` → `reserved_raw`; exact call `if crate::session_controls::reserved_raw(obj) {`; behavior test `human_target_reserved_syntax_is_filtered_but_not_executed`; verify `cargo test -p buzz-acp --lib human_target_reserved_syntax_is_filtered_but_not_executed -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool.rs` → `foreground_success`; exact call `dispatch.foreground_success();`; behavior test `bootstrap_success_and_failed_foreground_preserve_restart_policy`; verify `cargo test -p buzz-acp --lib bootstrap_success_and_failed_foreground_preserve_restart_policy -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool.rs` → `current`; exact call `    let stale = agent
        .state
        .control_dispatch
        .as_ref()
        .is_some_and(|dispatch| !dispatch.current())`; behavior test `second_restart_survives_old_result`; verify `cargo test -p buzz-acp --lib second_restart_survives_old_result -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool.rs` → `supplied_mention`; exact call `first_mention = session_history::supplied_mention(
                first_mention,
                &context_target,
                conversation_context.as_ref(),
                &rendered_batch_ids,
            );`; behavior test `none_assignment_omitted_after_profile_render_is_explicitly_not_found`; verify `cargo test -p buzz-acp --lib none_assignment_omitted_after_profile_render_is_explicitly_not_found -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool.rs` → `discard_retired_output`; exact call `(outcome.discard_retired_output(), None)`; behavior test `retired_cancel_drain_expiry_replaces_the_worker_through_result_handling`; verify `cargo test -p buzz-acp --lib retired_cancel_drain_expiry_replaces_the_worker_through_result_handling -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool.rs` → `discard_retired_output`; exact call `let outcome = match prompt_result {
            Ok(reason) => PromptOutcome::Ok(reason),
            Err(error) => PromptOutcome::Error(error),
        }
        .discard_retired_output();`; behavior test `retired_adapter_death_replaces_the_worker_through_result_handling`; verify `cargo test -p buzz-acp --lib retired_adapter_death_replaces_the_worker_through_result_handling -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool.rs` → `prepare_background_return`; exact call `agent.prepare_background_return();`; behavior test `completed_empty_turn_prepares_observation_before_listener_return`; verify `cargo test --locked -p buzz-acp --lib completed_empty_turn_prepares_observation_before_listener_return -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool.rs` → `confirmed_report`; exact call `crate::managed_work::confirmed_report(&ctx.rest_client, work).await`; behavior test `kimi_confirmed_report_prevents_any_new_provider_rpc`; verify `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`
- Verify: `just ci`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/queue.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `false`.

- Verify: `just ci`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/recovery_wake.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-acp/src/scope.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-acp/src/setup_mode.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-acp/tests/run_task.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-agent/tests/regressions.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-auth/Cargo.toml`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-auth/src/lib.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-auth/src/nip_fi/command.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-auth/src/nip_fi/command_replay.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-auth/src/nip_fi/community.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-auth/src/nip_fi/config.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-auth/src/nip_fi/deny_map.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-auth/src/nip_fi/jwks/mod.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-auth/src/nip_fi/jwks/tests.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-auth/src/nip_fi/mod.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-auth/src/nip_fi/startup/mod.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-auth/src/nip_fi/startup/tests.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-auth/src/nip_fi/verifier.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-auth/src/nip_fi/verifier/tests.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-core/src/tenant.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-db/src/runtime/migration.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-db/src/runtime/mod.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-db/src/runtime/replica_fence.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-db/src/runtime/tests.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-db/src/runtime/tests/thread_window_postgres_tests.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches Verify production prebuild migration reaches the latest embedded migration while preserving the ingestion witness/index checks.

New module: `false`.

- Verify: `just ci`
- Verify: `scripts/postgres-test-run.sh -p buzz-db --lib --tests`

## `crates/buzz-db/src/store/community.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-db/src/store/deletion.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-db/src/store/moderation.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-db/src/store/relay_admin_actions.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-db/src/store/relay_members.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-db/src/store/user.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-deletion/src/lib.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-dev-mcp/Cargo.toml`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-dev-mcp/src/lib.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-dev-mcp/src/shell.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-dev-mcp/src/todo.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-dev-mcp/src/view_image.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-dev-mcp/tests/lifecycle.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-media/src/error.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-pubsub/src/conn_control.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-pubsub/src/lib.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-pubsub/src/nip_fi_command_replay.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/Cargo.toml`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/api/admin/auth.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/api/admin/direct.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/api/admin/mod.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/api/bridge.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/api/gifs.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/api/git/settings.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/api/git/settings_tests.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/api/git/transport.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/api/invites.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/api/media.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/api/mod.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/api/nip_fi.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/api/operator.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/api/workflows.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/audio/handler.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/config.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/connection.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/handlers/admin_action_worker.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/handlers/admin_outbox_worker.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/handlers/artifact.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/handlers/auth.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/handlers/community_provisioning.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/handlers/count.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/handlers/event.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/handlers/ingest.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/handlers/moderation_commands.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/handlers/relay_admin.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/handlers/report_resolution.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/handlers/req.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/lib.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/main.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/nip11.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/nip_fi_config.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/nip_fi_core.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/nip_fi_http.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/nip_fi_session.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/nip_fi_shadow.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/nip_fi_shadow_session.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/nip_fi_test_hooks.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/nip_fi_upgrade.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/router.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/state.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `crates/buzz-relay/src/test_support.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/playwright.config.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches. Register the session-status smoke spec. Register the scoped session-control smoke workflow. Register long-thread and large-channel virtualization coverage in the smoke suite. Register the open-thread highlight smoke spec.

New module: `false`.

- Required symbol: `"**/session-status.spec.ts",`
- Required symbol: `"**/session-controls.spec.ts",`
- Required symbol: `"**/thread-virtualization.spec.ts"`
- Required symbol: `"**/open-thread-highlight.spec.ts",`
- Verify: `just ci`
- Verify: `cd desktop && pnpm test:e2e:smoke`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke session-controls.spec.ts session-status.spec.ts`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke thread-virtualization.spec.ts thread-focus-mode.spec.ts scroll-history.spec.ts thread-unread.spec.ts`
- Verify: `cd desktop && pnpm build:e2e && pnpm exec playwright test tests/e2e/open-thread-highlight.spec.ts --project=smoke`

## `desktop/src-tauri/icons/icon.icns`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/commands/admin/direct_action_tests.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/commands/admin/error.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/commands/admin/helpers.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/commands/admin/mod.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/commands/admin/mod_tests.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/commands/admin/routes.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/commands/agent_config.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/commands/agent_models_tests.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/commands/agents_admission_tests.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/commands/channels.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/commands/channels/starter_tests.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/commands/channels_tests.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/commands/dms.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/commands/mesh_llm.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/egress_guard.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/huddle/agents.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/managed_agents/admission_test_support.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/managed_agents/discovery.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/managed_agents/env_vars/tests.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/managed_agents/nest.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/managed_agents/nest/tests.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/managed_agents/nest_agents.md`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/managed_agents/nest_skill.md`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/managed_agents/pair_admission.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/managed_agents/reserved_env_keys.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/managed_agents/restore_admission_tests.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/managed_agents/runtime/process.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/managed_agents/runtime_commands_admission_tests.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src-tauri/src/mesh_llm/coordinator.rs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/app/App.tsx`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/app/AppShell.tsx`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/app/useCommunityNavigationTransitions.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/admin-console/AdminConsoleActionsTab.tsx`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/admin-console/AdminConsolePanel.tsx`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/admin-console/AdminConsolePanelHelpers.tsx`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/admin-console/AdminConsoleReportsTab.tsx`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/admin-console/adminConsolePanelActions.jsdom-test.mjs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/admin-console/adminConsolePanelTestHelpers.jsdom.mjs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/admin-console/api.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/hooks.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/lib/managedAgentControlActions.test.mjs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/lib/managedAgentControlActions.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/lib/useAutoRestartPolicy.relayRemoved.jsdom-test.mjs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/lib/useAutoRestartPolicy.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/managedAgentReconciliationPlan.test.mjs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/managedAgentReconciliationPlan.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/managedAgentRelayCleanup.test.mjs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/managedAgentRelayCleanup.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/managedAgentRuntimeHooks.test.mjs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/managedAgentRuntimeHooks.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/managedAgentRuntimeStatus.test.mjs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/managedAgentRuntimeStatus.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/ui/AgentInstanceEditDialog.tsx`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/ui/PersonaDropdownField.tsx`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/ui/PersonaDropdownOptionLabel.tsx`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/ui/PersonaModelCombobox.tsx`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/ui/activityRenderClasses/ThoughtActivity.jsdom-test.mjs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/ui/activityRenderClasses/ThoughtActivity.render.test.mjs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/ui/activityRenderClasses/ThoughtActivity.tsx`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/ui/agentConfigOptions.tsx`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/ui/modelCapabilities.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/ui/modelMenuDescriptions.jsdom-test.mjs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/ui/relayMeshModelPicker.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/ui/useManagedAgentActions.relayRemoved.jsdom-test.mjs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/ui/usePersonaModelDiscovery.jsdom-test.mjs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/ui/usePersonaModelDiscovery.test.mjs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/ui/usePersonaModelDiscovery.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/useManagedAgentRuntimeReconciliation.jsdom-test.mjs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/agents/useManagedAgentRuntimeReconciliation.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/channels/hooks.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/channels/rosterAdminKick.test.mjs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/channels/rosterFreshness.test.mjs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/channels/rosterFreshness.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/channels/ui/ChannelScreen.tsx`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/channels/ui/useMembersSidebarActions.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/channels/useMembershipNotifications.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/channels/useUnreadChannels.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/communities/legacyCommunityReadmit.jsdom-test.mjs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/communities/legacyCommunityStorage.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/communities/ui/CommunitySwitcher.tsx`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/communities/updateCommunityReadmit.jsdom-test.mjs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/communities/useCommunities.tsx`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/communities/useCommunityInit.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/huddle/components/HuddleBar.tsx`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/huddle/components/HuddleRoomHeader.tsx`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/messages/hooks.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/messages/ui/TimelineMessageList.tsx`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches. Pass the open thread root id into the channel timeline.

New module: `false`.

- Required symbol: `openThreadHeadId={openThreadHeadId}`
- Invocation: `desktop/src/features/messages/ui/TimelineMessageList.tsx` → `MessageRowItem`; exact call `<MessageRowItem
              openThreadHeadId={openThreadHeadId}`; behavior test `buzz: highlights only the open root with a reply summary`; verify `cd desktop && pnpm build:e2e && pnpm exec playwright test tests/e2e/open-thread-highlight.spec.ts --project=smoke`
- Verify: `just ci`
- Verify: `cd desktop && pnpm build:e2e && pnpm exec playwright test tests/e2e/open-thread-highlight.spec.ts --project=smoke`

## `desktop/src/features/messages/ui/TimelineMessageRow.tsx`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches. Mark only the open thread root with theme-derived background and aria-current.

New module: `false`.

- Required symbol: `const isOpenThreadHead =`
- Required symbol: `hoverBackground={!isOpenThreadHead}`
- Invocation: `desktop/src/features/messages/ui/TimelineMessageRow.tsx` → `MessageRow`; exact call `<MessageRow
        channelId={channelId}
        hoverBackground={!isOpenThreadHead}`; behavior test `buzz: highlights an open root without replies`; verify `cd desktop && pnpm build:e2e && pnpm exec playwright test tests/e2e/open-thread-highlight.spec.ts --project=smoke`
- Verify: `just ci`
- Verify: `cd desktop && pnpm build:e2e && pnpm exec playwright test tests/e2e/open-thread-highlight.spec.ts --project=smoke`

## `desktop/src/features/messages/ui/submitMessageEdit.test.mjs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/messages/ui/submitMessageEdit.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/messages/ui/useMentionSendFlow.test-support.mjs`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/messages/ui/useMentionSendFlow.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/profile/ui/useAgentLifecycleActions.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/sidebar/ui/AppSidebar.tsx`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/sidebar/ui/AppSidebar.types.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/features/sidebar/ui/SidebarProfileCard.tsx`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/protectedFeatures/bestie/api.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/shared/api/channelMembershipWrites.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/shared/api/tauriChannels.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/shared/api/tauriWorkspace.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/src/testing/e2eBridge.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches Align only native-command mock reply p-tags with the native builder while preserving thread routing.

New module: `false`.

- Required symbol: `).filter((tag) => tag[0] !== "p" || tag[1] !== mockPubkey),`
- Verify: `just ci`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke session-controls.spec.ts session-status.spec.ts`

## `desktop/tests/e2e/community-rail.spec.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/tests/e2e/forum-agent-invitation.spec.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/tests/e2e/huddle-transcription.spec.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/tests/e2e/image-attachment-gallery.spec.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/tests/e2e/persistent-agent-audience.spec.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/tests/e2e/profile-custom-emoji-status.spec.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `desktop/tests/e2e/thread-follow.spec.ts`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `docs/operator-community-deletion.md`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `migrations/0054_owner_deletion_quota_reservation.sql`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `migrations/0055_relay_admin_direct_actions.sql`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/assets/images/buzz-wordmark.png`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/assets/images/shell-gradient.png`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/ios/Runner.xcodeproj/project.pbxproj`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/ios/Runner/AppDelegate.swift`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/ios/Runner/NativeMessageMenu.swift`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/ios/Runner/NativeMessagePresentationCoordinator.swift`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/ios/Runner/NativeReactionDetails.swift`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/ios/RunnerTests/NativeMessageImageLoaderTests.swift`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/app.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/activity/activity_page.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/activity/activity_page/inbox_row.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/activity/activity_page/lists.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/channels/add_members_sheet.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/channels/channel_actions_sheet.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/channels/channel_detail_page.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/channels/channel_detail_page/app_bar.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/channels/channel_detail_page/huddle_call_avatar.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/channels/channel_detail_page/huddle_call_participants.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/channels/channel_detail_page/huddle_participant_cluster.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/channels/channel_detail_page/huddle_participant_overlay.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/channels/channel_detail_page/huddle_sheet.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/channels/channel_detail_page/system_rows.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/channels/channel_details_page.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/channels/channels_page.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/channels/channels_page/body.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/channels/channels_page/channel_tile.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/channels/channels_page/community.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/channels/channels_page/sheets.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/channels/compose_bar/draft_lifecycle.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/channels/dm_channel_labels.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/channels/members_sheet.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/channels/message_actions.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/channels/message_long_press_region.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/channels/reaction_row.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/home/home_page.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/pairing/pairing_page.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/pairing/pairing_page/onboarding_background.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/pairing/pairing_page/onboarding_colors.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/pairing/pairing_page/onboarding_glass_button.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/pairing/pairing_page/onboarding_wordmark.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/pairing/pairing_page/pairing_welcome_view.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/pairing/pairing_qr_scanner.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/pairing/pairing_qr_scanner/dynamic_island_portal.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/profile/presence_cache_provider.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/profile/settings_profile_header.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/profile/user_profile_sheet.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/search/search_page.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/settings/settings_page.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/settings/settings_page/connection_section.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/settings/settings_page/notifications_section.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/settings/settings_page/profile_section.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/settings/settings_page/status_section.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/features/settings/theme_picker_page/theme_preview_sheet.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/shared/identity_names/identity_names.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/shared/identity_names/identity_names_provider.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/shared/success_haptic.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/lib/shared/widgets/native_message_presentation.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/pubspec.yaml`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/shaders/onboarding_wordmark.frag`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/test/features/activity/activity_page_test.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/test/features/channels/add_members_sheet_names_test.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/test/features/channels/channel_detail_page_test.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/test/features/channels/channel_detail_page_test/presence_tests.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/test/features/channels/channels_page_test.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/test/features/channels/channels_page_test/presence_tests.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/test/features/channels/compose_bar_test.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/test/features/channels/compose_bar_test/durable_mention_tests.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/test/features/channels/message_actions_test.dart`

Keep message action fixture connectivity isolated using the current upstream mocked channel; native popover fixtures were replaced by grouped action sheets.

New module: `false`.

- Required symbol: `const MethodChannel('dev.fluttercommunity.plus/connectivity_status')`
- Verify: `just ci`
- Verify: `just mobile-install mobile-check mobile-test`

## `mobile/test/features/channels/reaction_row_test.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/test/features/pairing/onboarding_wordmark_test.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/test/features/pairing/pairing_page_test.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/test/features/pairing/pairing_qr_scanner_test.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/test/features/profile/presence_cache_provider_test.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/test/features/profile/presence_ordering_test.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/test/features/profile/presence_snapshot_test.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/test/features/profile/settings_profile_header_test.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/test/features/search/search_page_test.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/test/features/settings/connection_section_test.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/test/features/settings/notification_recovery_test.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/test/features/settings/settings_page_loading_test.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/test/features/settings/settings_page_test.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/test/shared/push/push_unconfigured_build_test.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `mobile/test/widget_test.dart`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `schema/schema.sql`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `scripts/cutover/README.md`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `scripts/generate-macos-icon.py`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `scripts/macos-icon.sh`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `scripts/reconcile-schema-after-pgschema.sql`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches

New module: `false`.

- Verify: `just ci`

## `scripts/run-tests.sh`

Integrate existing reynholm/buzz main 33f54de2 without retiring fork patches Keep the cargo-test fallback selecting the same demo echo receive-retention regression as nextest.

New module: `false`.

- Required symbol: `buzz-relay demo echo receive retention`
- Verify: `just ci`
- Verify: `bash -n scripts/run-tests.sh`
- Verify: `just test-unit`

## `.config/nextest.toml`

Fork hosted CI fixtures and publication boundaries

New module: `false`.

- Verify: `just ci`
- Verify: `python3 scripts/fork/sync.py validate`

## `.github/workflows/docker.yml`

Fork hosted CI fixtures and publication boundaries

New module: `false`.

- Verify: `just ci`
- Verify: `python3 scripts/fork/sync.py validate`

## `.github/workflows/sprig.yml`

Fork hosted CI fixtures and publication boundaries

New module: `false`.

- Verify: `just ci`
- Verify: `python3 scripts/fork/sync.py validate`

## `crates/buzz-db/TESTING.md`

Fork hosted CI fixtures and publication boundaries

New module: `false`.

- Verify: `just ci`
- Verify: `python3 scripts/fork/sync.py validate`

## `desktop/tests/e2e/deep-link-invite.spec.ts`

Fork hosted CI fixtures and publication boundaries

New module: `false`.

- Verify: `just ci`
- Verify: `python3 scripts/fork/sync.py validate`

## `desktop/tests/e2e/exact-key-profile.spec.ts`

Fork hosted CI fixtures and publication boundaries

New module: `false`.

- Verify: `just ci`
- Verify: `python3 scripts/fork/sync.py validate`

## `desktop/tests/helpers/bridge.ts`

Fork hosted CI fixtures and publication boundaries

New module: `false`.

- Verify: `just ci`
- Verify: `python3 scripts/fork/sync.py validate`

## `desktop/tests/e2e/scroll-history.spec.ts`

Model real reader scroll intent before passive virtualized coverage regression

New module: `false`.

- Required symbol: `fast middle-page scroll settles with continuous mounted coverage`
- Required symbol: `scrollTopBeforeWheel`
- Required symbol: `page.mouse.wheel(0, -300)`
- Required symbol: `viewportCoverage`
- Verify: `pnpm -C desktop exec playwright test --project=smoke tests/e2e/scroll-history.spec.ts`
- Verify: `python3 scripts/fork/sync.py validate`

## `.github/workflows/sprig-image.yml`

Preserve Sprig image PR builds and gate unconfigured upstream registry publication

New module: `false`.

- Required symbol: `vars.GHCR_SPRIG_IMAGE`
- Required symbol: `github.repository == 'block/buzz'`
- Verify: `python3 scripts/fork/sync.py validate`
- Verify: `gh run list --repo github.com/reynholm/buzz --workflow sprig-image.yml`

## `desktop/tests/e2e/project-conversation-load-failure.spec.ts`

Keep historical reply fixtures outside the inclusive live subscription so terminal fetch failure and Retry recovery remain exercised

New module: `false`.

- Required symbol: `historyTimestamp`
- Required symbol: `Math.floor(Date.now() / 1000) - 60`
- Required symbol: `message-thread-replies-error`
- Required symbol: `REPLY_CONTENT`
- Verify: `pnpm -C desktop exec playwright test --project=smoke tests/e2e/project-conversation-load-failure.spec.ts`
- Verify: `python3 scripts/fork/sync.py validate`

## `desktop/tests/e2e/empty-edit-delete.spec.ts`

Exercise an open channel suggestion and await its cleared-query settlement before keyboard Enter submits an empty edit

New module: `false`.

- Required symbol: `#general stream`
- Required symbol: `wait for the cleared query first`
- Required symbol: `await page.keyboard.press("Enter")`
- Verify: `pnpm -C desktop exec playwright test --project=smoke tests/e2e/empty-edit-delete.spec.ts`
- Verify: `python3 scripts/fork/sync.py validate`

## `scripts/fork/linux-toolchain.sh`

Linux x86_64 (glibc 2.35) fork release assets with verified compiled identity

New module: `true`.

- Required symbol: `activate-hermit`
- Required symbol: `VERSION_ID:-}" == 22.04`
- Required symbol: `uv python install 3.12`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `scripts/fork/build-linux.sh`

Linux x86_64 (glibc 2.35) fork release assets with verified compiled identity

New module: `true`.

- Required symbol: `unset BUZZ_UPDATER_ENDPOINT BUZZ_UPDATER_PUBLIC_KEY BUZZ_BUILD_DEMO_SLUG`
- Required symbol: `export BUZZ_FORK_SHA="$CANDIDATE_SHA" BUZZ_FORK_BASE_TAG="$BASE_TAG" BUZZ_FORK_REVISION="$REVISION"`
- Required symbol: `scripts/fork/verify_linux.py`
- Required symbol: `PY="$(uv python find`
- Invocation: `scripts/fork/build-linux.sh` → `verify_linux.py`; exact call `"$PY" scripts/fork/verify_linux.py --candidate-sha "$CANDIDATE_SHA" --fork-revision "$REVISION"`; behavior test `scripts/fork/tests/test_verify_linux.py`; verify `python3 -m unittest discover -s scripts/fork/tests -v`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `scripts/fork/verify_linux.py`

Linux x86_64 (glibc 2.35) fork release assets with verified compiled identity

New module: `true`.

- Required symbol: `def elf_record(`
- Required symbol: `def expected_identity(`
- Required symbol: `def verify_probe(`
- Required symbol: `def verify_deb(`
- Required symbol: `GLIBC_CEILING = (2, 35)`
- Invocation: `scripts/fork/verify_linux.py` → `verify_probe`; exact call `config = verify_probe(probe, identity, upstream)`; behavior test `scripts/fork/tests/test_verify_linux.py`; verify `python3 -m unittest discover -s scripts/fork/tests -v`
- Invocation: `scripts/fork/verify_linux.py` → `elf_record`; exact call `records = {name: elf_record(binaries / name) for name in names}`; behavior test `scripts/fork/tests/test_verify_linux.py`; verify `python3 -m unittest discover -s scripts/fork/tests -v`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `scripts/fork/publish_linux.py`

Linux x86_64 (glibc 2.35) fork release assets with verified compiled identity

New module: `true`.

- Required symbol: `def is_linux_asset(`
- Required symbol: `def linux_assets(`
- Required symbol: `def verify_release(`
- Required symbol: `def publish(`
- Required symbol: `never clobber`
- Required symbol: `def locate_assets(`
- Invocation: `scripts/fork/publish_linux.py` → `verify_release`; exact call `release = verify_release(directory, tag, gh)`; behavior test `scripts/fork/tests/test_verify_linux.py`; verify `python3 -m unittest discover -s scripts/fork/tests -v`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `scripts/fork/tests/test_verify_linux.py`

Linux x86_64 (glibc 2.35) fork release assets with verified compiled identity

New module: `true`.

- Required symbol: `test_glibc_above_ubuntu_22_04_floor_is_rejected`
- Required symbol: `test_release_tag_must_agree_with_revision_and_base`
- Required symbol: `test_different_existing_bytes_wrong_tag_or_draft_stop_before_upload`
- Required symbol: `test_macos_promotion_inventory_ignores_linux_assets_only`
- Required symbol: `test_assets_are_located_in_a_downloaded_artifact_tree`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `.github/workflows/fork-linux-release.yml`

Linux x86_64 (glibc 2.35) fork release assets with verified compiled identity

New module: `true`.

- Required symbol: `container: ubuntu:22.04@sha256:`
- Required symbol: `scripts/fork/build-linux.sh --tag "$TAG"`
- Required symbol: `environment: fork-promotion`
- Required symbol: `publish_linux.py --tag "$TAG" --artifacts linux-assets --publish`
- Required symbol: `TRIGGERING_ACTOR: ${{ github.triggering_actor }}`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `TESTING.md`

Document production-writer recovery, replica-floor premises, authenticated CLI/relay regression commands and evidence boundaries. Document fresh-build session-control smoke workflow and shared actual-parser fixture coverage. Document typed activity workflow, observed identity boundaries and unchanged observer ownership. Thread-composer context workflow and bounded jsdom failure diagnostics with external process-tree containment.

New module: `false`.

- Required symbol: `### Desktop scoped session controls`
- Required symbol: `Observer access remains owner-scoped;`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" cargo test -p buzz-relay --lib handlers::ingest::postgres_tests::managed_delivery_tests:: -- --ignored --test-threads=1`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke session-controls.spec.ts session-status.spec.ts`

## `crates/buzz-acp/TESTING.md`

Document original-session collection, bounded ordinary preemption and bundled agent/MCP/CLI delivery runtime fixtures. Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `false`.

- Verify: `env -u BUZZ_ACP_SESSION_POLICY -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP cargo test -p buzz-acp background -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/acp/process_tracking_tests.rs`

Inherited integration inventory: real ACP adapter regressions distinguish resident providers, active task children and unrelated sessions.

New module: `true`.

- Required symbol: `prompt_work_is_tracked_under_worker_and_resident_provider`
- Verify: `env -u BUZZ_ACP_SESSION_POLICY -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP cargo test -p buzz-acp process_tracking_tests -- --test-threads=1`

## `crates/buzz-acp/src/isolated_execution.rs`

Inherited integration inventory: fresh-process isolated prompt executor with cancellation, deadlines and bounded shutdown; module retention does not assert a new background-delivery invocation.

New module: `false`.

- Required symbol: `pub(crate) async fn execute(`
- Required symbol: `agent.acp.shutdown().await;`
- Verify: `env -u BUZZ_ACP_SESSION_POLICY -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP cargo test -p buzz-acp --lib -- --test-threads=1`

## `crates/buzz-acp/src/pool/background_work.rs`

Retain collection authority in the original worker/session, poll child exit independently of pool sleep, and compose stable-key final-report instructions. Release uncertainty-only holds without inventing collection; retain observed exit evidence across Unknown. Preserve completed-task evidence across service-only snapshots and remove only proven service instances. Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `true`.

- Required symbol: `pub(crate) struct RuntimeHold`
- Required symbol: `pub fn queue_finished_background_work`
- Required symbol: `pub(super) fn followup_prompt`
- Required symbol: `agent.acp.observed_background_instances.is_empty()`
- Invocation: `crates/buzz-acp/src/pool.rs` → `followup_prompt`; exact call `background_work::followup_prompt(batch.channel_id, continuation, &ctx.cwd)`; behavior test `background_managed_delivery_key_stays_on_collection_across_error_and_new_message`; verify `env -u BUZZ_ACP_SESSION_POLICY -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP cargo test -p buzz-acp background_managed_delivery_key_stays_on_collection_across_error_and_new_message -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool/background_work.rs` → `contains`; exact call `!self.retire_on_return.contains(&index)`; behavior test `retirement_targets_include_checked_out_copies_and_block_idle_claims`; verify `cargo test -p buzz-acp --lib retirement_targets_include_checked_out_copies_and_block_idle_claims -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool.rs` → `refresh_runtime_hold`; exact call `self.refresh_runtime_hold(&mut agent);`; behavior test `background_unknown_then_empty_releases_hold_without_inventing_collection`; verify `cargo test -p buzz-acp --lib background_unknown_then_empty_releases_hold_without_inventing_collection -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool/background_work.rs` → `apply_background_children`; exact call `self.apply_background_children(index, children, queue);`; behavior test `background_observed_then_unknown_then_empty_collects_exactly_once`; verify `cargo test -p buzz-acp --lib background_observed_then_unknown_then_empty_collects_exactly_once -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool/background_work.rs` → `background_child_pids_in`; exact call `agent.acp.background_child_pids_in(tree)`; behavior test `current_shared_snapshot_retains_actual_work_in_its_slot`; verify `cargo test --locked -p buzz-acp --lib current_shared_snapshot_retains_actual_work_in_its_slot -- --test-threads=1`
- Verify: `env -u BUZZ_ACP_SESSION_POLICY -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP cargo test -p buzz-acp background_work_tests -- --test-threads=1`
- Verify: `cargo test -p buzz-acp --lib background_unknown_then_empty_releases_hold_without_inventing_collection -- --test-threads=1`
- Verify: `cargo test -p buzz-acp --lib background_observed_then_unknown_then_empty_collects_exactly_once -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/pool/background_work_tests.rs`

Production-dispatch local-process regressions for route/session retention, collection progress under repeated preemption, cancellation, rotation and lost-task cleanup. Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `true`.

- Required symbol: `background_collection_completes_under_repeated_ordinary_preemption`
- Required symbol: `background_explicit_cancel_after_ordinary_control_never_resumes_collection`
- Verify: `env -u BUZZ_ACP_SESSION_POLICY -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP cargo test -p buzz-acp background_work_tests -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/pool/delivery_runtime_tests.rs`

Run actual bundled agent, MCP and CLI processes against a signature-checking mock relay: application-error replay, lost acknowledgments, process restart and concurrency; not real-relay evidence. Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `true`.

- Required symbol: `managed_runtime_bundled_agent_publication_then_application_error_retries_one_event`
- Required symbol: `managed_runtime_killed_after_relay_acceptance_replays_on_new_process`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_SESSION_POLICY -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP cargo test -p buzz-acp managed_runtime_ -- --ignored --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/pool/pi_prompt_tests.rs`

Inherited integration inventory: Pi ACP system-prompt metadata, workspace skill installation and restore compatibility fixtures.

New module: `false`.

- Required symbol: `pi_composed_prompt_uses_meta_without_capability_negotiation`
- Required symbol: `real_pi_preserves_buzz_prompt_and_launch_skills_on_restore`
- Verify: `env -u BUZZ_ACP_SESSION_POLICY -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP cargo test -p buzz-acp pi_prompt_tests -- --test-threads=1`

## `crates/buzz-acp/src/process_tree.rs`

Bounded process snapshots and resident-helper classification; macOS/Linux executable confirmation releases completed Codex turns while preserving actual child work. Exact Codex default Bash snapshots remain transient bookkeeping; their ordinary descendants and launchers remain work. Scoped missing executable evidence remains uncertainty while definite descendants remain observable. Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `true`.

- Required symbol: `pub fn snapshot()`
- Required symbol: `pub fn blocking_pids(`
- Required symbol: `pub fn is_resident_helper(`
- Required symbol: `npm_exec_mcp_chain_is_resident`
- Required symbol: `launcher_into_scoped_mcp_package_is_resident`
- Required symbol: `pub(crate) fn is_snapshot_helper(`
- Required symbol: `pub(crate) fn startup_resident_pids(`
- Required symbol: `pub(crate) fn is_snapshot_candidate(`
- Required symbol: `pub(crate) fn is_uncertain_snapshot(`
- Invocation: `crates/buzz-acp/src/acp.rs` → `snapshot`; exact call `crate::process_tree::snapshot()`; behavior test `prompt_work_is_tracked_under_worker_and_resident_provider`; verify `env -u BUZZ_ACP_SESSION_POLICY -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP cargo test -p buzz-acp prompt_work_is_tracked_under_worker_and_resident_provider -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/acp.rs` → `blocking_pids`; exact call `blocking.extend(tree.blocking_pids(parent, &self.resident_pids));`; behavior test `work_under_late_codex_host_holds_slot_until_child_exits`; verify `env -u BUZZ_ACP_SESSION_POLICY -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP cargo test -p buzz-acp resident_helper_tests -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/acp.rs` → `is_resident_helper`; exact call `self.resident_pids.contains(&child.pid) || tree.is_resident_helper(child)`; behavior test `work_under_late_codex_host_holds_slot_until_child_exits`; verify `env -u BUZZ_ACP_SESSION_POLICY -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP cargo test -p buzz-acp resident_helper_tests -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/acp.rs` → `startup_resident_pids`; exact call `self.resident_pids.extend(tree.startup_resident_pids(pid));`; behavior test `codex_snapshot_child_work_survives_late_and_session_capture`; verify `cargo test -p buzz-acp --lib snapshot_helper_tests -- --test-threads=1`
- Verify: `env -u BUZZ_ACP_SESSION_POLICY -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP cargo test -p buzz-acp process_tree -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/process_tree/snapshot_tests.rs`

Inherited integration inventory: process-list timeout/output bounds, malformed snapshots and zombie exclusion tests.

New module: `true`.

- Required symbol: `snapshot_rejects_partial_output_and_ignores_zombies`
- Required symbol: `listing_timeout_kills_a_stalled_command`
- Verify: `env -u BUZZ_ACP_SESSION_POLICY -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP cargo test -p buzz-acp process_tree -- --test-threads=1`

## `crates/buzz-acp/src/queue/background.rs`

Keep background collection separate from ordinary messages with stable delivery UUID, original thread/session ownership, finite retries and bounded ordinary preemption. Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `true`.

- Required symbol: `pub(crate) struct RuntimeContinuation`
- Required symbol: `pub ordinary_preempted: bool`
- Required symbol: `background_managed_delivery_key_stays_on_collection_across_error_and_new_message`
- Invocation: `crates/buzz-acp/src/queue.rs` → `enqueue_continuation`; exact call `self.enqueue_continuation(batch.scope, continuation);`; behavior test `background_collection_completes_under_repeated_ordinary_preemption`; verify `env -u BUZZ_ACP_SESSION_POLICY -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP cargo test -p buzz-acp background_collection_completes_under_repeated_ordinary_preemption -- --test-threads=1`
- Verify: `env -u BUZZ_ACP_SESSION_POLICY -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP cargo test -p buzz-acp background -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-cli/Cargo.toml`

Add filesystem locking and temporary-fixture dependencies for durable managed-report journals.

New module: `false`.

- Required symbol: `fs2 = "0.4"`
- Verify: `cargo test -p buzz-cli managed_ -- --test-threads=1`

## `crates/buzz-cli/TESTING.md`

Specify managed publisher scope, journal durability and retention, recovery opt-in/upgrade requirements and real versus mock-relay checks. Document session-status enrichment, its absence/failure behavior and its automated checks. Document mixed PDF, ZIP and Markdown attachment checks and authenticated byte comparisons. Document blank message rejection, edit stdin behavior, attachment-only sends and binary regressions. Kimi recovery: support stable managed-report routes and logical-work visibility without changing other adapter policies.

New module: `false`.

- Verify: `cargo test -p buzz-cli managed_ -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" cargo test -p buzz-relay --lib handlers::ingest::postgres_tests::managed_delivery_tests:: -- --ignored --test-threads=1`
- Verify: `cargo test -p buzz-cli session_status -- --test-threads=1`
- Verify: `cargo test -p buzz-cli`
- Verify: `cargo test --locked -p buzz-cli`

## `crates/buzz-cli/src/commands/delivery.rs`

Lock, validate and fsync a bounded original-report journal before submission; refresh signed recovery stamps and persist only verified canonical receipts while preserving original evidence. Kimi recovery: support stable managed-report routes and logical-work visibility without changing other adapter policies.

New module: `true`.

- Required symbol: `pub(super) async fn deliver(`
- Required symbol: `fn acknowledgment(`
- Required symbol: `fn durable_directory(`
- Required symbol: `recovery_candidate`
- Required symbol: `canonical_event`
- Invocation: `crates/buzz-cli/src/commands/messages.rs` → `deliver`; exact call `super::delivery::deliver(
                client,
                &channel,
                reply_to.as_deref().unwrap_or_default(),
                &content,
                &delivery_key,
                journal_dir.as_deref(),
                mentions,
            )`; behavior test `managed_top_level_route_is_immutable_and_does_not_invent_a_thread`; verify `cargo test --locked -p buzz-cli`
- Invocation: `crates/buzz-cli/src/commands/delivery.rs` → `submit_event`; exact call `write(&dir, &journal)?;
    let result = client
        .submit_event(submission)`; behavior test `managed_same_key_changed_text_returns_first_signed_delivery`; verify `cargo test -p buzz-cli managed_same_key_changed_text_returns_first_signed_delivery -- --test-threads=1`
- Verify: `cargo test -p buzz-cli managed_ -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" cargo test -p buzz-relay --lib handlers::ingest::postgres_tests::managed_delivery_tests:: -- --ignored --test-threads=1`
- Verify: `cargo test --locked -p buzz-cli`

## `crates/buzz-cli/src/commands/messages.rs`

Expose messages deliver and preserve original route, stable UUID and signed callback mentions without suppressing ordinary progress or explicit edits. Attach the sender's own best-effort session status to newly prepared kind-9 messages before signing, in the shared send/first-deliver preparation. Emit generic attachments as filename links rather than image markup. Reject blank send, first delivery, edit and diff bodies before building events; preserve attachment-only sends and resolve edit stdin before validation. Kimi recovery: support stable managed-report routes and logical-work visibility without changing other adapter policies.

New module: `false`.

- Required symbol: `MessagesCmd::Deliver {`
- Required symbol: `pub(super) fn event_mention_pubkeys(`
- Required symbol: `async fn resolve_content_mentions(`
- Invocation: `crates/buzz-cli/src/commands/delivery.rs` → `event_mention_pubkeys`; exact call `messages::event_mention_pubkeys(&chosen)`; behavior test `managed_recovery_mention_history_survives_lost_ack`; verify `BUZZ_TEST_BIN_DIR="$PWD/target/debug" cargo test -p buzz-relay --lib managed_recovery_mention_history_survives_lost_ack -- --ignored --test-threads=1`
- Invocation: `crates/buzz-cli/src/commands/messages.rs` → `message_status_tags`; exact call `.tags(super::session_status::message_status_tags(
                client,
                channel_uuid,
                thread_ref.as_ref().map(|t| &t.root_event_id),
            ))`; behavior test `status_reply_carries_thread_status_and_keeps_existing_tags`; verify `cargo test -p buzz-cli session_status -- --test-threads=1`
- Invocation: `crates/buzz-cli/src/commands/messages.rs` → `attachment_markdown`; exact call `media_content.push_str(&crate::attachments::attachment_markdown(&desc));`; behavior test `messages_emit_file_links_for_multiple_attachments_and_keep_media_markup`; verify `cargo test -p buzz-cli`
- Invocation: `crates/buzz-cli/src/commands/messages.rs` → `validate_nonempty_content`; exact call `validate_nonempty_content(&p.content)?;`; behavior test `blank_message_commands_fail_before_any_relay_request`; verify `cargo test -p buzz-cli --test message_content`
- Invocation: `crates/buzz-cli/src/commands/messages.rs` → `validate_nonempty_content`; exact call `validate_nonempty_content(&diff_content)?;`; behavior test `blank_message_commands_fail_before_any_relay_request`; verify `cargo test -p buzz-cli --test message_content`
- Invocation: `crates/buzz-cli/src/commands/messages.rs` → `validate_nonempty_content`; exact call `validate_nonempty_content(&content)?;`; behavior test `blank_message_commands_fail_before_any_relay_request`; verify `cargo test -p buzz-cli --test message_content`
- Invocation: `crates/buzz-cli/src/commands/messages.rs` → `read_or_stdin`; exact call `let content = read_or_stdin(content)?;`; behavior test `message_commands_preserve_stdin_bytes_in_signed_events`; verify `cargo test -p buzz-cli --test message_content`
- Verify: `cargo test -p buzz-cli managed_ -- --test-threads=1`
- Verify: `cargo test -p buzz-cli mention -- --test-threads=1`
- Verify: `cargo test -p buzz-cli session_status -- --test-threads=1`
- Verify: `cargo test -p buzz-cli`
- Verify: `cargo test --locked -p buzz-cli`

## `crates/buzz-cli/src/commands/mod.rs`

Wire the managed delivery module into the CLI message dispatcher. Register the read-only session-status lookup.

New module: `false`.

- Required symbol: `mod delivery;`
- Required symbol: `pub(crate) mod session_status;`
- Verify: `cargo test -p buzz-cli managed_ -- --test-threads=1`
- Verify: `cargo test -p buzz-cli session_status -- --test-threads=1`

## `crates/buzz-cli/src/delivery_tests.rs`

Production CLI regressions for same-key replay, namespace/route integrity, concurrency, durable ancestor links, bounded retention/retries and legacy wrapper rejection. Shares its mock relay (palette, upload and HTTP-error modes) with session-status regressions. Kimi recovery: support stable managed-report routes and logical-work visibility without changing other adapter policies.

New module: `true`.

- Required symbol: `managed_same_key_changed_text_returns_first_signed_delivery`
- Required symbol: `managed_expired_journal_never_treats_legacy_wrapper_ack_as_delivery`
- Required symbol: `managed_existing_nested_directory_requires_ancestor_link_sync_before_publication`
- Verify: `cargo test -p buzz-cli managed_ -- --test-threads=1`
- Verify: `cargo test -p buzz-cli session_status -- --test-threads=1`
- Verify: `cargo test --locked -p buzz-cli`

## `crates/buzz-cli/src/error.rs`

Expose managed-delivery pending as a safe same-key retry while preserving existing ambiguous unjournaled-send errors.

New module: `false`.

- Required symbol: `ManagedDeliveryPending`
- Verify: `cargo test -p buzz-cli managed_ -- --test-threads=1`

## `crates/buzz-cli/src/lib.rs`

Define messages deliver arguments and compile production-publisher regression tests. Configure the client with the harness-advertised session-status directory and compile session-status regressions. Compile the attachment formatter and production upload regressions. Document stdin support for message edits. Kimi recovery: support stable managed-report routes and logical-work visibility without changing other adapter policies.

New module: `false`.

- Required symbol: `Deliver {`
- Required symbol: `mod delivery_tests;`
- Required symbol: `mod session_status_tests;`
- Required symbol: `mod attachments;`
- Required symbol: `mod attachment_tests;`
- Invocation: `crates/buzz-cli/src/lib.rs` → `status_dir_from_env`; exact call `.with_session_status_dir(commands::session_status::status_dir_from_env());`; behavior test `binary_reads_the_status_directory_from_the_environment`; verify `cargo test -p buzz-cli --test session_status_env`
- Verify: `cargo test -p buzz-cli managed_ -- --test-threads=1`
- Verify: `cargo test -p buzz-cli session_status -- --test-threads=1`
- Verify: `cargo test -p buzz-cli --test session_status_env`
- Verify: `cargo test -p buzz-cli`
- Verify: `cargo test --locked -p buzz-cli`

## `crates/buzz-cli/src/client.rs`

Carry the optional session-status instance directory with the client's relay URL and signing key. Upload generic documents and archives and retain a sanitized basename across both Blossom endpoint paths.

New module: `false`.

- Required symbol: `pub fn with_session_status_dir(`
- Required symbol: `pub fn session_status_dir(&self)`
- Invocation: `crates/buzz-cli/src/client.rs` → `validate_upload`; exact call `validate_upload(&mime, bytes.len() as u64)?;`; behavior test `rejects_detected_executables_before_upload`; verify `cargo test -p buzz-cli`
- Verify: `cargo test -p buzz-cli session_status -- --test-threads=1`
- Verify: `cargo test -p buzz-cli`

## `crates/buzz-cli/src/commands/session_status.rs`

Bounded read-only status lookup: absolute instance directory, nonblocking instance and generation lock probes, identity/scope/instance validation, thread-or-conversation destination with ambiguity rejection; any failure yields no tags.

New module: `true`.

- Required symbol: `pub(crate) const SESSION_STATUS_DIR_ENV: &str = "BUZZ_SESSION_STATUS_DIR";`
- Required symbol: `pub(crate) fn status_tags(`
- Required symbol: `pub(crate) fn message_status_tags(`
- Required symbol: `fn held_by_producer(`
- Required symbol: `status_dir_value_must_be_a_nonempty_absolute_path`
- Verify: `cargo test -p buzz-cli session_status -- --test-threads=1`
- Verify: `cargo test -p buzz-cli --test session_status_env`

## `crates/buzz-cli/src/session_status_tests.rs`

Production send/deliver regressions against a harness-shaped status directory: destination scopes, absence/invalid/stale snapshots, released generation and instance locks, no directory mutation, failed sends, kind-9 only and managed retry/recovery keeping the original tags.

New module: `true`.

- Required symbol: `status_released_generation_lock_rejects_remaining_snapshot`
- Required symbol: `status_released_instance_lock_rejects_live_looking_snapshot`
- Required symbol: `status_publication_into_another_thread_never_inherits_status`
- Required symbol: `managed_status_retry_keeps_the_originally_prepared_tags`
- Required symbol: `managed_status_timestamp_recovery_preserves_original_tags`
- Verify: `cargo test -p buzz-cli session_status -- --test-threads=1`

## `crates/buzz-cli/tests/session_status_env.rs`

Prove the built buzz binary reads BUZZ_SESSION_STATUS_DIR from its environment.

New module: `true`.

- Required symbol: `binary_reads_the_status_directory_from_the_environment`
- Verify: `cargo test -p buzz-cli --test session_status_env`

## `crates/buzz-core/src/kind.rs`

Reserve ephemeral managed recovery kind 20003 for signed canonical election; upgraded relays route it through shared ingest.

New module: `false`.

- Required symbol: `KIND_MANAGED_DELIVERY_RECOVERY: u32 = 20004`
- Invocation: `crates/buzz-relay/src/handlers/event.rs` → `is_ephemeral`; exact call `if is_ephemeral(kind_u32)
        && kind_u32 != buzz_core::kind::KIND_NIP43_LEAVE_REQUEST
        && !buzz_core::managed_delivery::is_recovery_envelope(&event)`; behavior test `managed_ws_original_ack_correlates_after_recovery_wins`; verify `BUZZ_TEST_BIN_DIR="$PWD/target/debug" cargo test -p buzz-relay --lib managed_ws_original_ack_correlates_after_recovery_wins -- --ignored --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" cargo test -p buzz-relay --lib handlers::ingest::postgres_tests::managed_delivery_tests:: -- --ignored --test-threads=1`

## `crates/buzz-core/src/lib.rs`

Export the shared signed managed-report recovery protocol and the shared session-status contract. Export the pure session command, result and actor contract.

New module: `false`.

- Required symbol: `pub mod managed_delivery;`
- Required symbol: `pub mod session_status;`
- Required symbol: `pub mod session_control;`
- Verify: `cargo test -p buzz-core managed_delivery -- --test-threads=1`
- Verify: `cargo test -p buzz-core session_status -- --test-threads=1`
- Verify: `cargo test -p buzz-core -- --test-threads=1`

## `crates/buzz-core/src/session_status.rs`

Pure shared session-status contract: canonical status identity, scope file names, instance layout names, bounded reason vocabulary, snapshot format v1 with size/version validation, and the buzz-session wire tags. Read legacy v1 and current v2 snapshots, keep primary/info wire shapes, validate optional manual origin and add bounded control/departure reasons.

New module: `true`.

- Required symbol: `pub fn canonical_identity(`
- Required symbol: `pub fn scope_file_name(`
- Required symbol: `pub fn generation_lock_path(`
- Required symbol: `pub fn parse_snapshot(`
- Required symbol: `pub fn event_tags(`
- Required symbol: `pub struct StatusEnvelope`
- Required symbol: `pub struct ContextOccupancy`
- Required symbol: `pub struct HistoryCoverage`
- Required symbol: `pub const STATUS_FORMAT_VERSION: u8 = 2;`
- Required symbol: `pub origin: Option<ControlOrigin>`
- Invocation: `crates/buzz-core/src/session_status.rs` → `event_tag`; exact call `tags.push(
                ControlAction {
                    transition_id: self.status.transition_id,
                    origin: origin.clone(),
                }
                .event_tag()
                .ok_or(StatusError::InvalidOrigin)?,
            );`; behavior test `snapshot_legacy_v1_and_v2_actor_preserve_wire_dimensions`; verify `cargo test -p buzz-core --test session_control snapshot_legacy_v1_and_v2_actor_preserve_wire_dimensions -- --test-threads=1`
- Invocation: `crates/buzz-core/src/session_status.rs` → `validate`; exact call `envelope.validate()?;`; behavior test `snapshot_rejects_invalid_actor_and_automatic_actor`; verify `cargo test -p buzz-core --test session_control snapshot_rejects_invalid_actor_and_automatic_actor -- --test-threads=1`
- Verify: `cargo test -p buzz-core session_status -- --test-threads=1`
- Verify: `cargo test -p buzz-core -- --test-threads=1`

## `crates/buzz-core/src/managed_delivery.rs`

Validate author-signed original report and compact candidate stamp, immutable body/tags/route, canonical UUID and current wrapper/candidate timestamps.

New module: `true`.

- Required symbol: `pub struct RecoveryRequest`
- Required symbol: `pub struct ReportStamp`
- Required symbol: `pub fn delivery_key(`
- Required symbol: `pub fn same_report(`
- Required symbol: `pub fn decode(`
- Invocation: `crates/buzz-relay/src/handlers/ingest.rs` → `decode`; exact call `buzz_core::managed_delivery::RecoveryRequest::decode(
                &event,
                nostr::Timestamp::now().as_secs(),
            )`; behavior test `managed_old_absent_report_recovers_with_production_writer`; verify `BUZZ_TEST_BIN_DIR="$PWD/target/debug" cargo test -p buzz-relay --lib managed_old_absent_report_recovers_with_production_writer -- --ignored --test-threads=1`
- Verify: `cargo test -p buzz-core managed_delivery -- --test-threads=1`

## `crates/buzz-db/src/error.rs`

Return an explicit canonical delivery conflict for UUID reuse with another report or route.

New module: `false`.

- Required symbol: `ManagedDeliveryConflict`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" cargo test -p buzz-relay --lib handlers::ingest::postgres_tests::managed_delivery_tests:: -- --ignored --test-threads=1`

## `crates/buzz-db/src/store/event.rs`

Route ordinary marked kind-9 inserts through the same atomic canonical election as recovery while retaining transactional thread metadata and the production commit-time floor.

New module: `false`.

- Required symbol: `crate::store::managed_delivery::insert(`
- Invocation: `crates/buzz-db/src/store/event.rs` → `insert`; exact call `crate::store::managed_delivery::insert(
                pool,
                community_id,
                event,
                channel,
                thread_meta,
                None,
            )`; behavior test `managed_recovery_and_original_race_share_one_election`; verify `BUZZ_TEST_BIN_DIR="$PWD/target/debug" cargo test -p buzz-relay --lib managed_recovery_and_original_race_share_one_election -- --ignored --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" cargo test -p buzz-relay --lib handlers::ingest::postgres_tests::managed_delivery_tests:: -- --ignored --test-threads=1`

## `crates/buzz-db/src/store/managed_delivery.rs`

Elect once per community/author/UUID under a transaction advisory lock; adopt accepted or soft-deleted originals, otherwise commit candidate, thread counters, callback mentions and durable binding together.

New module: `true`.

- Required symbol: `pub async fn insert(`
- Required symbol: `pub async fn recover_managed_delivery(`
- Required symbol: `pg_advisory_xact_lock`
- Required symbol: `INSERT INTO managed_deliveries`
- Invocation: `crates/buzz-relay/src/handlers/ingest.rs` → `recover_managed_delivery`; exact call `.recover_managed_delivery(
                    tenant.community(),
                    &event,
                    channel,
                    thread_params,
                    &request.original,
                )`; behavior test `managed_old_accepted_report_recovers_with_production_writer`; verify `BUZZ_TEST_BIN_DIR="$PWD/target/debug" cargo test -p buzz-relay --lib managed_old_accepted_report_recovers_with_production_writer -- --ignored --test-threads=1`
- Invocation: `crates/buzz-db/src/store/managed_delivery.rs` → `insert_mentions_in_transaction`; exact call `crate::runtime::insert_mentions_in_transaction(&mut tx, &result.0.event, Some(channel))`; behavior test `managed_recovery_mention_history_survives_lost_ack`; verify `BUZZ_TEST_BIN_DIR="$PWD/target/debug" cargo test -p buzz-relay --lib managed_recovery_mention_history_survives_lost_ack -- --ignored --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" cargo test -p buzz-relay --lib handlers::ingest::postgres_tests::managed_delivery_tests:: -- --ignored --test-threads=1`

## `crates/buzz-db/src/store/mod.rs`

Register the canonical managed-delivery storage module.

New module: `false`.

- Required symbol: `pub mod managed_delivery;`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" cargo test -p buzz-relay --lib handlers::ingest::postgres_tests::managed_delivery_tests:: -- --ignored --test-threads=1`

## `crates/buzz-db/tests/observability_source.rs`

Repair stale aggregate-cardinality and bounded-helper assumptions while retaining typed Authorization/EventWrite checkout and direct-pool bypass guards.

New module: `false`.

- Required symbol: `p0_pool_acquisitions_use_typed_operation_pairs_without_other`
- Verify: `cargo test -p buzz-db --test observability_source`

## `crates/buzz-relay/src/handlers/ingest/postgres_tests/managed_delivery_tests.rs`

Use the armed production writer and authenticated CLI/router to test absent/accepted/deleted recovery, races, WS acknowledgment correlation, escaped-size limits and callback Home Mentions after lost acknowledgments.

New module: `true`.

- Required symbol: `managed_recovery_and_original_race_share_one_election`
- Required symbol: `managed_ws_original_ack_correlates_after_recovery_wins`
- Required symbol: `managed_recovery_mention_history_survives_lost_ack`
- Required symbol: `buzz.created_at_floor`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" cargo test -p buzz-relay --lib handlers::ingest::postgres_tests::managed_delivery_tests:: -- --ignored --test-threads=1`

## `crates/buzz-relay/src/handlers/ingest/timestamp.rs`

Keep the future ceiling and ordinary age checks; permit old self-marked reports to acknowledge retained acceptance while absent old inserts remain subject to the 960-second writer floor.

New module: `true`.

- Required symbol: `pub(super) fn validate(`
- Required symbol: `fn managed_report(`
- Required symbol: `managed_timestamp_exception_preserves_future_and_marker_authority_boundaries`
- Invocation: `crates/buzz-relay/src/handlers/ingest.rs` → `validate`; exact call `timestamp::validate(&event, &auth, nostr::Timestamp::now().as_secs())?;`; behavior test `managed_old_report_still_enforces_signature_route_membership_and_archive`; verify `BUZZ_TEST_BIN_DIR="$PWD/target/debug" cargo test -p buzz-relay --lib managed_old_report_still_enforces_signature_route_membership_and_archive -- --ignored --test-threads=1`
- Verify: `cargo test -p buzz-relay --lib timestamp::tests -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" cargo test -p buzz-relay --lib handlers::ingest::postgres_tests::managed_delivery_tests:: -- --ignored --test-threads=1`

## `docs/superpowers/plans/2026-10-14-background-result-atomic-recovery.md`

Record the owner-approved atomic canonical recovery plan and preserved replica-floor safety contract.

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`

## `docs/superpowers/plans/2026-10-14-background-result-delivery.md`

Record original-session collection and durable managed publication implementation/verification plan.

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`

## `docs/superpowers/reports/2026-10-14-background-result-delivery.md`

Preserve implementation scope, actual validation receipts, mutation evidence and remaining runtime/acceptance limits.

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`

## `migrations/0056_managed_report_delivery.sql`

Create bounded canonical delivery bindings independent of event redaction with community write fencing and per-author completion-key uniqueness.

New module: `true`.

- Required symbol: `CREATE TABLE managed_deliveries`
- Required symbol: `PRIMARY KEY (community_id, author, delivery_key)`
- Required symbol: `SELECT attach_community_write_fence('managed_deliveries');`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" cargo test -p buzz-relay --lib handlers::ingest::postgres_tests::managed_delivery_tests:: -- --ignored --test-threads=1`

## `desktop/src/shared/api/relayReconnectReplay.test.mjs`

Keep channel replay lookback coupled to the shared relay timestamp helper and production ingest wiring plus the unchanged DB floor/clock margin.

New module: `false`.

- Required symbol: `channel replay lookback stays coupled to relay and DB source constants`
- Verify: `pnpm -C desktop test`

## `scripts/check-postgres-test-discovery.py`

Resolve conventional out-of-line Rust tests inside inline postgres_tests modules, retaining ancestor discovery context.

New module: `false`.

- Required symbol: `inline_ranges = module_ranges(source)`
- Verify: `bash scripts/test-postgres-test-discovery.sh`

## `scripts/test-postgres-test-discovery.sh`

Exercise nested file and directory Rust modules in the PostgreSQL discovery contract.

New module: `false`.

- Required symbol: `nested_out_of_line_recovery_test`
- Verify: `bash scripts/test-postgres-test-discovery.sh`

## `deny.toml`

Enforce vulnerability advisories including the removed unsafe-libyaml graph; record individual transitive maintenance-only exceptions with removal criteria in DEPENDENCY_SECURITY.md.

New module: `false`.

- Required symbol: `[advisories]`
- Verify: `cargo deny check`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -p test_yaml_dependencies.py -v`

## `crates/buzz-relay/src/mesh_boot.rs`

Preserve one in-flight demo echo receive across drain ticks; retain bounded explicit shutdown and the existing validated frame/Goodbye handling.

New module: `false`.

- Required symbol: `tokio::pin!(receive);`
- Required symbol: `demo_echo_keeps_inflight_read_across_ticks_and_still_drains`
- Verify: `cargo test -p buzz-relay -- --test-threads=1`

## `crates/buzz-acp/src/process_tree/codex_helpers.rs`

Confirm Codex code-mode host and exact Bash snapshot shell/parent identities with bounded ps probes; uncertain identities remain work. Use macOS ucomm for short snapshot shell/server kernel names while preserving the full-path service probe.

New module: `true`.

- Required symbol: `pub(super) fn classify(`
- Required symbol: `fn confirmed_hosts(`
- Required symbol: `pub(super) fn apply_snapshot_identities(`
- Invocation: `crates/buzz-acp/src/process_tree.rs` → `classify`; exact call `let tree = codex_helpers::classify(tree);`; behavior test `six_finished_threads_reuse_five_slots_with_late_codex_hosts`; verify `env -u BUZZ_ACP_SESSION_POLICY -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP cargo test -p buzz-acp resident_helper_tests -- --test-threads=1`
- Verify: `env -u BUZZ_ACP_SESSION_POLICY -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP cargo test -p buzz-acp resident_helper_tests -- --test-threads=1`

## `crates/buzz-acp/src/pool/resident_helper_tests.rs`

Real local ACP/process regressions for sixth-thread admission, nested child exit and original-session collection on macOS/Linux; Linux executable unlink and symlink boundaries. Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `true`.

- Required symbol: `six_finished_threads_reuse_five_slots_with_late_codex_hosts`
- Required symbol: `work_under_late_codex_host_holds_slot_until_child_exits`
- Required symbol: `work_mentioning_codex_host_in_arguments_still_holds_slot`
- Required symbol: `child_of_codex_host_queues_original_collection_before_releasing_slot`
- Required symbol: `symlink_named_codex_host_does_not_hide_ordinary_work`
- Required symbol: `unlinked_codex_host_does_not_retain_finished_turn`
- Verify: `env -u BUZZ_ACP_SESSION_POLICY -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP cargo test -p buzz-acp resident_helper_tests -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `desktop/src/features/messages/lib/useComposerAutofocus.ts`

Preserve focus inside an open dialog/popover when late composer readiness or draft changes trigger autofocus; fixes the reproduced huddle smoke CI blocker.

New module: `false`.

- Required symbol: `useComposerAutofocus`
- Required symbol: `active.closest('[role="dialog"]')`
- Invocation: `desktop/src/features/messages/ui/MessageComposer.tsx` → `useComposerAutofocus`; exact call `  useComposerAutofocus(richText.focus, effectiveDraftKey, composerDisabled);`; behavior test `late composer readiness preserves focus on a dialog switch; existing huddle assigns distinct agent voices E2E`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/useComposerAutofocus.test.mjs`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/useComposerAutofocus.test.mjs`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/features/messages/lib/useComposerAutofocus.test.mjs`

Production-hook regression for late readiness and draft-switch focus retention in dialogs, preserving ordinary navigation autofocus.

New module: `true`.

- Required symbol: `late composer readiness preserves focus on a dialog switch`
- Required symbol: `a draft change preserves focus on an open dialog button`
- Required symbol: `initial mount and navigation from an ordinary button still focus the editor`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/useComposerAutofocus.test.mjs`

## `desktop/tests/e2e/voice-note.spec.ts`

Bind duplicate voice-note playback assertions to stable emitted message IDs through live reordering; loop the short media fixture so natural completion cannot impersonate mutual pause.

New module: `false`.

- Required symbol: `const messageIds = await page.evaluate(`
- Required symbol: `const firstCard = page.locator(`
- Required symbol: `(player as HTMLAudioElement).loop = true;`
- Verify: `cd desktop && pnpm exec playwright test --project=smoke tests/e2e/voice-note.spec.ts --retries=0`

## `crates/buzz-acp/src/session_store.rs`

Community-and-agent isolated, lease-fenced session records with bounded locks, identity validation and channel-wide failure fencing. Preserve restart intent and bounded receipts through claims, commits and tombstones; revision-fence tombstones, enforce byte ceilings, and recover only accepted control metadata from provider quarantine. Lease-fenced automatic attempt cleanup preserves accepted restart policy; explicit scope/channel revocation clears it. Distinguish Alive/Dead/Unknown owner evidence while retaining conservative resume admission.

New module: `true`.

- Required symbol: `pub struct SessionStore`
- Required symbol: `pub fn claim(`
- Required symbol: `pub fn commit(`
- Required symbol: `fn store_namespace(`
- Required symbol: `poisoned_channels`
- Required symbol: `pub revision: Option<Uuid>`
- Required symbol: `pub pending_restart: Option<RestartIntent>`
- Required symbol: `pub control_receipts: Vec<ControlReceipt>`
- Required symbol: `if json.len() as u64 > MAX_RECORD_BYTES || !controls::valid_controls(record) {`
- Required symbol: `record.pending_restart = owned.pending_restart;`
- Required symbol: `enum InstanceLiveness`
- Required symbol: `fn instance_liveness(`
- Invocation: `crates/buzz-acp/src/session_store.rs` → `read_record_file`; exact call `read_record_file(&with_suffix(path, "stale"), namespace)?`; behavior test `quarantine_retains_control_metadata_without_recovering_provider_state`; verify `cargo test -p buzz-acp quarantine_retains_control_metadata_without_recovering_provider_state -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/session_store.rs` → `valid_controls`; exact call `controls::valid_controls(&record)`; behavior test `malformed_or_oversized_receipt_records_are_rejected`; verify `cargo test -p buzz-acp malformed_or_oversized_receipt_records_are_rejected -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/session_store.rs` → `valid_controls`; exact call `controls::valid_controls(record)`; behavior test `malformed_or_oversized_receipt_records_are_rejected`; verify `cargo test -p buzz-acp malformed_or_oversized_receipt_records_are_rejected -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/session_store.rs` → `tombstone`; exact call `self.tombstone(Some(&owned))`; behavior test `failed_attempt_discard_preserves_restart_until_matching_completion`; verify `cargo test -p buzz-acp failed_attempt_discard_preserves_restart_until_matching_completion -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/session_store.rs` → `instance_liveness`; exact call `self.instance_liveness(instance) != InstanceLiveness::Dead`; behavior test `instance_publication_cannot_be_reaped_before_locking`; verify `cargo test -p buzz-acp --lib instance_publication_cannot_be_reaped_before_locking -- --test-threads=1`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`
- Verify: `cargo test -p buzz-acp --lib instance_publication_cannot_be_reaped_before_locking -- --test-threads=1`

## `crates/buzz-acp/src/session_store_tests.rs`

Durable record ownership, concurrency, corruption and write-failure regressions. Capture revision in the existing tombstone expectation assertion.

New module: `true`.

- Required symbol: `busy_does_not_quarantine_owner_record`
- Required symbol: `quarantine_requires_matching_lease`
- Required symbol: `instance_publication_cannot_be_reaped_before_locking`
- Required symbol: `namespace_normalizes_url_spelling_without_aliasing_communities`
- Required symbol: `copied_foreign_namespace_record_is_not_resumable`
- Required symbol: `legacy_unscoped_records_are_not_adopted`
- Required symbol: `channel_enumeration_failure_poisons_unknown_scopes_until_restart`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/pool/session_continuity.rs`

Resume or create provider sessions through bounded claim/RPC/commit rounds and preserve delivery state. Retire status generations on the existing discard and depart paths. Keep acquisition fallback exhaustive for the store control error vocabulary. Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `true`.

- Required symbol: `pub(super) async fn acquire_session(`
- Required symbol: `pub(crate) fn resume_decision(`
- Required symbol: `pub(crate) fn insert_session_marker(`
- Required symbol: `Err(AcpError::DispatchRetired)`
- Invocation: `crates/buzz-acp/src/pool/session_continuity.rs` → `retire_status`; exact call `self.retire_status(scope);`; behavior test `new_session_status_precedes_the_prompt_and_spans_ordinary_turns`; verify `cargo test -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool/session_continuity.rs` → `retire_all_status`; exact call `self.retire_all_status();`; behavior test `worker_crash_retires_status_before_the_replacement_prompt`; verify `cargo test -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool/session_continuity.rs` → `ensure_dispatch_current`; exact call `ensure_dispatch_current(agent)?;`; behavior test `restart_during_delayed_acquisition_fences_old_dispatch`; verify `cargo test -p buzz-acp --lib restart_during_delayed_acquisition_fences_old_dispatch -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool/session_continuity.rs` → `ensure_dispatch_current`; exact call `if ensure_dispatch_current(agent).is_err() {`; behavior test `restart_during_failed_resume_never_calls_fallback_new`; verify `cargo test -p buzz-acp --lib restart_during_failed_resume_never_calls_fallback_new -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool/session_continuity.rs` → `Err`; exact call `return Err(err);
                        }
                        let code = resume_failure_code(&err);`; behavior test `retired_resume_transport_preserves_health_without_fallback_new`; verify `cargo test -p buzz-acp --lib retired_resume_transport_preserves_health_without_fallback_new -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool/session_continuity.rs` → `recycled_reason`; exact call `ctx.session_controls.recycled_reason(scope)`; behavior test `checked_retirement_neighbors_resume_or_start_new_with_recycled_reason`; verify `cargo test -p buzz-acp --lib checked_retirement_neighbors_resume_or_start_new_with_recycled_reason -- --test-threads=1`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`
- Verify: `cargo test -p buzz-acp --lib retired_successful_acquisition_keeps_worker_health_and_fences_old_state -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/pool/session_continuity_tests.rs`

Production acquisition, delivery delta, rotation, crash and idle-sleep persistence regressions. Supply default new control fields in existing legacy-record fixtures. Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `true`.

- Required symbol: `resume_result_after_rotate_is_dropped`
- Required symbol: `max_turns_survives_resume`
- Required symbol: `rotate_before_first_resume_discards_disk_session`
- Required symbol: `one_reply_window_keeps_first_mention_over_agent_reply`
- Required symbol: `idle_worker_transport_death_keeps_session_resumable`
- Required symbol: `acquisition_does_not_resume_another_community_or_agent`
- Required symbol: `acquisition_resumes_canonical_same_community_after_restart`
- Required symbol: `failed_channel_removal_during_sleep_never_resumes_after_readd`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/pool/first_mention.rs`

Bounded forward thread scan with a composite cursor and first-mention context pinning.

New module: `true`.

- Required symbol: `pub(crate) async fn find_first_mention`
- Required symbol: `pub(crate) fn pin_first_mention`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/pool/first_mention_tests.rs`

First-mention discovery across pages, timestamp ties and scan-budget exhaustion.

New module: `true`.

- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/acp/session_resume_tests.rs`

ACP resume capability, request parameter and failure-code checks.

New module: `true`.

- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/pool/real_resume_tests.rs`

Opt-in real-provider memory continuity after adapter process replacement.

New module: `true`.

- Required symbol: `real_claude_resume_remembers_previous_turn`
- Required symbol: `real_codex_resume_remembers_previous_turn`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/Cargo.toml`

Declare the filesystem-lock dependency used by durable session ownership.

New module: `false`.

- Required symbol: `fs2 = "0.4"`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/session_status.rs`

Session status runtime: private instance directory held by an instance lock, atomic bounded per-scope snapshots, generation locks with shared revocable guards, per-scope failure disabling, usage routing by notification session ID and harness-owned status directory env. Serialize live binding display tags through the shared envelope, refusing retired or replaced generations.

New module: `true`.

- Required symbol: `pub(crate) const SESSION_STATUS_DIR_ENV: &str = "BUZZ_SESSION_STATUS_DIR";`
- Required symbol: `pub(crate) struct SessionStatusRuntime`
- Required symbol: `pub(crate) fn begin(`
- Required symbol: `pub(crate) fn bind_existing(`
- Required symbol: `pub(crate) fn retire_channel(`
- Required symbol: `fn revoke_locked(`
- Required symbol: `pub(crate) fn record_usage_update(`
- Required symbol: `pub(crate) fn apply_status_dir_env(`
- Required symbol: `pub(crate) fn open_for_config(`
- Required symbol: `pub(crate) fn compacted(`
- Required symbol: `pub(crate) fn event_tags(`
- Required symbol: `let mut next = state.status.clone();`
- Invocation: `crates/buzz-acp/src/pool/restart_status.rs` → `event_tags`; exact call `.and_then(crate::session_status::StatusBinding::event_tags)`; behavior test `first_signed_status_carries_selected_history_actor_and_current_or_unknown_occupancy`; verify `cargo test -p buzz-acp --lib first_signed_status_carries_selected_history_actor_and_current_or_unknown_occupancy -- --test-threads=1`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/session_status_tests.rs`

Status runtime layout, lock, revocation, failed removal/write and wire usage-routing regressions.

New module: `true`.

- Required symbol: `open_holds_instance_lock_in_private_layout`
- Required symbol: `successor_generation_revokes_predecessor`
- Required symbol: `retire_releases_lock_and_late_callbacks_never_recreate`
- Required symbol: `failed_snapshot_removal_still_revokes_generation`
- Required symbol: `write_failure_disables_only_that_scope`
- Required symbol: `retire_waits_for_a_delayed_writer_then_removes_snapshot`
- Required symbol: `status_dir_env_is_owned_by_the_harness`
- Required symbol: `claude_usage_updates_route_by_their_own_session_id`
- Required symbol: `codex_usage_updates_route_by_their_own_session_id`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `desktop/src/features/messages/lib/sessionStatus.ts`

Strictly parse an agent's self-reported buzz-session and buzz-session-info tags, map the bounded state/reason vocabulary to fixed English copy with truthful exact, lower-bound and omitted-history wording, and attach one transition explanation per loaded (signer, channel, scope, transition) to its earliest message; other messages keep only their reported percentage. Valid primary-only status keeps a per-message explanation without inferred grouping or history. Validate matching bounded manual actor tags, include actor/state/reason in grouping and render fixed compaction/restart/worker-recycle copy.

New module: `true`.

- Required symbol: `export function parseSessionStatusTags(`
- Required symbol: `export function attachSessionStatuses(`
- Required symbol: `export const CONTEXT_HIGHLIGHT_PERCENT = 80;`
- Required symbol: `normalizePubkey(message.pubkey) !== message.signerPubkey`
- Required symbol: `if (key === null || owners.get(key) === message)`
- Required symbol: `const action = uniqueTag(tags, "buzz-session-action");`
- Required symbol: `action[2] === info.transitionId`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/sessionStatus.test.mjs src/features/messages/lib/formatTimelineMessages.sessionStatus.test.mjs src/features/messages/lib/messageRowEquality.test.mjs src/features/messages/ui/MessageSessionStatus.test.mjs`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/sessionControl.test.mjs src/features/messages/lib/sessionControlPermissions.test.mjs`

## `desktop/src/features/messages/lib/sessionStatus.test.mjs`

Parser and copy regressions: states, known and unknown reasons, resume failure codes, percent boundaries, malformed numbers, duplicate/conflicting tags, malformed supporting metadata and hostile reason strings. Retain neutral fallback for unknown reasons and cover implemented bounded control transition copy.

New module: `true`.

- Required symbol: `conflicting primary tags yield no status`
- Required symbol: `unknown but well-formed reasons get neutral copy without the raw string`
- Required symbol: `copy: bounded compact and restart reasons describe the actual transition`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/sessionStatus.test.mjs src/features/messages/lib/formatTimelineMessages.sessionStatus.test.mjs src/features/messages/lib/messageRowEquality.test.mjs src/features/messages/ui/MessageSessionStatus.test.mjs`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/sessionControl.test.mjs src/features/messages/lib/sessionControlPermissions.test.mjs`

## `desktop/src/features/messages/lib/formatTimelineMessages.sessionStatus.test.mjs`

Projection regressions through the production timeline formatter: agents, scopes, channels, interleaving, same-second order, deletion, pagination, edit overlays, signer/author identity and missing supporting metadata.

New module: `true`.

- Required symbol: `first message of a transition explains it; later ones keep only the percentage`
- Required symbol: `an edit cannot add or change status; the original signed tags win`
- Required symbol: `a relay-signed event attributed to an agent cannot claim its session state`
- Required symbol: `valid primary-only status keeps its explanation per message without counts`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/sessionStatus.test.mjs src/features/messages/lib/formatTimelineMessages.sessionStatus.test.mjs src/features/messages/lib/messageRowEquality.test.mjs src/features/messages/ui/MessageSessionStatus.test.mjs`

## `desktop/src/features/messages/ui/MessageSessionStatus.tsx`

Render the derived session status as a muted, noninteractive line with screen-reader pauses and warning emphasis plus readable text from 80%.

New module: `true`.

- Required symbol: `export function MessageSessionStatus(`
- Required symbol: `font-medium text-warning`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/sessionStatus.test.mjs src/features/messages/lib/formatTimelineMessages.sessionStatus.test.mjs src/features/messages/lib/messageRowEquality.test.mjs src/features/messages/ui/MessageSessionStatus.test.mjs`

## `desktop/src/features/messages/ui/MessageSessionStatus.test.mjs`

Rendered status line text, emphasis threshold, absence of tab stops, live regions or labels, and MessageRow memo invalidation when only the derived status moves.

New module: `true`.

- Required symbol: `MessageRow memo rerenders when only the derived status moves`
- Required symbol: `primary-only signed metadata renders its projected explanation with unknown occupancy`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/sessionStatus.test.mjs src/features/messages/lib/formatTimelineMessages.sessionStatus.test.mjs src/features/messages/lib/messageRowEquality.test.mjs src/features/messages/ui/MessageSessionStatus.test.mjs`

## `desktop/tests/e2e/session-status.spec.ts`

Mock-bridge smoke workflow for session status: new session with exact history, continuation rows at 79% and 80%, unknown context, thread navigation and one explanation per loaded transition. Capture the current quiet status paragraph without a message-row control trigger. Preserve all quiet-status/history/percentage/count assertions while verifying the removed row trigger and the thread-composer context agent/action/Back/Escape flow with natural hover/click.

New module: `true`.

- Required symbol: `session status explains each transition once and shows reported context`
- Required symbol: `quiet-status.png`
- Required symbol: `await row(timeline, root).hover();`
- Required symbol: `name: "Manage context"`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke session-controls.spec.ts session-status.spec.ts`

## `desktop/src/features/messages/lib/formatTimelineMessages.ts`

Attach the derived agent session status to formatted rows from each event's original signed tags. Derive compact controls from original events and raw reactions, and collect validated manual actors for batched profile reads.

New module: `false`.

- Required symbol: `export function collectSessionActorPubkeys(`
- Invocation: `desktop/src/features/messages/lib/formatTimelineMessages.ts` → `attachSessionStatuses`; exact call `return attachSessionStatuses(
    messages,
    visibleEvents,
    channel?.id ?? null,
    profiles,
  );`; behavior test `first message of a transition explains it; later ones keep only the percentage`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test --test-name-pattern="first message of a transition" src/features/messages/lib/formatTimelineMessages.sessionStatus.test.mjs`
- Invocation: `desktop/src/features/messages/lib/formatTimelineMessages.ts` → `attachSessionControls`; exact call `attachSessionControls(
    messages,
    visibleEvents,`; behavior test `formatter reads original signed kind9 body`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/sessionControl.test.mjs src/features/messages/lib/sessionControlPermissions.test.mjs`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/sessionStatus.test.mjs src/features/messages/lib/formatTimelineMessages.sessionStatus.test.mjs src/features/messages/lib/messageRowEquality.test.mjs src/features/messages/ui/MessageSessionStatus.test.mjs`
- Verify: `just desktop-test desktop-typecheck`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/sessionControl.test.mjs src/features/messages/lib/sessionControlPermissions.test.mjs`

## `desktop/src/features/messages/ui/MessageRow.tsx`

Render quiet session status and signed compact command/result rows with value-based memo invalidation; context actions live only in the thread composer.

New module: `false`.

- Invocation: `desktop/src/features/messages/ui/MessageRow.tsx` → `MessageSessionStatus`; exact call `<MessageSessionStatus status={message.sessionStatus} />`; behavior test `session status explains each transition once and shows reported context`; verify `CI=1 pnpm --dir desktop test:e2e:smoke session-controls.spec.ts session-status.spec.ts`
- Invocation: `desktop/src/features/messages/ui/MessageRow.tsx` → `sessionStatusEqual`; exact call `sessionStatusEqual(
      prev.message.sessionStatus,
      next.message.sessionStatus,
    )`; behavior test `MessageRow memo rerenders when only the derived status moves`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test --test-name-pattern="MessageRow memo rerenders" src/features/messages/ui/MessageSessionStatus.test.mjs`
- Invocation: `desktop/src/features/messages/ui/MessageRow.tsx` → `MessageSessionControlRow`; exact call `<MessageSessionControlRow`; behavior test `compact row renders actor, action, target and independent outcome`; verify `cd desktop && node --import ./test-jsdom-setup.mjs --import ./test-loader.mjs --experimental-strip-types --test-force-exit --test src/features/messages/lib/useSessionControlAction.jsdom-test.mjs src/features/messages/ui/MessageSessionControlRow.jsdom-test.mjs`
- Invocation: `desktop/src/features/messages/ui/MessageRow.tsx` → `sessionControlEqual`; exact call `sessionControlEqual(
      prev.message.sessionControl,
      next.message.sessionControl,
    )`; behavior test `MessageRow memo invalidates when only a raw correlated result changes`; verify `cd desktop && node --import ./test-jsdom-setup.mjs --import ./test-loader.mjs --experimental-strip-types --test-force-exit --test src/features/messages/lib/useSessionControlAction.jsdom-test.mjs src/features/messages/ui/MessageSessionControlRow.jsdom-test.mjs`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/sessionStatus.test.mjs src/features/messages/lib/formatTimelineMessages.sessionStatus.test.mjs src/features/messages/lib/messageRowEquality.test.mjs src/features/messages/ui/MessageSessionStatus.test.mjs`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke session-controls.spec.ts session-status.spec.ts`
- Verify: `just desktop-test desktop-typecheck`
- Verify: `cd desktop && node --import ./test-jsdom-setup.mjs --import ./test-loader.mjs --experimental-strip-types --test-force-exit --test src/features/messages/lib/useSessionControlAction.jsdom-test.mjs src/features/messages/ui/MessageSessionControlRow.jsdom-test.mjs`

## `desktop/src/features/messages/types.ts`

Declare the optional derived session status field of a timeline message. Carry derived compact per-target session-control presentation.

New module: `false`.

- Required symbol: `export type MessageSessionStatus = {`
- Required symbol: `sessionStatus?: MessageSessionStatus;`
- Required symbol: `sessionControl?: MessageSessionControl;`
- Verify: `just desktop-test desktop-typecheck`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/sessionControl.test.mjs src/features/messages/lib/sessionControlPermissions.test.mjs`

## `desktop/src/features/messages/lib/messageRowEquality.ts`

Value equality for the derived session status used by the MessageRow memo. Compare compact per-target control outcomes by value.

New module: `false`.

- Required symbol: `export function sessionStatusEqual(`
- Required symbol: `export function sessionControlEqual(`
- Required symbol: `target.expiresAt === other.expiresAt`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/sessionStatus.test.mjs src/features/messages/lib/formatTimelineMessages.sessionStatus.test.mjs src/features/messages/lib/messageRowEquality.test.mjs src/features/messages/ui/MessageSessionStatus.test.mjs`
- Verify: `cd desktop && node --import ./test-jsdom-setup.mjs --import ./test-loader.mjs --experimental-strip-types --test-force-exit --test src/features/messages/lib/useSessionControlAction.jsdom-test.mjs src/features/messages/ui/MessageSessionControlRow.jsdom-test.mjs`

## `desktop/src/features/messages/lib/messageRowEquality.test.mjs`

Session status value-equality regressions.

New module: `false`.

- Required symbol: `sessionStatusEqual: value equality over explanation, history and percent`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/sessionStatus.test.mjs src/features/messages/lib/formatTimelineMessages.sessionStatus.test.mjs src/features/messages/lib/messageRowEquality.test.mjs src/features/messages/ui/MessageSessionStatus.test.mjs`

## `crates/buzz-acp/src/pool/session_visibility.rs`

Adapt canonical session scope, acquisition result and final cold-start prompt history to the status runtime; worker-level retirement hooks. Validate display-scope provenance using configured policy and already resolved metadata, revoking untrusted Conversation generations. Map newly bounded manual/automatic compact, restart and worker-recycled reasons to shared status types.

New module: `true`.

- Required symbol: `pub(crate) fn status_scope(`
- Required symbol: `pub(crate) fn status_reason(`
- Required symbol: `pub(super) fn publish_session_start(`
- Required symbol: `pub(super) fn rebind_session_status(`
- Required symbol: `pub(super) fn retire_status(`
- Required symbol: `pub(super) fn retire_all_status(`
- Required symbol: `pub(super) fn history_coverage(`
- Required symbol: `pub(crate) fn retire_scope(`
- Required symbol: `pub(crate) fn retire_channel(`
- Required symbol: `const KNOWN_FAILURE_CODES: &[&str]`
- Required symbol: `pub(super) fn reconcile_session_status_scope(`
- Invocation: `crates/buzz-acp/src/pool/session_visibility.rs` → `begin_with_origin`; exact call `Some(origin) => runtime.begin_with_origin(`; behavior test `selected_restart_depths_reach_actual_prompt_and_supplied_ids`; verify `cargo test -p buzz-acp --lib selected_restart_depths_reach_actual_prompt_and_supplied_ids -- --test-threads=1`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/pool/session_visibility_tests.rs`

Production run_prompt_task status lifecycle, usage routing, history coverage and spawn/MCP environment regressions. Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `true`.

- Required symbol: `new_session_status_precedes_the_prompt_and_spans_ordinary_turns`
- Required symbol: `resumed_session_reports_idle_sleep`
- Required symbol: `failed_resume_reports_the_actual_fallback_reason`
- Required symbol: `interleaved_sessions_on_one_worker_keep_their_own_occupancy`
- Required symbol: `heartbeat_turn_never_touches_scope_status`
- Required symbol: `rotate_during_a_delayed_turn_retires_before_late_usage`
- Required symbol: `pool_sleep_retires_status_and_wake_starts_a_new_transition`
- Required symbol: `worker_crash_retires_status_before_the_replacement_prompt`
- Required symbol: `successor_acquisition_revokes_the_old_generation_before_its_prompt`
- Required symbol: `status_write_failure_never_fails_the_turn`
- Required symbol: `pinned_assignment_outside_the_window_counts_supplied_events_once`
- Required symbol: `counts_need_no_extra_history_walk`
- Required symbol: `every_spawn_path_and_managed_mcp_get_the_current_status_dir`
- Required symbol: `fallback_codes_outside_the_allowlist_become_unknown`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/runtime.rs`

Open the instance session status directory once and route it through the shared adapter spawn environment and prompt context.

New module: `false`.

- Required symbol: `pub(crate) fn enable_session_status(`
- Invocation: `crates/buzz-acp/src/runtime.rs` → `open_for_config`; exact call `let runtime = session_status::open_for_config(&self.config)?;`; behavior test `every_spawn_path_and_managed_mcp_get_the_current_status_dir`; verify `cargo test -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/runtime.rs` → `session_policy`; exact call `session_policy: config.session_policy,`; behavior test `configured_policy_reaches_prompt_status_context_without_base_prompt`; verify `cargo test -p buzz-acp --lib session_visibility::tests::policy -- --test-threads=1`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/pool/session_visibility_policy_tests.rs`

Production prompt/status provenance regressions for uncertain Conversation acquisition, recovered metadata, failed sibling-thread status, retained-session revocation and configured-policy forwarding.

New module: `true`.

- Required symbol: `conversation_status_requires_channel_policy_or_verified_dm`
- Required symbol: `uncertain_conversation_cannot_outlive_metadata_recovery_and_failed_thread_status`
- Required symbol: `retained_conversation_revokes_generation_before_rebind_or_late_updates`
- Required symbol: `configured_policy_reaches_prompt_status_context_without_base_prompt`
- Verify: `cargo test -p buzz-acp --lib session_visibility::tests::policy -- --test-threads=1`

## `crates/buzz-core/src/session_control.rs`

Exact kind-9 command grammar, typed terminal/pending reaction results with precedence, strict actor enrichment and freshness constants; no I/O.

New module: `true`.

- Required symbol: `pub enum RestartDepth`
- Required symbol: `pub enum SessionCommand`
- Required symbol: `pub struct ControlOrigin`
- Required symbol: `pub enum ControlFailure`
- Required symbol: `pub type ControlOutcome`
- Required symbol: `pub enum ControlResult`
- Required symbol: `pub struct SessionResult`
- Required symbol: `pub struct ControlAction`
- Required symbol: `pub fn control_is_fresh(`
- Verify: `cargo test -p buzz-core -- --test-threads=1`

## `crates/buzz-core/src/session_status_tests.rs`

Extract unchanged status regression coverage from production module and update schema-version assertions for v2.

New module: `true`.

- Required symbol: `fn snapshot_round_trips()`
- Required symbol: `fn snapshot_rejects_unknown_versions_and_bad_shapes()`
- Verify: `cargo test -p buzz-core -- --test-threads=1`

## `crates/buzz-core/tests/session_control.rs`

Behavioral contract regressions for exact grammar, strict result/action parsing, precedence, v1/v2 origin rules and shared fixtures. Bind the shared command corpus to the actual Rust parser.

New module: `true`.

- Required symbol: `fn command_grammar_is_exact()`
- Required symbol: `fn shared_reason_and_origin_fixture_corpus()`
- Required symbol: `fn snapshot_rejects_invalid_actor_and_automatic_actor()`
- Required symbol: `for case in cases["commands"].as_array().unwrap() {`
- Verify: `cargo test -p buzz-core -- --test-threads=1`
- Verify: `cargo test -p buzz-core --test session_control`

## `crates/buzz-core/tests/fixtures/session_controls.json`

Shared Rust/TypeScript fixture corpus for bounded reasons, canonical actor origins and pending/terminal precedence. Cover all exact command spellings, ordinary near-misses and the cross-language whitespace boundary.

New module: `true`.

- Required symbol: `"commands": [`
- Verify: `cargo test -p buzz-core -- --test-threads=1`
- Verify: `cargo test -p buzz-core --test session_control`

## `crates/buzz-acp/src/session_store_controls.rs`

Atomic restart tombstone/intent/Started receipts, bounded fresh deduplication, token-fenced completion and explicit owner-generation orphan reconciliation.

New module: `true`.

- Required symbol: `pub struct RestartIntent`
- Required symbol: `pub enum ControlAttempt`
- Required symbol: `pub fn request_restart(`
- Required symbol: `pub fn complete_restart(`
- Required symbol: `pub fn begin_control_attempt(`
- Required symbol: `pub fn finish_control_attempt(`
- Required symbol: `pub(crate) fn reconcile_control_attempt(`
- Required symbol: `pub(crate) fn retry_control_outcome(`
- Required symbol: `if expected.owner_instance != self.instance`
- Required symbol: `receipt.owner_generation != expected.owner_generation`
- Required symbol: `if let ControlReceiptState::Finished(prior) = receipt.state`
- Required symbol: `pub(crate) fn begin_admitted_control_with_receipt(`
- Required symbol: `if !control_is_fresh(created_at, admitted_at)`
- Invocation: `crates/buzz-acp/src/session_store_controls.rs` → `add_started`; exact call `add_started(
                &mut record,
                &intent.origin.command_event_id,
                now,
                self.instance,
            );`; behavior test `restart_started_receipt_is_written_with_tombstone_intent`; verify `cargo test -p buzz-acp restart_started_receipt_is_written_with_tombstone_intent -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/session_store_controls.rs` → `add_started`; exact call `observed = Some(add_started(
                &mut record,
                command_id,
                created_at,
                self.instance,
            ));`; behavior test `started_compact_redelivery_never_reexecutes`; verify `cargo test -p buzz-acp started_compact_redelivery_never_reexecutes -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/session_store_controls.rs` → `update_with_quarantine`; exact call `self.update_with_quarantine(path, decide, false)`; behavior test `restart_started_receipt_is_written_with_tombstone_intent`; verify `cargo test -p buzz-acp restart_started_receipt_is_written_with_tombstone_intent -- --test-threads=1`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/session_store_control_tests.rs`

Durable controls regression coverage including concurrency, atomic failures, legacy records, ABA, expiry/capacity, orphan fences and quarantine boundaries. Prove failed-attempt intent retention and explicit policy revocation with stale-lease rejection.

New module: `true`.

- Required symbol: `fn restart_intent_is_atomic()`
- Required symbol: `fn orphan_started_becomes_terminal_without_reexecution()`
- Required symbol: `fn quarantine_retains_control_metadata_without_recovering_provider_state()`
- Required symbol: `fn malformed_or_oversized_receipt_records_are_rejected()`
- Required symbol: `fn failed_attempt_discard_preserves_restart_until_matching_completion()`
- Required symbol: `fn explicit_scope_and_channel_revocation_clear_intent_but_keep_receipts()`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/session_controls.rs`

Authorized session-control admission, per-dispatch generation fences and bounded selected restart history

New module: `true`.

- Required symbol: `pub(crate) fn restart_intent`
- Required symbol: `pub(crate) fn complete_restart`
- Required symbol: `pub(crate) fn restart(`
- Required symbol: `pub(crate) fn retirement_pending`
- Required symbol: `pub(crate) fn capture(`
- Required symbol: `pub(crate) fn current(`
- Required symbol: `pub(crate) fn revoke_channel`
- Required symbol: `pub(crate) fn finish(`
- Required symbol: `self.runtime.epoch(&self.scope) == self.epoch`
- Required symbol: `restart_status_token: Some(intent.token)`
- Invocation: `crates/buzz-acp/src/session_controls.rs` → `request_restart_status`; exact call `let superseded = store.request_restart_status(&job.scope, &intent, status)?;`; behavior test `checked_retirement_supersedes_started_compact_without_a_compact_rpc`; verify `cargo test -p buzz-acp --lib checked_retirement_supersedes_started_compact_without_a_compact_rpc -- --test-threads=1`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/pool/session_history.rs`

Authorized session-control admission, per-dispatch generation fences and bounded selected restart history

New module: `true`.

- Required symbol: `HISTORY_BYTES: usize = 256 * 1024`
- Required symbol: `RESPONSE_BYTES: usize = 4 * 1024 * 1024`
- Required symbol: `MAX_PAGES: usize = 20`
- Required symbol: `MAX_CANDIDATES: usize = 10_000`
- Required symbol: `pub(super) async fn selected`
- Required symbol: `pub(super) fn enforce_render_budget`
- Required symbol: `while crate::queue::format_conversation_context(context, profiles).len() > HISTORY_BYTES {`
- Required symbol: `const RETAINED_HEAD_BYTES: usize = HISTORY_BYTES + MAX_CANDIDATES * 5;`
- Required symbol: `pub(super) fn supplied_mention`
- Invocation: `crates/buzz-acp/src/pool/session_history.rs` → `query_raw_bounded_until`; exact call `.query_raw_bounded_until(filters, RESPONSE_BYTES, deadline)`; behavior test `restart_recent_keeps_history_across_slow_reads_partial_failure_and_recovery`; verify `cargo test -p buzz-acp --lib restart_recent_keeps_history_across_slow_reads_partial_failure_and_recovery -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool/session_history.rs` → `saturating_sub`; exact call `if requested > MAX_CANDIDATES.saturating_sub(self.candidates) {`; behavior test `all_history_request_budget_reserves_tail_without_overfetching_candidates`; verify `cargo test -p buzz-acp --lib all_history_request_budget_reserves_tail_without_overfetching_candidates -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool/session_history.rs` → `saturating_sub`; exact call `rendered.len().saturating_sub(empty.len()) + 5`; behavior test `all_history_many_small_messages_fit_the_actual_rendered_budget`; verify `cargo test -p buzz-acp --lib all_history_many_small_messages_fit_the_actual_rendered_budget -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool/session_history.rs` → `sort_by`; exact call `events.sort_by(|a, b| {
                b["created_at"]
                    .as_u64()
                    .cmp(&a["created_at"].as_u64())
                    .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
            });`; behavior test `dm_same_second_all_fits_with_older_assignment_under_real_relay_cursor`; verify `cargo test -p buzz-acp --lib dm_same_second_all_fits_with_older_assignment_under_real_relay_cursor -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool/session_history.rs` → `key`; exact call `root.is_none() && key(&event) < key(prior)`; behavior test `dm_same_second_none_finds_earliest_assignment_under_real_relay_cursor`; verify `cargo test -p buzz-acp --lib dm_same_second_none_finds_earliest_assignment_under_real_relay_cursor -- --test-threads=1`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/pool/session_history_tests.rs`

Production-seam regressions for authorized session controls and bounded selected history

New module: `true`.

- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/pool/session_control_runtime_tests.rs`

Production-seam regressions for authorized session controls and bounded selected history Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `true`.

- Verify: `cargo test -p buzz-acp -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/pool/session_control_history_tests.rs`

Production-seam regressions for authorized session controls and bounded selected history

New module: `true`.

- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/filter.rs`

Authorized session-control admission, per-dispatch generation fences and bounded selected restart history

New module: `false`.

- Invocation: `crates/buzz-acp/src/filter.rs` → `reserved_raw`; exact call `crate::session_controls::reserved_raw(&raw)`; behavior test `stale_replay_does_not_overwrite_completed_ack`; verify `cargo test -p buzz-acp --lib stale_replay_does_not_overwrite_completed_ack -- --test-threads=1`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/relay.rs`

Authorized session-control admission, per-dispatch generation fences and bounded selected restart history

New module: `false`.

- Required symbol: `pub async fn query_raw_bounded`
- Required symbol: `chunk.len() > max_bytes.saturating_sub(bytes.len())`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/acp/terminate.rs`

Checked bounded Unix process-group and Windows contained-job retirement Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `true`.

- Required symbol: `pub enum TerminationReport`
- Required symbol: `pub async fn terminate(`
- Required symbol: `fn terminate_with_snapshot`
- Required symbol: `fn probe_alive`
- Invocation: `crates/buzz-acp/src/acp/terminate.rs` → `snapshot`; exact call `tokio::task::spawn_blocking(crate::process_tree::retirement::snapshot)`; behavior test `checked_termination_kills_snapshotted_separate_shell_group`; verify `cargo test -p buzz-acp --lib checked_termination_kills_snapshotted_separate_shell_group -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/acp/terminate.rs` → `verified_groups`; exact call `let mut groups = first.verified_groups(&last, pid, own_group);`; behavior test `checked_termination_kills_snapshotted_separate_shell_group`; verify `cargo test -p buzz-acp --lib checked_termination_kills_snapshotted_separate_shell_group -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/acp/terminate.rs` → `probe_alive`; exact call `probe_alive(killpg(Pid::from_raw(group), None))`; behavior test `permission_denied_process_group_is_alive`; verify `cargo test -p buzz-acp --lib permission_denied_process_group_is_alive -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/acp/terminate.rs` → `worker_present`; exact call `snapshot().await.filter(|tree| tree.worker_present(pid))`; behavior test `checked_termination_missing_worker_in_initial_snapshot_never_starts`; verify `cargo test -p buzz-acp --lib checked_termination_missing_worker_in_initial_snapshot_never_starts -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/acp/terminate.rs` → `covers_observed`; exact call `first.covers_observed(&last, pid, &self.observed_background_instances)`; behavior test `kimi_missing_observed_descendant_proof_blocks_a_clean_retirement`; verify `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/acp/terminate_tests.rs`

Actual subprocess retirement escalation and no-signal regressions

New module: `true`.

- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/process_tree/retirement.rs`

Bounded PGID snapshots and escaped-process identity validation Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `true`.

- Required symbol: `struct RetirementSnapshot`
- Required symbol: `fn verified_groups`
- Required symbol: `fn snapshot()`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/process_tree/retirement_tests.rs`

Process-group parsing, harness exclusion and PID identity regressions Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `true`.

- Verify: `cargo test -p buzz-acp -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/pool/session_retirement.rs`

Reserve every cached worker copy and retire only after foreign work releases

New module: `true`.

- Required symbol: `fn retirement_targets`
- Required symbol: `fn mark_retire_on_return`
- Required symbol: `fn take_retirement_target`
- Required symbol: `fn finish_worker_retirement`
- Invocation: `crates/buzz-acp/src/pool/session_retirement.rs` → `mark_departed`; exact call `match store.mark_departed(`; behavior test `checked_retirement_cannot_depart_neighbor_owned_by_a_newer_worker`; verify `cargo test -p buzz-acp --lib checked_retirement_cannot_depart_neighbor_owned_by_a_newer_worker -- --test-threads=1`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/pool/session_retirement_tests.rs`

Pool reservation, foreign foreground/background and task-return retirement regressions Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `true`.

- Verify: `cargo test -p buzz-acp -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/pool/session_retirement_runtime_tests.rs`

Controller/executor, supersession, delayed completion and neighbor lifecycle regressions

New module: `true`.

- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/session_controls/retirement.rs`

Controller-owned checked gates, all-target results and retained failure retry

New module: `true`.

- Required symbol: `fn plan_retirement`
- Required symbol: `fn retirement_report`
- Required symbol: `fn take_publications_up_to`
- Required symbol: `fn lose_retirement_evidence`
- Required symbol: `fn retirement_evidence_lost`
- Required symbol: `fn owns_retirement`
- Required symbol: `let status_current = entry.epoch == job.dispatch_epoch;`
- Invocation: `crates/buzz-acp/src/session_controls/retirement.rs` → `finish_control_attempt`; exact call `store.finish_control_attempt(scope, &job.origin.command_event_id, &outcome)`; behavior test `checked_retirement_terminal_store_failure_retains_gate_and_reports_uncertainty`; verify `cargo test -p buzz-acp --lib checked_retirement_terminal_store_failure_retains_gate_and_reports_uncertainty -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/session_controls/retirement.rs` → `status_ready`; exact call `if release && outcome.is_ok() && status_current {
            Self::status_ready(state, store, scope);`; behavior test `unavailable_cancel_during_retirement_cannot_recreate_the_followup_after_clean_exit`; verify `cargo test -p buzz-acp --lib unavailable_cancel_during_retirement_cannot_recreate_the_followup_after_clean_exit -- --test-threads=1`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/session_controls/publish.rs`

Signed bounded restart and compaction result publication

New module: `true`.

- Required symbol: `fn build_result`
- Required symbol: `async fn publish_result`
- Invocation: `crates/buzz-acp/src/session_controls/publish.rs` → `submit_event_once`; exact call `rest.submit_event_once(&event)`; behavior test `control_result_publication_has_three_total_signed_attempts`; verify `cargo test -p buzz-acp --lib control_result_publication_has_three_total_signed_attempts -- --test-threads=1`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/session_store_orphans.rs`

Dead-owner durable Started receipt reconciliation at startup Retain autonomous revisit work for live or uncertain foreign Started owners.

New module: `true`.

- Required symbol: `fn sweep_orphan_controls`
- Required symbol: `pub revisit: bool`
- Required symbol: `progress.revisit = true;`
- Invocation: `crates/buzz-acp/src/session_store_orphans.rs` → `canonical_key`; exact call `let Some(path) = canonical_key(&entry_path) else {`; behavior test `startup_sweep_recovers_stale_only_started_receipt_and_deduplicates_record_key`; verify `cargo test -p buzz-acp --lib startup_sweep_recovers_stale_only_started_receipt_and_deduplicates_record_key -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/session_store_orphans.rs` → `instance_liveness`; exact call `if self.instance_liveness(receipt.owner_instance)
                                != InstanceLiveness::Dead
                            {`; behavior test `startup_sweep_terminalizes_dead_started_controls_but_preserves_live_owner`; verify `cargo test -p buzz-acp --lib startup_sweep_terminalizes_dead_started_controls_but_preserves_live_owner -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/session_store_orphans.rs` → `instance_liveness`; exact call `match self.instance_liveness(receipt.owner_instance) {`; behavior test `orphan_registry_unknown_revisits_after_lock_release_without_external_wake`; verify `cargo test -p buzz-acp --lib orphan_registry_unknown_revisits_after_lock_release_without_external_wake -- --test-threads=1`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`
- Verify: `cargo test -p buzz-acp --lib orphan_registry_unknown_revisits_after_lock_release_without_external_wake -- --test-threads=1`

## `crates/buzz-acp/src/worker_retirement.rs`

Nonblocking checked worker replacement and retained ownership on uncertainty Explicitly drain parked and cancellation-returned original ownership at orderly shutdown using retained checked evidence. Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `true`.

- Required symbol: `struct RetirementReturn`
- Required symbol: `struct RetirementGuard`
- Required symbol: `fn drive(`
- Required symbol: `fn finish(`
- Required symbol: `pub(crate) async fn shutdown(`
- Invocation: `crates/buzz-acp/src/worker_retirement.rs` → `terminate`; exact call `Some(agent) => agent.acp.terminate().await,`; behavior test `checked_retirement_executor_waits_for_verified_kill_and_excludes_refill`; verify `cargo test -p buzz-acp --lib checked_retirement_executor_waits_for_verified_kill_and_excludes_refill -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/worker_retirement.rs` → `retirement_report`; exact call `controls.retirement_report(result.index, TerminationReport::Clean);`; behavior test `checked_retirement_executor_waits_for_verified_kill_and_excludes_refill`; verify `cargo test -p buzz-acp --lib checked_retirement_executor_waits_for_verified_kill_and_excludes_refill -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/worker_retirement.rs` → `prepare_retired_neighbors`; exact call `agent.prepare_retired_neighbors(&scopes, &controls)`; behavior test `checked_retirement_cannot_depart_neighbor_owned_by_a_newer_worker`; verify `cargo test -p buzz-acp --lib checked_retirement_cannot_depart_neighbor_owned_by_a_newer_worker -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/worker_retirement.rs` → `lose_retirement_evidence`; exact call `controls.lose_retirement_evidence(result.index);`; behavior test `retirement_foreground_panic_cannot_attest_replacement_or_forget_original_on_retry`; verify `cargo test -p buzz-acp --lib retirement_foreground_panic_cannot_attest_replacement_or_forget_original_on_retry -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `shutdown`; exact call `worker_retirement::shutdown(`; behavior test `retirement_shutdown_retries_original_parked_after_snapshot_failure`; verify `cargo test -p buzz-acp --lib retirement_shutdown_retries_original_parked_after_snapshot_failure -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/worker_retirement.rs` → `terminate`; exact call `let report = agent.acp.terminate().await;`; behavior test `retirement_shutdown_drains_original_returned_by_aborted_guard`; verify `cargo test -p buzz-acp --lib retirement_shutdown_drains_original_returned_by_aborted_guard -- --test-threads=1`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`
- Verify: `cargo test -p buzz-acp --lib retirement_shutdown_retries_original_parked_after_snapshot_failure -- --test-threads=1`
- Verify: `cargo test -p buzz-acp --lib retirement_shutdown_drains_original_returned_by_aborted_guard -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/session_controls/orphans.rs`

Idle-waking bounded-backoff orphan reconciliation with partial durable publication Back off live/unknown revisits without declaring startup reconciliation complete.

New module: `true`.

- Required symbol: `struct OrphanReconciliation`
- Required symbol: `fn sweep_orphans`
- Required symbol: `fn drive(`
- Required symbol: `fn wake(`
- Required symbol: `controls.has_receipt_writes()`
- Required symbol: `revisit: progress.revisit,`
- Required symbol: `} else if progress.revisit {`
- Invocation: `crates/buzz-acp/src/session_controls/orphans.rs` → `sweep_orphan_controls`; exact call `store.sweep_orphan_controls(capacity.saturating_sub(local.completed))?`; behavior test `startup_sweep_keeps_healthy_and_committed_prefix_outcomes_across_bad_records`; verify `cargo test -p buzz-acp --lib startup_sweep_keeps_healthy_and_committed_prefix_outcomes_across_bad_records -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/session_controls/orphans.rs` → `publish`; exact call `state.publish(Publication {`; behavior test `startup_sweep_keeps_healthy_and_committed_prefix_outcomes_across_bad_records`; verify `cargo test -p buzz-acp --lib startup_sweep_keeps_healthy_and_committed_prefix_outcomes_across_bad_records -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/session_controls/orphans.rs` → `retry_receipt_writes`; exact call `self.retry_receipt_writes(capacity)`; behavior test `compact_terminal_write_retries_after_repair_without_reexecuting_effect`; verify `cargo test --locked -p buzz-acp --lib compact_terminal_write_retries_after_repair_without_reexecuting_effect -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/session_controls/orphans.rs` → `sweep_orphans`; exact call `controls.sweep_orphans(capacity.min(128))`; behavior test `orphan_alive_foreign_owner_is_revisited_after_it_dies`; verify `cargo test -p buzz-acp --lib orphan_alive_foreign_owner_is_revisited_after_it_dies -- --test-threads=1`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`
- Verify: `cargo test -p buzz-acp --lib orphan_alive_foreign_owner_is_revisited_after_it_dies -- --test-threads=1`

## `crates/buzz-acp/src/pool/session_retirement_review_tests.rs`

Actual executor no-resume neighbor race and panic escaped-group false-Clean regressions Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `true`.

- Verify: `cargo test -p buzz-acp -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/acp/windows_retirement_tests.rs`

Hosted Windows actual ACP containment and checked termination regressions

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`

## `crates/buzz-acp/src/pool/portable_retirement_tests.rs`

Portable actual fake-ACP all-target controller/store/foreign-return gates Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `scripts/check-acp-retirement.py`

Native hosted ACP retirement gate with test discovery and exact execution counts

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`

## `Cargo.toml`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `false`.

- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `crates/buzz-agent/Cargo.toml`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `false`.

- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `crates/buzz-agent/src/hints.rs`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `false`.

- Required symbol: `yaml_serde::from_str(yaml_block).ok()?`
- Invocation: `crates/buzz-agent/src/hints.rs` → `from_str`; exact call `yaml_serde::from_str(yaml_block).ok()?`; behavior test `hints::yaml_tests::skill_yaml_preserves_words_tags_aliases_and_multiline_text`; verify `cargo test --locked -p buzz-agent --lib hints::yaml_tests`
- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`
- Verify: `cargo test --locked -p buzz-agent --lib hints::yaml_tests`

## `crates/buzz-agent/src/hints_yaml_tests.rs`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `true`.

- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `crates/buzz-backend-kubernetes/src/client.rs`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `false`.

- Required symbol: `let config = config_for(context).await?;`
- Invocation: `crates/buzz-backend-kubernetes/src/client.rs` → `config_for`; exact call `let config = config_for(context).await?;`; behavior test `client::yaml_tests::kubeconfig_connect_and_exec_lookup_use_the_selected_context`; verify `cargo test --locked -p buzz-backend-kubernetes`
- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`
- Verify: `cargo test --locked -p buzz-backend-kubernetes`

## `crates/buzz-backend-kubernetes/src/client_yaml_tests.rs`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `true`.

- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `crates/buzz-backend-kubernetes/src/cluster.rs`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `false`.

- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `crates/buzz-backend-kubernetes/src/gc.rs`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `false`.

- Required symbol: `let age_nanos = now_nanos - created.0.as_nanosecond();`
- Invocation: `crates/buzz-backend-kubernetes/src/gc.rs` → `as_nanosecond`; exact call `let age_nanos = now_nanos - created.0.as_nanosecond();`; behavior test `gc::tests::age_gate_boundary_is_exact`; verify `cargo test --locked -p buzz-backend-kubernetes`
- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`
- Verify: `cargo test --locked -p buzz-backend-kubernetes`

## `crates/buzz-backend-kubernetes/src/observe.rs`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `false`.

- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `crates/buzz-backend-kubernetes/src/reconcile.rs`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `false`.

- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `crates/buzz-persona/Cargo.toml`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `false`.

- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `crates/buzz-persona/src/persona.rs`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `false`.

- Required symbol: `yaml_serde::from_str(fm_str)?`
- Invocation: `crates/buzz-persona/src/persona.rs` → `from_str`; exact call `yaml_serde::from_str(fm_str)?`; behavior test `persona_yaml_preserves_scalar_types_aliases_and_multiline_unicode`; verify `cargo test --locked -p buzz-persona --test yaml_compatibility`
- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`
- Verify: `cargo test --locked -p buzz-persona --test yaml_compatibility`

## `crates/buzz-persona/src/validate.rs`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `false`.

- Required symbol: `yaml_serde::from_str(fm_str)`
- Invocation: `crates/buzz-persona/src/validate.rs` → `from_str`; exact call `yaml_serde::from_str(fm_str)`; behavior test `pack_validation_preserves_skill_name_warnings_with_unknown_yaml_values`; verify `cargo test --locked -p buzz-persona --test yaml_compatibility`
- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`
- Verify: `cargo test --locked -p buzz-persona --test yaml_compatibility`

## `crates/buzz-persona/tests/yaml_compatibility.rs`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `true`.

- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `crates/buzz-workflow/Cargo.toml`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `false`.

- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `crates/buzz-workflow/src/error.rs`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `false`.

- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `crates/buzz-workflow/src/schema.rs`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `false`.

- Required symbol: `yaml_serde::from_str(yaml)?`
- Invocation: `crates/buzz-workflow/src/schema.rs` → `from_str`; exact call `yaml_serde::from_str(yaml)?`; behavior test `schema::yaml_tests::workflow_yaml_preserves_trigger_aliases_and_message_text`; verify `cargo test --locked -p buzz-workflow --lib schema::yaml_tests`
- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`
- Verify: `cargo test --locked -p buzz-workflow --lib schema::yaml_tests`

## `crates/buzz-workflow/src/schema_yaml_tests.rs`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `true`.

- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `desktop/src-tauri/src/commands/workflows.rs`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `false`.

- Required symbol: `yaml_serde::from_str::<Value>(yaml)`
- Invocation: `desktop/src-tauri/src/commands/workflows.rs` → `from_str`; exact call `yaml_serde::from_str::<Value>(yaml)`; behavior test `commands::workflows::tests::workflow_yaml_json_projection_preserves_words_and_fallbacks`; verify `cargo test --locked --manifest-path desktop/src-tauri/Cargo.toml --lib commands::workflows::tests::workflow_yaml_json_projection_preserves_words_and_fallbacks`
- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`
- Verify: `cargo test --locked --manifest-path desktop/src-tauri/Cargo.toml --lib commands::workflows::tests::workflow_yaml_json_projection_preserves_words_and_fallbacks`

## `desktop/src-tauri/src/commands/workflows_tests.rs`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `false`.

- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `desktop/src-tauri/src/managed_agents/config_bridge/goose.rs`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `false`.

- Required symbol: `yaml_serde::from_str(yaml_str).ok()?`
- Invocation: `desktop/src-tauri/src/managed_agents/config_bridge/goose.rs` → `from_str`; exact call `yaml_serde::from_str(yaml_str).ok()?`; behavior test `managed_agents::config_bridge::goose::yaml_tests::goose_yaml_preserves_nested_providers_and_ignores_unknown_tagged_fields`; verify `cargo test --locked --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::config_bridge::goose::yaml_tests`
- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`
- Verify: `cargo test --locked --manifest-path desktop/src-tauri/Cargo.toml --lib managed_agents::config_bridge::goose::yaml_tests`

## `desktop/src-tauri/src/managed_agents/config_bridge/goose_yaml_tests.rs`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `true`.

- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `desktop/src-tauri/src/migration_yaml_tests.rs`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `true`.

- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `docs/fork/YAML_COMPATIBILITY.md`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `true`.

- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `scripts/fork/tests/test_yaml_dependencies.py`

Preserve YAML consumer compatibility while removing the unmaintained dependency graph (issue19).

New module: `true`.

- Verify: `just ci`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `AGENTS.md`

Fork rule: public text is in English and carries no internal workspace details Fork screenshot publication requires explicit paired repository/remote selection and normal pushes with DCO.

New module: `false`.

- Required symbol: `## Fork: public text is in English`
- Verify: `python3 scripts/fork/sync.py validate`

## `crates/buzz-acp/src/acp/compaction.rs`

Bound compaction IDs and typed lifecycles by provider session and local generation; terminal enrichment cannot satisfy a later manual operation. Retain bounded creation advertisements under the current in-flight token and adopt only the successfully returned session.

New module: `true`.

- Required symbol: `if self.entries.len() >= MAX_IDS`
- Required symbol: `after: self.sequence,`
- Required symbol: `.is_some_and(|route| route.generation == generation)`
- Required symbol: `.filter(|route| route.adoptable_binding)`
- Required symbol: `route.adoptable_binding = false;`
- Required symbol: `        if self
            .routes
            .get(session)
            .is_none_or(|route| route.generation != generation)
        {
            return;
        }`
- Required symbol: `pub(super) fn prepare_new(`
- Required symbol: `pub(super) fn complete_new(`
- Required symbol: `pending.advertisements.len() < MAX_IDS`
- Required symbol: `bytes.saturating_add(session.len()) <= MAX_CREATION_BYTES`
- Required symbol: `self.creation = None;`
- Required symbol: `.is_none_or(|pending| pending.generation != generation)`
- Invocation: `crates/buzz-acp/src/acp/compaction.rs` → `retain`; exact call `self.entries
            .retain(|entry| sessions.contains(&entry.session));`; behavior test `compaction_standard_dispatch_never_guesses_success`; verify `cargo test --locked -p buzz-acp --lib compaction_standard_dispatch_never_guesses_success -- --test-threads=1`
- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/acp/compaction_tests.rs`

Exercise actual initialize frames, standard compaction dispatch, tracker bounds, and status transition ownership.

New module: `true`.

- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/pool/compaction.rs`

Acquire only the exact existing provider session, persist Started before a single compact prompt, and return separately typed control health/results. Reset stale worker observer context to the admitted channel, existing provider session and command identity before resume or compact I/O. Reject disk-only targets before lazy wake when provider resume is disabled, while preserving existing in-memory compaction.

New module: `true`.

- Required symbol: `pub(crate) struct ExistingSession`
- Required symbol: `pub(crate) async fn acquire_existing_session_for_compaction(`
- Required symbol: `pub(crate) async fn run_compaction_task(`
- Required symbol: `if live.lease != *lease || live.session_id != *session_id`
- Required symbol: `Ok(_) => terminal,`
- Required symbol: `let execution = tokio::time::timeout_at(job.deadline, async {`
- Required symbol: `job.job.origin.command_event_id.clone(),`
- Required symbol: `allow_disk_resume: bool,`
- Required symbol: `if !allow_disk_resume {`
- Invocation: `crates/buzz-acp/src/pool/compaction.rs` → `begin_compaction_effect`; exact call `ctx.session_controls.begin_compaction_effect(&job)`; behavior test `compact_exact_prompt_and_durable_started_preserve_delivery_and_turns`; verify `cargo test --locked -p buzz-acp --lib compact_exact_prompt_and_durable_started_preserve_delivery_and_turns -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool/compaction.rs` → `session_resume`; exact call `.session_resume(session_id, &ctx.cwd, ctx.mcp_servers.clone(), None, None)`; behavior test `compact_never_calls_session_new_memory_or_disk_success`; verify `cargo test --locked -p buzz-acp --lib compact_never_calls_session_new_memory_or_disk_success -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool/compaction.rs` → `session_prompt_blocks_with_idle_timeout`; exact call `agent.acp.session_prompt_blocks_with_idle_timeout(&session.session_id, &["/compact"], ctx.idle_timeout, ctx.max_turn_duration)`; behavior test `compact_exact_prompt_and_durable_started_preserve_delivery_and_turns`; verify `cargo test --locked -p buzz-acp --lib compact_exact_prompt_and_durable_started_preserve_delivery_and_turns -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/pool/compaction.rs` → `set_observer_context`; exact call `agent.acp.set_observer_context(observer::context_for_turn(`; behavior test `compact_observer_routes_resume_prompt_and_updates_to_admitted_scope`; verify `cargo test --locked -p buzz-acp compact_ -- --test-threads=1`
- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/pool/compaction_tests.rs`

Exercise existing-only acquisition and actual compact RPC, durable markers, signed result submission and failure/redelivery behavior. Regress actual disk-resume observer envelope routing from stale prior scope.

New module: `true`.

- Required symbol: `compact_observer_routes_resume_prompt_and_updates_to_admitted_scope`
- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/pool/compaction_runtime_tests.rs`

Exercise authorized controller admission, exact-owner scheduling, lazy wake, collection protection, bounded pending work and lifecycle termination. Regress actual cached-worker control-dispatch observer envelope routing. Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `true`.

- Required symbol: `compact_cached_worker_replaces_previous_observer_scope`
- Required symbol: `mod resume_policy_tests;`
- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/pool/compaction_resume_policy_tests.rs`

Bind authorized target selection and scheduler wake to disabled disk resume, durable refusal replay, enabled disk resume and successful no-store in-memory compaction.

New module: `true`.

- Required symbol: `compact_resume_disabled_disk_refusal_and_replay_never_wake`
- Required symbol: `compact_resume_enabled_disk_wakes_and_resumes_without_new_session`
- Required symbol: `compact_resume_disabled_memory_owner_still_compacts_without_resume`
- Verify: `cargo test --locked -p buzz-acp --lib compact_resume_ -- --test-threads=1`

## `crates/buzz-acp/src/session_controls/compaction.rs`

Keep pending manual compactions, bounded terminal outcomes and durable redelivery independent of disposable pools.

New module: `true`.

- Required symbol: `pub(crate) struct AdmittedSessionControl`
- Required symbol: `if scopes.len() + state.receipt_writes.len() >= 128`
- Required symbol: `if self.outcomes.len() >= 128`
- Required symbol: `if pending_scopes + state.receipt_writes.len() >= 128`
- Required symbol: `pending.started = Some(receipt);`
- Required symbol: `Err(_) => true,`
- Required symbol: `outcome = Err(ControlFailure::StoreUnavailable);`
- Required symbol: `COMPACT_QUEUE_WAIT`
- Required symbol: `COMPACT_EXECUTION_WAIT`
- Required symbol: `pub(crate) fn report_compaction_wait(`
- Invocation: `crates/buzz-acp/src/session_controls/compaction.rs` → `begin_admitted_control_with_receipt`; exact call `.begin_admitted_control_with_receipt(
                &job.job.scope,
                &job.job.origin.command_event_id,
                job.job.event.created_at.as_secs(),
                job.job.admitted_at,
            )`; behavior test `compact_exact_prompt_and_durable_started_preserve_delivery_and_turns`; verify `cargo test --locked -p buzz-acp --lib compact_exact_prompt_and_durable_started_preserve_delivery_and_turns -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/session_controls/compaction.rs` → `begin_admitted_control_with_receipt`; exact call `.begin_admitted_control_with_receipt(
                &job.job.scope,
                &job.job.origin.command_event_id,
                job.job.event.created_at.as_secs(),
                job.job.admitted_at,
            )`; behavior test `compact_finalized_started_survives_both_locked_finalization_calls`; verify `cargo test --locked -p buzz-acp --lib compact_finalized_started_survives_both_locked_finalization_calls -- --test-threads=1`
- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/control_tasks.rs`

Dispatch typed control tasks through shared worker ownership and retirement, with separate result accounting and completion activity. Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `true`.

- Required symbol: `queue.has_flushable_work()
        || !controls.pending_compactions().is_empty()
        || !controls.pending_statuses().is_empty()`
- Required symbol: `} else if tokio::time::Instant::now() >= job.deadline {`
- Required symbol: `*last_activity = tokio::time::Instant::now();`
- Invocation: `crates/buzz-acp/src/control_tasks.rs` → `run_compaction_task`; exact call `run_compaction_task(agent, job, ctx_clone, control_rx).await`; behavior test `compact_waits_for_exact_busy_owner_without_cancel_or_fork`; verify `cargo test --locked -p buzz-acp --lib compact_waits_for_exact_busy_owner_without_cancel_or_fork -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/control_tasks.rs` → `run_compaction_task`; exact call `run_compaction_task(agent, job, ctx_clone, control_rx).await`; behavior test `compact_valid_admission_crosses_wire_expiry_while_waiting_for_owner`; verify `cargo test --locked -p buzz-acp --lib compact_valid_admission_crosses_wire_expiry_while_waiting_for_owner -- --test-threads=1`
- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/session_controls/receipt_writes.rs`

Bound known-finalized local terminal receipt writes, fence persistence by exact owner/generation, and reuse reconciliation backoff without repeating provider effects. Use exact Started evidence captured before effects; retaining repair never requires a later store read.

New module: `true`.

- Required symbol: `pub(super) fn retain_receipt_write(`
- Required symbol: `pub(super) fn retry_receipt_writes(`
- Required symbol: `receipt: ControlReceipt,`
- Required symbol: `receipt.owner_instance != store.instance()`
- Invocation: `crates/buzz-acp/src/session_controls/receipt_writes.rs` → `retry_control_outcome`; exact call `store.retry_control_outcome(&entry.scope, &entry.receipt, &entry.outcome)`; behavior test `compact_terminal_write_retries_after_repair_without_reexecuting_effect`; verify `cargo test --locked -p buzz-acp --lib compact_terminal_write_retries_after_repair_without_reexecuting_effect -- --test-threads=1`
- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/acp/windows_job.rs`

Suspended Windows worker containment with independent zero-job and direct-child retirement proof

New module: `true`.

- Required symbol: `struct AssignQueryJob`
- Required symbol: `pub(super) fn spawn`
- Required symbol: `async fn terminate_windows`
- Required symbol: `query_process_id_list`
- Required symbol: `graceful_deadline`
- Required symbol: `verify_deadline`
- Required symbol: `async fn terminate_windows_with_query`
- Invocation: `crates/buzz-acp/src/acp.rs` → `spawn`; exact call `let spawned = windows_job::spawn(cmd);`; behavior test `checked_termination_windows_harness_drop_kills_detached_tree`; verify `cargo test -p buzz-acp --lib checked_termination_windows -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/acp/terminate.rs` → `terminate_windows`; exact call `self.terminate_windows().await`; behavior test `checked_termination_windows_detached_grandchild_blocks_clean_after_parent_exit`; verify `cargo test -p buzz-acp --lib checked_termination_windows -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/acp/windows_job.rs` → `terminate_windows_with_query`; exact call `self.terminate_windows_with_query(|job| {`; behavior test `checked_termination_windows_empty_query_requires_parent_reap`; verify `python scripts/check-acp-retirement.py`
- Verify: `cargo test -p buzz-acp --lib checked_termination_windows -- --test-threads=1`

## `desktop/src/features/messages/lib/sessionControl.ts`

Parse the shared exact grammar with Rust whitespace parity; project original signed command requests and independent raw target-signed bounded outcomes.

New module: `true`.

- Required symbol: `export function parseSessionCommand(`
- Required symbol: `export function attachSessionControls(`
- Required symbol: `event.pubkey === target`
- Required symbol: `parseSessionCommand(event.content)`
- Required symbol: `const EXTERNAL_WHITESPACE = /^\p{White_Space}+|\p{White_Space}+$/gu;`
- Required symbol: `content.replace(EXTERNAL_WHITESPACE, "")`
- Required symbol: `event.tags.some((tag) => tag[0] === "buzz-session-result")`
- Required symbol: `expiresAt?: number;`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/sessionControl.test.mjs src/features/messages/lib/sessionControlPermissions.test.mjs`

## `desktop/src/features/messages/lib/sessionControlPermissions.ts`

Authorize exact current scope from verified directory policy, owner/sibling coordinates, channel type and membership; preserve DM restrictions and remote cross-owner admission without local inventory authority.

New module: `true`.

- Required symbol: `export function canManageSession(`
- Required symbol: `!agent.channelIds.includes(channelId)`
- Required symbol: `if (channel.channelType === "dm") return false;`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/sessionControl.test.mjs src/features/messages/lib/sessionControlPermissions.test.mjs`

## `desktop/src/features/messages/lib/useSessionControlAction.ts`

Capture exact target, thread, relay and signer, refresh verified permission, fence stale async actions and publish one exact kind-9 control without composer or lifecycle mutations. Commit a prepared composer audience callback only after successful current-scope publication.

New module: `true`.

- Required symbol: `export function useSessionControlAction(`
- Required symbol: `if (!isCurrent()) return false;`
- Required symbol: `currentKey.current.generation === capturedGeneration`
- Invocation: `desktop/src/features/messages/lib/useSessionControlAction.ts` → `onSent`; exact call `onSent?.();`; behavior test `publishes the audience callback only after a successful current-scope send`; verify `cd desktop && node --import ./test-jsdom-setup.mjs --import ./test-loader.mjs --experimental-strip-types --test-force-exit --test src/features/messages/lib/useSessionControlAction.jsdom-test.mjs`
- Verify: `cd desktop && node --import ./test-jsdom-setup.mjs --import ./test-loader.mjs --experimental-strip-types --test-force-exit --test src/features/messages/lib/useSessionControlAction.jsdom-test.mjs src/features/messages/ui/MessageSessionControlRow.jsdom-test.mjs`

## `desktop/src/features/messages/lib/sessionControl.test.mjs`

Production formatter and shared grammar corpus regressions for original versus edited bodies, raw target results, conflicts, multiple targets, expiry, actor metadata and known bot membership.

New module: `true`.

- Required symbol: `shared exact grammar matches the actual core parser whitespace corpus`
- Required symbol: `formatter reads original signed kind9 body`
- Required symbol: `results require exact target raw signer`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/sessionControl.test.mjs src/features/messages/lib/sessionControlPermissions.test.mjs`

## `desktop/src/features/messages/lib/sessionControlPermissions.test.mjs`

Permission matrix covers owner/sibling, allowlist/anyone, DM restrictions, missing and mismatched scope evidence and local inventory denial.

New module: `true`.

- Required symbol: `verified scope and exact policy evidence admit owner`
- Required symbol: `unknown, stale local inventory`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/sessionControl.test.mjs src/features/messages/lib/sessionControlPermissions.test.mjs`

## `desktop/src/features/messages/lib/useSessionControlAction.jsdom-test.mjs`

Exercise the production hook for revocation, query/publication failures, scope switches, round trips, unmount, duplicate clicks and actual channel profile batching.

New module: `true`.

- Required symbol: `switching away and back cannot revive an old action`
- Required symbol: `manual actor joins the actual channel surface batch`
- Verify: `cd desktop && node --import ./test-jsdom-setup.mjs --import ./test-loader.mjs --experimental-strip-types --test-force-exit --test src/features/messages/lib/useSessionControlAction.jsdom-test.mjs src/features/messages/ui/MessageSessionControlRow.jsdom-test.mjs`

## `desktop/src/features/messages/ui/MessageSessionControls.tsx`

Selected-agent action surface with explicit captured scope, Back navigation, exact restart/compact grammar, single restart warning, shared-session disclosure and fresh fenced publication.

New module: `true`.

- Required symbol: `export function MessageSessionControls(`
- Required symbol: `Stops current work, including subagents. File changes are`
- Required symbol: `Back to agents`
- Invocation: `desktop/src/features/messages/ui/ThreadContextControls.tsx` → `MessageSessionControls`; exact call `<MessageSessionControls`; behavior test `production picker always lists one agent first, selection sends nothing and Back restores the roster`; verify `cd desktop && node --import ./test-jsdom-setup.mjs --import ./test-loader.mjs --experimental-strip-types --test-force-exit --test src/features/messages/ui/ThreadContextControls.jsdom-test.mjs`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke session-controls.spec.ts session-status.spec.ts`

## `desktop/src/features/messages/ui/MessageSessionControlRow.tsx`

Compact actor/action/target/outcome request presentation with expiry and fixed safe copy.

New module: `true`.

- Required symbol: `export function MessageSessionControlRow(`
- Verify: `cd desktop && node --import ./test-jsdom-setup.mjs --import ./test-loader.mjs --experimental-strip-types --test-force-exit --test src/features/messages/lib/useSessionControlAction.jsdom-test.mjs src/features/messages/ui/MessageSessionControlRow.jsdom-test.mjs`

## `desktop/src/features/messages/ui/MessageSessionControlRow.jsdom-test.mjs`

Rendered command presentation, pending/expiry semantics and MessageRow memo invalidation by correlated result.

New module: `true`.

- Required symbol: `MessageRow memo invalidates when only a raw correlated result changes`
- Verify: `cd desktop && node --import ./test-jsdom-setup.mjs --import ./test-loader.mjs --experimental-strip-types --test-force-exit --test src/features/messages/lib/useSessionControlAction.jsdom-test.mjs src/features/messages/ui/MessageSessionControlRow.jsdom-test.mjs`

## `desktop/tests/e2e/session-controls.spec.ts`

Registered thread-composer Manage context → agent → action workflow: canonical target routing, signed outcomes, draft preservation, toolbar order, keyboard/click/touch navigation, disabled-editor Compact, sibling isolation, denied/revoked access and unchanged owned activity projection. Scope the open-thread root capture/status assertion to its actual thread-head row, independently of the timeline copy. Cover context-only publication preserving a deliberately unpinned composer draft/attachment/native selection/caret, plus formatting/focus/narrow toolbar order and Space/arrow selection. Exercise reset acknowledgment separately from a generated reply or independent follow-up failure, preserving draft, signer, canonical thread and ordinary event deduplication.

New module: `true`.

- Required symbol: `known_agent_without_telemetry_still_has_current_scope_controls`
- Required symbol: `revoked send-time permission preserves draft and publishes nothing`
- Required symbol: `owned observer activity projects typed compaction lifecycle through the real store and panel`
- Required symbol: `two agent menus retain the selected signer and sibling scope`
- Required symbol: `denied viewer has no menu while another accessible agent retains recovery`
- Required symbol: `correct-clock viewer sees expired old request and validated manual actor`
- Required symbol: `context controls remain available while sending disables the thread editor`
- Required symbol: `touch selection and Back retain the first agent-list step without sending`
- Required symbol: `context action pins its agent while preserving the draft, attachment, selection and caret`
- Required symbol: `context stays before mentions across formatting, focus and narrow thread layouts`
- Required symbol: `restart_acknowledgment_is_distinct_from_generated_status`
- Required symbol: `restart_success_and_followup_failure_remain_independent`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke session-controls.spec.ts session-status.spec.ts`

## `desktop/src/features/channels/useMessageEventProfilePubkeys.ts`

Include validated manual session actors in the existing channel/thread batch.

New module: `false`.

- Required symbol: `...collectSessionActorPubkeys(events),`
- Invocation: `desktop/src/features/channels/useMessageEventProfilePubkeys.ts` → `collectSessionActorPubkeys`; exact call `...collectSessionActorPubkeys(events),`; behavior test `manual actor joins the actual channel surface batch`; verify `cd desktop && node --import ./test-jsdom-setup.mjs --import ./test-loader.mjs --experimental-strip-types --test-force-exit --test src/features/messages/lib/useSessionControlAction.jsdom-test.mjs src/features/messages/ui/MessageSessionControlRow.jsdom-test.mjs`
- Verify: `cd desktop && node --import ./test-jsdom-setup.mjs --import ./test-loader.mjs --experimental-strip-types --test-force-exit --test src/features/messages/lib/useSessionControlAction.jsdom-test.mjs src/features/messages/ui/MessageSessionControlRow.jsdom-test.mjs`

## `desktop/src/features/home/ui/HomeView.tsx`

Include validated manual session actors in the existing inbox/thread profile batch.

New module: `false`.

- Required symbol: `...collectSessionActorPubkeys(feedItems),`
- Required symbol: `...collectSessionActorPubkeys(threadContext.events),`
- Verify: `pnpm --dir desktop typecheck`

## `desktop/src/features/messages/lib/messageGrouping.ts`

Prevent session controls joining ordinary same-author groups.

New module: `false`.

- Required symbol: `if (previous?.sessionControl || current?.sessionControl) return false;`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/sessionControl.test.mjs src/features/messages/lib/sessionControlPermissions.test.mjs`

## `desktop/src/features/agents/ui/agentSessionCompaction.ts`

Validate typed ACP compaction wire updates and project bounded, generation-scoped, monotonic lifecycle rows with evidence-based origin, safe summary and legacy deduplication. Correlate bounded exact numeric session/new requests to successful ACKs, splitting reused SIDs only at the real backend binding point. Attribute a manual watch to only its first newly observed ID, preserving prior IDs and retracting attribution on late pre-request history. Preserve pre-I/O resume boundaries, clear identity on exact failed resume ACK and prevent direct attachment replay from consuming bounded history.

New module: `true`.

- Required symbol: `export function parseCompactionUpdate(`
- Required symbol: `export function processCompactionActivity(`
- Required symbol: `utf8.encode(update.compactionId).length > 128`
- Required symbol: `const terminal = prior && prior.status !== "in_progress";`
- Required symbol: `if (typedScopes.has(hookScope))`
- Required symbol: `Number.isSafeInteger(rpc.id)`
- Required symbol: `const firstNew = candidates.find(`
- Required symbol: `item.id === firstNew?.id`
- Required symbol: `sessionId: params.sessionId,`
- Required symbol: `update.summary.slice(0, MAX_SUMMARY)`
- Required symbol: `activity.status === "failed" || activity.conflict`
- Required symbol: `rpc.error === undefined &&`
- Required symbol: `const newer = next.pendingAttachments.some(`
- Required symbol: `next = rememberBoundary(next, {`
- Required symbol: `if (!newer)`
- Required symbol: `entry.id === boundary.id && entry.agent === boundary.agent`
- Invocation: `desktop/src/features/agents/ui/agentSessionCompaction.ts` → `parseCompactionUpdate`; exact call `parseCompactionUpdate(event.payload)`; behavior test `production transcript mutates one compaction row to failed`; verify `node --import ./desktop/test-loader.mjs --experimental-strip-types --test desktop/src/features/agents/ui/agentSessionCompaction.test.mjs desktop/src/features/agents/observerCompaction.test.mjs`
- Verify: `node --import ./desktop/test-loader.mjs --experimental-strip-types --test desktop/src/features/agents/ui/agentSessionCompaction.test.mjs desktop/src/features/agents/observerCompaction.test.mjs`

## `desktop/src/features/agents/ui/agentSessionTranscript.ts`

Consume the typed compaction lifecycle projection in actual incremental and rebuilt transcripts. Kimi recovery: support stable managed-report routes and logical-work visibility without changing other adapter policies.

New module: `false`.

- Required symbol: `compactions: emptyCompactionActivityState(),`
- Required symbol: `updateType !== "compaction_update"`
- Invocation: `desktop/src/features/agents/ui/agentSessionTranscript.ts` → `processCompactionActivity`; exact call `processCompactionActivity(d.compactions, event, d.items)`; behavior test `production transcript mutates one compaction row to failed`; verify `node --import ./desktop/test-loader.mjs --experimental-strip-types --test desktop/src/features/agents/ui/agentSessionCompaction.test.mjs desktop/src/features/agents/observerCompaction.test.mjs`
- Invocation: `desktop/src/features/agents/ui/agentSessionTranscript.ts` → `workStateTranscriptItem`; exact call `const workItem = workStateTranscriptItem(event, d.itemsById);`; behavior test `logical work states remain visible after the provider turn ends`; verify `pnpm --dir desktop test`
- Verify: `node --import ./desktop/test-loader.mjs --experimental-strip-types --test desktop/src/features/agents/ui/agentSessionCompaction.test.mjs desktop/src/features/agents/observerCompaction.test.mjs`
- Verify: `pnpm --dir desktop test`

## `desktop/src/features/agents/ui/agentSessionTypes.ts`

Carry typed compaction state on existing lifecycle items and a scoped legacy-hook correlation key. Kimi recovery: support stable managed-report routes and logical-work visibility without changing other adapter policies.

New module: `false`.

- Required symbol: `compaction?: CompactionActivity;`
- Required symbol: `compactionScope?: string;`
- Verify: `node --import ./desktop/test-loader.mjs --experimental-strip-types --test desktop/src/features/agents/ui/agentSessionCompaction.test.mjs desktop/src/features/agents/observerCompaction.test.mjs`
- Verify: `pnpm --dir desktop test`

## `desktop/src/features/agents/ui/agentSessionCompaction.test.mjs`

Bind compaction semantics to production incremental/build, grouping and meaningful-item seams.

New module: `true`.

- Required symbol: `production transcript mutates one compaction row to`
- Required symbol: `real worker initialization and session resume separate generations; turns do not`
- Verify: `node --import ./desktop/test-loader.mjs --experimental-strip-types --test desktop/src/features/agents/ui/agentSessionCompaction.test.mjs desktop/src/features/agents/observerCompaction.test.mjs`

## `desktop/src/features/agents/ui/agentSessionCompaction.jsdom-test.mjs`

Render actual parsed/grouped compaction failure through the production activity presenter with literal untrusted summary text. Mount the production channel panel against the actual observer store to verify worker context and absence of duplicate legacy rows.

New module: `true`.

- Required symbol: `typed compaction failure survives production grouping and renders salient literal summary`
- Required symbol: `production channel panel reads real observer store with worker context and hides only the correlated legacy duplicate`
- Verify: `node --import ./desktop/test-jsdom-setup.mjs --import ./desktop/test-loader.mjs --experimental-strip-types --test-force-exit --test desktop/src/features/agents/ui/agentSessionCompaction.jsdom-test.mjs`

## `desktop/src/features/agents/observerCompaction.test.mjs`

Exercise production observer-store out-of-order rebuild, reconnect replay dedup, raw retention and reset.

New module: `true`.

- Required symbol: `production observer store handles replay and out-of-order rebuild, retains raw frames and resets lifecycle state`
- Invocation: `desktop/src/features/agents/observerRelayStore.ts` → `processTranscriptEvent`; exact call `processTranscriptEvent(transcriptState, event)`; behavior test `production observer store handles replay and out-of-order rebuild, retains raw frames and resets lifecycle state`; verify `node --import ./desktop/test-loader.mjs --experimental-strip-types --test desktop/src/features/agents/ui/agentSessionCompaction.test.mjs desktop/src/features/agents/observerCompaction.test.mjs`
- Verify: `node --import ./desktop/test-loader.mjs --experimental-strip-types --test desktop/src/features/agents/ui/agentSessionCompaction.test.mjs desktop/src/features/agents/observerCompaction.test.mjs`

## `desktop/src/features/agents/ui/agentSessionPanelLayout.ts`

Keep global worker initialization as non-rendering context only when deriving a channel transcript; raw rail remains channel-scoped.

New module: `false`.

- Required symbol: `includeWorkerBoundaries = false,`
- Required symbol: `includeWorkerBoundaries &&`
- Verify: `node --import ./desktop/test-loader.mjs --experimental-strip-types --test desktop/src/features/agents/ui/agentSessionCompaction.test.mjs desktop/src/features/agents/ui/agentSessionPanelLayout.test.mjs desktop/src/features/agents/observerCompaction.test.mjs`

## `desktop/src/features/agents/ui/agentSessionPanelLayout.test.mjs`

Regress scoped lifecycle generation and scroll-key preservation while raw channel filtering remains exact.

New module: `false`.

- Required symbol: `channel transcript retains global worker boundaries without expanding the raw channel rail`
- Verify: `node --import ./desktop/test-loader.mjs --experimental-strip-types --test desktop/src/features/agents/ui/agentSessionCompaction.test.mjs desktop/src/features/agents/ui/agentSessionPanelLayout.test.mjs desktop/src/features/agents/observerCompaction.test.mjs`

## `desktop/src/features/agents/ui/ManagedAgentSessionPanel.tsx`

Use worker boundary context for channel transcript generation identity while preserving the raw/header channel window.

New module: `false`.

- Invocation: `desktop/src/features/agents/ui/ManagedAgentSessionPanel.tsx` → `scopeByChannel`; exact call `scopeByChannel(events, channelId, true)`; behavior test `production channel panel reads real observer store with worker context and hides only the correlated legacy duplicate`; verify `node --import ./desktop/test-jsdom-setup.mjs --import ./desktop/test-loader.mjs --experimental-strip-types --test-force-exit --test desktop/src/features/agents/ui/agentSessionCompaction.jsdom-test.mjs`
- Verify: `node --import ./desktop/test-loader.mjs --experimental-strip-types --test desktop/src/features/agents/ui/agentSessionCompaction.test.mjs desktop/src/features/agents/ui/agentSessionPanelLayout.test.mjs desktop/src/features/agents/observerCompaction.test.mjs`

## `desktop/src/features/channels/ui/AgentSessionThreadPanel.tsx`

Use worker boundary context for channel transcript generation identity while preserving the raw/header channel window.

New module: `false`.

- Invocation: `desktop/src/features/channels/ui/AgentSessionThreadPanel.tsx` → `scopeByChannel`; exact call `scopeByChannel(events, sessionChannelId, true)`; behavior test `production channel panel reads real observer store with worker context and hides only the correlated legacy duplicate`; verify `node --import ./desktop/test-jsdom-setup.mjs --import ./desktop/test-loader.mjs --experimental-strip-types --test-force-exit --test desktop/src/features/agents/ui/agentSessionCompaction.jsdom-test.mjs`
- Verify: `node --import ./desktop/test-loader.mjs --experimental-strip-types --test desktop/src/features/agents/ui/agentSessionCompaction.test.mjs desktop/src/features/agents/ui/agentSessionPanelLayout.test.mjs desktop/src/features/agents/observerCompaction.test.mjs`

## `desktop/src/features/agents/ui/agentSessionTranscriptGrouping.ts`

Exclude only proven typed-compaction duplicate legacy hooks from polished presentation; retain other suppressed rows and raw observer evidence.

New module: `false`.

- Required symbol: `item.descriptor?.reason !== "typed-compaction-duplicate"`
- Invocation: `desktop/src/features/agents/ui/agentSessionTranscriptGrouping.ts` → `splitIntoSessionRuns`; exact call `splitIntoSessionRuns(`; behavior test `grouped production transcript removes correlated legacy hook instead of rendering a quiet duplicate`; verify `node --import ./desktop/test-loader.mjs --experimental-strip-types --test desktop/src/features/agents/ui/agentSessionCompaction.test.mjs desktop/src/features/agents/ui/agentSessionPanelLayout.test.mjs desktop/src/features/agents/observerCompaction.test.mjs`
- Verify: `node --import ./desktop/test-loader.mjs --experimental-strip-types --test desktop/src/features/agents/ui/agentSessionCompaction.test.mjs desktop/src/features/agents/ui/agentSessionPanelLayout.test.mjs desktop/src/features/agents/observerCompaction.test.mjs`

## `scripts/post-screenshots.sh`

Publish immutable screenshot URLs to an explicit fork and remote with normal nonforce Git push, trusted DCO/signing, and no comment after a rejected push; preserve upstream defaults.

New module: `false`.

- Required symbol: `EXPLICIT_REPO=false`
- Required symbol: `TRACKING_REF=`
- Required symbol: `Signed-off-by: %s`
- Required symbol: `git push "$REMOTE"`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -p test_post_screenshots.py -v`

## `scripts/fork/tests/test_post_screenshots.py`

Exercise actual screenshot workflow with a local bare fork, real Git history/DCO/immutable URLs and rejected-push publication ordering, without network publication.

New module: `true`.

- Required symbol: `test_explicit_fork_preserves_remote_history_and_signs_off`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -p test_post_screenshots.py -v`

## `crates/buzz-acp/src/session_controls/orphan_owner_tests.rs`

Production-driver foreign owner reconciliation with real registry-lock uncertainty, later owner death and capped idle wake backoff.

New module: `true`.

- Required symbol: `orphan_registry_unknown_revisits_after_lock_release_without_external_wake`
- Required symbol: `orphan_alive_foreign_owner_is_revisited_after_it_dies`
- Required symbol: `orphan_live_owner_revisit_uses_capped_backoff_and_never_spins_without_capacity`
- Verify: `cargo test -p buzz-acp --lib orphan_registry_unknown_revisits_after_lock_release_without_external_wake -- --test-threads=1`
- Verify: `cargo test -p buzz-acp --lib orphan_alive_foreign_owner_is_revisited_after_it_dies -- --test-threads=1`
- Verify: `cargo test -p buzz-acp --lib orphan_live_owner_revisit_uses_capped_backoff_and_never_spins_without_capacity -- --test-threads=1`

## `crates/buzz-acp/src/worker_retirement_shutdown_tests.rs`

Production orderly-shutdown regressions for parked originals, abort guard returns and reused original retirement evidence. Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `true`.

- Required symbol: `retirement_shutdown_retries_original_parked_after_snapshot_failure`
- Required symbol: `retirement_shutdown_drains_original_returned_by_aborted_guard`
- Required symbol: `retirement_shutdown_reuses_original_evidence_after_parent_reaped`
- Required symbol: `retirement_shutdown_retains_uncertain_original_without_fabricating_clean`
- Verify: `cargo test -p buzz-acp --lib retirement_shutdown_retries_original_parked_after_snapshot_failure -- --test-threads=1`
- Verify: `cargo test -p buzz-acp --lib retirement_shutdown_drains_original_returned_by_aborted_guard -- --test-threads=1`
- Verify: `cargo test -p buzz-acp --lib retirement_shutdown_reuses_original_evidence_after_parent_reaped -- --test-threads=1`
- Verify: `cargo test -p buzz-acp --lib retirement_shutdown_retains_uncertain_original_without_fabricating_clean -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/pool/background_uncertainty_tests.rs`

Actual return/hold/poll regressions distinguish discovery uncertainty from observed completion.

New module: `true`.

- Required symbol: `background_unknown_then_empty_releases_hold_without_inventing_collection`
- Required symbol: `background_observed_then_unknown_then_empty_collects_exactly_once`
- Required symbol: `background_observed_exit_during_return_survives_later_unknown`
- Verify: `cargo test -p buzz-acp --lib background_unknown_then_empty_releases_hold_without_inventing_collection -- --test-threads=1`
- Verify: `cargo test -p buzz-acp --lib background_observed_then_unknown_then_empty_collects_exactly_once -- --test-threads=1`
- Verify: `cargo test -p buzz-acp --lib background_observed_exit_during_return_survives_later_unknown -- --test-threads=1`

## `desktop/tests/e2e/message-copy-link.spec.ts`

Unambiguous huddle event identity/kind/content with unchanged pending/huddle copy-link exclusions.

New module: `false`.

- Required symbol: `expect(huddleKind).toBe(KIND_HUDDLE_STARTED);`
- Required symbol: `getByTestId("huddle-attachment")`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke message-copy-link.spec.ts`

## `desktop/tests/e2e/reaction-order.spec.ts`

Preserve chronological reaction ordering, exact six-pixel body adjacency for ordinary and metadata-free agent messages, and absence of removed per-message session controls.

New module: `false`.

- Required symbol: `ordinary non-agent reactions stay adjacent to the message body`
- Required symbol: `getByTestId("manage-current-session")`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke reaction-order.spec.ts`

## `desktop/src/features/messages/ui/MessageActionBar.tsx`

Bound the reaction picker host to Radix available viewport height while preserving session controls and picker navigation.

New module: `false`.

- Required symbol: `[&_em-emoji-picker]:max-h-[var(--radix-popover-content-available-height)]`
- Required symbol: `[&_em-emoji-picker]:min-h-0`
- Required symbol: `collisionPadding={8}`
- Invocation: `desktop/src/features/messages/ui/MessageActionBar.tsx` → `EmojiPicker`; exact call `<EmojiPicker
                  autoFocus`; behavior test `emoji picker keeps Frequently used live within the app session`; verify `CI=1 pnpm --dir desktop test:e2e:smoke custom-emoji.spec.ts --workers=1 --retries=0`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke custom-emoji.spec.ts --workers=1 --retries=0`

## `desktop/tests/e2e/custom-emoji.spec.ts`

Preserve real emoji selection and Frequently-used interaction; assert and attach first-open/reopen full viewport bounds.

New module: `false`.

- Required symbol: `await expect(picker).toBeInViewport({ ratio: 1 });`
- Required symbol: `await expect(category).toBeInViewport({ ratio: 1 });`
- Required symbol: `await expectPickerInViewport("reopen");`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke custom-emoji.spec.ts --workers=1 --retries=0`

## `crates/buzz-acp/src/process_tree/codex_snapshots.rs`

Recognize the complete normalized default Bash capture only under a direct executable-confirmed Codex app-server; transient identity never promotes ordinary ancestors. Missing shell/parent identity is uncertainty; any positive executable mismatch remains work. Match the exact macOS ps octal-newline rendering without accepting modified captures.

New module: `true`.

- Required symbol: `pub(super) fn candidates(`
- Required symbol: `pub(super) fn classify_identities(`
- Required symbol: `snapshot_identity_does_not_promote_ordinary_ancestors`
- Required symbol: `missing_identity_is_uncertain_but_positive_mismatch_is_work`
- Required symbol: `snapshot_matches_macos_ps_escaped_newlines_without_accepting_changed_work`
- Invocation: `crates/buzz-acp/src/process_tree/codex_helpers.rs` → `classify_identities`; exact call `super::codex_snapshots::classify_identities(snapshots, output);`; behavior test `partial_snapshot_identity_does_not_invent_collection_but_preserves_observed_work`; verify `cargo test -p buzz-acp --lib snapshot_helper_tests -- --test-threads=1`
- Verify: `cargo test -p buzz-acp --lib snapshot -- --test-threads=1`

## `crates/buzz-acp/src/process_tree/codex_bash_snapshot.txt`

Exact source-derived default Codex Bash capture template; runtime only compares the complete normalized payload.

New module: `true`.

- Required symbol: `echo '# Snapshot file'`
- Required symbol: `__codex_snapshot_command()`
- Required symbol: `declare -xp`
- Verify: `cargo test -p buzz-acp --lib snapshot -- --test-threads=1`

## `crates/buzz-acp/src/pool/snapshot_helper_tests.rs`

Real ACP late snapshot/exit/return/poll regressions, ordinary shell lookalikes, wrong executable parent, transient exec and actual descendant work across setup capture. Copy the running macOS interpreter executable rather than a framework launcher to preserve the real fixture identity.

New module: `true`.

- Required symbol: `late_codex_snapshot_exit_does_not_enqueue_background_collection`
- Required symbol: `codex_snapshot_child_work_survives_late_and_session_capture`
- Required symbol: `ordinary_snapshot_lookalikes_still_collect_actual_work`
- Required symbol: `snapshot_pid_exec_into_ordinary_work_is_not_a_cached_resident`
- Verify: `cargo test -p buzz-acp --lib snapshot -- --test-threads=1`

## `crates/buzz-acp/src/pool/snapshot_uncertainty_tests.rs`

Production ACP discovery/adoption and original hold/poll regressions with injected raw census and partial executable outputs; preserve new definite work, isolate foreign uncertainty, prevent uncertain snapshot branch residency.

New module: `true`.

- Required symbol: `partial_snapshot_identity_does_not_invent_collection_but_preserves_observed_work`
- Required symbol: `uncertain_snapshot_capture_never_adopts_shell_or_real_descendants`
- Required symbol: `snapshot_uncertainty_is_owned_and_known_wrong_identity_stays_work`
- Required symbol: `new_real_work_alongside_uncertain_snapshot_collects_after_its_exit`
- Verify: `cargo test -p buzz-acp --lib snapshot_helper_tests -- --test-threads=1`

## `crates/buzz-cli/src/attachments.rs`

Preserve sanitized attachment basenames, enforce generic-file upload policy, and format download links while retaining image and video markup. Bound names to 255 UTF-8 bytes at character boundaries and preserve extensions.

New module: `true`.

- Required symbol: `fn sanitize_filename(`
- Required symbol: `fn validate_upload(`
- Required symbol: `fn attachment_markdown(`
- Required symbol: `pub filename: Option<String>`
- Verify: `cargo test -p buzz-cli`

## `crates/buzz-cli/src/attachment_tests.rs`

Exercise document, archive and Markdown uploads, primary and legacy metadata, message links, media compatibility, and executable and size rejection.

New module: `true`.

- Required symbol: `uploads_documents_archives_and_markdown_with_original_names`
- Required symbol: `messages_emit_file_links_for_multiple_attachments_and_keep_media_markup`
- Required symbol: `unicode_filenames_fit_relay_byte_limit_and_keep_extensions`
- Verify: `cargo test -p buzz-cli`

## `desktop/tests/e2e/file-attachment.spec.ts`

Verify Markdown and ZIP picker uploads, file-card labels and sizes, and native download filenames alongside existing attachment flows.

New module: `false`.

- Verify: `cd desktop && pnpm test:e2e:smoke -- file-attachment.spec.ts`

## `desktop/src-tauri/src/commands/media_download.rs`

Fetch generic attachments through a bounded 100 MiB native path while retaining 50 MiB image/editor/clipboard caps and existing auth, redirect, and cancellation protections.

New module: `false`.

- Required symbol: `MAX_FILE_DOWNLOAD_BYTES`
- Required symbol: `mod file_download_tests;`
- Invocation: `desktop/src-tauri/src/commands/media_download.rs` → `fetch_blob_bytes_with_cap`; exact call `fetch_blob_bytes_with_cap(url, state, MAX_FILE_DOWNLOAD_BYTES, None).await`; behavior test `generic_download_above_fifty_mib_is_byte_exact`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --lib commands::media_download::file_download_tests::generic_download_above_fifty_mib_is_byte_exact -- --exact`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace`

## `desktop/src-tauri/src/commands/media_download_file_tests.rs`

Exercise the native HTTP file fetch above 50 MiB, the 100 MiB boundary, declared and streamed oversize rejection, preserved image caps, and an opt-in real-relay hash check.

New module: `true`.

- Required symbol: `generic_download_above_fifty_mib_is_byte_exact`
- Required symbol: `generic_download_rejects_over_hundred_mib_with_or_without_length`
- Required symbol: `native_download_from_relay_matches_expected_sha256`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace`

## `desktop/src-tauri/src/commands/media_filename.rs`

Bound sanitized filenames by UTF-8 bytes at character boundaries while preserving the extension for imeta ingest.

New module: `false`.

- Required symbol: `unicode_labels_fit_relay_utf8_byte_limit_without_losing_extensions`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace`

## `crates/buzz-mesh-smoke/Cargo.toml`

Use the same immutable patched MeshLLM source as the desktop consumer to avoid duplicate runtime types.

New module: `false`.

- Verify: `cargo test -p buzz-mesh-smoke`

## `desktop/src-tauri/src/commands/agent_discovery/managed_node.rs`

Preserve confined Node.js extraction while propagating ZIP 9 entry-name decoding errors.

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/discovery/tests/codex_version.rs`

Exercise the production version probe with immutable scripts and explicit interpreter PATH inputs.

New module: `false`.

- Required symbol: `probe_codex_acp_version_uses_augmented_path_for_env_shebang_interpreter`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/discovery/tests/codex_version_fixture.rs`

Provide immutable executable symlinks, mutable version data and cleanup for the deliberately surviving descendant.

New module: `true`.

- Required symbol: `struct VersionFixture`
- Required symbol: `impl Drop for VersionFixture`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `docs/fork/DEPENDENCY_SECURITY.md`

Document patched release dependencies, independent TLS security updates, shipping-graph release checks, immutable compatibility pins, target scope and temporary transitive maintenance exceptions.

New module: `true`.

- Verify: `python3 scripts/fork/sync.py validate`

## `desktop/src-tauri/src/managed_agents/discovery/tests/fixtures/codex-version.sh`

Immutable executable fixture for production version-probe regression tests.

New module: `true`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/discovery/tests/fixtures/codex-hung.sh`

Immutable executable fixture for production version-probe regression tests.

New module: `true`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/discovery/tests/fixtures/codex-descendant.sh`

Immutable executable fixture for production version-probe regression tests.

New module: `true`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/discovery/tests/fixtures/env-node.sh`

Immutable executable fixture for production version-probe regression tests.

New module: `true`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `crates/buzz-acp/src/acp/claude_context.rs`

Use provider-confirmed Claude context capacity with same-session generation fencing; suppress provisional denominators, retain confirmed capacity across ordinary prompts, and consume filtered SDK results before diagnostic publication.

New module: `true`.

- Required symbol: `fn authoritative_window(result: &Value) -> Option<u64>`
- Required symbol: `state.confirmed_window`
- Required symbol: `state.generation != binding.generation_id()`
- Required symbol: `models.len() > 64`
- Invocation: `crates/buzz-acp/src/acp.rs` → `consume_claude_context_frame`; exact call `if self.consume_claude_context_frame(&msg) {`; behavior test `claude_context_uses_provider_window_and_the_final_context_numerator`; verify `cargo test --locked -p buzz-acp -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/acp.rs` → `begin_claude_context_turn`; exact call `self.begin_claude_context_turn(session_id);`; behavior test `claude_confirmed_capacity_survives_an_ordinary_prompt_for_tool_published_answers`; verify `cargo test --locked -p buzz-acp -- --test-threads=1`
- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`

## `desktop/tests/e2e/thread-head-stale-edit.spec.ts`

Exercise the edited thread head only after making its hover-only Reply action actionable; retain gated backfill and edited-content regression assertions.

New module: `false`.

- Required symbol: `await timelineRow.hover();`
- Required symbol: `await replyButton.click();`
- Required symbol: `deferThreadReplies: true`
- Verify: `pnpm --dir desktop exec playwright test --project=smoke tests/e2e/thread-head-stale-edit.spec.ts`

## `desktop/src/features/messages/ui/ThreadContextControls.tsx`

Thread-scoped accessible agent roster from bounded canonical raw history, verified directory and current authority; agent-first picker, scope reset, independent context toolbar trigger and selected action lifetime. Passively subscribe to the active owning validated thread query without replacing query/retry options; ownerless read-only forum previews retain an isolated bounded fallback, while other absent/disabled owners are explicitly unavailable. Resolve eligible-agent profile avatars with decorative shared squircle rendering and initials fallback, preserving accessible collision labels. Own publication progress across menu closure and preserve the selected action while authoritative history refreshes. Position roster and selected actions above the complete composer with a rem-scaled gap, tracking composer and pane resizing.

New module: `true`.

- Required symbol: `export function ThreadContextControls(`
- Required symbol: `key={JSON.stringify([relayUrl, signerPubkey, channel.id, rootId])}`
- Required symbol: `aria-label="Manage context"`
- Required symbol: `return query?.isActive() ? query.state : undefined;`
- Required symbol: `function useContextMenuPosition(`
- Invocation: `desktop/src/features/messages/ui/MessageThreadPanel.tsx` → `ThreadContextControls`; exact call `<ThreadContextControls
                  channel={channel}
                  threadHead={threadHead}
                  prepareAgentPin={prepareAgentPin}
                  composerRef={composerRef}
                />`; behavior test `known_agent_without_telemetry_still_has_current_scope_controls`; verify `CI=1 pnpm --dir desktop test:e2e:smoke session-controls.spec.ts session-status.spec.ts`
- Invocation: `desktop/src/features/messages/ui/ThreadContextControls.tsx` → `useSyncExternalStore`; exact call `const ownedReplies = React.useSyncExternalStore(`; behavior test `opening the picker preserves the owning notification query validation across gated retries (stream)`; verify `cd desktop && node --import ./test-jsdom-setup.mjs --import ./test-loader.mjs --experimental-strip-types --test-force-exit --test src/features/messages/ui/ThreadContextControls.jsdom-test.mjs`
- Invocation: `desktop/src/features/messages/ui/ThreadContextControls.tsx` → `projectThreadContextAgents`; exact call `projectThreadContextAgents({`; behavior test `same-name agents remain distinct and a relay switch closes selected actions`; verify `cd desktop && node --import ./test-jsdom-setup.mjs --import ./test-loader.mjs --experimental-strip-types --test-force-exit --test src/features/messages/ui/ThreadContextControls.jsdom-test.mjs`
- Invocation: `desktop/src/features/messages/ui/ThreadContextControls.tsx` → `loadThreadReplies`; exact call `queryFn: () => loadThreadReplies(queryClient, channel.id, rootId)`; behavior test `read-only forum previews retain a bounded roster without replacing the disabled owner`; verify `cd desktop && node --import ./test-jsdom-setup.mjs --import ./test-loader.mjs --experimental-strip-types --test-force-exit --test src/features/messages/ui/ThreadContextControls.jsdom-test.mjs`
- Invocation: `desktop/src/features/messages/ui/ThreadContextControls.tsx` → `useUsersBatchQuery`; exact call `const profiles = useUsersBatchQuery(`; behavior test `production roster resolves profile images and falls back without duplicating accessible labels`; verify `cd desktop && node --import ./test-jsdom-setup.mjs --import ./test-loader.mjs --experimental-strip-types --test-force-exit --test src/features/messages/ui/ThreadContextControls.jsdom-test.mjs`
- Invocation: `desktop/src/features/messages/ui/ThreadContextControls.tsx` → `UserAvatar`; exact call `<UserAvatar
                    avatarUrl={`; behavior test `production roster resolves profile images and falls back without duplicating accessible labels`; verify `cd desktop && node --import ./test-jsdom-setup.mjs --import ./test-loader.mjs --experimental-strip-types --test-force-exit --test src/features/messages/ui/ThreadContextControls.jsdom-test.mjs`
- Invocation: `desktop/src/features/messages/ui/ThreadContextControls.tsx` → `useContextMenuPosition`; exact call `useContextMenuPosition(
    open,
    composerRef,
  )`; behavior test `context stays before mentions across formatting, focus and narrow thread layouts`; verify `CI=1 pnpm --dir desktop test:e2e:smoke session-controls.spec.ts session-status.spec.ts`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/threadContextAgents.test.mjs`
- Verify: `cd desktop && node --import ./test-jsdom-setup.mjs --import ./test-loader.mjs --experimental-strip-types --test-force-exit --test src/features/messages/ui/ThreadContextControls.jsdom-test.mjs`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke session-controls.spec.ts session-status.spec.ts`

## `desktop/src/features/messages/lib/threadContextAgents.ts`

Project deduplicated verified agent signers and explicit agent p-tags from the canonical thread, excluding sibling/delegated identities, and preserve permission and same-name disambiguation independently of mention pins.

New module: `true`.

- Required symbol: `export function hasPersistedThread(`
- Required symbol: `export function projectThreadContextAgents(`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/threadContextAgents.test.mjs`

## `desktop/src/features/messages/lib/threadContextAgents.test.mjs`

Production projection and bounded loader regressions for root-only/pending heads, raw nested authors, addressed agents, same-name deduplication, permissions, partial pages and sibling isolation.

New module: `true`.

- Required symbol: `root-only addressed agents and raw nested signers deduplicate without sibling or display attribution leakage`
- Required symbol: `bounded production thread loader includes later-page collapsed participants and fails rather than returning partial history`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/lib/threadContextAgents.test.mjs`

## `desktop/src/features/messages/ui/ThreadContextControls.jsdom-test.mjs`

Mount the real picker for agent-first selection, no implicit send, Back, incomplete history, denied/revoked authority and thread/requester/relay reset. Wait for scheduled query notifications and assert boolean DOM absence to bound failure diagnostics. Mount the production owning query and gate notification-target retries to reject false completeness; non-forum missing-owner caches remain non-actionable, and a disabled forum owner retains isolated paginated recovery.

New module: `true`.

- Required symbol: `production picker always lists one agent first, selection sends nothing and Back restores the roster`
- Required symbol: `same-name agents remain distinct and a relay switch closes selected actions`
- Required symbol: `opening the picker preserves the owning notification query validation across gated retries`
- Required symbol: `a missing non-forum owning query is unavailable rather than a complete roster`
- Required symbol: `read-only forum previews retain a bounded roster without replacing the disabled owner`
- Verify: `cd desktop && node --import ./test-jsdom-setup.mjs --import ./test-loader.mjs --experimental-strip-types --test-force-exit --test src/features/messages/ui/ThreadContextControls.jsdom-test.mjs`

## `desktop/src/features/messages/ui/MessageThreadPanel.tsx`

Provide a real persisted thread scope to composer context management while retaining focus/overlay thread layout and draft ownership. Bound mounted thread replies while preserving targets, read-state semantics, branch controls and composer clearance.

New module: `false`.

- Required symbol: `threadContextControl={`
- Required symbol: `<VirtualizedThreadReplies`
- Required symbol: `virtualizerOwnsPrependAnchoring: useVirtualizer`
- Invocation: `desktop/src/features/messages/ui/MessageThreadPanel.tsx` → `VirtualizedThreadReplies`; exact call `<VirtualizedThreadReplies`; behavior test `long threads bound mounted replies and preserve navigation and live scroll ownership`; verify `CI=1 pnpm --dir desktop test:e2e:smoke thread-virtualization.spec.ts thread-focus-mode.spec.ts scroll-history.spec.ts thread-unread.spec.ts`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke session-controls.spec.ts session-status.spec.ts`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke thread-virtualization.spec.ts thread-focus-mode.spec.ts scroll-history.spec.ts thread-unread.spec.ts`

## `desktop/src/features/messages/ui/MessageComposer.types.ts`

Expose a narrow thread-only context control slot without extending general composer authority.

New module: `false`.

- Required symbol: `threadContextControl?:`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke session-controls.spec.ts session-status.spec.ts`

## `desktop/src/features/messages/ui/MessageComposer.tsx`

Forward the independent thread context slot through the composer dock without editor disabled/sending gates or draft mutations.

New module: `false`.

- Required symbol: `threadContextControl(prepareAgentPin, formRef)`
- Invocation: `desktop/src/features/messages/ui/MessageComposer.tsx` → `ComposerDockToolbar`; exact call `<ComposerDockToolbar
              addressedAgents={
                editTarget == null && !composerDisabled ? lockedAgents : []
              }
              autoPinConfirmationTitle={autoPinConfirmationTitle}
              layoutMode={layoutMode}
              composerDisabled={composerDisabled}
              editor={richText.editor}
              extraActions={toolbarExtraActions}
              threadContextControl={
                typeof threadContextControl === "function"
                  ? threadContextControl(prepareAgentPin, formRef)
                  : threadContextControl
              }`; behavior test `known_agent_without_telemetry_still_has_current_scope_controls`; verify `CI=1 pnpm --dir desktop test:e2e:smoke session-controls.spec.ts session-status.spec.ts`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke session-controls.spec.ts session-status.spec.ts`

## `desktop/src/features/messages/ui/MessageComposerToolbar.tsx`

Place context management directly before mention management and retain both positions across formatting/voice layouts; keep the context trigger independent of the disabled editor.

New module: `false`.

- Required symbol: `{threadContextControl}`
- Required symbol: `{threadContextControl ? mentionButton : null}`
- Invocation: `desktop/src/features/messages/ui/ComposerDockToolbar.tsx` → `MessageComposerToolbar`; exact call `<MessageComposerToolbar {...toolbarProps} />`; behavior test `known_agent_without_telemetry_still_has_current_scope_controls`; verify `CI=1 pnpm --dir desktop test:e2e:smoke session-controls.spec.ts session-status.spec.ts`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke session-controls.spec.ts session-status.spec.ts`

## `crates/buzz-acp/src/acp/restart_status.rs`

Bounded exact-session text capture for a status-only ACP turn, excluding thoughts, tools and overflow.

New module: `true`.

- Required symbol: `fn begin_status_capture`
- Required symbol: `fn take_status_text`
- Required symbol: `fn capture_status_update`
- Invocation: `crates/buzz-acp/src/acp.rs` → `capture_status_update`; exact call `self.capture_status_update(msg);`; behavior test `overflow_empty_thought_and_rate_limited_output_never_publish_partial_answers`; verify `cargo test -p buzz-acp --lib overflow_empty_thought_and_rate_limited_output_never_publish_partial_answers -- --test-threads=1`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/status_tasks.rs`

Typed finite-priority restart-status scheduling without human queue mutation or unrelated-scope blocking. Maintain ordinary scoped typing activity through the status turn and clear only its scope on completion. Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `true`.

- Required symbol: `fn dispatch(`
- Required symbol: `fn finish(`
- Invocation: `crates/buzz-acp/src/lib.rs` → `status_tasks::dispatch`; exact call `status_tasks::dispatch(&mut pool, &ctx, &status_tx, &mut typing_channels);`; behavior test `checked_restart_depths_publish_one_signed_status_and_resume_without_bootstrap`; verify `cargo test -p buzz-acp --lib checked_restart_depths_publish_one_signed_status_and_resume_without_bootstrap -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/status_tasks.rs` → `typing_channels.insert`; exact call `typing_channels.insert(
            scope.clone(),
            pool::restart_status::reply_tags(
                &job,
                ctx.channel_info.is_known_dm(scope.channel_id()),
            ),
        );`; behavior test `checked_restart_depths_publish_one_signed_status_and_resume_without_bootstrap`; verify `cargo test -p buzz-acp --lib checked_restart_depths_publish_one_signed_status_and_resume_without_bootstrap -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/status_tasks.rs` → `typing_channels.remove`; exact call `typing_channels.remove(&result.job.saved.scope);`; behavior test `restart_status_completion_clears_only_its_own_activity`; verify `cargo test -p buzz-acp --lib restart_status_completion_clears_only_its_own_activity -- --test-threads=1`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/pool/restart_status.rs`

First replacement-session status turn with selected context, exact-authority signed publication and separately prepared failure feedback. Attach shared live generation/history/actor/occupancy tags before signing the first successful reply, retaining exact Prepared bytes across retries and leaving failure notices untagged.

New module: `true`.

- Required symbol: `fn run_status_task`
- Required symbol: `fn publish_prepared`
- Required symbol: `fn report_expired`
- Required symbol: `.tags(status_tags)`
- Invocation: `crates/buzz-acp/src/pool/restart_status.rs` → `tags`; exact call `.tags(status_tags)`; behavior test `first_signed_status_carries_selected_history_actor_and_current_or_unknown_occupancy`; verify `cargo test -p buzz-acp --lib first_signed_status_carries_selected_history_actor_and_current_or_unknown_occupancy -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/status_tasks.rs` → `run_status_task`; exact call `run_status_task(agent, job, ctx, task_dispatch, control_rx).await`; behavior test `checked_restart_depths_publish_one_signed_status_and_resume_without_bootstrap`; verify `cargo test -p buzz-acp --lib checked_restart_depths_publish_one_signed_status_and_resume_without_bootstrap -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/status_tasks.rs` → `report_expired`; exact call `pool::restart_status::report_expired(job, ctx, dispatch)`; behavior test `expiry_scheduler_keeps_its_fence_when_cancellation_persistence_recovers`; verify `cargo test -p buzz-acp --lib expiry_scheduler_keeps_its_fence_when_cancellation_persistence_recovers -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `report_expired`; exact call `pool::restart_status::report_expired(job, ctx, dispatch)`; behavior test `a_known_local_panic_reports_failure_without_restarting_the_model`; verify `cargo test -p buzz-acp --lib a_known_local_panic_reports_failure_without_restarting_the_model -- --test-threads=1`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/pool/status_language.rs`

Bounded signed conversation-language inference under owner/requester trust policy; numbered prose retained, substantive unsupported prose replaces old certainty, verified agents excluded.

New module: `true`.

- Required symbol: `struct Evidence`
- Required symbol: `fn trusted_authors`
- Required symbol: `fn infer(`
- Invocation: `crates/buzz-acp/src/pool/session_history.rs` → `language.observe`; exact call `language.observe(event, humans, channel, target);`; behavior test `numbered_and_unsupported_latest_prose_replace_old_english_at_every_depth`; verify `cargo test -p buzz-acp --lib numbered_and_unsupported_latest_prose_replace_old_english_at_every_depth -- --test-threads=1`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/session_controls/status.rs`

Exact accepted-token local ownership and immediate cancellation fencing independent of store readability, fresh admission checks, observer cancellation targets and finite status priority.

New module: `true`.

- Required symbol: `struct StatusJob`
- Required symbol: `fn status_current`
- Required symbol: `fn cancel_status_for_token`
- Required symbol: `enum StatusCancellation`
- Required symbol: `fn cancellable_status_scopes`
- Invocation: `crates/buzz-acp/src/session_controls/retirement.rs` → `status_ready`; exact call `Self::status_ready(state, store, scope);`; behavior test `checked_restart_depths_publish_one_signed_status_and_resume_without_bootstrap`; verify `cargo test -p buzz-acp --lib checked_restart_depths_publish_one_signed_status_and_resume_without_bootstrap -- --test-threads=1`
- Invocation: `crates/buzz-acp/src/lib.rs` → `cancellable_status_scopes`; exact call `.cancellable_status_scopes()`; behavior test `unavailable_pending_record_cancel_retires_local_fence_before_a_worker_is_free`; verify `cargo test -p buzz-acp --lib unavailable_pending_record_cancel_retires_local_fence_before_a_worker_is_free -- --test-threads=1`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/session_store_status.rs`

Bounded backward-compatible restart-status metadata, immutable prepared events and no-effect dead-owner reconciliation.

New module: `true`.

- Required symbol: `struct RestartStatus`
- Required symbol: `fn status_transition`
- Required symbol: `fn recover_status`
- Invocation: `crates/buzz-acp/src/session_store_orphans.rs` → `recover_status`; exact call `match self.recover_status(saved) {`; behavior test `reconstruction_claims_only_dead_owners_and_never_replays_started_model_attempts`; verify `cargo test -p buzz-acp --lib reconstruction_claims_only_dead_owners_and_never_replays_started_model_attempts -- --test-threads=1`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/pool/restart_status_runtime_tests.rs`

Production-bound authorized Restart, scheduler, fake ACP, signed HTTP, language, failure, cancellation and reconstruction regressions. Assert real scheduler activity registration and scoped cleanup on successful or failed status completion.

New module: `true`.

- Required symbol: `fn checked_restart_depths_publish_one_signed_status_and_resume_without_bootstrap`
- Required symbol: `fn reconstruction_claims_only_dead_owners_and_never_replays_started_model_attempts`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/pool/restart_status_edge_tests.rs`

Production-bound overflow, rate-limit, reply routing, deadline, persistence failure and verified-agent language regressions.

New module: `true`.

- Required symbol: `fn overflow_empty_thought_and_rate_limited_output_never_publish_partial_answers`
- Required symbol: `fn acknowledgment_without_a_completion_write_retains_prepared_bytes_and_token`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/SESSION_CONTROLS.md`

Documented status-only Restart behavior, signed delivery, cancellation and crash uncertainty, language trust policy and provider limits.

New module: `true`.

- Required symbol: `# Restart status turns`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/session_model_thread.md`

Restart-status runtime guidance and acceptance contract.

New module: `false`.

- Required symbol: `<restart-status>`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/session_model_channel.md`

Restart-status runtime guidance and acceptance contract.

New module: `false`.

- Required symbol: `<restart-status>`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/pool/restart_status_review_tests.rs`

Production-path review regressions for numbered/unsupported language across history depths and true nonowner admission, pre-first-poll status/expiry fences, uncertain owner cancellation and durable failure delivery.

New module: `true`.

- Required symbol: `fn numbered_and_unsupported_latest_prose_replace_old_english_at_every_depth`
- Required symbol: `fn scheduler_keeps_its_original_fence_before_the_first_status_poll`
- Required symbol: `fn expiry_scheduler_keeps_its_fence_when_cancellation_persistence_recovers`
- Required symbol: `fn uncertain_owner_cancel_has_no_chat_sender_and_fences_held_feedback`
- Required symbol: `fn owned_failure_feedback_requires_persistence_and_retries_only_its_saved_bytes`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/pool/restart_status_unavailable_tests.rs`

Actual checked-retirement and scheduler regressions for owner/observer cancellation with missing/unreadable records, WaitingRetirement write failure, held expiry feedback, queued human preservation, stale-token protection and confirmed terminal status.

New module: `true`.

- Required symbol: `fn unavailable_pending_record_cancel_retires_local_fence_before_a_worker_is_free`
- Required symbol: `fn unavailable_cancel_during_retirement_cannot_recreate_the_followup_after_clean_exit`
- Required symbol: `fn unavailable_record_cancel_fences_already_scheduled_workerless_expiry_feedback`
- Required symbol: `fn stale_token_cleanup_cannot_fence_newer_pending_or_retiring_work_when_reads_fail`
- Required symbol: `fn cancellation_preserves_a_confirmed_terminal_status_even_when_the_busy_cache_lags`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/pool/restart_status_metadata_tests.rs`

Actual authorized Restart admission, scheduler, ACP usage notifications and signed HTTP publication regressions for first-reply history/actor/current-generation tags, current or unavailable occupancy and exact prepared event persistence across all selected depths.

New module: `true`.

- Required symbol: `fn first_signed_status_carries_selected_history_actor_and_current_or_unknown_occupancy`
- Verify: `cargo test -p buzz-acp --lib first_signed_status_carries_selected_history_actor_and_current_or_unknown_occupancy -- --test-threads=1`

## `desktop/src/features/messages/ui/useAgentAddressLockPicker.ts`

Prepare revision- and editor-generation-fenced context audience promotion with immediate exact-identity literal restoration while preserving authored text and selection.

New module: `false`.

- Required symbol: `const prepareAgentPin = React.useCallback(`
- Required symbol: `audienceGenerationRef.current !== generation`
- Required symbol: `unpinnedAgentPubkeysRef.current.delete(normalized);`
- Invocation: `desktop/src/features/messages/ui/useAgentAddressLockPicker.ts` → `promotePersistentAgentAudienceIfUnchanged`; exact call `const promotion = promotePersistentAgentAudienceIfUnchanged({`; behavior test `a context action pins its target and immediately restores its literal mention`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/ui/contextActionAudience.test.mjs`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/ui/contextActionAudience.test.mjs`

## `desktop/src/features/messages/ui/contextActionAudience.test.mjs`

Exercise the production picker with the actual persistent audience store for immediate context mention restoration, preserved selection, intervening audience edits and editor-scope round trips.

New module: `true`.

- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/ui/contextActionAudience.test.mjs`

## `crates/buzz-cli/src/validate.rs`

Reject whitespace-only message bodies without changing general size-only validation or blank state-clearing semantics; hint when reading interactive stdin.

New module: `false`.

- Required symbol: `pub fn validate_nonempty_content(`
- Required symbol: `std::io::stdin().is_terminal()`
- Verify: `cargo test -p buzz-cli --test message_content`

## `crates/buzz-cli/tests/message_content.rs`

Run the actual CLI against a signature-checking mock relay to prove blank message commands make no requests, edits consume stdin, nonblank bytes survive and attachment-only sends publish.

New module: `true`.

- Required symbol: `blank_message_commands_fail_before_any_relay_request`
- Required symbol: `message_commands_preserve_stdin_bytes_in_signed_events`
- Required symbol: `attachment_only_message_still_uploads_and_publishes`
- Verify: `cargo test -p buzz-cli --test message_content`

## `crates/buzz-acp/src/acp/provider_failure.rs`

Translate negotiated Air v1 terminal provider failures into bounded, classified ACP application errors Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `true`.

- Required symbol: `pub(super) fn check_response(`
- Verify: `cargo test -p buzz-acp --lib provider_failure -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/acp/provider_failure_tests.rs`

Exercise provider failure negotiation, error propagation, recovery, cancellation and authentication over real ACP subprocess pipes Kimi recovery: preserve original logical work and delivery identity across bounded retries, permission and checked child retirement.

New module: `true`.

- Required symbol: `async fn provider_failure_is_an_application_error_and_same_pipe_can_retry(`
- Verify: `cargo test -p buzz-acp --lib provider_failure -- --test-threads=1`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/pool/provider_failure_tests.rs`

Bind provider failure recovery to production dispatch, backoff, queue redelivery and successful delivery accounting

New module: `true`.

- Required symbol: `async fn provider_failure_requeues_original_event_and_commits_only_after_recovery(`
- Verify: `cargo test -p buzz-acp --lib provider_failure -- --test-threads=1`

## `crates/git-sign-nostr/src/lib.rs`

Validate NIP-OA owner BIP-340 curve points at signing, verification and envelope parsing boundaries instead of accepting hex length alone.

New module: `false`.

- Required symbol: `fn test_parse_envelope_rejects_invalid_oa_pubkey()`
- Verify: `cargo test --locked -p git-sign-nostr`

## `crates/buzz-audio-client/Cargo.toml`

Keep the shared Huddle audio graph on the immutable NetEq security fix.

New module: `false`.

- Verify: `just ci`

## `crates/buzz-db/src/runtime/follow_up_retirement_tests.rs`

Keep trigger retirement checks aligned with the checksum-preserving fork migration numbering.

New module: `false`.

- Verify: `just ci`

## `crates/buzz-db/src/runtime/push_enqueue.rs`

Use the existing durable match queue in the event transaction instead of the transient post-commit producer.

New module: `false`.

Registered upstream deletion: `true`.

- Verify: `just ci`

## `crates/buzz-db/src/store/event_follow_up.rs`

Persist eligible push work in the existing match queue inside the admitted event transaction, ordered by the lease gate; retain TTL lifecycle.

New module: `false`.

- Required symbol: `INSERT INTO push_match_queue (community_id, event_id)`
- Invocation: `crates/buzz-db/src/store/event_follow_up.rs` → `enqueue_push_match`; exact call `enqueue_push_match(tx.conn(), tx_community, event_id, kind).await?`; behavior test `queue_timeout_rolls_back_normal_and_managed_reports_before_retry`; verify `scripts/postgres-test-run.sh -p buzz-db --lib --tests`
- Verify: `just ci`

## `crates/buzz-db/src/store/event_follow_up_postgres_tests.rs`

Verify durable normal/managed enqueue, transaction rollback on queue error, more than256jobs, actual process death and restart claims on both schema paths.

New module: `false`.

- Verify: `just ci`

## `docker-compose.yml`

Use pinned Silo and mc for local development, matching the harness and deployment templates while preserving existing storage names and volumes; keep the separate cached MinIO CI fixture.

New module: `false`.

- Verify: `just ci`

## `docs/fork/UPSTREAM_SYNC_2026_10_22.md`

Record exact snapshot integration, rollout boundaries, preserved behavior, deferred work and acceptance gates.

New module: `true`.

- Verify: `just ci`

## `docs/push-gateway-deployment.md`

Document durable push handoff and compatible writer/schema rollout instead of upstream best-effort enqueue.

New module: `false`.

- Verify: `just ci`

## `migrations/0056_personal_read_state.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `false`.

Registered upstream deletion: `true`.

- Verify: `just ci`

## `migrations/0057_personal_read_started_at.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `false`.

Registered upstream deletion: `true`.

- Verify: `just ci`

## `migrations/0057_personal_read_state.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `true`.

- Verify: `just ci`

## `migrations/0058_personal_read_started_at.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `true`.

- Verify: `just ci`

## `migrations/0058_personal_read_through_message.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `false`.

Registered upstream deletion: `true`.

- Verify: `just ci`

## `migrations/0059_personal_read_through_message.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `true`.

- Verify: `just ci`

## `migrations/0059_push_rollback_gate.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `false`.

Registered upstream deletion: `true`.

- Verify: `just ci`

## `migrations/0060_push_rollback_gate.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `true`.

- Verify: `just ci`

## `migrations/0060_push_single_application.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `false`.

Registered upstream deletion: `true`.

- Verify: `just ci`

## `migrations/0061_channel_labels.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `false`.

Registered upstream deletion: `true`.

- Verify: `just ci`

## `migrations/0061_push_single_application.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `true`.

- Verify: `just ci`

## `migrations/0062_channel_labels.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `true`.

- Verify: `just ci`

## `migrations/0062_system_channel_type.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `false`.

Registered upstream deletion: `true`.

- Verify: `just ci`

## `migrations/0063_retire_push_match_trigger.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `false`.

Registered upstream deletion: `true`.

- Verify: `just ci`

## `migrations/0063_system_channel_type.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `true`.

- Verify: `just ci`

## `migrations/0064_profile_name_fields.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `false`.

Registered upstream deletion: `true`.

- Verify: `just ci`

## `migrations/0064_retire_push_match_trigger.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `true`.

- Verify: `just ci`

## `migrations/0065_profile_name_fields.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `true`.

- Verify: `just ci`

## `migrations/0065_retire_channel_ttl_trigger.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `false`.

Registered upstream deletion: `true`.

- Verify: `just ci`

## `migrations/0066_community_nip_fi_issuers.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `false`.

Registered upstream deletion: `true`.

- Verify: `just ci`

## `migrations/0066_retire_channel_ttl_trigger.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `true`.

- Verify: `just ci`

## `migrations/0067_channel_join_role.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `false`.

Registered upstream deletion: `true`.

- Verify: `just ci`

## `migrations/0067_community_nip_fi_issuers.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `true`.

- Verify: `just ci`

## `migrations/0068_channel_join_role.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `true`.

- Verify: `just ci`

## `migrations/0068_entity_deletion_requests.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `false`.

Registered upstream deletion: `true`.

- Verify: `just ci`

## `migrations/0069_entity_deletion_requests.sql`

Preserve applied fork migration 0056 by renumbering only unapplied upstream migrations.

New module: `true`.

- Verify: `just ci`

## `schema/tables/public/managed_deliveries.sql`

Declare the canonical managed-delivery ledger in desired-state setup.

New module: `true`.

- Verify: `just ci`

## `schema/triggers/managed_deliveries.sql`

Declare the canonical managed-delivery ledger in desired-state setup.

New module: `true`.

- Verify: `just ci`

## `crates/buzz-agent/tests/anthropic_continuation.rs`

Include the actual session/new response in native Anthropic continuation fixture failures.

New module: `false`.

- Required symbol: `session/new failed: {reply}`
- Verify: `cargo test --locked -p buzz-agent`

## `crates/buzz-db/src/runtime/admitted_tx.rs`

Keep admitted transactions and TTL commit hooks without a transient push handoff.

New module: `false`.

- Verify: `just ci`
- Verify: `scripts/postgres-test-run.sh`

## `crates/buzz-db/src/store/channel.rs`

Keep atomic channel creation and metadata edits on admitted transactions without transient producer injection.

New module: `false`.

- Verify: `just ci`
- Verify: `scripts/postgres-test-run.sh`

## `crates/buzz-db/src/store/channel_members.rs`

Keep member/channel writer regressions aligned with direct durable enqueue.

New module: `false`.

- Verify: `just ci`
- Verify: `scripts/postgres-test-run.sh`

## `crates/buzz-db/src/store/push.rs`

Keep lease ordering and durable matcher claims while exercising the actual admitted enqueue transaction.

New module: `false`.

- Verify: `just ci`
- Verify: `scripts/postgres-test-run.sh`

## `crates/buzz-db/src/store/replaceable.rs`

Preserve replaceable event transactional hooks without a transient producer.

New module: `false`.

- Verify: `just ci`
- Verify: `scripts/postgres-test-run.sh`

## `crates/buzz-relay/src/handlers/group_state_postgres_tests.rs`

Use a connected PostgreSQL fixture with bounded realistic acquisition time; preserve short lazy-pool unit fixtures.

New module: `false`.

- Verify: `just ci`
- Verify: `scripts/postgres-test-run.sh`

## `docs/database.md`

Keep relay/gateway migration references aligned with the preserved fork history.

New module: `false`.

- Verify: `python3 scripts/fork/sync.py validate`

## `.github/workflows/mesh-lifecycle.yml`

Preserve the MinIO CI image override for the relay-driven mesh lifecycle smoke

New module: `false`.

- Required symbol: `lifecycle-smoke:
    env:
      COMPOSE_FILE: docker-compose.yml:docker-compose.ci.yml`
- Verify: `python3 scripts/fork/sync.py validate`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `docker-compose.ci.yml`

Preserve the published MinIO CI image for the server and initialization services

New module: `false`.

- Required symbol: `  minio:
    image: ${MINIO_CI_IMAGE:-ghcr.io/block/buzz-minio:latest}`
- Required symbol: `  minio-init:
    image: ${MINIO_CI_IMAGE:-ghcr.io/block/buzz-minio:latest}`
- Verify: `python3 scripts/fork/sync.py validate`
- Verify: `python3 -m unittest discover -s scripts/fork/tests -v`

## `.github/workflows/_ci-desktop.yml`

Keep native Linux CI off the degraded runner Azure apt mirror without changing required checks; pin browser E2E to Ubuntu 24.04 supported by the locked Playwright release.

New module: `false`.

- Required symbol: `'https://archive.ubuntu.com/ubuntu/'`
- Required symbol: `'https://security.ubuntu.com/ubuntu/'`
- Required symbol: `  desktop-smoke-e2e:
`
- Required symbol: `    runs-on: ubuntu-24.04`
- Verify: `just ci`

## `desktop/src/features/channels/useObservedUnreadPersistence.ts`

Bound queued native marker snapshots per scope and preserve pending advances across acknowledgements.

New module: `false`.

- Required symbol: `pendingMarkerFlushScopesRef`
- Required symbol: `pending.get(marker.contextId)?.readAt === marker.readAt`
- Invocation: `desktop/src/features/channels/useObservedUnreadPersistence.ts` → `flushPendingMarkers`; exact call `else flushPendingMarkers(scopeKey);`; behavior test `desktop/src/features/channels/observedUnreadMarkerBatching.test.mjs`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/channels/observedUnreadMarkerBatching.test.mjs`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/channels/observedUnreadMarkerBatching.test.mjs`

## `desktop/src/features/channels/observedUnreadMarkerBatching.test.mjs`

Regress marker burst batching, duplicate pending advances, newer removals and retired-scope acknowledgements.

New module: `true`.

- Required symbol: `a marker burst writes each context once in bounded native batches`
- Required symbol: `a retired scope acknowledgement does not strand markers when returning to it`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/channels/observedUnreadMarkerBatching.test.mjs`

## `desktop/src/features/messages/ui/VirtualizedThreadReplies.tsx`

Virtualize variable-height replies inside the existing thread scroll region with measured head offset and cancellable bottom ownership. Preserve explicit reply navigation at a clamped physical floor and retire bottom intent when the reader leaves it, including direct scroll updates. Reconcile reader ownership on navigation input even when an already-clamped floor produces no scroll event.

New module: `true`.

- Required symbol: `export function VirtualizedThreadReplies`
- Required symbol: `startMargin={geometry.margin}`
- Required symbol: `scrollRef={scrollRef}`
- Invocation: `desktop/src/features/messages/ui/VirtualizedThreadReplies.tsx` → `useVirtualizedThreadAnchor`; exact call `useVirtualizedThreadAnchor(`; behavior test `a live reply in an earlier expanded branch preserves the reading row`; verify `CI=1 pnpm --dir desktop test:e2e:smoke thread-virtualization.spec.ts thread-focus-mode.spec.ts scroll-history.spec.ts thread-unread.spec.ts`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke thread-virtualization.spec.ts thread-focus-mode.spec.ts scroll-history.spec.ts thread-unread.spec.ts`
- Verify: `cd desktop && pnpm test:e2e:smoke messaging.spec.ts timeline-no-shift.spec.ts`

## `desktop/src/features/messages/ui/useAnchoredScroll.ts`

Wait for visible virtual targets before acknowledging navigation and preserve temporary centered targets through layout reflow. Cancel the deferred initial bottom pin when target navigation takes ownership.

New module: `false`.

- Required symbol: `const needsRealization`
- Required symbol: `else if (!virtualizerOwnsPrependAnchoring)`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test "src/features/messages/ui/useAnchoredScroll*.test.mjs"`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke thread-virtualization.spec.ts thread-focus-mode.spec.ts scroll-history.spec.ts thread-unread.spec.ts`

## `desktop/src/features/messages/ui/useAnchoredScroll.lifecycle.test.mjs`

Bind virtual target acknowledgement and initial-position ownership regressions to the production scroll hook. Deliver the initial settlement frame deterministically after target navigation to regress the cancellation race.

New module: `false`.

- Required symbol: `function PendingVirtualTargetHarness`
- Required symbol: `virtual arrivals count independently of DOM anchoring and reset at bottom`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/ui/useAnchoredScroll.lifecycle.test.mjs`

## `desktop/src/features/messages/ui/useVirtualizedBottomSettle.ts`

Support existing externally owned thread scroll nodes without changing channel scroll ownership.

New module: `false`.

- Required symbol: `scrollRef?: React.RefObject<HTMLDivElement | null>`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/ui/useVirtualizedBottomSettle.test.mjs`

## `desktop/src/features/messages/ui/useVirtualizedBottomSettle.test.mjs`

Verify external thread scrollers retain bottom intent and yield to reader input.

New module: `false`.

- Required symbol: `an external thread scroller owns bottom settlement and input cancellation`
- Verify: `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test src/features/messages/ui/useVirtualizedBottomSettle.test.mjs`

## `desktop/tests/e2e/thread-virtualization.spec.ts`

Exercise bounded mounts, offscreen targets, scroll ownership and composer clearance on 600 replies, and a channel containing 1200 threads.

New module: `true`.

- Required symbol: `long threads bound mounted replies and preserve navigation and live scroll ownership`
- Required symbol: `a channel with many thread roots keeps its mounted history bounded`
- Required symbol: `a live reply in an earlier expanded branch preserves the reading row`
- Required symbol: `End releases a short thread target even without physical scroll movement`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke thread-virtualization.spec.ts thread-focus-mode.spec.ts scroll-history.spec.ts thread-unread.spec.ts`

## `desktop/tests/e2e/thread-focus-mode.spec.ts`

Exercise reading-position preservation with explicit reader wheel ownership before a virtualized layout switch.

New module: `false`.

- Required symbol: `new WheelEvent("wheel", { bubbles: true, deltaY: -120 })`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke thread-focus-mode.spec.ts`

## `desktop/src/features/messages/ui/useVirtualizedThreadAnchor.ts`

Preserve a reading message and offset through indexed thread insertions, follow late measurements, and yield to reader navigation with a generation fence.

New module: `true`.

- Required symbol: `export function useVirtualizedThreadAnchor`
- Required symbol: `snapshot.generation !== generationRef.current`
- Required symbol: `list.scrollTo(scroller.scrollTop + drift)`
- Verify: `CI=1 pnpm --dir desktop test:e2e:smoke thread-virtualization.spec.ts thread-focus-mode.spec.ts scroll-history.spec.ts thread-unread.spec.ts`

## `crates/buzz-push-gateway/Cargo.toml`

Use P-256 0.14 with the patched constant-time arithmetic graph for App Attest.

New module: `false`.

- Verify: `cargo test --locked -p buzz-push-gateway --lib`
- Verify: `cargo clippy --locked -p buzz-push-gateway --all-targets -- -D warnings`

## `crates/buzz-push-gateway/src/app_attest/certificate.rs`

Keep App Attest certificate key extraction on the current SEC1 point API without changing encoding.

New module: `false`.

- Verify: `cargo test --locked -p buzz-push-gateway --lib`
- Verify: `cargo clippy --locked -p buzz-push-gateway --all-targets -- -D warnings`

## `crates/buzz-push-gateway/src/app_attest/authenticator.rs`

Keep App Attest authenticator key decoding on the current SEC1 point API without changing validation.

New module: `false`.

- Verify: `cargo test --locked -p buzz-push-gateway --lib`
- Verify: `cargo clippy --locked -p buzz-push-gateway --all-targets -- -D warnings`

## `crates/buzz-push-gateway/src/app_attest_assertion_tests.rs`

Exercise existing assertion verification regressions with the patched P-256 graph and current SEC1 names.

New module: `false`.

- Verify: `cargo test --locked -p buzz-push-gateway --lib`
- Verify: `cargo clippy --locked -p buzz-push-gateway --all-targets -- -D warnings`

## `desktop/tests/e2e/messaging.spec.ts`

Regress explicit inline reply navigation at a clamped thread floor and keyboard recovery of live bottom following.

New module: `false`.

- Required symbol: `Latest keyboard-follow reply`
- Verify: `cd desktop && pnpm test:e2e:smoke messaging.spec.ts`

## `.env.example`

Document optional bounded database pools for relay overlap

New module: `false`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-admin/Cargo.toml`

Dependencies for bounded migration CLI tests

New module: `false`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-admin/src/main.rs`

Bounded migration CLI and effective timeout evidence

New module: `false`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-admin/tests/postgres_migration_timeouts.rs`

Exercise migration locks, statement timeout and cancellation against PostgreSQL. Resolve the runtime CLI path when Nextest relocates a test archive.

New module: `true`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-db/src/lib.rs`

Export the bounded migration timeout interface

New module: `false`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-relay/src/api/git/deploy.rs`

Connect deployment lifecycle ownership to the existing relay or workflow path

New module: `true`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-relay/src/api/git/external_infra/deploy.rs`

Verify accepted-work drain and descendant completion

New module: `true`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-relay/src/api/git/hook.rs`

Connect deployment lifecycle ownership to the existing relay or workflow path

New module: `false`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-relay/src/api/git/mod.rs`

Connect deployment lifecycle ownership to the existing relay or workflow path

New module: `false`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-relay/src/api/git/policy.rs`

Connect deployment lifecycle ownership to the existing relay or workflow path

New module: `false`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-relay/src/api/git/read_lifetime.rs`

Connect deployment lifecycle ownership to the existing relay or workflow path

New module: `true`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-relay/src/api/media_deploy_postgres.rs`

Verify accepted-work drain and descendant completion

New module: `true`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-relay/src/audio/postgres_deploy_tests.rs`

Verify accepted-work drain and descendant completion

New module: `true`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-relay/src/background.rs`

Track post-commit dispatch and dependent effects until completion

New module: `true`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-relay/src/deploy_drain.rs`

Private per-boot admission fence and transitive accepted-work accounting

New module: `true`.

- Required symbol: `pub fn fence_http(`
- Required symbol: `pub struct WorkGuard`
- Required symbol: `pub enum DrainPhase`
- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-relay/src/deploy_runtime.rs`

Retain dependency readers and audit until accepted work and workers finish

New module: `true`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-relay/src/handlers/command_executor.rs`

Connect deployment lifecycle ownership to the existing relay or workflow path

New module: `false`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-relay/src/handlers/side_effects.rs`

Connect deployment lifecycle ownership to the existing relay or workflow path

New module: `false`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-relay/src/handlers/workflows_deploy_postgres.rs`

Verify accepted-work drain and descendant completion

New module: `true`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-relay/src/operator_listener.rs`

Connect deployment lifecycle ownership to the existing relay or workflow path

New module: `false`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-relay/src/readiness.rs`

Connect deployment lifecycle ownership to the existing relay or workflow path

New module: `false`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-relay/tests/deploy_drain.rs`

Verify accepted-work drain and descendant completion

New module: `true`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-relay/tests/postgres_deploy_drain.rs`

Verify accepted-work drain and descendant completion

New module: `true`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-test-client/Cargo.toml`

Verify accepted-work drain and descendant completion

New module: `false`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-test-client/src/bin/blue_green_probe.rs`

Dispatch the deployment verifier on Unix and reject unsupported platforms without compiling Unix-only permission APIs.

New module: `true`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-workflow/src/lib.rs`

Connect deployment lifecycle ownership to the existing relay or workflow path

New module: `false`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-workflow/src/lifecycle.rs`

Keep accepted workflow executions owned through finalization

New module: `true`.

- Verify: `just test`
- Verify: `just ci`

## `crates/buzz-workflow/tests/lifecycle.rs`

Verify accepted-work drain and descendant completion

New module: `true`.

- Verify: `just test`
- Verify: `just ci`

## `CONTRIBUTING.md`

Document Silo development storage and preservation of existing local datasets and compatible service names.

New module: `false`.

- Verify: `python3 scripts/fork/sync.py validate`

## `README.md`

Describe Silo as the bundled object store for development and deployment.

New module: `false`.

- Verify: `python3 scripts/fork/sync.py validate`

## `deploy/charts/buzz/README.md`

Name the existing bundled Silo server consistently while preserving the minio chart API.

New module: `false`.

- Verify: `python3 scripts/fork/sync.py validate`

## `deploy/charts/buzz/values.yaml`

Describe the bundled Silo quickstart in comments without changing chart values.

New module: `false`.

- Verify: `python3 scripts/fork/sync.py validate`

## `deploy/compose/README.md`

Name the existing bundled Silo dependency consistently.

New module: `false`.

- Verify: `python3 scripts/fork/sync.py validate`

## `crates/buzz-acp/src/background_poll.rs`

Preserve service launch provenance, nonblocking process polling, fair capacity dispatch, and their regression coverage.

New module: `true`.

- Invocation: `crates/buzz-acp/src/background_poll.rs` → `spawn_blocking`; exact call `self.tasks.spawn_blocking(move || {`; behavior test `slow_snapshot_keeps_event_loop_responsive_and_allows_only_one_poll`; verify `cargo test --locked -p buzz-acp --lib slow_snapshot_keeps_event_loop_responsive_and_allows_only_one_poll -- --test-threads=1`
- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/background_poll_tests.rs`

Preserve service launch provenance, nonblocking process polling, fair capacity dispatch, and their regression coverage.

New module: `true`.

- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/mcp_service.rs`

Register initialized service process instances and preserve independent, bounded forwarding in both stdio directions.

New module: `true`.

- Invocation: `crates/buzz-acp/src/mcp_service.rs` → `register_service_instances`; exact call `tree.register_service_instances(record.root, &record.instances);`; behavior test `registration_matches_worker_and_process_instance_without_hiding_work`; verify `cargo test --locked -p buzz-acp --lib registration_matches_worker_and_process_instance_without_hiding_work -- --test-threads=1`
- Verify: `cargo test --locked -p buzz-acp --test mcp_service -- --test-threads=1`
- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/mcp_service/codex.rs`

Preserve service launch provenance, nonblocking process polling, fair capacity dispatch, and their regression coverage.

New module: `true`.

- Invocation: `crates/buzz-acp/src/mcp_service/codex.rs` → `install`; exact call `super::codex_proxy::install(registry, adapter, launch, registered)`; behavior test `real_codex_native_mcp_config_survives_injected_server_configuration`; verify `BUZZ_TEST_BIN_DIR="$PWD/target/debug" cargo test --locked -p buzz-acp --lib real_codex_native_mcp_config -- --ignored --test-threads=1`
- Invocation: `crates/buzz-acp/src/mcp_service/codex.rs` → `output`; exact call `super::discovery::output(discovery)`; behavior test `native_discovery_retires_descendants_on_every_output_exit`; verify `cargo test --locked -p buzz-acp --test mcp_discovery`
- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/mcp_service/codex_proxy.rs`

Preserve service launch provenance, nonblocking process polling, fair capacity dispatch, and their regression coverage.

New module: `true`.

- Invocation: `crates/buzz-acp/src/mcp_service/codex_proxy.rs` → `apply_servers`; exact call `apply_servers(&mut request, &spec.servers)?;`; behavior test `real_codex_native_mcp_config_survives_injected_server_configuration`; verify `BUZZ_TEST_BIN_DIR="$PWD/target/debug" cargo build --locked -p buzz-acp && BUZZ_TEST_BIN_DIR="$PWD/target/debug" cargo test --locked -p buzz-acp --lib real_codex_native_mcp_config -- --ignored --test-threads=1`
- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/mcp_service_tests.rs`

Preserve service launch provenance, nonblocking process polling, fair capacity dispatch, and their regression coverage.

New module: `true`.

- Required symbol: `mod launcher_tests;`
- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/pool/dispatch_diagnostics.rs`

Preserve service launch provenance, nonblocking process polling, fair capacity dispatch, and their regression coverage.

New module: `true`.

- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/pool/real_codex_stall_tests.rs`

Preserve service launch provenance, nonblocking process polling, fair capacity dispatch, and their regression coverage. Limit tool-inventory preflight to the validated adapter 2.2.2 status API; preserve the original bundled 2.1.1 workflow and five actual ping receipts in both profiles.

New module: `true`.

- Required symbol: `async fn wait_for_mcp_tools(`
- Required symbol: `wait_for_mcp_tools(&mut agent, &session, index).await;`
- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/pool/stall_tests.rs`

Preserve service launch provenance, nonblocking process polling, fair capacity dispatch, and their regression coverage.

New module: `true`.

- Required symbol: `unavailable_oldest_scope_does_not_block_available_held_owner`
- Required symbol: `newly_registered_service_does_not_invent_collection_or_hide_reused_work_pid`
- Required symbol: `checked_restart_of_held_scope_releases_slot_and_retains_ordinary_input`
- Required symbol: `completed_work_survives_a_later_service_only_snapshot`
- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/tests/fixtures/late_service/package.json`

Preserve service launch provenance, nonblocking process polling, fair capacity dispatch, and their regression coverage.

New module: `true`.

- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/tests/fixtures/late_service/src/index.mjs`

Preserve service launch provenance, nonblocking process polling, fair capacity dispatch, and their regression coverage.

New module: `true`.

- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/tests/mcp_service.rs`

Exercise real npm service provenance, actual child work, and large pipelined messages through the production stdio wrapper.

New module: `true`.

- Required symbol: `real_npm_service_registers_before_tools_and_excludes_actual_work`
- Required symbol: `pipelined_large_messages_keep_both_stdio_directions_progressing`
- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/src/mcp_service/discovery.rs`

Own bounded native discovery process groups across output, timeout and cancellation, and verify cleanup before returning.

New module: `true`.

- Required symbol: `async fn output`
- Required symbol: `async fn retire`
- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`

## `crates/buzz-acp/tests/mcp_discovery.rs`

Exercise native discovery cleanup through the actual models command and observe child and grandchild exit.

New module: `true`.

- Required symbol: `native_discovery_retires_descendants_on_every_output_exit`
- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`

## `crates/sprig/Cargo.toml`

Use existing workspace test dependencies for real multicall MCP transport coverage.

New module: `false`.

- Verify: `cargo test --locked -p sprig`

## `crates/sprig/src/main.rs`

Route ACP internal MCP self-launch commands after Linux resolves the personality symlink to the Sprig target.

New module: `false`.

- Verify: `cargo test --locked -p sprig`

## `crates/sprig/tests/mcp_personalities.rs`

Exercise injected MCP registration and native configuration forwarding through the actual Sprig target.

New module: `true`.

- Required symbol: `multicall_target_runs_injected_mcp_transport`
- Required symbol: `multicall_target_runs_native_codex_proxy`
- Verify: `cargo test --locked -p sprig`

## `crates/buzz-acp/src/mcp_service/launcher_tests.rs`

Exercise actual adapter 2.2.2 and external Codex 0.164.0 through a CODEX_PATH-preserving operator launcher, with live proxy ancestry and native/injected MCP receipts.

New module: `true`.

- Required symbol: `real_launcher_wrapper_preserves_proxy_and_external_cli`
- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`
- Verify: `env -u CODEX_PATH cargo test --locked -p buzz-acp --lib real_launcher_wrapper -- --ignored --nocapture --test-threads=1`

## `crates/buzz-acp/tests/fixtures/codex_launcher.py`

Record launcher selection and exec the actual adapter/external CLI; reproduce inherited-proxy loss with an explicit negative-control flag.

New module: `true`.

- Required symbol: `selected = inherited or environment["BUZZ_TEST_LAUNCHER_FALLBACK"]`
- Required symbol: `os.execve(target, [target, *sys.argv[1:]], environment)`
- Verify: `cargo test --locked -p buzz-acp -- --test-threads=1`
- Verify: `env -u CODEX_PATH cargo test --locked -p buzz-acp --lib real_launcher_wrapper -- --ignored --nocapture --test-threads=1`

## `desktop/src/features/channels/ui/ChannelPane.tsx`

Pass the open thread root id into the channel timeline.

New module: `false`.

- Required symbol: `openThreadHeadId={openThreadHeadId}`
- Invocation: `desktop/src/features/channels/ui/ChannelPane.tsx` → `MessageTimeline`; exact call `<MessageTimeline
              openThreadHeadId={openThreadHeadId}`; behavior test `buzz: highlights only the open root with a reply summary`; verify `cd desktop && pnpm build:e2e && pnpm exec playwright test tests/e2e/open-thread-highlight.spec.ts --project=smoke`
- Verify: `cd desktop && pnpm build:e2e && pnpm exec playwright test tests/e2e/open-thread-highlight.spec.ts --project=smoke`

## `desktop/src/features/messages/ui/MessageTimeline.tsx`

Pass the open thread root id into the channel timeline.

New module: `false`.

- Required symbol: `openThreadHeadId={openThreadHeadId}`
- Invocation: `desktop/src/features/messages/ui/MessageTimeline.tsx` → `TimelineMessageList`; exact call `<TimelineMessageList
      openThreadHeadId={openThreadHeadId}`; behavior test `buzz: highlights only the open root with a reply summary`; verify `cd desktop && pnpm build:e2e && pnpm exec playwright test tests/e2e/open-thread-highlight.spec.ts --project=smoke`
- Verify: `cd desktop && pnpm build:e2e && pnpm exec playwright test tests/e2e/open-thread-highlight.spec.ts --project=smoke`

## `desktop/tests/e2e/open-thread-highlight.spec.ts`

Verify open thread root highlighting, assistive state, and screenshots in both themes with and without replies.

New module: `true`.

- Required symbol: `data-open-thread-head`
- Required symbol: `aria-current`
- Required symbol: `buzz-dark`
- Required symbol: `highlights only the open root with a reply summary`
- Verify: `cd desktop && pnpm build:e2e && pnpm exec playwright test tests/e2e/open-thread-highlight.spec.ts --project=smoke`

## `crates/buzz-acp/src/pool/restart_status_history_tests.rs`

Exercise restart history acquisition across delayed relay responses, partial failures, complete unavailability and authorized recovery through the production scheduler and signed HTTP publication.

New module: `true`.

- Required symbol: `fn restart_recent_keeps_history_across_slow_reads_partial_failure_and_recovery`
- Verify: `cargo test -p buzz-acp -- --test-threads=1`

## `crates/buzz-test-client/src/bin/blue_green_probe/unix.rs`

Preserve the signed channel-scoped deployment verifier with owner-only key configuration and ledger permissions on Unix.

New module: `true`.

- Required symbol: `pub(super) async fn main() -> Result<()>`
- Required symbol: `os::unix::fs::{OpenOptionsExt, PermissionsExt}`
- Verify: `cargo test -p buzz-test-client --bin blue_green_probe`
- Verify: `just ci`

## `crates/buzz-acp/KIMI_RECOVERY.md`

Document bounded logical-work recovery, permission decisions, uncertain effects, managed delivery and provider acceptance boundaries

New module: `true`.

- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/acp/kimi_diagnostics.rs`

Project Kimi plaintext wire diagnostics to bounded structural fields while preserving the owner-encrypted concrete request view

New module: `true`.

- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/acp/kimi_diagnostics_tests.rs`

Cover Kimi logical-work, permission, retirement or managed-delivery invariants through production seams and deterministic peers

New module: `true`.

- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/acp/kimi_manual_permission_tests.rs`

Cover Kimi logical-work, permission, retirement or managed-delivery invariants through production seams and deterministic peers

New module: `true`.

- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/acp/kimi_permission.rs`

Bind actual Kimi permission requests to active prompt, generation and offered options; bound decision, pipe write and post-response progress

New module: `true`.

- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/acp/kimi_permission/controls.rs`

Validate owner-signed, command-first permission decisions against the current route and random request token

New module: `true`.

- Required symbol: `return Err(AcpError::PermissionViewUnavailable);`
- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/acp/kimi_permission_tests.rs`

Cover Kimi logical-work, permission, retirement or managed-delivery invariants through production seams and deterministic peers

New module: `true`.

- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/acp/kimi_recovery.rs`

Preserve safe structured Kimi error identity with positive transient classification and authentication precedence

New module: `true`.

- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/acp/kimi_recovery_tests.rs`

Cover Kimi logical-work, permission, retirement or managed-delivery invariants through production seams and deterministic peers

New module: `true`.

- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/acp/kimi_retirement_tests.rs`

Cover Kimi logical-work, permission, retirement or managed-delivery invariants through production seams and deterministic peers

New module: `true`.

- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/config/kimi_recovery.rs`

Parse and bound configurable Kimi retry counts, delays, work budget and permission deadlines

New module: `true`.

- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/managed_work.rs`

Verify bounded signed delivery-journal snapshots and publish terminal notices through the original CLI delivery key and route

New module: `true`.

- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/managed_work_tests.rs`

Cover Kimi logical-work, permission, retirement or managed-delivery invariants through production seams and deterministic peers

New module: `true`.

- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/pool/background_adapter_fixture.rs`

Share deterministic background ACP peers while keeping provider observations separate from synthetic failures

New module: `true`.

- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/pool/kimi_background_tests.rs`

Cover Kimi logical-work, permission, retirement or managed-delivery invariants through production seams and deterministic peers

New module: `true`.

- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/pool/kimi_controls.rs`

Deliver permission decisions only to the current task, scope, route and control generation

New module: `true`.

- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/pool/kimi_runtime_tests.rs`

Cover Kimi logical-work, permission, retirement or managed-delivery invariants through production seams and deterministic peers

New module: `true`.

- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/queue/recovery.rs`

Retain original logical work, delivery identity, bounded independent jitter and revocation across retry and collection

New module: `true`.

- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/queue/recovery_observer.rs`

Emit safe logical-work state transitions correlated with the original route and publication identity

New module: `true`.

- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/queue/recovery_state.rs`

Track shared cancellation, terminality and uncertain external effects without claiming tool exactly-once execution

New module: `true`.

- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/src/queue/recovery_tests.rs`

Cover Kimi logical-work, permission, retirement or managed-delivery invariants through production seams and deterministic peers

New module: `true`.

- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/test-fixtures/kimi/README.md`

Distinguish normalized observed permission frames from synthetic OAuth and permission fault regressions

New module: `true`.

- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `crates/buzz-acp/test-fixtures/kimi/permission_request.json`

Preserve the normalized observed Kimi permission option IDs and request shape without credentials

New module: `true`.

- Verify: `BUZZ_TEST_BIN_DIR="$PWD/target/debug" env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test --locked -p buzz-acp -- --test-threads=4`

## `desktop/src/features/agents/ui/agentSessionTranscriptHelpers.ts`

Keep pure transcript payload helpers separate from state dispatch within the repository file-size limit

New module: `false`.

- Verify: `pnpm --dir desktop test`

## `desktop/src/features/agents/ui/agentWorkStateTranscript.test.mjs`

Cover visible logical-work states, stable cards, stale updates and rejection of unsafe diagnostic reasons

New module: `true`.

- Verify: `pnpm --dir desktop test`

## `desktop/src/features/agents/ui/agentWorkStateTranscript.ts`

Render bounded known logical-work states and safe reasons while rejecting stale sequence updates

New module: `true`.

- Verify: `pnpm --dir desktop test`
