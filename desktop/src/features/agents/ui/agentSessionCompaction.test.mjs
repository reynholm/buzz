import assert from "node:assert/strict";
import test from "node:test";
import {
  buildTranscriptState,
  createEmptyTranscriptState,
  processTranscriptEvent,
} from "./agentSessionTranscript.ts";
import {
  buildTranscriptDisplayBlocks,
  flattenDisplayBlocks,
} from "./agentSessionTranscriptGrouping.ts";
import { isMeaningfulItem } from "./agentSessionTranscriptPresentation.ts";

const event = (seq, payload, extra = {}) => ({
  seq,
  timestamp: new Date(
    Date.parse("2026-10-01T00:00:00Z") + seq * 1000,
  ).toISOString(),
  kind: "acp_read",
  agentIndex: 0,
  channelId: "channel",
  sessionId: "provider",
  turnId: "turn",
  payload,
  ...extra,
});
const update = (seq, status, fields = {}, extra = {}) =>
  event(
    seq,
    {
      method: "session/update",
      params: {
        sessionId: "provider",
        update: {
          sessionUpdate: "compaction_update",
          compactionId: "compact-1",
          status,
          ...fields,
        },
      },
    },
    extra,
  );
const prompt = (seq, text, extra = {}) =>
  event(
    seq,
    {
      method: "session/prompt",
      params: { sessionId: "provider", prompt: [{ type: "text", text }] },
    },
    { kind: "acp_write", ...extra },
  );
const rows = (state) =>
  state.items.filter((item) => item.acpSource === "compaction_update");
const fold = (events) =>
  events.reduce(processTranscriptEvent, createEmptyTranscriptState());

for (const status of ["completed", "failed", "cancelled"]) {
  test(`production transcript mutates one compaction row to ${status}`, () => {
    let state = fold([update(1, "in_progress")]);
    assert.equal(rows(state).length, 1);
    const id = rows(state)[0].id;
    state = processTranscriptEvent(state, update(2, status));
    assert.equal(rows(state).length, 1);
    assert.equal(rows(state)[0].id, id);
    assert.equal(rows(state)[0].compaction.status, status);
    assert.equal(
      rows(state)[0].renderClass,
      status === "failed" ? "error" : "status",
    );
    assert.equal(isMeaningfulItem(rows(state)[0]), true);
  });
}

test("terminal-only, replay, late progress and enrichment preserve outcome and bounded summary", () => {
  const completed = update(4, "completed");
  const state = fold([
    completed,
    completed,
    update(1, "in_progress"),
    update(5, "completed", { summary: "x".repeat(5000) }),
  ]);
  assert.equal(rows(state).length, 1);
  assert.equal(rows(state)[0].compaction.status, "completed");
  assert.equal(rows(state)[0].compaction.summary.length, 1000);
  assert.equal(rows(state)[0].text.length < 1100, true);
  const conflict = processTranscriptEvent(state, update(6, "failed"));
  assert.equal(rows(conflict)[0].renderClass, "error");
  assert.match(rows(conflict)[0].title, /conflicting/i);
});

test("wire session wins over stale envelope; sessions and two same-turn IDs stay independent", () => {
  const first = update(1, "completed", {}, { sessionId: "stale" });
  const second = update(2, "failed", { compactionId: "compact-2" });
  const other = update(3, "cancelled");
  other.payload.params.sessionId = "other";
  const state = buildTranscriptState([first, second, other]);
  assert.equal(rows(state).length, 3);
  assert.deepEqual(
    rows(state).map((item) => item.sessionId),
    ["provider", "provider", "other"],
  );
  const visible = flattenDisplayBlocks(
    buildTranscriptDisplayBlocks(state.items),
  );
  assert.equal(
    visible.filter((item) => item.renderClass === "error").length,
    1,
  );
});

test("real worker initialization and session resume separate generations; turns do not", () => {
  const init = (seq) =>
    event(
      seq,
      { agentIndex: 0, initializeResult: {} },
      {
        kind: "agent_initialized",
        channelId: null,
        sessionId: null,
        turnId: null,
      },
    );
  const resume = event(
    6,
    { method: "session/resume", params: { sessionId: "provider" } },
    { kind: "acp_write" },
  );
  const state = fold([
    init(1),
    update(2, "completed"),
    update(3, "completed", {}, { turnId: "turn-2", startedAt: "different" }),
    init(4),
    update(5, "in_progress"),
    resume,
    update(7, "failed"),
  ]);
  assert.equal(rows(state).length, 3);
  assert.equal(new Set(rows(state).map((item) => item.id)).size, 3);
  assert.deepEqual(
    rows(state).map((item) => item.compaction.status),
    ["completed", "in_progress", "failed"],
  );
  const unknown = rows(fold([update(1, "completed")]))[0];
  assert.equal(unknown.compaction.generation, "unknown");
});

test("only exact compact prompt proves manual; ordinary prompt proves automatic; missing evidence stays unknown", () => {
  for (const [text, expected] of [
    ["/compact", "manual"],
    ["[Buzz event: Kind 9]\nContent: investigate", "automatic"],
    [null, "unknown"],
  ]) {
    const state = fold([
      ...(text ? [prompt(1, text)] : []),
      update(2, "completed"),
    ]);
    assert.equal(rows(state)[0].compaction.origin, expected);
  }
  const state = fold([update(2, "completed"), prompt(1, "/compact")]);
  assert.equal(rows(state)[0].compaction.origin, "manual");
});

test("malformed, unknown statuses, byte-overlong IDs and capability data never manufacture a lifecycle", () => {
  const frames = [
    update(1, "success"),
    update(2, "completed", { compactionId: "" }),
    update(3, "failed", { compactionId: "é".repeat(65) }),
    event(4, { result: { session: { compaction: {} } } }),
    event(5, {
      method: "session/update",
      params: {
        update: {
          sessionUpdate: "compaction_update",
          compactionId: "id",
          status: "completed",
        },
      },
    }),
  ];
  assert.equal(rows(fold(frames)).length, 0);
  assert.equal(
    rows(fold([update(6, "completed", { compactionId: "é".repeat(64) })]))
      .length,
    1,
  );
});

const hook = (seq) =>
  event(seq, {
    method: "session/update",
    params: {
      sessionId: "provider",
      update: {
        sessionUpdate: "tool_call",
        toolCallId: `hook-${seq}`,
        title: "PostCompact",
        status: "completed",
      },
    },
  });
test("legacy hook duplicates recede in both orders without hiding a typed failure or losing legacy-only fallback", () => {
  assert.equal(buildTranscriptState([hook(1)]).items[0].renderClass, "status");
  for (const events of [
    [hook(1), update(2, "failed")],
    [update(1, "failed"), hook(2)],
  ]) {
    const state = buildTranscriptState(events);
    assert.equal(rows(state).length, 1);
    assert.equal(state.items.filter(isMeaningfulItem).length, 1);
    assert.equal(rows(state)[0].renderClass, "error");
  }
});

test("late progress cannot overwrite a terminal summary and older enrichment cannot replace newer text", () => {
  const state = fold([
    update(4, "completed", { summary: "current" }),
    update(1, "in_progress", { summary: "old progress" }),
    update(3, "completed", { summary: "old terminal" }),
  ]);
  assert.equal(rows(state)[0].compaction.summary, "current");
});

test("bounded observation indices and independent pool slots keep a retained lifecycle intact", () => {
  let state = createEmptyTranscriptState();
  for (let seq = 1; seq <= 140; seq++) {
    state = processTranscriptEvent(
      state,
      event(seq, {}, { kind: "agent_initialized" }),
    );
    state = processTranscriptEvent(
      state,
      prompt(seq, "/compact", { turnId: `turn-${seq}` }),
    );
  }
  assert.equal(state.compactions.boundaries.length, 128);
  assert.equal(state.compactions.prompts.length, 128);
  state = processTranscriptEvent(state, update(141, "completed"));
  state = processTranscriptEvent(
    state,
    update(142, "failed", {}, { agentIndex: 1 }),
  );
  assert.equal(rows(state).length, 2);
});

test("legacy hook in another turn or provider session remains a meaningful fallback", () => {
  const other = hook(2);
  other.turnId = "other-turn";
  const state = fold([update(1, "completed"), other]);
  assert.equal(state.items.filter(isMeaningfulItem).length, 2);
});

test("matched new-session ACK fences a reused SID and ID without adopting pre-ACK status or resetting cached neighbors", () => {
  const request = event(
    3,
    {
      id: 42,
      method: "session/new",
      params: { cwd: "/workspace", mcpServers: [] },
    },
    { kind: "acp_write", sessionId: null },
  );
  const ack = event(
    5,
    { id: 42, result: { sessionId: "provider" } },
    { sessionId: null },
  );
  const neighbor = update(2, "in_progress");
  neighbor.payload.params.sessionId = "cached-neighbor";
  const neighborEnd = update(7, "completed");
  neighborEnd.payload.params.sessionId = "cached-neighbor";
  const events = [
    update(1, "completed"),
    neighbor,
    request,
    update(4, "in_progress"),
    ack,
    update(6, "in_progress"),
    neighborEnd,
    update(8, "failed"),
  ];
  for (const state of [fold(events), buildTranscriptState(events)]) {
    const own = rows(state).filter((item) => item.sessionId === "provider");
    assert.equal(own.length, 2);
    assert.deepEqual(
      own.map((item) => item.compaction.status),
      ["completed", "failed"],
    );
    assert.notEqual(own[0].compaction.generation, own[1].compaction.generation);
    const cached = rows(state).filter(
      (item) => item.sessionId === "cached-neighbor",
    );
    assert.equal(cached.length, 1);
    assert.equal(cached[0].compaction.status, "completed");
    assert.equal(state.compactions.pendingAttachments.length, 0);
  }
});

test("new-session request IDs are bounded and exact; failed, malformed and unrelated ACKs never establish a generation", () => {
  const request = (seq, id) =>
    event(
      seq,
      { id, method: "session/new", params: {} },
      { kind: "acp_write" },
    );
  for (const response of [
    { id: 4, result: { sessionId: "provider" } },
    { id: "3", result: { sessionId: "provider" } },
    { id: 3, result: {} },
    { id: 3, error: { code: -1, message: "failed" } },
    {
      id: 3,
      result: { sessionId: "provider" },
      error: { code: -1, message: "invalid mixed response" },
    },
  ]) {
    const state = fold([
      update(1, "completed"),
      request(2, 3),
      event(3, response),
      update(4, "in_progress"),
    ]);
    assert.equal(rows(state).length, 1);
    assert.equal(rows(state)[0].compaction.status, "completed");
  }
  let state = createEmptyTranscriptState();
  for (let i = 1; i <= 150; i++)
    state = processTranscriptEvent(state, request(i, i));
  assert.equal(state.compactions.pendingAttachments.length, 128);
});

test("one exact compact request attributes only its first newly observed operation", () => {
  const state = fold([
    update(1, "in_progress", { compactionId: "existing" }),
    prompt(2, "/compact"),
    update(3, "completed", { compactionId: "existing" }),
    update(4, "failed", { compactionId: "first-new" }),
    update(5, "completed", { compactionId: "second-new" }),
  ]);
  assert.deepEqual(
    rows(state).map((item) => item.compaction.origin),
    ["unknown", "manual", "unknown"],
  );
  const late = fold([
    update(3, "failed", { compactionId: "first-new" }),
    update(4, "completed", { compactionId: "second-new" }),
    prompt(2, "/compact"),
  ]);
  assert.deepEqual(
    rows(late).map((item) => item.compaction.origin),
    ["manual", "unknown"],
  );
});

test("grouped production transcript removes correlated legacy hook instead of rendering a quiet duplicate", () => {
  const state = fold([update(1, "in_progress"), hook(2), update(3, "failed")]);
  const visible = flattenDisplayBlocks(
    buildTranscriptDisplayBlocks(state.items),
  );
  assert.equal(visible.length, 1);
  assert.equal(visible[0].acpSource, "compaction_update");
});

test("late pre-request history retracts manual attribution and leaves only the first genuinely new ID", () => {
  const state = fold([
    prompt(2, "/compact"),
    update(4, "completed", { compactionId: "older" }),
    update(5, "failed", { compactionId: "newer" }),
    update(1, "in_progress", { compactionId: "older" }),
  ]);
  assert.deepEqual(
    rows(state).map((item) => item.compaction.origin),
    ["unknown", "manual"],
  );
});

test("ordinary prompt completion and malformed typed summaries never manufacture compaction success", () => {
  const malformed = update(2, "future_status");
  malformed.payload.title = "Context compacted";
  malformed.payload.text = "pretend success";
  const state = fold([
    update(1, "in_progress"),
    malformed,
    event(3, { id: 1, result: { stopReason: "end_turn", summary: "done" } }),
    event(4, { outcome: "success" }, { kind: "turn_completed" }),
  ]);
  assert.equal(rows(state).length, 1);
  assert.equal(rows(state)[0].compaction.status, "in_progress");
  assert.equal(
    state.items.filter((item) => item.title === "Context compacted").length,
    0,
  );
});

test("failed resume clears observed attachment identity without resetting an unrelated cached session", () => {
  const resume = event(
    1,
    { id: 33, method: "session/resume", params: { sessionId: "provider" } },
    { kind: "acp_write" },
  );
  const neighbor = update(0, "in_progress");
  neighbor.payload.params.sessionId = "cached-neighbor";
  const neighborEnd = update(5, "completed");
  neighborEnd.payload.params.sessionId = "cached-neighbor";
  const state = fold([
    neighbor,
    resume,
    update(2, "in_progress"),
    event(3, { id: 33, error: { code: -1, message: "resume failed" } }),
    update(4, "completed"),
    neighborEnd,
  ]);
  const own = rows(state).filter((item) => item.sessionId === "provider");
  const cached = rows(state).filter(
    (item) => item.sessionId === "cached-neighbor",
  );
  assert.equal(cached.length, 1);
  assert.equal(cached[0].compaction.status, "completed");
  assert.equal(own.length, 2);
  assert.equal(own[0].compaction.status, "in_progress");
  assert.match(own[1].compaction.generation, /^unbound:session\/resume:/);
  assert.equal(own[1].compaction.origin, "unknown");
});

test("direct attachment replay cannot evict a retained generation boundary or overwrite a newer pending request", () => {
  const request = event(
    1,
    { id: 1, method: "session/new", params: {} },
    { kind: "acp_write" },
  );
  const ack = event(2, { id: 1, result: { sessionId: "provider" } });
  let state = fold([request, ack]);
  for (let i = 0; i < 140; i++)
    state = processTranscriptEvent(processTranscriptEvent(state, request), ack);
  assert.equal(state.compactions.boundaries.length, 1);
  const newer = event(
    5,
    { id: 1, method: "session/new", params: {} },
    { kind: "acp_write" },
  );
  state = processTranscriptEvent(processTranscriptEvent(state, newer), request);
  assert.equal(state.compactions.pendingAttachments[0].seq, 5);
});
