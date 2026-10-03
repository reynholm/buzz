# Device-bound agents Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep an agent executable on its home Desktop unless its owner explicitly permits execution on other own devices.

**Architecture:** A single Rust policy evaluates device proof, public origin and scoped retained instance events. Creation, lifecycle and deletion enforce that policy; React consumes its computed capabilities. Durable home-operation recovery coordinates JSON changes with retained release/tombstone events.

**Tech Stack:** Existing Tauri/Rust, serde, SQLite retention, OS keychain, React/TypeScript, Node test runner and jsdom; Hermit toolchain.

**Spec:** [Approved design](../specs/2026-10-03-device-bound-agents-design.md). Approved by Andrey B. in Buzz event `6bfd6a11243107b0c5a47086cb56e2d06190cb7513536ba1b2375946819973b2`.

## Global Constraints

- Existing worktree: `/Users/reynholm/.buzz/REPOS/buzz-device-fork`; branch `fork/device-bound-agents`; baseline `desktop-v0.5.26`, `2b4b138dc5cf2d9cc1a0ceb21d9063ff56fe8bf4`. Read repository and applicable path-local AGENTS.md before edits.
- Changes belong to Desktop; relay/mobile/ACP instruction authorization remain unchanged. `shared` means community catalog publication, independently of device sharing.
- `share_across_devices: Option<bool>`: only `Some(true)` preserves unrestricted device execution; false/absent mean private. V1 toggle is creation-only, default false, including legacy and drafts without the field.
- Public optional metadata: `origin_device_id`, `origin_device_label`, `origin_released`. Append event fields; omit None; exclude all four new public fields from persona content hash.
- Local-only `device_host_binding: Option<String>` never enters IPC create input, public events, catalog exports or snapshots. No keychain marker fallback to JSON.
- Production marker service `buzz-desktop-device-host`. Device JSON path `<app_data_dir>/device.json`; UUID v4; default label hostname; nonempty trimmed label.
- Russian copy: `Разрешить запуск на других моих устройствах`; `При включении на другой машине можно создать отдельный экземпляр этого агента`; `Имя этого устройства`; `На другом устройстве`; `Работает на: <label>`.
- Backend refusal has stable code `definition_hosted_elsewhere` and home label. Missing evidence or read failure cannot authorize an action.
- No transfer, automated duplicate repair or policy editing after creation. Stop/offline never release a home.
- Activation: `. ./bin/activate-hermit`. Full Rust Desktop suite: `just desktop-tauri-test`; frontend: `just desktop-test`. Type gates: `just desktop-tauri-check desktop-typecheck`. Do not substitute module-only test passes.
- Every commit uses `/Users/reynholm/.local/bin/git`; immediately verify effective user.name/email match globals, email nonempty, commit.gpgsign empty after activation. Stage explicit task paths, diff-check, commit `-s`; abort if identity guard fails. Never use a guessed trailer.
- Independent review is required for persistence/lifecycle changes. At every seam, deliberately remove/bypass its guard and verify at least one behavior test fails, then restore it before committing.
- Delivery also requires [fork maintenance plan](2026-10-03-buzz-fork-maintenance.md). Owner installation, two-device acceptance and a real subsequent upstream tag remain explicit gates; no root ✅ while any agreed gate is outstanding.

## Review Focus

1. Keychain unavailable with an already bound JSON record: fail closed without issuing a new binding (Tasks 2, 4, 6).
2. Workspace switches while creation awaits provider/preflight work: do not publish or persist into the new relay/owner scope (Task 5).
3. Recovery encounters a new home instance after an interrupted deletion: remove only the old target and do not publish an obsolete release (Task 8).
4. Create returns a definition without computed home: immediate Start must fetch authoritative capabilities, never assume unclaimed (Task 9).
5. Private/shared selection combined with Only me/allowlist/Anyone: preserve instruction authorization independently in create, reuse and restore (Tasks 5, 10, 12).

---

## File responsibilities and interface conventions

- `managed_agents/types.rs`, `types/requests.rs`, `persona_events.rs`: storage/wire compatibility, creation-only input, explicit hash exclusions. `agent_snapshot.rs` remains an explicit portable allowlist.
- New `device_identity.rs`: atomic public identity and verified non-synchronizing marker; `commands/device_identity.rs`: narrow IPC, not lifecycle logic.
- New `managed_agents/definition_home.rs`: pure decision plus scoped evidence adapter; new `device_home_migration.rs`: boot claim/recovery wiring.
- New `managed_agents/device_home_operations.rs`: durable delete/label intents; existing retention and pending-event helpers retain ownership of event signing, timestamps and SQL.
- Existing command/runtime entry points call the policy; they do not implement their own version of home classification.
- New `managed_agents/persona_device_view.rs`: flattened definition plus computed home/capabilities. This is a command response, never a saved record.
- New `shared/api/deviceTypes.ts`, `tauriDevice.ts`, `features/agents/lib/definitionCapabilities.ts`: IPC types and frontend action decision. UI components render the result.
- Tests live beside the production responsibility, using existing Rust test modules, Node `.test.mjs` and mounted `.jsdom-test.mjs`; no Vitest dependency.

New Rust interfaces below use existing `AgentDefinition`, `ManagedAgentRecord`, `RetentionScope`, `AppHandle`, `AppState`, `nostr::Event` and `rusqlite::Connection`. `HostProof` contains an opaque marker and is not Serialize. `DevicePolicyContext { scope: RetentionScope, identity: DeviceIdentity, proof: HostProof, evidence: Vec<RemoteInstanceEvidence> }` owns the captured inputs; it is refreshed after unlocked work. No std mutex guard crosses `.await`.

### Task 1: Compatible public policy and local binding records

**Files:** Modify `desktop/src-tauri/src/managed_agents/{types.rs,types/requests.rs,persona_events.rs,agent_events.rs,agent_snapshot.rs}` and `docs/nips/NIP-AP.md`. Test `managed_agents/persona_events/tests.rs`, `managed_agents/agent_snapshot_tests.rs`; create `managed_agents/device_policy_types_tests.rs` and register it in `managed_agents/mod.rs`.

**Interfaces:** Add the four public Option fields to `AgentDefinition`, `ManagedAgentRecord` and `PersonaEventContent`; add binding only to `ManagedAgentRecord`. `CreatePersonaRequest.share_across_devices: Option<bool>` is camelCase on IPC; UpdatePersonaRequest remains without a policy setter. `AgentDefinition::into_agent_record` and `ManagedAgentRecord::to_definition_view` preserve public metadata in both directions.

- [ ] Write `legacy_policy_bytes_and_hash_stay_identical`, `policy_roundtrip_survives_unified_projection`, `portable_exports_exclude_device_metadata`. Assert legacy serialized event bytes equal existing reference vectors; metadata changes leave `persona_content_hash` equal; true/false survive both projection directions; exported 30177 and snapshot have none of the five fields, including binding. Deserialize a legacy record without fields successfully.

```rust
assert_eq!(legacy_bytes, reference_bytes);
assert_eq!(persona_content_hash(&before), persona_content_hash(&after));
assert!(snapshot_json.get("device_host_binding").is_none());
```

- [ ] Run `just desktop-tauri-test`; expect new compatibility assertions to fail before production changes; record the failing assertion and HEAD.
- [ ] Add optional fields with `serde(default, skip_serializing_if = "Option::is_none")`; append public event fields in the specified order. Explicitly omit metadata in hash projection, agent event allowlist and snapshot; update all struct literals mechanically, keeping legacy defaults None. Update NIP-AP optional-field and old-client enforcement limitations.
- [ ] Run `just desktop-tauri-test desktop-tauri-check`; expect zero failures, old vectors unchanged. Inspect diff for accidental metadata export or request binding fields.
- [ ] Commit explicit listed files and necessary struct-literal compile repairs: `feat(desktop): add compatible device policy metadata`.

### Task 2: Stable device identity and keychain proof

**Files:** Create `desktop/src-tauri/src/device_identity.rs`, `device_identity/tests.rs`, `commands/device_identity.rs`; modify `secret_store.rs`, `build_identity.rs`, `lib.rs`, `commands/mod.rs`.

**Interfaces:** `DeviceIdentity { device_id: String, label: String, created_at: String }`; `HostProof` exposes marker comparison inside Desktop only. `load_or_create_device_identity(path: &Path, default_label: &str) -> Result<DeviceIdentity, String>`; `load_or_create_host_proof(store: &SecretStore) -> Result<HostProof, String>`; `get_device_identity(app: AppHandle) -> Result<DeviceIdentity, String>`. Add `SecretStore::get_or_create_verified(&self, key: &str, generate: impl FnOnce() -> String) -> Result<String, String>` using its existing fresh-read interprocess mutation lock. Label setter is deferred to Task 8.

- [ ] Write `concurrent_identity_initialization_has_one_uuid`, `deleted_device_json_keeps_host_proof`, `unavailable_keychain_does_not_rebind`, `demo_marker_does_not_touch_production`. Assert concurrent processes see one durable UUID; recreating JSON changes public id but not marker; injected load/store/verify failure returns Err with no fallback file; demo service differs from production.

```rust
assert_eq!(first.device_id, concurrent.device_id);
assert_ne!(first.device_id, recreated.device_id);
assert!(load_or_create_host_proof(&unavailable_store).is_err());
```

- [ ] Run `just desktop-tauri-test`; expect these new identity tests to fail. All tests use temporary directories and injectable secret storage, never the user's production keychain.
- [ ] Serialize creation with an interprocess lock and atomic rename; fail on malformed JSON instead of resetting it. Under SecretStore's blob lock, use an existing value or generate once, persist and verify raw bytes. Production service stays exactly `buzz-desktop-device-host`; named demo service is `buzz-desktop-device-host-demo.<slug>`, consistent with existing build_identity isolation. Do not use SecretStore::shared or introduce unsafe code.
- [ ] Run `just desktop-tauri-test desktop-tauri-check`; expect pass including existing secret-store concurrency and demo tests. Confirm no fallback marker is serialized.
- [ ] Commit `feat(desktop): establish local device identity and host proof`.

### Task 3: One home classification and command projection

**Files:** Create `desktop/src-tauri/src/managed_agents/definition_home.rs`, `definition_home/tests.rs`, `persona_device_view.rs`; modify `managed_agents/mod.rs`, `commands/personas/mod.rs`.

**Interfaces:** `HomeKind = Local | Remote | Unclaimed`; `DefinitionHome { kind, label: Option<String>, remote_instance_pubkeys: Vec<String> }`; `DefinitionCapabilities { can_create_instance: bool, can_delete_definition: bool }`; `RemoteInstanceEvidence { pubkey: String, device_id: Option<String>, device_label: Option<String> }`. `classify_definition_home(definition: &AgentDefinition, instances: &[ManagedAgentRecord], device: &DeviceIdentity, proof: &HostProof, evidence: &[RemoteInstanceEvidence]) -> DefinitionHome`; `definition_capabilities(definition: &AgentDefinition, home: &DefinitionHome) -> DefinitionCapabilities`; `load_device_policy_context(app: &AppHandle, state: &AppState) -> Result<DevicePolicyContext, String>` captures owner/relay. `PersonaDeviceView` uses serde(flatten) for definition, adds home/capabilities; list_personas returns `Vec<PersonaDeviceView>`.

- [ ] Write `home_matrix`, `release_never_overrides_active_instance`, `evidence_is_owner_and_relay_scoped`, `read_failure_is_not_unclaimed`. Enumerate true/false/None, own/foreign/no binding, own/foreign/absent/released origin. Assert own proof wins; foreign binding refuses spawn; active foreign 30177 blocks released origin; own origin without instance is Local; absent/released origin without evidence is Unclaimed; true permits both capabilities. Wrong-owner/relay and tombstoned heads contribute no remote pubkeys.

```rust
assert_eq!(home.kind, HomeKind::Remote);
assert!(!definition_capabilities(&private_definition, &home).can_create_instance);
assert_eq!(released_without_heads.kind, HomeKind::Unclaimed);
```

- [ ] Run `just desktop-tauri-test`; expect new policy tests to fail.
- [ ] Implement pure classification and capabilities; adapter reads only active retained kind30177 for captured relay+owner through existing retention APIs, validating author/content and excluding deleted heads. A proven local instance means binding matches proof, not merely pubkey or auth_tag. No key minting or claim in list. Error propagates; flatten response remains compatible with existing definition JSON consumers.
- [ ] Run `just desktop-tauri-test desktop-tauri-check`; expect pass. List should not mutate store, keychain or retained events.
- [ ] Commit `feat(desktop): classify agent homes and expose capabilities`.

### Task 4: Legacy migration and recovery before publication or restore

**Files:** Create `desktop/src-tauri/src/managed_agents/device_home_migration.rs`, `device_home_migration/tests.rs`; modify `commands/workspace.rs`, `migration.rs`, `managed_agents/mod.rs`, `managed_agents/reconcile.rs`, `event_sync.rs`.

**Interfaces:** `migrate_device_homes_locked(app: &AppHandle, context: &DevicePolicyContext) -> Result<(), String>` runs with existing store lock after fold/detach and owner/workspace hydration, before `run_event_sync_blocking` and restore. `may_publish_local_instance(record: &ManagedAgentRecord, definition: Option<&AgentDefinition>, context: &DevicePolicyContext) -> Result<bool, String>` is also used by reconciliation. Task 8 inserts operation recovery before this migration.

- [ ] Write `legacy_claim_requires_available_key`, `legacy_foreign_origin_is_not_claimed`, `migration_is_idempotent`, `json_copy_does_not_publish`, `new_public_id_reclaims_proven_home`. Assert actual key lookup is necessary, empty pubkey/auth_tag alone insufficient; conflicting foreign origin unchanged; second run writes/publishes nothing; copied binding produces no outbound30177/30175; deleted device.json with preserved marker updates origin and queues30175 without new agent key. Keychain errors leave disk unchanged and workspace apply failed.

```rust
assert_eq!(record_after_second_run.device_host_binding, record_after_first_run.device_host_binding);
assert_eq!(foreign_copy_publish_count, 0);
assert_eq!(reclaimed_definition.origin_device_id.as_deref(), Some(new_device.device_id.as_str()));
```

- [ ] Run `just desktop-tauri-test`; expect failed claim/publication assertions.
- [ ] Apply migration to existing key-bearing records only; validate key using existing key resolver, then persist binding and origin atomically under lock. Unclaimed keyless definitions remain unclaimed. Add hook in workspace apply before scoped event-sync, not in preidentity process setup. Disable reconcile publication for copied foreign records. Preserve existing scope adoption and team-head ordering. Document irreducible pre-migration inline-key-copy limitation.
- [ ] Run `just desktop-tauri-test desktop-tauri-check`; expect pass including workspace/event-sync tests. Mutate proof check to show copy test fails, restore it.
- [ ] Commit `feat(desktop): migrate device homes before runtime restoration`.

### Task 5: Creation barriers and defaults for every source

**Files:** Modify `desktop/src-tauri/src/commands/{agents.rs,personas/create.rs,personas/snapshot/import.rs,team_snapshot.rs,teams/adopt/apply.rs}`, `managed_agents/personas.rs`, `migration/fold.rs`; create `managed_agents/device_creation.rs`, `device_creation/tests.rs`; register module.

**Interfaces:** `stamp_new_definition(definition: &mut AgentDefinition, requested_share: Option<bool>, device: &DeviceIdentity)` sets explicit policy, own origin, release false; import callers always pass None. `authorize_definition_action(context: &DevicePolicyContext, definition: &AgentDefinition, instances: &[ManagedAgentRecord], action: DefinitionAction) -> Result<(), String>` with `DefinitionAction = CreateInstance | StartInstance | DeleteDefinition`; common error contains code+label. `bind_new_instance(record: &mut ManagedAgentRecord, proof: &HostProof)` is backend-only.

- [ ] Write `remote_create_has_no_mint_publish_or_save`, `create_rechecks_home_after_await`, `create_scope_is_pinned`, `all_new_sources_default_private`, `sharing_does_not_change_respond_to`. Assert refusal before Keys::generate; another home claim during await prevents phase3 persistence/publication; community/owner switch returns error rather than redirecting work; dialog/draft/builtin/catalog/team/snapshot default false with current origin; explicit true preserved only local create. Test true/false × owner-only/allowlist/anyone with exact unchanged access fields.

```rust
assert_eq!((mint_count, save_count, publish_count), (0, 0, 0));
assert_eq!(new_definition.share_across_devices, Some(false));
assert_eq!(shared_instance.respond_to, original_instance.respond_to);
```

- [ ] Run `just desktop-tauri-test`; expect failed barrier/default assertions, with fake mint/persist/publish counters.
- [ ] Guard phase1 before key generation and phase3 after reload under store lock in create_managed_agent. Capture scope before awaited provider validation/auth/preflight and assert it still matches before side effects. Assign binding internally. Stamp all local new-definition constructors, including standalone migration manufacture; inbound deserialization is not a local creation source. Snapshot/catalog import ignores foreign lineage/policy. Inspect draft approval to confirm absent field reaches false; no changes to relay draft protocol.
- [ ] Run `just desktop-tauri-test desktop-tauri-check`; expect pass. Remove each create guard separately and verify counter/changed-home test fails, then restore it.
- [ ] Commit `feat(desktop): enforce device policy during agent creation`.

### Task 6: Guard every runtime path including provider execution

**Files:** Modify `desktop/src-tauri/src/managed_agents/{runtime.rs,runtime_commands.rs,restore.rs}`, `commands/{agents.rs,agents_deploy.rs,agents/provider_deploy.rs,agent_models_update.rs,global_agent_config.rs,agent_discovery.rs,personas/inbound.rs}`; create `managed_agents/device_runtime_tests.rs`.

**Interfaces:** `authorize_instance_start(record: &ManagedAgentRecord, definition: Option<&AgentDefinition>, context: &DevicePolicyContext) -> Result<(), String>` uses Task5 action authorization and verifies private binding. Existing `spawn_agent_child` and `start_local_agent_pairs_with_preflight` keep signatures; add device context at their internal policy boundary, without recursively acquiring a held store lock.

- [ ] Write `copied_record_never_reaches_spawn`, `start_refuses_before_terminating_existing_receipt`, `restore_and_reconcile_skip_foreign_pairs`, `provider_redeploy_obeys_home`, `shared_lifecycle_matches_baseline`. Assert process/log/provider-deploy spies see zero effects for private copied data; restart refusal does not kill a permitted existing process; auth_tag/inline key cannot override binding; no-keychain-access is Err; shared true runs existing path. Execute actual entry-point seams, not only pure policy helpers.

```rust
assert_eq!((spawn_count, provider_deploy_count, terminate_count), (0, 0, 0));
assert!(authorize_instance_start(&foreign_copy, Some(&private_definition), &context).is_err());
```

- [ ] Run `just desktop-tauri-test`; expect at least one failing test for each actual manual/restore/reconcile/provider/inbound restart entry point.
- [ ] Guard common spawn before logs/process launch; guard start_pair before terminate/reuse effects; guard preflight both before await and fresh store reload. Filter restore/reconcile candidates before probes, retain final spawn checks against current evidence. Guard payload/deploy for provider-backed agents. Existing definition-less legacy records retain their existing behavior; new dialog creates a definition first. Never use failed persona load as an empty authorized set.
- [ ] Run `just desktop-tauri-test desktop-tauri-check`; expect pass. Bypass each independent seam in turn and retain evidence that an entry-point test fails.
- [ ] Commit `fix(desktop): prevent foreign device runtime starts`.

### Task 7: Inbound metadata, tombstones and safe deletion policy

**Files:** Modify `desktop/src-tauri/src/commands/personas/{inbound.rs,mod.rs}`, `desktop/src-tauri/src/commands/teams/mod.rs`, `desktop/src-tauri/src/managed_agents/teams.rs`; extend `managed_agents/teams_tests.rs`; test `commands/personas/inbound/inbound_tests.rs`, `commands/personas/delete_cascade_tests.rs`; create `managed_agents/device_inbound_tests.rs`.

**Interfaces:** `merge_inbound_device_metadata(current: &AgentDefinition, incoming: &mut AgentDefinition, locally_proven: bool)` runs only for an accepted retention head; missing metadata preserves known values; proven local home retains current origin, while explicit sharing policy syncs. DeleteDefinition uses Task5 authorization before cascade/key cleanup/signing.

- [ ] Write `old_writer_cannot_clear_device_policy`, `stale_head_cannot_release_home`, `accepted_release_updates_remote_card`, `release_tombstone_orders_agree`, `remote_delete_has_no_cascade`, `owner_tombstone_still_applies`. Assert None fields from old writers keep known origin/policy; explicit accepted release updates remote; own proven home does not move on echo; both arrival orders remain blocked until last active30177 disappears; wrong-author events rejected; valid home-owner deletion applies remotely.

```rust
assert_eq!(merged.share_across_devices, Some(false));
assert_eq!(merged.origin_device_id, current.origin_device_id);
assert_eq!(delete_cascade_count, 0);
```

- [ ] Run `just desktop-tauri-test`; expect failed merge/delete assertions.
- [ ] Merge only after existing newest-wins retention decision. Reuse `commit_inbound_tombstone_with_store` for covered-head removal and timestamp ordering. Refresh persona projection on accepted30175/30177/k5 without invalidating catalog from local-only events. Guard local delete_persona and team cascades before all side effects; team deletion refuses atomically with reason when its requested cascade includes a remote definition. Do not guard legitimate inbound owner deletion as a local Delete intent.
- [ ] Run `just desktop-tauri-test desktop-tauri-check`; expect pass including stale inbound and team cascade suites; mutation-check both local-delete and tombstone-order seams.
- [ ] Commit `fix(desktop): retain device policy across sync and deletion`.

### Task 8: Durable release and label operations

**Files:** Create `desktop/src-tauri/src/managed_agents/device_home_operations.rs`, `device_home_operations/tests.rs`; modify `commands/{agents.rs,agents_pending.rs,device_identity.rs,workspace.rs}`, `managed_agents/{retention.rs,mod.rs}`.

**Interfaces:** `HomeOperation { id: String, relay_url: String, owner_pubkey: String, kind: HomeOperationKind, signed_events: Vec<Event> }`; kind Delete stores target pubkey/persona and expected store revision; kind Label stores new label and affected definition ids. Journal is `<app_data_dir>/device-home-operations.json`, atomically saved with mode0600, contains no key/auth_tag. `prepare_home_delete_locked(app: &AppHandle, context: &DevicePolicyContext, pubkey: &str) -> Result<HomeOperation, String>`; `recover_home_operations_locked(app: &AppHandle) -> Result<(), String>`; `enqueue_home_events(conn: &mut Connection, operation: &HomeOperation) -> Result<(), String>` uses one SQL transaction. IPC `set_device_label(app: AppHandle, state: State<AppState>, label: String) -> Result<DeviceLabelResult, String>` returns `{ identity: DeviceIdentity, publication: Queued | Complete }`.

- [ ] Write `last_instance_delete_queues_release_and_tombstone_atomically`, `partial_delete_and_stop_do_not_release`, `recovery_reuses_signed_event_ids`, `new_instance_cancels_obsolete_release`, `label_failure_keeps_retry_worklist`. Inject failure at intent write, unified save, each SQL enqueue, journal clear and restart. Assert recoverable intent survives; no partial retention batch; archived9035 included; replay targets original scope without minting; a new instance remains and blocks release; label persists with queued state while offline, content hash unchanged.

```rust
assert_eq!(replayed_event_ids, original_event_ids);
assert!(new_instance_still_exists);
assert!(!obsolete_release_enqueued);
```

- [ ] Run `just desktop-tauri-test`; expect journal/release assertions to fail.
- [ ] Before deleting JSON, sign release+k5+9035 at monotonic timestamps and durably save intent; then update unified snapshot once and transactionally enqueue all events, clear intent only after durable enqueue. Refactor agents_pending transaction internals for caller-owned transaction; do not nest BEGIN or swallow failure. Preserve existing assignment cleanup. Recovery runs before migration/sync/restore, opens original scope and compares current instances/revision: do not erase new instances; suppress obsolete release and retain tombstone/archive for the old target. Label intent precedes device.json update; create durable per-scope worklists for affected definitions across known owner/relay scopes, not just the active community; rebuild only definitions whose origin is current home and record retries durably. Publication via existing event-sync, no success promise before durable enqueue.
- [ ] Run `just desktop-tauri-test desktop-tauri-check`; expect all failure-injection cases pass. Verify retry offline and restart in isolated app-data using the production journal/retention seam.
- [ ] Commit `feat(desktop): recover durable device home operations`.

### Task 9: Frontend authoritative capability mapping

**Files:** Modify `desktop/src/shared/api/{personaTypes.ts,tauriPersonas.ts,types.ts}`; create `deviceTypes.ts`, `tauriDevice.ts`, `features/agents/lib/definitionCapabilities.ts`, `definitionCapabilities.test.mjs`; extend `shared/api/tauriPersonas.test.mjs`.

**Interfaces:** TS `DefinitionHome` has `kind: "local" | "remote" | "unclaimed"`, `label: string | null`, `remoteInstancePubkeys: string[]`; `DefinitionCapabilities` has `canCreateInstance`, `canDeleteDefinition`. AgentPersona receives optional policy/origin fields and optional home/capabilities for raw mutation responses. `requireDefinitionCapability(persona: AgentPersona, action: "createInstance" | "deleteDefinition"): void` refuses missing projection; `getDefinitionForAction(id: string, action: ...): Promise<AgentPersona>` reloads listPersonas and applies that check. Device IPC wrappers map snake_case to camelCase.

- [ ] Write `raw_home_maps_pubkeys_and_capabilities`, `missing_projection_never_authorizes_start`, `create_payload_default_false`, `edit_payload_does_not_change_policy`. Assert exact mapping; getter refresh after create observes backend refusal instead of treating missing home as unclaimed; create sends shareAcrossDevices false by default/true explicitly; update sends no policy/origin/binding fields. Preserve respondTo/allowlist mapping.

```javascript
assert.throws(() => requireDefinitionCapability(withoutProjection, "createInstance"));
assert.equal(createPayload.shareAcrossDevices, false);
assert.equal("shareAcrossDevices" in editPayload, false);
```

- [ ] Run `just desktop-test`; expect new API/action tests to fail.
- [ ] Map list computed fields, preserve absence distinctly in raw create/update results. Add authoritative action getter; create/start callers will use it in Task10. No client-origin stamp or local proof. Expose label queued result to settings.
- [ ] Run `just desktop-test desktop-typecheck`; expect pass across both Node and jsdom suites.
- [ ] Commit `feat(desktop): expose authoritative device capabilities to UI`.

### Task 10: Close frontend automated and interactive creation paths

**Files:** Modify `desktop/src/features/agents/lib/instanceInputForDefinition.ts`, `agents/ui/{useManagedAgentActions.ts,usePersonaActions.ts,useTeamActions.ts}`, `agents/{useAgentManagement.ts,channelAgents.ts}`, `onboarding/welcomeGuide.ts`, `profile/ui/UserProfilePanel.tsx`, `desktop/src-tauri/src/managed_agents/{types.rs,runtime.rs}`, `desktop/src/shared/api/{types.ts,tauri.ts}`; extend `agents/channelAgents.accessPolicy.test.mjs`; create `agents/lib/deviceActionPaths.test.mjs`.

**Interfaces:** Existing builder keeps signature and first calls Task9 requireDefinitionCapability. Every caller obtains fresh projection via getDefinitionForAction; `channelAgents` reuse checks both definition capability and instance eligibility from backend summary. Export `canReuseManagedAgentOnDevice` from the same frontend policy file; signature `(canStartOnDevice: boolean | undefined): boolean`, missing means false. Backend managed-agent summary supplies computed `can_start_on_device` through existing `build_managed_agent_summary` in runtime.rs, mapped to `ManagedAgent.canStartOnDevice` in tauri.ts; never persisted.

- [ ] Write `all_builder_callers_observe_remote_refusal`, `onboarding_and_team_skip_with_reason`, `channel_reuse_never_attaches_copied_instance`, `reuse_preserves_access_policy`. Mutation spies must see zero create/start calls for remote and no copied-agent membership change. Assert manual intent reports reason; automatic batches report skipped members; shared true retains prior create/reuse behavior; Only me/allowlist/Anyone have exactly the preexisting signed-access operations.

```javascript
assert.equal(createMutationCalls.length, 0);
assert.equal(startMutationCalls.length, 0);
assert.deepEqual(accessOperations, baselineAccessOperations);
```

- [ ] Run `just desktop-test`; expect failed mutation-spy assertions.
- [ ] Guard all five builder callers plus independent channelAgents paths. Keep channel access policy logic separate. Add backend summary capability using Task6 authorization without spawning. Block private remote definition deletion before team UI mutation; backend Task7 remains authority. Freeze production caller list in a test using rg/source discovery excluding declaration/tests; a new unguarded builder caller fails the test.
- [ ] Run `just desktop-test desktop-typecheck` and `just desktop-tauri-test`; expect pass. Record actual production builder callers and verify each has behavioral mutation-spy coverage.
- [ ] Commit `fix(desktop): apply device capabilities across agent actions`.

### Task 11: Creation toggle, remote card and device label settings

**Files:** Create `desktop/src/features/agents/ui/{AgentDeviceSharingField.tsx,PersonaRemoteRuntime.tsx,UnifiedAgentsSectionDeviceHome.jsdom-test.mjs}`, `settings/ui/{DeviceIdentitySettingsCard.tsx,DeviceIdentitySettingsCard.jsdom-test.mjs}`; modify `agents/ui/{AgentDialog.tsx,AgentDefinitionDialog.tsx,personaDialogState.ts,UnifiedAgentsSection.tsx,PersonaActionsMenu.tsx,RequestedAgentCreateDialogs.tsx}`, `settings/ui/AgentsSettingsPanel.tsx`, `agents/AGENTS.md`.

**Interfaces:** `AgentDeviceSharingField({ value, onChange }: { value: boolean; onChange: (value: boolean) => void })`; `PersonaRemoteRuntime({ persona }: { persona: AgentPersona })` reads relay presence by home.remoteInstancePubkeys using existing query; `DeviceIdentitySettingsCard()` owns label mutation/queued/error UI. Create dialog input carries shareAcrossDevices; edit shows read-only policy and home label.

- [ ] Mounted tests `create_toggle_defaults_off_and_survives_submit`, `duplicate_and_draft_default_off`, `remote_card_has_no_start_or_delete`, `presence_uses_pubkey_not_name`, `label_keyboard_save_reports_queued`. Assert checkbox off, explicit on sends true; duplicate resets false; remote avatar/name/description/Edit remain; `persona-runtime-remote-<id>` exists and Start/Delete absent; unrelated same-name pubkey cannot show online; failed label save visible; empty/whitespace rejected; queued distinct from completed.

```javascript
assert.equal(checkbox.checked, false);
assert.equal(screen.queryByTestId(`persona-runtime-start-${id}`), null);
assert.ok(screen.getByTestId(`persona-runtime-remote-${id}`));
```

- [ ] Run `just desktop-test`; expect mounted assertions to fail.
- [ ] Render exact Global Constraints copy; remote badge `На устройстве <label> · в сети/не в сети`, unknown fallback `На другом устройстве`. Existing edit shows `Работает на: <label>`; runtime controls use capabilities, not absence of local record. Settings uses rem tokens, keyboard labels and textual status. Update agents/AGENTS.md to distinguish catalog, device policy and instruction authorization. If provider/where-to-run dialog shares AgentDialog, enforce capability there before mutation too.
- [ ] Run `just desktop-test desktop-typecheck`; expect pass. Exercise actual create/edit/profile/team/onboarding/settings in isolated Desktop and retain screenshots plus observable IPC/network results at exact candidate SHA.
- [ ] Commit `feat(desktop): show agent device sharing and home status`.

### Task 12: Full validation and two-device acceptance handoff

**Files:** Create `docs/fork/DEVICE_AGENT_ACCEPTANCE.md`, `desktop/src-tauri/src/managed_agents/device_workflow_tests.rs` and register in `managed_agents/mod.rs`; update existing workspace research/log and maintenance plan's `FORK_PATCHES.md` through its owning task. No production refactor in this task.

**Interfaces:** Acceptance record contains candidate SHA, baseline SHA, artifact checksum, A/B device ids and labels, owner and agent pubkeys, exact commands and observed effects; secrets redacted. It distinguishes agent evidence, mandatory owner acceptance and future-tag gate.

- [ ] Add integration cases `private_two_device_single_pubkey`, `release_then_reclaim`, `shared_two_device_baseline`, `device_policy_access_matrix`. For automated Rust workflow tests, use two temporary stores, injectable proof stores and the production create/start/restore/reconcile/inbound seams with side-effect counters; then reproduce the same cases in two isolated native Desktop instances against a real local relay. Record automated and native observations separately. Assert no second private pubkey on B after repeated boot/onboarding/team/profile; released last instance becomes startable on B only after both release/tombstone; checkbox true allows baseline creation; owner-only rejects other human instructions while allowlist/Anyone retains existing behavior regardless of device policy.

```rust
assert_eq!(private_agent_pubkeys_after_repeated_b_boot.len(), 1);
assert!(!before_tombstone.can_create_instance);
assert!(after_release_and_tombstone.can_create_instance);
```

- [ ] Run the full integration workflow before declaring the feature implemented; record any failure as a blocker and fix its owning task with RED/GREEN. Simulated devices supplement the mandatory two physical notebook test; they do not satisfy it.
- [ ] Run at exact `git rev-parse HEAD`: `just fmt-check clippy test-unit desktop-check desktop-typecheck desktop-tauri-fmt-check desktop-tauri-test desktop-test`, then `just ci`. Also run original-spec `pnpm --dir desktop lint` and `pnpm --dir desktop build` (both scripts were confirmed on the baseline); record environment failure candidly, never narrow it into a passing claim. Repeat only after relevant fixes. Perform fresh branch review against VISION/TESTING/AGENTS and accepted spec; resolve concrete blockers.
- [ ] Document owner steps: install candidate on A/B preserving existing data; check legacy A local/B remote; delete device.json on A preserves local agent; copy managed-agents.json to B refuses spawn; change label updates B without restart badge; test Only me and allowed participant; remove last A instance and reclaim B; test shared on. Include keychain prompt, data backup and recovery instructions. Wait for explicit human confirmation before ready/review-completed/✅.
- [ ] Commit docs with `docs(fork): record device agent acceptance procedure`; publish candidate and remaining gates in the feature thread with Andrey B. mention. Follow maintenance plan for artifact/tag delivery. Feature completion requires both plans' gates, not this task's agent test results alone.

## Self-review and execution order

Tasks 1–3 establish storage/proof/policy; 4–8 integrate migration, creation, lifecycle, sync and recovery; 9–11 integrate UX; 12 validates the contract. Task8 recovery hook must precede migration in final boot order even though its implementation follows Task4. Backend Delete operations must not be released to users between Tasks7 and8. Maintenance baseline comparison can execute before product edits; candidate build follows all production tasks.

Coverage: spec §§3–4 → Tasks1–5; §5 → Tasks3,5–7,9–11; §6 → Tasks7–8; §7 → Tasks9–11; §8 → companion plan; §9 → each owning task plus Task12. All five Review Focus conditions have explicit tests. Existing build_identity is extended by companion plan, not replaced. No unimplemented helper is assumed to exist on the 0.5.26 baseline.

Self-review: local document links and existing Modify targets were checked; planned new paths consumed by later tasks are explicitly introduced above. Concrete assertion fragments show the decisive checks; test input construction follows each named case and existing package fixtures. Product suites have not been run during planning.

Before execution, owner reviews this plan and its companion and chooses Subagent-driven or Native as required by writing-plans. Do not treat spec approval as execution-method selection.
