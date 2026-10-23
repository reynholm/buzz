import * as React from "react";
import type { VirtualizerHandle } from "virtua";

type ReadingAnchor = { id: string; offset: number; generation: number };

/** Preserve the reading row when depth-first thread entries change above it. */
export function useVirtualizedThreadAnchor(
  indices: ReadonlyMap<string, number>,
  hostRef: React.RefObject<HTMLDivElement | null>,
  scrollRef: React.RefObject<HTMLDivElement | null>,
  listRef: React.RefObject<VirtualizerHandle | null>,
) {
  const committedIndicesRef = React.useRef(indices);
  const indicesRef = React.useRef(indices);
  indicesRef.current = indices;
  const anchorRef = React.useRef<ReadingAnchor | null>(null);
  const generationRef = React.useRef(0);
  const frameRef = React.useRef<number | null>(null);
  const cancel = React.useCallback(() => {
    generationRef.current += 1;
    anchorRef.current = null;
    if (frameRef.current !== null) {
      cancelAnimationFrame(frameRef.current);
      frameRef.current = null;
    }
  }, []);
  const restore = React.useCallback(() => {
    const anchor = anchorRef.current;
    const scroller = scrollRef.current;
    const list = listRef.current;
    if (!anchor || !scroller || !list) return;
    const index = indicesRef.current.get(anchor.id);
    if (index === undefined) {
      cancel();
      return;
    }
    const row = hostRef.current?.querySelector<HTMLElement>(
      `[data-message-id="${CSS.escape(anchor.id)}"]`,
    );
    if (!row) {
      list.scrollToIndex(index, { align: "start", offset: -anchor.offset });
      return;
    }
    const drift =
      row.getBoundingClientRect().top -
      scroller.getBoundingClientRect().top -
      anchor.offset;
    if (Math.abs(drift) > 0.5) list.scrollTo(scroller.scrollTop + drift);
  }, [cancel, hostRef, listRef, scrollRef]);

  // Capture old DOM geometry before committing the new indexed rows. Keep the
  // snapshot local to this render so an abandoned concurrent render cannot
  // publish a reading anchor or replace the last committed index map.
  const snapshot = React.useMemo(() => {
    const scroller = scrollRef.current;
    const host = hostRef.current;
    if (
      !scroller ||
      !host ||
      committedIndicesRef.current === indices ||
      scroller.scrollHeight - scroller.clientHeight - scroller.scrollTop <= 32
    )
      return null;
    const top = scroller.getBoundingClientRect().top;
    const rows = host.querySelectorAll<HTMLElement>("[data-message-id]");
    for (const row of rows) {
      const rect = row.getBoundingClientRect();
      if (rect.bottom <= top) continue;
      if (rect.top >= top + scroller.clientHeight) break;
      const id = row.dataset.messageId;
      if (!id || !indices.has(id)) continue;
      return committedIndicesRef.current.get(id) !== indices.get(id)
        ? { id, offset: rect.top - top, generation: generationRef.current }
        : null;
    }
    return null;
  }, [hostRef, indices, scrollRef]);

  React.useLayoutEffect(() => {
    committedIndicesRef.current = indices;
    if (!snapshot || snapshot.generation !== generationRef.current) return;
    anchorRef.current = snapshot;
    restore();
    if (frameRef.current !== null) cancelAnimationFrame(frameRef.current);
    frameRef.current = requestAnimationFrame(() => {
      frameRef.current = null;
      restore();
    });
  }, [indices, restore, snapshot]);
  React.useEffect(() => cancel, [cancel]);
  return { cancel, restore };
}
