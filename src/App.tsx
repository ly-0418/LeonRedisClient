import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ConnectionDialog } from "./components/ConnectionDialog";
import { KeyBrowser } from "./components/KeyBrowser";
import { KeyDetail } from "./components/KeyDetail";
import { PerformancePage } from "./components/PerformancePage";
import { Sidebar } from "./components/Sidebar";
import { Toast } from "./components/Toast";
import { TopBar } from "./components/TopBar";
import { useResizableLayout } from "./hooks/useResizableLayout";
import { isTauri } from "./lib/api";
import type { ConnectionProfile } from "./lib/types";
import { useAppStore } from "./stores/appStore";
import { useSettingsStore } from "./stores/settingsStore";

export type DialogState =
  | { mode: "create" }
  | { mode: "edit"; profile: ConnectionProfile }
  | null;

export default function App() {
  const bootstrap = useAppStore((state) => state.bootstrap);
  const refreshKeys = useAppStore((state) => state.refreshKeys);
  const connection = useAppStore((state) => state.connection);
  const themeMode = useSettingsStore((state) => state.themeMode);
  const [dialog, setDialog] = useState<DialogState>(null);
  const [performanceOpen, setPerformanceOpen] = useState(false);
  const {
    sidebarWidth,
    keyWidth,
    startResize,
    resetWidths,
    sidebarHandleLeft,
    keyHandleLeft,
  } = useResizableLayout();

  useEffect(() => {
    void bootstrap();
  }, [bootstrap]);

  useEffect(() => {
    if (!connection) setPerformanceOpen(false);
  }, [connection]);

  useEffect(() => {
    const observer = new MutationObserver((records) => {
      for (const record of records) {
        for (const removed of record.removedNodes) {
          if (!(removed instanceof HTMLElement)) continue;
          const candidates = removed.matches(".modal-backdrop")
            ? [removed]
            : Array.from(removed.querySelectorAll(".modal-backdrop"));
          for (const candidate of candidates) {
            if (
              !(candidate instanceof HTMLElement) ||
              candidate.classList.contains("is-closing")
            ) {
              continue;
            }
            const clone = candidate.cloneNode(true) as HTMLElement;
            clone.classList.add("is-closing");
            clone.setAttribute("aria-hidden", "true");
            document.body.appendChild(clone);
            window.setTimeout(() => clone.remove(), 260);
          }
        }
      }
    });
    observer.observe(document.body, { childList: true, subtree: true });
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    const window = getCurrentWindow();
    const theme = themeMode === "system" ? null : themeMode;
    void window.setTheme(theme).catch(() => {
      // 某些 Windows 环境下更新标题栏主题可能失败，不影响页面主题。
    });
  }, [themeMode]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        document.querySelector<HTMLInputElement>("#key-search")?.focus();
      }
      if (event.key === "F5") {
        event.preventDefault();
        void refreshKeys();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [refreshKeys]);

  useEffect(() => {
    const onContextMenu = (event: MouseEvent) => {
      // 输入框保留系统右键菜单（复制/粘贴），其他区域禁用默认菜单。
      const target = event.target as HTMLElement | null;
      if (target?.closest("input, textarea, [contenteditable='true']")) {
        return;
      }
      event.preventDefault();
    };
    window.addEventListener("contextmenu", onContextMenu);
    return () => window.removeEventListener("contextmenu", onContextMenu);
  }, []);

  return (
    <div
      className="app-shell"
      style={{ gridTemplateColumns: `${sidebarWidth}px minmax(0, 1fr)` }}
    >
      <Sidebar
        onNew={() => setDialog({ mode: "create" })}
        onEdit={(profile) => setDialog({ mode: "edit", profile })}
      />
      <main className="workspace">
        <TopBar
          performanceOpen={performanceOpen}
          onTogglePerformance={() =>
            setPerformanceOpen((value) => !value)
          }
        />
        {performanceOpen ? (
          <PerformancePage />
        ) : (
          <div
            key={connection?.profileId ?? "disconnected"}
            className="workspace-body"
            style={{ gridTemplateColumns: `${keyWidth}px minmax(0, 1fr)` }}
          >
            <KeyBrowser />
            <KeyDetail />
            <div
              className="resize-handle key-resize"
              role="separator"
              aria-orientation="vertical"
              aria-label="拖动调整键浏览器宽度"
              title="拖动调整宽度，双击恢复默认"
              style={{ left: keyHandleLeft }}
              onPointerDown={(event) => startResize("key", event)}
              onDoubleClick={resetWidths}
            />
          </div>
        )}
      </main>
      <div
        className="resize-handle sidebar-resize"
        role="separator"
        aria-orientation="vertical"
        aria-label="拖动调整连接栏宽度"
        title="拖动调整宽度，双击恢复默认"
        style={{ left: sidebarHandleLeft }}
        onPointerDown={(event) => startResize("sidebar", event)}
        onDoubleClick={resetWidths}
      />

      {dialog && (
        <ConnectionDialog
          key={
            dialog.mode === "create"
              ? "create"
              : `${dialog.mode}-${dialog.profile.id}`
          }
          mode={dialog.mode}
          profile={dialog.mode === "create" ? undefined : dialog.profile}
          onClose={() => setDialog(null)}
        />
      )}
      <Toast />
    </div>
  );
}
