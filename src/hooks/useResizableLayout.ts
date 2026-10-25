import { useCallback, useEffect, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";

const SIDEBAR_STORAGE_KEY = "leonredis.layout.sidebarWidth";
const KEY_STORAGE_KEY = "leonredis.layout.keyWidth";

const DEFAULT_SIDEBAR = 246;
const DEFAULT_KEY = 318;
const MIN_SIDEBAR = 200;
const MAX_SIDEBAR = 380;
const MIN_KEY = 260;
const MAX_KEY = 560;
const MIN_DETAIL = 420;
const HANDLE_OFFSET = 3;

type ResizeTarget = "sidebar" | "key";

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), Math.max(min, max));
}

function readWidth(key: string, fallback: number) {
  if (typeof window === "undefined") return fallback;
  const value = Number(window.localStorage.getItem(key));
  return Number.isFinite(value) && value > 0 ? value : fallback;
}

function maxSidebarWidth() {
  return Math.min(
    MAX_SIDEBAR,
    Math.max(MIN_SIDEBAR, window.innerWidth - MIN_KEY - MIN_DETAIL),
  );
}

function maxKeyWidth(sidebarWidth: number) {
  return Math.min(
    MAX_KEY,
    Math.max(MIN_KEY, window.innerWidth - sidebarWidth - MIN_DETAIL),
  );
}

export function useResizableLayout() {
  const [sidebarWidth, setSidebarWidth] = useState(() =>
    clamp(readWidth(SIDEBAR_STORAGE_KEY, DEFAULT_SIDEBAR), MIN_SIDEBAR, MAX_SIDEBAR),
  );
  const [keyWidth, setKeyWidth] = useState(() =>
    clamp(readWidth(KEY_STORAGE_KEY, DEFAULT_KEY), MIN_KEY, MAX_KEY),
  );

  const clampToWindow = useCallback(() => {
    const nextSidebar = clamp(
      sidebarWidth,
      MIN_SIDEBAR,
      maxSidebarWidth(),
    );
    const nextKey = clamp(keyWidth, MIN_KEY, maxKeyWidth(nextSidebar));
    setSidebarWidth(nextSidebar);
    setKeyWidth(nextKey);
  }, [sidebarWidth, keyWidth]);

  useEffect(() => {
    const onResize = () => clampToWindow();
    onResize();
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, [clampToWindow]);

  useEffect(() => {
    window.localStorage.setItem(SIDEBAR_STORAGE_KEY, String(sidebarWidth));
  }, [sidebarWidth]);

  useEffect(() => {
    window.localStorage.setItem(KEY_STORAGE_KEY, String(keyWidth));
  }, [keyWidth]);

  const startResize = useCallback(
    (target: ResizeTarget, event: ReactPointerEvent<HTMLDivElement>) => {
      event.preventDefault();
      const startX = event.clientX;
      const startWidth = target === "sidebar" ? sidebarWidth : keyWidth;
      const sidebarLimit = maxSidebarWidth();
      const keyLimit = maxKeyWidth(sidebarWidth);

      document.body.classList.add("is-resizing");
      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";

      const onMove = (moveEvent: PointerEvent) => {
        const delta = moveEvent.clientX - startX;
        if (target === "sidebar") {
          const nextSidebar = clamp(
            startWidth + delta,
            MIN_SIDEBAR,
            sidebarLimit,
          );
          setSidebarWidth(nextSidebar);
          setKeyWidth((current) =>
            Math.min(current, maxKeyWidth(nextSidebar)),
          );
        } else {
          setKeyWidth(clamp(startWidth + delta, MIN_KEY, keyLimit));
        }
      };

      const stop = () => {
        document.body.classList.remove("is-resizing");
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", stop);
        window.removeEventListener("pointercancel", stop);
      };

      window.addEventListener("pointermove", onMove);
      window.addEventListener("pointerup", stop);
      window.addEventListener("pointercancel", stop);
    },
    [sidebarWidth, keyWidth],
  );

  const resetWidths = useCallback(() => {
    setSidebarWidth(DEFAULT_SIDEBAR);
    setKeyWidth(DEFAULT_KEY);
  }, []);

  return {
    sidebarWidth,
    keyWidth,
    startResize,
    resetWidths,
    sidebarHandleLeft: sidebarWidth - HANDLE_OFFSET,
    keyHandleLeft: keyWidth - HANDLE_OFFSET,
  };
}
