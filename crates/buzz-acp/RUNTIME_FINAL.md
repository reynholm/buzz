# Opt-in runtime final replies

Runtime delivery publishes the terminal user-facing text of an accepted request
through the existing `buzz messages deliver` journal. The default remains
`agent_tools`. A delivered answer, including a clarification question, confirms
publication only; it does not certify completion of the user's task.

Runtime delivery is enabled by an explicit operator opt-in and a negotiated ACP
contract, independently of the adapter's product name or version:

```sh
buzz-acp --agent-command <adapter> --reply-delivery runtime_final \
  <existing relay, identity and agent settings>
```

`BUZZ_ACP_REPLY_DELIVERY=runtime_final` selects the same mode. The optional
`--reply-profile runtime_final_v1` names the protocol, not a product release.
The old `b24_0_26_7_8` profile spelling and `--reply-build-file` option remain
accepted for configuration compatibility. They confer no support or trust:
VERSION files are not read, and an old profile cannot bypass negotiation.

Only opted-in connections send
`initialize.params.clientCapabilities._meta.buzz.replyDelivery` equal to
`{"version":1,"mode":"runtime_final"}`. The adapter must acknowledge the same
values in `initialize.result._meta.buzz.replyDelivery`. Missing fields, other
versions or modes, and mistyped values fail closed before new session/model/tool
execution; there is no silent fallback. Every replacement process negotiates
again. `agentInfo` is diagnostic and never selects delivery support.

Negotiation declares support without activating every prompt. Each actual
runtime-final `session/prompt` carries the same object under
`_meta.buzz.replyDelivery`, alongside the mode instruction. Setup, resume and
collection use this explicit mode; ordinary tool-delivery and heartbeat prompts
do not carry it. A capability response on a default tool-delivery connection
cannot enable runtime capture. This metadata is not an authorization grant.

Protocol v1 requires terminal provider failures to be observable as JSON-RPC
errors or the supported Air `sessionFailure` extension. Partial text followed by
an error, cancellation or a non-normal stop is never a successful final answer.
The adapter must honor the main-session/turn final contract and mode instruction,
including suppression of duplicate manual final publication. An adapter must not
advertise this contract until it implements those behaviors. Native providers
without the capability keep their existing tool-delivery behavior by default.
Choosing an adapter binary remains an operator trust decision; a self-declared
capability is a compatibility contract, not binary attestation or authorization.

Local `buzz-acp run` tasks have no conversation publication route and do not
support `runtime_final`. Selecting it by flag or environment returns the terminal
error `runtime_final_requires_conversation` with exit code 2 before configuration
file reads or adapter launch. Default local tasks keep their existing terminal
record and tool-delivery instructions.

Saved journal replay is independent of adapter availability and negotiation.
New journals use the stable `runtime_final_v1` profile. Existing runtime records
with the old profile remain readable and retain their signed bytes, route,
signer, delivery key, receipts and revocation fences.

The capture is bounded to the kind-9 limit of 64 KiB and the current prompt/session. Main tools and
thoughts clear earlier commentary. Child-agent text and tool events, synthetic
sources, thought text and tool output are excluded. Empty success-lifecycle and
usage chunks preserve a preceding final. Missing, malformed, oversized or
ambiguous terminal text produces a reasoned failure notice under the same
managed key. Local diagnostics distinguish `missing_final`, `answer_confirmed`,
`failure_notice_confirmed`, `delivery_pending` and `cancelled`.

Runtime prose is literal content: `@Name` and `nostr:npub` text do not add
recipients or trigger name resolution. The harness supplies structured callbacks
only for original accepted request authors, including the verified original
callback list carried by an existing collection. Authors are deduplicated,
most recent first, up to the SDK's 50-recipient cap; older excess authors are
omitted. Current channel membership still applies. Callbacks enter the first
signed event and remain unchanged during retry/restart. Explicit `--mention`
remains an authorized CLI tool input; free-form model prose cannot supply it.

Each accepted runtime request gets its own publication UUID and original route.
Steer, interrupt and a busy model switch retain the accepted batch and its
publication identity for the existing authorized continuation. Explicit cancel,
rotate and provider cancellation terminate that turn without publishing partial
text. Heartbeats have no accepted publication route: they retain tool delivery
instructions and disable runtime capture and runtime delivery metadata for that turn.
This does not enable Kimi permission watchdogs, total-work deadlines or
provider/task retries for B24. An ended turn can deliver a clarification while
an existing background child is pending. Finishing that receipt does not revoke
the ordinary B24 collector: it retains its existing independent collection key,
and its later final is a separate delivery. Collecting an existing result is
ordinary execution, not a provider replay to recover publication.
The current contract is delivered in prompts and persisted separately from the
session store's CAS revision after successful delivery to the provider. Legacy
or mismatched durable and cached sessions are replaced with a fresh session and
the accepted request's available context. The old provider transcript is not
claimed to have been reconstructed.

## Recovery and rollback

Use the rebuilt `buzz` CLI beside the candidate harness, or put that CLI first on
PATH. The CLI is the sole signer and locked journal writer. Runtime purpose,
profile, contract revision, original session scope and revocation are saved atomically with the original
signed event before submission, using journal version 3 in the separate `.buzz-runtime-deliveries` directory.
Legacy `.buzz-deliveries` stays unchanged for other profiles and installed older
CLI/harness readers. This uses the same signing, locking and publication
implementation; it does not introduce another delivery authority.
Version 1/2 journals
remain readable. Pending records retain original text, key and route.

On startup in runtime mode, one delivery-only pass runs before provider startup:
at most 32 eligible records, at most 60 seconds overall, with the existing eight
submissions per record. It checks current membership and never invokes a
provider/session prompt or replays tools. Within the existing freshness window
it resubmits the same signed bytes. Older events use the CLI's existing signed
recovery envelope and relay canonical election. Exhausted, malformed, revoked or
inaccessible records stay retained; they are never redirected to another scope.

Inspect and retry an existing publication explicitly:

```sh
buzz messages delivery-pending --journal-dir /absolute/workspace/.buzz-runtime-deliveries
buzz messages delivery-retry --delivery-key <saved-uuid> \
  --journal-dir /absolute/workspace/.buzz-runtime-deliveries
```

Owner `!cancel` awaits revocation of matching pending publications through the
same journal API, including after a failed send has returned and execution
ownership has been dropped. This retains no provider task or execution replay.
New records distinguish conversation scope from thread scope even when their
reply parents match. Scope-less earlier v3 records retain conservative original
channel/parent cancellation matching. A failed journal operation is reported as
unconfirmed; it never certifies durable cancellation.
`delivery-revoke` is also available for operator cancellation;
it does not require current channel membership and does not erase evidence.
An already accepted event cannot be unsent by revoking a later retry.

For mode rollback, keep the candidate-compatible harness and CLI, set
`--reply-delivery agent_tools`, and remove `--reply-profile` and
`--reply-build-file` (and their environment equivalents). Keep `.buzz-runtime-deliveries`, legacy `.buzz-deliveries`
and `.buzz-sessions`. Contract mismatch forces fresh manual-delivery sessions.
This mode does not automatically replay runtime entries; use the explicit saved
key retry when wanted. Older CLIs with strict v2 readers cannot read a v3
journal; default directory isolation keeps them on their unchanged v2 journal. Keep a v3-compatible CLI/harness available for recovery when rolling
back binaries; do not delete the journal, downgrade it by hand, invent a new key
or rerun the original task to deliver an existing reply.

The runtime cache contains private signed reply text. This repository ignores
`.buzz-runtime-deliveries/`. When running in another Git checkout, add that
pattern to its `.gitignore` or private `.git/info/exclude` before use; never stage
or publish the cache. Preserve it across restart and rollback. Explicit CLI
`--journal-dir`/`BUZZ_DELIVERY_JOURNAL_DIR` overrides choose the schema destination;
do not point runtime writes at a shared legacy directory during a mixed-version
rollout. Runtime pending/retry/revoke commands default to the runtime directory.

## Evidence and acceptance

The automated matrix uses synthetic ACP subprocesses, the rebuilt original Buzz
CLI, real signed events and a local signature-checking mock relay. It covers
plain/tool-to-final text, thought and child boundaries, missing finals,
error/cancel after partial text, clarification delivery, same-byte ACK-loss and
startup replay, parallel scopes, confirmed suppression, persisted revocation,
clarification while a real child waits and subsequent independent collection,
legacy/cached contract replacement and unsupported build checks. See
[TESTING.md](TESTING.md) for commands. These tests do not attest an installed
candidate, a real production relay, provider failure reproduction, or human
native acceptance. Native checks must record the exact candidate binaries and
unchanged adapter build, exercise the supported matrix, and inspect the original
thread and local delivery diagnostics separately. No native runtime acceptance
is claimed by the source tests.
