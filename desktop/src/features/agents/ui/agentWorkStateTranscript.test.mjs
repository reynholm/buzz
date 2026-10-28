import assert from "node:assert/strict";
import test from "node:test";
import { buildTranscript } from "./agentSessionTranscript.ts";

const work = "123e4567-e89b-42d3-a456-426614174000";
const frame = (seq, state, extra = {}) => ({
  seq,
  kind: "work_state",
  timestamp: `2026-10-28T18:00:${String(seq).padStart(2, "0")}Z`,
  agentIndex: 0,
  channelId: "123e4567-e89b-42d3-a456-426614174001",
  turnId: null,
  sessionId: null,
  payload: {
    workId: work,
    state,
    reason: "oauth_connection",
    nextAttemptInSecs: 5,
    ...extra,
  },
});

test("logical work states remain visible after the provider turn ends", () => {
  for (const [state, title] of [
    ["running", "Running"],
    ["retrying", "Retrying"],
    ["waiting_approval", "Waiting for approval"],
    ["waiting_background", "Waiting for background task"],
    ["failed", "Failed"],
    ["waiting_human", "Needs your attention"],
  ]) {
    const items = buildTranscript([frame(1, state)]);
    assert.equal(items.length, 1);
    assert.equal(items[0].title, title);
    assert.equal(items[0].type, "lifecycle");
  }
});

test("work state updates stay on the same work and ignore stale replay", () => {
  const newer = frame(3, "waiting_background", { reason: "child_running" });
  const items = buildTranscript([
    frame(1, "running"),
    newer,
    frame(2, "retrying"),
  ]);
  assert.equal(items.length, 1);
  assert.equal(items[0].title, "Waiting for background task");
  assert.equal(items[0].timestamp, newer.timestamp);
});

test("work states reject unknown states and never render raw diagnostic text", () => {
  assert.deepEqual(buildTranscript([frame(1, "untrusted_state")]), []);
  const items = buildTranscript([
    frame(1, "waiting_human", {
      reason: "private command secret=do-not-display",
    }),
  ]);
  assert.equal(items.length, 1);
  assert.doesNotMatch(items[0].text, /secret|private command/);
});
