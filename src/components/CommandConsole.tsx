import { useEffect, useRef, useState } from "react";
import { CornerDownLeft, Loader2, Terminal, X } from "lucide-react";
import { api, errorMessage } from "../lib/api";
import { useAppStore } from "../stores/appStore";
import { Portal } from "./Portal";

type Entry = {
  id: number;
  kind: "command" | "output" | "error";
  text: string;
  meta?: string;
};

let entrySeq = 0;

function nextEntry(entry: Omit<Entry, "id">): Entry {
  entrySeq += 1;
  return { id: entrySeq, ...entry };
}

/**
 * 命令行面板：直接执行 Redis 命令。
 *
 * 只读模式下整条通道关闭；会阻塞连接或影响服务器稳定性的命令由后端拒绝。
 */
export function CommandConsole({ onClose }: { onClose: () => void }) {
  const activeProfileId = useAppStore((state) => state.activeProfileId);
  const readOnly = useAppStore((state) => state.readOnly);

  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [entries, setEntries] = useState<Entry[]>([
    nextEntry({
      kind: "output",
      text: "输入 Redis 命令后回车执行，例如 GET user:1:name、HGETALL user:1。",
    }),
  ]);
  const [history, setHistory] = useState<string[]>([]);
  const [historyIndex, setHistoryIndex] = useState(-1);
  const outputRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [busy, onClose]);

  useEffect(() => {
    const element = outputRef.current;
    if (!element) return;
    // 内容未溢出时保持顶部对齐，避免首次挂载时把首行滚出可视区域。
    element.scrollTop =
      element.scrollHeight > element.clientHeight
        ? element.scrollHeight
        : 0;
  }, [entries]);

  const submit = async () => {
    const line = input.trim();
    if (!line || !activeProfileId || busy || readOnly) return;

    setEntries((prev) => [...prev, nextEntry({ kind: "command", text: line })]);
    setHistory((prev) => [...prev, line]);
    setHistoryIndex(-1);
    setInput("");
    setBusy(true);

    try {
      const result = await api.runCommand(activeProfileId, line);
      setEntries((prev) => [
        ...prev,
        nextEntry({
          kind: "output",
          text: result.output,
          meta: `${result.elapsedMs} ms`,
        }),
      ]);
    } catch (error) {
      setEntries((prev) => [
        ...prev,
        nextEntry({ kind: "error", text: errorMessage(error) }),
      ]);
    } finally {
      setBusy(false);
    }
  };

  const recall = (direction: 1 | -1) => {
    if (history.length === 0) return;
    const next =
      historyIndex === -1
        ? history.length - 1
        : Math.min(
            history.length - 1,
            Math.max(0, historyIndex + direction),
          );
    setHistoryIndex(next);
    setInput(history[next]);
  };

  return (
    <Portal>
      <div className="modal-backdrop" onMouseDown={() => !busy && onClose()}>
        <div
          className="modal console-modal"
          onMouseDown={(event) => event.stopPropagation()}
        >
          <div className="modal-head">
            <div className="modal-title">
              <div className="modal-icon">
                <Terminal size={18} />
              </div>
              <div>
                <h3>命令行</h3>
                <p>直接执行 Redis 命令，支持用引号包裹带空格的参数</p>
              </div>
            </div>
            <button
              className="icon-btn"
              onClick={onClose}
              disabled={busy}
              title="关闭"
            >
              <X size={16} />
            </button>
          </div>

          <div
            className="console-output"
            ref={outputRef}
            aria-live="polite"
            aria-label="命令输出"
          >
            {entries.map((entry) => (
              <div
                key={entry.id}
                className={`console-entry console-entry-${entry.kind}`}
              >
                {entry.kind === "command" && (
                  <span className="console-prompt" aria-hidden="true">
                    ›
                  </span>
                )}
                <pre>{entry.text}</pre>
                {entry.meta && <span className="console-meta">{entry.meta}</span>}
              </div>
            ))}
          </div>

          <form
            className="console-form"
            onSubmit={(event) => {
              event.preventDefault();
              void submit();
            }}
          >
            <span className="console-prompt" aria-hidden="true">
              ›
            </span>
            <input
              className="text-input"
              autoFocus
              spellCheck={false}
              value={input}
              disabled={readOnly || busy}
              placeholder={
                readOnly ? "只读模式已开启" : "GET user:1:name"
              }
              onChange={(event) => setInput(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "ArrowUp") {
                  event.preventDefault();
                  recall(-1);
                } else if (event.key === "ArrowDown") {
                  event.preventDefault();
                  recall(1);
                }
              }}
            />
            <button
              className="btn primary"
              type="submit"
              disabled={!input.trim() || readOnly || busy}
            >
              {busy ? (
                <Loader2 size={14} className="spin" />
              ) : (
                <CornerDownLeft size={14} />
              )}
              执行
            </button>
          </form>

          <div className="modal-footer console-footer">
            <span>
              {readOnly
                ? "只读模式已开启，命令行不可用"
                : "SUBSCRIBE / MONITOR / 阻塞类命令已禁用"}
            </span>
            <span>↑ ↓ 切换历史 · Esc 关闭</span>
          </div>
        </div>
      </div>
    </Portal>
  );
}
