import { Gauge, ShieldCheck, ShieldOff } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauri } from "../lib/api";
import { SettingsMenu } from "./SettingsMenu";
import { WindowControls } from "./WindowControls";
import { useAppStore } from "../stores/appStore";

function toggleMaximize() {
  if (!isTauri()) return;
  void getCurrentWindow().toggleMaximize().catch(() => {
    // 最大化失败时保持当前窗口状态。
  });
}

export function TopBar({
  performanceOpen,
  onTogglePerformance,
}: {
  performanceOpen: boolean;
  onTogglePerformance: () => void;
}) {
  const connection = useAppStore((state) => state.connection);
  const connecting = useAppStore((state) => state.connecting);
  const readOnly = useAppStore((state) => state.readOnly);
  const toggleReadOnly = useAppStore((state) => state.toggleReadOnly);

  return (
    <header className="topbar">
      <div
        className="window-title"
        data-tauri-drag-region
        onDoubleClick={toggleMaximize}
      >
        {connection ? (
          <>
            <span className="connection-name">{connection.name}</span>
            <span className="title-pills">
              {connecting ? (
                <span className="inline-loading">
                  <span className="mini-spinner" /> 正在连接
                </span>
              ) : (
                <>
                  <span className={`pill ${readOnly ? "pill-warn" : "pill-ok"}`}>
                    {readOnly ? "只读模式" : "可写模式"}
                  </span>
                </>
              )}
            </span>
          </>
        ) : (
          <>
            <span className="connection-name">未连接</span>
            <span className="title-hint">从左侧选择一个连接</span>
          </>
        )}
      </div>

      <div
        className="topbar-drag"
        data-tauri-drag-region
        onDoubleClick={toggleMaximize}
      />

      <div className="topbar-actions">
        <button
          className={`icon-btn ${readOnly ? "active-warn" : ""}`}
          title={readOnly ? "关闭只读模式" : "开启只读模式，拦截写操作"}
          onClick={() => void toggleReadOnly()}
          disabled={!connection}
        >
          {readOnly ? <ShieldCheck size={16} /> : <ShieldOff size={16} />}
        </button>
        <button
          className={`icon-btn ${performanceOpen ? "active" : ""}`}
          title={performanceOpen ? "关闭性能分析" : "性能分析"}
          onClick={onTogglePerformance}
          disabled={!connection}
        >
          <Gauge size={16} />
        </button>
        <SettingsMenu />
        <WindowControls />
      </div>
    </header>
  );
}
