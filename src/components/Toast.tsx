import { AlertCircle, CheckCircle2, Info, X } from "lucide-react";
import { useAppStore } from "../stores/appStore";

export function Toast() {
  const notice = useAppStore((state) => state.notice);
  const clearNotice = useAppStore((state) => state.clearNotice);

  if (!notice) return null;

  const Icon =
    notice.kind === "success"
      ? CheckCircle2
      : notice.kind === "error"
        ? AlertCircle
        : Info;

  return (
    <div className={`toast toast-${notice.kind}`} role="status">
      <Icon size={17} />
      <span>{notice.message}</span>
      <button className="toast-close" onClick={clearNotice} title="关闭">
        <X size={14} />
      </button>
    </div>
  );
}
