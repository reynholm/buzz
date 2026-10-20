import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { CANONICAL_CONTROL_ID } from "./sessionControl";
/**
 * Agent session status: strict parsing of the signed `buzz-session` tags,
 * English copy, and the per-transition projection used by the timeline.
 *
 * Wire format (produced by the agent CLI for the agent's own kind-9 messages):
 *
 * - `["buzz-session", state, reason, percent]`: `percent` is an integer
 *   0..100 or the empty string when occupancy is unavailable (never 0);
 * - `["buzz-session-info", "1", transitionUuid, "thread"|"conversation",
 *   read, total, "exact"|"lower_bound"|"unknown"]`: optional supporting
 *   metadata. `exact` means complete history (read and total exact);
 *   `lower_bound` means known-incomplete history (total, when present, is a
 *   proven lower bound); `unknown` makes no completeness claim.
 *
 * Status is self-reported by the signing agent. It is display metadata only:
 * not verified provider telemetry and not evidence of any permission. Raw tag
 * strings are never rendered; copy comes from the fixed tables below.
 */
import type {
  MessageSessionStatus,
  SessionStatusExplanation,
  TimelineMessage,
} from "@/features/messages/types";
import {
  getChannelIdFromTags,
  getThreadReference,
} from "@/features/messages/lib/threading";
import { KIND_STREAM_MESSAGE } from "@/shared/constants/kinds";
import { normalizePubkey } from "@/shared/lib/pubkey";

export const SESSION_TAG = "buzz-session";
export const SESSION_INFO_TAG = "buzz-session-info";
export const SESSION_INFO_VERSION = "1";
/** Maximum wire reason length in ASCII bytes (mirrors the producer). */
export const MAX_REASON_BYTES = 96;
/** Reported occupancy at or above this percentage is emphasized. */
export const CONTEXT_HIGHLIGHT_PERCENT = 80;

export type SessionState = "new" | "resumed" | "compacted" | "restarted";
export type SessionScopeKind = "thread" | "conversation";
export type SessionHistoryMode = "exact" | "lower_bound" | "unknown";

export type SessionPrimary = {
  state: SessionState;
  /** Well-formed wire reason; for matching only, never for display. */
  reason: string;
  percent: number | null;
};

export type SessionInfo = {
  transitionId: string;
  scope: SessionScopeKind;
  read: number | null;
  total: number | null;
  mode: SessionHistoryMode;
};

export type ParsedSessionTags = {
  primary: SessionPrimary;
  /** `null` when missing, malformed, of another version or conflicting. */
  info: SessionInfo | null;
  actorPubkey?: string;
};

const STATES: ReadonlySet<string> = new Set([
  "new",
  "resumed",
  "compacted",
  "restarted",
]);
const MODES: ReadonlySet<string> = new Set(["exact", "lower_bound", "unknown"]);
const REASON_RE = /^[a-z0-9_:-]+$/;
const PERCENT_RE = /^(?:100|[1-9]?[0-9])$/;
const COUNT_RE = /^(?:0|[1-9][0-9]*)$/;
const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const RESUME_FAILED_PREFIX = "resume_failed:";

function isStringTag(tag: unknown, name: string, length: number) {
  return (
    Array.isArray(tag) &&
    tag.length === length &&
    tag[0] === name &&
    tag.every((field) => typeof field === "string")
  );
}

function parseCount(field: string): number | null | undefined {
  if (field === "") return null;
  if (!COUNT_RE.test(field)) return undefined;
  const value = Number(field);
  return Number.isSafeInteger(value) ? value : undefined;
}

/** Parse the primary tag; anything outside the contract yields `null`. */
export function parseSessionPrimaryTag(tag: unknown): SessionPrimary | null {
  if (!isStringTag(tag, SESSION_TAG, 4)) return null;
  const [, state, reason, percent] = tag as string[];
  if (!STATES.has(state)) return null;
  if (reason.length > MAX_REASON_BYTES || !REASON_RE.test(reason)) return null;
  if (percent !== "" && !PERCENT_RE.test(percent)) return null;
  return {
    state: state as SessionState,
    reason,
    percent: percent === "" ? null : Number(percent),
  };
}

/** Parse the supporting tag; malformed or inconsistent metadata is `null`. */
export function parseSessionInfoTag(tag: unknown): SessionInfo | null {
  if (!isStringTag(tag, SESSION_INFO_TAG, 7)) return null;
  const [, version, transitionId, scope, readField, totalField, mode] =
    tag as string[];
  if (version !== SESSION_INFO_VERSION || !UUID_RE.test(transitionId)) {
    return null;
  }
  if ((scope !== "thread" && scope !== "conversation") || !MODES.has(mode)) {
    return null;
  }
  const read = parseCount(readField);
  const total = parseCount(totalField);
  if (read === undefined || total === undefined) return null;
  if (read !== null && total !== null && total < read) return null;
  if (mode === "exact" && (read === null || total === null)) return null;
  if (mode === "unknown" && total !== null) return null;
  return {
    transitionId,
    scope,
    read,
    total,
    mode: mode as SessionHistoryMode,
  };
}

function sameFields(a: readonly string[], b: readonly string[]) {
  return a.length === b.length && a.every((field, index) => field === b[index]);
}

/**
 * Collect every tag named `name`; all copies must be identical. Returns the
 * single tag, `undefined` when absent, or `null` on conflict.
 */
function uniqueTag(
  tags: readonly (readonly string[])[],
  name: string,
): readonly string[] | null | undefined {
  let found: readonly string[] | undefined;
  for (const tag of tags) {
    if (!Array.isArray(tag) || tag[0] !== name) continue;
    if (found && !sameFields(found, tag)) return null;
    found = tag;
  }
  return found;
}

/** Parse a message's status tags. Conflicting primary tags yield `null`. */
export function parseSessionStatusTags(
  tags: readonly (readonly string[])[] | undefined,
): ParsedSessionTags | null {
  if (!tags) return null;
  const primaryTag = uniqueTag(tags, SESSION_TAG);
  const primary = primaryTag ? parseSessionPrimaryTag(primaryTag) : null;
  if (!primary) return null;
  const infoTag = uniqueTag(tags, SESSION_INFO_TAG);
  const info = infoTag ? parseSessionInfoTag(infoTag) : null;
  const action = uniqueTag(tags, "buzz-session-action");
  const manual =
    primary.reason === "user_compact" ||
    /^user_restart:(none|recent|all)$/.test(primary.reason);
  const actorPubkey =
    manual &&
    info &&
    action &&
    action.length === 5 &&
    action[1] === "1" &&
    action[2] === info.transitionId &&
    CANONICAL_CONTROL_ID.test(action[3]) &&
    CANONICAL_CONTROL_ID.test(action[4])
      ? action[4]
      : null;
  return { primary, info, ...(actorPubkey ? { actorPubkey } : {}) };
}

// ---------------------------------------------------------------------------
// Copy (English, fixed tables)
// ---------------------------------------------------------------------------

const RESUMED_TITLES: Readonly<Record<string, string>> = {
  idle_sleep: "Resumed after sleep",
  harness_restart: "Resumed after restart",
  worker_crash: "Resumed after worker failure",
  worker_recycled: "Resumed after worker restart",
};

const NEW_DETAILS: Readonly<Record<string, string>> = {
  discarded: "previous session was discarded",
  config_changed: "agent configuration changed",
  live_elsewhere: "previous session is active elsewhere",
  resume_unsupported: "agent cannot resume sessions",
  context_overflow: "context window was full",
  max_turns: "turn limit reached",
};

const RESUME_FAILURE_DETAILS: Readonly<Record<string, string>> = {
  not_persisted: "previous session was not saved",
  store_unreadable: "saved session could not be read",
  store_busy: "session store was busy",
  not_found: "previous session not found",
  timeout: "timed out",
  protocol: "protocol error",
};

const RPC_CODE_RE = /^rpc_-?[0-9]{1,19}$/;

function hasOwn(table: Readonly<Record<string, string>>, key: string) {
  return Object.hasOwn(table, key);
}

function resumeFailureDetail(code: string): string {
  if (hasOwn(RESUME_FAILURE_DETAILS, code)) {
    return `resume failed: ${RESUME_FAILURE_DETAILS[code]}`;
  }
  if (RPC_CODE_RE.test(code)) return "resume failed: agent error";
  return "resume failed";
}

/**
 * Title and optional detail for a transition. Unknown but well-formed
 * reasons get neutral copy. Manual transitions may receive separately validated actor metadata.
 */
export function describeSessionTransition(
  primary: SessionPrimary,
): Omit<SessionStatusExplanation, "history"> {
  const { state, reason } = primary;
  if (
    state === "compacted" &&
    (reason === "user_compact" || reason === "auto_compact")
  )
    return {
      title:
        reason === "auto_compact"
          ? "Context compacted automatically"
          : "Context compacted",
    };
  if (state === "restarted" && /^user_restart:(none|recent|all)$/.test(reason))
    return {
      title: "Session started over",
      detail:
        reason === "user_restart:none"
          ? "from scratch"
          : reason === "user_restart:all"
            ? "with the entire thread"
            : "with recent history",
    };
  if (state === "resumed") {
    return {
      title: hasOwn(RESUMED_TITLES, reason)
        ? RESUMED_TITLES[reason]
        : "Resumed session",
    };
  }
  if (state !== "new") return { title: "Session updated" };
  if (reason.startsWith(RESUME_FAILED_PREFIX)) {
    return {
      title: "New session",
      detail: resumeFailureDetail(reason.slice(RESUME_FAILED_PREFIX.length)),
    };
  }
  if (hasOwn(NEW_DETAILS, reason)) {
    return { title: "New session", detail: NEW_DETAILS[reason] };
  }
  return { title: "New session" };
}

function messages(count: number) {
  return count === 1 ? "message" : "messages";
}

/**
 * Truthful cold-start history phrases. Exact: "read N of M messages"; proven
 * lower bound: "read N of at least M messages"; known incomplete without a
 * ratio: "Earlier history may be omitted"; no completeness claim: at most
 * the read count, without a ratio or omission note.
 */
export function describeHistory(info: SessionInfo): string[] {
  const { read, total, mode } = info;
  if (mode === "exact" && read !== null && total !== null) {
    return [`read ${read} of ${total} ${messages(total)}`];
  }
  if (mode === "lower_bound") {
    if (read !== null && total !== null) {
      return [`read ${read} of at least ${total} ${messages(total)}`];
    }
    const omitted = "Earlier history may be omitted";
    return read !== null
      ? [`read ${read} ${messages(read)}`, omitted]
      : [omitted];
  }
  return read !== null ? [`read ${read} ${messages(read)}`] : [];
}

export function isHighContext(percent: number): boolean {
  return percent >= CONTEXT_HIGHLIGHT_PERCENT;
}

/** Reported occupancy without an approximation sign. */
export function formatContextPercent(percent: number): string {
  return isHighContext(percent)
    ? `${percent}% context (high)`
    : `${percent}% context`;
}

// ---------------------------------------------------------------------------
// Projection
// ---------------------------------------------------------------------------

type Candidate = {
  message: TimelineMessage;
  parsed: ParsedSessionTags;
  key: string | null;
};

function isEarlier(a: TimelineMessage, b: TimelineMessage) {
  return a.createdAt !== b.createdAt ? a.createdAt < b.createdAt : a.id < b.id;
}

/**
 * Attach `sessionStatus` to agent messages, in place.
 *
 * `messages[i]` must be the formatted row of `events[i]`. Only an agent's
 * own, relay-accepted kind-9 message whose signer is the represented author
 * is considered, and only its original signed tags are read: edits cannot
 * add or change status.
 *
 * The explanation belongs to the earliest loaded message, by
 * `(createdAt, id)`, of one transition: (signer, channel, scope kind and
 * thread root, transition UUID). Every other message keeps only its
 * percentage. Without valid supporting metadata there is no grouping key,
 * so each such message keeps its primary explanation without history counts.
 * The result depends only on the loaded events: no "already seen" state is
 * kept anywhere.
 */
export function attachSessionStatuses(
  messages: TimelineMessage[],
  events: ReadonlyArray<{
    kind: number;
    pending?: boolean;
    tags: string[][];
  }>,
  fallbackChannelId: string | null,
  profiles?: UserProfileLookup,
): TimelineMessage[] {
  const candidates: Candidate[] = [];
  const owners = new Map<string, TimelineMessage>();
  messages.forEach((message, index) => {
    const event = events[index];
    if (
      !event ||
      event.kind !== KIND_STREAM_MESSAGE ||
      event.pending ||
      !message.isAgent ||
      !message.pubkey ||
      !message.signerPubkey ||
      normalizePubkey(message.pubkey) !== message.signerPubkey
    ) {
      return;
    }
    const parsed = parseSessionStatusTags(event.tags);
    if (!parsed) return;
    let key: string | null = null;
    const { info } = parsed;
    const channelId = getChannelIdFromTags(event.tags) ?? fallbackChannelId;
    const rootId = getThreadReference(event.tags).rootId;
    if (info && channelId && (info.scope === "conversation" || rootId)) {
      key = [
        message.signerPubkey,
        channelId,
        info.scope,
        info.scope === "thread" ? rootId : "",
        info.transitionId,
        parsed.primary.state,
        parsed.primary.reason,
        parsed.actorPubkey ?? "",
      ].join("\u0000");
      const owner = owners.get(key);
      if (!owner || isEarlier(message, owner)) owners.set(key, message);
    }
    candidates.push({ message, parsed, key });
  });

  for (const { message, parsed, key } of candidates) {
    const status: MessageSessionStatus = {};
    if (key === null || owners.get(key) === message) {
      const explanation: SessionStatusExplanation = describeSessionTransition(
        parsed.primary,
      );
      const actorName = parsed.actorPubkey
        ? profiles?.[parsed.actorPubkey]?.displayName
        : null;
      if (
        actorName &&
        (parsed.primary.state === "restarted" ||
          parsed.primary.state === "compacted")
      )
        explanation.title += ` by ${actorName}`;
      const history =
        key !== null && parsed.info ? describeHistory(parsed.info) : [];
      if (history.length > 0) explanation.history = history;
      status.explanation = explanation;
    }
    if (parsed.primary.percent !== null) {
      status.percent = parsed.primary.percent;
    }
    if (status.explanation || status.percent !== undefined) {
      message.sessionStatus = status;
    }
  }
  return messages;
}
