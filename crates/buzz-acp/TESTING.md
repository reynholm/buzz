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
