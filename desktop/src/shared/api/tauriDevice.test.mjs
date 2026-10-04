import assert from "node:assert/strict";
import test from "node:test";
import { getDeviceIdentity, setDeviceLabel } from "./tauriDevice.ts";

function ipc(t, handler) {
  const previous = globalThis.window;
  globalThis.window = { __TAURI_INTERNALS__: { invoke: handler } };
  t.after(() => {
    if (previous === undefined) delete globalThis.window;
    else globalThis.window = previous;
  });
}

test("device IPC maps snake_case identity and preserves queued or complete publication", async (t) => {
  const raw = {
    device_id: "device-a",
    label: "Office",
    created_at: "2026-10-03",
  };
  const expected = {
    deviceId: "device-a",
    label: "Office",
    createdAt: "2026-10-03",
  };
  const calls = [];
  let publication = "queued";
  ipc(t, async (command, args) => {
    calls.push({ command, args });
    return command === "get_device_identity"
      ? raw
      : { identity: raw, publication };
  });
  assert.deepEqual(await getDeviceIdentity(), expected);
  assert.deepEqual(await setDeviceLabel(" Office "), {
    identity: expected,
    publication: "queued",
  });
  publication = "complete";
  assert.deepEqual(await setDeviceLabel("Office"), {
    identity: expected,
    publication: "complete",
  });
  assert.deepEqual(calls, [
    { command: "get_device_identity", args: {} },
    { command: "set_device_label", args: { label: " Office " } },
    { command: "set_device_label", args: { label: "Office" } },
  ]);
});

test("device IPC propagates validation and durable-publication failures", async (t) => {
  ipc(t, async (command, args) => {
    if (command === "get_device_identity") throw "device identity unavailable";
    throw args.label.trim()
      ? "label enqueue failed"
      : "device label is required";
  });
  await assert.rejects(getDeviceIdentity(), /device identity unavailable/);
  await assert.rejects(setDeviceLabel("  "), /device label is required/);
  await assert.rejects(setDeviceLabel("Office"), /label enqueue failed/);
});
