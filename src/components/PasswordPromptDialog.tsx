import { useEffect, useState } from "react";
import { AlertCircle, Check, Eye, EyeOff, KeyRound, Loader2, X } from "lucide-react";
import { errorMessage } from "../lib/api";
import type { ConnectionProfile, ProfileInput } from "../lib/types";
import { useAppStore } from "../stores/appStore";
import { Portal } from "./Portal";

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

/**
 * 连接需要密码但没有保存过密码时弹出。
 *
 * 之前这种情况只会弹一个 NOAUTH 报错，用户无法直接补密码，只能先编辑连接。
 */
export function PasswordPromptDialog({
  profile,
  onClose,
}: {
  profile: ConnectionProfile;
  onClose: () => void;
}) {
  const connectTo = useAppStore((state) => state.connectTo);
  const saveProfile = useAppStore((state) => state.saveProfile);
  const showNotice = useAppStore((state) => state.showNotice);

  const [password, setPassword] = useState("");
  const [reveal, setReveal] = useState(false);
  const [remember, setRemember] = useState(profile.rememberPassword);
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
    if (!password) {
      setError("请输入 Redis 密码");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      if (remember) {
        await saveProfile(
          { ...toInput(profile), rememberPassword: true },
          password,
        );
      } else if (profile.rememberPassword) {
        // 取消了「记住密码」：保存一次以删除已有密文。
        await saveProfile({ ...toInput(profile), rememberPassword: false });
      }
      await connectTo(profile.id, password);
      showNotice("success", "连接成功");
      onClose();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Portal>
      <div className="modal-backdrop" onMouseDown={() => !busy && onClose()}>
        <form
          className="modal modal-small"
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
                <h3>需要密码</h3>
                <p>
                  {profile.name} · {profile.host}:{profile.port}
                </p>
              </div>
            </div>
            <button
              type="button"
              className="icon-btn"
              onClick={onClose}
              title="关闭"
              disabled={busy}
            >
              <X size={16} />
            </button>
          </div>

          <div className="modal-body">
            <div className="field">
              <span className="field-label">Redis 密码</span>
              <div className="password-row">
                <div className="password-input-wrap">
                  <input
                    className="text-input"
                    type={reveal ? "text" : "password"}
                    autoFocus
                    autoComplete="off"
                    value={password}
                    onChange={(event) => {
                      setPassword(event.target.value);
                      setError(null);
                    }}
                    placeholder="输入后按回车连接"
                  />
                  <button
                    type="button"
                    className="password-eye"
                    title={reveal ? "隐藏密码" : "查看密码"}
                    aria-label={reveal ? "隐藏密码" : "查看密码"}
                    disabled={!password}
                    onClick={() => setReveal((value) => !value)}
                  >
                    {reveal ? <EyeOff size={14} /> : <Eye size={14} />}
                  </button>
                </div>
                <label className="remember-inline">
                  <input
                    className="remember-input"
                    type="checkbox"
                    checked={remember}
                    onChange={(event) => setRemember(event.target.checked)}
                  />
                  <span
                    className={`remember-box ${remember ? "checked" : ""}`}
                    aria-hidden="true"
                  >
                    {remember && <Check size={11} />}
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
              disabled={busy || !password}
            >
              {busy ? <Loader2 size={14} className="spin" /> : null}
              连接
            </button>
          </div>
        </form>
      </div>
    </Portal>
  );
}
