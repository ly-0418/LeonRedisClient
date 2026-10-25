import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { AlertTriangle, Loader2, Trash2, X } from "lucide-react";
import { useAppStore } from "../stores/appStore";

interface ClearDatabaseDialogProps {
  db: number;
  keys: number;
  onClose: () => void;
}

export function ClearDatabaseDialog({
  db,
  keys,
  onClose,
}: ClearDatabaseDialogProps) {
  const clearDatabase = useAppStore((state) => state.clearDatabase);
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [busy, onClose]);

  const submit = async () => {
    if (!confirmed || busy) return;
    setBusy(true);
    const cleared = await clearDatabase();
    setBusy(false);
    if (cleared) onClose();
  };

  return createPortal(
    <div className="modal-backdrop" onMouseDown={() => !busy && onClose()}>
      <div
        className="modal modal-small clear-db-modal"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="modal-head">
          <h3>清空数据库</h3>
          <button
            className="icon-btn"
            onClick={onClose}
            title="关闭"
            disabled={busy}
          >
            <X size={16} />
          </button>
        </div>

        <div className="modal-body">
          <div className="danger-box">
            <AlertTriangle size={18} />
            <div>
              <p>确定要清空 db{db} 吗？</p>
              <span>
                将永久删除当前数据库中的全部 {keys} 个键，此操作不可撤销。
                其他数据库不会受到影响。
              </span>
            </div>
          </div>

          <label className="danger-check">
            <input
              type="checkbox"
              checked={confirmed}
              disabled={busy || keys === 0}
              onChange={(event) => setConfirmed(event.target.checked)}
            />
            <span>
              我确认清空 db{db} 的全部数据
            </span>
          </label>
        </div>

        <div className="modal-footer">
          <button className="btn ghost" onClick={onClose} disabled={busy}>
            取消
          </button>
          <button
            className="btn danger"
            onClick={() => void submit()}
            disabled={!confirmed || busy || keys === 0}
          >
            {busy ? (
              <Loader2 size={14} className="spin" />
            ) : (
              <Trash2 size={14} />
            )}
            清空数据库
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
