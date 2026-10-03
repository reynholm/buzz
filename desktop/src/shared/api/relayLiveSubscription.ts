import type { RelayEvent } from "@/shared/api/types";
import type {
  LiveSubscriptionHealth,
  LiveSubscriptionReadiness,
  RelaySubscription,
  RelaySubscriptionFilter,
} from "@/shared/api/relayClientShared";
import { clearClosedRetry } from "@/shared/api/relayClosedRecovery";
import type { RelayLiveReqDrain } from "@/shared/api/relayLiveReqDrain";

type LiveSubscriptionSession = {
  epoch: number;
  signal: AbortSignal;
  currentEpoch: () => number;
  subscriptions: Map<string, RelaySubscription>;
  liveReqDrain: RelayLiveReqDrain;
  ensureConnected: () => Promise<number>;
  closeSubscription: (subId: string) => Promise<void>;
  sendRawWithReconnectRetry: (
    payload: unknown[],
    fallbackMessage: string,
    onDispatch?: () => void,
  ) => Promise<void>;
};

/** Register live delivery with session-owned readiness and cancellation cleanup. */
export async function subscribeLiveSession(
  session: LiveSubscriptionSession,
  filter: RelaySubscriptionFilter,
  onEvent: (event: RelayEvent) => void,
  onReady: ((readiness: LiveSubscriptionReadiness) => void) | undefined,
  readinessTimeoutMs: number,
  signal: AbortSignal | undefined,
  priority: "interactive" | undefined,
  onHealth: ((health: LiveSubscriptionHealth) => void) | undefined,
) {
  const epoch = session.epoch;
  const sessionSignal = session.signal;
  signal?.throwIfAborted();
  const subId = `live-${crypto.randomUUID()}`;
  let fallbackTimeout: number | undefined;
  let settleReady = () => {};
  let removed = false;
  const onRemoved = () => {
    if (removed) return;
    removed = true;
    signal?.removeEventListener("abort", abort);
    sessionSignal.removeEventListener("abort", abort);
    window.clearTimeout(fallbackTimeout);
    onHealth?.("removed");
    settleReady();
  };
  const subscription: Extract<RelaySubscription, { mode: "live" }> = {
    mode: "live",
    filter,
    priority,
    onEvent,
    onRemoved,
    onHealth,
  };
  const dispose = async () => {
    if (session.subscriptions.get(subId) !== subscription) return;
    session.subscriptions.delete(subId);
    session.liveReqDrain.cancel(subId);
    clearClosedRetry(subscription);
    onRemoved();
    // Workspace teardown closes its socket; never send on its replacement.
    if (epoch === session.currentEpoch())
      await session.closeSubscription(subId);
  };
  let rejectCancelled = (_error: Error) => {};
  const cancelled = new Promise<never>((_resolve, reject) => {
    rejectCancelled = reject;
  });
  const abort = () => {
    rejectCancelled(
      new DOMException("Live subscription cancelled.", "AbortError"),
    );
    void dispose().catch(() => {});
    onRemoved();
  };
  signal?.addEventListener("abort", abort, { once: true });
  sessionSignal.addEventListener("abort", abort, { once: true });
  try {
    await Promise.race([session.ensureConnected(), cancelled]);
    signal?.throwIfAborted();
    sessionSignal.throwIfAborted();
    const ready = new Promise<void>((resolve) => {
      settleReady = resolve;
    });
    let readySettled = false;
    subscription.resolveReady = (readiness) => {
      if (readySettled) return;
      readySettled = true;
      window.clearTimeout(fallbackTimeout);
      onReady?.(readiness);
      settleReady();
    };
    session.subscriptions.set(subId, subscription);
    await Promise.race([
      session.sendRawWithReconnectRetry(
        ["REQ", subId, filter],
        "Failed to restore relay subscription.",
        () => {
          if (readySettled) return;
          window.clearTimeout(fallbackTimeout);
          fallbackTimeout = window.setTimeout(() => {
            subscription.onHealth?.("timeout");
            subscription.resolveReady?.("timeout");
          }, readinessTimeoutMs);
        },
      ),
      cancelled,
    ]);
    await Promise.race([ready, cancelled]);
    return dispose;
  } catch (error) {
    await dispose().catch(() => {});
    onRemoved();
    throw error;
  }
}
