import { createContext, useContext } from "react";

export type MarkdownPreviewAttachment = {
  href: string;
  filename: string;
  size?: number;
};

export const MarkdownAttachmentPreviewContext = createContext<
  ((attachment: MarkdownPreviewAttachment, trigger: HTMLElement) => void) | null
>(null);

export const MarkdownAttachmentPreviewOpenContext = createContext(false);

export function useMarkdownAttachmentPreviewOpen() {
  return useContext(MarkdownAttachmentPreviewOpenContext);
}

export function useOpenMarkdownAttachmentPreview() {
  return useContext(MarkdownAttachmentPreviewContext);
}
