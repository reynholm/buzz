import assert from "node:assert/strict";
import test from "node:test";
import {
  getAgentObserverSnapshot,
  getAgentTranscript,
  resetAgentObserverStore,
  syncAgentObserverEvents,
} from "./observerRelayStore.ts";

const agent = "c".repeat(64);
const update = (seq, status, fields = {}) => ({
  seq,
  timestamp: new Date(
    Date.parse("2026-10-01T00:00:00Z") + seq * 1000,
  ).toISOString(),
  kind: "acp_read",
  agentIndex: 0,
  channelId: "channel",
  sessionId: "stale-envelope",
  turnId: "turn",
  payload: {
    method: "session/update",
    params: {
      sessionId: "provider",
      update: {
        sessionUpdate: "compaction_update",
        compactionId: "id",
        status,
        ...fields,
      },
    },
  },
});

test("production observer store handles replay and out-of-order rebuild, retains raw frames and resets lifecycle state", () => {
  resetAgentObserverStore();
  const completed = update(3, "completed", {
    summary: "Adapter supplied detail",
  });
  syncAgentObserverEvents(agent, [completed]);
  assert.equal(getAgentTranscript(agent).length, 1);
  syncAgentObserverEvents(agent, [update(1, "in_progress"), completed]);
  const transcript = getAgentTranscript(agent);
  assert.equal(transcript.length, 1);
  assert.equal(transcript[0].compaction.status, "completed");
  assert.equal(transcript[0].sessionId, "provider");
  assert.equal(getAgentObserverSnapshot(agent).events.length, 2);
  assert.deepEqual(
    getAgentObserverSnapshot(agent).events[1].payload,
    completed.payload,
  );
  resetAgentObserverStore();
  syncAgentObserverEvents(agent, [update(4, "in_progress")]);
  assert.equal(getAgentTranscript(agent)[0].compaction.status, "in_progress");
  resetAgentObserverStore();
});
