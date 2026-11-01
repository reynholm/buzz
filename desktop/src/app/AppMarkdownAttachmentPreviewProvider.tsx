import * as React from "react";
import { useLocation } from "@tanstack/react-router";

import {
  MarkdownAttachmentPreviewContext,
  MarkdownAttachmentPreviewOpenContext,
  type MarkdownPreviewAttachment,
} from "@/shared/context/MarkdownAttachmentPreviewContext";

const MarkdownAttachmentPreview = React.lazy(() =>
  import("@/shared/ui/markdown/MarkdownAttachmentPreview").then((module) => ({
    default: module.MarkdownAttachmentPreview,
  })),
);

/** Own one document overlay in the current conversation surface. */
export function AppMarkdownAttachmentPreviewProvider({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  const routeHref = useLocation({ select: (location) => location.href });
  const [preview, setPreview] = React.useState<{
    attachment: MarkdownPreviewAttachment;
    routeHref: string;
  } | null>(null);
  const triggerRef = React.useRef<HTMLElement | null>(null);
  const open = React.useCallback(
    (attachment: MarkdownPreviewAttachment, trigger: HTMLElement) => {
      triggerRef.current = trigger;
      setPreview({ attachment, routeHref });
    },
    [routeHref],
  );
  const close = React.useCallback(() => {
    setPreview(null);
    if (triggerRef.current?.isConnected) {
      triggerRef.current.focus({ preventScroll: true });
    }
    triggerRef.current = null;
  }, []);

  React.useEffect(() => {
    setPreview((current) =>
      current?.routeHref === routeHref ? current : null,
    );
  }, [routeHref]);

  return (
    <MarkdownAttachmentPreviewContext.Provider value={open}>
      <MarkdownAttachmentPreviewOpenContext.Provider
        value={preview?.routeHref === routeHref}
      >
        <div className="relative flex min-h-0 min-w-0 flex-1 flex-col">
          {children}
          {preview?.routeHref === routeHref ? (
            <React.Suspense fallback={null}>
              <MarkdownAttachmentPreview
                key={preview.attachment.href}
                attachment={preview.attachment}
                onClose={close}
              />
            </React.Suspense>
          ) : null}
        </div>
      </MarkdownAttachmentPreviewOpenContext.Provider>
    </MarkdownAttachmentPreviewContext.Provider>
  );
}
