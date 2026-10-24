import * as React from "react";
import { Virtualizer, type VirtualizerHandle } from "virtua";

import type { TimelineMessage } from "@/features/messages/types";
import { estimateRowHeight } from "@/features/messages/lib/rowHeightEstimate";
import type { TimelineVirtualizerApi } from "./TimelineMessageList";
import { useVirtualizedBottomSettle } from "./useVirtualizedBottomSettle";
import { useVirtualizedThreadAnchor } from "./useVirtualizedThreadAnchor";

type ReplyItem = {
  entry: { message: TimelineMessage; summary: unknown };
  isContinuation: boolean;
};

type Props<T extends ReplyItem> = {
  items: T[];
  renderItem: (item: T) => React.ReactElement;
  scrollRef: React.RefObject<HTMLDivElement | null>;
  contentRef: React.RefObject<HTMLDivElement | null>;
  headId: string;
  onApiChange: (api: TimelineVirtualizerApi | null) => void;
  onAtBottomChange: (atBottom: boolean) => void;
  onRangeChange: () => void;
};

export function VirtualizedThreadReplies<T extends ReplyItem>({
  items,
  renderItem,
  scrollRef,
  contentRef,
  headId,
  onApiChange,
  onAtBottomChange,
  onRangeChange,
}: Props<T>) {
  const hostRef = React.useRef<HTMLDivElement>(null);
  const listRef = React.useRef<VirtualizerHandle>(null);
  const lengthRef = React.useRef(items.length + 1);
  lengthRef.current = items.length + 1;
  const indexById = React.useMemo(
    () => new Map(items.map((item, index) => [item.entry.message.id, index])),
    [items],
  );
  const indexRef = React.useRef(indexById);
  indexRef.current = indexById;
  const data = React.useMemo(() => [...items, null], [items]);
  const [geometry, setGeometry] = React.useState({ margin: 0, buffer: 1_000 });
  const { cancel: cancelReadingAnchor, restore: restoreReadingAnchor } =
    useVirtualizedThreadAnchor(indexById, hostRef, scrollRef, listRef);
  const targetIntentRef = React.useRef(false);
  const onReaderNavigation = React.useCallback(() => {
    targetIntentRef.current = false;
    cancelReadingAnchor();
    // End at an already-clamped floor may produce no scroll event. Reconcile
    // ownership now so the next live reply can resume bottom following.
    const scroller = scrollRef.current;
    if (scroller)
      onAtBottomChange(
        scroller.scrollHeight - scroller.clientHeight - scroller.scrollTop <=
          32,
      );
  }, [cancelReadingAnchor, onAtBottomChange, scrollRef]);
  const { cancel: cancelBottom, settle: settleBottom } =
    useVirtualizedBottomSettle(
      hostRef,
      listRef,
      lengthRef,
      scrollRef,
      onReaderNavigation,
    );
  const cancel = React.useCallback(() => {
    // A centered target can coincide with the physical floor in a short
    // thread. Appends must not turn that target into bottom-follow intent.
    targetIntentRef.current = true;
    onAtBottomChange(false);
    cancelReadingAnchor();
    cancelBottom();
  }, [cancelBottom, cancelReadingAnchor, onAtBottomChange]);
  const settle = React.useCallback(() => {
    targetIntentRef.current = false;
    cancelReadingAnchor();
    settleBottom();
  }, [cancelReadingAnchor, settleBottom]);

  React.useLayoutEffect(() => {
    const host = hostRef.current;
    const scroller = scrollRef.current;
    const content = contentRef.current;
    if (!host || !scroller || !content) return;
    const measure = () => {
      const margin =
        host.getBoundingClientRect().top -
        scroller.getBoundingClientRect().top +
        scroller.scrollTop;
      const buffer = scroller.clientHeight * 3;
      setGeometry((previous) =>
        previous.margin === margin && previous.buffer === buffer
          ? previous
          : { margin, buffer },
      );
      restoreReadingAnchor();
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(content);
    observer.observe(scroller);
    return () => observer.disconnect();
  }, [contentRef, restoreReadingAnchor, scrollRef]);

  React.useLayoutEffect(() => {
    onApiChange({
      cancelBottomIntent: cancel,
      scrollToBottom: settle,
      settleAtBottom: settle,
      scrollToMessage(id) {
        cancel();
        if (id === headId) {
          scrollRef.current?.scrollTo({ top: 0 });
          return true;
        }
        const index = indexRef.current.get(id);
        if (index === undefined) return false;
        listRef.current?.scrollToIndex(index, { align: "center" });
        return true;
      },
    });
    return () => onApiChange(null);
  }, [cancel, headId, onApiChange, scrollRef, settle]);

  const handleScroll = React.useCallback(() => {
    const scroller = scrollRef.current;
    if (!scroller) return;
    onRangeChange();
    const atBottom =
      scroller.scrollHeight - scroller.clientHeight - scroller.scrollTop <= 32;
    if (!atBottom) cancelBottom();
    onAtBottomChange(!targetIntentRef.current && atBottom);
  }, [cancelBottom, onAtBottomChange, onRangeChange, scrollRef]);
  const estimate = React.useCallback((item: T | null) => {
    if (!item) return 96;
    return (
      estimateRowHeight(item.entry.message, {
        isContinuation: item.isContinuation,
      }) + (item.entry.summary ? 36 : 0)
    );
  }, []);

  // Virtualizer items create layout containment stacking contexts. Keep their
  // floating action rails above the thread head when the two overlap.
  return (
    <div
      ref={hostRef}
      className="relative z-10"
      data-testid="virtualized-thread-replies"
    >
      <Virtualizer
        ref={listRef}
        data={data}
        itemSize={estimate}
        scrollRef={scrollRef}
        startMargin={geometry.margin}
        bufferSize={geometry.buffer}
        onScroll={handleScroll}
      >
        {(item) =>
          item ? (
            renderItem(item)
          ) : (
            <div
              aria-hidden
              className="h-[var(--composer-overlay-height,6rem)]"
              key="composer-spacer"
            />
          )
        }
      </Virtualizer>
    </div>
  );
}
