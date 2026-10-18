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
  const [expired, setExpired] = React.useState(
    () => Date.now() / 1000 - createdAt > 900,
  );
  React.useEffect(() => {
    setExpired(Date.now() / 1000 - createdAt > 900);
    if (!control.targets.some((target) => target.state === "unconfirmed"))
      return;
    const delay = Math.max(0, (createdAt + 901) * 1000 - Date.now());
    const timer = setTimeout(() => setExpired(true), delay);
    return () => clearTimeout(timer);
  }, [createdAt, control.targets]);
  return (
    <div
      className="space-y-1 text-sm text-muted-foreground"
      data-testid="message-session-control"
    >
      {control.targets.map((target) => (
        <p
          key={target.pubkey}
          data-state={
            expired && target.state === "unconfirmed" ? "expired" : target.state
          }
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
            {expired && target.state === "unconfirmed"
              ? "Request expired · Result not confirmed"
              : target.detail}
          </span>
        </p>
      ))}
    </div>
  );
}
