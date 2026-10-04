# Fork maintenance

The accepted base is `desktop-v0.5.26`, commit
`2b4b138dc5cf2d9cc1a0ceb21d9063ff56fe8bf4`.
The approved contract is [design sections 8–9](../superpowers/specs/2026-10-03-device-bound-agents-design.md#8-форк-и-обновления).

Upstream wins for behavior outside registered patches. Retirement of a patch
requires an owner decision, even when upstream appears to provide an equivalent.
Maintain `upstream` as a fast-forward mirror and accepted releases on `fork/main`.
Use temporary `fork/sync-vX.Y.Z` merge branches without rebasing. The owner merges,
installs and authorizes final `fork-vX.Y.Z-N` tags; preceding artifacts are candidates.
Do not change the upstream app version or bundle identifier. Preserve demo isolation.

Every changed tracked path since the base, including tests and documentation,
belongs in `scripts/fork/patches.json`. New modules use `is_new: true`. Register
exact production call text, callee symbol and a behavior test/command for each
invocation seam. Run that command after deliberately removing the seam and restore
it before committing. Text validation catches drift; it does not execute behavior
tests or prove runtime reachability. Add new task files explicitly before validation
so the tracked diff includes them. Regenerate and validate with:

```sh
python3 scripts/fork/sync.py render
python3 scripts/fork/sync.py validate
python3 -m unittest discover -s scripts/fork/tests -v
```

## Clean upstream baseline

Run only in the disposable detached worktree
`/Users/reynholm/.buzz/REPOS/buzz-device-baseline-v0_5_26`; never launch the baseline
app against production data or keychain. Capture `git rev-parse HEAD` in the same
shell as each gate. Install dependencies with `pnpm install`; do not run `just setup`
or commands that bootstrap/migrate a live relay.

Required gates include `just ci` plus `just desktop-typecheck`; Desktop requires
full Rust and frontend suites, fmt/clippy/typechecks.
Build six real sidecars from this exact tree before invoking the release recipe:

```sh
. ./bin/activate-hermit
/Users/reynholm/.local/bin/git rev-parse HEAD
cargo build --release --target aarch64-apple-darwin -p buzz-acp -p buzz-agent \
  -p buzz-backend-kubernetes -p buzz-dev-mcp -p git-credential-nostr -p buzz-cli
./scripts/bundle-sidecars.sh aarch64-apple-darwin
just desktop-release-build aarch64-apple-darwin
```

Verify all six sidecars are executable, nonempty Mach-O arm64 binaries and record
SHA256SUMS with candidate SHA. Inspect actual bundled updater config/resources.
For fork builds clear `BUZZ_UPDATER_ENDPOINT` and `BUZZ_UPDATER_PUBLIC_KEY`; baseline
comparison reflects upstream environment and behavior. Empty placeholder binaries
are never baseline success. A failed clean tag is an upstream blocker, recorded
separately from candidate results; never silently fix the baseline.

Baseline status and exact logs/artifacts are recorded in the M1 task report under
`.superpowers/sdd/2026-10-03-buzz-fork-maintenance/`. No successful baseline is claimed
until all required commands and real artifact inspection finish. The M1 run identified
managed-runtime `BUZZ_ACP_SESSION_POLICY` and `GIT_CONFIG_*` contamination; clean
test subprocesses remove those settings. Native Git integration requires Git 2.46+
([helper prerequisites](../../crates/git-credential-nostr/README.md)); the task used
a local Git 2.49 toolchain without changing global Git configuration. On a runner
whose Finder automation times out, upstream supports `CI=true` packaging, which
skips Finder DMG decoration. Preserve initial failures alongside corrected evidence.

Scheduled updates require owner confirmation that `fork/main` is the GitHub default
branch. No new tags means no chat notification. A real next upstream tag after
0.5.26, owner installation and two-device acceptance remain explicit delivery gates.

## Recorded baseline build: 2026-10-03

Source was the untouched detached tag `desktop-v0.5.26`, SHA
`2b4b138dc5cf2d9cc1a0ceb21d9063ff56fe8bf4`. The baseline worktree remained
clean after dependency installation, gates and packaging. This build was separate
from the fork; its digests are comparison evidence, not fork candidate digests.

The six-package target-specific release build and `bundle-sidecars.sh` succeeded.
The original `just desktop-release-build aarch64-apple-darwin` failed at Finder
AppleScript automation. A verbose packaging diagnostic confirmed AppleEvent
`-1712` timeout while setting Finder's icon view. Upstream-supported `CI=true`
adds `--skip-jenkins`, skipping GUI DMG decoration; the full release recipe then
exited 0 and produced both `.app` and `.dmg`. No upstream source was repaired.
All six bundled sidecars and the app executable were nonempty executable arm64
Mach-O binaries. App version `0.5.26` and identifier `xyz.block.buzz.app` were
verified from the actual app plist. No baseline app was launched.

Original unit evidence had two environment failures in each ACP library run
(962 passed, 2 failed, 3 ignored):
`session_new_forwards_complete_git_block_without_duplicate_names` selected the
inherited Git keyfile rather than its new temporary keyfile, and
`test_session_policy_default_is_channel` inherited the managed agent's session
policy. Individually clearing their relevant settings made each pass; removing
only `GIT_CONFIG_*` and `BUZZ_ACP_SESSION_POLICY` made the full library pass
(964 passed, 3 ignored). Full ACP integration then exposed the host's Apple Git
2.39.5, below the documented 2.46 credential capability requirement. An official
Git 2.49.0 source build in a task-local toolchain resolved this: full ACP package
exited 0, including all four native Git bootstrap tests. The commit wrapper and
global author configuration were preserved. Git source archive SHA256:
`618190cf590b7e9f6c11f91f23b1d267cd98c3ab33b850416d8758f8b5a85628`.

Both updater environment variables were absent for the baseline build. Actual
app Resources contained only `icon.icns`; the actual release build output omitted
`cargo:rustc-cfg=buzz_updater_enabled`. Source endpoints were empty. These facts
are separate from the later fork artifact's exact runtime bundled-config probe;
upstream has no runtime config-dump command, so no such probe is claimed here.

Artifact digests (under the baseline target's `release/bundle/`):

| Artifact | Bytes | SHA256 |
| --- | ---: | --- |
| `Buzz_0.5.26_aarch64.dmg` | 107527608 | `3d31090c5681073483cdd5ba0c92d65bfc7a8e34502976eda83a694d3147843d` |
| `Buzz_0.5.26_aarch64.app.tar.gz` | 106865130 | `2876155fdf64ecd8e5a3466af62281e77ba17b3f3d3e1173e94f5a0bdd5f4828` |

Detailed local command logs and the seven-binary checksum manifest are in
`.superpowers/sdd/2026-10-03-buzz-fork-maintenance/M1-baseline-*.log`,
`M1-baseline-artifacts.json` and `M1-baseline-SHA256SUMS`. This tracked result
preserves source revision, diagnosis and artifact identity when local logs are
unavailable. Corrected baseline `just ci desktop-typecheck` exited 0. It covered repository
fmt/clippy, Desktop fmt/clippy/check/typecheck and builds, workspace unit lanes,
web/mobile checks/builds/tests, security review and file-size gates. Desktop
frontend: 6,747 main tests plus 93 secret-sanitizer tests passed; Tauri workspace:
3,486 tests passed, 20 ignored; mobile: 2,559 passed, 4 skipped, plus 3 tests for
the build without a push gateway. Full native ACP package also exited 0. These
counts are tied to the exact upstream SHA above; they do not attest fork product
changes or the later two-device owner acceptance.

## Guarded release preparation

`scripts/fork/sync.py` reads stable `desktop-vX.Y.Z` refs from
`https://github.com/block/buzz.git`, selects by numeric semver and reports skipped
intermediate releases. `prepare` writes a structured `no_update` report and emits
no stdout or PR when the current base is already newest. It does not create real
accepted refs or configure remotes on the maintainer's behalf. A selected tag with
no accepted `fork/main` is blocked. For a real update:

```sh
python3 scripts/fork/sync.py select --report selection.json
python3 scripts/fork/sync.py baseline --target-sha <selected-exact-sha> --report baseline.json
python3 scripts/fork/sync.py prepare --repo . --base fork/main \
  --manifest scripts/fork/patches.json --baseline-report baseline.json --report preparation.json
```

The baseline uses an independent detached upstream worktree. It executes the full
Task12 gates, `just ci`, and the six real arm64 sidecar/app release recipes, then
checks executable/nonzero Mach-O arm64 binaries and hashes the app binaries and
DMG. Its JSON includes `upstream_sha`, `gate_commands`, `result` and a separate
`artifact_manifest`; logs accompany each gate. Failure records evidence and stops
before candidate branches, merges, tests, builds or candidate uploads. Baseline
and candidate evidence are never interchangeable. An existing baseline worktree
requires an explicit maintenance handoff; the tool does not delete it. Baseline
environment removes managed Git/session policy and fork identity overrides; it
uses upstream's `CI=true` non-GUI packaging. Git 2.46+ remains a prerequisite.

After baseline success for that exact SHA, preparation fast-forwards the local
`upstream` mirror and merges the target into an owned `fork/sync-vX.Y.Z` worktree.
It never rebases or moves accepted `fork/main`. The candidate registry compares
against the fetched target SHA, so upstream-only changes are excluded; the report
retains the prior registry base. Protected paths and exact calls are validated
before and after merging. The upstream side of an unregistered conflict is used
only after the accepted registry passed. Registered conflicts, missing seams and
staged conflict markers block the candidate; no buildable SHA is advertised.

Conflicts stay in the separate preparation worktree. `prepare_blocked_report`
creates a clean `fork/blocked-vX.Y.Z` commit based on accepted `fork/main` changing
only `docs/fork/sync-reports/desktop-vX.Y.Z.json`. It records base/target SHAs and
relative conflicts without private workspace paths. Local OS locking and Actions
concurrency (`cancel-in-progress: false`) serialize runs. Owned interrupted work
resumes; moved bases, changed targets and foreign branches/worktrees stop with
ownership evidence, without deletion or reset. A blocked report interrupted
between branch creation and report commit requires owner handoff. Retiring a
blocked branch after a clean sync PR is a reviewed owner action.

Publication is separate and requires explicit `--publish`. It pushes only the
clean candidate/report branch without force, opens one draft PR in
`https://github.com/reynholm/buzz`, and reuses that draft. On ephemeral runners,
reuse verifies published merge parent SHAs, durable preparation trailers and the
entire accepted registry; report reuse verifies a single report-only compare diff.
Closed/promoted PRs or ownership mismatches require handoff. No promotion, merge,
final fork tag or release is performed. Authorized workflow runs link the eventual
verified artifact run URL to the existing draft; failures leave the PR draft.

The JSON-form YAML [fork-sync workflow](../../.github/workflows/fork-sync.yml)
separates selection, clean baseline, candidate and diagnostic reporting. The
candidate job requires baseline success and explicitly runs the full fork Python
suite before full Task12 gates and M3's real build/verification. Diagnostic report
uploads on failure never count as candidate success. macOS jobs use `macos-15`,
documented as arm64 by [GitHub's hosted-runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
(checked 2026-10-04), and assert `uname -m` is `arm64`. Actual hosted execution is
still an external gate. M3 must provide `build-candidate.sh --candidate-sha SHA
--baseline-report PATH`, `verify_artifact.py`, and verified files in
`artifacts/fork/`; absent tooling fails closed, never substitutes placeholders.
The source workflow is pending owner/default-branch authorization. The owner must
configure verified `FORK_GIT_AUTHOR_NAME`/`FORK_GIT_AUTHOR_EMAIL` repository variables
for the repository's author policy before activation. Schedule `0 6 * * *` runs
only once `fork/main` is confirmed as the default branch and the owner enables it.

Synthetic disposable Git/GH fixtures prove automation behavior, including actual
clean merges/conflicts, one draft PR, no force-push and failing baseline ordering.
They do not satisfy next-real-tag, hosted-runner, native app, owner installation or
two-physical-device acceptance. The real read-only check on 2026-10-04 found no
stable tag newer than `desktop-v0.5.26`; local `prepare` returned `no_update`, with
empty stdout and no publication or accepted-ref/mirror changes. Repeat with the
explicit full Python suite and registry validation before any real update.

## Immutable Apple Silicon candidates

After the exact clean upstream baseline succeeds and the candidate source is
committed and clean, build locally with the pinned Hermit tools:

```sh
. ./bin/activate-hermit
scripts/fork/build-candidate.sh --candidate-sha <full-40-character-SHA> --baseline-report <baseline.json>
```

The producer accepts M2's successful baseline JSON only when `upstream_sha`
matches the registry's exact `base_sha`, gate commands are present, and every
real arm64 app/sidecar plus DMG has a nonzero size and SHA256 receipt. An earlier
baseline for another selected tag fails closed. `BUZZ_FORK_REVISION` defaults to
`1`; maintainers can explicitly supply the next positive fork number. The Actions
source passes owner-configured `FORK_REVISION` with the same default. This number
labels candidate builds and does not create a release tag.

The script checks the exact clean source both before and after building, clears
inherited updater/demo/capability/provider/relay build inputs, disables Cargo
incremental compilation, builds all six real arm64 sidecars, verifies them before
packaging, and runs upstream's release recipe with `CI=true`. Version `0.5.26`
and identifier `xyz.block.buzz.app` remain upstream values. Partial fork identity
variables, malformed SHAs and invalid fork numbers fail during compilation.

`--fork-artifact-probe` on the actual built executable returns the same generated
Tauri context used at app startup, the compile-time identity, updater compile
flag and demo namespace. It exits before runtime/GUI/data/keychain startup.
Verification compares actual plist and embedded config with the clean upstream
config, rejects configured updater endpoints/keys/activation in generated config
or JSON/plist resources, rejects demo production candidates, and checks executable
thin arm64 Mach-O binaries with their sizes, modes and hashes. About requests the
native immutable identity and shows its fork number and full SHA beside the
unchanged upstream version. Upstream builds return no fork identity.

Verified output is kept in `artifacts/fork/<candidate-sha>/` with an app archive,
DMG, manifest, baseline receipt, build log and `SHA256SUMS`. Existing output is
preserved rather than overwritten; failed build logs remain diagnostic evidence.
Run `shasum -a 256 -c SHA256SUMS` in that directory before sharing. A later source
commit requires rebuilding/reverification for the new SHA; never relabel earlier
assets. These unsigned candidate assets make no Developer ID/notarization promise.

The full Python gate remains mandatory in every candidate workflow. Native probe,
synthetic IPC/About tests and local packaging prove different boundaries. Native
GUI/data migration/keychain prompt, owner installation, two physical devices,
hosted workflow, default-branch schedule and next real upstream tag acceptance
remain explicit external gates until their corresponding evidence exists. Native
GUI exercises must use the existing named-demo build recipe and isolated fixtures;
never launch the production candidate against the owner's data or keychain.

## Owner merge and exact-tree promotion

`promote.py` performs a read-only preflight by default. It consumes M3's existing
`manifest.json`, `baseline.json`, archive, DMG and `SHA256SUMS` without rewriting
any of them. A separate owner acceptance receipt pins the candidate SHA/tree,
merged SHA, upstream baseline tag/SHA and SHA256 digests of the manifest and
checksum file. All acceptance references share that immutable receipt scope.
A checkbox or synthetic test fixture never substitutes for actual owner review,
native acceptance or two physical devices.

The owner first reviews the draft's baseline/candidate SHAs, skipped tags,
upstream range/changelog, persona/agent/device/sync diff, conflict/seam report,
full tests and candidate artifact run. Follow
[device acceptance](DEVICE_AGENT_ACCEPTANCE.md) for the physical checks.
Record independently accessible evidence for every item below, using the actual
candidate artifacts. Keep the PR draft until the repository's review and human
acceptance requirements hold; do not add `buzz-review-completed` early. Owner
`reynholm` merges into `fork/main`; automation never merges a PR.

Create `owner-acceptance.json` outside the producer output directory. This
incomplete schema example is deliberately rejected until actual owner evidence
and exact hashes are supplied:

```json
{
  "synthetic": false,
  "owner_login": "reynholm",
  "pr_url": "https://github.com/reynholm/buzz/pull/<number>",
  "candidate_sha": "<full tested candidate SHA>",
  "candidate_tree": "<git rev-parse candidate^{tree}>",
  "merged_sha": "<full owner-merged SHA>",
  "base_tag": "desktop-v0.5.26",
  "upstream_sha": "2b4b138dc5cf2d9cc1a0ceb21d9063ff56fe8bf4",
  "manifest_sha256": "<SHA256 of unchanged M3 manifest.json>",
  "checksums_sha256": "<SHA256 of unchanged M3 SHA256SUMS>",
  "evidence": {
    "owner_review": {"accepted": false, "reference": ""},
    "full_tests": {"accepted": false, "reference": ""},
    "native_about": {"accepted": false, "reference": ""},
    "existing_data": {"accepted": false, "reference": ""},
    "sidecar_runtime": {"accepted": false, "reference": ""},
    "keychain_prompt": {"accepted": false, "reference": ""},
    "two_physical_devices": {"accepted": false, "reference": ""},
    "maintenance_command": {"accepted": false, "reference": ""}
  }
}
```

An owner records `accepted: true` only after that check passed for the pinned
candidate. `full_tests` references the complete candidate Python/companion/CI
receipts and successful clean upstream baseline; `owner_review` references the
owner's completed review. The remaining records reference actual native About
identity, isolated existing-data migration, full sidecar runtime, keychain prompt,
two physical devices and the owner's maintenance command exercise. Evidence
references are reviewed human attestations; the script does not independently
perform physical tests or authenticate a hand-written receipt. Publication checks
GitHub's authoritative owner merge actor and exact head/base/merged SHAs again.

Fetch the owner's accepted merge and base tag into the maintenance checkout
without changing its checked-out source, ensure local `fork/main` contains the
merge, then run:

```sh
python3 scripts/fork/promote.py --candidate <tested-sha> --merged <owner-merged-sha> \
  --manifest <candidate-directory>/manifest.json --fork-revision <tested-N> \
  --acceptance owner-acceptance.json
```

Preflight requires candidate and merged Git trees to match exactly. The embedded
artifact identity remains the tested candidate SHA, even if the owner's merge
commit has a different SHA. It checks upstream tag/config/baseline provenance,
all four producer checksum entries, DMG size/hash and archived plist, actual
binary/resource bytes, modes and arm64 Mach-O headers; it rejects updater config
and archive links/traversal. BSD tar's regular AppleDouble metadata is accepted
only with safe normalized paths, an existing associated app entry, bounded size
and valid header/table/entry bounds. Metadata can inherit executable modes; it
never substitutes for a real binary/resource record. M3's native probe remains the source of embedded
identity/config evidence, cryptographically pinned by the unchanged owner-reviewed
manifest; promotion does not start an app. If any merge changes the tree, rebuild,
retest and reaccept a new exact-SHA candidate. Never relabel earlier assets.

Only after owner authorization, append `--publish`. It requires the existing
`reynholm/buzz` origin, remote `fork/main` at the exact merged SHA and GitHub's
PR record attesting the owner merge of this candidate branch. It copies and
reverifies a private asset snapshot before external calls, so producer-directory
changes cannot change published bytes. It creates `fork-vX.Y.Z-N` on the merged
commit with an ordinary nonforce tag push. A same-name tag on any other commit,
even with the same tree, is an error. Existing release provenance and asset bytes
must match; missing assets of an identical draft resume without overwriting.
The release initially stays draft, includes exact candidate/merged/tree/checksum
provenance and the unchanged candidate assets, and becomes public only after all
downloaded asset hashes verify.

After successful final tag/release verification, cleanup targets only the exact
merged `fork/sync-vX.Y.Z` remote branch. A deletion-only compare-and-swap lease
pins its current SHA to the candidate; it never force-updates a branch or rewrites
history. A concurrent replacement refuses deletion and preserves the branch for
owner handoff. No other branch or local worktree is removed. Repeating the same
accepted promotion verifies the same tag/release/bytes and safely resumes; wrong
release provenance, altered uploaded bytes or moved accepted refs require owner
handoff. Command/API errors propagate; a partial tag/draft remains inspectable.

The separate JSON-form YAML [owner promotion workflow](../../.github/workflows/fork-promote.yml)
is manual only. It requires owner dispatch plus environment `fork-promotion`, runs
the full Python suite, downloads the exact supplied candidate artifact/run IDs,
retains the owner's acceptance JSON and invokes the same explicit publication CLI.
Before enabling it, the owner must configure that environment with owner-only
required approval and branch restrictions. Source configuration does not prove
hosted execution or environment protection exists. It shares sync concurrency,
never auto-merges, never installs, and never changes the default branch.

Disposable bare-Git/executable-GH fixtures use `synthetic: true` only for tooling
correctness. `--fixture --publish` accepts only an absolute local bare origin;
synthetic receipts cannot publish to a network remote. Tests cover exact-tree
rejection, stale/missing acceptance, corrupt assets, GitHub merge actor, tag
conflicts, byte-preserving publication/resume and concurrent cleanup refusal.
They do not provide owner or physical acceptance.

M3's locally built `fcbe5c649594e323b94224ab0634a81e884a6251` artifacts remain
exactly that earlier scoped candidate. M4 and subsequent implementation commits
require a later exact-SHA rebuild/reverification before delivery as the final
candidate. Current owner instructions authorize local commits only: no real tag,
release, push, merge, promotion, installation or workflow activation has occurred.
Owner review/native/physical acceptance and the next actual upstream tag remain
pending external gates.


## M5 local handoff and open acceptance: 2026-10-04

The newest owner instruction ends implementation with local commits and an
unpushed branch on `fork/device-bound-agents`. M5's local evidence preparation
is complete only after registry falsification, restored validation, full Python,
size and documentation checks. Overall feature acceptance stays open. Owner
review is a separate next action; no new independent review, hosted activation,
publication, merge, tag, installation or completion reaction is authorized here.

The read-only upstream tag query on 2026-10-04 at 08:54 UTC found no stable desktop
tag after `desktop-v0.5.26`. Criterion 17 is **next-real-tag unavailable**. This
is neither an upgrade success nor a conflict/baseline failure. When a real tag
appears and hosted execution is authorized, selection must record its exact SHA,
skipped tags and upstream range; the recurring clean-target baseline must pass
before any candidate job. A repeated target must reuse its owned draft/result,
and scheduled/manual work must serialize. Source workflow tests prove these
contracts locally; they do not prove a hosted job or schedule ran.

The complete registry at Task12 HEAD contained 199 paths, 78 new modules and
211 protected invocation entries. In a disposable detached clone, M5 removed
each new module and each exact invocation independently and invoked the actual
`sync.py validate` CLI. All 289 mutations produced the matching missing-path or
missing-invocation rejection; restored validation passed. Original/mutant/restored
hashes, exact commands, exits and all raw-log hashes are retained in
`.superpowers/sdd/2026-10-03-device-bound-agents/M5-registry-mutations.json`.
The source checkout was never mutated. Deletion of the validator or its registry
uses the original external validator and manifest to measure missing-path rejection.
These are **validator failures**, not compile failures or runtime behavior
assertions. Prior owning runtime mutation receipts, including recorded survivors
and their later targeted fixes, keep their original commits and hashes.

Task12's actual full `just ci` exit0 was at
`6f2ef4a026bc90a9ee1456a31f882c3d87baf997`, tree
`b182ee71e9194dfb4967e529fda5c7346f6bd336`. Its supplemental full Python59/0,
lint/build/typecheck/registry/size receipts passed at that same commit. The mesh
3800/0 receipt retains its original precommit HEAD and identical tested source
closure; it is not a later-SHA rerun. M5 documentation/inventory changes carry
these results only through verified unchanged product/test/build-source bytes.
Run new relevant gates when source or environment changes warrant them. On this
host, native Git tests require the official task-local Git2.50.1 full runtime
(templates and exec-path), not Apple Git2.39.5 or a binary-only PATH override.
Keep the test toolchain local; commits still use the trusted identity wrapper.

Task12's actual named-demo arm64 build and bounded launch prove only isolated
packaging/config/probe/process startup. Host-proof initialization failed with
`keyring write: Platform secure storage failure: UNIX[Operation not permitted]`.
Native Wry About/agent workflows, real local-relay integration, successful keychain
storage/prompt/recovery, existing-data migration and full sidecar runtime remain
unverified. Two temporary stores or two demo instances cannot pass two physical
notebook acceptance. Use [the owner procedure](DEVICE_AGENT_ACCEPTANCE.md).

After the last local source/document commit, build and verify the exact final
candidate SHA. Retain that SHA/tree, immutable artifact directory, manifest,
archive/DMG and SHA256SUMS hashes in ignored task evidence and the timestamped
workspace WORK_LOG; do not add another source commit solely to record its own
artifact identity. If the build fails, record exact-final artifact pending.
Existing fcbe assets and the independent upstream baseline retain their identities.
A successful final build still leaves owner review, all native/physical acceptance,
maintenance-command acceptance, next real tag, confirmed default branch/hosted
schedule, configured promotion environment and owner merge/promotion open.

## Owner-directed main integration: 2026-10-04

Buzz request `da40df459b2ecc64a36321e65ec6e6741395e121692a943dc86f1a14a5d460f7`
authorizes translation, commit/push, integration into `main`, and publication of a
new GitHub release exclusively in `reynholm/buzz`. This explicit request supersedes
the earlier `fork/main` destination and owner-only manual publication workflow
for this release. It does not attest installed-app or two-device acceptance.

Existing fork `main` at `33f54de2dd27a8f6bce0d359183f5ebe5f2fa9ba` contains 47
commits outside the device-bound branch. Integrate both histories by an ordinary
merge, preserving community-removal admission and device ownership/fencing.
The patch inventory includes inherited changes from that existing main snapshot;
these entries identify integration history rather than a new upstream fetch.
Explicit upstream deletions use `deleted: true`; the validator requires the path
to exist in the base, have no required symbols or invocation seams, and be absent
from the result. Missing custom modules and guards still fail closed.

The release uses tag `fork-v0.5.26-2` and embedded fork revision `2`, preserving
base app version `0.5.26` and identifier `xyz.block.buzz.app`. Build only after the
merged tree passes its full gates and the remote main SHA is verified. Publish
pinned assets and checksums to the fork; never run upstream release publication.
