import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauri } from "../lib/api";

export function WindowControls() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (!isTauri()) return;
    const appWindow = getCurrentWindow();
    let disposed = false;
    let unlisten: (() => void) | undefined;

    const sync = async () => {
      try {
        const value = await appWindow.isMaximized();
        if (!disposed) setMaximized(value);
      } catch {
        // 窗口状态读取失败时保持当前图标。
      }
    };

    void sync();
    appWindow
      .onResized(() => void sync())
      .then((dispose) => {
        if (disposed) dispose();
        else unlisten = dispose;
      })
      .catch(() => {
        // 浏览器预览没有 Tauri 窗口事件。
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const act = (action: "minimize" | "toggleMaximize" | "close") => {
    if (!isTauri()) return;
    const appWindow = getCurrentWindow();
    const promise =
      action === "minimize"
        ? appWindow.minimize()
        : action === "toggleMaximize"
          ? appWindow.toggleMaximize()
          : appWindow.close();
    void promise.catch(() => {
      // 关闭/最小化失败时不打断界面。
    });
  };

  return (
    <div className="window-controls" aria-label="窗口控制">
      <button
        className="window-control"
        title="最小化"
        aria-label="最小化"
        onClick={() => act("minimize")}
      >
        <svg viewBox="0 0 10 10" aria-hidden="true">
          <path d="M1 5.5h8" />
        </svg>
      </button>
      <button
        className="window-control"
        title={maximized ? "还原" : "最大化"}
        aria-label={maximized ? "还原" : "最大化"}
        onClick={() => act("toggleMaximize")}
      >
        {maximized ? (
          <svg viewBox="0 0 10 10" aria-hidden="true">
            <path d="M3 1.5h5.5V7M1.5 3H7v5.5H1.5z" />
          </svg>
        ) : (
          <svg viewBox="0 0 10 10" aria-hidden="true">
            <rect x="1.5" y="1.5" width="7" height="7" />
          </svg>
        )}
      </button>
      <button
        className="window-control close"
        title="关闭"
        aria-label="关闭"
        onClick={() => act("close")}
      >
        <svg viewBox="0 0 10 10" aria-hidden="true">
          <path d="M1.5 1.5l7 7M8.5 1.5l-7 7" />
        </svg>
      </button>
    </div>
  );
}
