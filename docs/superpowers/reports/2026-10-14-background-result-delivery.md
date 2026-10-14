# Background result delivery — local implementation report

Specification: [task](../specs/2026-10-14-background-result-delivery-task.md). Plan: [implementation](../plans/2026-10-14-background-result-delivery.md).

Repository: `reynholm/buzz`; origin was fetched before work. Base: `e72a377e5fd3d552e0e30dc523535d9a79cf1e99`. Branch: `fork/background-result-delivery`, isolated worktree `/Users/reynholm/.buzz/REPOS/buzz-background-result-delivery`. No GitHub publication, PR or push was performed.

## A: preserve collection

Commit: `cabc051c3a4ad7343db3e1d87df6740362447540`. Ordinary traffic cannot replace the pending collection or its original scope/session/generation/thread route. Collection has separate bounded retry state; ready collection takes precedence, while backoff permits ordinary traffic. Automatic rotation retains the original session until collection ends. Steer/Interrupt preserve collection; explicit owner cancel/rotate, unambiguous observer cancellation and membership revocation invalidate it. Lost prompt tasks release retained authority.

The implementation changes `queue.rs`, `queue/background.rs`, `pool.rs`, `pool/background_work.rs` and `lib.rs` under `crates/buzz-acp/src`, with real-process regressions in `pool/background_work_tests.rs` and package documentation. Marker-controlled local ACP adapters exercise production dispatch/result handling, real children, Channel/Thread/DM routing, queue/steer/interrupt, backoff/finite failure, eager/lazy/sleep modes, max-turn rotation, owner/observer cancellation and access/generation fences.

A final complete package run passed **1096 tests, 0 failed, 3 ignored**. Formatting and ACP Clippy passed. Independent task review passed after fixing a reproduced explicit-cancel race and lost-task cleanup. Exact receipts: `.superpowers/sdd/2026-10-14-background-result-delivery/task-1-report.md`, `stage-a-final-complete.log`, `stage-a-final-fmt.log`, `stage-a-final-clippy.log`, `task-1-fix-2-review.md`. Reviewed file hashes were independently matched to the commit; tests ran on that matching pre-commit source, not on a future SHA.

## B: managed final-report delivery

B implementation commit: `33966afed44c35067f344a503fdf02f1d12d7489`. Review fix: `c07ce95adc6b82dc87aef28a4d67c16b43ae9460`. Implementation and local verification are complete; requested Astride24 whole-branch review remains pending.

One random UUID is allocated per observed background group and retained in `RuntimeContinuation`. A collection prompt supplies `buzz messages deliver`, original channel/parent and a quoted absolute persistent journal path. The signed `buzz-delivery` tag distinguishes different keys even with equal content/time. Identity is normalized relay URL + signer public key + UUID. This does not identify internal provider tasks that ACP never exposes.

The managed CLI stores the first signed event and original route before submitting through the existing Nostr publisher. A fixed advisory lock serializes cooperating processes; atomic replacement and file/directory fsync preserve the event across restart. Repeating the key with changed text returns the first event; changing route conflicts. Current signed channel membership is checked before submission or confirmed replay. Ordinary progress send and explicit edit remain independent; mentions belong to the first signed event.

The journal is bounded to 1024 records, 512 KiB per record and 16 MiB serialized total. Each record permits eight managed submissions, each with up to three existing transport attempts. The lock wait is bounded to 30 seconds. Pending/terminal records are retained; capacity/corruption/attempt exhaustion fail visibly. Confirmed entries may expire after seven days; expiry, manual deletion or volume destruction ends idempotency. Pending errors are structured `managed_delivery_pending`, exit 2, `retryable:true`: replay exactly the same key/route/journal, rather than creating another key/send.

Source responsibility: private `crates/buzz-cli/src/commands/delivery.rs` owns locking, bounded journal, validation, access checks, replay and acknowledgments. CLI parser/errors/dependency/message preparation wire the managed command. ACP continuation state/prompt/tests carry the key and original route. Package guides describe command use and native smoke checks.

Four real local runtime scenarios use actual CLI processes and rebuilt embedded MCP/bundled agent: publication followed by model HTTP 400 and ACP AgentError/retry; response-body loss and process restart; SIGKILL after relay acceptance before local acknowledgment; concurrent publishers. The mock relay verifies signed event readback and counts one unique event for one key. These are mock-relay/model workflows, not authenticated relay/provider or human app evidence. Detailed source fingerprints, RED/GREEN failures, resource/access/corruption/mention cases and full logs are indexed in `.superpowers/sdd/2026-10-14-background-result-delivery/task-2-report.md`.

Initial B-source validation before the ancestor-link review fix (all logs in the artifact directory above):

| Check | Result | Receipt |
| --- | --- | --- |
| Complete CLI | 512 passed, 0 failed; 1 ignored doctest | `stage-b-final-cli-complete.log` |
| Complete ACP | 1097 passed, 0 failed; 7 ignored | `stage-b-final-acp-complete.log` |
| Complete embedded MCP | 80 passed, 0 failed | `stage-b-final-mcp-complete.log` |
| Explicit built-binary managed workflows | 4 passed, 0 failed | `stage-b-final-runtime.log` |
| Format / CLI+ACP+MCP Clippy / rebuilt CLI+MCP | exit 0 | `stage-b-final-fmt.log`, `stage-b-final-clippy.log`, `stage-b-final-build.log` |

The four managed runtime tests are included among ACP's seven default-lane ignored tests and were explicitly exercised separately. CLI baseline was 493/0; final CLI adds 19 managed regressions. Prior failed command-catalog checks and a reproduced directory-creation fsync race are retained in the report; both are fixed. Final source seal: `8af029b4906a5e8a80cff75af485eebdf22660d290e43d431bdcd477f9007686`. Controller verified all 15 source file hashes. The complete ACP run started before final CLI-only corrections; exact ACP diff/file hashes stayed identical, so its result applies to the final ACP bytes.

## Reproduce verification

Activate Hermit and run complete affected packages serially. ACP requires all four harness overrides cleared and modern Git for the existing credential fixture:

```sh
. ./bin/activate-hermit
cargo test -p buzz-cli -- --test-threads=1
PATH=/Users/reynholm/.buzz/.scratch/M1-git-toolchain/install/bin:$PATH \
  env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL \
  -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY \
  cargo test -p buzz-acp -- --test-threads=1
cargo test -p buzz-dev-mcp -- --test-threads=1
cargo fmt --all -- --check
cargo clippy -p buzz-cli -p buzz-acp -p buzz-dev-mcp --all-targets -- -D warnings
cargo build -p buzz-cli -p buzz-agent -p buzz-dev-mcp
BUZZ_TEST_BIN_DIR="$PWD/target/debug" \
  env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL \
  -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY \
  cargo test -p buzz-acp managed_runtime_ -- --ignored --test-threads=1 --nocapture
```

No PR is being prepared, so `just ci`/PR gates are not represented as completed. Relay/db/auth production code is unchanged.

## Human smoke checks and evidence limits

The exact commands and expected event IDs/tags, same-key/fresh-key/concurrent runs, independent progress/edit and access revocation are in [CLI testing](../../../crates/buzz-cli/TESTING.md#managed-final-report-delivery). Use an existing authorized channel/root and an absolute persistent journal. Repeat the same key with changed summary and read back exactly one original report; a fresh key gives another. Remove publisher membership using an authorized owner and expect exit 3 with no redirected/new report. Check the native app shows original-thread delivery and callback mention.

For [ACP testing](../../../crates/buzz-acp/TESTING.md#managed-background-final-reports), launch a real background child; interleave another-thread traffic while live, ready, collecting and backing off. Publish via the supplied command, cause application error, and retry in the same session with changed summary. Expect one signed final report. Repeat with a second group and expect another key/report. Verify Channel/Thread/DM, Queue/Steer/Interrupt, eager/lazy, sleep 0/2, max-turns 1, explicit cancel/rotate and membership removal as in the lifecycle runbook. Repeated collection failures must reach a visible finite terminal outcome.

No human smoke confirmation or real authenticated relay/provider test is claimed. Process kill/lost response verifies ordering and recovery boundaries, not hardware power loss. Directory fsync/permissions were exercised on macOS/Unix; Windows/network filesystem durability is unverified and unsupported operations fail visibly. Detached/reparented-child tracking is unchanged. Provider results not journaled before loss, vanished persistent storage, harness queue/session resurrection and automatic startup replay are outside coverage. Arbitrary shell/MCP publication bypasses remain outside the managed-command guarantee.

## Rulings made during execution

1. Execute the user-approved task without another planning approval: the user explicitly instructed implementation. If wrong, local changes are reversible.
2. Explicitly prepend the owner-provided Git identity guard because the harness PATH omitted it. Effective identity/signing is checked before each commit. If wrong, commit attribution would be invalid.
3. Clear inherited `BUZZ_ACP_SESSION_POLICY` in verification, in addition to the three documented overrides; keep production defaults unchanged. If wrong, a deployment-input failure could be concealed. Original failing baseline is retained.
4. Use the existing scratch Git 2.49 for unchanged credential fixtures, without global install/configuration changes; actual source commits retain the owner guard. If wrong, an integration defect could be masked. Original Apple Git failures and matching modern-Git fixture results are retained.
5. Include the existing observer `cancel_turn` path for the same active-collection cancellation race. Preserve unambiguous active scope, sibling rejection and idle behavior. If wrong, Stop scope could broaden; production guard regressions constrain it.

## Final review

Internal A review: passed. Internal B review identified IMPORTANT B1, documented below; its complete final verdict was not preserved. Requested Astride24 review-master whole-branch review: pending. Thread completion remains unset while the required final review is pending.


## Review finding B1 and final verified source

The preserved internal B review finding concerns concurrent **nested** directory creation: after a creator exposes `/stable/new-parent/journal` but before it fsyncs `/stable`, another publisher can observe the existing chain. Syncing only `new-parent` leaves its own link unpersisted when publication begins. A process kill does not demonstrate the ensuing hardware-loss scenario; the filesystem ordering defect is nevertheless reachable before a send.

`managed_existing_nested_directory_requires_ancestor_link_sync_before_publication` exercises actual CLI parser/dispatch with a real filesystem and signature-checking mock relay. It makes the higher ancestor execute-only while the immediate journal parent remains readable. Before the fix the command published successfully (exit 0 instead of 4): RED 0 passed / 1 failed, exit 101 (`stage-b-ancestor-sync-red.log`). After the fix it refuses publication before any event: GREEN 1 passed / 0 failed (`stage-b-ancestor-sync-green.log`). Every invocation now fsyncs all ancestor directories, even an already existing chain, with a 128-ancestor bound. No ACP source changed.

The post-fix verification commands used the reproduction commands above on parent HEAD `33966afed44c35067f344a503fdf02f1d12d7489` plus the exact later committed fix. All logs below are under the same evidence directory:

| Check | Actual result | Receipt |
| --- | --- | --- |
| Complete CLI | 513 passed / 0 failed; one ignored doctest | `stage-b-post-ancestor-cli-complete.log` |
| Complete embedded MCP | 80 passed / 0 failed | `stage-b-post-ancestor-mcp-complete.log` |
| Format, CLI+ACP+MCP Clippy, rebuilt CLI+agent+MCP | exit 0 | `stage-b-post-ancestor-fmt.log`, `stage-b-post-ancestor-clippy.log`, `stage-b-post-ancestor-build.log` |
| Explicit actual-process managed runtime scenarios | 4 passed / 0 failed | `stage-b-post-ancestor-runtime.log` |
| Complete ACP | Earlier 1097 passed / 0 failed / 7 ignored applies to byte-identical ACP source; not rerun for this CLI-only fix | `stage-b-final-acp-complete.log`, source manifests |

Final B source seal relative to A: `370ad223eb08da963e6a63af2ccc7ebac03c28f69ccc7a9e355d116a3bc2a87a`. Files are listed in `stage-b-post-ancestor-source-manifest.json`; `stage-b-post-ancestor-reviewed.diff` is the sealed diff. All committed source hashes were checked against this manifest. The package guide was clarified after the checks; only documentation changed after those source checks. Final source commits are A `cabc051c`, B `33966afe`, B1 fix `c07ce95a`. The plan and this report are recorded separately. No new real-relay, hardware-loss or human-testing claim follows from these checks.
