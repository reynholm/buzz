import type { Channel, RelayAgent, RelayEvent } from "@/shared/api/types";
import type { TimelineMessage } from "@/features/messages/types";
import { getThreadReference, getChannelIdFromTags } from "./threading";
import { canManageSession } from "./sessionControlPermissions";
import { parseSessionStatusTags } from "./sessionStatus";
import { formatMentionDisplayLabel } from "@/shared/lib/mentionDisplay";
import { truncateNpub } from "@/shared/lib/pubkey";

/** A persisted head, including a root-only thread, establishes a real scope. */
export function hasPersistedThread(head: TimelineMessage | null): boolean {
  return Boolean(
    head &&
      !head.pending &&
      /^[0-9a-f]{64}$/i.test(head.id) &&
      head.signerPubkey,
  );
}

/** Project raw signers and explicit p-tags from the bounded canonical thread data. */
export function projectThreadContextAgents({
  rootId,
  channel,
  events,
  directory,
  signerPubkey,
}: {
  rootId: string;
  channel: Channel;
  events: readonly RelayEvent[];
  directory: readonly RelayAgent[];
  signerPubkey: string;
}) {
  const candidates = new Set<string>();
  const shared = new Set<string>();
  for (const event of events) {
    if (
      ![9, 40002].includes(event.kind) ||
      getChannelIdFromTags(event.tags) !== channel.id
    )
      continue;
    if (event.id !== rootId && getThreadReference(event.tags).rootId !== rootId)
      continue;
    candidates.add(event.pubkey.toLowerCase());
    for (const tag of event.tags) {
      if (tag[0] === "p" && /^[0-9a-f]{64}$/i.test(tag[1] ?? ""))
        candidates.add(tag[1].toLowerCase());
    }
    if (parseSessionStatusTags(event.tags)?.info?.scope === "conversation")
      shared.add(event.pubkey.toLowerCase());
  }
  const requester =
    directory.find((agent) => agent.pubkey === signerPubkey) ?? null;
  const accessible = directory.filter(
    (agent) =>
      candidates.has(agent.pubkey) &&
      canManageSession(
        { targetPubkey: agent.pubkey, agent, requester, channel },
        signerPubkey,
        channel.id,
      ),
  );
  return [
    ...new Map(accessible.map((agent) => [agent.pubkey, agent])).values(),
  ].map((agent) => {
    const name = agent.name.trim() || truncateNpub(agent.pubkey);
    const sameName = accessible.some(
      (other) =>
        other.pubkey !== agent.pubkey &&
        other.name.trim().toLowerCase() === agent.name.trim().toLowerCase(),
    );
    return {
      pubkey: agent.pubkey,
      label: sameName
        ? formatMentionDisplayLabel(`${name} (${agent.pubkey})`, agent.pubkey)
        : name,
      shared: shared.has(agent.pubkey),
    };
  });
}
