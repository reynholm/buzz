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

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/agents_tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/personas/create.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/personas/delete_cascade_tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

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

Mechanical struct-literal compatibility repair: new device fields default to None; Read-only catalog projection for list without creating retained databases

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

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/personas/snapshot/tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/personas/update/name_propagation_tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/team_snapshot.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/team_snapshot/tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/teams/adopt/apply.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/teams/adopt/tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

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

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

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

Register the device policy compatibility test module; Register home policy, sync and projection modules; Register device-home migration production module

New module: `false`.

- Required symbol: `mod device_policy_types_tests;`
- Required symbol: `pub(crate) mod definition_home;`
- Required symbol: `pub(crate) mod device_home_sync;`
- Required symbol: `pub(crate) mod persona_device_view;`
- Required symbol: `pub(crate) mod device_home_migration;`
- Required symbol: `pub(crate) use restore::child_ownership::{retry_restore_cleanup, RestoreCleanup};`
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

Mechanical struct-literal compatibility repair: new device fields default to None; Use existing pure built-in merge for policy list visibility without saving

New module: `false`.

- Required symbol: `fn persona_definitions_for_policy`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/personas/tests.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/readiness.rs`

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

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

Mechanical struct-literal compatibility repair: new device fields default to None

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

Expose public device metadata using app data directory and hostname; storage behavior covered below IPC routing, isolated native UI routing remains later acceptance

New module: `true`.

- Required symbol: `pub fn get_device_identity`
- Required symbol: `load_or_create_device_identity(&directory.join("device.json"), label)`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/mod.rs`

Register device identity IPC command

New module: `false`.

- Required symbol: `pub use device_identity::*;`
- Required symbol: `pub use device_home_sync::*;`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/lib.rs`

Register device identity module and public metadata IPC command

New module: `false`.

- Required symbol: `mod device_identity;`
- Required symbol: `commands::get_device_identity,`
- Required symbol: `commands::begin_device_home_sync,`
- Required symbol: `commands::hydrate_device_home_history,`
- Required symbol: `commands::finish_device_home_sync,`
- Required symbol: `commands::invalidate_device_home_sync,`
- Required symbol: `device_identity::initialize_device_authority(&app_handle)`
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

Backend token/scope hydration, exhaustive authenticated paging and live-apply completion barrier; Run deferred migration with prospective readiness under hydrated/drained/token/scope barrier before installing Ready

New module: `true`.

- Required symbol: `fn begin_session`
- Required symbol: `fn finish_session`
- Required symbol: `async fn hydrate_history`
- Required symbol: `struct DeviceHomeHistory`
- Required symbol: `fn finish_session_with`
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

Read-only flattened list views with shared capability fast path on explicit unavailable context

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
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/app_state.rs`

In-memory scoped backend device home sync state; retain failed restore child cleanup handles separately from authorized runtime pairs

New module: `false`.

- Required symbol: `device_home_sync:`
- Required symbol: `managed_agent_restore_cleanup: crate::managed_agents::RestoreCleanup`
- Required symbol: `managed_agent_restore_cleanup: Default::default()`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/workspace.rs`

Invalidate backend home evidence on workspace apply; Migrate proven homes before scoped event sync without waiting for frontend history; top-level Wry entry wiring is compile coverage with native acceptance outstanding; initialize captured scoped retention schema before read-only policy, propagating open/schema errors while retaining Pending

New module: `false`.

- Required symbol: `device_home_sync::reset(&state)?;`
- Required symbol: `fn prepare_workspace_event_sync`
- Required symbol: `fn prepare_workspace_event_sync_with`
- Invocation: `desktop/src-tauri/src/commands/workspace.rs` → `prepare_workspace_event_sync`; exact call `prepare_workspace_event_sync(&restore_app, &scope)?;`; behavior test `commands::workspace::device_home_preparation_tests::fresh_workspace_preparation_initializes_scope_without_authorizing_absence`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml commands::workspace::device_home_preparation_tests::fresh_workspace_preparation_initializes_scope_without_authorizing_absence -- --exact`
- Invocation: `desktop/src-tauri/src/commands/workspace.rs` → `open_retention_db`; exact call `crate::managed_agents::retention::open_retention_db(&scope.db_path)?;`; behavior test `commands::workspace::device_home_preparation_tests::fresh_workspace_preparation_initializes_scope_without_authorizing_absence`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml commands::workspace::device_home_preparation_tests::fresh_workspace_preparation_initializes_scope_without_authorizing_absence -- --exact`
- Invocation: `desktop/src-tauri/src/commands/workspace.rs` → `migrate_device_homes_before_sync`; exact call `crate::managed_agents::device_home_migration::migrate_device_homes_before_sync(app)`; behavior test `commands::workspace::device_home_preparation_tests::fresh_workspace_preparation_initializes_scope_without_authorizing_absence`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml commands::workspace::device_home_preparation_tests::fresh_workspace_preparation_initializes_scope_without_authorizing_absence -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/personas/inbound.rs`

Token-scoped live apply leases fence async reconciliation and record failures

New module: `false`.

- Required symbol: `session_token: Option<String>`
- Required symbol: `lease.complete(&result)?;`
- Required symbol: `fn reconcile_inbound_tombstone_with_refresh`
- Invocation: `desktop/src-tauri/src/commands/personas/inbound.rs` → `reconcile_inbound_tombstone_with_refresh`; exact call `reconcile_inbound_tombstone_with_refresh(event, arrival_relay_url, app, state, || {
        try_regenerate_nest(app);
    })`; behavior test `commands::personas::inbound::device_sync_tests::signed_multi_coordinate_deletion_keeps_unselected_remote_head_and_newer_recreation`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml commands::personas::inbound::device_sync_tests::signed_multi_coordinate_deletion_keeps_unselected_remote_head_and_newer_recreation -- --exact`
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

Preserved coalescing/catalog/gap/degraded/retry behavior plus IPC token, completion, cancellation and reconnect tests

New module: `false`.

- Required symbol: `backend sync waits for buffered live applies`
- Required symbol: `late previous hydration cannot finalize a replacement session`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/shared/api/tauriPersonas.ts`

Backend sync IPC client and optional live reconciliation token

New module: `false`.

- Required symbol: `export async function beginDeviceHomeSync`
- Required symbol: `export async function hydrateDeviceHomeHistory`
- Required symbol: `sessionToken?: string`
- Invocation: `desktop/src/shared/api/tauriPersonas.ts` → `invokeTauri`; exact call `await invokeTauri("reconcile_inbound_persona_event", {
    eventJson,
    arrivalRelayUrl,
    sessionToken,
  });`; behavior test `backend sync waits for buffered live applies before finish and carries its token`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test --test-name-pattern="backend sync waits for buffered" src/features/agents/lib/usePersonaSync.test.mjs`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/shared/api/relayClientShared.ts`

Optional sustained live subscription health while preserving initial readiness API

New module: `false`.

- Required symbol: `type LiveSubscriptionHealth`
- Required symbol: `onHealth?: (health: LiveSubscriptionHealth) => void;`
- Verify: `just desktop-test desktop-typecheck`

## `desktop/src/shared/api/relayClientSession.ts`

Backward-compatible health observer for live readiness timeout and one-shot removal cleanup

New module: `false`.

- Required symbol: `type LiveSubscriptionHealth`
- Required symbol: `onHealth?.("removed")`
- Required symbol: `subscription.onHealth?.("timeout")`
- Invocation: `desktop/src/shared/api/relayClientSession.ts` → `subscribe`; exact call `return this.subscribe(
      filter,
      onEvent,
      onReady,
      readinessTimeoutMs,
      signal,
      undefined,
      onHealth,
    );`; behavior test `unconfirmed timeout cannot hydrate or finish and confirmed retry starts a fresh session`; verify `cd desktop && node --import ./test-loader.mjs --experimental-strip-types --test --test-name-pattern="unconfirmed timeout" src/features/agents/lib/usePersonaSyncRelayHealth.test.mjs`
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

Atomic legacy binding/origin snapshot, read-only key verification, prewrite scope fence, shared bypass and durable signed-head retry

New module: `true`.

- Required symbol: `fn migrate_device_homes_locked`
- Required symbol: `fn migrate_device_homes_in_dir`
- Required symbol: `fn may_publish_local_instance`
- Required symbol: `fn publication_allowed`
- Required symbol: `fn needs_private_authority`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_migration.rs` → `resolve`; exact call `resolve(&records[i])?`; behavior test `managed_agents::device_home_migration::tests::legacy_claim_requires_available_key`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::device_home_migration::tests::legacy_claim_requires_available_key -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/device_home_migration.rs` → `matches`; exact call `context.proof.matches(b)`; behavior test `managed_agents::device_home_migration::tests::json_copy_does_not_publish`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::device_home_migration::tests::json_copy_does_not_publish -- --exact`
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

Guard each30177 instance publication; proof/context failure propagates in private scopes, shared-only best effort preserved

New module: `false`.

- Required symbol: `fn reconcile_agents_in_dir_with_context`
- Invocation: `desktop/src-tauri/src/managed_agents/reconcile.rs` → `publication_allowed`; exact call `super::device_home_migration::publication_allowed(record, definition.as_ref(), context)?`; behavior test `managed_agents::device_home_migration::tests::copy_suppresses_both_outbound_kinds_and_restore_candidates`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::device_home_migration::tests::copy_suppresses_both_outbound_kinds_and_restore_candidates -- --exact`
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

Select proven/shared/standalone auto-start candidates before key hydration or lifecycle work, retaining existing live-pair duplicate guards; top-level Wry entry wiring is compile coverage with native acceptance outstanding; authority lookup excludes private records not selected for auto-start; PhaseA and fresh PhaseC protected raw-store merge preserves excluded rows/definitions, keys restricted to actual authorized targets, baseline safe disabled housekeeping remains structural-only; mesh preflight error uses same protected writeback; retain spawned-child ownership across fresh authority failures and delegate bounded settlement/retry to child_ownership

New module: `false`.

- Required symbol: `fn select_auto_start_candidates`
- Required symbol: `fn needs_auto_start_authority`
- Required symbol: `fn prepare_restore_phase_a_with`
- Required symbol: `fn complete_restore_phase_c_with`
- Required symbol: `fn authorized_restore_updates`
- Required symbol: `fn persist_restore_error_with`
- Required symbol: `child_ownership::complete_restore_spawn_results_with`
- Required symbol: `child_ownership::retry_restore_cleanup`
- Invocation: `desktop/src-tauri/src/managed_agents/restore.rs` → `auto_start_allowed`; exact call `super::device_home_migration::auto_start_allowed(record, &definitions, context)?`; behavior test `managed_agents::runtime_commands::device_home_job_tests::copied_and_deferred_auto_start_jobs_have_zero_hydration_and_probes`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::runtime_commands::device_home_job_tests::copied_and_deferred_auto_start_jobs_have_zero_hydration_and_probes -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore.rs` → `select_auto_start_candidates`; exact call `select_auto_start_candidates(&policy_records, context.as_ref())?;`; behavior test `managed_agents::restore::device_home_restore_tests::mixed_restore_phase_a_save_and_phase_c_writeback_preserve_foreign_inline_copy`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::mixed_restore_phase_a_save_and_phase_c_writeback_preserve_foreign_inline_copy -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore.rs` → `save_restore_records_with`; exact call `super::storage::save_restore_records_with(app, &records, &eligible, persist)?;`; behavior test `managed_agents::restore::device_home_restore_tests::mixed_restore_phase_a_save_and_phase_c_writeback_preserve_foreign_inline_copy`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::mixed_restore_phase_a_save_and_phase_c_writeback_preserve_foreign_inline_copy -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore.rs` → `save_restore_records_with`; exact call `super::storage::save_restore_records_with(app, &records, &key_targets, persist)?;`; behavior test `managed_agents::restore::device_home_restore_tests::mixed_restore_phase_c_reload_and_save_never_import_excluded_copy`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::mixed_restore_phase_c_reload_and_save_never_import_excluded_copy -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore.rs` → `authorized_restore_updates`; exact call `authorized_restore_updates(&policy_records, context.as_ref())?;`; behavior test `managed_agents::restore::device_home_restore_tests::disabled_safe_housekeeping_persists_without_key_operations`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::disabled_safe_housekeeping_persists_without_key_operations -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore.rs` → `authorized_restore_updates`; exact call `authorized_restore_updates(&raw, context.as_ref())?;`; behavior test `managed_agents::restore::device_home_restore_tests::phase_c_fresh_authority_blocks_changed_target_and_preserves_concurrent_rows`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::phase_c_fresh_authority_blocks_changed_target_and_preserves_concurrent_rows -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore.rs` → `complete_restore_phase_c_with`; exact call `complete_restore_phase_c_with(
        app,
        &[pubkey.to_string()].into_iter().collect(),`; behavior test `managed_agents::restore::device_home_restore_tests::mesh_preflight_error_writeback_preserves_excluded_inline_copy`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml --features mesh-llm managed_agents::restore::device_home_restore_tests::mesh_preflight_error_writeback_preserves_excluded_inline_copy -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore.rs` → `complete_restore_spawn_results_with`; exact call `child_ownership::complete_restore_spawn_results_with(
        app,
        spawn_results,`; behavior test `managed_agents::restore::device_home_restore_tests::post_spawn_authority_error_settles_all_owned_children`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::post_spawn_authority_error_settles_all_owned_children -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore.rs` → `retry_restore_cleanup`; exact call `child_ownership::retry_restore_cleanup(app)?;`; behavior test `managed_agents::restore::device_home_restore_tests::failed_child_cleanup_propagates_and_retains_retry_ownership`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::failed_child_cleanup_propagates_and_retains_retry_ownership -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/runtime_commands.rs`

Guard actual auto-start job selection before key hydration and bounded relay probes; preserve shared and standalone baseline; shared-only selected jobs avoid proof lookup even when unrelated disabled private rows exist

New module: `false`.

- Required symbol: `fn auto_start_jobs_with`
- Required symbol: `async fn probe_auto_start_jobs`
- Invocation: `desktop/src-tauri/src/managed_agents/runtime_commands.rs` → `select_auto_start_candidates`; exact call `super::restore::select_auto_start_candidates(&records, context.as_ref())?;`; behavior test `managed_agents::runtime_commands::device_home_job_tests::copied_and_deferred_auto_start_jobs_have_zero_hydration_and_probes`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::runtime_commands::device_home_job_tests::copied_and_deferred_auto_start_jobs_have_zero_hydration_and_probes -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/runtime_commands.rs` → `needs_auto_start_authority`; exact call `super::restore::needs_auto_start_authority(&records)`; behavior test `managed_agents::runtime_commands::device_home_job_tests::shared_jobs_ignore_unselected_private_authority`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::runtime_commands::device_home_job_tests::shared_jobs_ignore_unselected_private_authority -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/migration.rs`

Document owner-dependent device-home migration belongs after fold/detach in workspace apply, never preidentity boot

New module: `false`.

- Required symbol: `Device-home migration deliberately waits for owner/workspace hydration`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/managed_agents/reconcile/tests.rs`

Existing linked-instance slim event/idempotence fixture injects matching host proof through guarded production reconcile engine

New module: `false`.

- Required symbol: `fn slimming_republish_wave_is_one_time`
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/commands/workspace_device_home_tests.rs`

Task4 fix1 regression tests bind production owning orchestration with temporary app storage and injected existing authority/process/KeyStore boundaries

New module: `true`.

- Required symbol: `fn fresh_workspace_preparation_initializes_scope_without_authorizing_absence`
- Required symbol: `fn workspace_preparation_database_open_and_schema_errors_are_fatal`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml device_home_preparation_tests`

## `desktop/src-tauri/src/managed_agents/restore/device_home_tests.rs`

Task4 fix1 regression tests bind production owning orchestration with temporary app storage and injected existing authority/process/KeyStore boundaries

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
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml device_home_restore_tests`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --features mesh-llm device_home_restore_tests`

## `desktop/src-tauri/src/managed_agents/restore/child_ownership.rs`

Own spawned restore children through authorized registration or bounded terminate/reap; retain failed cleanup handles in an app-owned queue reached by restore retry and shutdown without replacing existing tracked children

New module: `true`.

- Required symbol: `fn complete_restore_spawn_results_with`
- Required symbol: `fn settle_restore_child`
- Required symbol: `fn retry_restore_cleanup`
- Required symbol: `fn wait_for_restore_child_exit`
- Required symbol: `struct RestoreCleanup`
- Invocation: `desktop/src-tauri/src/managed_agents/restore/child_ownership.rs` → `settle_restore_child`; exact call `settle_restore_child(app, key, process, &mut cleanup)`; behavior test `managed_agents::restore::device_home_restore_tests::post_spawn_authority_error_settles_all_owned_children`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::post_spawn_authority_error_settles_all_owned_children -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore/child_ownership.rs` → `retry_restore_cleanup_with`; exact call `retry_restore_cleanup_with(app, terminate_restore_child)`; behavior test `managed_agents::restore::device_home_restore_tests::failed_child_cleanup_propagates_and_retains_retry_ownership`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::failed_child_cleanup_propagates_and_retains_retry_ownership -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore/child_ownership.rs` → `terminate_process`; exact call `super::super::terminate_process(process.child.id())?;`; behavior test `managed_agents::restore::device_home_restore_tests::restore_child_exit_confirmation_is_bounded_and_tree_is_reaped`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::restore_child_exit_confirmation_is_bounded_and_tree_is_reaped -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore/child_ownership.rs` → `wait_for_restore_child_exit`; exact call `wait_for_restore_child_exit(process, std::time::Duration::from_secs(1))`; behavior test `managed_agents::restore::device_home_restore_tests::restore_child_exit_confirmation_is_bounded_and_tree_is_reaped`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::restore_child_exit_confirmation_is_bounded_and_tree_is_reaped -- --exact`
- Invocation: `desktop/src-tauri/src/managed_agents/restore/child_ownership.rs` → `write_agent_runtime_receipt`; exact call `super::super::write_agent_runtime_receipt(app, &receipt)`; behavior test `managed_agents::restore::device_home_restore_tests::receipt_failure_settles_unregistered_child_and_preserves_error`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::receipt_failure_settles_unregistered_child_and_preserves_error -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`
- Verify: `cargo test --manifest-path desktop/src-tauri/Cargo.toml --workspace --features mesh-llm`

## `desktop/src-tauri/src/shutdown.rs`

Retry independently owned failed restore cleanup under shutdown transition before structural/key reads; preserve cleanup errors while existing tracked-agent shutdown continues

New module: `false`.

- Required symbol: `let restore_cleanup_error = managed_agents::retry_restore_cleanup(app).err();`
- Required symbol: `match restore_cleanup_error`
- Invocation: `desktop/src-tauri/src/shutdown.rs` → `retry_restore_cleanup`; exact call `managed_agents::retry_restore_cleanup(app).err();`; behavior test `managed_agents::restore::device_home_restore_tests::failed_child_cleanup_propagates_and_retains_retry_ownership`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml managed_agents::restore::device_home_restore_tests::failed_child_cleanup_propagates_and_retains_retry_ownership -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`
