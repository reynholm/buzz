# Releasing Buzz

Buzz has three independent release lanes. Desktop and relay use release PRs.
Mobile uses immutable release-candidate tags cut from remote `main` by default,
with an optional stable source branch:

| Lane | Entry point | Artifact |
|------|-------------|----------|
| Desktop | `just release-desktop <version>` | Packaged desktop app (signed/notarized macOS, unsigned Windows, and Linux) |
| Relay | `just release-relay` | `ghcr.io/block/buzz` container image |
| Mobile | `scripts/mobile-release.sh candidate X.Y.Z` | Exact `mobile-vX.Y.Z-rc.N` source identity |

The lanes version independently. Desktop reads its manifests, relay reads its
crate manifest, and mobile derives both source and marketing version from the
exact candidate tag. Mobile candidate tags are pushed by a human release
manager, not by CI or a GitHub App. The handoff to the private `buzz-releases`
pipeline remains manual because OSS CI cannot trigger private CI.

## Quick Start

Prepare desktop releases locally from an up-to-date, clean `main` checkout:

```sh
just release-desktop 0.5.3
```

The recipe generates the immutable candidate and opens or updates its pull
request. Candidate branch creation uses the operator's GitHub permissions; the
release App is intentionally limited to creating protected release tags.

```sh
# Relay release
just release-relay
just release-relay 0.4.0

# Publish the next mobile candidate from the exact current remote main commit
# (requires configured fork policies and exact-source qualification, see Mobile)
scripts/mobile-release.sh candidate 0.5.0
```

Desktop uses an immutable generated candidate PR; relay continues using its
metadata PR. Mobile does not. Each `mobile-vX.Y.Z-rc.N` tag is an immutable
candidate and the artifact of record.
Optional stable branches use `mobile-vMAJOR.MINOR`. There is no
stable mobile tag alias, finalization step,
or mobile GitHub Release.

---

## How It Works

### Desktop

Repository admins can manually validate an existing candidate with:

```sh
gh workflow run desktop-release-candidate.yml --repo block/buzz --ref main -f version=0.5.3
```

This checks out `version-bump/0.5.3` and runs the same immutable-candidate
validator as the PR check. Dispatch from `main`; both the original caller and
anyone rerunning the workflow must currently have repository admin permission.
GitHub allows users with write access to dispatch workflows, but this workflow
fails their manual runs before checkout. Permission lookup failures also stop
the run. Manual validation does not create a candidate, publish a release, or
replace the required checks on the candidate PR.

1. Run `just release-desktop <version>` from a clean, up-to-date `main` checkout.
   The script creates one deterministic candidate commit and records both its
   frozen base and the verified prior release ledger in candidate metadata.
2. Review the exact candidate SHA, complete changelog, and CI. Regenerating or
   pushing the branch creates a new candidate and requires checks to run again.
3. **Squash merge** the PR after all protected-branch checks pass. The merge is
   the human authorization event; an authorized owner/admin bypass is treated
   the same way. Unrelated changes reaching `main` do not invalidate the
   reviewed candidate.
4. `auto-tag-on-release-pr-merge` verifies the closed event against GitHub's PR
   identity, validates candidate content, and proves every required check came
   from its trusted producer and was successful when the PR merged. It creates
   `desktop-v<version>` at the exact reviewed PR head—not the squash commit.
   Retries accept that tag only at the same SHA and never move it. GitHub does
   not expose when an individual check rerun was created, so an ordinary rerun
   after merge deliberately makes tag verification fail closed; inspect that
   run and create a new candidate version rather than retrying the blocked tag.
5. The tag triggers `release.yml`. It builds and stages all platform artifacts,
   publishes the versioned release only after the complete set succeeds, then
   updates the rolling updater manifest last for stable versions.

Because squash merging leaves immutable candidate tags on side history, the next
release uses validated prior candidate metadata as its ledger boundary. It
includes unrelated commits after the prior frozen base and excludes exactly the
prior release's recorded squash commit; tag ancestry is deliberately irrelevant.

### Relay

1. **`just release-relay`** runs locally on `main`, creates or updates a
   `relay-release/<version>` PR, bumps `crates/buzz-relay/Cargo.toml`,
   regenerates `Cargo.lock`, and updates the relay changelog.
2. **Merge the PR.** `auto-tag-on-release-pr-merge` pushes
   `relay-v<version>`.
3. **The tag triggers `docker.yml`.** Stable releases update the version
   aliases and `latest`; prereleases do not. Each release also publishes an
   optimized, symbol-bearing image under matching `debug-` tags (for example,
   `debug-0.3.0` and `debug-latest`) for native profiling. The ordinary tags
   remain stripped and are the default for deployments that do not need it.

Every push to `main` that changes the image build context continues to
publish the rolling relay `:main` and `:sha-<7>` tags, plus matching
`:debug-main` and `:debug-sha-<7>` variants. Pushes that only touch paths
`.dockerignore` excludes (such as `desktop/`, `mobile/` and `docs/`) publish
nothing.

#### NIP-FI upload proof freshness (relay floor)

Desktop, CLI and mobile clients mint Blossom upload proofs with a 60-second
expiry. Before distributing these clients, verify that every supported relay
runs a qualified release containing `75e9bef748d2149ce459b14da842e706a51a5f78`
and record its deployed tag/SHA. That repair verifies freshness and authority
before reading the body, then checks only the declared SHA-256 after transfer
(`verify_upload_hash_only`). Older relays that repeat full expiry verification
after the body may reject a proof that was fresh on admission during a slow
upload. Clients cannot extend the proof beyond the Strict 60-second limit.

An unknown or unverified deployment does not satisfy this prerequisite.
Record a supported-relay matrix with concrete release/SHA and workflow
evidence before distributing clients; updating this document does not qualify
a deployment. Upgrading a pre-floor relay requires the normal release and
deployment authorization.

### Mobile

Mobile candidate publication in this fork is limited to `reynholm/buzz` and
GitHub operator `reynholm`. It is **closed until the operator configures and
verifies the fork release policies and exact-source qualification below**.
There are no default ruleset IDs and no fallback to upstream settings. The
removed mobile candidate workflows remain removed; this command does not
dispatch a workflow, build a mobile artifact, or publish to an app store.

1. **Qualify an immutable source.** Required checks must finish successfully for
   the exact selected commit in the protected source history. A different local
   HEAD or a later branch tip cannot supply its qualification. Missing, failed,
   skipped, pending, wrong-producer or wrong-workflow checks stop publication.
2. **Publish a candidate only after those gates hold.** From a clean checkout
   whose configured and effective `origin` fetch/push URLs all identify the
   authorized fork, run `scripts/mobile-release.sh candidate X.Y.Z`. The script
   snapshots `origin/main`, checks the authenticated operator and configured
   policies, verifies the selected commit's trusted checks, then creates an
   annotated `mobile-vX.Y.Z-rc.N` tag and pushes exactly that one ref. Your Git
   author/tag-signing configuration remains separate from GitHub authorization.
   Existing tags are never moved or forced, and other local tags are not pushed.
   Use `--commit FULL_SHA` to select a qualified historical ancestor. A later
   branch advance does not change the selected commit or its required checks.
3. **Verify publication before using it.** GitHub must report the exact tag
   object in the authorized fork, and an independent remote fetch must resolve
   the annotated tag to the selected source SHA. If a rejected push left no
   remote tag, the local tag is removed for a clean retry. An ambiguous push or
   unavailable readback retains the local evidence and reports failure rather
   than claiming publication.
4. **Build and distribute separately.** A source tag is not a signed native
   artifact or an app-store acceptance record. A separately authorized fork
   build/distribution process must record the exact tag, source SHA, platform
   artifact identity and native acceptance. This repository does not grant
   access to or dispatch upstream's private mobile build pipeline.

Each iOS or Android artifact records its exact candidate tag. Different platform
artifacts can use different RC tags for the same marketing version; no mutable
stable alias or finalization ref is created.

### Optional stable release branches

For a release that needs isolation from ongoing development, create a branch
such as `mobile-v0.25` from an explicitly selected, verified source commit.
Publish candidates from its current remote tip:

```bash
scripts/mobile-release.sh candidate 0.25.0 --source-branch mobile-v0.25
```

The branch must match the marketing version's major and minor components:
`mobile-v0.25` accepts 0.25.x, but rejects 0.26.x and 1.25.x. Without the option,
the source remains `main`. The script validates the source;
a missing branch or a commit outside its history fails rather than falling back
to `main`. `--commit FULL_SHA` also works with a stable source branch. Its
review and check protections must cover the selected branch without bypass
actors, including the release operator. The same exact-source qualification is
required for `main` and stable sources; there is no main-branch exemption.

General fix PRs must target `main` and land there first. At each fix landing,
ask the release owner whether to
backport the merged commit to the stable branch. For each explicitly approved
fix, cherry-pick with `-x --signoff` onto a backport branch and open a backport-only
PR targeting the stable branch. Require review and passing checks before merging
that PR. Preserve the original commit provenance and validate the resulting
stable source. A correction for a regression introduced by an earlier backport
may target the stable branch directly; identify the faulty backport and keep the
correction scoped to that regression. The same review and checks apply.
Never merge `main` wholesale.
Remote agents and App Clip are post-conference work. There is no preselected
fix list and no requirement to create a stable branch for every minor release.

The initial 0.25.0 candidate pins that baseline with `--commit`. The second
candidate uses an empty commit above that baseline to verify
that the existing pipeline builds a commit outside `main`. Verify identical
source trees, absence of that commit from remote `main`, and the exact tag,
commit, marketing version and build number in the uploaded artifacts. Upload
and external TestFlight review do not establish production App Store approval.

`mobile/pubspec.yaml` keeps `0.0.0+1` only as a valid, visibly non-release
fallback for local development and validation builds. Release jobs always
inject both version fields. `mobile/CHANGELOG.md` is retained as historical
release data. It is not a release ledger for this flow.

---

## Version Sources

| Lane | Release version authority |
|------|---------------------------|
| Desktop | `desktop/package.json` and synchronized desktop manifests |
| Relay | `crates/buzz-relay/Cargo.toml` |
| Mobile | Exact `mobile-vX.Y.Z-rc.N` remote tag |

`just bump-desktop-version <version>` updates the desktop manifests and
regenerates their lockfiles. `just bump-relay-version <version>` updates the
relay crate and regenerates `Cargo.lock`. Mobile has no bump recipe or
release-metadata PR.

---

## Signed macOS Canary

Use the manual **Signed macOS Canary** workflow when you need an Apple Silicon
build of current `main` for explicit testing without publishing a release:

```sh
gh workflow run signed-macos-canary.yml --repo block/buzz --ref main
```

The workflow derives a `-test.<run-number>` version, signs and notarizes the
DMG, verifies it with Gatekeeper, and uploads it as a short-lived Actions
artifact with seven-day retention. Because this is a public repository, any
signed-in GitHub user can download that artifact while it exists; it is
unpublished, not private. The workflow has no release permissions, does not
create or move tags, and cannot update `buzz-desktop-latest` or `latest.json`.

Download the artifact from the completed run:

```sh
gh run download <run-id> --repo block/buzz --name <artifact-name>
```

The workflow intentionally accepts only `main`. Use the normal release process
for distributable builds or builds from an immutable release tag.

---

## Release Retry

`release.yml` has no manual dispatch and cannot build from `main` or another
caller-selected ref. If a run for an existing immutable
`desktop-v<version>` tag fails, rerun that failed workflow from GitHub Actions
(or use `gh run rerun <run-id> --failed --repo block/buzz`). A rerun
repairs the versioned draft if publication did not complete. It does not
promote that version to the auto-updater; promotion is a separate manual
action. Do not recreate, move, or push the immutable tag again.

Mobile intentionally has no branch or arbitrary-ref fallback. A separately
configured fork build process must consume the exact qualified candidate tag.

---

## Internal Releases

Fork mobile builds do not use upstream's private release pipeline. Configure
and qualify a separate build/distribution process before using an exact mobile
RC tag. For upstream desktop internal builds, start
[Release Desktop](https://buildkite.com/runway/sprout-releases) and enter the
exact public source tag as `desktop_ref=desktop-v<version>`; a generic
`v<version>` tag is intentionally rejected. See the
[buzz-releases README](https://github.com/squareup/buzz-releases#cutting-a-release)
for the rest of the private pipeline contract.

---

## What Gets Published

Desktop publishes two GitHub releases:

1. **`desktop-v<version>`**: the user-facing release with installers and the
   exact `updater-manifest.json` promotion candidate. Publishing this release
   does not expose it through in-app auto-update.
2. **`buzz-desktop-latest`**: the rolling auto-updater release. Its
   `latest.json` changes only through the manual promotion workflow.

### Promote an OSS desktop release to auto-update

After installing and testing the published `desktop-v<version>` artifacts, run
**Promote OSS Desktop Auto-Update** from the `main` branch and enter the exact
stable `X.Y.Z` version. The workflow validates the immutable tag and release,
the retained manifest and every referenced updater asset, and requires the
version to be newer than the currently promoted version before replacing
`buzz-desktop-latest/latest.json`. Same-version retries succeed only when the
manifest is identical; downgrades are rejected.

Withholding promotion leaves existing clients on the previous version. If a
promoted release is bad, ship and promote a higher patch version; changing the
manifest to an older version does not downgrade clients that already updated.

Mobile publishes only annotated `mobile-vX.Y.Z-rc.N` git tags. Store artifacts
and rollout records retain the exact tag they used. Mobile does not publish a
GitHub Release or a stable `mobile-vX.Y.Z` alias.

---

## Platform Support

The release workflow builds **two separate macOS DMGs**: Apple
Silicon (`darwin-aarch64`, the `release` job) and Intel
(`darwin-x86_64`, the `release-macos-x64` job), an unsigned Windows x64
NSIS installer (its filename includes `_alpha-unsigned`), and Linux `.deb` and
`.AppImage` packages. Both macOS DMGs are codesigned, notarized, and attached
to the same `desktop-v<version>` release. Intel users
download the `_x64.dmg`.

The Linux AppImage is post-processed by `desktop/scripts/fix-appimage.sh`,
which strips infra libraries over-bundled by linuxdeploy (they crash on
Mesa 25+ / GLib 2.88 distros; see
[tauri-apps/tauri#15665](https://github.com/tauri-apps/tauri/issues/15665))
and re-signs the artifact. As a result the AppImage relies on the
host's Wayland/GStreamer/graphics stack and requires GLib >= 2.72
(Ubuntu 22.04 or newer). The `release-linux` job builds inside a
`ubuntu:22.04` container for broad GLIBC compatibility.

---

## Prerequisites

- **Write access** to the `block/buzz` GitHub repository
- An `origin` remote whose configured URL is the canonical `block/buzz`
  repository
- `gh` CLI authenticated with permission to push the candidate branch and open
  its pull request
- The Default `main` ruleset configured for squash-only merging, strict required
  checks, stale-review dismissal, and the **Desktop Release Candidate** check
- Release tag ruleset [`14378754`](https://github.com/block/buzz/rules/14378754)
  active for all tags (`~ALL`), with creation, update, deletion, and
  non-fast-forward protections and `buzz-release-bot` as its sole always-bypass
  actor. It must exclude `refs/tags/mobile-*`: rulesets stack, so otherwise it
  rejects release managers' mobile tag pushes. `scripts/mobile-release.sh`
  checks that exclusion before tagging.
- The `buzz-release-bot` App credentials configured for GitHub Actions (desktop,
  relay, and chart lanes only; mobile does not use them)
- The following **GitHub Actions variables and secrets** configured for the
  desktop release lane:

  | Name | Kind | Purpose |
  |------|------|---------|
  | `BUZZ_RELEASE_TAGGER_CLIENT_ID` | Variable | GitHub App client ID used to create protected release tags |
  | `BUZZ_RELEASE_TAGGER_PRIVATE_KEY` | Secret | GitHub App private key |
  | `OSX_CODESIGN_ROLE` | Secret | macOS signing role used by `block/apple-codesign-action` |
  | `CODESIGN_S3_BUCKET` | Secret | macOS signing exchange bucket |
  | `BUZZ_UPDATER_PUBLIC_KEY` or `SPROUT_UPDATER_PUBLIC_KEY` | Secret | Tauri updater public key |
  | `TAURI_SIGNING_PRIVATE_KEY` | Secret | Tauri updater private key |
  | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Secret | Password for the private key |

### Fork mobile publication gates

The desktop/relay prerequisites above do not configure mobile release authority.
For mobile, an operator must first establish and independently verify two active
rulesets in `reynholm/buzz`, then supply their actual IDs through
`BUZZ_MOBILE_RELEASE_TAG_RULESET_ID` and
`BUZZ_MOBILE_RELEASE_BRANCH_RULESET_ID`. Empty, invalid, inaccessible or
mismatched policy is an error before tag creation. Do not copy upstream IDs or
weaken protection to make a release command succeed. The script never creates
or edits repository settings.

- The tag policy targets only `refs/tags/mobile-*` or `refs/tags/mobile-v*`, has
  no exclusions, and prevents creation, update, deletion and non-fast-forward
  changes. Its sole always-bypass actor is the verified GitHub user `reynholm`;
  no team or App substitutes for that operator. The authenticated user's
  effective bypass must also be `always`.
- The branch policy always covers `refs/heads/main` and also the selected stable
  branch (explicitly or with `refs/heads/mobile-v*`), without exclusions or bypass
  actors. It prevents deletion/force pushes, requires at least one current
  approval, dismisses stale approvals, requires last-push approval and resolved
  review threads, and enforces strict required checks.
- At least the existing `Mobile` and `DCO Check` contexts are required.
  Allowlisted CI contexts are pinned to GitHub Actions (App ID 15368).
  `DCO Check` must explicitly select a positive integer `integration_id` in the
  authenticated fork branch policy. The operator must establish that this App
  is the intended DCO producer when configuring protection; no inherited
  upstream ID, public App discovery or fixture establishes its installation or
  authority in the fork. An absent, null, zero, negative, fractional or string
  ID is refused. The latest exact-source DCO check must succeed and match that
  configured App; a same-name check from another App cannot substitute.
  Every additional context in that branch policy is also verified and must be
  one of the existing fork CI gates: `Rust Lint`, `Unit Tests`, `Security`,
  `Windows Rust (x86_64-pc-windows-msvc)`, `Desktop`, `Desktop Build (macOS)`,
  `Desktop E2E Relay`, `Desktop E2E Integration`,
  `Backend Integration (relay e2e)`, `PostgreSQL Tests`, `Relay E2E` or `Web`.
  An unknown producer/context fails closed instead of being silently ignored.
- The latest trusted check for each required context must be completed with
  `success` on the selected full SHA. GitHub Actions checks must belong to the
  fork's `.github/workflows/ci.yml`, with matching workflow, run, check suite
  and source identities, and a successful internal push/PR run. A copied check
  name, skipped context, another repository/workflow, or a successful older
  attempt cannot authorize publication. Local development hooks do not replace
  this qualification and are not bypassed by the release script.

Absent policy configuration or qualification is a release configuration gate,
not completed acceptance. Configuring rulesets, generating trusted CI receipts,
creating tags, building signed artifacts and distributing them are separately
authorized operator actions.

---

## Troubleshooting

### The desktop candidate is stale or cannot be squash merged

Do not update the branch manually and do not weaken the ruleset. Run
`just release-desktop <version>` again from current `main`; this regenerates the
candidate, reruns CI, and requires a fresh trusted approval on the new exact
head. The post-merge verifier refuses to tag a squash whose parent differs from
the recorded candidate base or whose tree differs from the validated PR head.

### Local `just release-desktop` fails with "must be on main branch"
Switch to `main` and pull latest before running the release recipe.

### Local `just release-desktop` fails with "working tree is dirty"
Commit or stash your changes before running the release recipe.

### New commits land after publishing a mobile candidate

Run `scripts/mobile-release.sh candidate <version>` again after the intended
fix reaches the intended remote source branch. Use the same `--source-branch`
option for a stable release. The command publishes a new immutable RC tag at the new exact
remote commit. Continue referring to each tested or shipped platform artifact by
its own exact tag.

### The source branch advances during candidate publication

Publication remains pinned to the selected SHA. The script verifies that it
is an ancestor of a resolved source-branch snapshot, then creates, pushes, and
verifies an immutable annotated tag for that exact SHA. It verifies the tag,
not the branch's later tip. A newer commit does not require retrying or creating
another candidate. Missing or unverifiable ancestry still fails closed.

### A mobile candidate command selects the wrong RC number

Do not retry by moving or deleting a tag. Inspect the exact remote `mobile-v*`
tags and resolve the unexpected state. Candidate numbers are monotonically
increasing remote identities.

### A mobile candidate publication is rejected by repository rules

Confirm the authenticated operator and configured fork policies match the
mobile publication gates above. Missing policy or exact-source qualification
must be resolved before creating a tag. A rejected push removes its local tag
only when the remote certainly has none; retry after an administrator verifies
the intended configuration. Do not add bypass actors or weaken update/deletion
protection to force a release. Existing candidate tags remain immutable.

### Auto-updater reports "no update available"
Verify that the `buzz-desktop-latest` release exists and contains a
valid `latest.json`. The manifest covers all four platform keys
(`darwin-aarch64`, `darwin-x86_64`, `linux-x86_64`,
`windows-x86_64`); a missing entry usually means that platform's
release job failed. Check the workflow run.
