import { invoke } from "@tauri-apps/api/core";
import type {
  CommandResult,
  ConnectionProfile,
  ConnectionSnapshot,
  CreateKeyInput,
  DatabaseSelection,
  DatabasesInfo,
  ExportResult,
  ImportResult,
  KeyBytes,
  KeyDetailBundle,
  KeyValue,
  PerformanceReport,
  PerformanceSummary,
  ProfileInput,
  ScanPage,
} from "./types";

export const isTauri = () =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function assertTauri() {
  if (!isTauri()) {
    throw new Error("请使用 `pnpm tauri dev` 启动桌面应用，浏览器预览不包含 Redis 后端。");
  }
}

export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return String(error);
}

export function isAuthError(message: string): boolean {
  const lower = message.toLowerCase();
  return (
    lower.includes("noauth") ||
    lower.includes("wrongpass") ||
    lower.includes("invalid password") ||
    lower.includes("authentication") ||
    lower.includes("认证失败")
  );
}

/// 与后端 `TARGET_EXISTS_MESSAGE` 保持一致：重命名时目标键已存在。
export const TARGET_EXISTS_MESSAGE = "目标键名已存在";

export function isTargetExistsError(message: string): boolean {
  return message.includes(TARGET_EXISTS_MESSAGE);
}

export const api = {
  listProfiles(): Promise<ConnectionProfile[]> {
    assertTauri();
    return invoke<ConnectionProfile[]>("list_profiles");
  },

  /// 启动时配置读取出现的问题（已从备份恢复等），取一次即清空。
  takeStoreWarning(): Promise<string | null> {
    assertTauri();
    return invoke<string | null>("take_store_warning");
  },

  saveProfile(
    input: ProfileInput,
    password?: string,
  ): Promise<ConnectionProfile> {
    assertTauri();
    return invoke<ConnectionProfile>("save_profile", {
      input,
      password: password || null,
    });
  },

  deleteProfile(id: string): Promise<void> {
    assertTauri();
    return invoke<void>("delete_profile", { id });
  },

  testConnection(input: ProfileInput, password?: string): Promise<ConnectionSnapshot> {
    assertTauri();
    return invoke<ConnectionSnapshot>("test_connection", {
      input,
      password: password || null,
    });
  },

  connect(profileId: string, password?: string): Promise<ConnectionSnapshot> {
    assertTauri();
    return invoke<ConnectionSnapshot>("connect", {
      profileId,
      password: password || null,
    });
  },

  /// 连接失效后原地重建，保留当前库与只读状态。
  reconnect(profileId: string): Promise<void> {
    assertTauri();
    return invoke<void>("reconnect", { profileId });
  },

  /// 轻量探活，用于区分命令错误和连接断开。
  connectionAlive(profileId: string): Promise<boolean> {
    assertTauri();
    return invoke<boolean>("connection_alive", { profileId });
  },

  getSavedPassword(profileId: string): Promise<string | null> {
    assertTauri();
    return invoke<string | null>("get_saved_password", { profileId });
  },

  loadSettings(): Promise<unknown | null> {
    assertTauri();
    return invoke<unknown | null>("load_settings");
  },

  saveSettings(settings: unknown): Promise<void> {
    assertTauri();
    return invoke<void>("save_settings", { settings });
  },

  disconnect(profileId: string): Promise<void> {
    assertTauri();
    return invoke<void>("disconnect", { profileId });
  },

  getDatabases(profileId: string): Promise<DatabasesInfo> {
    assertTauri();
    return invoke<DatabasesInfo>("get_databases", { profileId });
  },

  selectDatabase(
    profileId: string,
    db: number,
  ): Promise<DatabaseSelection> {
    assertTauri();
    return invoke<DatabaseSelection>("select_database", { profileId, db });
  },

  flushDatabase(profileId: string): Promise<void> {
    assertTauri();
    return invoke<void>("flush_db", { profileId });
  },

  /// 性能页轮询用的轻量指标（INFO + PING），不逐键统计内存。
  getPerformanceSummary(profileId: string): Promise<PerformanceSummary> {
    assertTauri();
    return invoke<PerformanceSummary>("get_performance_summary", { profileId });
  },

  /// 深度扫描：逐键统计内存和大 Key，耗时随键数量增长。
  analyzePerformance(profileId: string): Promise<PerformanceReport> {
    assertTauri();
    return invoke<PerformanceReport>("analyze_performance", { profileId });
  },

  cancelPerformanceScan(profileId: string): Promise<void> {
    assertTauri();
    return invoke<void>("cancel_performance_scan", { profileId });
  },

  exportData(profileId: string, path: string): Promise<ExportResult> {
    assertTauri();
    return invoke<ExportResult>("export_data", { profileId, path });
  },

  importData(profileId: string, path: string): Promise<ImportResult> {
    assertTauri();
    return invoke<ImportResult>("import_data", { profileId, path });
  },

  setReadOnly(profileId: string, readOnly: boolean): Promise<void> {
    assertTauri();
    return invoke<void>("set_read_only", { profileId, readOnly });
  },

  scanKeys(
    profileId: string,
    cursor: number,
    pattern: string,
    count: number,
    typeFilter: string | null,
  ): Promise<ScanPage> {
    assertTauri();
    return invoke<ScanPage>("scan_keys", {
      profileId,
      cursor,
      pattern,
      count,
      typeFilter,
    });
  },

  /// 键信息与值一次取回，避免两次 IPC 和重复的 TYPE。
  getKeyDetail(
    profileId: string,
    key: string,
    cursor = 0,
    limit = 200,
  ): Promise<KeyDetailBundle> {
    assertTauri();
    return invoke<KeyDetailBundle>("get_key_detail", {
      profileId,
      key,
      cursor,
      limit,
    });
  },

  getKeyValue(
    profileId: string,
    key: string,
    cursor = 0,
    limit = 200,
    kind?: string,
  ): Promise<KeyValue> {
    assertTauri();
    return invoke<KeyValue>("get_key_value", {
      profileId,
      key,
      cursor,
      limit,
      kind: kind ?? null,
    });
  },

  /// 命令行面板：直接执行一条 Redis 命令。
  runCommand(profileId: string, command: string): Promise<CommandResult> {
    assertTauri();
    return invoke<CommandResult>("run_command", { profileId, command });
  },

  /// 读取 String 键的原始字节，用于十六进制查看。
  getKeyBytes(
    profileId: string,
    key: string,
    offset = 0,
    limit = 4096,
  ): Promise<KeyBytes> {
    assertTauri();
    return invoke<KeyBytes>("get_key_bytes", {
      profileId,
      key,
      offset,
      limit,
    });
  },

  setString(profileId: string, key: string, value: string): Promise<void> {
    assertTauri();
    return invoke<void>("set_string", { profileId, key, value });
  },

  createKey(profileId: string, input: CreateKeyInput): Promise<void> {
    assertTauri();
    return invoke<void>("create_key", {
      profileId,
      key: input.key,
      keyType: input.keyType,
      value: input.value ?? null,
      field: input.field ?? null,
      score: input.score ?? null,
      ttlSeconds: input.ttlSeconds ?? null,
    });
  },

  hashSet(
    profileId: string,
    key: string,
    field: string,
    value: string,
  ): Promise<void> {
    assertTauri();
    return invoke<void>("hash_set", { profileId, key, field, value });
  },

  hashDelete(profileId: string, key: string, field: string): Promise<void> {
    assertTauri();
    return invoke<void>("hash_delete", { profileId, key, field });
  },

  listPush(
    profileId: string,
    key: string,
    value: string,
    side: "left" | "right",
  ): Promise<void> {
    assertTauri();
    return invoke<void>("list_push", { profileId, key, value, side });
  },

  listSet(
    profileId: string,
    key: string,
    index: number,
    value: string,
  ): Promise<void> {
    assertTauri();
    return invoke<void>("list_set", { profileId, key, index, value });
  },

  /// 按索引删除 List 元素，重复值不会删错行。
  listRemoveAt(
    profileId: string,
    key: string,
    index: number,
  ): Promise<number> {
    assertTauri();
    return invoke<number>("list_remove_at", { profileId, key, index });
  },

  setAdd(profileId: string, key: string, member: string): Promise<void> {
    assertTauri();
    return invoke<void>("set_add", { profileId, key, member });
  },

  setRemove(profileId: string, key: string, member: string): Promise<void> {
    assertTauri();
    return invoke<void>("set_remove", { profileId, key, member });
  },

  zsetAdd(
    profileId: string,
    key: string,
    member: string,
    score: number,
  ): Promise<void> {
    assertTauri();
    return invoke<void>("zset_add", { profileId, key, member, score });
  },

  zsetRemove(profileId: string, key: string, member: string): Promise<void> {
    assertTauri();
    return invoke<void>("zset_remove", { profileId, key, member });
  },

  deleteKey(profileId: string, key: string): Promise<number> {
    assertTauri();
    return invoke<number>("delete_key", { profileId, key });
  },

  updateKey(
    profileId: string,
    key: string,
    newKey: string,
    ttlSeconds: number | null,
    overwrite = false,
  ): Promise<string> {
    assertTauri();
    return invoke<string>("update_key", {
      profileId,
      key,
      newKey,
      ttlSeconds,
      overwrite,
    });
  },
};
