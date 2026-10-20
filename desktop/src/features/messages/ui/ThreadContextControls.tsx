import * as React from "react";
import { BrainCog, LoaderCircle } from "lucide-react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useRelayAgentsQuery } from "@/features/agents/hooks";
import { useChannelDetailsQuery } from "@/features/channels/hooks";
import { useCommunities } from "@/features/communities/useCommunities";
import { useUsersBatchQuery } from "@/features/profile/hooks";
import { useIdentityQuery } from "@/shared/api/hooks";
import { getEventById } from "@/shared/api/tauriEvents";
import type { Channel, RelayEvent } from "@/shared/api/types";
import type { TimelineMessage } from "@/features/messages/types";
import { loadThreadReplies } from "@/features/messages/useThreadReplies";
import { threadRepliesKey } from "@/features/messages/lib/messageQueryKeys";
import { useSessionControlAction } from "@/features/messages/lib/useSessionControlAction";
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
import { UserAvatar } from "@/shared/ui/UserAvatar";
import { MessageSessionControls } from "./MessageSessionControls";

/** Scope changes unmount selected actions and invalidate their asynchronous work. */
export function ThreadContextControls({
  channel,
  threadHead,
  prepareAgentPin,
  composerRef,
}: {
  channel: Channel | null;
  threadHead: TimelineMessage | null;
  prepareAgentPin?: (pubkey: string) => () => void;
  composerRef?: React.RefObject<HTMLFormElement | null>;
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
      prepareAgentPin={prepareAgentPin}
      composerRef={composerRef}
    />
  );
}

function ThreadContextPicker({
  channel,
  head,
  rootId,
  relayUrl,
  signerPubkey,
  prepareAgentPin,
  composerRef,
}: {
  channel: Channel;
  head: TimelineMessage;
  rootId: string;
  relayUrl: string;
  signerPubkey: string;
  prepareAgentPin?: (pubkey: string) => () => void;
  composerRef?: React.RefObject<HTMLFormElement | null>;
}) {
  const [open, setOpen] = React.useState(false);
  const [selected, setSelected] = React.useState<string | null>(null);
  const { triggerRef, menuPosition } = useContextMenuPosition(
    open,
    composerRef,
  );
  const action = useSessionControlAction({
    targetPubkey: selected ?? "",
    channelId: channel.id,
    rootEventId: rootId,
    relayUrl,
    signerPubkey,
  });
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
  const profiles = useUsersBatchQuery(
    agents.map((agent) => agent.pubkey),
    { enabled: open },
  );
  const target = agents.find((agent) => agent.pubkey === selected);
  React.useEffect(() => {
    if (selected && !target && !loading && !error) setSelected(null);
  }, [error, loading, selected, target]);
  return (
    <DropdownMenu
      open={open}
      onOpenChange={(value) => {
        if (value && !action.pending) setSelected(null);
        setOpen(value);
      }}
    >
      <Tooltip disableHoverableContent>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              ref={triggerRef}
              aria-label="Manage context"
              aria-busy={action.pending}
              aria-description={
                action.pending ? "Sending session control…" : undefined
              }
              data-testid="manage-thread-context"
              className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-foreground hover:bg-accent hover:text-accent-foreground focus-visible:outline-hidden focus-visible:ring-1 focus-visible:ring-ring"
              onMouseDown={(event) => event.preventDefault()}
            >
              {action.pending ? (
                <LoaderCircle
                  aria-hidden="true"
                  className="h-4 w-4 shrink-0 animate-spin motion-reduce:animate-none"
                />
              ) : (
                <BrainCog aria-hidden="true" className="h-4 w-4 shrink-0" />
              )}
            </button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent>Manage context</TooltipContent>
      </Tooltip>
      {target ? (
        <MessageSessionControls
          action={action}
          label={target.label}
          shared={target.shared}
          onBack={() => setSelected(null)}
          prepareAudience={() => prepareAgentPin?.(target.pubkey)}
          menuPosition={menuPosition}
        />
      ) : (
        <DropdownMenuContent
          {...menuPosition}
          align="start"
          className="max-w-80"
          key="agents"
        >
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
                <span aria-hidden="true" className="shrink-0">
                  <UserAvatar
                    avatarUrl={
                      profiles.data?.profiles[agent.pubkey]?.avatarUrl ?? null
                    }
                    displayName={agent.label}
                    shape="squircle"
                    size="xs"
                    testId="thread-context-agent-avatar"
                  />
                </span>
                <span className="min-w-0 break-words">{agent.label}</span>
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

/** Match the mention overlay's gap above the entire composer, including uploads. */
function useContextMenuPosition(
  open: boolean,
  composerRef?: React.RefObject<HTMLFormElement | null>,
) {
  const triggerRef = React.useRef<HTMLButtonElement>(null);
  const [offsets, setOffsets] = React.useState({
    sideOffset: 4,
    alignOffset: 0,
  });
  React.useLayoutEffect(() => {
    const composer = composerRef?.current;
    const trigger = triggerRef.current;
    if (!open || !composer || !trigger) return;
    const update = () => {
      const form = composer.getBoundingClientRect();
      const button = trigger.getBoundingClientRect();
      const gap =
        Number.parseFloat(getComputedStyle(document.documentElement).fontSize) /
          4 || 4;
      const next = {
        sideOffset: button.top - form.top + gap,
        alignOffset: form.left - button.left,
      };
      setOffsets((previous) =>
        previous.sideOffset === next.sideOffset &&
        previous.alignOffset === next.alignOffset
          ? previous
          : next,
      );
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(composer);
    observer.observe(trigger);
    window.addEventListener("resize", update);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", update);
    };
  }, [composerRef, open]);
  return { triggerRef, menuPosition: { side: "top" as const, ...offsets } };
}
