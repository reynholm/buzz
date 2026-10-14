# Background result delivery — local implementation report

Specification: [task](../specs/2026-10-14-background-result-delivery-task.md). Plan: [implementation](../plans/2026-10-14-background-result-delivery.md).

**Current status: verification/review pending.** A2 is committed as `59fe7b69817395933225c7b91e02dcaecabba8e8`. The owner selected option 1 for B2, now implemented in the working tree as atomic canonical report election with a guarded production writer. Ten explicit recovery/race/WS cases pass; full package checks and fresh review are in progress. The previously reviewed timestamp-only proposal was insufficient. Inherited gate decisions remain open; no completion reaction or GitHub publication.

Repository: `reynholm/buzz`; origin was fetched before work. Base: `e72a377e5fd3d552e0e30dc523535d9a79cf1e99`. Branch: `fork/background-result-delivery`, isolated worktree `/Users/reynholm/.buzz/REPOS/buzz-background-result-delivery`. No GitHub publication, PR or push was performed.

## A: preserve collection

Commit: `cabc051c3a4ad7343db3e1d87df6740362447540`. Ordinary traffic cannot replace the pending collection or its original scope/session/generation/thread route. Collection has separate bounded retry state; ready collection takes precedence, while backoff permits ordinary traffic. Automatic rotation retains the original session until collection ends. Steer/Interrupt preserve collection; explicit owner cancel/rotate, unambiguous observer cancellation and membership revocation invalidate it. Lost prompt tasks release retained authority.

The implementation changes `queue.rs`, `queue/background.rs`, `pool.rs`, `pool/background_work.rs` and `lib.rs` under `crates/buzz-acp/src`, with real-process regressions in `pool/background_work_tests.rs` and package documentation. Marker-controlled local ACP adapters exercise production dispatch/result handling, real children, Channel/Thread/DM routing, queue/steer/interrupt, backoff/finite failure, eager/lazy/sleep modes, max-turn rotation, owner/observer cancellation and access/generation fences.

A final complete package run passed **1096 tests, 0 failed, 3 ignored**. Formatting and ACP Clippy passed. Independent task review passed after fixing a reproduced explicit-cancel race and lost-task cleanup. Exact receipts: `.superpowers/sdd/2026-10-14-background-result-delivery/task-1-report.md`, `stage-a-final-complete.log`, `stage-a-final-fmt.log`, `stage-a-final-clippy.log`, `task-1-fix-2-review.md`. Reviewed file hashes were independently matched to the commit; tests ran on that matching pre-commit source, not on a future SHA.

## B: managed final-report delivery

B implementation commit: `33966afed44c35067f344a503fdf02f1d12d7489`. Review fix: `c07ce95adc6b82dc87aef28a4d67c16b43ae9460`. Local B implementation is committed, but whole-feature verification is incomplete. Astron whole-branch review reported A2/B2; A2 is fixed and B2 remains open after scoped re-review.

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

No PR is being prepared, so `just ci`/PR gates are not represented as completed. Before A2/B2 review corrections, relay/db/auth production code was unchanged. B2 below adds a narrowly scoped shared-relay timestamp admission policy; database and auth code remain unchanged.

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

Internal A review: passed. Internal B review identified IMPORTANT B1, documented below; its complete final verdict was not preserved. Owner corrected the reviewer to Astron. Whole-branch review at `904bc0a5`: FAIL with A2/B2. Scoped V2 re-review: FAIL, A2 fixed and B2 partially fixed. Thread completion remains unset while B2 and gate decisions remain open.


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


## Astron review A2/B2 corrections

Reviewer report: `/Users/reynholm/.buzz/RESEARCH/BACKGROUND_RESULT_DELIVERY_REVIEW_ASTRON_2026_10_14.md`, reviewed base `e72a377e` through `904bc0a5`. Both findings were independently verified; A1, lost-task cleanup, observer cancellation and B1 remained fixed. Astride24 stopped without a verdict after the owner corrected the reviewer name to Astron.

**A2:** A collection's token permits one ordinary cancellation. `requeue_as_cancelled` records that cancellation in `RuntimeContinuation.ordinary_preempted`; both exact-scope and unambiguous-channel signal paths then defer ordinary Steer/Interrupt, while explicit controls remain allowed. Native steering queues during all collection attempts, preventing a repeatedly extended deadline. A protected retry completes in its original session, then queued traffic runs; errors retain their existing separate finite retry budget/backoff. A fresh background group starts with a fresh false flag. No independent cancellation counter or unbounded table was added.

Marker-controlled `background_collection_completes_under_repeated_ordinary_preemption` failed before changes (RED 0/1, exit101, `A2_RED.log`). Its first post-fix fixture incorrectly expected recoverable metadata on a normal Drop-mode foreground turn; that test-only assumption was replaced by assertions on the actual subsequent wire prompt. `A2_GREEN.log` retains this initial fixture failure. Final background regression group:31/0 (`A2_BACKGROUND_GREEN.log`). The new test exercises actual listener fallback/Interrupt paths, eight following ordinary events, native-steer admission refusal, collection completion and subsequent message delivery. Complete ACP:1098/0, seven ignored (`A2_ACP_COMPLETE.log`). Exact six ACP source hashes are sealed in `A2_ACP_TESTED_SOURCE.json` and remained unchanged after this run; package documentation was clarified separately.

**B2 first proposal — insufficient, superseded by the production-writer finding below:** Preserve the saved signed event rather than re-signing an uncertain delivery. Shared relay ingest now permits a past timestamp beyond900 seconds only for a signature-verified self-published kind9 with exactly one two-element `buzz-delivery` tag whose UUID is canonical. The future900-second limit remains; unsigned saturating comparisons also reject extreme future timestamps safely. All subsequent current scope, membership, restriction, archive, signature/content and thread checks still run before storage/deduplication. There is no early-success bypass, new HTTP endpoint, schema migration, journal format change or UUID-based relay deduplication. Ordinary old messages and malformed markers remain refused.

This is an explicit sender opt-in to historical timestamp admission: the marker does not prove a background task, a prior attempt or a durable journal. A currently authorized publisher can choose a backdated managed message. The event preserves its historical ordering. Older relays retain their fifteen-minute window; this recovery fix requires an updated relay. It was exercised locally, not deployed to the user's relay. Existing eight-attempt and retention limits remain in force.

The initial shared-ingest B2 RED used isolated schema-compatible Postgres with an unarmed `Db::from_pool`, not the production writer: an hour-old saved event failed timestamp admission (0/1, exit101, `B2_RED.log`). Final ignored real-ingest cases cover an absent old event, an already accepted old event, duplicate thread-counter behavior, invalid signature/author/ancestry, revoked membership and archive. An additional case runs separate actual CLI processes against the **real HTTP router with NIP98 verification, real Redis replay protection and real Postgres**, restores pending journals in both remote states and verifies the exact original event, one stored ID and durable local acknowledgment. This is not a mock HTTP relay, but the storage setup omits the production floor guard and cannot prove production late insertion. It remains an in-process local server fixture, not a deployed provider/app or human acceptance test. Original journals are seeded as explicit crash/outage states; the already accepted case passes through actual ingest.

Initial three explicit integration cases on the unarmed fixture:3/0 (`B2_FINAL_REAL_RELAY.log`). Fixed-clock production timestamp boundary test:1/0 (`B2_BOUNDARIES.log`). Four bundled-agent/MCP/CLI mock-runtime scenarios remain4/0 (`A2_B2_RUNTIME.log`). Final fmt/clippy exit0 (`A2_B2_FINAL_FMT.log`, `A2_B2_FINAL_CLIPPY.log`); all five binaries rebuilt (`A2_B2_BUILD.log`). Tests ran on parent904bc0a5 plus the exact sealed working fixes, not on a future commit SHA. `A2_B2_FINAL_SOURCE.json` records the final source; only the package guide's attempt-budget wording was clarified afterward.

**Inherited verification failure:** two complete relay-package attempts returned1411 passed /1 failed /367 ignored in the library, stopping before later targets (`B2_RELAY_COMPLETE.log`, `B2_RELAY_COMPLETE_RECHECK.log`). The failing existing test is `api::mesh_demo::tests::demo_join_forwarded_arm_round_trips_echo`, returning504 instead of200. It also fails in an isolated module and exact-test process; one earlier exact run passed. Disabling Redis makes the fixture silently skip and is not accepted as verification. The five mesh implementation/fixture files are unchanged from basee72a377e. Source-supported likely cause: `run_demo_echo` selects a100ms tick against `recv_validated`, which consumes a frame then awaits Redis; cancellation can lose that frame. No cancellation trace was instrumented, so this remains a likely cause rather than an observed mechanism. No unrelated mesh fix was included.

A separate full-target remainder run explicitly skipped only that test:1411 library +27 binary +9 boot-lifecycle =1447 passed,0 failed,371 ignored,1 filtered out (`B2_RELAY_REMAINDER.log`). This is **not** a green full relay package. The mandatory repository `just test` was also launched against newly created task-owned standard dev services; it finished exit1 with two failed steps: `buzz-db tests` and `workspace integration tests`, both at the unchanged source assertion `crates/buzz-db/tests/observability_source.rs:357`. `DB_OBSERVABILITY_BASELINE.json` verifies the test and moderation source are byte-identical to base, where the required `fetch_optional` is already absent. Feature completion remains unset pending B2 and owner decisions about inherited gates.

Additional ruling: choose narrow relay admission over readback followed by re-signing, because a negative read cannot fence an in-flight original publication and does not deliver a saved absent expired event. Cost if wrong: managed publishers can opt into past-dated admission; current authority, signature and future-limit regressions constrain that policy. Documentation and this report make the changed policy and required relay upgrade explicit.


## V2 re-review: production writer exposes remaining B2

Astron report: `/Users/reynholm/.buzz/RESEARCH/BACKGROUND_RESULT_DELIVERY_REREVIEW_ASTRON_2026_10_14.md`, STATUS FAIL. Reviewer independently confirms A2 fixed, earlier A1/cleanup/observer/B1 preserved, and B2 partially fixed. Its own full ACP run passed 1098/0, seven ignored. The first V2 manifest contains 13/13 files; the report's B2 conclusion follows production code and SQL, not a reviewer DB run.

`Db::new` arms `buzz.created_at_floor=960` on every writer connection (`crates/buzz-db/src/runtime/mod.rs:676`), matching relay startup at `main.rs:308`. Migration 0021 rejects a new old channel row at deferred COMMIT. Already-stored events use `ON CONFLICT DO NOTHING`, so they can recover without inserting; absent old events cannot. Disabling this guard would invalidate the replica-fence ordering premise and is not a valid fix.

Recovery fixtures now use `Db::new` and assert the production GUC. Accepted historical events are seeded separately through an unarmed fixture pool to represent a pre-outage committed state; all tested recovery uses the guarded writer. Tests for accepted and absent states are split so a failure cannot hide the accepted branch. The first fixture edit had a duplicate pool-reference compile error; it is retained in `B2_PRODUCTION_WRITER_COMPILE_ATTEMPT.log` and was corrected before measuring behavior.

Owner contract question published as Buzz event `7b8cb24e0a9124786c98611c264d04f6fd88e177a0de290a3401153653e36519`: choose full atomic delivery-key recovery that may create fresh signed bytes only for an unaccepted report, or explicitly bounded recovery with visible terminal expiry. No answer or waiver is inferred from silence. No database guard change, blind re-signing, GitHub publication or root completion reaction has been made.

Corrected production-writer run: **3 passed, 2 failed**, exit101 (`B2_PRODUCTION_WRITER_RED.log`). Accepted-event shared ingest and actual CLI/HTTP/NIP98 recovery pass, as do current signature/route/membership/archive checks. Both absent-event cases fail: shared ingest exposes the deferred floor exception, and actual CLI returns `managed_delivery_pending` after HTTP500 while preserving the signed event. This confirms the remaining B2 defect without disabling the guard. No full green relay claim follows.

A2 local commit attribution and contents were verified after committing: `59fe7b69817395933225c7b91e02dcaecabba8e8`, configured owner author/committer, required DCO, no signing. All six committed ACP source hashes match `A2_ACP_TESTED_SOURCE.json` and the 1098/0 package run. The package guide is the seventh committed file. B2 source and corrected reports remain working changes, deliberately uncommitted pending the contract decision.


## Owner-approved atomic B2 recovery

Owner decision `e5539b60bf6d93287d8c94c11d8ec97748bfc13e900fd00cc4b225fd25a05a2c` selects option 1 from the published tradeoffs. This authorizes fresh signed bytes only for an unaccepted report while preserving original author, content and exact route/tags. Implementation plan: `docs/superpowers/plans/2026-10-14-background-result-atomic-recovery.md`.

New ephemeral Nostr command kind 20003 carries a verified original and current candidate. Shared ingest verifies all three signatures and immutable fields, then sends the candidate through the ordinary kind-9 scope, membership, restriction, archive, content and ancestry gates. HTTP uses the existing bridge; WS excludes this command from generic ephemeral fanout and retains its effect permit. WS OK correlates with the outer request ID, while its `managed-delivery:` message contains the canonical author-signed report; HTTP `event_id` names that report. No relay signing or new HTTP endpoint is introduced.

Normal marked publication and recovery acquire the same transaction advisory lock by community/author/UUID. The guarded writer returns an existing canonical binding or adopts a historical original, including soft-deleted rows, before inserting a current candidate. Candidate, thread metadata and signed canonical ledger row commit together. A deferred floor error rolls back the binding too. Only newly inserted candidates fan out or contribute counters. Different immutable payload/routes conflict. Migration 0056, desired schema and community deletion expected/purge inventories include the ledger and its universal write fence. One bounded signed-event row is retained per elected report until community purge; ordinary soft deletion does not reopen election.

CLI journal v1 migrates under its lock to v2. Original evidence, a refreshed candidate and a differing canonical event remain bounded by the existing record/journal limits. The command is used once original age exceeds 600 seconds, with fresh-clock candidate renewal. Before publication the candidate is journaled and fsynced; after a signature/identity/immutable-payload-checked receipt, confirmation is persisted before returning. Lost acknowledgment can return an earlier candidate. Old-server wrapper acknowledgments are refused. Timely normal publication remains compatible with the prior relay, but full recovery needs a coordinated writer upgrade and drained old requests. Keys must not be reused across completion groups.

Initial protocol RED: unknown-kind refusal, 0/1 (`B2_ATOMIC_COMMAND_RED.log`). The first GREEN attempt exposed missing test schema (`B2_ATOMIC_COMMAND_FIRST_GREEN.log`). Updating the existing desired-state fixture through pgschema failed on inherited partition PK alterations (`B2_ATOMIC_SCHEMA.log`), atomically rolling back. Switched angle to a new task-owned `buzz_atomic` database bootstrapped by the actual production migration runner; no guard or schema premise bypass. Intermediate tests exposed an outdated same-ID expectation and a test-only string borrowing compile error; both are retained in intermediate logs rather than hidden.

Eight explicit migrated production-writer cases passed (`B2_ATOMIC_REAL_RACE.log`): absent/accepted hour-old reports, deleted-original adoption, lost acknowledgment of an earlier candidate, renewal after another restart, actual standalone CLI/NIP98/HTTP and current authority boundaries, plus eight three-way original/distinct-candidate races with one event and one thread-counter contribution each. Historical rows and journals are seeded outage snapshots, not actual elapsed-hour or hardware-crash experiments. Full core/CLI checks: CLI 514/0, core 264/0 plus two doctests; one ignored CLI doctest (`B2_ATOMIC_CORE_CLI_FULL.log`). All-target core/DB/CLI/relay clippy exit0 (`B2_ATOMIC_CLIPPY.log`). Final rebuilt-binary runs and full gates follow below.

The first full DB package found two change-related migration expectations (count 55 and missing new fence target); updated these existing parity guards to include migration 56 and the new ledger. The live desired/migrated schema parity test now covers ledger columns/indexes. Full relay still fails the unchanged mesh echo (1411/1, 372 ignored, `B2_ATOMIC_RELAY_FULL.log`); this is not a green complete relay package. No scope expansion to unrelated mesh or DB observability fixes has been authorized.


## Internal atomic-recovery review corrections

Finite independent code review found two IMPORTANT regressions in the initial atomic implementation: WS ordinary-original ACK used the elected candidate ID, and duplicate full-event payloads exceeded the recovery cap for previously admitted quote-heavy 64 KiB reports. Original scenarios were reproduced through actual NIP42 WS (`B2_WS_ACK_RED.log`, 0/1) and actual CLI/NIP98 HTTP (`B2_SIZE_RED.log`, 0/1).

Both are fixed: all managed WS OK messages correlate the submitted request ID while the signed receipt names the canonical report. Wire/journal candidates use `ReportStamp {created_at,id,sig}`, reconstructing the original immutable body/tags/author and verifying the resulting event. This removes duplicate body copies without increasing the 512 KiB record/16 MiB journal bounds. Recovery content is capped at512 KiB and full HTTP envelope at1 MiB; the canonical receipt remains a full signed Event. Legacy v1 fixtures contain no new fields, and v2 candidate renewal remains separately covered.

Final explicit run:10/0 (`B2_STAMP_WS_FINAL.log`), including actual WS original and wrapper submissions, three maximum-size bodies (quotes/backslashes/U+0001), true legacy v1 recovery, lost previous-candidate ack and the prior guarded-writer races/authority cases. These use production migrations and `Db::new`; no floor bypass. All-target clippy passed for core/DB/CLI/relay (`B2_STAMP_CLIPPY.log`). Independent targeted re-review returned PASS by reading and serialization probes; it did not run Rust tests. Its probe confirmed maximum-size bodies fit all bounds. Full reviewer receipt will be preserved with the final packet.

Live desired-state versus production migration schema parity:1/0 (`B2_ATOMIC_SCHEMA_PARITY.log`), now covering the canonical ledger columns/indexes in addition to existing admin tables. Complete DB library after updating change-related migration expectations:138/0,391 ignored. Complete DB package remains red solely at the unchanged observability source assertion (2/1 source tests, `B2_ATOMIC_DB_FULL_V2.log`). This is a baseline limitation, not a full-green claim.


Final compact-source checks: complete CLI514/0 and core264/0 plus2 doctests (`B2_STAMP_CORE_CLI_FULL.log`); full relay **1448/0**,378 ignored (`B2_STAMP_RELAY_FULL.log`), including the formerly failing unchanged mesh test. Earlier failures remain evidence of an intermittent baseline test, but no test was filtered from this successful complete run. Freshly rebuilt bundled agent/MCP/CLI workflows4/0 (`B2_STAMP_RUNTIME.log`), build exit0 (`B2_STAMP_BUILD_ALL.log`), fmt/clippy exit0. The final explicit guarded recovery run also asserts a rejected expired ordinary write leaves no canonical binding, then recovers through the current candidate (`B2_STAMP_GUARD_FINAL.log`).

The broad pre-commit `just test` finished exit1 with the same two inherited failed steps: DB package and workspace integration at `observability_source.rs:357`. Its earlier core/CLI stages ran before compact-stamp review corrections, so it is not attributed as final stamp-source evidence; a post-commit required gate is planned. Updated migration expectations pass; the unchanged moderation/test hashes remain those in `DB_OBSERVABILITY_BASELINE.json`. No source-assertion fix or gate waiver is inferred from the owner's B2 contract decision.
