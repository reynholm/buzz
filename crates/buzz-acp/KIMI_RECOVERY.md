# Kimi logical-work recovery

The managed Kimi profile extends the existing ACP client, queue and checked
worker retirement. It adds no persistent proxy and never changes Kimi source,
OAuth credentials or refresh locks. Queue recovery is in memory: child-process
death is covered while the harness lives; restarting the harness does not
reconstruct its outstanding work from the delivery journal.

## Work and retry limits

The profile is enabled for a configured executable whose normalized name is
`kimi`. The ACP decoder also recognizes the exact initialize name `Kimi Code
CLI` for safe error and permission handling; that metadata alone does not
change the configured queue policy. Other adapters keep their existing rules.

One logical work owns its original events, scope, reply route, random delivery
UUID, journal directory, cancellation token and wall-clock deadline. ACP
request/session IDs and process generations belong to individual executions.
A retry cannot start before the previous execution returns or checked
retirement confirms it stopped. An unverified stop retains the old worker and
blocks that slot; other available slots/scopes continue independently.

| Flag | Environment variable suffix after `BUZZ_ACP_` | Default |
| --- | --- | --- |
| `--kimi-recovery-retries` | `KIMI_RECOVERY_RETRIES` | 4 retries after the initial attempt |
| `--kimi-recovery-delays` | `KIMI_RECOVERY_DELAYS` | `5,15,45,120` seconds |
| `--kimi-recovery-budget` | `KIMI_RECOVERY_BUDGET` | 1800 seconds |
| `--kimi-approval-decision-timeout` | `KIMI_APPROVAL_DECISION_TIMEOUT` | 120 seconds |
| `--kimi-approval-progress-timeout` | `KIMI_APPROVAL_PROGRESS_TIMEOUT` | 120 seconds |

Each retry independently multiplies its base delay by a uniform draw in the
inclusive range 0.8–1.2. The default maximum delay is 144 seconds and the sum of
default backoffs is at most 222 seconds. OS-random UUID bits supply independent
process draws; coinciding delays remain possible. Tests inject unit draws and
clock deadlines without waiting through production backoffs. Configured
retries are limited to 16, each base delay to 600 seconds (720 after jitter),
and the work budget to 86400 seconds. A shorter delay list repeats its last
entry. Zero retries disables retries.

The work budget includes startup/context acquisition, all attempts, approval,
background waiting and backoff. Every turn also retains its configured idle
and hard deadlines; traffic and steer do not extend the Kimi hard deadline.
The number of configured retries is a ceiling, not a promise that all fit the
remaining time. Expiry does not authorize overlapping execution. Checked
process cleanup and terminal publication have their own bounded deadlines
outside the work budget: the existing Unix retirement uses bounded snapshots,
TERM after 3 seconds, KILL at 15 seconds and up to 10 seconds of verification.
A snapshot has a 2-second bound; failed initial acquisition can make three
attempts. Terminal CLI publication is limited to 30 seconds plus a 2-second
kill wait; each journal snapshot read is limited to 2 seconds.

Fresh traffic does not reset the original budget or delivery route. Queue mode
waits. Same-route steer/interrupt preserves the original work and incorporates
the new input using the existing merge framing; after one ordinary preemption
the work receives a protected execution window. Under channel policy, traffic
from another thread queues instead of redirecting the original report.
Explicit cancel, access revocation and forced session reset revoke the work
including backoff, pending approval, respawn and terminal publication. A send
already submitted can still have reached the relay; cancellation cannot undo
an accepted event.

## Failures and uncertain effects

The JSON-RPC decoder preserves only safe error name/type, numeric code/status
and a classification. Authentication evidence (401/403, revoked credentials,
login or reauthentication) overrides timeout similarity. Only recognized
OAuth connection/timeout, the exact OAuth refresh-lock message and recognized
API/network timeout types authorize the Kimi application-error retry. Generic
OAuth, auth, config/model and unknown application errors terminate. Existing
transport failures have their own bounded recovery.

The existing negotiated Air v1 `sessionFailure` result seam receives the same
Kimi classification directly from its title and access/login fields. It is
not represented as an invented JSON-RPC payload. Other adapters retain the
previous Air translation. Unsupported/malformed metadata fails closed through
the existing protocol path. Kimi error diagnostics redact raw error text in
both envelopes.

Arbitrary shell/MCP actions have no exactly-once guarantee. Before granting an
unknown/mutating action, the work records a possible external effect. A matching
`completed` tool status settles that pending result; `failed` may mean a
partial effect and remains uncertain. Declared read/search actions are exempt.
On failure, an unresolved action stops automatic replay in `waiting_human`.
After observed effects, falling back to a new session without resumed context
also requires human reconciliation. Missing/unknown background results and
unverified retirement are similarly explicit reasons to inspect the existing
result. A model prompt instructs the agent to verify an uncertain action before
repeating it, but this is not a general transaction log for provider tools.

## Permission lifecycle

Each received request is tied to its actual JSON-RPC ID, active session/prompt,
process generation, tool-call ID and random decision token. The diagnostic
stages distinguish received, decision, response written and correlated progress.
Writing a pipe is not evidence that Kimi consumed the decision. Duplicate IDs,
wrong sessions, retired mailboxes and late replies never authorize another
request. Selected option IDs always come from the offered request.

`bypassPermissions` explicitly selects an offered `allow_once`; it is not proof
that Kimi accepted an adapter mode. `dontAsk`/`plan` select rejection.
`acceptEdits` automatically allows only a declared edit; other actions and
`default`/`auto` use the owner decision path. This does not introduce a new
Desktop permissions dialog or silently turn timeout into approval.

An interactive request posts a signed notice in the original thread and appears
in owner-scoped activity. Inspect the actual tool request there, then send
`!approve <request-token> @Agent Name` or `!deny <request-token> @Agent Name`
in that same thread. The command must come first, must be signed by the owner
and must mention the agent. The current prompt/scope/control generation must
still be valid. The display status does not confer decision authority. A missing
listener, failed notice or expired decision fails closed and requires explicit
human follow-up; an old token cannot revive a cancelled request.

Decision waiting is bounded by the configured timeout and remaining turn/work
limits. Permission writes have at most 30 seconds and are also capped by those
limits. After a written reply, a separate progress watchdog requires correlated
tool progress or a prompt outcome, not unrelated service chatter. The initial
120-second progress bound is conservative relative to one observed successful
4.169-second reply-to-end-turn exchange; it is not an incident-derived timeout.
Idle-pool sleep remains blocked by an active turn, background hold or pending
continuation/retry. This does not disable the turn/work watchdogs.

## Reports and visible states

Every attempt receives the same `buzz messages deliver` key, original route and
explicit journal directory. The CLI owns locking, signed event preparation,
readback and confirmation. The harness only reads bounded journal snapshots
and verifies the signature, namespace, route and canonical receipt. A confirmed
report prevents another provider attempt and wins over a later model failure.
A terminal harness notice invokes the same managed command, so a previously
prepared report remains the first report. An unreadable receipt never becomes
an invented success. This guarantee concerns managed reports only.

Owner activity exposes `running`, `retrying`, `waiting_approval`,
`waiting_background`, `failed`, `waiting_human` and confirmed `completed`.
Safe structural diagnostics connect work/key, scope, attempt, request/session,
process generation, next delay, permission stages and delivery confirmation.
The Desktop renders known reason labels and rejects stale state sequences;
raw error text is not a display reason or control authority.

## Verification and acceptance

Run the full packages with inherited managed settings cleared:

```sh
. ./bin/activate-hermit
cargo build --locked -p buzz-acp -p buzz-cli -p buzz-agent -p buzz-dev-mcp
BUZZ_TEST_BIN_DIR="$PWD/target/debug" \
  env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL \
  -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY \
  cargo test --locked -p buzz-acp -- --test-threads=1
cargo test --locked -p buzz-cli
BUZZ_TEST_BIN_DIR="$PWD/target/debug" \
  cargo test --locked -p buzz-acp managed_runtime_ -- --ignored --test-threads=1
BUZZ_TEST_BIN_DIR="$PWD/target/debug" \
  cargo test --locked -p buzz-acp kimi_managed_terminal_preserves_pending_success \
  -- --ignored --test-threads=1
just ci
```

Record HEAD, dirty source digest and actual binary hashes before/after package
validation; do not change sources or rebuild the target concurrently. The
production-pipe/queue regressions cover error classification, request fences,
deadlines, failed writes, stalled progress, checked crash collection, explicit
controls, partial effects and managed report replay. Their deterministic peers
and mock relay are simulations, not provider/canary evidence.

The normalized permission fixture records an actual successful Kimi 2.3.1
exchange. The historical incident trace is unavailable; OAuth errors, Air
failure results and injected permission faults in regressions are synthetic.
Direct child-only network probes produced no ACP result/error within their
bounded observation and do not establish an OAuth payload or an infinite hang.
See [test-fixtures/kimi/README.md](test-fixtures/kimi/README.md).

Before acceptance, use one explicitly selected disposable managed Kimi agent
with the exact candidate harness/CLI and unchanged installed Kimi. Record its
version/hashes, original assignment/thread/key, safe activity and receipt. Run a
real background completion/collection/approval, a controlled child-only network
fault and child death during collection/approval. Verify bounded recovery or
an honest uncertain-effect outcome, one final report, and cancellation/access
revocation. Do not alter credentials/locks, substitute installed runtime for an
unrecorded candidate, or restart other agents. In the real client, check the
interactive notice and owner decision, wrong/late/cancelled tokens, timeout and
absence of unintended approval. Separate these provider and human results from
simulation results; an agent cannot certify human acceptance.

### Observed descendants during crash retirement

Native background enumeration carries process-group and birth evidence from the
same bounded `ps` snapshot as classification. Kimi retains a bounded graph of
owned descendants, including identity-checked orphan anchors seen before the
adapter exited. Missing proof or a live child's changed command with unchanged
birth yields `Unverified`; an `exec` is not proof of process exit. A different
recorded birth prevents signalling a reused PID. Retention above 4096 anchors
also fails closed. These ownership checks protect the checked-stop boundary;
they do not promise containment of descendants deliberately escaping observation.

An application error immediately after a successful background launch preserves
the collection origin and waits for the existing child. If terminal failure,
retry exhaustion or the work deadline prevents collection, the result is marked
for human reconciliation before finishing work or retiring the worker. A
confirmed managed report still takes precedence over a fallback notice.

### Plaintext diagnostics and the owner request view

Kimi plaintext wire diagnostics retain only bounded structural correlation and
safe failure classes. Prompt, MCP configuration, tool input/output, arbitrary
titles and message/thought text are not copied into harness tracing. The
initialize reply is projected even when its adapter identity is learned there.
Other adapters retain their existing diagnostics.

Owner activity is a different boundary: its raw ACP frames are encrypted to the
owner and intentionally retain the concrete permission request for inspection.
Interactive notices direct the owner to `session/request_permission` in Activity
→ Raw ACP and name the request/tool IDs. This projection does not sanitize the
adapter's inherited stderr; provider evidence must use reviewed, redacted
excerpts rather than treating arbitrary third-party stderr as safe telemetry.

Native steer requests use the same Kimi plaintext projection. Provider error
text and unknown outcome payloads are reduced before they enter the main-loop
acknowledgement log; error codes, acknowledgement variants and fallback rules
remain unchanged. Interactive policy requires owner activity: without that view
the request is cancelled, no vote is offered or accepted, and work asks the
owner to enable `--relay-observer` before retrying. Automatic policy decisions
retain their explicit allow/reject behavior without requiring an activity view.
