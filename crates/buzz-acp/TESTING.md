# Pi adapter integration

Buzz uses the [buzz-pi-acp fork](https://github.com/salman1993/buzz-pi-acp).
Install Node.js 22 or newer, configure Pi, and install the latest development
adapter from the fork's `main` branch:

```sh
npm install -g @earendil-works/pi-coding-agent
pi
npm install -g --install-links=true 'git+https://github.com/salman1993/buzz-pi-acp.git#main'
```

This test setup intentionally tracks `main`; the Desktop runtime catalog pins a
reviewed adapter revision for users.

Make sure `pi` and `buzz-pi-acp` are on PATH, then restart Buzz.

## Tests

```sh
cargo test -p buzz-acp
```

### Provider failures and retries

The ACP client negotiates the Air v1 `sessionFailure` extension. A terminal
failure in a prompt response must become an application error even when the
adapter also returns `stopReason: "end_turn"`. This preserves the existing
bounded queue retries and prevents failed events from being recorded as delivered.
Warnings, successful recovery, legacy responses and explicit cancellation retain
their ordinary behavior. Authentication failures use the existing non-retryable
authentication path.

```sh
cargo test -p buzz-acp --lib provider_failure -- --test-threads=1
```

Metadata and error-classification checks run on every platform. On Unix, the
integration checks use real ACP subprocess pipes and the production dispatch/result
queue. They require no provider credentials or external relay. The queue check
fails once, waits for its actual retry deadline, redelivers the same event in
the same session, and commits delivery only after recovery. Real provider
availability remains a separate check: a successful short request does not
establish the reliability of a long response stream.

Managed agent sessions may already export harness options. Clear them when
running the package suite: CLI parsing tests assert the unset defaults,
and inherited values would change the inputs those tests exercise. Running the
package serially also avoids scheduling flakes in existing short-deadline tests.

```sh
env -u BUZZ_ACP_ALLOWED_RESPOND_TO \
  -u BUZZ_ACP_LAZY_POOL \
  -u BUZZ_ACP_IDLE_POOL_SLEEP \
  -u BUZZ_ACP_SESSION_POLICY \
  cargo test -p buzz-acp -- --test-threads=1
```

Run the ignored real-adapter test with a built fork checkout:

```sh
BUZZ_TEST_PI_ACP=/absolute/buzz-pi-acp/dist/index.js \
  cargo test -p buzz-acp real_pi_preserves -- --ignored
```

## Background process lifecycle

On Unix, the `background` and `process_` test filters exercise real local ACP
adapters and child processes. They cover provider residency, direct and nested
work, bounded process enumeration, queue serialization, retry exhaustion,
session rotation, and completion while a result is being delivered. Marker-controlled
ACP adapters exercise `dispatch_pending` and result handling for collection before
and after new messages, backoff across successful ordinary turns, steer/interrupt,
original Channel/Thread/DM routes, generation replacement and channel revocation.
A retry-exhaustion test drives real ACP failures and captures the signed notice;
it advances queue retry eligibility directly to avoid waiting the full backoff.
They do not require provider credentials or an external relay.

For a human check, build `buzz-acp` and use the existing relay/auth/adapter
configuration with `--lazy-pool --idle-pool-sleep 2 --agents 2`:

1. Ask the agent to start a background task lasting at least ten seconds and
   finish its turn before the task exits. The worker must stay alive beyond the
   two-second idle bound, then collect the result once in the original thread.
2. Send a message after the child exits but before collection starts, while
   collection is running, and during a failed collection's retry backoff. Repeat
   with queue, steer, and interrupt. Ready collection runs first; collection
   backoff permits ordinary messages without resetting its retry budget or
   deadline. An interrupted collection resumes in the original session.
   One ordinary cancel is permitted per collection; after that, ordinary
   Interrupt/OwnerInterrupt and fallback-Steer queue behind its protected retry.
   Native steer also queues during collection rather than extending its deadline.
   Keep sending messages during the retry: collection must finish, then all
   retained messages must reach a foreground turn. Explicit owner/observer
   cancel and owner rotate still stop a protected retry.
3. Under `--session-policy channel`, send the new message in a different thread
   while the child is still running. Its result must still be collected in the
   first thread. Repeat with `--session-policy thread` and a DM.
4. Repeat with `--idle-pool-sleep 0`, and without `--lazy-pool`. Completion must
   still trigger collection. With Claude and no MCP servers, an ordinary turn
   must leave the worker available for other channels.
5. Repeat with `--max-turns-per-session 1`: collection must reuse the original
   session before rotation. In a separate run, send owner `!rotate` while the
   child is running: its exit must not trigger collection in a replacement
   session. Repeat with owner `!cancel` and removal from the channel: no old
   collection may resume. Repeated collection errors must end with a failure
   notice in the original thread and leave the worker available.

The harness holds at most one observed process group's collection per worker.
Ready collections use original arrival age for FIFO among collections and take
precedence over ordinary batches. Their retry counters and deadlines are
separate from ordinary messages. Holds protect original worker/session ownership
until success, terminal failure, or explicit invalidation. This is an in-process
collection guarantee; successful ACP completion alone does not confirm publication.

Process tracking follows children under the worker and resident providers;
detached/reparented processes are outside that topology.

### macOS/Linux Codex service processes and completed slots

The Codex code-mode host may start after `session/new` and stay alive after a
turn. A bounded executable-name lookup recognizes this service, including
installation paths containing spaces. macOS uses `ps comm`; Linux uses `ps exe`
and accepts the kernel's ` (deleted)` suffix for a still-running unlinked binary.
Linux's truncated `comm`, argv-only impersonation and symlinks to unrelated
executables do not establish identity. Failed/unsupported lookups keep the work
held. Its real child processes still protect the worker; other platforms retain
their existing classification.

The `resident_helper_tests` regressions run real local ACP adapters and shell
processes through pool return/claim. They cover six finished threads with five
slots, child exit beneath a live host, and ordinary shell work mentioning the
host path in arguments. They also verify that child exit queues collection in
the original session and retains the slot until collection finishes. Linux-only
cases cover an unlinked helper executable and a symlink to ordinary work. The
fixtures require no provider credentials or relay. Run them on native Linux:
Rosetta may expose its own executable in `/proc/<pid>/exe`, so emulated x86_64
containers on ARM do not attest Linux helper identity.

For native acceptance, run the patched harness with Codex, `--agents 5` and
`--session-policy thread` (or the equivalent desktop settings).
Complete a short task using code-mode tools in each of six distinct threads.
The sixth must start without restarting the agent. Then leave a real background
child running under code mode: its original worker must remain protected until
exit, and collection must still return to the original thread. This check needs
the rebuilt harness; source tests do not update the installed desktop app.

## Saturated workers and late MCP services

Capacity dispatch visits every eligible scope once per pass. An unavailable
scope stays outside the queue until the pass ends, then returns with its event
age and retry budget. Ready background collections retain their precedence.
The `stall_tests` regressions cover a blocked oldest scope followed by the
available owner of a held worker, unchanged retry exhaustion, checked retirement
with retained ordinary input, failed discovery and stale worker/hold results.

The listener launches at most one shared process enumeration on a blocking
thread. Completion is selected separately from incoming events and controls.
Worker instance IDs and hold epochs fence application after claim/return,
replacement or retirement. Failed discovery retains holds. Prompt tasks prepare
return observations before handing workers to the listener; pool return does
not run process enumeration. `background_poll_tests` exercises a blocked
collector on a single-thread runtime while an incoming control is received.

On Unix, Codex stdio MCP launches use the harness's internal transport wrapper.
After a successful MCP initialize, before forwarding its response, the wrapper
registers its service process instances in a private worker-generation directory.
Registrations compare PID, birth stamp and command and require the original
wrapper to remain an ancestor. Registered services remain traversal roots;
subsequent tool children still hold the worker. Resolving an observed process
into the same registered service does not invent a background completion. A
different process instance reusing an old task PID still collects that task.
Completion evidence also survives an intermediate snapshot containing only an
unregistered service: registering that service later still queues the completed
task exactly once in its original session.

Native Codex servers are discovered through the adapter's CLI with bounded time
and output. A controlled Codex stdio proxy preserves their registered launches
when the adapter constructs thread start/resume/fork configuration containing
injected servers. Native environment, cwd and tool settings are retained; secrets
stay in a private launch file instead of discovery arguments. Unsupported or
failed discovery emits a warning and leaves unknown processes protected. These
native hooks are exercised with codex-acp 2.1.1; an adapter changing its CLI or
app-server methods needs its own acceptance run. Windows keeps its existing
process-lifecycle behavior.

Native discovery owns a separate Unix process group. Timeout, excess output,
failed or malformed output, successful discovery with lingering descendants,
and cancellation must retire the discovery processes. The `mcp_discovery`
integration test runs the actual `models` command with a local adapter and
observes both child and grandchild exit; the cancellation test aborts the
production configuration-discovery future. Cleanup has a separate bounded
verification deadline and cannot be accepted as successful discovery on error.
Cancellation signals the group from its drop guard; its regression observes
descendant exit after cancellation rather than claiming an awaited cleanup
acknowledgment. Containment covers the inherited process group used by the
supported adapter and bundled CLI. A descendant deliberately creating a new
session with `setsid` escapes this boundary; these tests do not attest cleanup
of such daemonized processes.

Sprig routes the two internal MCP transport commands when invoked by its target
name, as Linux `current_exe()` resolves the ACP personality symlink. Run the
complete `cargo test --locked -p sprig` package on Linux to exercise both the
registered injected transport and native Codex proxy through the real binary.

Run the package suite normally. The `mcp_service` integration test additionally
requires Node.js and npm on PATH and starts a real `npm start` service with no
package download. It verifies registration before tool calls and exclusion of
real marker-controlled work. A sequential Python stdio server additionally
pipelines two-megabyte requests and responses through the production wrapper;
independent input/output forwarding must keep both pipes progressing while
the initialize response still waits for registration. For configured
Codex/provider acceptance:

```sh
cargo build --locked -p buzz-acp
BUZZ_TEST_BIN_DIR="$PWD/target/debug" \
  cargo test --locked -p buzz-acp --lib real_codex_ -- --ignored --nocapture --test-threads=1
```

Set `BUZZ_TEST_CODEX_ACP` when the adapter is outside PATH. The five-worker check
uses actual provider calls, services through npm and a missed session census.
It verifies a sixth scope and a later original-scope request, a real child held
for ten seconds, and production collection in the original scope. The separate
native check combines configured and injected servers and waits for late MCP
initialization. These checks do not publish to a relay or attest a human app
test. Repeat the native-app workflow above with the candidate harness for that
acceptance boundary.

### Operator launcher with an external Codex CLI

The separate opt-in `real_launcher_wrapper` check requires adapter **2.2.2** and
external Codex **0.164.0**. It uses an isolated recording launcher with the
operator contract: preserve an inherited `CODEX_PATH`, otherwise select the
configured external CLI. It does not modify an installed launcher. Set paths to
the actual adapter entry point and external executable, then run:

```sh
cargo build --locked -p buzz-acp
BUZZ_TEST_BIN_DIR="$PWD/target/debug" \
BUZZ_TEST_CODEX_ADAPTER_222=/path/to/codex-acp-2.2.2/dist/index.js \
BUZZ_TEST_CODEX_EXTERNAL_0164=/path/to/codex-0.164.0 \
  env -u CODEX_PATH cargo test --locked -p buzz-acp --lib real_launcher_wrapper \
  -- --ignored --nocapture --test-threads=1
```

The test checks both wrapper fallback and an explicitly configured CLI. It
observes the real external executable below the live MCP proxy, requires native
and injected npm MCP initialize receipts, and rejects a false worker hold. No
model prompt or relay publication is performed. As a negative control, set
`BUZZ_TEST_LAUNCHER_OVERWRITE=1`: the same test must fail because the launcher
replaces the inherited proxy path.

For the existing five-worker, background-work and resume checks through this
profile, copy `tests/fixtures/codex_launcher.py` into a temporary directory under
the names `codex-acp` and `external-cli`, then run `chmod 700` on both copies. Set
`BUZZ_TEST_CODEX_ACP` to that temporary `codex-acp`,
`BUZZ_TEST_LAUNCHER_ADAPTER` to the 2.2.2 entry point,
`BUZZ_TEST_LAUNCHER_REAL_CLI` to the external 0.164.0 executable,
`BUZZ_TEST_LAUNCHER_FALLBACK` to the temporary `external-cli`, and
`BUZZ_TEST_LAUNCHER_TRACE` to a temporary JSONL receipt file. Unset `CODEX_PATH`
and the negative-control flag, then run the `real_codex_` command above. The
recording fixture execs the selected actual binary; it does not substitute an
app-server implementation. For adapter 2.2.2, identified by its initialize
metadata, the five-worker fixture first uses the local `/mcp` command to require
both discovered tool inventories before each initial model prompt;
`session/new` can acknowledge asynchronous MCP startup. This preflight is
limited to that validated status API profile. Adapter 2.1.1 with bundled Codex
0.159.3 keeps its original provider workflow: its global `/mcp` response lists
injected server names without tool counts and cannot prove readiness. Both
profiles still require five actual ping receipts. These provider checks still
require configured credentials and remain separate from human native-app
acceptance.

Capacity stalls log their scope, queue age, active/held/idle slots, hold reason
and PIDs on transition and at most once per minute per scope. Process observation
logs include only executable basenames, capped at sixteen PIDs; arguments may
contain credentials. Slow or failed snapshots warn at most once per minute.
Scoped `!restart`/`!rotate` remains the explicit checked recovery path for the
old held session. No TTL silently releases or kills real work. Incoming queue
durability across a full harness restart remains outside this change.

## Git bootstrap

The credential fixture requires Git 2.46 or newer for the `authtype` credential
protocol. Ensure the test subprocess PATH selects a compatible Git. Older Apple
Git can complete signing checks but fail `git credential fill` with a username
prompt error; that is a toolchain limitation, not a relay dependency.

`cargo test -p buzz-acp --test git_bootstrap` starts the actual harness with a
probe adapter, runs real signed commits/tags and scoped credential resolution,
and verifies key cleanup on startup failure and SIGTERM. No relay is contacted.

To exercise the real runtime boundaries on Unix:

```sh
cargo build -p buzz-acp -p buzz-agent -p buzz-dev-mcp
BUZZ_TEST_BIN_DIR="$PWD/target/debug" cargo test -p buzz-acp git_runtime_tests -- --ignored --nocapture
```

The Buzz Agent test uses a deterministic local OpenAI-compatible response to
invoke the actual MCP shell. The Goose test requires an installed, configured
Goose and uses its provider to invoke the native developer shell. Both operate
only on temporary local repositories, verify commit/tag signatures and identity,
check unrelated-remote credential scoping, and assert keyfile removal. They do
not replace authenticated relay clone/push/readback testing.

## Managed background final reports

A `RuntimeContinuation` receives a random delivery UUID when its background
origin is recorded. Queue retries, ordinary message interleaving and healthy
steer/interrupt preserve it. A fresh observed group receives a fresh UUID. Only
collection prompts supply the key, original channel/parent and an explicitly
quoted workspace `.buzz-deliveries` path in `buzz messages deliver`. Progress
uses ordinary `messages send`; corrections use `messages edit`. Callback
mentions can use `--mention` or ordinary member-name mentions. Pending managed
publication must retry the same key/route/journal, including after ACP
application errors; changing the summary returns the first signed report.

```sh
cargo build -p buzz-cli -p buzz-agent -p buzz-dev-mcp
BUZZ_TEST_BIN_DIR="$PWD/target/debug" \
  env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL \
  -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY \
  cargo test -p buzz-acp managed_runtime_ -- --ignored --test-threads=1
```

These four checks exercise actual CLI processes and actual bundled agent + MCP
shell, production collection dispatch and error handling, a model error after
publication, same-key replay with changed text, lost response bodies, process
kill after relay acceptance, concurrent callers and a workspace name containing
spaces/apostrophes. Readback counts unique verified signed events in a **local
mock relay** that follows the real relay's duplicate acknowledgment contract.
They do not use a real authenticated relay/provider or attest human testing.
The bundled-agent fixture seeds a preserved continuation in the original ACP
session; the separate background lifecycle tests exercise actual child groups.

The persistent CLI journal recovers an already prepared signed event when the
same key/route is explicitly invoked after restart. ACP's queue, provider
session, group observation and ownership token remain in-memory; harness death
does not reconstruct an unjournaled lost result or automatically replay the
journal. Neither a successful ACP prompt nor arbitrary shell/MCP publication
proves managed delivery. The managed path covers kind-9 channel/thread reports,
including DM channel membership, only when the provider calls the supplied
command. See `crates/buzz-cli/TESTING.md` for bounded attempts, seven-day confirmed
retention, capacity/corruption handling and filesystem durability limits.

For human smoke testing with the native app and an authorized local relay,
start a task with a real background child. Send another-thread traffic while it
runs and while collection backs off. Make collection publish through the
supplied command, then cause an ACP application error. Let the same session
retry with a changed summary: the app and CLI readback must show one original
report/event ID, in the original thread, with its callback mention. A second
background group must produce a second report/key. Explicit cancel/rotate and
membership removal still invalidate collection according to the lifecycle
runbook above. Agent fixtures cannot mark this human check complete.

## Restart status

The `restart_status_tests` filter covers authorized admission, checked retirement,
the typed scheduler, fake ACP processes, selected history and a signature-checking
HTTP relay. It covers context depths, conversation-language hints, reply routing,
queued input, duplicates, bounded failures, cancellation and crash uncertainty.
See [SESSION_CONTROLS.md](SESSION_CONTROLS.md) for the contract and its limits.
Use the full package suite before accepting the change; a focused filter is an
iteration aid, not a substitute for package validation.

For each supported real adapter, use a disposable session and the actual build
under review:

1. Start an English assignment, then discuss progress in Russian. Restart with
   None, Recent and All. Each successful reset should produce one useful Russian
   status without another mention. With None, omitted progress must be reported
   as unknown. Repeat an English conversation and expect English.
2. Repeat while the old worker is busy. Wait for checked reset completion; no
   status may run before that boundary. Queue a real new instruction during the
   status turn: it must run afterward in its original form. A sibling thread
   must continue independently.
3. Cancel, remove the agent, or issue a newer Restart during status generation.
   Late output must not begin a newly authorized send. A send already submitted
   may still become visible; lost acknowledgment remains unconfirmed. Inspect
   adapter activity and verify that no task tool or subagent executed.
4. Check that the reply is signed by the selected agent, in the canonical thread
   or DM, with no mention/broadcast tags. Repeat the same signed Restart and
   confirm it does not create another model turn or message.
5. Send an ordinary follow-up after a confirmed status. It should reuse the new
   session while ordinary provider-resume and rotation limits still apply.
   Simulate provider failure and loss of acknowledgment separately: the reset
   acknowledgment stays successful, and failure/uncertainty is reported honestly.
6. Restart the disposable harness with a pending or prepared follow-up. Recovery
   must preserve uncertainty and prepared bytes without invoking the model or
   automatically posting that old reply. Later ordinary input remains available.

Record build identity, adapter, selected depth, actual language, signed reply
provenance and observed tool activity. Browser fixtures alone cannot certify
provider behavior. Human acceptance remains separate from agent validation.

## Restart history acquisition

The restart-status HTTP/ACP journey covers a history response taking longer
than three seconds, failed original-assignment discovery after a successful
recent window, complete history failure without dispatching an empty model
prompt, and a fresh authorized restart after the relay recovers. Notices must
be signed in the original thread. The recent window reuses one bounded newest
page; forward assignment discovery stops once it finds the first mention.
The newest page also preserves human language evidence when agent replies push
it outside the rendered window. Later pages and all-history restarts retain
the bounded full-page scan.

Acquisition shares a ten-second deadline, a raw-candidate/request cap and a
response-byte ceiling. A failed page is retried at most once within that budget.
Partial failure keeps already acquired permitted messages and marks coverage
incomplete. Complete failure reports that history could not be loaded and
leaves the provider session available for the next ordinary turn. The test
uses real local HTTP responses and ACP subprocesses, without contacting a
provider or changing an installed application.
