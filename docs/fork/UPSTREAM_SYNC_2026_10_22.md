# Upstream snapshot integration

The candidate integrates upstream main at `2d289f720de0012fa36b5f6faa0e3bdd845ccfcf`
into fork `cd40d18f19e5f44c2660fd64248be98065eb59a6`. Its version floor is
`desktop-v0.5.28`; the base SHA is a later main snapshot, not that release tag.
The patch registry records the exact snapshot separately from the release floor.
This document specifies integration and rollout constraints; it is not release
acceptance evidence. Existing artifacts from either parent do not validate this tree.

## Preserved and adopted behavior

- Device authority, exact host proof, private-by-default ownership, atomic home
  claims, shared instances and release/tombstone rules remain enforced before
  side effects and after asynchronous creation work. Claude effort is applied
  inside the protected record commit path.
- Top-level direct messages may steer an ordinary active turn. Agent-only
  replies gain exact trigger anchors. Human-facing replies retain thread roots.
  Background collection still rejects native steering, permits at most one
  ordinary interruption, then protects its retry and original route.
- Session controls, child process ownership and idle-slot release remain fork
  contracts. CLI journal recovery after explicit retry is distinct from
  recovering a lost in-memory ACP queue after harness process death.
- CLI output remains full JSON by default. Mobile push preferences use the new
  upstream opt-in key, including resetting previous OFF settings to ON. A new
  OFF choice persists normally. Announce guests remain read-only; reporting
  agents require an explicit role with write permission.
- Bundled object storage retains MinIO/mc. Git attribution follows repository
  policy, and immutable dependency security pins remain part of the fork.

## Protocol and database rollout

Owner presence uses kind 20003. New CLI recovery uses kind 20004. Updated relays
also route legacy JSON recovery envelopes at kind 20003 through signature and
recovery validation; plaintext presence remains WebSocket-only. The JSON hint
is dispatch classification, never authorization. Deploy compatible relays
before new CLIs; new kind 20004 recovery is not supported by old relays.

Applied fork migration 0056 is checksum-frozen. Upstream migrations 0056–0068
are renumbered to 0057–0069 without changing SQL contents. These migration histories target fork databases and fresh installs.
An already-upgraded independent upstream database has a different SQLx history
and requires a separately designed migration; do not relabel its applied rows.

Keep the existing managed ledger in migrations, desired-state includes,
write fences and deletion inventories. Desired-state setup and repeated schema
apply require the reconcile script and `.pgschemaignore`. Verify populated
ledger recovery, catalog admission, and runtime partition data after apply.
Reconciliation also normalizes the known pgschema 1.7.4 events-PK permutation
through the parent after partition attachment. The declared tenant-leading
order is retained; unexpected key shapes fail rather than being rewritten.
This prevents repeat apply from dropping inherited child constraints.

Eligible event inserts persist work in the existing `push_match_queue` inside
the admitted event transaction. Queue write failures reject that transaction;
process shutdown after commit leaves durable work for the replacement matcher.
There is no new tenant outbox table or memory handoff. This follows the current
no-new-fences/functions/foreign-keys rule in [database architecture](../database.md).
See [push deployment](../push-gateway-deployment.md).
Drain old writers before the trigger retirement and deploy the full compatible
schema/writer pair. Preserve the qualified pair on rollback. Back up and test
upgrade on a copy before changing an existing database.

## Deferred work

1. **CLI default agent format:** audit scripts, workflows and nested commands for
   signature/raw-tag use; change the default only after compatibility validation.
   The format remains available through explicit opt-in.
2. **NIP-AP runtime:** design editor authorization, instruction precedence,
   offline cache, config-channel archive/deletion, and compatibility with host
   authority. This integration adopts specification documents only.
3. **MinIO to Silo:** a separate migration with backup/restore, existing attachment
   reads and writes, data-copy validation and rollback rehearsal. Keep the old
   images and volumes until that work is accepted.

## Acceptance

Run the full repository CI, desktop typecheck, complete touched package suites,
PostgreSQL integration suites and fork registry validation. Fault injection must
cover durable enqueue saturation, timeout/error, restart and duplicate recovery.
Use native platform workflows and the existing two-physical-machine device
acceptance runbook before release. A successful source review or package test is
not a substitute for those release gates.

## Registry reconciliation

Nine obsolete mobile path entries came from the inherited upstream rollup and
had no fork-only changes after the common ancestor. The pinned upstream removes
native popover actions, the old Pulse UI/tests and the former settings community
section. Their absent paths are removed from the inventory; fork-owned device,
background, document, session-control and security patches remain registered.
The message-action test's previous no-reactor provider override is replaced by
upstream's explicit connectivity method-channel fixture for grouped sheets.
