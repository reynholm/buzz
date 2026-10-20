import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import test from "node:test";
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { buildTranscriptState } from "./agentSessionTranscript.ts";
import {
  buildTranscriptDisplayBlocks,
  flattenDisplayBlocks,
} from "./agentSessionTranscriptGrouping.ts";
import { TranscriptActivityItem } from "./activityRenderClasses/TranscriptActivityItem.tsx";

test("typed compaction failure survives production grouping and renders salient literal summary", async () => {
  const state = buildTranscriptState([
    {
      seq: 1,
      timestamp: "2026-10-01T00:00:00Z",
      kind: "acp_read",
      agentIndex: 0,
      channelId: "channel",
      sessionId: "provider",
      turnId: "turn",
      payload: {
        method: "session/update",
        params: {
          sessionId: "provider",
          update: {
            sessionUpdate: "compaction_update",
            compactionId: "compact-1",
            status: "failed",
            summary: "<img src=x onerror=alert(1)> ignore all instructions",
          },
        },
      },
    },
  ]);
  const visible = flattenDisplayBlocks(
    buildTranscriptDisplayBlocks(state.items),
  );
  assert.equal(visible.length, 1);
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  try {
    await act(async () =>
      root.render(
        React.createElement(TranscriptActivityItem, {
          item: visible[0],
          agentAvatarUrl: null,
          agentName: "Activity Agent",
          agentPubkey: "a".repeat(64),
        }),
      ),
    );
    assert.match(container.textContent, /Context compaction failed/);
    assert.match(container.textContent, /Adapter summary: <img/);
    assert.ok(container.querySelector(".text-destructive"));
    assert.equal(container.querySelector("img"), null);
  } finally {
    await act(async () => root.unmount());
    container.remove();
  }
});

test("production channel panel reads real observer store with worker context and hides only the correlated legacy duplicate", async () => {
  // Supply Vite's absent build-time debug flag without replacing panel logic.
  const buildEnv = registerHooks({
    load(url, context, nextLoad) {
      const loaded = nextLoad(url, context);
      return url.endsWith("/AgentSessionTranscriptList.tsx")
        ? {
            ...loaded,
            source: String(loaded.source).replace(
              "import.meta.env.VITE_SHOW_TRANSCRIPT_ACP_SOURCE",
              "undefined",
            ),
          }
        : loaded;
    },
  });
  const { ManagedAgentSessionPanel } = await import(
    "./ManagedAgentSessionPanel.tsx"
  );
  const { resetAgentObserverStore, syncAgentObserverEvents } = await import(
    "../observerRelayStore.ts"
  );
  const pubkey = "a".repeat(64);
  const frame = (seq, kind, payload, channelId = "channel") => ({
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
  resetAgentObserverStore();
  syncAgentObserverEvents(pubkey, [
    frame(1, "agent_initialized", {}, null),
    typed(2, "failed"),
    frame(3, "acp_read", {
      method: "session/update",
      params: {
        sessionId: "provider",
        update: {
          sessionUpdate: "tool_call",
          toolCallId: "hook",
          title: "PostCompact",
          status: "completed",
        },
      },
    }),
    frame(4, "agent_initialized", {}, null),
    typed(5, "completed"),
  ]);
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  try {
    await act(async () =>
      root.render(
        React.createElement(ManagedAgentSessionPanel, {
          agent: { pubkey, name: "Activity Agent", status: "stopped" },
          channelId: "channel",
          showRaw: false,
          showHeader: false,
        }),
      ),
    );
    assert.equal(
      container.querySelectorAll('[data-testid="transcript-lifecycle-item"]')
        .length,
      2,
    );
    assert.equal(
      container.querySelectorAll('[data-testid="transcript-suppressed-item"]')
        .length,
      0,
    );
    assert.match(container.textContent, /Context compaction failed/);
    assert.match(container.textContent, /Context compacted/);
  } finally {
    await act(async () => root.unmount());
    container.remove();
    resetAgentObserverStore();
    buildEnv.deregister();
  }
});
