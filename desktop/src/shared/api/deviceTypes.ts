/** Backend-computed home; absence and a failed read are not Unclaimed. */
export type DefinitionHome = {
  kind: "local" | "remote" | "unclaimed";
  label: string | null;
  remoteInstancePubkeys: string[];
};

/** Backend-owned permissions, including distinct hydration refusal reasons. */
export type DefinitionCapabilities = {
  canCreateInstance: boolean;
  canDeleteDefinition: boolean;
  blockedReason: string | null;
};

/** Public installation metadata; never contains host proof or instance binding. */
export type DeviceIdentity = {
  deviceId: string;
  label: string;
  createdAt: string;
};

/** Queued work drains through existing event-sync; enqueue is not delivery. */
export type DeviceLabelResult = {
  identity: DeviceIdentity;
  publication: "queued" | "complete";
};

/** Opaque backend authority for one owner, relay and workspace generation. */
export type DeviceHomeSyncSession = {
  token: string;
  ownerPubkey: string;
  relayUrl: string;
  workspaceGeneration: number;
};

/** Exhaustively applied history IDs, including superseded heads. */
export type DeviceHomeHistoryResult = {
  coveredEventIds: string[];
};
