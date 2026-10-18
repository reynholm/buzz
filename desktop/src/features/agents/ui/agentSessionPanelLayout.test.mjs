import assert from "node:assert/strict";
import test from "node:test";

import {
  deriveLatestSessionId,
  observerEventScrollId,
  resolveDisplayEvents,
  resolveRawRailLayout,
  scopeByChannel,
} from "./agentSessionPanelLayout.ts";

// ---- scopeByChannel ----

const items = [
  { id: "a", channelId: "channel-1" },
  { id: "b", channelId: "channel-2" },
  { id: "c", channelId: "channel-1" },
];

test("scopeByChannel returns the input unchanged when channelId is null", () => {
  assert.equal(scopeByChannel(items, null), items);
});

test("scopeByChannel returns the input unchanged when channelId is undefined", () => {
  assert.equal(scopeByChannel(items, undefined), items);
});

test("scopeByChannel filters items down to the requested channel", () => {
  const scoped = scopeByChannel(items, "channel-1");
  assert.deepEqual(
    scoped.map((item) => item.id),
    ["a", "c"],
  );
});

test("scopeByChannel returns an empty array when no item matches", () => {
  assert.deepEqual(scopeByChannel(items, "channel-99"), []);
});

// ---- deriveLatestSessionId ----

test("deriveLatestSessionId returns null for an empty list", () => {
  assert.equal(deriveLatestSessionId([]), null);
});

test("deriveLatestSessionId returns the last event's sessionId", () => {
  const events = [
    { seq: 1, sessionId: "sess-1" },
    { seq: 2, sessionId: "sess-2" },
  ];
  assert.equal(deriveLatestSessionId(events), "sess-2");
});

test("deriveLatestSessionId skips trailing events without a sessionId", () => {
  const events = [
    { seq: 1, sessionId: "sess-1" },
    { seq: 2, sessionId: null },
    { seq: 3, sessionId: undefined },
  ];
  assert.equal(deriveLatestSessionId(events), "sess-1");
});

test("deriveLatestSessionId returns null when no event carries a sessionId", () => {
  const events = [{ seq: 1, sessionId: null }, { seq: 2 }];
  assert.equal(deriveLatestSessionId(events), null);
});

// ---- resolveDisplayEvents ----

test("resolveDisplayEvents returns raw override events unchanged", () => {
  const scopedEvents = [{ seq: 1, channelId: "channel-1" }];
  const rawEventsOverride = [{ seq: 2, channelId: "debug-channel" }];
  assert.equal(
    resolveDisplayEvents(scopedEvents, rawEventsOverride),
    rawEventsOverride,
  );
});

test("resolveDisplayEvents falls back to scoped live events", () => {
  const scopedEvents = [{ seq: 1, channelId: "channel-1" }];
  assert.equal(resolveDisplayEvents(scopedEvents, undefined), scopedEvents);
});

// ---- resolveRawRailLayout (raw-ACP view toggle) ----

test("resolveRawRailLayout hides the rail when showRaw is off", () => {
  assert.deepEqual(resolveRawRailLayout(false, "responsive"), {
    mode: "hidden",
  });
  assert.deepEqual(resolveRawRailLayout(false, "exclusive"), {
    mode: "hidden",
  });
});

test("resolveRawRailLayout renders the rail exclusively when toggled on in exclusive layout", () => {
  assert.deepEqual(resolveRawRailLayout(true, "exclusive"), {
    mode: "exclusive",
  });
});

test("resolveRawRailLayout renders the rail beside the transcript in responsive layout", () => {
  assert.deepEqual(resolveRawRailLayout(true, "responsive"), { mode: "side" });
});

// ---- observerEventScrollId ----
// seq is process-local (buzz-acp's ObserverHandle) and resets to 1 on every
// agent restart while timestamp keeps climbing, so seq alone is not unique
// across an agent's combined observer history — pair it with timestamp,
// the same identity mergeObserverEventWindows dedups on above.

test("observerEventScrollId combines seq and timestamp", () => {
  const event = { seq: 1, timestamp: "2026-07-13T21:00:00.000Z" };
  assert.equal(observerEventScrollId(event), "1:2026-07-13T21:00:00.000Z");
});

test("observerEventScrollId returns distinct ids for same seq across a restart", () => {
  const before = { seq: 1, timestamp: "2026-07-13T20:00:00.000Z" };
  const after = { seq: 1, timestamp: "2026-07-13T21:00:00.000Z" };
  assert.notEqual(observerEventScrollId(before), observerEventScrollId(after));
});

test("channel transcript retains global worker boundaries without expanding the raw channel rail", async () => {
  const { buildTranscriptState } = await import("./agentSessionTranscript.ts");
  const { deriveTranscriptBlockIds } = await import(
    "./agentSessionTranscriptGrouping.ts"
  );
  const frame = (seq, kind, payload, channelId = "channel-1") => ({
    seq,
    timestamp: new Date(
      Date.parse("2026-10-01T00:00:00Z") + seq * 1000,
    ).toISOString(),
    kind,
    payload,
    channelId,
    sessionId: "provider",
    agentIndex: 0,
    turnId: "turn",
  });
  const typed = (seq, status) =>
    frame(seq, "acp_read", {
      method: "session/update",
      params: {
        sessionId: "provider",
        update: {
          sessionUpdate: "compaction_update",
          compactionId: "id",
          status,
        },
      },
    });
  const events = [
    frame(1, "agent_initialized", {}, null),
    typed(2, "completed"),
    frame(3, "agent_initialized", {}, null),
    typed(4, "in_progress"),
    frame(5, "turn_started", {}, "other-channel"),
  ];
  const raw = scopeByChannel(events, "channel-1");
  const transcriptEvents = scopeByChannel(events, "channel-1", true);
  assert.equal(raw.length, 2);
  const state = buildTranscriptState(transcriptEvents);
  assert.equal(state.items.length, 2);
  assert.notEqual(state.items[0].id, state.items[1].id);
  assert.equal(
    state.items.every((item) => item.channelId === "channel-1"),
    true,
  );
  assert.equal(
    new Set(deriveTranscriptBlockIds(transcriptEvents)).size,
    deriveTranscriptBlockIds(transcriptEvents).length,
  );
});
