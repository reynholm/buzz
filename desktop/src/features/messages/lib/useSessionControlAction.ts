import * as React from "react";
import { getChannelDetails } from "@/shared/api/tauriChannels";
import { revalidateRelayAgents } from "@/shared/api/tauriRelayAgents";
import { sendChannelMessage } from "@/shared/api/tauriMessages";
import {
  canManageSession,
  type SessionPermissionEvidence,
} from "./sessionControlPermissions";
import { parseSessionCommand, type SessionCommand } from "./sessionControl";

export type SessionControlScope = {
  targetPubkey: string;
  channelId: string;
  rootEventId: string;
  relayUrl: string;
  signerPubkey: string;
};
/** Targeted, fresh evidence: a failed read propagates instead of authorizing stale data. */
export async function fetchSessionPermissionEvidence(
  scope: SessionControlScope,
): Promise<SessionPermissionEvidence> {
  const [agents, channel] = await Promise.all([
    revalidateRelayAgents(
      [scope.targetPubkey, scope.signerPubkey],
      scope.channelId,
    ),
    getChannelDetails(scope.channelId),
  ]);
  return {
    targetPubkey: scope.targetPubkey,
    agent: agents.find((agent) => agent.pubkey === scope.targetPubkey) ?? null,
    requester:
      agents.find((agent) => agent.pubkey === scope.signerPubkey) ?? null,
    channel,
  };
}
const production = {
  fetchEvidence: fetchSessionPermissionEvidence,
  send: sendChannelMessage,
};
/** Send a scoped control independently of the composer, fenced across every await. */
export function useSessionControlAction(
  scope: SessionControlScope,
  dependencies = production,
) {
  const key = JSON.stringify(scope);
  const currentKey = React.useRef({ key, generation: 0 });
  if (currentKey.current.key !== key)
    currentKey.current = { key, generation: currentKey.current.generation + 1 };
  const mounted = React.useRef(false);
  const inFlight = React.useRef<{ key: string; generation: number } | null>(
    null,
  );
  const [state, setState] = React.useState<{
    key: string;
    generation: number;
    pending: boolean;
    error: string | null;
  }>({ key, generation: 0, pending: false, error: null });
  React.useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      currentKey.current.generation += 1;
    };
  }, []);
  const run = React.useCallback(
    async (command: SessionCommand, onSent?: () => void) => {
      if (
        !mounted.current ||
        (inFlight.current?.key === key &&
          inFlight.current.generation === currentKey.current.generation)
      )
        return false;
      const captured = { ...scope };
      const capturedKey = key;
      const capturedGeneration = currentKey.current.generation;
      const isCurrent = () =>
        mounted.current &&
        currentKey.current.key === capturedKey &&
        currentKey.current.generation === capturedGeneration;
      // Never send arbitrary text supplied through a UI action.
      const parsed = parseSessionCommand(command.content);
      if (
        !parsed ||
        !captured.relayUrl ||
        !captured.signerPubkey ||
        !captured.rootEventId
      )
        return false;
      const flight = { key, generation: capturedGeneration };
      inFlight.current = flight;
      setState({
        key,
        generation: capturedGeneration,
        pending: true,
        error: null,
      });
      try {
        const evidence = await dependencies.fetchEvidence(captured);
        if (!isCurrent()) return false;
        if (
          !canManageSession(evidence, captured.signerPubkey, captured.channelId)
        )
          throw new Error("Permission changed");
        await dependencies.send(
          captured.channelId,
          parsed.content,
          captured.rootEventId,
          undefined,
          [captured.targetPubkey],
          9,
          undefined,
          undefined,
          undefined,
          undefined,
          captured.relayUrl,
          captured.signerPubkey,
          captured.rootEventId,
        );
        if (!isCurrent()) return false;
        onSent?.();
        return true;
      } catch {
        if (isCurrent())
          setState({
            key,
            generation: capturedGeneration,
            pending: false,
            error:
              "Could not send session control. Check access and connection, then retry.",
          });
        return false;
      } finally {
        if (inFlight.current === flight) inFlight.current = null;
        if (isCurrent()) setState((value) => ({ ...value, pending: false }));
      }
    },
    [dependencies, key, scope],
  );
  const ownsState =
    state.key === key && state.generation === currentKey.current.generation;
  return {
    run,
    pending: ownsState && state.pending,
    error: ownsState ? state.error : null,
  };
}
