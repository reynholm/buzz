import * as React from "react";
import { BrainCog } from "lucide-react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useRelayAgentsQuery } from "@/features/agents/hooks";
import { useChannelDetailsQuery } from "@/features/channels/hooks";
import { useCommunities } from "@/features/communities/useCommunities";
import { useIdentityQuery } from "@/shared/api/hooks";
import { getEventById } from "@/shared/api/tauriEvents";
import type { Channel, RelayEvent } from "@/shared/api/types";
import type { TimelineMessage } from "@/features/messages/types";
import { loadThreadReplies } from "@/features/messages/useThreadReplies";
import { threadRepliesKey } from "@/features/messages/lib/messageQueryKeys";
import {
  hasPersistedThread,
  projectThreadContextAgents,
} from "@/features/messages/lib/threadContextAgents";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/tooltip";
import { MessageSessionControls } from "./MessageSessionControls";

/** Scope changes unmount selected actions and invalidate their asynchronous work. */
export function ThreadContextControls({
  channel,
  threadHead,
}: {
  channel: Channel | null;
  threadHead: TimelineMessage | null;
}) {
  const { activeCommunity } = useCommunities();
  const identity = useIdentityQuery();
  if (!channel || !threadHead || !hasPersistedThread(threadHead)) return null;
  const rootId = threadHead.rootId ?? threadHead.id;
  const relayUrl = activeCommunity?.relayUrl ?? "";
  const signerPubkey = identity.data?.pubkey ?? "";
  return (
    <ThreadContextPicker
      key={JSON.stringify([relayUrl, signerPubkey, channel.id, rootId])}
      channel={channel}
      head={threadHead}
      rootId={rootId}
      relayUrl={relayUrl}
      signerPubkey={signerPubkey}
    />
  );
}

function ThreadContextPicker({
  channel,
  head,
  rootId,
  relayUrl,
  signerPubkey,
}: {
  channel: Channel;
  head: TimelineMessage;
  rootId: string;
  relayUrl: string;
  signerPubkey: string;
}) {
  const [open, setOpen] = React.useState(false);
  const [selected, setSelected] = React.useState<string | null>(null);
  const queryClient = useQueryClient();
  const directory = useRelayAgentsQuery();
  const currentChannel = useChannelDetailsQuery(channel.id);
  const repliesKey = React.useMemo(
    () => threadRepliesKey(channel.id, rootId),
    [channel.id, rootId],
  );
  // Observe the owner's complete raw data without replacing its notification
  // target validation, retry policy or query function with another observer.
  const ownedReplies = React.useSyncExternalStore(
    React.useCallback(
      (notify) => queryClient.getQueryCache().subscribe(notify),
      [queryClient],
    ),
    React.useCallback(() => {
      const query = queryClient.getQueryCache().find<RelayEvent[]>({
        queryKey: repliesKey,
        exact: true,
      });
      return query?.isActive() ? query.state : undefined;
    }, [queryClient, repliesKey]),
  );
  // Read-only project forum previews disable the ordinary thread owner. Keep
  // their bounded recovery roster isolated from the validated shared query.
  const forumReplies = useQuery({
    queryKey: ["thread-context-forum-replies", channel.id, rootId],
    queryFn: () => loadThreadReplies(queryClient, channel.id, rootId),
    enabled: open && channel.channelType === "forum" && !ownedReplies,
    staleTime: 0,
  });
  const replies =
    ownedReplies ??
    (channel.channelType === "forum" ? forumReplies : undefined);
  const root = useQuery({
    queryKey: ["thread-context-root", channel.id, rootId],
    queryFn: () => getEventById(rootId),
    enabled: open && head.id !== rootId,
  });
  const rootEvent: RelayEvent | undefined =
    head.id === rootId
      ? {
          id: head.id,
          pubkey: head.signerPubkey ?? "",
          kind: head.kind ?? 9,
          tags: head.tags ?? [],
          created_at: head.createdAt,
          content: head.body,
          sig: "",
        }
      : root.data;
  const error =
    directory.isError ||
    currentChannel.isError ||
    replies?.status === "error" ||
    (head.id !== rootId && root.isError);
  const loading =
    directory.isPending ||
    currentChannel.isPending ||
    replies?.status === "pending" ||
    replies?.fetchStatus !== "idle" ||
    (head.id !== rootId && (root.isPending || root.isFetching));
  const agents =
    replies && !error && !loading && rootEvent && currentChannel.data
      ? projectThreadContextAgents({
          rootId,
          channel: currentChannel.data,
          events: [rootEvent, ...(replies.data ?? [])],
          directory: directory.data ?? [],
          signerPubkey,
        })
      : [];
  const target = agents.find((agent) => agent.pubkey === selected);
  React.useEffect(() => {
    if (selected && !target) setSelected(null);
  }, [selected, target]);
  return (
    <DropdownMenu
      open={open}
      onOpenChange={(value) => {
        if (value) setSelected(null);
        setOpen(value);
      }}
    >
      <Tooltip disableHoverableContent>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              aria-label="Manage context"
              data-testid="manage-thread-context"
              className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-foreground hover:bg-accent hover:text-accent-foreground focus-visible:outline-hidden focus-visible:ring-1 focus-visible:ring-ring"
              onMouseDown={(event) => event.preventDefault()}
            >
              <BrainCog aria-hidden="true" className="h-4 w-4 shrink-0" />
            </button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent>Manage context</TooltipContent>
      </Tooltip>
      {target ? (
        <MessageSessionControls
          scope={{
            targetPubkey: target.pubkey,
            channelId: channel.id,
            rootEventId: rootId,
            relayUrl,
            signerPubkey,
          }}
          label={target.label}
          shared={target.shared}
          onBack={() => setSelected(null)}
        />
      ) : (
        <DropdownMenuContent align="start" className="max-w-80" key="agents">
          {!replies ? (
            <p
              role="status"
              className="px-2 py-1.5 text-xs text-muted-foreground"
            >
              Thread context is unavailable here.
            </p>
          ) : error ? (
            <>
              <p
                role="status"
                className="px-2 py-1.5 text-xs text-muted-foreground"
              >
                Could not load thread agents.
              </p>
              <DropdownMenuItem
                onSelect={(event) => {
                  event.preventDefault();
                  if (directory.isError) void directory.refetch();
                  if (currentChannel.isError) void currentChannel.refetch();
                  if (replies.status === "error") {
                    if (ownedReplies) {
                      void queryClient.refetchQueries({
                        queryKey: repliesKey,
                        exact: true,
                      });
                    } else {
                      void forumReplies.refetch();
                    }
                  }
                  if (head.id !== rootId && root.isError) void root.refetch();
                }}
              >
                Retry
              </DropdownMenuItem>
            </>
          ) : loading ? (
            <p
              role="status"
              className="px-2 py-1.5 text-xs text-muted-foreground"
            >
              Loading thread agents…
            </p>
          ) : agents.length ? (
            agents.map((agent) => (
              <DropdownMenuItem
                key={agent.pubkey}
                onSelect={(event) => {
                  event.preventDefault();
                  setSelected(agent.pubkey);
                }}
              >
                {agent.label}
              </DropdownMenuItem>
            ))
          ) : (
            <p
              role="status"
              className="px-2 py-1.5 text-xs text-muted-foreground"
            >
              No agents you can manage in this thread.
            </p>
          )}
        </DropdownMenuContent>
      )}
    </DropdownMenu>
  );
}
