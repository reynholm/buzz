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
    hydrate = async () => ({ coveredEventIds: [] }),
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
          return {
            token: `health-${++sequence}`,
            ownerPubkey: "owner",
            relayUrl: "wss://health.invalid",
            workspaceGeneration: 1,
          };
        if (command === "hydrate_device_home_history") return hydrate(args);
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
  await h.advance(250);
  assert.equal(h.count("invalidate_device_home_sync"), 1);
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
