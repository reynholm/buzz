# Buzz fork maintenance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce verified Apple Silicon fork artifacts and reviewable release-tag updates without silently losing device-agent patches.

**Architecture:** Fork-specific tooling tracks explicit upstream seams, serializes update preparation and publishes candidate evidence in draft PRs. Build verification binds all assets to the candidate SHA; final tagging follows owner merge and tree verification.

**Tech Stack:** Existing Git/GitHub Actions, Python standard library, Hermit, cargo, pnpm, Tauri and macOS build tools.

**Spec:** [Approved design §8–9](../specs/2026-10-03-device-bound-agents-design.md); [device-agent plan](2026-10-03-device-bound-agents.md).

## Global Constraints

- Reuse `reynholm/buzz`; never create a duplicate repository/project. Existing worktree/identity/commit guards are specified in the companion plan and apply here.
- Base `desktop-v0.5.26`, `2b4b138dc5cf2d9cc1a0ceb21d9063ff56fe8bf4`; preserve upstream app version and bundle identifier. Extend existing `desktop/src-tauri/src/build_identity.rs`, preserving demo isolation.
- `upstream` FF mirror, `fork/main` accepted release+patches, `fork/sync-vX.Y.Z` temporary merge branch without rebase, final tag `fork-vX.Y.Z-N`.
- Target only `aarch64-apple-darwin`. Real binaries: `buzz-acp`, `buzz-agent`, `buzz-backend-kubernetes`, `buzz-dev-mcp`, `git-credential-nostr`, `buzz` (package buzz-cli).
- Updater endpoints remain empty in actual bundled config and resources; clear inherited BUZZ_UPDATER_ENDPOINT and BUZZ_UPDATER_PUBLIC_KEY during fork build. No Developer ID/notarization promise.
- Owner merges and installs. Before that, artifacts/releases are candidates/drafts; never create a final fork tag early. Lost seam/module, patch conflict or failing clean baseline stops with report.
- Scheduled workflow lives on GitHub's default branch. Owner must confirm `fork/main` as default before scheduled operation is claimed live; manual workflow can be tested first. No unattended force-push/default-branch change.
- No new upstream tags means no chat notification. Always report skipped tags and candidate SHA when there is a new update. Actual next-tag criterion cannot be passed using a synthetic tag.

## Review Focus

1. A textual merge succeeds after upstream removes a protected invocation: stop on missing seam rather than ship an inert patch (Task M2).
2. Two scheduled/manual invocations target one version: one branch/PR/candidate, without deleting another run's work (Task M2).
3. Release recipe supplies executable placeholders or inherited updater settings: reject the bundle (Tasks M1, M3).
4. Owner merge yields a different tree from tested candidate: do not promote or tag (Task M4).
5. A future upstream tag fails without fork patches: report an upstream blocker before attempting patch repair (Tasks M1, M5).

---

## File responsibilities

- `FORK_PATCHES.md` and machine-readable `scripts/fork/patches.json`: base tag/SHA and every touched upstream path, module, invocation seam and falsifiable verification. JSON is tooling input, Markdown is the human registry; test their consistency.
- New `scripts/fork/sync.py`: fetch/tag selection, guarded merge preparation, patch validation, status/report and candidate branch lifecycle. New `.github/workflows/fork-sync.yml`: daily/manual invocation and build gates, not conflict guessing.
- New `scripts/fork/build-candidate.sh`, `verify_artifact.py`: real-sidecar build and actual app-bundle checks; delegate existing bundle/release recipes.
- Existing `build_identity.rs` and `build.rs`, SettingsView.tsx: candidate identity display without modifying product version.
- New `scripts/fork/promote.py`: verify owner merge/tree and then final tag/release. `docs/fork/MAINTENANCE.md`: operations/acceptance and external gates.
- Python tooling tests use `python3 -m unittest discover -s scripts/fork/tests -v`; shell build contracts are exercised through these tests and a real macOS build, not mocked-only verification.

### Task M1: Clean baseline build and patch registry

**Files:** Create `FORK_PATCHES.md`, `scripts/fork/sync.py`, `scripts/fork/patches.json`, `scripts/fork/tests/test_patch_registry.py`, `docs/fork/MAINTENANCE.md`.

**Interfaces:** `validate_patches(repo: Path, manifest: dict) -> list[str]` returns missing/unlisted path or seam errors; empty means valid. JSON `{ base_tag, base_sha, patches: [{ path, responsibility, is_new, required_symbols, invocation_seams, verification_commands }] }`. Registry contains upstream paths modified by feature commits and lists new modules with is_new=true. An invocation seam includes caller path, callee symbol and the production behavior test that fails when removed.

- [ ] Write `test_registry_rejects_unlisted_modified_upstream_path`, `test_registry_rejects_missing_seam`, `test_registry_markdown_matches_json`. Assert every modified upstream file has a registry entry and every named seam exists; new modules cannot disappear unnoticed.

```python
self.assertEqual(validate_patches(repo, manifest), [])
self.assertIn("missing invocation", "\n".join(errors_after_removed_call))
```

- [ ] Run full Python suite; expect failed registry assertions before implementing the validator. Before product changes, build clean tag in a separate disposable worktree with real sidecars and required original-spec checks. Record exact SHA and baseline artifact/workflow behavior; keep production data/keychain untouched. If it fails, record baseline blocker, do not silently repair upstream in patch work.
- [ ] Implement validate_patches with tracked diff against base_sha, exact symbol/invocation checks and Markdown/JSON consistency. Populate registry after each feature task; compare final branch against base, including test/build/document paths as applicable. Record upstream-winning policy outside registry and owner decision required before retiring patches.
- [ ] Run full Python suite after implementing M1 validator; expect pass. Maintain baseline results separately from candidate results, including failures; no placeholder build counts as baseline success.
- [ ] Commit `docs(fork): track upstream patch seams and baseline evidence` with explicit registry/doc/test paths.

### Task M2: Idempotent release-tag preparation and reporting

**Files:** Modify `scripts/fork/sync.py`; create `scripts/fork/tests/test_sync.py`, `scripts/fork/tests/test_workflow_contract.py`, `.github/workflows/fork-sync.yml`; update registry/maintenance guide.

**Interfaces:** Python `select_update(tags: list[str], current_tag: str) -> UpdateSelection | None`, where UpdateSelection contains target_tag/skipped_tags; consume M1 `validate_patches`; `prepare_update(repo: Path, selection: UpdateSelection, manifest: dict) -> SyncReport`; CLI `python3 scripts/fork/sync.py prepare --repo <path> --base fork/main --manifest scripts/fork/patches.json --report <path>`. SyncReport includes state no_update/ready/blocked, base/target/candidate SHA, branch, skipped tags, conflicts and missing seams. Add separate clean-target baseline evidence `{ upstream_sha, gate_commands, result, artifact_manifest }`; candidate cannot begin without its success for this selected target. `prepare_blocked_report(repo: Path, selection: UpdateSelection, report: SyncReport) -> str` creates/reuses a clean report-only commit/branch based on fork/main, returns its SHA and never pushes an unmerged index.

- [ ] Write `test_semver_selection_ignores_nonrelease_tags`, `test_no_update_is_silent`, `test_same_version_reuses_branch_and_pr`, `test_merge_success_missing_seam_blocks`, `test_conflict_reports_without_overwriting_patch`, `test_every_candidate_workflow_runs_fork_python_suite`, `test_clean_target_failure_prevents_candidate`, `test_unmerged_index_produces_report_only_draft`. Write fork-sync.yml in JSON-form YAML so Python standard-library json can inspect the actual jobs/step graph without a new parser dependency. Workflow-contract tests parse that actual workflow: removal of the exact full Python-suite step or baseline needs-condition fails; injected failing clean target yields zero candidate tests/build/uploads even if a hypothetical fork candidate is green. Conflict fixture produces a clean report-only commit against base, publishable as a blocked draft, with no conflict-marked product changes. Temporary git fixtures must prove actual clean merge and conflict behavior; injected GH API verifies one draft PR, no force-push. An interrupted branch preparation resumes or reports ownership conflict; it never deletes foreign work.

```python
self.assertIsNone(select_update(tags_without_new_release, "desktop-v0.5.26"))
self.assertEqual(second_report.branch, first_report.branch)
self.assertEqual(gh_draft_pr_create_count, 1)
self.assertEqual(candidate_build_count_after_baseline_failure, 0)
self.assertIn("python3 -m unittest discover -s scripts/fork/tests -v", candidate_workflow_run_commands)
```

- [ ] Run full Python suite; expect new preparation/validation assertions to fail.
- [ ] Use subprocess argv arrays, local process lock and Actions concurrency group with cancel-in-progress false. Fetch upstream tags; semver order desktop-v*; FF update upstream mirror; merge target tag into fork branch without rebase. For the candidate registry, update base_tag/base_sha to the actual fetched target tag before comparing candidate changes; retain prior-base evidence in the report. Validate all registered paths/seams before proceeding, without treating upstream-only changes as fork patches. Every scheduled/manual selected-target run first checks out the exact clean upstream target SHA in a separate worktree/job, executes companion Task12 required baseline gates and real upstream macOS sidecar/app build, and records independent upstream_sha/results/manifest. Candidate jobs depend on success of that baseline job; do not use always() or continue-on-error to admit candidate work after failure. Report upload can run on failure, but is never a passing candidate. Every candidate workflow then explicitly runs `python3 -m unittest discover -s scripts/fork/tests -v` (just ci does not include it), companion Task12 full gates including just ci, and M3 real build verification; Python regression or other gate failure leaves PR draft and prevents promotion. Workflow-contract test asserts actual ordering/needs/conditions, not only a matching comment. Unlisted conflicts choose upstream only after registry proves they are outside patches; registered conflicts are reported for agent repair/review, never auto-guessed. For unresolved merge, keep the conflicted index in a separate local preparation worktree; produce a clean `fork/blocked-vX.Y.Z` report branch from accepted fork/main with a report-only commit containing target/base SHAs, conflicting paths and blocked status (no keys/raw private workspace data). Push that clean branch and open/reuse a blocked draft PR so it has an actual compare diff. Candidate sync branch/artifact is not presented as buildable. Resume reuses the report/PR and recorded target, verifies fork/main has not moved or restarts safely, then resolves in a fresh preparation worktree; no force-push, no `git add -A` of conflict markers. Link the eventual clean sync PR and retire report-only branch only after reviewed handoff. Draft PR/report links include real repository URLs and artifact once available. Daily cron `0 6 * * *` and workflow_dispatch; scheduled activation is a documented owner/default-branch gate. GH workflow permission only contents/pull-requests needed for authorized candidate publication, no merge action.
- [ ] Run full Python suite; expect pass. Exercise manual prepare against real upstream with no newer tag: structured no_update and no chat message/PR. Use fixture tags only for automation correctness; label them synthetic.
- [ ] Commit `feat(fork): prepare guarded upstream release updates`.

### Task M3: Real Apple Silicon candidate, checksum and About identity

**Files:** Create `scripts/fork/build-candidate.sh`, `verify_artifact.py`, `tests/test_artifact.py`, `desktop/src/shared/api/tauriBuildIdentity.ts`, `tauriBuildIdentity.test.mjs`; modify `desktop/src-tauri/{build.rs,src/build_identity.rs,src/lib.rs}`, `desktop/src/features/settings/ui/SettingsView.tsx`; update workflow/registry.

**Interfaces:** Rust `ForkBuildIdentity { fork_revision: String, commit_sha: String, base_tag: String }`; command `get_fork_build_identity() -> Option<ForkBuildIdentity>` reads compile-time fork variables and is absent for upstream builds. Env names `BUZZ_FORK_REVISION`, `BUZZ_FORK_SHA`, `BUZZ_FORK_BASE_TAG`. Python `verify_artifact(app: Path, candidate_sha: str, baseline_config: dict) -> ArtifactManifest` includes arch, each sidecar size/hash/mode, embedded config, app version/id and identity. CLI takes explicit candidate SHA; never infer a changing HEAD after build.

- [ ] Write `test_placeholder_or_wrong_arch_is_rejected`, `test_bundled_updater_is_rejected`, `test_identity_matches_candidate`, plus Rust existing build_identity tests preserving demo namespace and `desktop/src/shared/api/tauriBuildIdentity.test.mjs` testing About identity mapping; test actual About rendering in the native build workflow. Assert zero-byte/non-executable/x86 sidecars fail, actual updater config/endpoints fail, mismatched SHA fails, upstream version/id unchanged. About shows fork number and SHA without replacing v0.5.26.

```python
with self.assertRaises(ValueError):
    verify_artifact(app_with_zero_byte_sidecar, candidate_sha, baseline_config)
self.assertEqual(manifest.commit_sha, candidate_sha)
```

- [ ] Run Python full suite, `just desktop-tauri-test desktop-test`; expect new identity/artifact assertions to fail.
- [ ] Build at captured clean candidate SHA: `cargo build --release --target aarch64-apple-darwin -p buzz-acp -p buzz-agent -p buzz-backend-kubernetes -p buzz-dev-mcp -p git-credential-nostr -p buzz-cli`; `scripts/bundle-sidecars.sh aarch64-apple-darwin`; `just desktop-release-build aarch64-apple-darwin`, with updater env unset and fork identity env pinned. Verify before and after Tauri bundle; inspect Mach-O architecture, executable bits, nonzero sizes, embedded resources/config/identity, updater feature absence and checksum. Generate DMG plus SHA256SUMS/manifest and draft candidate assets. Do not change tauri version/bundle id or replace existing build_identity logic.
- [ ] Run full `python3 -m unittest discover -s scripts/fork/tests -v`, both Desktop suites and type gates, then real macOS build; every candidate workflow contains that same full Python gate. Record candidate SHA in same shell as every claim; verify final worktree unchanged. Launch built app against isolated data; test existing 0.5.26 data copy, sidecar launch, About SHA and keychain prompt. No claim of user's existing-data acceptance until owner test.
- [ ] Commit `feat(fork): verify and identify Apple Silicon candidates`; rebuild after commit because artifact identity must match final candidate SHA.

### Task M4: Owner merge and exact-tree promotion

**Files:** Create `scripts/fork/promote.py`, `tests/test_promote.py`; modify maintenance guide and workflow.

**Interfaces:** `verify_promotion(repo: Path, candidate_sha: str, merged_sha: str, artifact_manifest: dict) -> Promotion`; require candidate tree == merged tree, artifact identity/checksum matches candidate, explicit owner-merged PR, all required acceptance evidence. CLI `python3 scripts/fork/promote.py --candidate <sha> --merged <sha> --manifest <path> --fork-revision <N>`; no auto merge.

- [ ] Write `test_premerge_candidate_cannot_tag`, `test_changed_merge_tree_cannot_promote`, `test_existing_tag_is_idempotent_only_for_same_commit`, `test_postmerge_promotion_keeps_tested_tree`. Assert differing tree or missing acceptance rejects before tag/release/branch delete; existing conflicting tag is error; same accepted state safely resumes.

```python
with self.assertRaises(ValueError):
    verify_promotion(repo, candidate_sha, changed_merge_sha, artifact_manifest)
self.assertEqual(created_final_tags, [])
```

- [ ] Run full Python suite; expect promotion assertions to fail.
- [ ] Draft PR includes baseline/candidate SHAs, upstream tag range/changelog, persona/agent/sync changes, conflicts/seam checks, full tests, manifest and acceptance instructions. After explicit owner merge, compare trees and preserve artifact-to-candidate provenance; tag merged commit only when its tree matches. Publish fork-vX.Y.Z-N release, remove only the merged sync branch after successful tag/release verification. If merge changes code, rebuild/retest/reaccept instead of promoting previous assets.
- [ ] Run full Python suite; expect pass. Exercise CLI end-to-end in local git/GH fixtures; perform real remote promotion only after owner merge/acceptance, reporting returned links.
- [ ] Commit `feat(fork): promote only reviewed and verified fork trees`.

### Task M5: Actual upgrade and final feature acceptance

**Files:** Update `docs/fork/{MAINTENANCE.md,DEVICE_AGENT_ACCEPTANCE.md}`, `FORK_PATCHES.md`; record timestamped workspace WORK_LOGS evidence.

**Interfaces:** Evidence ties real target tag and upstream SHA to sync branch, draft PR, build manifest/artifact, test outcomes and owner decision. Reports distinguish success, conflict stop, upstream baseline failure, human acceptance pending and next-tag unavailable.

- [ ] When the next actual desktop tag exists, run the real manual sync workflow whose clean-target job runs baseline gates/build before candidate. This stage also runs for every later cron/manual target, not only this first upgrade acceptance. Expected: draft PR and verified candidate, or documented conflict/baseline/seam blocker. Repeat request for same tag must reuse the same result; daily job must serialize with manual run.
- [ ] Deliberately remove every registered protected invocation/new module in disposable test trees; expect patch validator/test gate to fail. Restore patches; full Python/desktop/CI gates pass at candidate SHA. No synthetic upgrade is counted as the original criterion17.
- [ ] Owner runs both-notebook and existing-data checks from companion plan, tests the maintenance command, confirms schedule/default-branch setup, then chooses merge. Record explicit acceptance before review-completed/ready status, tag or final completion.
- [ ] Publish one self-contained feature result with PR/artifact/checksum/evidence links and any remaining blocker, mentioning Andrey B. Only when both plans and all agreed acceptance gates complete, verify successful ✅ reaction on thread root `90b9a649c5271252866d159bdaa4e40a5ed1ba859e16bdfb7c5c68f3e74da5e0` before announcing completion.
- [ ] Commit acceptance evidence using guarded configured identity, without credentials or user private data. If no next real tag exists, leave this task open and publish that specific remaining criterion; do not mark the feature complete.

## Self-review and execution order

This plan owns spec §8 and checks9.13/16–18; companion plan owns device behavior. M1 clean baseline precedes product changes, registry evolves with them; M2 automation and M3 build are independently reviewable; M4 never executes real promotion before owner approval; M5 is future-release/live acceptance. Scheduled operation and next-tag execution are external gates, not claimed capabilities based only on source YAML. All five Review Focus conditions have owning production-tool tests.

Self-review: all maintenance acceptance criteria map to M1–M5; validator belongs to M1 so its tests pass before M2 consumes it. Existing build identity and sidecar target handling were checked in the baseline source. Document preparation does not imply baseline/candidate build success.

Owner reviews both plans and selects execution approach before implementation; no product-code change or scheduled job is created by writing these documents.


## Plan review corrections (Astron, 2026-10-03)

Review event `15d922cc63805f0535741fe2b6e45c2dfe2968ab07d3bc0351a2376aefd589c6`: R5 recurring full Python suite and workflow deletion test → M2/M3; R6 separate clean selected-upstream baseline required on every cron/manual update with failure preventing candidate → M2/M5. Nonblocking conflict-publication clarification → M2 clean report-only commit/blocked draft and safe resume. Source YAML/tooling/product changes are not made by this documentation correction.
