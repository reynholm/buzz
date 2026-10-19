import { MoreHorizontal } from "lucide-react";
import { useId } from "react";
import { useRelayAgentsQuery } from "@/features/agents/hooks";
import { useChannelDetailsQuery } from "@/features/channels/hooks";
import { useCommunities } from "@/features/communities/useCommunities";
import { useIdentityQuery } from "@/shared/api/hooks";
import { canManageSession } from "@/features/messages/lib/sessionControlPermissions";
import { useSessionControlAction } from "@/features/messages/lib/useSessionControlAction";
import { parseSessionStatusTags } from "@/features/messages/lib/sessionStatus";
import type { TimelineMessage } from "@/features/messages/types";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubTrigger,
  DropdownMenuSubContent,
  DropdownMenuPortal,
} from "@/shared/ui/dropdown-menu";

/** A separate actionable sibling of the quiet, self-reported status paragraph. */
export function MessageSessionControls({
  message,
  channelId,
}: {
  message: TimelineMessage;
  channelId: string;
}) {
  const descriptionId = useId();
  const directory = useRelayAgentsQuery();
  const channel = useChannelDetailsQuery(channelId);
  const { activeCommunity } = useCommunities();
  const identity = useIdentityQuery();
  const targetPubkey = message.signerPubkey ?? "";
  const signerPubkey = identity.data?.pubkey ?? "";
  const action = useSessionControlAction({
    targetPubkey,
    signerPubkey,
    channelId,
    rootEventId: message.rootId ?? message.id,
    relayUrl: activeCommunity?.relayUrl ?? "",
  });
  const evidence = {
    targetPubkey,
    agent:
      directory.data?.find((agent) => agent.pubkey === targetPubkey) ?? null,
    requester:
      directory.data?.find((agent) => agent.pubkey === signerPubkey) ?? null,
    channel: channel.data ?? null,
  };
  if (
    message.pending ||
    message.pubkey !== targetPubkey ||
    directory.error ||
    channel.error ||
    !canManageSession(evidence, signerPubkey, channelId)
  )
    return null;
  const shared =
    parseSessionStatusTags(message.tags)?.info?.scope === "conversation";
  return (
    <span className="inline-flex flex-col items-start">
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            aria-label="Manage current session"
            disabled={action.pending}
            className="mt-1 rounded p-1 text-muted-foreground/70 hover:text-foreground focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
            data-testid="manage-current-session"
          >
            <MoreHorizontal aria-hidden="true" className="size-4" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="max-w-80">
          {shared ? (
            <p className="max-w-72 px-2 py-1.5 text-xs text-muted-foreground">
              This session is shared across the channel.
            </p>
          ) : null}
          <DropdownMenuSub>
            <DropdownMenuSubTrigger
              aria-label="Restart"
              aria-describedby={`${descriptionId}-restart`}
            >
              <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                <span>Restart</span>
                <span
                  id={`${descriptionId}-restart`}
                  className="whitespace-normal text-2xs text-muted-foreground"
                >
                  Stops current work, including subagents. File changes are
                  kept.
                </span>
              </span>
            </DropdownMenuSubTrigger>
            <DropdownMenuPortal>
              <DropdownMenuSubContent className="max-w-80">
                <DropdownMenuItem
                  aria-label="From scratch"
                  aria-describedby={`${descriptionId}-none`}
                  className="flex-col items-start gap-0.5"
                  onSelect={() => {
                    void action.run({
                      type: "restart",
                      depth: "none",
                      content: "!restart none",
                    });
                  }}
                >
                  <span>From scratch</span>
                  <span
                    id={`${descriptionId}-none`}
                    className="whitespace-normal text-2xs text-muted-foreground"
                  >
                    Keep the original assignment and your next request.
                  </span>
                </DropdownMenuItem>
                <DropdownMenuItem
                  aria-label="Recent history"
                  aria-describedby={`${descriptionId}-recent`}
                  className="flex-col items-start gap-0.5"
                  onSelect={() => {
                    void action.run({
                      type: "restart",
                      depth: "recent",
                      content: "!restart",
                    });
                  }}
                >
                  <span>Recent history</span>
                  <span
                    id={`${descriptionId}-recent`}
                    className="whitespace-normal text-2xs text-muted-foreground"
                  >
                    Keep the original assignment and recent messages.
                  </span>
                </DropdownMenuItem>
                <DropdownMenuItem
                  aria-label="Entire thread"
                  aria-describedby={`${descriptionId}-all`}
                  className="flex-col items-start gap-0.5"
                  onSelect={() => {
                    void action.run({
                      type: "restart",
                      depth: "all",
                      content: "!restart all",
                    });
                  }}
                >
                  <span>Entire thread</span>
                  <span
                    id={`${descriptionId}-all`}
                    className="whitespace-normal text-2xs text-muted-foreground"
                  >
                    Use the available thread history, within the history limit.
                  </span>
                </DropdownMenuItem>
              </DropdownMenuSubContent>
            </DropdownMenuPortal>
          </DropdownMenuSub>
          <DropdownMenuItem
            onSelect={() => {
              void action.run({ type: "compact", content: "!compact" });
            }}
          >
            Compact context
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      {action.error ? (
        <span role="alert" className="text-xs text-destructive">
          {action.error}
        </span>
      ) : null}
    </span>
  );
}
