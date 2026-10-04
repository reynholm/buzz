import assert from "node:assert/strict";
import test, { mock } from "node:test";

import { relayClient } from "@/shared/api/relayClient";
import {
  KIND_DELETION,
  KIND_MANAGED_AGENT,
  KIND_PERSONA,
  KIND_TEAM,
  KIND_TEAM_CATALOG,
} from "@/shared/constants/kinds";
import {
  coalesceManagedAgentBackfill,
  orderCatalogHeadsLast,
  PersonaHistoryDenseBoundaryError,
  startPersonaSync,
} from "./usePersonaSync.ts";

const EXPECTED_KINDS = [
  KIND_PERSONA,
  KIND_TEAM,
  KIND_MANAGED_AGENT,
  KIND_TEAM_CATALOG,
  KIND_DELETION,
];

// These tests exercise the actual hook/API through the native IPC boundary.
// History paging/application is covered by Rust hydrate_history tests.
function nativeSync(
  t,
  {
    hydrate = async () => ({ coveredEventIds: [] }),
    reconcile = async () => {},
    relayUrl = "wss://relay.example",
    subscribe,
    begin,
  } = {},
) {
  const calls = [];
  const warnings = [];
  mock.method(console, "warn", (...args) => {
    warnings.push(args);
  });
  let sequence = 0;
  globalThis.window = {
    __TAURI_INTERNALS__: {
      invoke: async (cmd, args) => {
        calls.push({ cmd, args });
        if (cmd === "begin_device_home_sync") {
          const session = {
            token: `token-${++sequence}`,
            ownerPubkey: "owner-pubkey",
            relayUrl,
            workspaceGeneration: 1,
          };
          return begin ? begin(session) : session;
        }
        if (cmd === "hydrate_device_home_history") return hydrate(args);
        if (cmd === "reconcile_inbound_persona_event") return reconcile(args);
        return undefined;
      },
    },
  };
  let live;
  let connection;
  let reconnect;
  mock.method(
    relayClient,
    "subscribeLive",
    async (filter, listener, onReady) => {
      live = listener;
      const dispose = subscribe
        ? await subscribe(filter, listener)
        : async () => {};
      onReady?.("eose");
      return dispose;
    },
  );
  mock.method(relayClient, "fetchEvents", async () => []);
  mock.method(relayClient, "subscribeToConnectionState", (listener) => {
    connection = listener;
    listener("connected");
    return () => {};
  });
  mock.method(relayClient, "subscribeToReconnects", (listener) => {
    reconnect = listener;
    return () => {};
  });
  t.after(() => {
    mock.reset();
    delete globalThis.window;
  });
  return {
    calls,
    warnings,
    live: (e) => live(e),
    connection: (s) => connection(s),
    reconnect: () => reconnect(),
  };
}

async function settleSync() {
  for (let i = 0; i < 6; i++)
    await new Promise((resolve) => setImmediate(resolve));
}

test("backend sync waits for buffered live applies before finish and carries its token", async (t) => {
  let releaseHistory;
  let releaseApply;
  const history = new Promise((resolve) => {
    releaseHistory = resolve;
  });
  const apply = new Promise((resolve) => {
    releaseApply = resolve;
  });
  const native = nativeSync(t, {
    hydrate: () => history,
    reconcile: () => apply,
  });
  const dispose = startPersonaSync(
    "owner-pubkey",
    "wss://relay.example",
    () => false,
  );
  await settleSync();
  native.live(event({ id: "live", createdAt: 1 }));
  assert.equal(
    native.calls.filter((c) => c.cmd === "reconcile_inbound_persona_event")
      .length,
    0,
  );
  releaseHistory({ coveredEventIds: [] });
  await settleSync();
  assert.equal(
    native.calls.filter((c) => c.cmd === "finish_device_home_sync").length,
    0,
  );
  const call = native.calls.find(
    (c) => c.cmd === "reconcile_inbound_persona_event",
  );
  assert.equal(call.args.sessionToken, "token-1");
  releaseApply();
  await settleSync();
  assert.deepEqual(
    native.calls
      .filter((c) => c.cmd === "finish_device_home_sync")
      .map((c) => c.args),
    [{ sessionToken: "token-1" }],
  );
  await dispose();
});

test("reconcile rejection is latched and cannot finish despite a later successful apply", async (t) => {
  let releaseHistory;
  const native = nativeSync(t, {
    hydrate: () =>
      new Promise((resolve) => {
        releaseHistory = resolve;
      }),
    reconcile: async (args) => {
      if (JSON.parse(args.eventJson).id === "bad")
        throw new Error("apply failed");
    },
  });
  const dispose = startPersonaSync(
    "owner-pubkey",
    "wss://relay.example",
    () => false,
  );
  await settleSync();
  native.live(event({ id: "bad", createdAt: 1 }));
  native.live(event({ id: "good", createdAt: 2, dTag: "other" }));
  releaseHistory({ coveredEventIds: [] });
  await settleSync();
  assert.equal(
    native.calls.filter((c) => c.cmd === "finish_device_home_sync").length,
    0,
  );
  assert.equal(
    native.calls.filter((c) => c.cmd === "reconcile_inbound_persona_event")
      .length,
    2,
  );
  assert.equal(
    native.warnings.filter((w) => w[0].includes("reconcile failed")).length,
    1,
  );
  await dispose();
});

test("deferred apply rejection remains failed and disposal invalidates exactly its backend token", async (t) => {
  let releaseHistory;
  let rejectApply;
  const native = nativeSync(t, {
    hydrate: () =>
      new Promise((resolve) => {
        releaseHistory = resolve;
      }),
    reconcile: () =>
      new Promise((_resolve, reject) => {
        rejectApply = reject;
      }),
  });
  const dispose = startPersonaSync(
    "owner-pubkey",
    "wss://relay.example",
    () => false,
  );
  await settleSync();
  native.live(event({ id: "deferred", createdAt: 1 }));
  releaseHistory({ coveredEventIds: [] });
  await settleSync();
  const apply = native.calls.find(
    (call) => call.cmd === "reconcile_inbound_persona_event",
  );
  assert.equal(apply.args.sessionToken, "token-1");
  assert.equal(
    native.calls.filter((call) => call.cmd === "finish_device_home_sync")
      .length,
    0,
  );
  rejectApply("deferred apply failed");
  await settleSync();
  assert.equal(
    native.calls.filter((call) => call.cmd === "finish_device_home_sync")
      .length,
    0,
  );
  assert.equal(
    native.warnings.filter(
      (warning) =>
        warning[0].includes("reconcile failed") &&
        String(warning[1]).includes("deferred apply failed"),
    ).length,
    1,
  );
  await dispose();
  assert.deepEqual(
    native.calls
      .filter((call) => call.cmd === "invalidate_device_home_sync")
      .map((call) => call.args),
    [{ sessionToken: "token-1" }],
  );
});

test("connection loss invalidates readiness and reconnect starts a fresh complete session", async (t) => {
  const native = nativeSync(t);
  const dispose = startPersonaSync(
    "owner-pubkey",
    "wss://relay.example",
    () => false,
  );
  await settleSync();
  native.connection("reconnecting");
  await settleSync();
  assert.deepEqual(
    native.calls
      .filter((c) => c.cmd === "invalidate_device_home_sync")
      .map((c) => c.args),
    [{ sessionToken: "token-1" }],
  );
  native.connection("connected");
  native.reconnect();
  await settleSync();
  assert.equal(
    native.calls.filter((c) => c.cmd === "begin_device_home_sync").length,
    2,
  );
  assert.deepEqual(
    native.calls
      .filter((c) => c.cmd === "finish_device_home_sync")
      .map((c) => c.args.sessionToken),
    ["token-1", "token-2"],
  );
  await dispose();
});

test("disposal during hydration cannot finish or dispatch buffered events", async (t) => {
  let releaseHistory;
  const native = nativeSync(t, {
    hydrate: () =>
      new Promise((resolve) => {
        releaseHistory = resolve;
      }),
  });
  const dispose = startPersonaSync(
    "owner-pubkey",
    "wss://relay.example",
    () => false,
  );
  await settleSync();
  native.live(event({ id: "old", createdAt: 1 }));
  await dispose();
  releaseHistory({ coveredEventIds: [] });
  await settleSync();
  assert.equal(
    native.calls.filter((c) => c.cmd === "finish_device_home_sync").length,
    0,
  );
  assert.equal(
    native.calls.filter((c) => c.cmd === "reconcile_inbound_persona_event")
      .length,
    0,
  );
  assert.equal(
    native.calls.filter((c) => c.cmd === "invalidate_device_home_sync").length,
    1,
  );
});

test("disposal before begin returns invalidates its late token without subscribing", async (t) => {
  let release;
  const native = nativeSync(t, {
    begin: (session) =>
      new Promise((resolve) => {
        release = () => resolve(session);
      }),
  });
  const dispose = startPersonaSync(
    "owner-pubkey",
    "wss://relay.example",
    () => false,
  );
  await settleSync();
  await dispose();
  release();
  await settleSync();
  assert.equal(relayClient.subscribeLive.mock.callCount(), 0);
  assert.equal(
    native.calls.filter((c) => c.cmd === "hydrate_device_home_history").length,
    0,
  );
  assert.deepEqual(
    native.calls
      .filter((c) => c.cmd === "invalidate_device_home_sync")
      .map((c) => c.args.sessionToken),
    ["token-1"],
  );
});

test("late previous hydration cannot finalize a replacement session", async (t) => {
  let releaseOld;
  let attempts = 0;
  const native = nativeSync(t, {
    hydrate: () =>
      ++attempts === 1
        ? new Promise((resolve) => {
            releaseOld = resolve;
          })
        : Promise.resolve({ coveredEventIds: [] }),
  });
  const dispose = startPersonaSync(
    "owner-pubkey",
    "wss://relay.example",
    () => false,
  );
  await settleSync();
  native.connection("reconnecting");
  native.connection("connected");
  await settleSync();
  releaseOld({ coveredEventIds: [] });
  await settleSync();
  assert.deepEqual(
    native.calls
      .filter((c) => c.cmd === "finish_device_home_sync")
      .map((c) => c.args.sessionToken),
    ["token-2"],
  );
  await dispose();
});

test("backend session for another scope is invalidated without history or live registration", async (t) => {
  const native = nativeSync(t, { relayUrl: "wss://other.example" });
  const dispose = startPersonaSync(
    "owner-pubkey",
    "wss://relay.example",
    () => false,
  );
  await settleSync();
  assert.equal(relayClient.subscribeLive.mock.callCount(), 0);
  assert.equal(
    native.calls.filter((c) => c.cmd === "hydrate_device_home_history").length,
    0,
  );
  assert.equal(
    native.calls.filter((c) => c.cmd === "invalidate_device_home_sync").length,
    1,
  );
  await dispose();
});

function event({
  id,
  kind = KIND_MANAGED_AGENT,
  createdAt,
  pubkey = "owner-pubkey",
  dTag = "agent-pubkey",
}) {
  return {
    id,
    pubkey,
    created_at: createdAt,
    kind,
    tags: dTag ? [["d", dTag]] : [],
    content: "{}",
    sig: "sig",
  };
}

test("startup backfill keeps only the newest managed-agent head per coordinate", () => {
  const persona = event({
    id: "persona",
    kind: KIND_PERSONA,
    createdAt: 1,
    dTag: "persona-id",
  });
  const otherAgent = event({
    id: "other-agent",
    createdAt: 2,
    dTag: "other-agent",
  });
  const oldest = event({ id: "oldest", createdAt: 1 });
  const sameSecondLoser = event({ id: "f", createdAt: 3 });
  const newest = event({ id: "a", createdAt: 3 });

  assert.deepEqual(
    coalesceManagedAgentBackfill([
      oldest,
      persona,
      newest,
      otherAgent,
      sameSecondLoser,
    ]).map(({ id }) => id),
    ["persona", "a", "other-agent"],
    "NIP-33 uses newest created_at and lowest id on a tie",
  );
});

// Regression guard for the fresh-device backfill ordering defect (Carl r10 P1,
// finding 2): the relay serves history newest-first, so a freshly shared 30178
// head arrives before the 30176 team and 30175 personas it projects. Dispatched
// in that order, the inbound team refresh runs before device B's personas
// hydrate — member resolution fails and the owner's valid shared head is purged
// plus falsely tombstoned. `orderCatalogHeadsLast` MUST defer every catalog head
// past its constituents while preserving relay order within each group.
test("orderCatalogHeadsLast defers catalog heads past all constituents", () => {
  const catalog = event({ id: "cat", kind: KIND_TEAM_CATALOG, createdAt: 30 });
  const team = event({ id: "team", kind: KIND_TEAM, createdAt: 20 });
  const persona = event({ id: "p1", kind: KIND_PERSONA, createdAt: 10 });
  const deletion = event({ id: "del", kind: KIND_DELETION, createdAt: 5 });

  assert.deepEqual(
    // Relay newest-first order: catalog head lands before its constituents.
    orderCatalogHeadsLast([catalog, team, persona, deletion]).map(
      ({ id }) => id,
    ),
    ["team", "p1", "del", "cat"],
    "constituents dispatch first; the catalog head is deferred to the end",
  );
});

test("orderCatalogHeadsLast preserves relay order within each group", () => {
  const catA = event({ id: "cat-a", kind: KIND_TEAM_CATALOG, createdAt: 40 });
  const catB = event({ id: "cat-b", kind: KIND_TEAM_CATALOG, createdAt: 30 });
  const teamA = event({ id: "team-a", kind: KIND_TEAM, createdAt: 20 });
  const teamB = event({ id: "team-b", kind: KIND_TEAM, createdAt: 10 });

  assert.deepEqual(
    orderCatalogHeadsLast([catA, teamA, catB, teamB]).map(({ id }) => id),
    ["team-a", "team-b", "cat-a", "cat-b"],
    "a stable partition keeps newest-wins order inside constituents and heads",
  );
});

// Regression guard for the fresh-start backfill gap (F3): a device that comes
// online AFTER another published gets zero history from a live-only `limit: 0`
// subscription, because reconnect-replay's since-cursor is undefined until the
// first live event. `startPersonaSync` MUST do a one-shot history fetch up
// front, and both the backfill and the live sub MUST carry the deletion kind
// so tombstones catch up too.
test("startPersonaSync backfills history including the deletion kind", async (t) => {
  const liveCalls = [];
  const native = nativeSync(t, {
    subscribe: async (filter) => {
      liveCalls.push(filter);
      return async () => {};
    },
  });
  const dispose = startPersonaSync(
    "owner-pubkey",
    "wss://relay.example",
    () => false,
  );
  await settleSync();
  assert.equal(
    native.calls.filter((c) => c.cmd === "hydrate_device_home_history").length,
    1,
  );
  assert.deepEqual(liveCalls[0].kinds, EXPECTED_KINDS);
  assert.deepEqual(liveCalls[0].authors, ["owner-pubkey"]);
  assert.equal(liveCalls[0].limit, 0);
  assert.equal(
    relayClient.fetchEvents.mock.callCount(),
    0,
    "history must be backend-owned",
  );
  await dispose();
});

// Regression guard for Thufir r10 P2 finding 2 (hydration boundary). The history
// fetch and the live subscription start concurrently into ONE reconcile chain. A
// live/replayed 30178 catalog head that arrives BEFORE the backfill has
// reconciled its 30175/30176 constituents drives the inbound team refresh against
// an unhydrated persona store — member resolution fails and the owner's valid
// witness is purged plus falsely tombstoned. `startPersonaSync` MUST buffer live
// events until the ordered backfill is dispatched, then drain them. Removing the
// buffer dispatches the live head first and turns this RED.
test("startPersonaSync buffers live catalog heads until the backfill hydrates constituents", async (t) => {
  let resolveHistory;
  const native = nativeSync(t, {
    hydrate: () =>
      new Promise((resolve) => {
        resolveHistory = resolve;
      }),
  });
  const dispose = startPersonaSync(
    "owner-pubkey",
    "wss://relay.example",
    () => false,
  );
  await settleSync();
  native.live(
    event({ id: "cat-live", kind: KIND_TEAM_CATALOG, createdAt: 100 }),
  );
  assert.equal(
    native.calls.filter((c) => c.cmd === "reconcile_inbound_persona_event")
      .length,
    0,
  );
  resolveHistory({ coveredEventIds: [] });
  await settleSync();
  const calls = native.calls.map((c) => c.cmd);
  assert.ok(
    calls.indexOf("hydrate_device_home_history") <
      calls.indexOf("reconcile_inbound_persona_event"),
  );
  assert.ok(
    calls.indexOf("reconcile_inbound_persona_event") <
      calls.indexOf("finish_device_home_sync"),
  );
  await dispose();
});

// Regression guard for Thufir r10 P2 finding 3 (capped page). The relay clamps a
// REQ to `max_limit` and serves newest-first, so a large owner's full history
// exceeds one 500-event page: a newer 30178/30176 can land in-page while an older
// required 30175 constituent falls beyond it. `startPersonaSync` MUST page to
// exhaustion via the `until` cursor so every constituent hydrates before the
// catalog head. Removing pagination leaves the required persona unfetched.
test("startPersonaSync delegates history to backend and orders buffered constituents before catalog heads", async (t) => {
  // Real signed inclusive paging and catalog-last application now bind Rust hydrate_history.
  let resolveHistory;
  const native = nativeSync(t, {
    hydrate: () =>
      new Promise((resolve) => {
        resolveHistory = resolve;
      }),
  });
  const dispose = startPersonaSync(
    "owner-pubkey",
    "wss://relay.example",
    () => false,
  );
  await settleSync();
  native.live(event({ id: "cat", kind: KIND_TEAM_CATALOG, createdAt: 1000 }));
  native.live(
    event({
      id: "required",
      kind: KIND_PERSONA,
      createdAt: 100,
      dTag: "required",
    }),
  );
  resolveHistory({ coveredEventIds: [] });
  await settleSync();
  assert.deepEqual(
    native.calls
      .filter((c) => c.cmd === "reconcile_inbound_persona_event")
      .map((c) => JSON.parse(c.args.eventJson).id),
    ["required", "cat"],
  );
  assert.equal(
    native.calls.filter((c) => c.cmd === "finish_device_home_sync").length,
    1,
  );
  await dispose();
});

// Regression guard for the arrival-scope fix (F6): the reconcile must carry the
// relay this subscription was opened on, NOT whichever community happens to be
// active when the reconcile runs. Without the forwarded URL the backend falls
// back to the active workspace and an in-flight event lands in the wrong
// community's scoped retention store on a mid-flight switch.
test("startPersonaSync forwards its own relay as the event arrival relay", async (t) => {
  const native = nativeSync(t, { relayUrl: "wss://community-a.example" });
  const dispose = startPersonaSync(
    "owner-pubkey",
    "wss://community-a.example",
    () => false,
  );
  await settleSync();
  native.live(event({ id: "own", createdAt: 1 }));
  native.live(event({ id: "foreign", createdAt: 1, pubkey: "someone-else" }));
  await settleSync();
  const reconciles = native.calls.filter(
    (c) => c.cmd === "reconcile_inbound_persona_event",
  );
  assert.equal(reconciles.length, 1);
  assert.equal(reconciles[0].args.arrivalRelayUrl, "wss://community-a.example");
  assert.equal(reconciles[0].args.sessionToken, "token-1");
  await dispose();
});

// Regression guard for Carl r12 P1 finding 1 (startup gap between backfill and
// live registration). The live REQ only registers at the relay after
// `subscribe()`'s send completes, so if the backfill runs FIRST an owner event
// published after the history EOSE but before live registration is returned by
// neither path and is lost until remount. `startPersonaSync` MUST establish the
// live subscription first, then run the backfill — so an event arriving in that
// window is delivered live (buffered) and still reconciles. Restoring
// backfill-before-subscribe order fires `fetchEvents` before the listener
// exists, so the gap event is never delivered: reconciled zero times, RED.
test("startPersonaSync subscribes live before backfilling so a gap event still reconciles once", async (t) => {
  const order = [];
  let listener;
  const native = nativeSync(t, {
    subscribe: async (_filter, onEvent) => {
      order.push("subscribe");
      listener = onEvent;
      return async () => {};
    },
    hydrate: async () => {
      order.push("hydrate");
      listener(event({ id: "gap-event", kind: KIND_PERSONA, createdAt: 500 }));
      return { coveredEventIds: [] };
    },
  });
  const dispose = startPersonaSync(
    "owner-pubkey",
    "wss://relay.example",
    () => false,
  );
  await settleSync();
  assert.deepEqual(order, ["subscribe", "hydrate"]);
  assert.deepEqual(
    native.calls
      .filter((c) => c.cmd === "reconcile_inbound_persona_event")
      .map((c) => JSON.parse(c.args.eventJson).id),
    ["gap-event"],
  );
  await dispose();
});

// Regression guard for Carl r12 P1 finding 1 (dedupe). Because the live sub now
// registers before the backfill queries history, an event published in that
// window is delivered live (buffered) AND returned by the backfill.
// `reconcileInboundPersonaEvent` is not idempotent, so the drain MUST skip any
// buffered event the backfill already reconciled. Removing the dedupe skip
// dispatches the buffered duplicate too, reconciling it twice — RED.
test("startPersonaSync reconciles an event only once when it appears both live-buffered and in the backfill", async (t) => {
  let resolveHistory;
  const native = nativeSync(t, {
    hydrate: () =>
      new Promise((resolve) => {
        resolveHistory = resolve;
      }),
  });
  const dispose = startPersonaSync(
    "owner-pubkey",
    "wss://relay.example",
    () => false,
  );
  await settleSync();
  native.live(event({ id: "overlap", createdAt: 400 }));
  native.live(event({ id: "superseded", createdAt: 300, dTag: "old" }));
  native.live(event({ id: "new-live", createdAt: 500 }));
  resolveHistory({ coveredEventIds: ["overlap", "superseded"] });
  await settleSync();
  assert.deepEqual(
    native.calls
      .filter((c) => c.cmd === "reconcile_inbound_persona_event")
      .map((c) => JSON.parse(c.args.eventJson).id),
    ["new-live"],
  );
  await dispose();
});

// Regression guard for Carl r12 P2 finding 1 (initial subscription rejection
// must not leave both paths inert). `startPersonaSync` now runs `runBackfill()`
// inside `subscribeLive(...).then(...)`. `relayClientSession.subscribe()` really
// rejects (relay unreachable, or a terminal auth state) — deleting the sub and
// rethrowing. Since the mount restarts only on `[pubkey, relayUrl]` change and
// nothing else remounts this effect, a bare `.then()` chain would leave ALL
// history unhydrated for the mount lifetime plus emit an unhandled rejection.
// The `.catch()` MUST consume the rejection and still backfill. Restoring the
// bare `.then()` (dropping the catch) fires no `fetchEvents` and leaks an
// unhandled rejection — RED.
test("startPersonaSync still backfills history when the live subscription rejects", async (t) => {
  const native = nativeSync(t, {
    subscribe: async () => {
      throw new Error("initial subscription failed");
    },
  });
  const dispose = startPersonaSync(
    "owner-pubkey",
    "wss://relay.example",
    () => false,
  );
  await settleSync();
  assert.equal(
    native.calls.filter((c) => c.cmd === "hydrate_device_home_history").length,
    1,
  );
  assert.equal(
    native.calls.filter((c) => c.cmd === "finish_device_home_sync").length,
    0,
    "live-blind hydration cannot install Ready",
  );
  await dispose();
});

// Regression guard for Will r10 P3 finding 1 (dense-boundary pagination). The WS
// filter exposes only a time-only `until` cursor, so when MORE than one page of
// events shares the oldest boundary second the cursor cannot advance: page 2
// returns the same slice, and older events (a required 30175) are unreachable.
// `fetchOwnerHistoryToExhaustion` MUST throw `PersonaHistoryDenseBoundaryError`
// rather than treat the unadvanceable page as exhaustion. The pipeline then
// enters degraded-live and DROPS catalog heads (their constituents never
// hydrated). Reverting to time-only `added === 0` termination silently completes
// backfill as if exhaustive: the live catalog head is reconciled (the purge
// path) instead of dropped, turning this RED.
test("startPersonaSync fails loudly on a dense boundary and degrades to catalog-dropping live sync", async (t) => {
  const native = nativeSync(t, {
    hydrate: async () => {
      throw "device_home_sync_dense_history_boundary";
    },
  });
  const dispose = startPersonaSync(
    "owner-pubkey",
    "wss://relay.example",
    () => false,
  );
  await settleSync();
  native.live(event({ id: "catalog", kind: KIND_TEAM_CATALOG, createdAt: 1 }));
  native.live(event({ id: "runtime", createdAt: 2 }));
  await settleSync();
  assert.deepEqual(
    native.calls
      .filter((c) => c.cmd === "reconcile_inbound_persona_event")
      .map((c) => JSON.parse(c.args.eventJson).id),
    ["runtime"],
  );
  assert.equal(
    native.calls.filter((c) => c.cmd === "finish_device_home_sync").length,
    0,
  );
  await dispose();
});

// Regression guard for Thufir r10-delta finding (degraded-live false unshare).
// A witness-holding device that drops back to degraded-live still receives live
// team/persona edits. Live delivery is newest-first, so a 30176 team edit that
// ADDS a new persona reaches the backend BEFORE that persona's 30175. The
// backend's KIND_TEAM arm unconditionally refreshes the catalog head after a
// save; against the retained witness the new member cannot resolve, so it purges
// the valid witness and queues a false tombstone — the OLD constituents on disk
// don't cover a NEW member. Degraded mode MUST drop the whole catalog dependency
// set (30175/30176 + kind-5 deletions targeting them), not just 30178, so the
// backend never sees the un-hydrated edit. Narrowing the gate back to 30178-only
// dispatches the 30176 to the backend and turns this RED.
test("degraded-live drops a team edit adding a new persona so a witness is not falsely tombstoned", async (t) => {
  const native = nativeSync(t, {
    hydrate: async () => {
      throw "device_home_sync_dense_history_boundary";
    },
  });
  const dispose = startPersonaSync(
    "owner-pubkey",
    "wss://relay.example",
    () => false,
  );
  await settleSync();
  native.live(event({ id: "new-team", kind: KIND_TEAM, createdAt: 10 }));
  native.live(event({ id: "new-persona", kind: KIND_PERSONA, createdAt: 9 }));
  native.live(event({ id: "runtime", createdAt: 8 }));
  await settleSync();
  assert.deepEqual(
    native.calls
      .filter((c) => c.cmd === "reconcile_inbound_persona_event")
      .map((c) => JSON.parse(c.args.eventJson).id),
    ["runtime"],
  );
  await dispose();
});

// Regression guard for Thufir r11 finding (degraded gate vs Rust router). A
// kind-5 deletion is classified by scanning ALL `a` tags, because Rust's
// `parse_deletion_coordinate` find_maps across every tag and routes the first
// signer-owned dependency coordinate. A valid kind-5 can carry a malformed or
// foreign first `a` tag ahead of an owned 30176 coordinate: reading only the
// first tag returns null and dispatches it, but Rust skips the bad first tag,
// routes the owned 30176, deletes the team, and fires the destructive catalog
// refresh this gate exists to suppress. Degraded mode MUST hold the deletion
// whenever ANY parseable `a` tag names a dependency kind. Narrowing the
// classifier back to the first `a` tag dispatches this deletion and turns RED.
test("degraded-live drops a kind-5 whose owned dependency `a` tag is not first", async (t) => {
  const native = nativeSync(t, {
    hydrate: async () => {
      throw "device_home_sync_dense_history_boundary";
    },
  });
  const dispose = startPersonaSync(
    "owner-pubkey",
    "wss://relay.example",
    () => false,
  );
  await settleSync();
  const deletion = event({ id: "deletion", kind: KIND_DELETION, createdAt: 1 });
  deletion.tags = [
    ["a", "30177:someone-else:instance"],
    ["a", "30176:owner-pubkey:team"],
  ];
  native.live(deletion);
  native.live(event({ id: "runtime", createdAt: 2 }));
  await settleSync();
  assert.deepEqual(
    native.calls
      .filter((c) => c.cmd === "reconcile_inbound_persona_event")
      .map((c) => JSON.parse(c.args.eventJson).id),
    ["runtime"],
  );
  await dispose();
});

// Regression guard for Will r10 P3 finding 2 (backfill rejection stranding live
// sync). A transient history-fetch rejection must not leave the subscription
// permanently unhydrated with `liveBuffer` accumulating forever. The backfill
// MUST retry with bounded backoff; on success the buffer drains and live events
// reconcile. Restoring a log-only `.catch` (no retry, `hydrated` never set)
// leaves the buffered live event unreconciled, turning this RED.
test("startPersonaSync retries a transient backfill rejection so a buffered live event still reconciles", async (t) => {
  mock.timers.enable({ apis: ["setTimeout"] });
  t.after(() => mock.timers.reset());
  let attempts = 0;
  const native = nativeSync(t, {
    hydrate: async () => {
      if (++attempts === 1) throw new Error("transport failed");
      return { coveredEventIds: [] };
    },
  });
  const dispose = startPersonaSync(
    "owner-pubkey",
    "wss://relay.example",
    () => false,
  );
  await settleSync();
  native.live(event({ id: "buffered", createdAt: 1 }));
  mock.timers.tick(500);
  await settleSync();
  assert.equal(attempts, 2);
  assert.deepEqual(
    native.calls
      .filter((c) => c.cmd === "hydrate_device_home_history")
      .map((c) => c.args.sessionToken),
    ["token-1", "token-2"],
  );
  assert.deepEqual(
    native.calls
      .filter((c) => c.cmd === "reconcile_inbound_persona_event")
      .map((c) => JSON.parse(c.args.eventJson).id),
    ["buffered"],
  );
  assert.equal(
    native.calls.filter((c) => c.cmd === "finish_device_home_sync").length,
    1,
  );
  await dispose();
});

// `PersonaHistoryDenseBoundaryError` is deterministic (a dense second cannot
// clear on retry), so the pipeline must NOT retry it — it goes straight to
// degraded-live. Guards against a future refactor that lumps it in with
// transient rejections and burns three fetch attempts on an unrecoverable state.
test("startPersonaSync does not retry a dense-boundary error", async (t) => {
  const native = nativeSync(t, {
    hydrate: async () => {
      throw "device_home_sync_dense_history_boundary";
    },
  });
  const dispose = startPersonaSync(
    "owner-pubkey",
    "wss://relay.example",
    () => false,
  );
  await settleSync();
  assert.equal(
    native.calls.filter((c) => c.cmd === "hydrate_device_home_history").length,
    1,
  );
  assert.equal(
    native.calls.filter((c) => c.cmd === "finish_device_home_sync").length,
    0,
  );
  await dispose();
});

test("PersonaHistoryDenseBoundaryError names the boundary second", () => {
  const error = new PersonaHistoryDenseBoundaryError(42);
  assert.ok(error instanceof Error);
  assert.equal(error.name, "PersonaHistoryDenseBoundaryError");
  assert.match(error.message, /42/);
});

test("startPersonaSync serializes inbound reconciliation in relay order", async (t) => {
  let release;
  const first = new Promise((resolve) => {
    release = resolve;
  });
  const ids = [];
  const native = nativeSync(t, {
    reconcile: async (args) => {
      const id = JSON.parse(args.eventJson).id;
      ids.push(id);
      if (id === "first") await first;
    },
  });
  const dispose = startPersonaSync(
    "owner-pubkey",
    "wss://relay.example",
    () => false,
  );
  await settleSync();
  native.live(event({ id: "first", createdAt: 1 }));
  native.live(event({ id: "second", createdAt: 2 }));
  await settleSync();
  assert.deepEqual(ids, ["first"]);
  release();
  await settleSync();
  assert.deepEqual(ids, ["first", "second"]);
  await dispose();
});
