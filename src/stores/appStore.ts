import { create } from "zustand";
import { api, errorMessage } from "../lib/api";
import type {
  ConnectionProfile,
  ConnectionSnapshot,
  CreateKeyInput,
  DatabaseOption,
  KeyInfo,
  KeyItem,
  KeyValue,
  Notice,
  ProfileInput,
  ServerInfo,
} from "../lib/types";

let noticeSeq = 0;
let scanSeq = 0;
let detailSeq = 0;
let bootStarted = false;

function mergeKeys(existing: KeyItem[], incoming: KeyItem[]): KeyItem[] {
  if (existing.length === 0) return incoming;
  const seen = new Set(existing.map((item) => item.name));
  const merged = [...existing];
  for (const item of incoming) {
    if (!seen.has(item.name)) {
      seen.add(item.name);
      merged.push(item);
    }
  }
  return merged;
}

interface AppStore {
  booted: boolean;
  bootError: string | null;
  profiles: ConnectionProfile[];
  activeProfileId: string | null;
  connection: ConnectionSnapshot | null;
  connecting: boolean;
  serverInfo: ServerInfo | null;
  databases: DatabaseOption[];
  maxDatabases: number;
  databaseLoading: boolean;

  keys: KeyItem[];
  scanCursor: number;
  scanDone: boolean;
  scanning: boolean;
  refreshingKeys: boolean;
  pattern: string;
  typeFilter: string;

  selectedKey: string | null;
  keyInfo: KeyInfo | null;
  keyValue: KeyValue | null;
  detailLoading: boolean;

  readOnly: boolean;
  notice: Notice | null;

  bootstrap: () => Promise<void>;
  showNotice: (kind: Notice["kind"], message: string) => void;
  clearNotice: () => void;

  saveProfile: (
    input: ProfileInput,
    password?: string,
  ) => Promise<ConnectionProfile>;
  deleteProfile: (id: string) => Promise<void>;
  connectTo: (profileId: string, password?: string) => Promise<void>;
  disconnect: () => Promise<void>;

  setPattern: (pattern: string) => void;
  setTypeFilter: (typeFilter: string) => void;
  refreshKeys: () => Promise<void>;
  loadMoreKeys: () => Promise<void>;
  refreshDatabases: () => Promise<void>;
  selectDatabase: (db: number) => Promise<void>;
  clearDatabase: () => Promise<boolean>;
  selectKey: (key: string) => Promise<void>;
  reloadSelected: () => Promise<void>;
  loadMoreValue: () => Promise<void>;
  refreshServer: () => Promise<void>;

  clearSelection: () => void;

  toggleReadOnly: () => Promise<void>;
  createKey: (input: CreateKeyInput) => Promise<boolean>;
  saveString: (value: string) => Promise<void>;
  hashSet: (field: string, value: string) => Promise<void>;
  hashDelete: (field: string) => Promise<void>;
  listPush: (value: string, side: "left" | "right") => Promise<void>;
  listSet: (index: number, value: string) => Promise<void>;
  listRemove: (value: string) => Promise<void>;
  setAdd: (member: string) => Promise<void>;
  setRemove: (member: string) => Promise<void>;
  zsetAdd: (member: string, score: number) => Promise<void>;
  zsetRemove: (member: string) => Promise<void>;
  deleteSelected: () => Promise<void>;
  updateSelectedKey: (
    newKey: string,
    ttlSeconds: number | null,
  ) => Promise<boolean>;
}

export const useAppStore = create<AppStore>((set, get) => {
  const writable = (): boolean => {
    if (get().readOnly) {
      get().showNotice("error", "只读模式已开启，写操作已被前端拦截");
      return false;
    }
    return true;
  };

  const resetData = () => {
    scanSeq += 1;
    detailSeq += 1;
    set({
      serverInfo: null,
      databases: [],
      maxDatabases: 0,
      databaseLoading: false,
      keys: [],
      scanCursor: 0,
      scanDone: false,
      scanning: false,
      refreshingKeys: false,
      selectedKey: null,
      keyInfo: null,
      keyValue: null,
      detailLoading: false,
    });
  };

  const runMutation = async (
    action: () => Promise<void>,
    successMessage: string,
  ) => {
    try {
      await action();
      await get().reloadSelected();
      get().showNotice("success", successMessage);
    } catch (error) {
      get().showNotice("error", errorMessage(error));
    }
  };

  return {
    booted: false,
    bootError: null,
    profiles: [],
    activeProfileId: null,
    connection: null,
    connecting: false,
    serverInfo: null,
    databases: [],
    maxDatabases: 0,
    databaseLoading: false,

    keys: [],
    scanCursor: 0,
    scanDone: false,
    scanning: false,
    refreshingKeys: false,
    pattern: "*",
    typeFilter: "",

    selectedKey: null,
    keyInfo: null,
    keyValue: null,
    detailLoading: false,

    readOnly: false,
    notice: null,

    async bootstrap() {
      if (get().booted || bootStarted) return;
      bootStarted = true;
      try {
        const profiles = await api.listProfiles();
        set({ profiles, booted: true, bootError: null });
      } catch (error) {
        set({
          booted: true,
          bootError: errorMessage(error),
        });
        get().showNotice("error", errorMessage(error));
      }
    },

    showNotice(kind, message) {
      const id = ++noticeSeq;
      set({ notice: { id, kind, message } });
      window.setTimeout(() => {
        if (get().notice?.id === id) set({ notice: null });
      }, 4500);
    },

    clearNotice() {
      set({ notice: null });
    },

    async saveProfile(input, password) {
      const profile = await api.saveProfile(input, password);
      const profiles = get().profiles;
      const exists = profiles.some((item) => item.id === profile.id);
      set({
        profiles: exists
          ? profiles.map((item) => (item.id === profile.id ? profile : item))
          : [...profiles, profile],
      });
      return profile;
    },

    async deleteProfile(id) {
      await api.deleteProfile(id);
      const isActive = get().activeProfileId === id;
      set({
        profiles: get().profiles.filter((profile) => profile.id !== id),
      });
      if (isActive) {
        resetData();
        set({ activeProfileId: null, connection: null, readOnly: false });
      }
    },

    async connectTo(profileId, password = "") {
      scanSeq += 1;
      set({ connecting: true, activeProfileId: profileId });
      try {
        const snapshot = await api.connect(profileId, password);
        set({
          connection: snapshot,
          connecting: false,
          activeProfileId: profileId,
          bootError: null,
          readOnly: false,
        });
        resetData();
        await Promise.all([get().refreshKeys(), get().refreshServer()]);
      } catch (error) {
        set({ connecting: false, connection: null, activeProfileId: null });
        throw error;
      }
    },

    async disconnect() {
      const profileId = get().activeProfileId;
      if (profileId) {
        try {
          await api.disconnect(profileId);
        } catch {
          // 后端没有连接时忽略。
        }
      }
      resetData();
      set({ activeProfileId: null, connection: null, readOnly: false });
    },

    setPattern(pattern) {
      set({ pattern });
    },

    setTypeFilter(typeFilter) {
      set({ typeFilter });
      if (get().activeProfileId) void get().refreshKeys();
    },

    async refreshKeys() {
      const profileId = get().activeProfileId;
      if (!profileId) return;
      if (get().scanning) {
        scanSeq += 1;
      }
      const requestId = ++scanSeq;
      set({
        scanning: true,
        refreshingKeys: true,
        scanCursor: 0,
        scanDone: false,
      });

      try {
        const page = await api.scanKeys(
          profileId,
          0,
          get().pattern || "*",
          300,
          get().typeFilter || null,
        );
        if (requestId !== scanSeq) return;

        set({
          keys: page.keys,
          scanCursor: page.cursor,
          scanDone: page.done,
          scanning: false,
          refreshingKeys: false,
        });
        void get().refreshDatabases();
        void get().refreshServer();

        const currentKey = get().selectedKey;
        const nextKey =
          currentKey && page.keys.some((item) => item.name === currentKey)
            ? currentKey
            : page.keys[0]?.name;
        if (nextKey) {
          void get().selectKey(nextKey);
        } else {
          get().clearSelection();
        }
      } catch (error) {
        if (requestId !== scanSeq) return;
        set({ scanning: false, refreshingKeys: false });
        get().showNotice("error", errorMessage(error));
      }
    },

    async loadMoreKeys() {
      const profileId = get().activeProfileId;
      if (!profileId || get().scanning || get().scanDone) return;
      const requestId = scanSeq;
      set({ scanning: true });
      try {
        const page = await api.scanKeys(
          profileId,
          get().scanCursor,
          get().pattern || "*",
          300,
          get().typeFilter || null,
        );
        if (requestId !== scanSeq) return;
        set({
          keys: mergeKeys(get().keys, page.keys),
          scanCursor: page.cursor,
          scanDone: page.done,
          scanning: false,
        });
      } catch (error) {
        if (requestId !== scanSeq) return;
        set({ scanning: false });
        get().showNotice("error", errorMessage(error));
      }
    },

    async refreshDatabases() {
      const profileId = get().activeProfileId;
      if (!profileId) return;
      try {
        const info = await api.getDatabases(profileId);
        if (get().activeProfileId !== profileId) return;
        set({
          databases: info.databases,
          maxDatabases: info.maxDatabases,
        });
      } catch {
        // 数据库列表失败不阻塞键浏览，保留现有选项。
      }
    },

    async selectDatabase(db) {
      const profileId = get().activeProfileId;
      const current = get().connection;
      if (!profileId || !current || current.db === db) return;

      scanSeq += 1;
      detailSeq += 1;
      set({ databaseLoading: true });
      try {
        const result = await api.selectDatabase(profileId, db);
        if (get().activeProfileId !== profileId) return;
        const latest = get().connection;
        set({
          connection: latest
            ? { ...latest, db: result.db, dbSize: result.dbSize }
            : latest,
          keys: [],
          scanCursor: 0,
          scanDone: false,
          scanning: false,
          refreshingKeys: false,
          selectedKey: null,
          keyInfo: null,
          keyValue: null,
          detailLoading: false,
          serverInfo: null,
          databaseLoading: false,
        });
        await Promise.all([get().refreshKeys(), get().refreshServer()]);
      } catch (error) {
        set({ databaseLoading: false });
        get().showNotice("error", errorMessage(error));
      }
    },

    async clearDatabase() {
      if (!writable()) return false;
      const profileId = get().activeProfileId;
      const db = get().connection?.db ?? 0;
      if (!profileId) return false;
      scanSeq += 1;
      detailSeq += 1;
      try {
        await api.flushDatabase(profileId);
        set({
          keys: [],
          scanCursor: 0,
          scanDone: false,
          scanning: false,
          refreshingKeys: false,
          selectedKey: null,
          keyInfo: null,
          keyValue: null,
          detailLoading: false,
        });
        get().showNotice("success", `db${db} 已清空`);
        await Promise.all([get().refreshKeys(), get().refreshServer()]);
        return true;
      } catch (error) {
        get().showNotice("error", errorMessage(error));
        return false;
      }
    },

    async selectKey(key) {
      const profileId = get().activeProfileId;
      if (!profileId) return;
      const sameKey = get().selectedKey === key;
      const requestId = ++detailSeq;
      set({
        selectedKey: key,
        detailLoading: true,
        ...(sameKey ? {} : { keyInfo: null, keyValue: null }),
      });
      try {
        const [info, value] = await Promise.all([
          api.getKeyInfo(profileId, key),
          api.getKeyValue(profileId, key, 0, 200),
        ]);
        if (requestId !== detailSeq) return;
        set({ keyInfo: info, keyValue: value, detailLoading: false });
      } catch (error) {
        if (requestId !== detailSeq) return;
        set({ detailLoading: false });
        get().showNotice("error", errorMessage(error));
      }
    },

    async reloadSelected() {
      const key = get().selectedKey;
      if (key) await get().selectKey(key);
    },

    async loadMoreValue() {
      const profileId = get().activeProfileId;
      const key = get().selectedKey;
      const current = get().keyValue;
      if (!profileId || !key || !current || current.cursor === 0) return;
      const requestId = detailSeq;
      try {
        const next = await api.getKeyValue(profileId, key, current.cursor, 200);
        if (requestId !== detailSeq || get().selectedKey !== key) return;
        set({
          keyValue: {
            ...next,
            rows: [...current.rows, ...next.rows],
          },
        });
      } catch (error) {
        get().showNotice("error", errorMessage(error));
      }
    },

    async refreshServer() {
      const profileId = get().activeProfileId;
      if (!profileId) return;
      try {
        const info = await api.getServerInfo(profileId);
        if (get().activeProfileId !== profileId) return;
        const current = get().connection;
        set({
          serverInfo: info,
          connection: current
            ? { ...current, dbSize: info.dbSize }
            : current,
        });
      } catch {
        // 状态栏信息不是关键路径，失败时保持旧值。
      }
    },

    clearSelection() {
      detailSeq += 1;
      set({ selectedKey: null, keyInfo: null, keyValue: null });
    },

    async toggleReadOnly() {
      const profileId = get().activeProfileId;
      if (!profileId) return;
      const next = !get().readOnly;
      try {
        await api.setReadOnly(profileId, next);
        set({ readOnly: next });
        get().showNotice("info", next ? "已开启只读模式" : "已关闭只读模式");
      } catch (error) {
        get().showNotice("error", errorMessage(error));
      }
    },

    async createKey(input) {
      if (!writable()) return false;
      const profileId = get().activeProfileId;
      if (!profileId) return false;
      try {
        await api.createKey(profileId, input);
        get().showNotice("success", "键已创建");
        await get().refreshKeys();
        await get().selectKey(input.key);
        return true;
      } catch (error) {
        get().showNotice("error", errorMessage(error));
        return false;
      }
    },

    async saveString(value) {
      if (!writable()) return;
      const profileId = get().activeProfileId;
      const key = get().selectedKey;
      if (!profileId || !key) return;
      await runMutation(
        () => api.setString(profileId, key, value),
        "字符串已保存",
      );
    },

    async hashSet(field, value) {
      if (!writable()) return;
      const profileId = get().activeProfileId;
      const key = get().selectedKey;
      if (!profileId || !key) return;
      await runMutation(
        () => api.hashSet(profileId, key, field, value),
        "Hash 字段已保存",
      );
    },

    async hashDelete(field) {
      if (!writable()) return;
      const profileId = get().activeProfileId;
      const key = get().selectedKey;
      if (!profileId || !key) return;
      await runMutation(
        () => api.hashDelete(profileId, key, field),
        "Hash 字段已删除",
      );
    },

    async listPush(value, side) {
      if (!writable()) return;
      const profileId = get().activeProfileId;
      const key = get().selectedKey;
      if (!profileId || !key) return;
      await runMutation(
        () => api.listPush(profileId, key, value, side),
        side === "right" ? "已追加到列表末尾" : "已插入到列表开头",
      );
    },

    async listSet(index, value) {
      if (!writable()) return;
      const profileId = get().activeProfileId;
      const key = get().selectedKey;
      if (!profileId || !key) return;
      await runMutation(
        () => api.listSet(profileId, key, index, value),
        "List 元素已保存",
      );
    },

    async listRemove(value) {
      if (!writable()) return;
      const profileId = get().activeProfileId;
      const key = get().selectedKey;
      if (!profileId || !key) return;
      await runMutation(
        () => api.listRemove(profileId, key, value).then(() => undefined),
        "List 元素已删除",
      );
    },

    async setAdd(member) {
      if (!writable()) return;
      const profileId = get().activeProfileId;
      const key = get().selectedKey;
      if (!profileId || !key) return;
      await runMutation(() => api.setAdd(profileId, key, member), "Set 成员已添加");
    },

    async setRemove(member) {
      if (!writable()) return;
      const profileId = get().activeProfileId;
      const key = get().selectedKey;
      if (!profileId || !key) return;
      await runMutation(
        () => api.setRemove(profileId, key, member),
        "Set 成员已删除",
      );
    },

    async zsetAdd(member, score) {
      if (!writable()) return;
      const profileId = get().activeProfileId;
      const key = get().selectedKey;
      if (!profileId || !key) return;
      await runMutation(
        () => api.zsetAdd(profileId, key, member, score),
        "ZSet 成员已保存",
      );
    },

    async zsetRemove(member) {
      if (!writable()) return;
      const profileId = get().activeProfileId;
      const key = get().selectedKey;
      if (!profileId || !key) return;
      await runMutation(
        () => api.zsetRemove(profileId, key, member),
        "ZSet 成员已删除",
      );
    },

    async deleteSelected() {
      if (!writable()) return;
      const profileId = get().activeProfileId;
      const key = get().selectedKey;
      if (!profileId || !key) return;
      try {
        const deleted = await api.deleteKey(profileId, key);
        set({
          keys: get().keys.filter((item) => item.name !== key),
          selectedKey: null,
          keyInfo: null,
          keyValue: null,
        });
        get().showNotice("success", deleted > 0 ? "键已删除" : "键不存在");
        void get().refreshServer();
        void get().refreshDatabases();
      } catch (error) {
        get().showNotice("error", errorMessage(error));
      }
    },

    async updateSelectedKey(newKey, ttlSeconds) {
      if (!writable()) return false;
      const profileId = get().activeProfileId;
      const oldKey = get().selectedKey;
      if (!profileId || !oldKey) return false;
      const target = newKey.trim();
      if (!target) {
        get().showNotice("error", "键名不能为空");
        return false;
      }
      try {
        const finalKey = await api.updateKey(
          profileId,
          oldKey,
          target,
          ttlSeconds,
        );
        set({
          keys: get().keys.map((item) =>
            item.name === oldKey ? { ...item, name: finalKey } : item,
          ),
          selectedKey: finalKey,
        });
        await get().selectKey(finalKey);
        get().showNotice("success", "键信息已更新");
        return true;
      } catch (error) {
        get().showNotice("error", errorMessage(error));
        return false;
      }
    },
  };
});
