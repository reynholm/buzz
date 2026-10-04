// Actual persona hook and RelayClient CLOSED/EOSE/timeout handling. Only IPC and
// the clock are injected; no network connection or native application is used.
import assert from "node:assert/strict";
import { test } from "node:test";
import { relayClient } from "@/shared/api/relayClient";
import { startPersonaSync } from "./usePersonaSync.ts";
import {
  isRateLimited,
  resetRateLimitGate,
} from "@/shared/api/relayRateLimitGate";

async function flush() {
  for (let i = 0; i < 30; i++) await Promise.resolve();
}

function harness(
  t,
  {
    reply = () => "EOSE",
    begin = (session) => session,
    hydrate = async () => ({ coveredEventIds: [] }),
    reconcile = async () => {},
  } = {},
) {
  const oldWindow = globalThis.window;
  let now = 0;
  let nextTimer = 0;
  let sequence = 0;
  const timers = new Map();
  const calls = [];
  const requests = [];
  const warnings = [];
  const setTimer = (fn, ms) => {
    const id = ++nextTimer;
    timers.set(id, { fn, at: now + ms });
    return id;
  };
  const clearTimer = (id) => timers.delete(id);
  t.mock.method(Date, "now", () => now);
  t.mock.method(globalThis, "setTimeout", setTimer);
  t.mock.method(globalThis, "clearTimeout", clearTimer);
  t.mock.method(console, "warn", (...args) => warnings.push(args));
  globalThis.window = {
    setTimeout: setTimer,
    clearTimeout: clearTimer,
    __TAURI_INTERNALS__: {
      async invoke(command, args) {
        calls.push({ command, args });
        if (command === "begin_device_home_sync")
          return begin({
            token: `health-${++sequence}`,
            ownerPubkey: "owner",
            relayUrl: "wss://health.invalid",
            workspaceGeneration: 1,
          });
        if (command === "hydrate_device_home_history") return hydrate(args);
        if (command === "reconcile_inbound_persona_event")
          return reconcile(args);
        if (command === "plugin:websocket|send") {
          const frame = JSON.parse(args.message.data);
          if (frame[0] === "REQ") {
            requests.push(frame[1]);
            const response = reply(requests.length);
            if (response)
              await deliver([response, frame[1], "restricted: access revoked"]);
          }
          return;
        }
        if (
          [
            "finish_device_home_sync",
            "invalidate_device_home_sync",
            "plugin:websocket|disconnect",
          ].includes(command)
        )
          return;
        assert.fail(`unexpected native call ${command}`);
      },
    },
  };
  resetRateLimitGate();
  relayClient.wsId = 7;
  const deliver = (frame) =>
    relayClient.handleWsMessage(
      { type: "Text", data: JSON.stringify(frame) },
      relayClient.connectionGeneration,
    );
  const dispose = startPersonaSync(
    "owner",
    "wss://health.invalid",
    () => false,
  );
  const count = (command) =>
    calls.filter((call) => call.command === command).length;
  const advance = async (ms) => {
    const target = now + ms;
    for (let fired = 0; ; fired++) {
      const next = [...timers]
        .filter(([, timer]) => timer.at <= target)
        .sort((a, b) => a[1].at - b[1].at || a[0] - b[0])[0];
      if (!next) break;
      assert.ok(fired < 100, "retry timer must be bounded");
      now = next[1].at;
      timers.delete(next[0]);
      next[1].fn();
      await flush();
    }
    now = target;
    await flush();
  };
  t.after(async () => {
    await dispose();
    relayClient.disconnect();
    resetRateLimitGate();
    await flush();
    assert.equal(timers.size, 0, "disposal releases readiness/retry timers");
    globalThis.window = oldWindow;
  });
  return {
    calls,
    requests,
    warnings,
    count,
    deliver,
    advance,
    dispose,
    pendingTimers: () => timers.size,
  };
}

test("terminal CLOSED before confirmation refuses Ready and bounds fresh subscription attempts", async (t) => {
  const h = harness(t, { reply: () => "CLOSED" });
  await flush();
  assert.equal(h.count("finish_device_home_sync"), 0);
  assert.equal(h.count("hydrate_device_home_history"), 0);
  assert.equal(h.count("invalidate_device_home_sync"), 1);
  await h.advance(2000);
  assert.equal(h.count("begin_device_home_sync"), 3);
  assert.equal(h.count("invalidate_device_home_sync"), 3);
  assert.equal(h.count("finish_device_home_sync"), 0);
  assert.equal(relayClient.subscriptions.size, 0);
  await h.advance(10000);
  assert.equal(h.count("begin_device_home_sync"), 3);
});

test("terminal CLOSED after Ready immediately invalidates and replacement exhaustively hydrates", async (t) => {
  const h = harness(t);
  await flush();
  assert.equal(h.count("finish_device_home_sync"), 1);
  await h.deliver(["CLOSED", h.requests[0], "restricted: access revoked"]);
  await flush();
  assert.equal(h.count("invalidate_device_home_sync"), 1);
  await h.advance(500);
  assert.equal(h.count("hydrate_device_home_history"), 2);
  assert.equal(h.count("finish_device_home_sync"), 2);
  const finished = h.calls.filter(
    (call) => call.command === "finish_device_home_sync",
  );
  assert.deepEqual(
    finished.map((call) => call.args.sessionToken),
    ["health-1", "health-2"],
  );
  assert.equal(relayClient.subscriptions.size, 1);
});

test("retryable CLOSED after Ready retires the degraded subscription before fresh recovery", async (t) => {
  const h = harness(t);
  await flush();
  await h.deliver(["CLOSED", h.requests[0], "error: temporarily unavailable"]);
  await flush();
  assert.equal(h.count("invalidate_device_home_sync"), 1);
  assert.equal(relayClient.subscriptions.size, 0);
  await h.advance(500);
  assert.equal(h.count("finish_device_home_sync"), 2);
  await h.advance(1500);
  assert.equal(
    h.requests.length,
    2,
    "retired relay retry cannot resubscribe the old ID",
  );
  assert.notEqual(h.requests[0], h.requests[1]);
});

test("unconfirmed timeout cannot hydrate or finish and confirmed retry starts a fresh session", async (t) => {
  const h = harness(t, { reply: (attempt) => (attempt === 1 ? null : "EOSE") });
  await flush();
  assert.equal(h.count("finish_device_home_sync"), 0);
  await h.advance(5000);
  assert.equal(h.count("invalidate_device_home_sync"), 0);
  assert.equal(h.count("hydrate_device_home_history"), 0);
  assert.equal(h.count("finish_device_home_sync"), 0);
  await h.advance(500);
  assert.equal(h.count("hydrate_device_home_history"), 1);
  assert.equal(h.count("finish_device_home_sync"), 1);
  assert.equal(
    h.calls.find((call) => call.command === "finish_device_home_sync").args
      .sessionToken,
    "health-2",
  );
});

test("CLOSED during hydration retires the old generation and late history cannot finish it", async (t) => {
  let release;
  const pending = new Promise((resolve) => {
    release = resolve;
  });
  const h = harness(t, {
    hydrate: (args) =>
      args.sessionToken === "health-1"
        ? pending
        : Promise.resolve({ coveredEventIds: [] }),
  });
  await flush();
  assert.equal(h.count("hydrate_device_home_history"), 1);
  await h.deliver(["CLOSED", h.requests[0], "restricted: access revoked"]);
  await flush();
  assert.equal(h.count("invalidate_device_home_sync"), 1);
  await h.advance(500);
  release({ coveredEventIds: [] });
  await flush();
  assert.equal(h.count("finish_device_home_sync"), 1);
  assert.equal(
    h.calls.find((call) => call.command === "finish_device_home_sync").args
      .sessionToken,
    "health-2",
  );
});

test("disposal cancels a health retry and late frames cannot launch another session", async (t) => {
  const h = harness(t, { reply: () => "CLOSED" });
  await flush();
  await h.dispose();
  assert.equal(
    h.pendingTimers(),
    0,
    "dispose releases the queued health retry immediately",
  );
  await h.deliver(["EOSE", h.requests[0]]);
  await h.advance(5000);
  assert.equal(h.count("begin_device_home_sync"), 1);
  assert.equal(h.count("finish_device_home_sync"), 0);
  assert.equal(relayClient.subscriptions.size, 0);
});

test("disposal after retryable CLOSED leaves no detached relay retry timer", async (t) => {
  const h = harness(t);
  await flush();
  await h.deliver(["CLOSED", h.requests[0], "error: temporarily unavailable"]);
  await flush();
  await h.dispose();
  assert.equal(h.pendingTimers(), 0);
  assert.equal(relayClient.subscriptions.size, 0);
  await h.advance(5000);
  assert.equal(h.requests.length, 1);
});

test("rate-limited CLOSED retires readiness and preserves shared admission cooldown", async (t) => {
  const h = harness(t);
  await flush();
  await h.deliver([
    "CLOSED",
    h.requests[0],
    "rate-limited: quota exceeded; retry in 1s",
  ]);
  await flush();
  assert.equal(h.count("invalidate_device_home_sync"), 1);
  assert.ok(
    isRateLimited(),
    "retiring a subscription must not bypass the shared quota gate",
  );
  await h.advance(500);
  assert.equal(h.count("finish_device_home_sync"), 1);
  assert.equal(
    h.requests.length,
    1,
    "fresh subscription waits for relay admission",
  );
  await h.advance(500);
  assert.equal(h.requests.length, 2);
  assert.equal(h.count("hydrate_device_home_history"), 2);
  assert.equal(h.count("finish_device_home_sync"), 2);
});

function ownerEvent(id, kind = 30177) {
  return {
    id,
    kind,
    pubkey: "owner",
    created_at: 1,
    tags: [["d", id]],
    content: "{}",
    sig: "fixture",
  };
}

test("300ms EOSE hydrates exhaustive history and keeps every owner live event kind", async (t) => {
  const h = harness(t, { reply: () => null });
  await flush();
  await h.advance(300);
  await h.deliver(["EOSE", h.requests[0]]);
  await flush();
  assert.equal(h.count("hydrate_device_home_history"), 1);
  assert.equal(h.count("finish_device_home_sync"), 1);
  for (const kind of [30175, 30176, 30177, 30178, 5]) {
    await h.deliver(["EVENT", h.requests[0], ownerEvent(`live-${kind}`, kind)]);
  }
  await h.advance(16);
  assert.equal(h.count("reconcile_inbound_persona_event"), 5);
  assert.equal(relayClient.subscriptions.size, 1);
});

test("exhausted confirmation burst retains Pending live delivery and late EOSE recovers exhaustive history", async (t) => {
  let releaseHistory;
  const h = harness(t, {
    reply: () => null,
    hydrate: () =>
      new Promise((resolve) => {
        releaseHistory = resolve;
      }),
  });
  await flush();
  await h.advance(5000);
  await h.advance(500);
  await h.advance(5000);
  await h.advance(1000);
  await h.advance(5000);
  assert.equal(
    h.requests.length,
    3,
    "readiness retries have a bounded initial burst",
  );
  assert.equal(
    relayClient.subscriptions.size,
    1,
    "fallback must retain live delivery during cooldown",
  );
  assert.equal(h.count("finish_device_home_sync"), 0);
  await h.deliver(["EVENT", h.requests[2], ownerEvent("pending-runtime")]);
  await h.deliver(["EVENT", h.requests[2], ownerEvent("pending-team", 30176)]);
  await h.advance(16);
  const applied = h.calls.filter(
    (call) => call.command === "reconcile_inbound_persona_event",
  );
  assert.deepEqual(
    applied.map((call) => JSON.parse(call.args.eventJson).id),
    ["pending-runtime"],
  );
  await h.deliver(["EOSE", h.requests[2]]);
  await flush();
  assert.equal(h.count("hydrate_device_home_history"), 1);
  assert.equal(
    h.count("finish_device_home_sync"),
    0,
    "EOSE alone cannot replace successful full history",
  );
  await h.deliver(["EVENT", h.requests[2], ownerEvent("buffered-team", 30176)]);
  await h.advance(16);
  releaseHistory({ coveredEventIds: [] });
  await flush();
  assert.equal(h.count("finish_device_home_sync"), 1);
  assert.ok(
    h.calls.some(
      (call) =>
        call.command === "reconcile_inbound_persona_event" &&
        JSON.parse(call.args.eventJson).id === "buffered-team",
    ),
  );
  await h.advance(60000);
  assert.equal(
    h.requests.length,
    3,
    "successful recovery cancels cooldown retry",
  );
});

test("confirmation exhaustion keeps a single live subscription until bounded cooldown recovery", async (t) => {
  const h = harness(t, { reply: (attempt) => (attempt > 3 ? "EOSE" : null) });
  await flush();
  await h.advance(16500);
  assert.equal(h.requests.length, 3);
  assert.equal(relayClient.subscriptions.size, 1);
  assert.equal(h.count("finish_device_home_sync"), 0);
  await h.advance(30000);
  assert.equal(h.requests.length, 4);
  assert.equal(h.count("hydrate_device_home_history"), 1);
  assert.equal(h.count("finish_device_home_sync"), 1);
  assert.equal(relayClient.subscriptions.size, 1);
});

test("late EOSE recovery survives a failed native session renewal without abandoning live delivery", async (t) => {
  let hydrateCalls = 0;
  const h = harness(t, {
    reply: (attempt) => (attempt === 1 ? null : "EOSE"),
    begin: (session) => {
      if (session.token === "health-2")
        throw new Error("retention temporarily unavailable");
      return session;
    },
    hydrate: async () => {
      if (++hydrateCalls === 1)
        throw new Error("history transport unavailable");
      return { coveredEventIds: [] };
    },
  });
  await flush();
  await h.advance(5000);
  await h.deliver(["EOSE", h.requests[0]]);
  await flush();
  await h.advance(500);
  assert.equal(h.count("finish_device_home_sync"), 0);
  assert.equal(relayClient.subscriptions.size, 1);
  await h.advance(30000);
  assert.equal(h.count("finish_device_home_sync"), 1);
  assert.equal(relayClient.subscriptions.size, 1);
});

test("late EOSE keeps Pending after exhausted history attempts and recovers through cooldown", async (t) => {
  let hydrateCalls = 0;
  const h = harness(t, {
    reply: (attempt) => (attempt === 1 ? null : "EOSE"),
    hydrate: async () => {
      if (++hydrateCalls <= 3) throw new Error("history transport unavailable");
      return { coveredEventIds: [] };
    },
  });
  await flush();
  await h.advance(5000);
  await h.deliver(["EOSE", h.requests[0]]);
  await flush();
  await h.advance(1500);
  assert.equal(h.count("hydrate_device_home_history"), 3);
  assert.equal(h.count("finish_device_home_sync"), 0);
  assert.equal(relayClient.subscriptions.size, 1);
  await h.advance(30000);
  assert.equal(h.count("finish_device_home_sync"), 1);
  assert.equal(relayClient.subscriptions.size, 1);
});

test("fresh backend token after late EOSE clears only the retired token's failed apply barrier", async (t) => {
  let hydrated = 0;
  const h = harness(t, {
    reply: () => null,
    reconcile: async () => {
      throw new Error("temporary disk error");
    },
    hydrate: async () => {
      if (++hydrated === 1)
        throw new Error("device_home_sync_session_not_pending");
      return { coveredEventIds: [] };
    },
  });
  await flush();
  await h.advance(5000);
  await h.deliver(["EVENT", h.requests[0], ownerEvent("pending-runtime")]);
  await h.advance(16);
  await h.deliver(["EOSE", h.requests[0]]);
  await flush();
  await h.advance(500);
  assert.equal(h.count("hydrate_device_home_history"), 2);
  assert.equal(h.count("begin_device_home_sync"), 2);
  assert.deepEqual(
    h.calls
      .filter((call) => call.command === "finish_device_home_sync")
      .map((call) => call.args.sessionToken),
    ["health-2"],
  );
  await h.advance(60000);
  assert.equal(h.count("finish_device_home_sync"), 1);
});

test("late EOSE history renewal drains outstanding old-token applies before resetting the barrier", async (t) => {
  let rejectApply;
  let hydrated = 0;
  const h = harness(t, {
    reply: () => null,
    reconcile: () =>
      new Promise((_resolve, reject) => {
        rejectApply = reject;
      }),
    hydrate: async () => {
      if (++hydrated === 1) throw new Error("history transport unavailable");
      return { coveredEventIds: [] };
    },
  });
  await flush();
  await h.advance(5000);
  await h.deliver(["EVENT", h.requests[0], ownerEvent("pending-runtime")]);
  await h.advance(16);
  await h.deliver(["EOSE", h.requests[0]]);
  await flush();
  await h.advance(500);
  assert.equal(
    h.count("begin_device_home_sync"),
    1,
    "old token must keep owning its in-flight apply",
  );
  rejectApply(new Error("old apply failed"));
  await flush();
  assert.equal(h.count("begin_device_home_sync"), 2);
  assert.deepEqual(
    h.calls
      .filter((call) => call.command === "finish_device_home_sync")
      .map((call) => call.args.sessionToken),
    ["health-2"],
  );
});

test("failed apply in the current hydrated token refuses Ready and retains bounded fresh-session recovery", async (t) => {
  let releaseHistory;
  const h = harness(t, {
    hydrate: (args) =>
      args.sessionToken === "health-1"
        ? new Promise((resolve) => {
            releaseHistory = resolve;
          })
        : Promise.resolve({ coveredEventIds: [] }),
    reconcile: async () => {
      throw new Error("apply still unavailable");
    },
  });
  await flush();
  await h.deliver(["EVENT", h.requests[0], ownerEvent("buffered-runtime")]);
  await h.advance(16);
  releaseHistory({ coveredEventIds: [] });
  await flush();
  assert.equal(
    h.count("finish_device_home_sync"),
    0,
    "successful history cannot erase a current-token apply failure",
  );
  await h.advance(30000);
  assert.equal(
    h.requests.length,
    2,
    "failed token must retain a bounded recovery affordance",
  );
  assert.deepEqual(
    h.calls
      .filter((call) => call.command === "finish_device_home_sync")
      .map((call) => call.args.sessionToken),
    ["health-2"],
  );
});
