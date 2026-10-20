import * as React from "react";

import {
  formatContextPercent,
  isHighContext,
} from "@/features/messages/lib/sessionStatus";
import type { MessageSessionStatus as SessionStatus } from "@/features/messages/types";
import { cn } from "@/shared/lib/cn";

type Segment = { key: string; text: string; percent?: number };

function segmentsOf(status: SessionStatus): Segment[] {
  const segments: Segment[] = [];
  const { explanation, percent } = status;
  if (explanation) {
    segments.push({ key: "title", text: explanation.title });
    if (explanation.detail) {
      segments.push({ key: "detail", text: explanation.detail });
    }
    explanation.history?.forEach((text, index) => {
      segments.push({ key: `history-${index}`, text });
    });
  }
  if (percent !== undefined) {
    segments.push({
      key: "percent",
      text: formatContextPercent(percent),
      percent,
    });
  }
  return segments;
}

/**
 * The agent's self-reported session status beneath its message: a muted,
 * noninteractive line. Plain text only (no tab stop, live region or label);
 * visual separators are hidden from assistive technology and replaced by a
 * spoken pause. A high context percentage is emphasized with the warning
 * token and says "(high)", so it never relies on color alone.
 */
export function MessageSessionStatus({ status }: { status: SessionStatus }) {
  const segments = segmentsOf(status);
  if (segments.length === 0) return null;
  return (
    <p
      className="mt-1 text-xs leading-4 text-muted-foreground/70"
      data-testid="message-session-status"
    >
      {segments.map((segment, index) => {
        const high =
          segment.percent !== undefined && isHighContext(segment.percent);
        return (
          <React.Fragment key={segment.key}>
            {index > 0 ? <span aria-hidden="true">{" · "}</span> : null}
            <span
              className={cn(high && "font-medium text-warning")}
              data-level={
                segment.percent === undefined
                  ? undefined
                  : high
                    ? "high"
                    : "normal"
              }
              data-testid={
                segment.percent === undefined
                  ? undefined
                  : "message-session-status-percent"
              }
            >
              {index > 0 ? <span className="sr-only">, </span> : null}
              {segment.text}
            </span>
          </React.Fragment>
        );
      })}
    </p>
  );
}
