import type {
  MessageSessionStatus,
  TimelineMessage,
  TimelineReaction,
} from "@/features/messages/types";

/**
 * Value-equality helpers for `MessageRow`'s memo comparator.
 *
 * Several row props are rebuilt with fresh identities on every ingest even
 * when their values are unchanged: `message.tags` (every relay/IPC refetch
 * deserializes new event objects), `message.reactions` (rebuilt per
 * `formatTimelineMessages` run), and the thread panel's per-row layout
 * arrays. Comparing them by identity made every row re-render on every
 * streamed event — in a long open thread with agents streaming, that
 * re-reconciled every row's action bar/tooltip/provider subtree several
 * times per second and saturated the main thread (see
 * typing-latency.perf.ts, scenario "thread68+"). These value comparisons
 * are O(a few tiny arrays) per row — far cheaper than one spurious row
 * render.
 */

export function tagsEqual(
  a: TimelineMessage["tags"],
  b: TimelineMessage["tags"],
): boolean {
  if (a === b) return true;
  if (!a || !b || a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) {
    const tagA = a[i];
    const tagB = b[i];
    if (tagA.length !== tagB.length) return false;
    for (let j = 0; j < tagA.length; j += 1) {
      if (tagA[j] !== tagB[j]) return false;
    }
  }
  return true;
}

export function reactionsEqual(
  a: TimelineReaction[] | undefined,
  b: TimelineReaction[] | undefined,
): boolean {
  if (a === b) return true;
  if (!a || !b || a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) {
    const left = a[i];
    const right = b[i];
    if (
      left.emoji !== right.emoji ||
      left.emojiUrl !== right.emojiUrl ||
      left.count !== right.count ||
      left.reactedByCurrentUser !== right.reactedByCurrentUser ||
      left.users.length !== right.users.length
    ) {
      return false;
    }
    for (let j = 0; j < left.users.length; j += 1) {
      const userA = left.users[j];
      const userB = right.users[j];
      if (
        userA.pubkey !== userB.pubkey ||
        userA.displayName !== userB.displayName ||
        userA.avatarUrl !== userB.avatarUrl
      ) {
        return false;
      }
    }
  }
  return true;
}

/** Derived session status: rebuilt per format pass, compared by value. The
 * explanation can move between rows whose own tags did not change, so this
 * must be compared in addition to `tagsEqual`. */
export function sessionStatusEqual(
  a: MessageSessionStatus | undefined,
  b: MessageSessionStatus | undefined,
): boolean {
  if (a === b) return true;
  if (!a || !b || a.percent !== b.percent) return false;
  const left = a.explanation;
  const right = b.explanation;
  if (left === right) return true;
  if (!left || !right) return false;
  if (left.title !== right.title || left.detail !== right.detail) return false;
  const leftHistory = left.history ?? [];
  const rightHistory = right.history ?? [];
  return (
    leftHistory.length === rightHistory.length &&
    leftHistory.every((phrase, index) => phrase === rightHistory[index])
  );
}

export function numberArrayEqual(
  a: readonly number[] | undefined,
  b: readonly number[] | undefined,
): boolean {
  if (a === b) return true;
  if (!a || !b || a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

/** Thread depth-guide actions: value-equal when every action matches on its
 * scalar fields and refers to the same ancestor message (by id). Structural
 * type mirrors `ThreadDepthGuideAction` (MessageRow.tsx) — importing it here
 * would create an import cycle with MessageRow. */
export function depthGuideActionsEqual(
  a:
    | ReadonlyArray<{
        active?: boolean;
        depth: number;
        label: string;
        message: TimelineMessage;
      }>
    | undefined,
  b:
    | ReadonlyArray<{
        active?: boolean;
        depth: number;
        label: string;
        message: TimelineMessage;
      }>
    | undefined,
): boolean {
  if (a === b) return true;
  if (!a || !b || a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) {
    if (
      a[i].active !== b[i].active ||
      a[i].depth !== b[i].depth ||
      a[i].label !== b[i].label ||
      a[i].message.id !== b[i].message.id
    ) {
      return false;
    }
  }
  return true;
}

/** Results arrive independently of the request body and ordinary reaction pills. */
export function sessionControlEqual(
  a: TimelineMessage["sessionControl"],
  b: TimelineMessage["sessionControl"],
): boolean {
  if (a === b) return true;
  return Boolean(
    a &&
      b &&
      a.action === b.action &&
      a.targets.length === b.targets.length &&
      a.targets.every((target, i) => {
        const other = b.targets[i];
        return (
          target.pubkey === other.pubkey &&
          target.name === other.name &&
          target.state === other.state &&
          target.detail === other.detail
        );
      }),
  );
}
