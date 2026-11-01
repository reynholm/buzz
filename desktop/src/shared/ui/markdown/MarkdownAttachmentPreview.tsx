import * as React from "react";
import { Download, X } from "lucide-react";
import { toast } from "sonner";

import { invokeTauri } from "@/shared/api/tauri";
import { fetchMediaBytes } from "@/shared/api/tauriMedia";
import type { MarkdownPreviewAttachment } from "@/shared/context/MarkdownAttachmentPreviewContext";
import { Button } from "@/shared/ui/button";
import { Markdown } from "@/shared/ui/markdown";
import { useMarkdownPreviewWidth } from "./useMarkdownPreviewWidth";

const MAX_PREVIEW_BYTES = 2 * 1024 * 1024;
const TOO_LARGE =
  "This document exceeds the 2 MiB preview limit. Download it to read the complete file.";

type PreviewState =
  | { phase: "loading" }
  | { phase: "ready"; content: string }
  | { phase: "error"; message: string; retry: boolean };

// Parser/render failures belong to this document, preserving the header and chat.
class MarkdownDocumentErrorBoundary extends React.Component<
  { children: React.ReactNode },
  { failed: boolean }
> {
  override state = { failed: false };

  static getDerivedStateFromError() {
    return { failed: true };
  }

  override render() {
    if (this.state.failed) {
      return (
        <p role="alert" className="text-sm text-muted-foreground">
          Could not render this document. Download it to read the complete file.
        </p>
      );
    }
    return this.props.children;
  }
}

/** Scroll a Markdown document above the conversation without resizing it. */
export function MarkdownAttachmentPreview({
  attachment,
  onClose,
}: {
  attachment: MarkdownPreviewAttachment;
  onClose: () => void;
}) {
  const { href, filename, size } = attachment;
  const [state, setState] = React.useState<PreviewState>({ phase: "loading" });
  const [attempt, setAttempt] = React.useState(0);
  const closeRef = React.useRef<HTMLButtonElement>(null);
  const width = useMarkdownPreviewWidth();

  React.useEffect(() => {
    closeRef.current?.focus({ preventScroll: true });
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.defaultPrevented) {
        event.preventDefault();
        onClose();
      }
    };
    document.addEventListener("keydown", handleEscape);
    return () => document.removeEventListener("keydown", handleEscape);
  }, [onClose]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: attempt explicitly starts a fresh request when Retry is pressed.
  React.useEffect(() => {
    const controller = new AbortController();
    setState({ phase: "loading" });
    if (size != null && size > MAX_PREVIEW_BYTES) {
      setState({ phase: "error", message: TOO_LARGE, retry: false });
      return () => controller.abort();
    }
    async function load() {
      try {
        const bytes = await fetchMediaBytes(
          href,
          controller.signal,
          MAX_PREVIEW_BYTES,
        );
        if (controller.signal.aborted) return;
        if (bytes.byteLength > MAX_PREVIEW_BYTES) {
          setState({ phase: "error", message: TOO_LARGE, retry: false });
          return;
        }
        let content: string;
        try {
          content = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
        } catch {
          setState({
            phase: "error",
            message:
              "This document is not valid UTF-8. Download it to open it with another encoding.",
            retry: false,
          });
          return;
        }
        setState({ phase: "ready", content });
      } catch (error) {
        if (controller.signal.aborted) return;
        const message =
          error instanceof Error
            ? error.message
            : typeof error === "string"
              ? error
              : "Could not load this document.";
        setState({ phase: "error", message, retry: true });
      }
    }
    void load();
    return () => controller.abort();
  }, [href, size, attempt]);

  return (
    <section
      ref={width.panelRef}
      aria-label={`Markdown preview: ${filename}`}
      className="absolute inset-y-0 right-0 z-50 flex w-full min-w-0 flex-col border-l border-border bg-background shadow-xl sm:w-[var(--markdown-preview-width)]"
      data-testid="markdown-preview"
      style={width.style}
    >
      <button
        aria-label="Resize Markdown preview"
        className="group absolute inset-y-0 left-0 z-10 hidden w-3 -translate-x-1/2 cursor-col-resize touch-none sm:block"
        onDoubleClick={width.onReset}
        onKeyDown={width.onKeyDown}
        onPointerDown={width.onPointerDown}
        title="Drag to resize. Double-click to reset width. Use arrow keys, Home or End."
        type="button"
      >
        <span className="absolute inset-y-0 left-1/2 w-px bg-transparent group-hover:bg-border group-focus-visible:bg-border" />
      </button>
      <header className="flex min-h-13 shrink-0 items-center gap-2 border-b border-border px-4 py-2">
        <h2
          className="min-w-0 flex-1 truncate text-sm font-semibold"
          title={filename}
        >
          {filename}
        </h2>
        <Button
          aria-label={`Download ${filename}`}
          onClick={() => {
            void invokeTauri("download_file", { url: href, filename }).catch(
              (error: unknown) => {
                toast.error(
                  error instanceof Error
                    ? error.message
                    : typeof error === "string"
                      ? error
                      : "Download failed",
                );
              },
            );
          }}
          size="icon"
          variant="ghost"
        >
          <Download aria-hidden="true" />
        </Button>
        <Button
          ref={closeRef}
          aria-label="Close Markdown preview"
          onClick={onClose}
          size="icon"
          variant="ghost"
        >
          <X aria-hidden="true" />
        </Button>
      </header>
      <div
        aria-label="Document content"
        className="min-h-0 min-w-0 flex-1 overflow-auto overscroll-contain p-6"
        data-testid="markdown-preview-body"
        role="document"
        // biome-ignore lint/a11y/noNoninteractiveTabindex: Keyboard focus lets WebKit users scroll the document body.
        tabIndex={0}
      >
        {state.phase === "loading" ? (
          <p role="status" className="text-sm text-muted-foreground">
            Loading document…
          </p>
        ) : null}
        {state.phase === "error" ? (
          <div className="space-y-3">
            <p role="alert" className="text-sm text-muted-foreground">
              {state.message}
            </p>
            {state.retry ? (
              <Button
                onClick={() => setAttempt((value) => value + 1)}
                size="sm"
                variant="outline"
              >
                Retry
              </Button>
            ) : null}
          </div>
        ) : null}
        {state.phase === "ready" ? (
          <MarkdownDocumentErrorBoundary key={`${href}:${attempt}`}>
            {state.content.trim() ? (
              <Markdown
                blockCode
                content={state.content}
                hardLineBreaks={false}
                maxNestingDepth={128}
                interactive={false}
              />
            ) : (
              <p className="text-sm text-muted-foreground">
                This document is empty.
              </p>
            )}
          </MarkdownDocumentErrorBoundary>
        ) : null}
      </div>
    </section>
  );
}
