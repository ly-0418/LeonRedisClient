import { create } from "zustand";
import { api } from "../lib/api";

export type ThemeMode = "dark" | "light" | "system";
export type FontPreset =
  | "system"
  | "yahei"
  | "segoe"
  | "dengxian"
  | "simsun"
  | "simhei"
  | "consolas"
  | "cascadia"
  | "mono";
export type FontSize = "small" | "normal" | "large" | "xlarge";
export type AccentColor = "emerald" | "blue" | "violet" | "amber" | "rose";

export interface AppSettings {
  themeMode: ThemeMode;
  fontPreset: FontPreset;
  fontSize: FontSize;
  accentColor: AccentColor;
  bigKeyThresholdBytes: number;
}

const LEGACY_STORAGE_KEY = "leonredis.settings.v1";
/// 同步缓存：settings.json 需要异步 IPC 读取，首帧先用它渲染，避免主题闪烁。
const CACHE_STORAGE_KEY = "leonredis.settings.cache";

const defaults: AppSettings = {
  themeMode: "system",
  fontPreset: "system",
  fontSize: "normal",
  accentColor: "emerald",
  bigKeyThresholdBytes: 1024,
};

export const fontPresetOptions: Array<{
  value: FontPreset;
  label: string;
  sample: string;
}> = [
  { value: "system", label: "系统默认", sample: "Segoe UI / 微软雅黑" },
  { value: "yahei", label: "微软雅黑", sample: "微软雅黑" },
  { value: "segoe", label: "Segoe UI", sample: "Segoe UI" },
  { value: "dengxian", label: "等线", sample: "DengXian" },
  { value: "simsun", label: "宋体", sample: "SimSun" },
  { value: "simhei", label: "黑体", sample: "SimHei" },
  { value: "consolas", label: "Consolas", sample: "Consolas" },
  { value: "cascadia", label: "Cascadia Code", sample: "Cascadia Code" },
  { value: "mono", label: "等宽字体", sample: "Cascadia Mono" },
];

export const fontSizeOptions: Array<{ value: FontSize; label: string }> = [
  { value: "small", label: "小" },
  { value: "normal", label: "标准" },
  { value: "large", label: "大" },
  { value: "xlarge", label: "特大" },
];

export const accentOptions: Array<{ value: AccentColor; label: string }> = [
  { value: "emerald", label: "翡翠" },
  { value: "blue", label: "海蓝" },
  { value: "violet", label: "紫罗兰" },
  { value: "amber", label: "琥珀" },
  { value: "rose", label: "玫红" },
];

export function resolveTheme(mode: ThemeMode): "dark" | "light" {
  if (mode === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light";
  }
  return mode;
}

function isThemeMode(value: unknown): value is ThemeMode {
  return value === "dark" || value === "light" || value === "system";
}

function isFontPreset(value: unknown): value is FontPreset {
  return (
    value === "system" ||
    value === "yahei" ||
    value === "segoe" ||
    value === "dengxian" ||
    value === "simsun" ||
    value === "simhei" ||
    value === "consolas" ||
    value === "cascadia" ||
    value === "mono"
  );
}

function isFontSize(value: unknown): value is FontSize {
  return (
    value === "small" ||
    value === "normal" ||
    value === "large" ||
    value === "xlarge"
  );
}

function isAccentColor(value: unknown): value is AccentColor {
  return (
    value === "emerald" ||
    value === "blue" ||
    value === "violet" ||
    value === "amber" ||
    value === "rose"
  );
}

function normalizeSettings(value: unknown): AppSettings {
  const parsed =
    value && typeof value === "object"
      ? (value as Partial<AppSettings>)
      : {};
  return {
    themeMode: isThemeMode(parsed.themeMode)
      ? parsed.themeMode
      : defaults.themeMode,
    fontPreset: isFontPreset(parsed.fontPreset)
      ? parsed.fontPreset
      : defaults.fontPreset,
    fontSize: isFontSize(parsed.fontSize)
      ? parsed.fontSize
      : defaults.fontSize,
    accentColor: isAccentColor(parsed.accentColor)
      ? parsed.accentColor
      : defaults.accentColor,
    bigKeyThresholdBytes:
      typeof parsed.bigKeyThresholdBytes === "number" &&
      Number.isFinite(parsed.bigKeyThresholdBytes) &&
      parsed.bigKeyThresholdBytes > 0
        ? Math.floor(parsed.bigKeyThresholdBytes)
        : defaults.bigKeyThresholdBytes,
  };
}

function readLegacySettings(): AppSettings | null {
  try {
    const raw = localStorage.getItem(LEGACY_STORAGE_KEY);
    if (!raw) return null;
    return normalizeSettings(JSON.parse(raw));
  } catch {
    return null;
  }
}

function applySettings(settings: AppSettings) {
  const root = document.documentElement;
  const resolved = resolveTheme(settings.themeMode);
  root.dataset.theme = resolved;
  root.dataset.themeMode = settings.themeMode;
  root.dataset.fontPreset = settings.fontPreset;
  root.dataset.fontSize = settings.fontSize;
  root.dataset.accent = settings.accentColor;
  root.style.colorScheme = resolved;
}

function readCachedSettings(): AppSettings | null {
  try {
    const raw = localStorage.getItem(CACHE_STORAGE_KEY);
    if (!raw) return null;
    return normalizeSettings(JSON.parse(raw));
  } catch {
    return null;
  }
}

function cacheSettings(settings: AppSettings) {
  try {
    localStorage.setItem(CACHE_STORAGE_KEY, JSON.stringify(settings));
  } catch {
    // 缓存失败只影响下次启动的首帧，不影响功能。
  }
}

function persistSettings(settings: AppSettings) {
  cacheSettings(settings);
  void api.saveSettings(settings).catch(() => {
    // 配置文件不可写时只保持当前会话设置。
  });
}

interface SettingsStore extends AppSettings {
  setThemeMode: (mode: ThemeMode) => void;
  setFontPreset: (preset: FontPreset) => void;
  setFontSize: (size: FontSize) => void;
  setAccentColor: (accent: AccentColor) => void;
  setBigKeyThresholdBytes: (thresholdBytes: number) => void;
  resetSettings: () => void;
}

export const useSettingsStore = create<SettingsStore>((set, get) => {
  const update = (patch: Partial<AppSettings>) => {
    const next: AppSettings = {
      themeMode: get().themeMode,
      fontPreset: get().fontPreset,
      fontSize: get().fontSize,
      accentColor: get().accentColor,
      bigKeyThresholdBytes: get().bigKeyThresholdBytes,
      ...patch,
    };
    applySettings(next);
    persistSettings(next);
    set(patch);
  };

  return {
    ...defaults,
    setThemeMode: (themeMode) => update({ themeMode }),
    setFontPreset: (fontPreset) => update({ fontPreset }),
    setFontSize: (fontSize) => update({ fontSize }),
    setAccentColor: (accentColor) => update({ accentColor }),
    setBigKeyThresholdBytes: (bigKeyThresholdBytes) =>
      update({ bigKeyThresholdBytes }),
    resetSettings: () => {
      applySettings(defaults);
      persistSettings(defaults);
      set(defaults);
    },
  };
});

let initialized = false;

export async function initSettings() {
  if (initialized) return;
  initialized = true;

  // 首帧用同步缓存（没有则用默认值）：settings.json 要走异步 IPC，
  // 等它返回再应用主题会让「深色系统 + 浅色设置」闪一下深色。
  const cached = readCachedSettings();
  const first = cached ?? defaults;
  applySettings(first);
  useSettingsStore.setState(first);

  try {
    const stored = await api.loadSettings();
    if (stored) {
      const initial = normalizeSettings(stored);
      applySettings(initial);
      useSettingsStore.setState(initial);
      cacheSettings(initial);
    } else {
      const legacy = readLegacySettings();
      const initial = legacy ?? first;
      applySettings(initial);
      useSettingsStore.setState(initial);
      await api.saveSettings(initial);
      cacheSettings(initial);
    }
    try {
      localStorage.removeItem(LEGACY_STORAGE_KEY);
    } catch {
      // 旧缓存清理失败不影响设置文件。
    }
  } catch {
    // 读取安装目录设置失败时使用缓存/默认值，保证界面可启动。
  }

  const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
  mediaQuery.addEventListener("change", () => {
    if (useSettingsStore.getState().themeMode === "system") {
      applySettings({
        themeMode: "system",
        fontPreset: useSettingsStore.getState().fontPreset,
        fontSize: useSettingsStore.getState().fontSize,
        accentColor: useSettingsStore.getState().accentColor,
        bigKeyThresholdBytes:
          useSettingsStore.getState().bigKeyThresholdBytes,
      });
    }
  });
}
