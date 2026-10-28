import type { ObserverEvent, TranscriptItem } from "./agentSessionTypes";
import { asRecord, asString } from "./agentSessionUtils";

const titles: Record<string, string> = {
  running: "Running",
  retrying: "Retrying",
  waiting_approval: "Waiting for approval",
  waiting_background: "Waiting for background task",
  failed: "Failed",
  waiting_human: "Needs your attention",
  completed: "Completed",
};

const reasons: Record<string, string> = {
  oauth_connection: "The provider connection was interrupted.",
  oauth_refresh_lock: "Waiting for the provider to refresh its session.",
  network_timeout: "The provider connection timed out.",
  child_exited: "The agent process stopped unexpectedly.",
  permission_request: "Review the permission request in this thread.",
  child_running: "The existing background task is still running.",
  retirement_unverified:
    "Confirm the previous execution has stopped before continuing.",
  external_effect_uncertain: "Verify the action's result before continuing.",
  permission_decision_required:
    "Review the permission request before retrying.",
  permission_identity_conflict: "Check the requested action before continuing.",
  delivery_journal_unverified:
    "Check the saved report's delivery before continuing.",
  managed_delivery_unconfirmed:
    "Report delivery is unconfirmed. Check it before continuing.",
  managed_report_unconfirmed:
    "The turn ended without a confirmed report. Check the existing result.",
};

/** One visible state per work; stale replay cannot replace a newer transition. */
export function workStateTranscriptItem(
  event: ObserverEvent,
  items: ReadonlyMap<string, TranscriptItem>,
): TranscriptItem | null {
  if (event.kind !== "work_state") return null;
  const payload = asRecord(event.payload);
  const work = asString(payload.workId);
  const state = asString(payload.state);
  if (!work || !/^[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}$/.test(work))
    return null;
  if (!state || !Object.hasOwn(titles, state)) return null;
  const id = `work:${event.channelId ?? "global"}:${work}`;
  const existing = items.get(id);
  if (
    existing?.type === "lifecycle" &&
    (existing.workSequence ?? -1) >= event.seq
  )
    return null;
  const reason = asString(payload.reason);
  let text = reason && Object.hasOwn(reasons, reason) ? reasons[reason] : "";
  const delay = payload.nextAttemptInSecs;
  if (
    state === "retrying" &&
    typeof delay === "number" &&
    Number.isFinite(delay) &&
    delay >= 0 &&
    delay <= 86400
  ) {
    text = `${text} Retry scheduled in ${Math.ceil(delay)}s.`.trim();
  }
  return {
    id,
    type: "lifecycle",
    renderClass: state === "failed" ? "error" : "status",
    title: titles[state],
    text,
    timestamp: event.timestamp,
    channelId: event.channelId,
    turnId: event.turnId,
    sessionId: event.sessionId,
    acpSource: event.kind,
    workSequence: event.seq,
  };
}
