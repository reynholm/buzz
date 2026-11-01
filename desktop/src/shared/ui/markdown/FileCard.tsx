import * as React from "react";
import { Download, FileText } from "lucide-react";
import { toast } from "sonner";

import { invokeTauri } from "@/shared/api/tauri";
import { useOpenMarkdownAttachmentPreview } from "@/shared/context/MarkdownAttachmentPreviewContext";
import { useSmoothCorners } from "@/shared/ui/smoothCorners";

/** Human-readable byte size: "820 B", "12.4 KB", "3.1 MB". */
function formatFileSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let size = bytes / 1024;
  let i = 0;
  while (size >= 1024 && i < units.length - 1) {
    size /= 1024;
    i += 1;
  }
  return `${size < 10 ? size.toFixed(1) : Math.round(size)} ${units[i]}`;
}

/**
 * File card for a generic (non-image, non-video) attachment: icon, filename,
 * size, and a download action.
 *
 * Downloads go through the native `download_file` Tauri command (HTTP inside
 * the app's tunnel + a save dialog), not a plain `<a download>` link. A bare
 * link navigates the webview to the blob URL, which escapes to the OS browser
 * and gets bounced to a corporate CDN interstitial ("browser not supported").
 * The native command mirrors the image-download path.
 */
export function FileCard({
  href,
  filename,
  size,
}: {
  href: string;
  filename: string;
  size?: number;
}) {
  const cardRef = React.useRef<HTMLButtonElement | null>(null);
  const sizeLabel = size != null ? formatFileSize(size) : "";
  useSmoothCorners(cardRef);
  const openPreview = useOpenMarkdownAttachmentPreview();
  const canPreview =
    openPreview != null && /\.(?:md|markdown)$/i.test(filename);
  const download = () => {
    void invokeTauri("download_file", { url: href, filename }).catch(
      (err: unknown) => {
        toast.error(
          err instanceof Error
            ? err.message
            : typeof err === "string"
              ? err
              : "Download failed",
        );
      },
    );
  };

  if (canPreview) {
    return (
      <span
        data-testid="file-card"
        className="my-1 inline-flex max-w-sm items-center rounded-2xl border border-border/70 bg-muted/40"
        style={{ borderRadius: "1rem" }}
      >
        <button
          ref={cardRef}
          type="button"
          aria-label={`Preview ${filename}`}
          onClick={(event) =>
            openPreview({ href, filename, size }, event.currentTarget)
          }
          className="inline-flex min-w-0 flex-1 items-center gap-3 rounded-l-2xl px-3 py-2 text-left transition-colors hover:bg-muted/70 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
        >
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-background text-muted-foreground">
            <FileText aria-hidden="true" className="h-4 w-4" />
          </span>
          <span className="min-w-0 flex-1">
            <span className="block truncate text-sm font-medium text-foreground">
              {filename}
            </span>
            {sizeLabel ? (
              <span className="block text-xs text-muted-foreground">
                {sizeLabel}
              </span>
            ) : null}
          </span>
        </button>
        <button
          type="button"
          aria-label={`Download ${filename}`}
          onClick={download}
          className="mr-2 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-muted/70 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
        >
          <Download aria-hidden="true" className="h-4 w-4" />
        </button>
      </span>
    );
  }

  return (
    <button
      ref={cardRef}
      type="button"
      onClick={download}
      data-testid="file-card"
      className="my-1 inline-flex max-w-sm items-center gap-3 rounded-2xl border border-border/70 bg-muted/40 px-3 py-2 text-left no-underline transition-colors hover:bg-muted/70"
      style={{ borderRadius: "1rem" }}
    >
      <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-background text-muted-foreground">
        <FileText className="h-4 w-4" />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm font-medium text-foreground">
          {filename}
        </span>
        {sizeLabel ? (
          <span className="block text-xs text-muted-foreground">
            {sizeLabel}
          </span>
        ) : null}
      </span>
      <Download className="h-4 w-4 shrink-0 text-muted-foreground" />
    </button>
  );
}
