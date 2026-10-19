import * as React from "react";
import type { MessageSessionControl } from "@/features/messages/lib/sessionControl";
/** Compact request presentation; raw emoji pills never confer confirmation. */
export function MessageSessionControlRow({
  actor,
  control,
  createdAt,
}: {
  actor: string;
  createdAt: number;
  control: MessageSessionControl;
}) {
  const [now, setNow] = React.useState(() => Date.now() / 1000);
  React.useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const deadlines = control.targets.flatMap((target) =>
      target.state === "unconfirmed"
        ? [createdAt + 901]
        : target.state === "pending" && target.expiresAt !== undefined
          ? [target.expiresAt]
          : [],
    );
    const refresh = () => {
      const current = Date.now() / 1000;
      setNow(current);
      const next = Math.min(
        ...deadlines.filter((deadline) => deadline > current),
      );
      if (Number.isFinite(next))
        timer = setTimeout(refresh, Math.max(1, (next - current) * 1000));
    };
    refresh();
    return () => clearTimeout(timer);
  }, [createdAt, control.targets]);
  return (
    <div
      className="space-y-1 text-sm text-muted-foreground"
      data-testid="message-session-control"
    >
      {control.targets.map((target) => {
        const expired =
          (target.state === "unconfirmed" && now >= createdAt + 901) ||
          (target.state === "pending" &&
            target.expiresAt !== undefined &&
            now >= target.expiresAt);
        return (
          <p
            key={target.pubkey}
            data-state={expired ? "expired" : target.state}
          >
            <span className="font-medium text-foreground">{actor}</span>
            {": "}
            {control.action}
            {" · "}
            <span className="font-medium">{target.name}</span>
            {" · "}
            <span
              className={
                target.state === "failure" ? "text-destructive" : undefined
              }
            >
              {expired
                ? "Request expired · Result not confirmed"
                : target.detail}
            </span>
          </p>
        );
      })}
    </div>
  );
}
