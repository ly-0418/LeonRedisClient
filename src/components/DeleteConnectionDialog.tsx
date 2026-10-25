import { useState } from "react";
import { AlertTriangle, Loader2, Trash2, X } from "lucide-react";
import { errorMessage } from "../lib/api";
import type { ConnectionProfile } from "../lib/types";
import { useAppStore } from "../stores/appStore";
import { Portal } from "./Portal";

interface DeleteConnectionDialogProps {
  profile: ConnectionProfile;
  onClose: () => void;
}

export function DeleteConnectionDialog({
  profile,
  onClose,
}: DeleteConnectionDialogProps) {
  const deleteProfile = useAppStore((state) => state.deleteProfile);
  const showNotice = useAppStore((state) => state.showNotice);
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await deleteProfile(profile.id);
      showNotice("success", "连接已删除");
      onClose();
    } catch (error) {
      showNotice("error", errorMessage(error));
      setBusy(false);
    }
  };

  return (
    <Portal>
      <div className="modal-backdrop" onMouseDown={() => !busy && onClose()}>
        <div
          className="modal modal-small delete-conn-modal"
          onMouseDown={(event) => event.stopPropagation()}
        >
          <div className="modal-head">
            <h3>删除连接</h3>
            <button
              className="icon-btn"
              onClick={onClose}
              disabled={busy}
              title="关闭"
            >
              <X size={16} />
            </button>
          </div>
          <div className="modal-body">
            <div className="danger-box">
              <AlertTriangle size={18} />
              <div>
                <p>确定删除连接“{profile.name}”？</p>
                <span>
                  只删除本机连接配置，不会影响 Redis 数据。当前连接会先断开，删除后无法恢复。
                </span>
              </div>
            </div>
          </div>
          <div className="modal-footer">
            <button className="btn ghost" onClick={onClose} disabled={busy}>
              取消
            </button>
            <button
              className="btn danger"
              onClick={() => void submit()}
              disabled={busy}
            >
              {busy ? (
                <Loader2 size={14} className="spin" />
              ) : (
                <Trash2 size={14} />
              )}
              删除连接
            </button>
          </div>
        </div>
      </div>
    </Portal>
  );
}
