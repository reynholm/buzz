# Background result delivery implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Preserve background collection when new messages arrive (A), then provide durable idempotent final-report publication (B).

**Architecture:** Collection obligations remain separate from ordinary queue batches, retain their original session and route, and have independent bounded retries. A managed CLI publisher journals one signed event per stable completion key before sending it and reuses it across retries and restarts. A succeeds independently of B.

**Tech Stack:** Rust, Tokio, existing ACP queue/pool, Nostr SDK and CLI HTTP transport, filesystem journal and advisory locks.

**Spec:** `docs/superpowers/specs/2026-10-14-background-result-delivery-task.md`

## Global Constraints

- Implement A first, independently reviewable; B is a separate change.
- Read root AGENTS.md, VISION.md, VISION_AGENT.md, VISION_REMOTE_AGENTS.md, TESTING.md and affected package TESTING.md before editing.
- No GitHub publication, PR or push. Work remains local.
- No unsafe or new production unwrap/expect; document public API.
- Preserve explicit cancel/rotate and membership revocation; never deliver a revoked channel's report elsewhere.
- Do not change detached/reparented child tracking or introduce unrelated refactors.
- Clean ACP environment variables (`BUZZ_ACP_ALLOWED_RESPOND_TO`, `BUZZ_ACP_LAZY_POOL`, `BUZZ_ACP_IDLE_POOL_SLEEP`, `BUZZ_ACP_SESSION_POLICY`) and run the complete affected package suite serially, plus fmt and clippy. The fourth variable was discovered as inherited input contamination during baseline.
- Activate Hermit; prepend `/Users/reynholm/.local/bin` to PATH to use the owner's identity guard. Before any commit verify user.name is global Andrey Bruhanov, user.email matches global configuration, and commit.gpgsign is empty; commit with `-s`.

## Review Focus

- New traffic during retry backoff cannot erase delivery or borrow its retry budget (Task 1).
- Channel-scoped messages from another thread cannot reroute a live child's result (Task 1).
- Interrupt cancellation and automatic rotation after child exit cannot invalidate pending collection (Task 1).
- Two concurrent publisher processes or response loss cannot construct distinct signed events for one key (Task 2).
- Revoked access, corrupt/full journal and expiry cannot silently drop a pending publication or route it elsewhere (Task 2).

---

### Task 1: Preserve and schedule original background collection (A)

**Files:** Modify `crates/buzz-acp/src/queue.rs`, `queue/background.rs`, `pool/background_work.rs`, `pool.rs`, `lib.rs`; extend `pool/background_work_tests.rs`, queue tests and `crates/buzz-acp/TESTING.md` as needed. Keep auxiliary changes within ACP.

**Interfaces:** Consumes existing RuntimeContinuation ownership/session/generation/thread tags. Produces preserved continuation lifecycle used unchanged by B; report exact changed signatures. Retain FIFO by ready age for continuations before ordinary messages where practical; ordinary messages may run during collection backoff. Independent continuation retry counts/deadlines must survive message completion. Preserve original RuntimeHold across ordinary turns. Hold the original session until collection succeeds or fails terminally; explicit control actions invalidate it.

- [x] Write falsifiable production-seam regressions: pending-before-message, message-before-child-exit, backoff-survives-message, interrupted-collection-resumes, live-child-route-survives-other-thread, max-turns=1-pending-collection-protects-session, generation/revocation clears work, and finite terminal retries. Exercise `dispatch_pending` with real local ACP child processes controlled by marker files. Cover queue/steer/interrupt, Channel/Thread/DM and sleep modes using existing test infrastructure.
- [x] Run new regressions and record expected missing-behavior failures before implementation. Also establish complete package baseline.
- [x] Implement minimal queue and ownership lifecycle changes. Separate ordinary message retry state from continuation retry state. Do not silently supersede on push/requeue/requeue_as_cancelled. Ready collection wins over continuous new traffic, while backoff permits message dispatch. Terminal collection failure remains visible through the existing notice path.
- [x] Run `env -u BUZZ_ACP_ALLOWED_RESPOND_TO -u BUZZ_ACP_LAZY_POOL -u BUZZ_ACP_IDLE_POOL_SLEEP -u BUZZ_ACP_SESSION_POLICY cargo test -p buzz-acp -- --test-threads=1`, `cargo fmt --all -- --check`, and `cargo clippy -p buzz-acp --all-targets -- -D warnings`. Record exact HEAD and outcomes, real workflow limitations, and human reproduction steps.
- [x] Self-review and commit only A files with verified guarded identity and DCO. Write implementation report to supplied report path; do not spawn reviewers or helpers.

### Task 2: Durable managed final-report delivery (B)

**Files:** Add focused journal/publisher module under `crates/buzz-cli/src/commands/`; wire `lib.rs`, `commands/mod.rs`, `commands/messages.rs`, `Cargo.toml` and lockfile if required. Add stable key to ACP `RuntimeContinuation` and prompt construction, tests and package guides. Add production-seam CLI and ACP/bundled-agent integration regressions.

**Interfaces:** Consumes A's preserved continuation. Produces a stable random completion delivery ID allocated once when background group origin is recorded, retained through queue/prompt retries. Managed command: `buzz messages deliver --delivery-key KEY --channel UUID --reply-to EVENT --content TEXT`, with an explicit optional journal directory for tests/operators and ordinary explicit/automatic mention support for callback notifications. Existing `messages send` stays independent for progress; `messages edit` stays independent for corrections. Prompt supplies this final-report command and the original route. Default journal is persistent under agent's Buzz workspace; document override and actual coverage. Bind the delivery key into the signed event (a tag), so two keys with identical content in the same second produce two distinct event IDs.

**Delivery contract (describe in report before implementation):** Identity namespaces include normalized relay URL, signer pubkey and random group key; entry binds original channel/reply destination. Atomically journal the first signed event before publish, then lock and replay the same event. Changed content for the same key returns the original delivery; changed route conflicts. Check current channel membership before every publication/ack replay and use ordinary Nostr `/events` publication. Journal retains unconfirmed signed events, bounded to 1024 records and 8 publication attempts; terminal failure is explicit and retains evidence. Confirmed entries may expire after 7 days; idempotency is guaranteed only while retained. Unconfirmed entries never silently expire to permit re-signing. Reject at capacity rather than evict pending data. Lock files do not grow per key without bound. Validate saved events and file sizes; atomic replacement and fsync cover restart between relay acceptance and local acknowledgment. No general exactly-once claim for bypassing shell/MCP publishers.

- [x] Write production publisher regressions and observe RED: changed text same key yields one event, equal text distinct keys yields two, concurrent attempts one event, body-loss retry preserves bytes, restart after relay acceptance reuses event, progress/edit unaffected, revoked channel refuses, bounds/corruption fail visibly.
- [x] Implement managed command and journal. Reuse `BuzzClient::sign_event` and `submit_event`; do not add relay endpoints. Use fs2 locking (already used by bundled agent) or equally robust existing facilities; cap journal/record bytes and attempts. Validate `accepted` and event identity instead of treating arbitrary successful HTTP as acknowledgment.
- [x] Wire stable completion key and managed final command into ACP continuation prompt after A's task gate. Ensure unrelated messages never receive/reuse its key. Add a deterministic local model/ACP flow that invokes the actual production publisher, then returns application error, and retries with a single visible signed event in local mock-relay readback. Clearly distinguish mock from real relay evidence.
- [x] Run complete `buzz-cli`, `buzz-acp` and any modified `buzz-agent` package suites, fmt and clippy. Run `just ci` only if preparing a PR (no PR currently authorized). Provide precise human smoke-test commands and evidence limits.
- [x] Self-review and commit B separately with guarded identity/DCO. Write report and return commit IDs, outcomes and limitations. Do not spawn helpers or reviewers.


## Current handoff

A: `cabc051c3a4ad7343db3e1d87df6740362447540`.
B: `33966afed44c35067f344a503fdf02f1d12d7489`, followed by B1 durability fix `c07ce95adc6b82dc87aef28a4d67c16b43ae9460`.
All local implementation checks completed; final whole-branch Astride24 review-master review is pending. No GitHub publication or thread completion marker.
