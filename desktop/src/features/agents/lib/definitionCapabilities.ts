import type { AgentPersona } from "@/shared/api/types";
import { listPersonas } from "@/shared/api/tauriPersonas";

export type DefinitionAction = "createInstance" | "deleteDefinition";

/** A backend refusal code with its reported home label, suitable for UI reasons. */
export class DefinitionCapabilityError extends Error {
  readonly code: string;
  readonly homeLabel: string | null;

  constructor(code: string, homeLabel: string | null = null) {
    const reason =
      code === "device_home_sync_pending" ||
      code === "device_home_sync_failed" ||
      code === "definition_capabilities_unavailable"
        ? `${code}: Device history is not ready. Wait for synchronization and retry.`
        : code;
    super(
      homeLabel && code === "definition_hosted_elsewhere"
        ? `${reason}: ${homeLabel}`
        : reason,
    );
    this.name = "DefinitionCapabilityError";
    this.code = code;
    this.homeLabel = homeLabel;
  }
}

/** Require an explicit computed permission; never infer it from policy/origin. */
export function requireDefinitionCapability(
  persona: AgentPersona,
  action: DefinitionAction,
): void {
  if (persona.home === undefined || !persona.capabilities) {
    throw new DefinitionCapabilityError("definition_capabilities_unavailable");
  }
  const allowed =
    action === "createInstance"
      ? persona.capabilities.canCreateInstance
      : persona.capabilities.canDeleteDefinition;
  if (allowed !== true) {
    throw new DefinitionCapabilityError(
      persona.capabilities.blockedReason ?? "definition_action_unavailable",
      persona.home?.label ?? null,
    );
  }
}

/** Reload the authoritative list before acting on a cached/raw definition. */
export async function getDefinitionForAction(
  id: string,
  action: DefinitionAction,
): Promise<AgentPersona> {
  const persona = (await listPersonas()).find((item) => item.id === id);
  if (!persona) throw new DefinitionCapabilityError("definition_not_found");
  requireDefinitionCapability(persona, action);
  return persona;
}

/** Reuse requires the backend permission for this exact instance. */
export function canReuseManagedAgentOnDevice(
  canStartOnDevice: boolean | undefined,
): boolean {
  return canStartOnDevice === true;
}
