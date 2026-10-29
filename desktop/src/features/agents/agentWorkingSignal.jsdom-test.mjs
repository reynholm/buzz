import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import { act, cleanup, renderHook } from "@testing-library/react";

import { useThreadWorkingAgentPubkeys } from "./agentWorkingSignal.ts";
import {
  resetActiveAgentTurnsStore,
  syncAgentTurnsFromEvents,
} from "./activeAgentTurnsStore.ts";

afterEach(() => {
  cleanup();
  resetActiveAgentTurnsStore();
});

test("thread working signal survives typing loss, follows scope changes and clears on completion", () => {
  const agent = "ab".repeat(32);
  const fallback = "cd".repeat(32);
  const hook = renderHook(
    ({ channel, root, typing }) =>
      useThreadWorkingAgentPubkeys(channel, root, typing),
    {
      initialProps: {
        channel: "channel",
        root: "root",
        typing: [agent.toUpperCase(), fallback],
      },
    },
  );
  let seq = 0;
  const emit = (kind, turnId, root) =>
    act(() => {
      syncAgentTurnsFromEvents(agent, [
        {
          seq: ++seq,
          timestamp: new Date().toISOString(),
          kind,
          channelId: "channel",
          sessionId: null,
          agentIndex: 0,
          turnId,
          payload: { threadRootEventId: root },
        },
      ]);
    });
  assert.deepEqual(hook.result.current, [agent, fallback]);
  emit("turn_liveness", "first", "root");
  emit("turn_started", "second", "sibling");
  assert.deepEqual(
    hook.result.current,
    [agent, fallback],
    "typing and observer are deduplicated",
  );
  hook.rerender({ channel: "channel", root: "root", typing: [] });
  assert.deepEqual(
    hook.result.current,
    [agent],
    "typing expiry cannot hide a live turn",
  );
  emit("turn_completed", "first", "root");
  assert.deepEqual(
    hook.result.current,
    [],
    "sibling work cannot keep this thread active",
  );
  hook.rerender({ channel: "channel", root: "sibling", typing: [] });
  assert.deepEqual(hook.result.current, [agent]);
  hook.rerender({ channel: "foreign", root: "sibling", typing: [] });
  assert.deepEqual(hook.result.current, []);
  hook.rerender({ channel: "channel", root: null, typing: [fallback] });
  assert.deepEqual(hook.result.current, []);
});
