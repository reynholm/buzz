import type { TimelineMessage } from "@/features/messages/types";
import type { RelayEvent } from "@/shared/api/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";

export type SessionCommand = {
  type: "compact" | "restart";
  depth?: "none" | "recent" | "all";
  content: "!compact" | "!restart" | "!restart none" | "!restart all";
};
/** Rust str::trim's Unicode White_Space set; JS trim differs at NEL and BOM. */
const EXTERNAL_WHITESPACE = /^\p{White_Space}+|\p{White_Space}+$/gu;
export const CANONICAL_CONTROL_ID = /^[0-9a-f]{64}$/;
/** Parse the five spellings consumed by the harness, without altering ordinary text. */
export function parseSessionCommand(content: string): SessionCommand | null {
  switch (content.replace(EXTERNAL_WHITESPACE, "")) {
    case "!compact":
      return { type: "compact", content: "!compact" };
    case "!restart":
    case "!rotate":
      return { type: "restart", depth: "recent", content: "!restart" };
    case "!restart none":
      return { type: "restart", depth: "none", content: "!restart none" };
    case "!restart all":
      return { type: "restart", depth: "all", content: "!restart all" };
    default:
      return null;
  }
}
export type SessionControlTarget = {
  pubkey: string;
  name: string;
  state:
    | "publishing"
    | "pending"
    | "expired"
    | "unconfirmed"
    | "success"
    | "failure";
  detail: string;
};
export type MessageSessionControl = {
  action: string;
  targets: SessionControlTarget[];
};
const FAILURE_COPY: Readonly<Record<string, string>> = {
  no_session: "No existing session",
  unsupported: "Compaction is unavailable",
  scope_unavailable: "Session scope unavailable",
  busy: "Session control is busy",
  stale_target: "Session changed",
  stale_request: "Request expired",
  store_unavailable: "Session could not be saved",
  resume_failed: "Session could not resume",
  compact_failed: "Compaction failed",
  cancelled: "Request cancelled",
  timeout: "Request timed out",
  unconfirmed: "Result not confirmed",
  superseded: "Replaced by another request",
  internal: "Session control failed",
};
function parseResult(event: RelayEvent, commandId: string): string | null {
  const tags = event.tags.filter((tag) => tag[0] === "buzz-session-result");
  if (!tags.length) return null;
  const first = tags[0];
  if (
    tags.some(
      (tag) => tag.length !== 5 || tag.some((value, i) => value !== first[i]),
    ) ||
    first[1] !== "1" ||
    first[2] !== commandId
  )
    return null;
  const [, , , outcome, code] = first;
  if (outcome === "ok" && code === "" && event.content === "✅") return "ok";
  if (outcome === "pending" && code === "" && event.content === "⏳")
    return "pending";
  if (
    outcome === "error" &&
    Object.hasOwn(FAILURE_COPY, code) &&
    event.content === "❌"
  )
    return code;
  return null;
}
function targetResult(
  command: RelayEvent,
  target: string,
  reactions: readonly RelayEvent[],
  now: number,
): Pick<SessionControlTarget, "state" | "detail"> {
  if (command.pending)
    return { state: "publishing", detail: "Sending request…" };
  const reports = reactions.filter(
    (event) =>
      event.kind === 7 &&
      !event.pending &&
      event.pubkey === target &&
      event.tags.some((tag) => tag[0] === "buzz-session-result") &&
      event.tags.some((tag) => tag[0] === "e" && tag[1] === command.id),
  );
  const outcomes = reports.map((event) => parseResult(event, command.id));
  const terminals = new Set(
    outcomes.filter((outcome) => outcome !== "pending"),
  );
  if (terminals.has(null) || terminals.size > 1)
    return { state: "unconfirmed", detail: "Result not confirmed" };
  const terminal = [...terminals][0];
  if (terminal === "ok") return { state: "success", detail: "Completed" };
  if (terminal)
    return {
      state: terminal === "unconfirmed" ? "unconfirmed" : "failure",
      detail: FAILURE_COPY[terminal],
    };
  if (outcomes.includes("pending"))
    return { state: "pending", detail: "Waiting for current work to stop" };
  return now - command.created_at > 900
    ? { state: "expired", detail: "Request expired · Result not confirmed" }
    : { state: "unconfirmed", detail: "Result not confirmed" };
}
/** Derive command rows from original signed requests and raw signed reactions. */
export function attachSessionControls(
  messages: TimelineMessage[],
  originals: readonly RelayEvent[],
  reactions: readonly RelayEvent[],
  profiles?: UserProfileLookup,
  now = Date.now() / 1000,
  knownAgentRoles: ReadonlyMap<string, string> = new Map(),
): TimelineMessage[] {
  messages.forEach((message, index) => {
    const event = originals[index];
    if (event.kind !== 9) return;
    const command = parseSessionCommand(event.content);
    if (!command) return;
    const targets = [
      ...new Set(
        event.tags
          .filter(
            (tag) => tag[0] === "p" && CANONICAL_CONTROL_ID.test(tag[1] ?? ""),
          )
          .map((tag) => tag[1]),
      ),
    ];
    // An unknown address cannot silently become an agent control.
    if (
      !targets.length ||
      targets.some(
        (key) =>
          profiles?.[key]?.isAgent !== true &&
          knownAgentRoles.get(key) !== "bot",
      )
    )
      return;
    message.sessionControl = {
      action:
        command.type === "compact"
          ? "Compact context"
          : command.depth === "none"
            ? "Start over from scratch"
            : command.depth === "all"
              ? "Start over with the entire thread"
              : "Start over with recent history",
      targets: targets.map((pubkey) => ({
        pubkey,
        name: profiles?.[pubkey]?.displayName || "Agent",
        ...targetResult(event, pubkey, reactions, now),
      })),
    };
  });
  return messages;
}
