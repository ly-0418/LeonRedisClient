import { useState } from "react";
import {
  Database,
  Link,
  Link2Off,
  Pencil,
  Plus,
  Server,
  Trash2,
} from "lucide-react";
import { isAuthError, errorMessage } from "../lib/api";
import type { ConnectionProfile } from "../lib/types";
import { useAppStore } from "../stores/appStore";
import { MarqueeText } from "./MarqueeText";
import { DeleteConnectionDialog } from "./DeleteConnectionDialog";
import { PasswordPromptDialog } from "./PasswordPromptDialog";

interface SidebarProps {
  onNew: () => void;
  onEdit: (profile: ConnectionProfile) => void;
}

export function Sidebar({ onNew, onEdit }: SidebarProps) {
  const profiles = useAppStore((state) => state.profiles);
  const activeProfileId = useAppStore((state) => state.activeProfileId);
  const connection = useAppStore((state) => state.connection);
  const connecting = useAppStore((state) => state.connecting);
  const bootError = useAppStore((state) => state.bootError);
  const disconnect = useAppStore((state) => state.disconnect);
  const connectTo = useAppStore((state) => state.connectTo);
  const showNotice = useAppStore((state) => state.showNotice);
  const [deleteTarget, setDeleteTarget] = useState<ConnectionProfile | null>(
    null,
  );
  // 认证失败时弹出补密码的弹框，而不是只丢一条 NOAUTH 错误。
  const [authTarget, setAuthTarget] = useState<ConnectionProfile | null>(null);

  const connectProfile = (profile: ConnectionProfile) => {
    if (connecting) return;
    if (activeProfileId === profile.id && connection) return;
    void connectTo(profile.id).catch((error) => {
      const message = errorMessage(error);
      if (isAuthError(message)) {
        setAuthTarget(profile);
        return;
      }
      showNotice("error", message);
    });
  };

  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <div className="sidebar-brand">
          <span className="sidebar-brand-mark">
            <img src="/leonredis-icon.png" alt="" />
          </span>
          <span className="sidebar-brand-name">LeonRedis</span>
        </div>
      </div>

      <div className="profile-list">
        {profiles.length === 0 && !bootError && (
          <div className="sidebar-empty">
            <Server size={20} />
            <p>还没有保存的连接</p>
          </div>
        )}

        {profiles.map((profile) => {
          const active = activeProfileId === profile.id;
          const connected = active && connection !== null;
          return (
            <div
              key={profile.id}
              className={`profile-card ${active ? "active" : ""}`}
            >
              <button
                className={`profile-conn-btn ${connected ? "is-on" : ""}`}
                title={connected ? "断开连接" : "连接"}
                disabled={!connected && (connection !== null || connecting)}
                onClick={(event) => {
                  event.stopPropagation();
                  if (connected) {
                    void disconnect();
                  } else {
                    connectProfile(profile);
                  }
                }}
              >
                {connected ? (
                  <Link2Off size={17} />
                ) : (
                  <Link size={17} />
                )}
              </button>
              <div className="profile-main">
                <div className="profile-name">
                  <MarqueeText text={profile.name} />
                  {connecting && active && <span className="mini-spinner" />}
                </div>
                <div className="profile-meta">
                  <MarqueeText text={`${profile.host}:${profile.port}`} />
                </div>
              </div>
              <div className="profile-actions">
                <button
                  className="icon-btn profile-action-btn"
                  title={
                    connection ? "请先断开当前连接，再编辑连接" : "编辑连接"
                  }
                  disabled={connection !== null || connecting}
                  onClick={(event) => {
                    event.stopPropagation();
                    onEdit(profile);
                  }}
                >
                  <Pencil size={15} />
                </button>
                <button
                  className="icon-btn profile-action-btn danger"
                  title={
                    connection ? "请先断开当前连接，再删除连接" : "删除连接"
                  }
                  disabled={connection !== null || connecting}
                  onClick={(event) => {
                    event.stopPropagation();
                    setDeleteTarget(profile);
                  }}
                >
                  <Trash2 size={15} />
                </button>
              </div>
            </div>
          );
        })}
      </div>

      {bootError && (
        <div className="sidebar-warning">
          <Database size={15} />
          <span>{bootError}</span>
        </div>
      )}

      <button className="sidebar-new-btn" onClick={onNew} title="新建连接">
        <Plus size={15} />
        <span>新建连接</span>
      </button>

      <div className="sidebar-footer">
        <div className="footer-version">v1.0.0</div>
      </div>

      {deleteTarget && (
        <DeleteConnectionDialog
          profile={deleteTarget}
          onClose={() => setDeleteTarget(null)}
        />
      )}

      {authTarget && (
        <PasswordPromptDialog
          profile={authTarget}
          onClose={() => setAuthTarget(null)}
        />
      )}
    </aside>
  );
}
