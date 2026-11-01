import * as React from "react";

const SESSION_KEY = "buzz.desktop.markdown-preview-width";
const MIN_WIDTH = 320;

function readWidth(): number | null {
  try {
    const raw = window.sessionStorage.getItem(SESSION_KEY);
    const width = Number(raw);
    return raw && Number.isFinite(width) && width >= MIN_WIDTH ? width : null;
  } catch {
    return null;
  }
}

/** Keep document width independent from the conversation's panel preferences. */
export function useMarkdownPreviewWidth() {
  const panelRef = React.useRef<HTMLElement>(null);
  const cleanupRef = React.useRef<(() => void) | null>(null);
  const [width, setWidth] = React.useState(readWidth);

  React.useEffect(() => {
    try {
      if (width === null) window.sessionStorage.removeItem(SESSION_KEY);
      else window.sessionStorage.setItem(SESSION_KEY, String(width));
    } catch {
      // Resizing still works when session storage is unavailable.
    }
  }, [width]);
  React.useEffect(() => () => cleanupRef.current?.(), []);

  const clampWidth = (next: number) => {
    const available = panelRef.current?.parentElement?.clientWidth ?? MIN_WIDTH;
    return Math.max(Math.min(MIN_WIDTH, available), Math.min(available, next));
  };
  const onPointerDown = (event: React.PointerEvent<HTMLButtonElement>) => {
    if (event.button !== 0 || !panelRef.current) return;
    event.preventDefault();
    cleanupRef.current?.();
    const panel = panelRef.current;
    const startX = event.clientX;
    const startWidth = panel.offsetWidth;
    const scale = startWidth / panel.getBoundingClientRect().width;
    const cursor = document.body.style.cursor;
    const userSelect = document.body.style.userSelect;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    const move = (next: PointerEvent) => {
      setWidth(clampWidth(startWidth + (startX - next.clientX) * scale));
    };
    const end = () => {
      document.body.style.cursor = cursor;
      document.body.style.userSelect = userSelect;
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
      cleanupRef.current = null;
    };
    cleanupRef.current = end;
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
  };
  const onKeyDown = (event: React.KeyboardEvent<HTMLButtonElement>) => {
    const current = panelRef.current?.offsetWidth ?? MIN_WIDTH;
    const next = {
      ArrowLeft: current + 32,
      ArrowRight: current - 32,
      Home: MIN_WIDTH,
      End: Number.MAX_SAFE_INTEGER,
    }[event.key];
    if (next === undefined) return;
    event.preventDefault();
    setWidth(clampWidth(next));
  };

  return {
    panelRef,
    onPointerDown,
    onKeyDown,
    onReset: () => setWidth(null),
    style: {
      "--markdown-preview-width":
        width === null
          ? "min(42rem, max(320px, calc(100% - 6rem)), 100%)"
          : `min(${width}px, 100%)`,
    } as React.CSSProperties,
  };
}
