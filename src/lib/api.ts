import { invoke } from "@tauri-apps/api/core";
import type {
  ConnectionProfile,
  ConnectionSnapshot,
  CreateKeyInput,
  DatabaseSelection,
  DatabasesInfo,
  ExportResult,
  ImportResult,
  KeyInfo,
  KeyValue,
  PerformanceReport,
  ProfileInput,
  ScanPage,
  ServerInfo,
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

export const api = {
  listProfiles(): Promise<ConnectionProfile[]> {
    assertTauri();
    return invoke<ConnectionProfile[]>("list_profiles");
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

  analyzePerformance(profileId: string): Promise<PerformanceReport> {
    assertTauri();
    return invoke<PerformanceReport>("analyze_performance", { profileId });
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

  getKeyInfo(profileId: string, key: string): Promise<KeyInfo> {
    assertTauri();
    return invoke<KeyInfo>("get_key_info", { profileId, key });
  },

  getKeyValue(
    profileId: string,
    key: string,
    cursor = 0,
    limit = 200,
  ): Promise<KeyValue> {
    assertTauri();
    return invoke<KeyValue>("get_key_value", {
      profileId,
      key,
      cursor,
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

  listRemove(
    profileId: string,
    key: string,
    value: string,
  ): Promise<number> {
    assertTauri();
    return invoke<number>("list_remove", { profileId, key, value });
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
  ): Promise<string> {
    assertTauri();
    return invoke<string>("update_key", {
      profileId,
      key,
      newKey,
      ttlSeconds,
    });
  },

  getServerInfo(profileId: string): Promise<ServerInfo> {
    assertTauri();
    return invoke<ServerInfo>("get_server_info", { profileId });
  },
};
