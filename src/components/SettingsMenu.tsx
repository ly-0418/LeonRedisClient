import { useCallback, useEffect, useRef, useState } from "react";
import {
  Check,
  ChevronDown,
  Monitor,
  Moon,
  Palette,
  RotateCcw,
  Ruler,
  Settings2,
  Sun,
  Type,
} from "lucide-react";
import {
  accentOptions,
  fontPresetOptions,
  fontSizeOptions,
  useSettingsStore,
  type ThemeMode,
} from "../stores/settingsStore";

const themeOptions: Array<{
  value: ThemeMode;
  label: string;
  icon: typeof Moon;
}> = [
  { value: "dark", label: "深色", icon: Moon },
  { value: "light", label: "浅色", icon: Sun },
  { value: "system", label: "跟随系统", icon: Monitor },
];

type ThresholdUnit = "B" | "KB" | "MB" | "GB";
const THRESHOLD_UNITS: Record<ThresholdUnit, number> = {
  B: 1,
  KB: 1024,
  MB: 1024 * 1024,
  GB: 1024 * 1024 * 1024,
};

function splitThreshold(bytes: number): {
  value: string;
  unit: ThresholdUnit;
} {
  if (bytes < THRESHOLD_UNITS.KB) {
    return { value: String(Math.max(bytes, 1)), unit: "B" };
  }
  if (bytes >= THRESHOLD_UNITS.GB && bytes % THRESHOLD_UNITS.GB === 0) {
    return { value: String(bytes / THRESHOLD_UNITS.GB), unit: "GB" };
  }
  if (bytes >= THRESHOLD_UNITS.MB && bytes % THRESHOLD_UNITS.MB === 0) {
    return { value: String(bytes / THRESHOLD_UNITS.MB), unit: "MB" };
  }
  return {
    value: String(Math.max(bytes / THRESHOLD_UNITS.KB, 0.1)),
    unit: "KB",
  };
}

export function SettingsMenu() {
  const themeMode = useSettingsStore((state) => state.themeMode);
  const fontPreset = useSettingsStore((state) => state.fontPreset);
  const fontSize = useSettingsStore((state) => state.fontSize);
  const accentColor = useSettingsStore((state) => state.accentColor);
  const bigKeyThresholdBytes = useSettingsStore(
    (state) => state.bigKeyThresholdBytes,
  );
  const setThemeMode = useSettingsStore((state) => state.setThemeMode);
  const setFontPreset = useSettingsStore((state) => state.setFontPreset);
  const setFontSize = useSettingsStore((state) => state.setFontSize);
  const setAccentColor = useSettingsStore((state) => state.setAccentColor);
  const setBigKeyThresholdBytes = useSettingsStore(
    (state) => state.setBigKeyThresholdBytes,
  );
  const resetSettings = useSettingsStore((state) => state.resetSettings);

  const [visible, setVisible] = useState(false);
  const [closing, setClosing] = useState(false);
  const closeTimer = useRef<number | null>(null);
  const thresholdInitial = splitThreshold(bigKeyThresholdBytes);
  const [thresholdValue, setThresholdValue] = useState(thresholdInitial.value);
  const [thresholdUnit, setThresholdUnit] = useState<ThresholdUnit>(
    thresholdInitial.unit,
  );
  const thresholdTouched = useRef(false);
  const fontIndex = Math.max(
    0,
    fontSizeOptions.findIndex((option) => option.value === fontSize),
  );

  useEffect(() => {
    if (thresholdTouched.current) return;
    const next = splitThreshold(bigKeyThresholdBytes);
    setThresholdValue(next.value);
    setThresholdUnit(next.unit);
  }, [bigKeyThresholdBytes]);

  const commitThreshold = (value: string, unit: ThresholdUnit) => {
    const parsed = Number(value);
    if (!Number.isFinite(parsed) || parsed <= 0) return;
    setBigKeyThresholdBytes(
      Math.max(1, Math.round(parsed * THRESHOLD_UNITS[unit])),
    );
  };

  const handleReset = () => {
    thresholdTouched.current = false;
    resetSettings();
  };

  const openDrawer = useCallback(() => {
    if (closeTimer.current !== null) {
      window.clearTimeout(closeTimer.current);
      closeTimer.current = null;
    }
    setClosing(false);
    setVisible(true);
  }, []);

  const closeDrawer = useCallback(() => {
    if (!visible || closing) return;
    setClosing(true);
    closeTimer.current = window.setTimeout(() => {
      setVisible(false);
      setClosing(false);
      closeTimer.current = null;
    }, 220);
  }, [visible, closing]);

  useEffect(
    () => () => {
      if (closeTimer.current !== null) window.clearTimeout(closeTimer.current);
    },
    [],
  );

  useEffect(() => {
    if (!visible) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeDrawer();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [visible, closeDrawer]);

  return (
    <div className="settings-wrap">
      <button
        className={`icon-btn ${visible ? "active" : ""}`}
        title="设置"
        onClick={() => (visible ? closeDrawer() : openDrawer())}
        aria-expanded={visible}
      >
        <Settings2 size={16} />
      </button>

      {visible && (
        <div
          className={`settings-drawer-layer ${
            closing ? "is-closing" : ""
          }`}
        >
          <div
            className="settings-drawer-backdrop"
            onClick={closeDrawer}
          />
          <aside className="settings-drawer" role="dialog" aria-label="设置">
            <div className="settings-drawer-head">
              <div>
                <div className="settings-drawer-title">设置</div>
                <div className="settings-drawer-sub">设置会保存在本机</div>
              </div>
            </div>

            <div className="settings-drawer-body">
              <section className="drawer-section">
                <div className="drawer-section-title">
                  <Moon size={14} /> 主题
                </div>
                <div className="theme-options">
                  {themeOptions.map((option) => {
                    const Icon = option.icon;
                    return (
                      <button
                        key={option.value}
                        className={`theme-option ${
                          themeMode === option.value ? "active" : ""
                        }`}
                        onClick={() => setThemeMode(option.value)}
                        aria-pressed={themeMode === option.value}
                      >
                        <Icon size={15} />
                        <span>{option.label}</span>
                        {themeMode === option.value && <Check size={12} />}
                      </button>
                    );
                  })}
                </div>
              </section>

              <section className="drawer-section">
                <div className="drawer-section-title">
                  <Palette size={14} /> 主题色
                </div>
                <div className="accent-options">
                  {accentOptions.map((option) => (
                    <button
                      key={option.value}
                      className={`accent-option ${
                        accentColor === option.value ? "active" : ""
                      }`}
                      onClick={() => setAccentColor(option.value)}
                      aria-pressed={accentColor === option.value}
                    >
                      <span
                        className={`accent-swatch accent-${option.value}`}
                      >
                        {accentColor === option.value && <Check size={12} />}
                      </span>
                      <span>{option.label}</span>
                    </button>
                  ))}
                </div>
              </section>

              <section className="drawer-section">
                <div className="drawer-section-title">
                  <Type size={14} /> 界面字体
                </div>
                <div className="settings-select-wrap">
                  <select
                    className="settings-select"
                    value={fontPreset}
                    onChange={(event) =>
                      setFontPreset(
                        event.target
                          .value as (typeof fontPresetOptions)[number]["value"],
                      )
                    }
                  >
                    {fontPresetOptions.map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label} · {option.sample}
                      </option>
                    ))}
                  </select>
                  <ChevronDown size={13} />
                </div>
              </section>

              <section className="drawer-section">
                <div className="drawer-section-title">
                  字体大小
                </div>
                <input
                  className="font-size-range"
                  type="range"
                  min={0}
                  max={fontSizeOptions.length - 1}
                  step={1}
                  value={fontIndex}
                  onChange={(event) =>
                    setFontSize(
                      fontSizeOptions[Number(event.target.value)].value,
                    )
                  }
                />
                <div className="font-size-ticks">
                  {fontSizeOptions.map((option) => (
                    <span key={option.value}>{option.label}</span>
                  ))}
                </div>
                <div className="font-preview">
                  <span className="font-preview-mono">user:1:name</span>
                  <span>Redis 键值浏览 Aa</span>
                </div>
              </section>

              <section className="drawer-section drawer-section-last">
                <div className="drawer-section-title">
                  <Ruler size={14} /> 大 Key 阈值
                  <span className="settings-value">
                    ≥ {thresholdValue || "-"} {thresholdUnit}
                  </span>
                </div>
                <div className="threshold-controls">
                  <input
                    className="text-input threshold-value"
                    type="number"
                    min={1}
                    step="any"
                    value={thresholdValue}
                    onChange={(event) => {
                      thresholdTouched.current = true;
                      setThresholdValue(event.target.value);
                      commitThreshold(event.target.value, thresholdUnit);
                    }}
                  />
                  <select
                    className="settings-select threshold-unit-select"
                    value={thresholdUnit}
                    onChange={(event) => {
                      const unit = event.target.value as ThresholdUnit;
                      thresholdTouched.current = true;
                      setThresholdUnit(unit);
                      commitThreshold(thresholdValue, unit);
                    }}
                  >
                    <option value="B">B</option>
                    <option value="KB">KB</option>
                    <option value="MB">MB</option>
                    <option value="GB">GB</option>
                  </select>
                </div>
                <p className="field-help">
                  仅显示内存占用大于等于该阈值的键，默认 1 KB。
                </p>
              </section>
            </div>

            <div className="settings-drawer-footer">
              <button className="btn ghost" onClick={handleReset}>
                <RotateCcw size={14} /> 恢复默认
              </button>
              <span className="settings-drawer-hint">Esc 或点击空白处关闭</span>
            </div>
          </aside>
        </div>
      )}
    </div>
  );
}
