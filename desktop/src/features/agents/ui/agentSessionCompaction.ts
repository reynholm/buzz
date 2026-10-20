import type { ObserverEvent, TranscriptItem } from "./agentSessionTypes";
import { asRecord } from "./agentSessionUtils";

export type CompactionUpdate = {
  sessionId: string;
  compactionId: string;
  status: "in_progress" | "completed" | "failed" | "cancelled";
  summary: string | null;
};
export type CompactionActivity = CompactionUpdate & {
  /** Observed worker/attachment boundary, never the backend private tracker UUID. */
  generation: string;
  origin: "manual" | "automatic" | "unknown";
  conflict: boolean;
  scope: string;
  summaryAt: Position | null;
  firstSeen: Position;
};
type Position = { timestamp: string; seq: number };
type Boundary = Position & {
  agent: number | null;
  session: string | null;
  id: string;
};
type PromptEvidence = Position & {
  scope: string;
  origin: CompactionActivity["origin"];
};
export type CompactionActivityState = {
  boundaries: Boundary[];
  prompts: PromptEvidence[];
  pendingAttachments: (Position & {
    agent: number | null;
    requestId: number;
    session: string | null;
  })[];
};
const MAX_TRACKED = 128;
const MAX_SUMMARY = 1000;
const utf8 = new TextEncoder();
const nonempty = (value: unknown): value is string =>
  typeof value === "string" && value.length > 0;

/** Decode the actual ACP notification; capabilities and unknown frames are not outcomes. */
export function parseCompactionUpdate(
  payload: unknown,
): CompactionUpdate | null {
  const rpc = asRecord(payload);
  const params = asRecord(rpc.params);
  const update = asRecord(params.update);
  const status = update.status;
  if (
    rpc.method !== "session/update" ||
    update.sessionUpdate !== "compaction_update" ||
    !nonempty(params.sessionId) ||
    !nonempty(update.compactionId) ||
    update.compactionId.length > 128 ||
    utf8.encode(update.compactionId).length > 128 ||
    typeof status !== "string" ||
    !["in_progress", "completed", "failed", "cancelled"].includes(status)
  )
    return null;
  return {
    sessionId: params.sessionId,
    compactionId: update.compactionId,
    status: status as CompactionUpdate["status"],
    summary:
      typeof update.summary === "string"
        ? update.summary.slice(0, MAX_SUMMARY)
        : null,
  };
}

/** Small bounded observation index; the observer store owns the retained event window. */
export function emptyCompactionActivityState(): CompactionActivityState {
  return { boundaries: [], prompts: [], pendingAttachments: [] };
}
function compare(a: Position, b: Position) {
  return Date.parse(a.timestamp) - Date.parse(b.timestamp) || a.seq - b.seq;
}
function rememberBoundary(state: CompactionActivityState, boundary: Boundary) {
  if (
    state.boundaries.some(
      (entry) => entry.id === boundary.id && entry.agent === boundary.agent,
    )
  )
    return state;
  return {
    ...state,
    boundaries: [...state.boundaries, boundary]
      .sort(compare)
      .slice(-MAX_TRACKED),
  };
}
function generation(
  state: CompactionActivityState,
  event: ObserverEvent,
  session: string,
) {
  return (
    state.boundaries
      .filter(
        (boundary) =>
          boundary.agent === event.agentIndex &&
          (boundary.session === null || boundary.session === session) &&
          compare(boundary, event) <= 0,
      )
      .at(-1)?.id ?? "unknown"
  );
}
function scope(
  state: CompactionActivityState,
  event: ObserverEvent,
  session: string,
) {
  return JSON.stringify([
    event.agentIndex,
    generation(state, event, session),
    session,
    event.channelId,
    event.turnId,
  ]);
}
function lifecycle(
  item: Extract<TranscriptItem, { type: "lifecycle" }>,
  activity: CompactionActivity,
) {
  const title = activity.conflict
    ? "Context compaction has conflicting outcomes"
    : activity.status === "in_progress"
      ? "Compacting context"
      : activity.status === "completed"
        ? "Context compacted"
        : activity.status === "failed"
          ? "Context compaction failed"
          : "Context compaction cancelled";
  const provenance =
    activity.origin === "manual"
      ? "Requested compaction"
      : activity.origin === "automatic"
        ? "Automatic compaction"
        : "Compaction origin unknown";
  return {
    ...item,
    title,
    renderClass:
      activity.status === "failed" || activity.conflict
        ? ("error" as const)
        : ("status" as const),
    text: [
      provenance,
      activity.summary ? `Adapter summary: ${activity.summary}` : null,
    ]
      .filter(Boolean)
      .join(" · "),
    compaction: activity,
  };
}

/** Project typed lifecycle and observed prompt provenance without treating summaries as authority. */
export function processCompactionActivity(
  state: CompactionActivityState,
  event: ObserverEvent,
  items: readonly TranscriptItem[],
): { state: CompactionActivityState; upserts: TranscriptItem[] } {
  const rpc = asRecord(event.payload);
  const params = asRecord(rpc.params);
  const session = nonempty(params.sessionId)
    ? params.sessionId
    : event.sessionId;
  let next = state;
  const upserts: TranscriptItem[] = [];
  let affectedScope: string | null = null;
  if (event.kind === "agent_initialized") {
    next = {
      ...next,
      pendingAttachments: next.pendingAttachments.filter(
        (pending) =>
          pending.agent !== event.agentIndex || compare(pending, event) > 0,
      ),
    };
  }
  if (
    event.kind === "acp_write" &&
    (rpc.method === "session/new" ||
      (rpc.method === "session/resume" && nonempty(params.sessionId))) &&
    Number.isSafeInteger(rpc.id)
  ) {
    const requestId = rpc.id as number;
    const newer = next.pendingAttachments.some(
      (entry) =>
        entry.agent === event.agentIndex &&
        entry.requestId === requestId &&
        compare(entry, event) > 0,
    );
    if (!newer)
      next = {
        ...next,
        pendingAttachments: [
          ...next.pendingAttachments.filter(
            (pending) =>
              pending.agent !== event.agentIndex ||
              pending.requestId !== requestId,
          ),
          {
            agent: event.agentIndex,
            requestId,
            session:
              rpc.method === "session/resume"
                ? (params.sessionId as string)
                : null,
            timestamp: event.timestamp,
            seq: event.seq,
          },
        ]
          .sort(compare)
          .slice(-MAX_TRACKED),
      };
  }
  if (
    event.kind === "acp_read" &&
    rpc.method === undefined &&
    Number.isSafeInteger(rpc.id)
  ) {
    const pending = next.pendingAttachments.find(
      (entry) =>
        entry.agent === event.agentIndex &&
        entry.requestId === rpc.id &&
        compare(entry, event) <= 0,
    );
    if (pending) {
      next = {
        ...next,
        pendingAttachments: next.pendingAttachments.filter(
          (entry) => entry !== pending,
        ),
      };
      const returnedSession = asRecord(rpc.result).sessionId;
      if (pending.session && rpc.error !== undefined) {
        // Resume installs its route before I/O and clears it on failure.
        next = rememberBoundary(next, {
          agent: event.agentIndex,
          session: pending.session,
          timestamp: event.timestamp,
          seq: event.seq,
          id: `unbound:session/resume:${pending.timestamp}:${pending.seq}:${event.timestamp}:${event.seq}`,
        });
      }
      if (
        !pending.session &&
        rpc.error === undefined &&
        nonempty(returnedSession)
      ) {
        // The backend binds a new route only after this successful ACK. Typed
        // pre-ACK updates still belong to an old route (or unknown history);
        // only command advertisements are staged by the creation tracker.
        next = rememberBoundary(next, {
          agent: event.agentIndex,
          session: returnedSession,
          timestamp: event.timestamp,
          seq: event.seq,
          id: `session/new:${pending.timestamp}:${pending.seq}:${event.timestamp}:${event.seq}`,
        });
      }
    }
  }
  if (
    event.kind === "agent_initialized" ||
    (event.kind === "acp_write" &&
      rpc.method === "session/resume" &&
      nonempty(params.sessionId))
  ) {
    const boundary: Boundary = {
      timestamp: event.timestamp,
      seq: event.seq,
      agent: event.agentIndex,
      session: event.kind === "agent_initialized" ? null : session,
      id: `${event.kind}:${event.timestamp}:${event.seq}`,
    };
    next = rememberBoundary(next, boundary);
  }
  if (
    event.kind === "acp_write" &&
    rpc.method === "session/prompt" &&
    session &&
    event.turnId
  ) {
    const blocks = Array.isArray(params.prompt) ? params.prompt : [];
    const exactManual =
      blocks.length === 1 &&
      asRecord(blocks[0]).type === "text" &&
      asRecord(blocks[0]).text === "/compact";
    const ordinary = blocks.some((block) => {
      const text = asRecord(block).text;
      return (
        asRecord(block).type === "text" &&
        typeof text === "string" &&
        text.startsWith("[Buzz event:")
      );
    });
    const evidence: PromptEvidence = {
      timestamp: event.timestamp,
      seq: event.seq,
      scope: scope(next, event, session),
      origin: exactManual ? "manual" : ordinary ? "automatic" : "unknown",
    };
    if (evidence.origin !== "unknown") {
      next = {
        ...next,
        prompts: [
          ...next.prompts.filter((entry) => entry.scope !== evidence.scope),
          evidence,
        ]
          .sort(compare)
          .slice(-MAX_TRACKED),
      };
      affectedScope = evidence.scope;
    }
  }
  const update =
    event.kind === "acp_read" ? parseCompactionUpdate(event.payload) : null;
  if (update) {
    const workerGeneration = generation(next, event, update.sessionId);
    const operationScope = scope(next, event, update.sessionId);
    const id = `compaction:${JSON.stringify([event.agentIndex, workerGeneration, update.sessionId, update.compactionId])}`;
    const previous = items.find((item) => item.id === id);
    const prior =
      previous?.type === "lifecycle" ? previous.compaction : undefined;
    const terminal = prior && prior.status !== "in_progress";
    const conflict = Boolean(
      prior?.conflict ||
        (terminal &&
          update.status !== "in_progress" &&
          update.status !== prior.status),
    );
    const acceptSummary =
      update.summary !== null &&
      (!terminal || update.status === prior.status) &&
      (!prior?.summaryAt || compare(prior.summaryAt, event) <= 0);
    const activity: CompactionActivity = {
      ...update,
      generation: workerGeneration,
      scope: prior?.scope ?? operationScope,
      status: terminal ? prior.status : update.status,
      summary: acceptSummary ? update.summary : (prior?.summary ?? null),
      summaryAt: acceptSummary
        ? { timestamp: event.timestamp, seq: event.seq }
        : (prior?.summaryAt ?? null),
      origin: prior?.origin ?? "unknown",
      firstSeen:
        prior && compare(prior.firstSeen, event) <= 0
          ? prior.firstSeen
          : { timestamp: event.timestamp, seq: event.seq },
      conflict,
    };
    const item: Extract<TranscriptItem, { type: "lifecycle" }> =
      previous?.type === "lifecycle"
        ? previous
        : {
            id,
            type: "lifecycle",
            renderClass: "status",
            title: "",
            text: "",
            timestamp: event.timestamp,
            channelId: event.channelId,
            turnId: event.turnId,
            sessionId: update.sessionId,
            acpSource: "compaction_update",
          };
    upserts.push(lifecycle(item, activity));
    affectedScope = activity.scope;
  }
  if (affectedScope) {
    const evidence = next.prompts
      .filter((prompt) => prompt.scope === affectedScope)
      .at(-1);
    if (evidence) {
      const candidates = [
        ...new Map(
          [...items, ...upserts].map((item) => [item.id, item]),
        ).values(),
      ]
        .filter(
          (
            item,
          ): item is Extract<TranscriptItem, { type: "lifecycle" }> & {
            compaction: CompactionActivity;
          } =>
            item.type === "lifecycle" &&
            item.compaction?.scope === affectedScope,
        )
        .sort((a, b) =>
          compare(a.compaction.firstSeen, b.compaction.firstSeen),
        );
      const firstNew = candidates.find(
        (item) => compare(evidence, item.compaction.firstSeen) <= 0,
      );
      for (const item of candidates) {
        // The backend manual watch claims one newly observed ID, not every
        // operation in the turn. Later IDs have no manual attribution evidence.
        const attribution =
          compare(evidence, item.compaction.firstSeen) <= 0 &&
          (evidence.origin !== "manual" || item.id === firstNew?.id)
            ? evidence.origin
            : "unknown";
        if (item.compaction.origin !== attribution)
          upserts.push(
            lifecycle(item, { ...item.compaction, origin: attribution }),
          );
      }
    }
  }
  const toolUpdate = asRecord(params.update);
  const hookId = `tool:${event.channelId ?? "global"}:${toolUpdate.toolCallId}`;
  const currentHook =
    event.kind === "acp_read" &&
    rpc.method === "session/update" &&
    (toolUpdate.sessionUpdate === "tool_call" ||
      toolUpdate.sessionUpdate === "tool_call_update")
      ? items.find(
          (item) =>
            item.id === hookId &&
            item.type === "tool" &&
            item.descriptor.groupKey === "status:post-compact",
        )
      : null;
  if (!update && !currentHook) return { state: next, upserts };
  // Keep the legacy fallback unless this exact observed scope also has a typed row.
  const typedScopes = new Set(
    [...items, ...upserts].flatMap((item) =>
      item.type === "lifecycle" && item.compaction
        ? [item.compaction.scope]
        : [],
    ),
  );
  for (const item of update ? items : currentHook ? [currentHook] : []) {
    if (
      item.type !== "tool" ||
      item.descriptor.groupKey !== "status:post-compact"
    )
      continue;
    const isCurrentHook =
      item.timestamp === event.timestamp &&
      item.channelId === event.channelId &&
      item.turnId === event.turnId;
    const hookScope =
      item.compactionScope ??
      (session && isCurrentHook ? scope(next, event, session) : null);
    if (!hookScope) continue;
    if (typedScopes.has(hookScope)) {
      if (item.renderClass !== "suppressed")
        upserts.push({
          ...item,
          compactionScope: hookScope,
          renderClass: "suppressed",
          descriptor: {
            ...item.descriptor,
            renderClass: "suppressed",
            reason: "typed-compaction-duplicate",
          },
        });
    } else if (!item.compactionScope) {
      upserts.push({ ...item, compactionScope: hookScope });
    }
  }
  return { state: next, upserts };
}
