import { useEffect, useState } from "react";
import { AlertCircle, KeyRound, Loader2, Plus, X } from "lucide-react";
import type { CreatableKeyType } from "../lib/types";
import { useAppStore } from "../stores/appStore";
import { Portal } from "./Portal";

const keyTypes: Array<{ value: CreatableKeyType; label: string }> = [
  { value: "string", label: "String" },
  { value: "hash", label: "Hash" },
  { value: "list", label: "List" },
  { value: "set", label: "Set" },
  { value: "zset", label: "ZSet" },
];

const valueLabel: Record<CreatableKeyType, string> = {
  string: "初始值",
  hash: "字段值",
  list: "第一个元素",
  set: "第一个成员",
  zset: "成员",
};

export function NewKeyDialog({ onClose }: { onClose: () => void }) {
  const createKey = useAppStore((state) => state.createKey);
  const readOnly = useAppStore((state) => state.readOnly);
  const [keyName, setKeyName] = useState("");
  const [keyType, setKeyType] = useState<CreatableKeyType>("string");
  const [value, setValue] = useState("");
  const [field, setField] = useState("");
  const [score, setScore] = useState("0");
  const [ttlMode, setTtlMode] = useState<"never" | "custom">("never");
  const [ttlValue, setTtlValue] = useState("3600");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [busy, onClose]);

  const submit = async () => {
    if (!keyName.trim()) {
      setError("键名不能为空");
      return;
    }
    if (keyType === "hash" && !field.trim()) {
      setError("Hash 字段不能为空");
      return;
    }
    const parsedScore = Number(score);
    if (keyType === "zset" && !Number.isFinite(parsedScore)) {
      setError("ZSet 分数必须是数字");
      return;
    }
    let ttlSeconds: number | null = null;
    if (ttlMode === "custom") {
      const seconds = Number(ttlValue);
      if (!Number.isFinite(seconds) || seconds <= 0) {
        setError("过期时间必须是大于 0 的秒数");
        return;
      }
      ttlSeconds = Math.floor(seconds);
    }

    setBusy(true);
    setError(null);
    const created = await createKey({
      key: keyName,
      keyType,
      value,
      field: field || undefined,
      score: keyType === "zset" ? parsedScore : undefined,
      ttlSeconds,
    });
    setBusy(false);
    if (created) onClose();
  };

  return (
    <Portal>
      <div className="modal-backdrop" onMouseDown={() => !busy && onClose()}>
      <form
        className="modal new-key-modal"
        onMouseDown={(event) => event.stopPropagation()}
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <div className="modal-head">
          <div className="modal-title">
            <div className="modal-icon">
              <KeyRound size={18} />
            </div>
            <div>
              <h3>新建键</h3>
              <p>创建后会立即出现在键列表中</p>
            </div>
          </div>
          <button
            type="button"
            className="icon-btn"
            onClick={onClose}
            disabled={busy}
            title="关闭"
          >
            <X size={16} />
          </button>
        </div>

        <div className="modal-body">
          <label className="field">
            <span className="field-label">键名</span>
            <input
              className="text-input"
              autoFocus
              value={keyName}
              onChange={(event) => {
                setKeyName(event.target.value);
                setError(null);
              }}
              placeholder="例如 user:1:profile"
              spellCheck={false}
            />
          </label>

          <div className="field">
            <span className="field-label">类型</span>
            <div className="key-type-options">
              {keyTypes.map((type) => (
                <button
                  key={type.value}
                  type="button"
                  className={`key-type-option ${
                    keyType === type.value ? "active" : ""
                  }`}
                  onClick={() => {
                    setKeyType(type.value);
                    setError(null);
                  }}
                >
                  {type.label}
                </button>
              ))}
            </div>
          </div>

          {keyType === "hash" && (
            <label className="field">
              <span className="field-label">初始字段</span>
              <input
                className="text-input"
                value={field}
                onChange={(event) => {
                  setField(event.target.value);
                  setError(null);
                }}
                placeholder="例如 name"
                spellCheck={false}
              />
            </label>
          )}

          <label className="field">
            <span className="field-label">
              {valueLabel[keyType]}
              {keyType === "string" && (
                <span className="field-optional">可留空</span>
              )}
            </span>
            {keyType === "string" ? (
              <textarea
                className="text-input new-key-textarea"
                value={value}
                onChange={(event) => setValue(event.target.value)}
                placeholder="输入初始字符串内容"
                spellCheck={false}
              />
            ) : (
              <input
                className="text-input"
                value={value}
                onChange={(event) => setValue(event.target.value)}
                placeholder={
                  keyType === "hash"
                    ? "输入字段值"
                    : keyType === "list"
                      ? "输入第一个元素"
                      : keyType === "set"
                        ? "输入第一个成员"
                        : "输入 ZSet 成员"
                }
                spellCheck={false}
              />
            )}
          </label>

          {keyType === "zset" && (
            <label className="field">
              <span className="field-label">分数</span>
              <input
                className="text-input"
                type="number"
                step="any"
                value={score}
                onChange={(event) => setScore(event.target.value)}
              />
            </label>
          )}

          <div className="field">
            <span className="field-label">过期时间</span>
            <div className="ttl-options">
              <button
                type="button"
                className={`ttl-option ${
                  ttlMode === "never" ? "active" : ""
                }`}
                onClick={() => {
                  setTtlMode("never");
                  setError(null);
                }}
              >
                永不过期
              </button>
              <button
                type="button"
                className={`ttl-option ${
                  ttlMode === "custom" ? "active" : ""
                }`}
                onClick={() => {
                  setTtlMode("custom");
                  setError(null);
                }}
              >
                自定义
              </button>
            </div>
          </div>

          {ttlMode === "custom" && (
            <div className="field">
              <label className="field-label">过期秒数</label>
              <div className="ttl-input-row">
                <input
                  className="text-input"
                  type="number"
                  min={1}
                  value={ttlValue}
                  onChange={(event) => {
                    setTtlValue(event.target.value);
                    setError(null);
                  }}
                />
                <span className="ttl-unit">秒</span>
              </div>
              <div className="ttl-presets">
                {[
                  ["1 小时", "3600"],
                  ["1 天", "86400"],
                  ["7 天", "604800"],
                  ["30 天", "2592000"],
                ].map(([label, seconds]) => (
                  <button
                    key={seconds}
                    type="button"
                    onClick={() => setTtlValue(seconds)}
                  >
                    {label}
                  </button>
                ))}
              </div>
            </div>
          )}

          {readOnly && (
            <div className="inline-warning">
              <AlertCircle size={15} />
              只读模式已开启，无法新建键。
            </div>
          )}

          {error && (
            <div className="form-error">
              <AlertCircle size={15} />
              <span>{error}</span>
            </div>
          )}
        </div>

        <div className="modal-footer">
          <button
            type="button"
            className="btn ghost"
            onClick={onClose}
            disabled={busy}
          >
            取消
          </button>
          <button
            type="submit"
            className="btn primary"
            disabled={busy || readOnly || !keyName.trim()}
          >
            {busy ? <Loader2 size={14} className="spin" /> : <Plus size={14} />}
            创建键
          </button>
        </div>
      </form>
      </div>
    </Portal>
  );
}
