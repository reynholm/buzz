import type { Channel, RelayAgent } from "@/shared/api/types";
import { normalizePubkey } from "@/shared/lib/pubkey";

/** Evidence from the verified relay directory and channel metadata, never inventory. */
export type SessionPermissionEvidence = {
  targetPubkey: string;
  agent: Pick<
    RelayAgent,
    "pubkey" | "ownerPubkey" | "respondTo" | "respondToAllowlist" | "channelIds"
  > | null;
  channel: Pick<
    Channel,
    "id" | "channelType" | "isMember" | "archivedAt"
  > | null;
  /** Optional verified requester coordinate for the harness's sibling rule. */
  requester?: Pick<RelayAgent, "pubkey" | "ownerPubkey"> | null;
};

/** Mirror normal listener admission, including the owner-only DM policy. */
export function canManageSession(
  evidence: SessionPermissionEvidence,
  viewerPubkey: string,
  channelId: string,
): boolean {
  const { agent, channel, requester } = evidence;
  const viewer = normalizePubkey(viewerPubkey);
  if (
    !viewer ||
    !agent ||
    !channel ||
    channel.id !== channelId ||
    !channel.isMember ||
    channel.archivedAt !== null ||
    !["stream", "forum", "dm"].includes(channel.channelType) ||
    normalizePubkey(agent.pubkey) !== normalizePubkey(evidence.targetPubkey) ||
    !agent.channelIds.includes(channelId) ||
    !["owner-only", "allowlist", "anyone"].includes(agent.respondTo ?? "")
  )
    return false;
  const owner = agent.ownerPubkey ? normalizePubkey(agent.ownerPubkey) : null;
  const isOwner =
    owner !== null &&
    (viewer === owner ||
      (requester?.ownerPubkey &&
        normalizePubkey(requester.pubkey) === viewer &&
        normalizePubkey(requester.ownerPubkey) === owner));
  if (isOwner) return true;
  if (channel.channelType === "dm") return false;
  return (
    agent.respondTo === "anyone" ||
    (agent.respondTo === "allowlist" &&
      agent.respondToAllowlist.some((key) => normalizePubkey(key) === viewer))
  );
}
