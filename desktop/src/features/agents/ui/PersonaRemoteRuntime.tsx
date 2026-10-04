import { useQueryClient } from "@tanstack/react-query";

import { personasQueryKey } from "../hooks";
import { useAgentAvailabilityLookup } from "../lib/useAgentAvailability";
import type { AgentPersona } from "@/shared/api/types";

/** Display backend home and exact-key relay availability without runtime authority. */
export function PersonaRemoteRuntime({ persona }: { persona: AgentPersona }) {
  const client = useQueryClient();
  const pubkeys = persona.home?.remoteInstancePubkeys ?? [];
  const { getAvailability } = useAgentAvailabilityLookup(pubkeys);
  const reason = persona.capabilities?.blockedReason;
  const remote =
    persona.home?.kind === "remote" && reason === "definition_hosted_elsewhere";
  const availability = pubkeys.map((pubkey) => getAvailability(pubkey));
  const online = availability.some(
    (status) => status === "online" || status === "away",
  );
  const known =
    availability.length > 0 &&
    availability.every((status) => status !== undefined);
  const label = persona.home?.label?.trim();

  return (
    <div
      className="space-y-1 text-xs text-muted-foreground"
      data-testid={`persona-runtime-remote-${persona.id}`}
    >
      {remote ? (
        <p>
          {label ? `On device ${label}` : "On another device"} ·{" "}
          {online ? "online" : known ? "offline" : "connection unknown"}
        </p>
      ) : reason === "device_home_sync_pending" ? (
        <p role="status">Checking where this agent runs</p>
      ) : (
        <div className="space-y-1">
          <p role="alert">
            Could not verify where this agent runs. Check your connection and
            refresh the status. History sync will resume when connected.
          </p>
          <button
            className="text-primary underline"
            onClick={() =>
              void client.invalidateQueries({ queryKey: personasQueryKey })
            }
            type="button"
          >
            Refresh status
          </button>
        </div>
      )}
    </div>
  );
}
