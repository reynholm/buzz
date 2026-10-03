import * as React from "react";

import { relayClient } from "@/shared/api/relayClient";
import {
  beginDeviceHomeSync,
  hydrateDeviceHomeHistory,
  finishDeviceHomeSync,
  invalidateDeviceHomeSync,
  reconcileInboundPersonaEvent,
} from "@/shared/api/tauriPersonas";
import type { RelayEvent } from "@/shared/api/types";
import {
  KIND_DELETION,
  KIND_MANAGED_AGENT,
  KIND_PERSONA,
  KIND_TEAM,
  KIND_TEAM_CATALOG,
} from "@/shared/constants/kinds";

// Persona/team/managed-agent projections (upserts), the owner's own team
// catalog heads (30178), plus kind:5 NIP-09 deletions, so a tombstone published
// by another device also removes the local record here.
//
// The 30178 head has no local record — it is retained only as this device's
// publication witness, so a second device's boot reconcile and interactive
// refresh have a row to supersede or retract. Without it, device B never learns
// device A published, and B's later edit or delete cannot update A's
// discoverable catalog entry.
const PERSONA_SYNC_KINDS = [
  KIND_PERSONA,
  KIND_TEAM,
  KIND_MANAGED_AGENT,
  KIND_TEAM_CATALOG,
  KIND_DELETION,
];

// Bounded retry for a transient backfill failure. A deterministic
// dense-boundary rejection is NOT retried (a retry cannot clear a genuinely
// dense second); only network/transport rejections are. After the last attempt
// the pipeline falls to degraded-live rather than looping forever.
const BACKFILL_MAX_ATTEMPTS = 3;
const BACKFILL_RETRY_BASE_DELAY_MS = 500;

// Thrown when `fetchOwnerHistoryToExhaustion` reaches a full page whose oldest
// event cannot advance the time-only cursor: more than one page of events share
// the boundary second, and the WS filter has no `(created_at, id)` cursor to
// escape it. Distinct from a transport error so the caller fails loudly into
// degraded-live instead of silently proceeding with partial history.
export class PersonaHistoryDenseBoundaryError extends Error {
  constructor(boundarySecond: number) {
    super(
      `owner history has >1 page of events at created_at ${boundarySecond}; ` +
        `the time-only relay cursor cannot page past it`,
    );
    this.name = "PersonaHistoryDenseBoundaryError";
  }
}

async function backfillBackoff(attempt: number): Promise<void> {
  const ms = BACKFILL_RETRY_BASE_DELAY_MS * 2 ** attempt;
  await new Promise((resolve) => setTimeout(resolve, ms));
}

function eventDTag(event: RelayEvent): string | null {
  return event.tags.find((tag) => tag[0] === "d")?.[1] ?? null;
}

function eventIsNewer(candidate: RelayEvent, current: RelayEvent): boolean {
  return (
    candidate.created_at > current.created_at ||
    (candidate.created_at === current.created_at && candidate.id < current.id)
  );
}

// The catalog dependency set: the 30178 head and the 30175/30176 coordinates a
// team-catalog refresh resolves against. Degraded-live drops every live event
// that could drive (or destructively re-trigger) a catalog refresh against an
// unhydrated store — see `dispatchLive`. A kind-5 deletion is held whenever ANY
// parseable `a` tag names a dependency coordinate (`<kind>:<owner>:<d_tag>`):
// Rust's `parse_deletion_coordinate` scans all `a` tags and routes the first
// signer-owned dependency target, so classifying on the first tag alone would
// dispatch a deletion that Rust still routes destructively (a foreign/malformed
// first tag ahead of an owned 30176). We do not duplicate Rust's signer/owner
// validation — false-positive holding during an abnormal self-healing state is
// safer than letting a Rust-routable destructive tombstone through.
const CATALOG_DEPENDENCY_KINDS: ReadonlySet<number> = new Set([
  KIND_PERSONA,
  KIND_TEAM,
  KIND_TEAM_CATALOG,
]);

function deletionTargetsDependency(event: RelayEvent): boolean {
  return event.tags.some((tag) => {
    if (tag[0] !== "a" || !tag[1]) return false;
    const kind = Number.parseInt(tag[1].split(":", 1)[0], 10);
    return !Number.isNaN(kind) && CATALOG_DEPENDENCY_KINDS.has(kind);
  });
}

function isCatalogDependencyEvent(event: RelayEvent): boolean {
  if (event.kind === KIND_DELETION) {
    return deletionTargetsDependency(event);
  }
  return CATALOG_DEPENDENCY_KINDS.has(event.kind);
}

/**
 * Keep only the NIP-33 head for each managed-agent coordinate in a startup
 * backfill. Applying historical policy revisions one by one can stop and start
 * the same runtime for every revision; the retained store only needs the final
 * head. Other event kinds stay in relay order because persona/team projections
 * do not trigger runtime policy transitions and deletion ordering is separate.
 */
export function coalesceManagedAgentBackfill(
  events: readonly RelayEvent[],
): RelayEvent[] {
  const heads = new Map<string, RelayEvent>();

  for (const event of events) {
    if (event.kind !== KIND_MANAGED_AGENT) continue;
    const dTag = eventDTag(event);
    if (!dTag) continue;
    const coordinate = `${event.pubkey.toLowerCase()}:${dTag.toLowerCase()}`;
    const current = heads.get(coordinate);
    if (!current || eventIsNewer(event, current)) heads.set(coordinate, event);
  }

  return events.filter((event) => {
    if (event.kind !== KIND_MANAGED_AGENT) return true;
    const dTag = eventDTag(event);
    if (!dTag) return true;
    return (
      heads.get(`${event.pubkey.toLowerCase()}:${dTag.toLowerCase()}`) === event
    );
  });
}

/**
 * Dispatch owner catalog heads (30178) AFTER their constituents within the
 * complete hydration batch. A freshly shared 30178 head is typically newer than
 * the 30176 team and 30175 personas it projects, so relay newest-first order
 * places it first. Reconciling in that raw order lets the inbound team refresh
 * run while device B's personas have not hydrated: member resolution fails, and
 * the resolution-failure arm purges the just-retained witness and queues a
 * dominating false tombstone — deleting the owner's valid catalog entry on
 * ordinary first sync.
 *
 * Deferring only the 30178 heads to the end of the batch preserves newest-wins
 * within every other coordinate (order among non-catalog events is untouched)
 * while guaranteeing the constituents are all applied before any catalog
 * refresh could fire. A stable partition keeps relay order within each group.
 * This is only sound over a COMPLETE batch — `startPersonaSync` pages history to
 * exhaustion and buffers concurrent live events so every constituent is present
 * before the partition runs.
 */
export function orderCatalogHeadsLast(
  events: readonly RelayEvent[],
): RelayEvent[] {
  const constituents = events.filter(
    (event) => event.kind !== KIND_TEAM_CATALOG,
  );
  const catalogHeads = events.filter(
    (event) => event.kind === KIND_TEAM_CATALOG,
  );
  return [...constituents, ...catalogHeads];
}

// The backend fetches and applies exhaustive history. The frontend establishes
// live delivery first, buffers during hydration and drains successful applies
// before finalizing the backend-owned readiness barrier.
export function startPersonaSync(
  pubkey: string,
  relayUrl: string,
  onCancelled: () => boolean,
): () => Promise<void> {
  type Run = {
    generation: number;
    token: string;
    controller: AbortController;
    dispose: (() => Promise<void>) | null;
  };
  let generation = 0;
  let disposed = false;
  let active: Run | null = null;
  let restartQueued: number | null = null;
  let restartDelay: {
    timer: ReturnType<typeof setTimeout>;
    resolve: () => void;
  } | null = null;
  let subscriptionFailures = 0;
  const cancelRestart = () => {
    restartQueued = null;
    if (restartDelay) {
      clearTimeout(restartDelay.timer);
      restartDelay.resolve();
      restartDelay = null;
    }
  };
  const cancelled = () => disposed || onCancelled();
  const current = (run: Run) =>
    !cancelled() && active === run && run.generation === generation;
  const invalidate = async (run: Run) => {
    // Send token-scoped invalidation immediately, before awaiting subscription teardown.
    const invalidation = invalidateDeviceHomeSync(run.token).catch((error) => {
      console.warn("[usePersonaSync] session invalidation failed:", error);
    });
    run.controller.abort();
    await Promise.all([invalidation, run.dispose?.()]);
  };
  const retire = () => {
    generation += 1;
    const old = active;
    active = null;
    return old ? invalidate(old) : Promise.resolve();
  };
  const normalizeRelay = (url: string) => url.trim().replace(/\/+$/, "");

  const launch = async () => {
    const epoch = generation;
    let run: Run | null = null;
    try {
      const session = await beginDeviceHomeSync();
      run = {
        generation: epoch,
        token: session.token,
        controller: new AbortController(),
        dispose: null,
      };
      if (
        cancelled() ||
        epoch !== generation ||
        session.ownerPubkey !== pubkey ||
        normalizeRelay(session.relayUrl) !== normalizeRelay(relayUrl)
      ) {
        await invalidate(run);
        return;
      }
      active = run;
      const sessionRun = run;
      let hydrated = false;
      let liveConfirmed = false;
      const unavailable = (health: string) => {
        if (!current(sessionRun)) return;
        console.warn("[usePersonaSync] live subscription unavailable:", health);
        subscriptionFailures += 1;
        if (subscriptionFailures < BACKFILL_MAX_ATTEMPTS)
          queueRestart(subscriptionFailures - 1);
        else
          void retire().catch((error) =>
            console.warn(
              "[usePersonaSync] subscription teardown failed:",
              error,
            ),
          );
      };
      let degraded = false;
      let failed = false;
      let chain = Promise.resolve();
      const buffer: RelayEvent[] = [];
      const reconcile = (event: RelayEvent) => {
        if (event.pubkey !== pubkey || !current(sessionRun)) return;
        if (degraded && isCatalogDependencyEvent(event)) return;
        chain = chain
          .then(async () => {
            if (current(sessionRun))
              await reconcileInboundPersonaEvent(
                JSON.stringify(event),
                relayUrl,
                sessionRun.token,
              );
          })
          .catch((error) => {
            // Keep processing later events, but never erase this session's failure.
            failed = true;
            console.warn("[usePersonaSync] reconcile failed:", error);
          });
      };
      const drain = (coveredIds: readonly string[]) => {
        hydrated = true;
        const covered = new Set(coveredIds);
        for (const event of orderCatalogHeadsLast(
          coalesceManagedAgentBackfill(buffer),
        )) {
          if (!covered.has(event.id)) reconcile(event);
        }
        buffer.length = 0;
      };
      try {
        const dispose = await relayClient.subscribeLive(
          { kinds: PERSONA_SYNC_KINDS, authors: [pubkey], limit: 0 },
          (event) => {
            if (!current(sessionRun) || event.pubkey !== pubkey) return;
            if (hydrated) reconcile(event);
            else buffer.push(event);
          },
          (readiness) => {
            if (readiness === "eose") liveConfirmed = true;
            else unavailable(readiness);
          },
          undefined,
          sessionRun.controller.signal,
          (health) => {
            if (health !== "eose") unavailable(health);
          },
        );
        if (!current(sessionRun)) {
          await dispose();
          return;
        }
        sessionRun.dispose = dispose;
      } catch (error) {
        if (!current(sessionRun)) return;
        degraded = true;
        console.warn(
          "[usePersonaSync] live subscription failed; backfilling in degraded-live sync:",
          error,
        );
      }
      for (let attempt = 0; attempt < BACKFILL_MAX_ATTEMPTS; attempt += 1) {
        try {
          const history = await hydrateDeviceHomeHistory(sessionRun.token);
          if (!current(sessionRun)) return;
          drain(history.coveredEventIds);
          // Events arriving while we await an apply can extend the chain.
          // Drain to a stable tail before asking the backend to install Ready.
          for (;;) {
            const tail = chain;
            await tail;
            if (!current(sessionRun)) return;
            if (tail === chain) break;
          }
          if (liveConfirmed && !degraded && !failed) {
            await finishDeviceHomeSync(sessionRun.token);
            if (current(sessionRun)) subscriptionFailures = 0;
          }
          return;
        } catch (error) {
          if (!current(sessionRun)) return;
          const dense =
            error instanceof PersonaHistoryDenseBoundaryError ||
            String(error).includes("device_home_sync_dense_history_boundary");
          if (!dense && !hydrated && attempt < BACKFILL_MAX_ATTEMPTS - 1) {
            console.warn("[usePersonaSync] backfill failed, retrying:", error);
            await backfillBackoff(attempt);
            if (!current(sessionRun)) return;
            const replacement = await beginDeviceHomeSync();
            if (
              !current(sessionRun) ||
              replacement.ownerPubkey !== pubkey ||
              normalizeRelay(replacement.relayUrl) !== normalizeRelay(relayUrl)
            ) {
              await invalidateDeviceHomeSync(replacement.token);
              return;
            }
            sessionRun.token = replacement.token;
            continue;
          }
          console.warn(
            "[usePersonaSync] backfill failed; entering degraded-live sync:",
            error,
          );
          degraded = true;
          drain([]);
          return;
        }
      }
    } catch (error) {
      console.warn("[usePersonaSync] sync initialization failed:", error);
      if (run && current(run)) await retire();
    }
  };
  const queueRestart = (delayAttempt?: number) => {
    if (cancelled() || restartQueued !== null) return;
    const retirement = retire();
    const epoch = generation;
    restartQueued = epoch;
    void retirement
      .then(async () => {
        if (restartQueued !== epoch) return;
        if (delayAttempt !== undefined) {
          await new Promise<void>((resolve) => {
            const timer = setTimeout(
              () => {
                restartDelay = null;
                resolve();
              },
              BACKFILL_RETRY_BASE_DELAY_MS * 2 ** delayAttempt,
            );
            restartDelay = { timer, resolve };
          });
        }
        if (restartQueued !== epoch) return;
        restartQueued = null;
        if (!cancelled() && generation === epoch) void launch();
      })
      .catch((error) => {
        if (restartQueued === epoch) restartQueued = null;
        console.warn("[usePersonaSync] subscription teardown failed:", error);
      });
  };
  const restart = () => {
    if (restartQueued !== null) return;
    subscriptionFailures = 0;
    queueRestart();
  };
  let seeded = false;
  let lost = false;
  const unsubscribeState = relayClient.subscribeToConnectionState((state) => {
    if (!seeded) {
      seeded = true;
      return;
    }
    if (state !== "connected") {
      lost = true;
      cancelRestart();
      void retire().catch((error) =>
        console.warn(
          "[usePersonaSync] connection-loss teardown failed:",
          error,
        ),
      );
    } else if (lost) {
      lost = false;
      restart();
    }
  });
  const unsubscribeReconnect = relayClient.subscribeToReconnects(restart);
  void launch();
  return async () => {
    disposed = true;
    cancelRestart();
    unsubscribeState();
    unsubscribeReconnect();
    await retire();
  };
}

// Subscribes to this device's own persona/team/agent projection + deletion
// events and patches each into the local store. The subscription is keyed on
// the active pubkey and relay: an identity or community switch re-runs the
// effect, whose cleanup closes the old subscription before a new one opens on
// the new filter — so no stale-coordinate subscription survives, and every
// reconcile is attributed to the community it was subscribed to.
//
// A fresh device that comes online AFTER another already published gets no
// history from a live-only subscription: relayClient's replayLiveSubscriptions
// only replays from a since-cursor that is undefined until the first live
// event arrives. So `startPersonaSync` does an explicit one-shot history fetch
// up front and feeds each event through the same reconcile path.
export function usePersonaSync(
  pubkey: string | undefined,
  relayUrl: string | undefined,
): void {
  React.useEffect(() => {
    if (!pubkey || !relayUrl) return;
    let cancelled = false;
    const dispose = startPersonaSync(pubkey, relayUrl, () => cancelled);
    return () => {
      cancelled = true;
      void dispose();
    };
  }, [pubkey, relayUrl]);
}
