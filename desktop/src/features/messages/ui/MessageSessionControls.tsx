import { useId } from "react";
import type { useSessionControlAction } from "@/features/messages/lib/useSessionControlAction";
import type { SessionCommand } from "@/features/messages/lib/sessionControl";
import {
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSub,
  DropdownMenuSubTrigger,
  DropdownMenuSubContent,
  DropdownMenuPortal,
} from "@/shared/ui/dropdown-menu";

/** The thread picker owns the action lifetime while this menu closes. */
export function MessageSessionControls({
  action,
  label,
  shared,
  onBack,
  prepareAudience,
}: {
  action: ReturnType<typeof useSessionControlAction>;
  label: string;
  shared: boolean;
  onBack: () => void;
  prepareAudience?: () => (() => void) | undefined;
}) {
  const descriptionId = useId();
  const run = (command: SessionCommand) => {
    void action.run(command, prepareAudience?.());
  };
  return (
    <>
      <DropdownMenuContent align="start" className="max-w-80">
        <DropdownMenuItem
          disabled={action.pending}
          onSelect={(event) => {
            event.preventDefault();
            onBack();
          }}
        >
          Back to agents
        </DropdownMenuItem>
        <DropdownMenuLabel>{label}</DropdownMenuLabel>
        {shared ? (
          <p className="max-w-72 px-2 py-1.5 text-xs text-muted-foreground">
            This session is shared across the channel.
          </p>
        ) : null}
        <DropdownMenuSub>
          <DropdownMenuSubTrigger
            disabled={action.pending}
            aria-label="Restart"
            aria-describedby={`${descriptionId}-restart`}
          >
            <span className="flex min-w-0 flex-1 flex-col gap-0.5">
              <span>Restart</span>
              <span
                id={`${descriptionId}-restart`}
                className="whitespace-normal text-2xs text-[color:color-mix(in_srgb,hsl(var(--foreground))_90%,black)]"
              >
                Stops current work, including subagents. File changes are kept.
              </span>
            </span>
          </DropdownMenuSubTrigger>
          <DropdownMenuPortal>
            <DropdownMenuSubContent className="max-w-80">
              <DropdownMenuItem
                disabled={action.pending}
                aria-label="From scratch"
                aria-describedby={`${descriptionId}-none`}
                className="flex-col items-start gap-0.5"
                onSelect={() => {
                  run({
                    type: "restart",
                    depth: "none",
                    content: "!restart none",
                  });
                }}
              >
                <span>From scratch</span>
                <span
                  id={`${descriptionId}-none`}
                  className="whitespace-normal text-2xs text-[color:color-mix(in_srgb,hsl(var(--foreground))_90%,black)]"
                >
                  Keep the original assignment and your next request.
                </span>
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={action.pending}
                aria-label="Recent history"
                aria-describedby={`${descriptionId}-recent`}
                className="flex-col items-start gap-0.5"
                onSelect={() => {
                  run({
                    type: "restart",
                    depth: "recent",
                    content: "!restart",
                  });
                }}
              >
                <span>Recent history</span>
                <span
                  id={`${descriptionId}-recent`}
                  className="whitespace-normal text-2xs text-[color:color-mix(in_srgb,hsl(var(--foreground))_90%,black)]"
                >
                  Keep the original assignment and recent messages.
                </span>
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={action.pending}
                aria-label="Entire thread"
                aria-describedby={`${descriptionId}-all`}
                className="flex-col items-start gap-0.5"
                onSelect={() => {
                  run({
                    type: "restart",
                    depth: "all",
                    content: "!restart all",
                  });
                }}
              >
                <span>Entire thread</span>
                <span
                  id={`${descriptionId}-all`}
                  className="whitespace-normal text-2xs text-[color:color-mix(in_srgb,hsl(var(--foreground))_90%,black)]"
                >
                  Use the available thread history, within the history limit.
                </span>
              </DropdownMenuItem>
            </DropdownMenuSubContent>
          </DropdownMenuPortal>
        </DropdownMenuSub>
        <DropdownMenuItem
          disabled={action.pending}
          onSelect={() => {
            run({ type: "compact", content: "!compact" });
          }}
        >
          Compact context
        </DropdownMenuItem>
      </DropdownMenuContent>
      {action.error ? (
        <span role="alert" className="text-xs text-destructive">
          {action.error}
        </span>
      ) : null}
    </>
  );
}
