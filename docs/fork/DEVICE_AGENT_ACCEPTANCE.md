# Device agent acceptance

This procedure implements the owner acceptance contract in the
[approved design](../superpowers/specs/2026-10-03-device-bound-agents-design.md),
[contributor rules](../../AGENTS.md), [testing rules](../../TESTING.md), and
[fork maintenance guide](MAINTENANCE.md). Automated tests, an isolated native
run on one host, and two physical notebooks are separate evidence. None grants
owner review or installation approval.

Delivery requires owner review, native workflow, two physical notebooks, an
exact-final-SHA artifact and a genuine subsequent upstream tag. The final artifact
status is recorded after the last source commit in the task/workspace evidence.
The latest owner instruction authorizes local commits only. Do not push,
activate workflows, promote, install, or add `buzz-review-completed` on the basis
of agent tests. The owner performs review separately after implementation.

## Bind the record to the candidate

Use the final branch SHA after all implementation and maintenance commits,
including M5. Build/reverify it using the maintenance producer. Do not reuse an
earlier archive under a later SHA. M3's existing real artifacts belong only to
`fcbe5c649594e323b94224ab0634a81e884a6251`, not subsequent M4/Task12/M5 commits.
Its DMG SHA256 is
`60d66c59c0aef6e763bea16254e0e121c1bf01671604f064d396adf0d154cae7`.
That checksum identifies historical evidence; it is not the final delivery.

Capture exact source and checksum commands in the acceptance record:

```sh
git rev-parse HEAD
git rev-parse 'HEAD^{tree}'
git status --porcelain
cd artifacts/fork/<exact-candidate-sha>
shasum -a 256 -c SHA256SUMS
```

Record these fields, keeping secret keys, auth tags, credentials, private host
markers and message content out of logs:

| Field | Required value |
| --- | --- |
| Candidate SHA/tree | Exact committed source tested and built |
| Baseline tag/SHA | `desktop-v0.5.26` / `2b4b138dc5cf2d9cc1a0ceb21d9063ff56fe8bf4` |
| Artifact | DMG/archive name, size and SHA256; manifest and SHA256SUMS hashes |
| App identity | About upstream version, fork revision/full compiled SHA; bundle ID |
| A/B | Physical notebook description, device UUID and chosen label |
| Owner/agents | Owner public key and every observed private/shared agent public key |
| Environment | OS/architecture, relay URL, runtime/provider versions; no secrets |
| Observations | Exact commands/UI actions, timestamps, event IDs/kinds and observed effects |
| Evidence boundary | Automated, isolated native on one host, or owner physical test |
| Acceptance | Owner name/date and explicit pass/failure/reference for each gate |

A blank or pending observation is not a pass. A new source/artifact candidate
requires fresh applicable acceptance. An owner merge with a different Git tree
invalidates the candidate; rebuild/retest/reaccept it.

## Backup and recovery before owner installation

1. Quit Desktop and its managed runtime on both notebooks. Preserve a copy of
   each current app and a timestamped backup of each complete app-data directory,
   including device metadata, unified agent store and scoped retention/journals.
   Keep backups local and protected because stores can contain inline keys.
   Do not replace B's whole app-data with A's directory.
2. Verify each owner's existing supported identity/recovery backup is usable.
   A JSON backup does not restore the independent system-keychain host marker or
   missing agent keys. Keep recovery secrets offline; never paste them in this
   record. Do not delete the host-marker service or export the keychain for this test.
3. Install the exact verified candidate on A and B, preserving existing data.
   This is an owner action. Approve only expected Desktop keychain prompts after
   checking the app identity. Record whether a prompt appears and its outcome,
   without its secret contents. Refusing/locking keychain access must leave an
   honest error; it must not create a new host proof through JSON fallback.
4. For rollback, stop the candidate/runtimes, restore the prior app and that
   notebook's own protected data backup. Preserve failed candidate logs/journals
   for diagnosis. If keychain proof is unavailable, restore access through the
   supported OS recovery path; do not hand-edit bindings or copy A's proof to B.

## Owner physical workflow

Use the same owner identity and relay on two physical notebooks. Give them
recognizable labels in Settings → Agents → `Имя этого устройства`. Capture the
UUIDs and labels. Wait for successful scoped owner/relay history hydration;
`Обновить состояние` reloads projections and does not itself backfill history.

1. **Legacy and private default.** On A open the migrated existing agent and
   record its public key. A remains local/startable. On B the same definition is
   visible/editable with `На устройстве <A label>` (or `На другом устройстве`
   when unknown), while Start/Delete are absent. Create another agent on A with
   the device checkbox left off; verify definition-first creation and that its
   linked instance uses the recorded definition ID. Confirm the setting is
   creation-only and Edit cannot change it.
   Open the definition profile on B: its home state matches the directory and
   Start/Delete remain absent while Edit/Duplicate/Export remain available.
   For legacy data with a previously archived duplicate, verify A claims its
   existing local key after complete history plus a relay-signed archive read;
   the public key stays unchanged. An active duplicate or unavailable/invalid
   archive must still block automatic claim. An archived local key is not claimed.
   While WS/history remain healthy, stall NIP-11 headers/body or fail NIP-11/query.
   Completion must time out within 30 seconds and retain recovery; restore HTTP
   without reconnect/restart and confirm cooldown retry binds the same local key.
2. **Repeated B actions.** Restart B three times. Try Welcome/onboarding,
   profile Start, team activation/channel batch and direct relevant native
   command paths. Record refused/skipped IDs and synchronization versus remote
   reasons. No additional private public key, key creation, spawn, provider
   deploy or identity publication may originate on B. Existing permitted team
   members still work. Compare keys/event coordinates, not names or presence.
3. **Saved-definition recovery.** During creation make history unavailable after
   the definition is saved. The UI explains that the definition was saved and
   closes Create; no instance starts. Restore the connection/hydration and Start
   that saved definition. Exactly one definition exists. Exercise a backend
   refusal after a fresh UI allow; it must preserve the same saved definition
   and show the native reason without creating a duplicate.
4. **Public metadata loss.** Quit A, back up and delete only its `device.json`,
   then restart. The public UUID changes; its independent keychain proof and
   existing agent public key remain. The agent stays local, and new lineage is
   durably published. Verify B eventually receives the new device information.
5. **Copied record refusal.** Quit B and preserve its own store backup. Copy A's
   migrated `managed-agents.json` into B's isolated test data, without copying
   keychain/proof. Restart B and try Start/restart/restore/provider/inbound-access
   refresh. B must refuse the copied private instance even if JSON includes an
   inline key or `device_host_binding`. Restore B's original store afterwards.
   Historical legacy inline-key copies before migration are inherently
   indistinguishable; this test uses already bound records.
6. **Live label.** Change A's label to a nonempty trimmed value. B updates
   `Работает на: <label>` and its remote badge without restarting. Repeat offline:
   show queued status, reconnect, and observe durable delivery. Empty labels
   fail visibly. Record exact presence pubkeys and unknown/offline/online state;
   relay presence is not a claim of process health.
7. **Instruction access.** For both device policies test Only me, allowlist and
   Anyone on the hosting machine. Only me accepts the owner and verified
   same-owner agents, and rejects another human. Allowlist accepts the selected
   participant plus the owner/siblings and rejects an unlisted human. Anyone
   retains existing channel authorization behavior. Device sharing and community
   catalog publication must not change this audience. Repeat through any
   provider used in acceptance; a distribution owner-only clamp, if compiled,
   remains an independent constraint and must be recorded.
8. **Stop versus release.** Stop A or disconnect it: B remains blocked. Delete
   A's last private instance, leaving its definition. Capture the signed 30175
   release and kind:5 tombstone for 30177. Deliver either one first: B stays
   blocked until both have applied. After successful hydration of both, Start
   on B claims a new home/new instance; A becomes remote and refuses that copy.
   Repeat the reverse delivery order and replay the old tombstone; it must not
   delete B's new instance. No simultaneous offline transfer is promised.
9. **Shared baseline.** Create a new definition with
   `Разрешить запуск на других моих устройствах` checked. A and B can each create
   their own instance with distinct public keys, including existing Start,
   restore, team/profile and permitted provider behavior. Its selected
   instruction audience and catalog visibility remain unchanged.
10. **Native and maintenance checks.** Inspect About in Wry, confirm upstream
    version/bundle identity and exact compiled fork SHA, exercise the actual
    bundled sidecars in an agent/local-relay workflow and existing 0.5.26 data
    migration, and record keychain prompts. Test the maintenance command as the
    owner. A future real upstream desktop tag must then run the upgrade gates;
    a synthetic tag cannot satisfy that requirement.

Keep native screenshots scoped to the test app/fixture, with candidate SHA and
public observations. Browser release-smoke and jsdom screenshots prove only
browser/bridge behavior. Two isolated demo instances on one Mac supplement
these steps and never substitute for the two physical notebooks.

## Automated evidence and final owner receipt

The four workflow tests live in
[device_workflow_tests.rs](../../desktop/src-tauri/src/managed_agents/device_workflow_tests.rs).
They use two temporary data/retention stores, signed public events and independent
in-memory injected proof stores. They exercise production create/claim,
start, restore Phase A, reconcile, signed inbound/release/tombstone and runtime
refresh seams. Process/key boundaries are counted/injected and Tauri uses
MockRuntime. The access matrix covers all Desktop instruction modes, sharing
absent/false/true, own/foreign binding, Ready/Pending/Failed and distribution
clamp. Actual instruction-author authorization is owned by the unchanged ACP
suite, included in `just test-unit`; environment projection alone does not prove
message delivery or native runtime execution.

Run at a clean exact HEAD and retain contemporaneous HEAD/tree/source hashes,
commands, exits and raw logs:

```sh
. ./bin/activate-hermit
export CARGO_INCREMENTAL=0
just fmt-check clippy test-unit desktop-check desktop-typecheck desktop-tauri-fmt-check desktop-tauri-test desktop-test
just ci
pnpm --dir desktop lint
pnpm --dir desktop build
python3 -m unittest discover -s scripts/fork/tests -v
python3 scripts/fork/sync.py validate
env CHECK_FILE_SIZES_BASE=2b4b138dc5cf2d9cc1a0ceb21d9063ff56fe8bf4 node desktop/scripts/check-file-sizes.mjs
```

Full-suite/environment failures remain recorded. Existing frontend timing
history and Task9's bounded settlement fix remain part of provenance; a new
failure requires diagnosis rather than blind repeats or weakened assertions.

After explicit owner review, native and physical confirmation, use M4's
[exact-tree promotion receipt](MAINTENANCE.md#owner-merge-and-exact-tree-promotion)
with `candidate_sha`, `candidate_tree`, owner `merged_sha`, `base_tag`,
`upstream_sha`, `manifest_sha256`, `checksums_sha256`, `owner_login: "reynholm"`,
and the actual merged `pr_url`. Its `evidence` requires independently scoped
`accepted: true` and a nonempty reference for `owner_review`, `full_tests`,
`native_about`, `existing_data`, `sidecar_runtime`, `keychain_prompt`,
`two_physical_devices`, and `maintenance_command`. Leave all pending entries
unaccepted; never fill them from synthetic fixture booleans. Follow M4's
read-only preflight before any separately authorized publication.


## Handoff evidence is scoped; acceptance remains open

M5's full registry deletion campaign rejected all 78 new-module and 211 invocation
removals in disposable fixture trees and passed after exact restoration. This
proves registry protection, separately from the original owning runtime mutation
receipts. Task12 full CI/supplemental results are pinned to commit
`6f2ef4a026bc90a9ee1456a31f882c3d87baf997` and tree
`b182ee71e9194dfb4967e529fda5c7346f6bd336`; documentation-only successor commits
must cite unchanged source closure without renaming those test runs.

The named-demo native attempt built and launched, but host-proof keyring writes
failed with macOS `Operation not permitted`. Native authority/UI, real local-relay
agent workflow, keychain storage/prompt/recovery, existing-data and full sidecar
runtime acceptance remain pending. Do not record these as accepted from process
liveness, the compiled probe or mock/jsdom/two-store tests. The real upstream tag
query on 2026-10-04 found no newer stable tag; future-release criterion 17 remains
unavailable, and synthetic releases cannot satisfy it.

The final app/archive/DMG must identify the exact SHA after M5's last local commit.
Its checksums and build receipt belong in task/workspace evidence after that
commit; prior fcbe assets and Task12's demo stay unchanged historical artifacts.
If no exact-final build succeeds, label that artifact pending. In either case,
leave every owner receipt gate unaccepted until its actual confirmation exists.
Owner review, two physical notebooks, the maintenance command, confirmed GitHub
default branch/hosted schedule, protected promotion environment and actual owner
merge remain required. The requested local handoff ends with an unpushed branch;
it does not grant remote activation or overall completion status.
