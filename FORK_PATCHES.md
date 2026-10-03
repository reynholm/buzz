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

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

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

Register the device policy compatibility test module

New module: `false`.

- Required symbol: `mod device_policy_types_tests;`
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

Mechanical struct-literal compatibility repair: new device fields default to None

New module: `false`.

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
- Invocation: `desktop/src-tauri/src/device_identity.rs` → `validate_identity`; exact call `validate_identity(&identity)?;`; behavior test `malformed_identity_is_preserved_and_reported`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml device_identity::tests::malformed_identity_is_preserved_and_reported -- --exact`
- Invocation: `desktop/src-tauri/src/device_identity.rs` → `get_or_create_verified`; exact call `store.get_or_create_verified("host", || uuid::Uuid::new_v4().to_string())?`; behavior test `unavailable_keychain_does_not_rebind`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml device_identity::tests::unavailable_keychain_does_not_rebind -- --exact`
- Invocation: `desktop/src-tauri/src/device_identity.rs` → `lock`; exact call `lock.lock()`; behavior test `concurrent_identity_initialization_has_one_uuid`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml device_identity::tests::concurrent_identity_initialization_has_one_uuid -- --exact`
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
- Verify: `just desktop-tauri-test desktop-tauri-check`

## `desktop/src-tauri/src/lib.rs`

Register device identity module and public metadata IPC command

New module: `false`.

- Required symbol: `mod device_identity;`
- Required symbol: `commands::get_device_identity,`
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
- Invocation: `desktop/src-tauri/src/secret_store.rs` → `acquire_blob_lock`; exact call `let _lock = acquire_blob_lock(&self.service)?;`; behavior test `concurrent_host_proof_initialization_generates_once`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml device_identity::tests::concurrent_host_proof_initialization_generates_once -- --exact`
- Invocation: `desktop/src-tauri/src/secret_store.rs` → `read_blob_raw`; exact call `let raw = self.read_blob_raw()?;`; behavior test `verified_value_uses_fresh_storage_and_generates_once`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml device_identity::tests::verified_value_uses_fresh_storage_and_generates_once -- --exact`
- Invocation: `desktop/src-tauri/src/secret_store.rs` → `read_blob_raw`; exact call `if verify && self.read_blob_raw()?.as_deref() != Some(json.as_bytes())`; behavior test `unavailable_keychain_does_not_rebind`; verify `cargo test --manifest-path desktop/src-tauri/Cargo.toml device_identity::tests::unavailable_keychain_does_not_rebind -- --exact`
- Verify: `just desktop-tauri-test desktop-tauri-check`
