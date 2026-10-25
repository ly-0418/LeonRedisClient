export interface ConnectionProfile {
  id: string;
  name: string;
  host: string;
  port: number;
  db: number;
  username?: string | null;
  rememberPassword: boolean;
  lastUsedAt?: number | null;
}

export interface ProfileInput {
  id?: string | null;
  name: string;
  host: string;
  port: number;
  db: number;
  username?: string | null;
  rememberPassword: boolean;
}

export interface ConnectionSnapshot {
  profileId: string;
  name: string;
  host: string;
  port: number;
  db: number;
  version: string;
  mode: string;
  role: string;
  dbSize: number;
  latencyMs: number;
}

export interface KeyItem {
  name: string;
  keyType: string;
}

export type CreatableKeyType = "string" | "hash" | "list" | "set" | "zset";

export interface CreateKeyInput {
  key: string;
  keyType: CreatableKeyType;
  value?: string;
  field?: string;
  score?: number;
  ttlSeconds?: number | null;
}

export interface ScanPage {
  cursor: number;
  keys: KeyItem[];
  done: boolean;
}

export interface DatabaseOption {
  index: number;
  keys: number;
}

export interface DatabasesInfo {
  maxDatabases: number;
  current: number;
  databases: DatabaseOption[];
}

export interface DatabaseSelection {
  db: number;
  dbSize: number;
}

export interface DatabaseMemory {
  db: number;
  keys: number;
  memory: number;
  memoryHuman: string;
  percent: number;
}

export interface BigKeyInfo {
  db: number;
  key: string;
  keyType: string;
  memory: number;
  memoryHuman: string;
}

export interface PerformanceReport {
  usedMemory: number;
  usedMemoryHuman: string;
  databasesMemory: number;
  databasesMemoryHuman: string;
  totalKeys: number;
  maxDatabases: number;
  latencyMs: number;
  server: ServerInfo;
  databases: DatabaseMemory[];
  bigKeys: BigKeyInfo[];
}

export interface ExportResult {
  path: string;
  databases: number;
  keys: number;
  bytes: number;
}

export interface ImportResult {
  path: string;
  databases: number;
  keys: number;
}

export interface KeyInfo {
  key: string;
  keyType: string;
  ttl: number;
  size?: number | null;
  encoding?: string | null;
}

export interface ValueRow {
  field: string;
  value: string;
  score?: number | null;
}

export interface KeyValue {
  kind: string;
  value: string;
  rows: ValueRow[];
  cursor: number;
  total: number;
  truncated: boolean;
  message?: string | null;
}

export interface DbInfo {
  db: number;
  keys: number;
  expires: number;
}

export interface ServerInfo {
  version: string;
  mode: string;
  role: string;
  os: string;
  archBits: number;
  connectedClients: number;
  blockedClients: number;
  usedMemory: number;
  usedMemoryHuman: string;
  maxmemory: number;
  maxmemoryHuman: string;
  totalCommandsProcessed: number;
  instantaneousOpsPerSec: number;
  keyspaceHits: number;
  keyspaceMisses: number;
  usedCpuSys: number;
  usedCpuUser: number;
  uptimeInSeconds: number;
  dbSize: number;
  keyspace: DbInfo[];
}

export interface Notice {
  id: number;
  kind: "info" | "success" | "error";
  message: string;
}
