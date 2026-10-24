import assert from "node:assert/strict";
import test from "node:test";
import { act } from "react";

import {
  installDOMShim,
  installFreshStorage,
  mountHook,
} from "./observedUnreadTestHarness.mjs";
import { installNativeRig } from "./observedUnreadNativeRig.mjs";

installDOMShim();

const RELAY = "wss://relay.example.com";
const NOW = Math.floor(Date.now() / 1_000);
const props = (pubkey, getTs = () => NOW) => ({
  pubkey,
  relay: RELAY,
  isReady: true,
  readStateVersion: 0,
  getTs,
  getOwn: getTs,
});
const refs = () => ({
  eventsRef: { current: new Map() },
  latestRef: { current: new Map() },
});
async function settle() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

test("a marker burst writes each context once in bounded native batches", async () => {
  installFreshStorage();
  const rig = installNativeRig();
  let harness;
  try {
    harness = await mountHook(props("marker-burst"), refs());
    await settle();
    const contexts = Array.from({ length: 200 }, (_, i) => `thread:root-${i}`);
    await act(async () => {
      for (const context of contexts) harness.api.syncMarkers([context]);
    });
    await settle();
    await settle();

    assert.ok(
      rig.requests("observed_unread_ingest").length <= 2,
      "a synchronous burst needs at most an initial write and one coalesced write",
    );
    assert.equal(rig.markerUpdates().length, contexts.length);
    assert.deepEqual(
      new Set(rig.markerUpdates().map((marker) => marker.contextId)),
      new Set(contexts),
    );
  } finally {
    await harness?.unmount();
    rig.restore();
  }
});

test("identical pending markers share one native acknowledgement", async () => {
  installFreshStorage();
  const rig = installNativeRig();
  let harness;
  try {
    harness = await mountHook(props("marker-repeat"), refs());
    await settle();
    await act(async () => {
      for (let i = 0; i < 200; i += 1) {
        harness.api.syncMarkers(["thread:unchanged"]);
      }
    });
    await settle();
    await settle();
    assert.deepEqual(rig.markerUpdates(), [
      { contextId: "thread:unchanged", readAt: NOW },
    ]);
  } finally {
    await harness?.unmount();
    rig.restore();
  }
});

test("an in-flight marker acknowledgement preserves the newest pending removal", async () => {
  installFreshStorage();
  const rig = installNativeRig();
  const original = window.__TAURI_INTERNALS__.invoke;
  let release;
  const gate = new Promise((resolve) => {
    release = resolve;
  });
  let blocked = false;
  window.__TAURI_INTERNALS__.invoke = async (command, args) => {
    if (command === "observed_unread_ingest" && !blocked) {
      blocked = true;
      await gate;
    }
    return original(command, args);
  };
  let readAt = NOW;
  let harness;
  try {
    harness = await mountHook(
      props("marker-removal", () => readAt),
      refs(),
    );
    await settle();
    harness.api.syncMarkers(["channel"]);
    await settle();
    assert.equal(blocked, true);
    readAt = NOW + 1;
    harness.api.syncMarkers(["channel"]);
    readAt = null;
    harness.api.syncMarkers(["channel"]);
    release();
    await settle();
    await settle();
    assert.deepEqual(rig.markerUpdates(), [
      { contextId: "channel", readAt: NOW },
      { contextId: "channel", readAt: null },
    ]);
    assert.equal(
      rig
        .scope({ pubkey: "marker-removal", relayUrl: RELAY })
        .markers.has("channel"),
      false,
    );
  } finally {
    release();
    await harness?.unmount();
    rig.restore();
  }
});

test("a retired scope acknowledgement does not strand markers when returning to it", async () => {
  installFreshStorage();
  const rig = installNativeRig();
  const original = window.__TAURI_INTERNALS__.invoke;
  let release;
  const gate = new Promise((resolve) => {
    release = resolve;
  });
  let blocked = false;
  window.__TAURI_INTERNALS__.invoke = async (command, args) => {
    if (command === "observed_unread_ingest" && !blocked) {
      blocked = true;
      await gate;
    }
    return original(command, args);
  };
  let harness;
  try {
    harness = await mountHook(props("marker-scope-a"), refs());
    await settle();
    harness.api.syncMarkers(["channel-a"]);
    await settle();
    harness.api.syncMarkers(["channel-a"], new Map([["channel-a", NOW + 1]]));
    await harness.render(props("marker-scope-b"));
    await settle();
    harness.api.syncMarkers(["channel-b"]);
    release();
    await settle();
    await settle();
    assert.equal(
      rig
        .scope({ pubkey: "marker-scope-b", relayUrl: RELAY })
        .markers.get("channel-b"),
      NOW,
    );

    await harness.render(props("marker-scope-a"));
    await settle();
    await settle();
    assert.equal(
      rig
        .scope({ pubkey: "marker-scope-a", relayUrl: RELAY })
        .markers.get("channel-a"),
      NOW + 1,
      "the retired scope must release its flush fence before the new scope check",
    );
  } finally {
    release();
    await harness?.unmount();
    rig.restore();
  }
});
