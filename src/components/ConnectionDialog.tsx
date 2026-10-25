import { useEffect, useRef, useState } from "react";
import {
  AlertCircle,
  Check,
  CheckCircle2,
  ChevronDown,
  Database,
  Eye,
  EyeOff,
  Loader2,
  PlugZap,
  X,
} from "lucide-react";
import { api, errorMessage } from "../lib/api";
import type {
  ConnectionProfile,
  ConnectionSnapshot,
  ProfileInput,
} from "../lib/types";
import { useAppStore } from "../stores/appStore";

type Mode = "create" | "edit";

interface ConnectionDialogProps {
  mode: Mode;
  profile?: ConnectionProfile;
  onClose: () => void;
}

function toInput(profile: ConnectionProfile): ProfileInput {
  return {
    id: profile.id,
    name: profile.name,
    host: profile.host,
    port: profile.port,
    db: profile.db,
    username: profile.username ?? "",
    rememberPassword: profile.rememberPassword,
  };
}

const emptyForm: ProfileInput = {
  name: "本地 Redis",
  host: "127.0.0.1",
  port: 6379,
  db: 0,
  username: "",
  rememberPassword: false,
};

function parseHostAddress(
  input: string,
): { host: string; port?: number } | null {
  const value = input.trim();
  if (!value) return null;

  if (/^redis:\/\//i.test(value)) {
    try {
      const url = new URL(value);
      const host = url.hostname.replace(/^\[|\]$/g, "");
      const port = url.port ? Number(url.port) : undefined;
      return host
        ? { host, port: port && port > 0 && port <= 65535 ? port : undefined }
        : null;
    } catch {
      return null;
    }
  }

  const bracketed = value.match(/^\[([^\]]+)\](?::(\d+))?$/);
  if (bracketed) {
    const port = bracketed[2] ? Number(bracketed[2]) : undefined;
    return {
      host: bracketed[1],
      port: port && port > 0 && port <= 65535 ? port : undefined,
    };
  }

  if (value.split(":").length === 2) {
    const [host, rawPort] = value.split(":");
    if (host && /^\d+$/.test(rawPort)) {
      const port = Number(rawPort);
      if (port > 0 && port <= 65535) return { host, port };
    }
  }

  return null;
}

export function ConnectionDialog({
  mode,
  profile,
  onClose,
}: ConnectionDialogProps) {
  const saveProfile = useAppStore((state) => state.saveProfile);
  const connectTo = useAppStore((state) => state.connectTo);
  const showNotice = useAppStore((state) => state.showNotice);

  const [form, setForm] = useState<ProfileInput>(() =>
    profile ? toInput(profile) : { ...emptyForm },
  );
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [testResult, setTestResult] = useState<ConnectionSnapshot | null>(null);
  const [invalidFields, setInvalidFields] = useState<string[]>([]);
  const [saveMenuOpen, setSaveMenuOpen] = useState(false);
  const saveMenuRef = useRef<HTMLDivElement>(null);
  const passwordTouched = useRef(false);

  const title = mode === "create" ? "新建连接" : "编辑连接";

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || busy) return;
      if (saveMenuOpen) {
        setSaveMenuOpen(false);
        return;
      }
      onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [busy, onClose, saveMenuOpen]);

  useEffect(() => {
    if (!saveMenuOpen) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!saveMenuRef.current?.contains(event.target as Node)) {
        setSaveMenuOpen(false);
      }
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [saveMenuOpen]);

  useEffect(() => {
    if (mode !== "edit" || !profile?.id || !profile.rememberPassword) return;
    let cancelled = false;
    void api
      .getSavedPassword(profile.id)
      .then((saved) => {
        if (!cancelled && saved !== null && !passwordTouched.current) {
          setPassword(saved);
        }
      })
      .catch((err) => {
        if (!cancelled) setError(errorMessage(err));
      });
    return () => {
      cancelled = true;
    };
  }, [mode, profile?.id, profile?.rememberPassword]);

  const update = (patch: Partial<ProfileInput>) => {
    setForm((current) => ({ ...current, ...patch }));
    setInvalidFields([]);
    setTestResult(null);
    setError(null);
  };

  const validate = (): string | null => {
    const invalid: string[] = [];
    let message: string | null = null;

    if (!form.name.trim()) {
      invalid.push("name");
      message = message ?? "连接名称不能为空";
    }
    if (!form.host.trim()) {
      invalid.push("host");
      message = message ?? "主机地址不能为空";
    }
    if (!Number.isInteger(form.port) || form.port < 1 || form.port > 65535) {
      invalid.push("port");
      message = message ?? "端口必须在 1-65535 之间";
    }

    setInvalidFields(invalid);
    return message;
  };

  const handleTest = async () => {
    const message = validate();
    if (message) {
      setError(message);
      return;
    }
    setBusy(true);
    setError(null);
    setTestResult(null);
    try {
      const result = await api.testConnection(form, password);
      setTestResult(result);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  const handleConnect = async () => {
    setSaveMenuOpen(false);
    const message = validate();
    if (message) {
      setError(message);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const saved = await saveProfile(form, password);
      await connectTo(saved.id, password || undefined);
      showNotice("success", "连接成功");
      onClose();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  const handleSave = async () => {
    setSaveMenuOpen(false);
    const message = validate();
    if (message) {
      setError(message);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await saveProfile(form, password);
      showNotice("success", "连接配置已保存");
      onClose();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="modal-backdrop" onMouseDown={() => !busy && onClose()}>
      <form
        className="modal connection-modal"
        onMouseDown={(event) => event.stopPropagation()}
        onSubmit={(event) => {
          event.preventDefault();
          void handleSave();
        }}
      >
        <div className="modal-head">
          <div className="modal-title">
            <div className="modal-icon">
              <Database size={18} />
            </div>
            <div>
              <h3>{title}</h3>
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
          <div className="form-grid">
            <label className="field span-2">
              <span className="field-label">
                连接名称 <span className="required-mark">*</span>
              </span>
              <input
                className={`text-input ${
                  invalidFields.includes("name") ? "invalid" : ""
                }`}
                autoFocus
                aria-required="true"
                aria-invalid={invalidFields.includes("name")}
                value={form.name}
                onChange={(event) => update({ name: event.target.value })}
                placeholder="例如：本地开发 / 公网缓存"
              />
            </label>
            <label className="field">
              <span className="field-label">
                主机地址 <span className="required-mark">*</span>
              </span>
              <input
                className={`text-input ${
                  invalidFields.includes("host") ? "invalid" : ""
                }`}
                aria-required="true"
                aria-invalid={invalidFields.includes("host")}
                value={form.host}
                onChange={(event) => update({ host: event.target.value })}
                onBlur={() => {
                  const parsed = parseHostAddress(form.host);
                  if (parsed) {
                    update({
                      host: parsed.host,
                      ...(parsed.port ? { port: parsed.port } : {}),
                    });
                  }
                }}
                placeholder="redis.example.com"
                spellCheck={false}
                autoCapitalize="off"
                autoCorrect="off"
              />
              <span className="field-help">
                支持公网域名、IPv4 / IPv6 和外网 Redis 端口
              </span>
            </label>
            <label className="field">
              <span className="field-label">
                端口 <span className="required-mark">*</span>
              </span>
              <input
                className={`text-input ${
                  invalidFields.includes("port") ? "invalid" : ""
                }`}
                type="number"
                min={1}
                max={65535}
                aria-required="true"
                aria-invalid={invalidFields.includes("port")}
                value={form.port}
                onChange={(event) =>
                  update({ port: Number(event.target.value) })
                }
              />
            </label>
            <label className="field span-2">
              <span className="field-label">
                ACL 用户名 <span className="field-optional">可选</span>
              </span>
              <input
                className="text-input"
                value={form.username ?? ""}
                onChange={(event) =>
                  update({ username: event.target.value })
                }
                placeholder="default"
                spellCheck={false}
              />
            </label>
          </div>

          <div className="field">
            <span className="field-label">密码</span>
            <div className="password-row">
              <div className="password-input-wrap">
                <input
                  className="text-input"
                  type={showPassword ? "text" : "password"}
                  value={password}
                  autoComplete="off"
                  onChange={(event) => {
                    passwordTouched.current = true;
                    setPassword(event.target.value);
                    setError(null);
                  }}
                  placeholder="Redis 密码 / ACL 密码"
                />
                <button
                  type="button"
                  className="password-eye"
                  title={showPassword ? "隐藏密码" : "查看密码"}
                  aria-label={showPassword ? "隐藏密码" : "查看密码"}
                  disabled={!password}
                  onClick={() => setShowPassword((value) => !value)}
                >
                  {showPassword ? <EyeOff size={14} /> : <Eye size={14} />}
                </button>
              </div>
              <label className="remember-inline">
                <input
                  className="remember-input"
                  type="checkbox"
                  checked={form.rememberPassword}
                  onChange={(event) =>
                    update({ rememberPassword: event.target.checked })
                  }
                />
                <span
                  className={`remember-box ${
                    form.rememberPassword ? "checked" : ""
                  }`}
                  aria-hidden="true"
                >
                  {form.rememberPassword && <Check size={11} />}
                </span>
                <span>记住密码</span>
              </label>
            </div>
          </div>

          {error && (
            <div className="form-error">
              <AlertCircle size={15} />
              <span>{error}</span>
            </div>
          )}

          {testResult && (
            <div className="test-result">
              <CheckCircle2 size={16} />
              <div>
                <strong>连接成功</strong>
                <span>
                  Redis {testResult.version} · {testResult.latencyMs} ms ·{" "}
                  {testResult.dbSize} keys
                </span>
              </div>
            </div>
          )}
        </div>

        <div className="modal-footer dialog-footer">
          <div className="dialog-footer-left">
            <button
              type="button"
              className="btn ghost"
              onClick={() => void handleTest()}
              disabled={busy}
            >
              {busy ? <Loader2 size={14} className="spin" /> : null}
              测试连接
            </button>
          </div>
          <div className="dialog-footer-right">
            <div className="save-group" ref={saveMenuRef}>
              <button
                type="submit"
                className="btn primary save-main"
                disabled={busy}
              >
                <span className="save-label">
                  <span>保</span>
                  <span>存</span>
                </span>
              </button>
              <button
                type="button"
                className="btn primary save-toggle"
                aria-haspopup="menu"
                aria-expanded={saveMenuOpen}
                title="更多保存选项"
                disabled={busy}
                onClick={() => setSaveMenuOpen((value) => !value)}
              >
                <ChevronDown size={14} />
              </button>
              {saveMenuOpen && (
                <div className="save-menu" role="menu">
                  <button
                    type="button"
                    role="menuitem"
                    className="save-menu-item"
                    onClick={() => void handleConnect()}
                  >
                    <PlugZap size={14} />
                    保存并连接
                  </button>
                </div>
              )}
            </div>
          </div>
        </div>
      </form>
    </div>
  );
}
