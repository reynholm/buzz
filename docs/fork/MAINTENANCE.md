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
