use serde::{Deserialize, Serialize};

/// 保存在本机的连接配置。密码单独使用 Windows 凭据加密保存。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionProfile {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub db: u32,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub remember_password: bool,
    #[serde(default)]
    pub last_used_at: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub db: u32,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub remember_password: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme_mode: String,
    pub font_preset: String,
    pub font_size: String,
    pub accent_color: String,
    #[serde(default = "default_big_key_threshold")]
    pub big_key_threshold_bytes: u64,
}

fn default_big_key_threshold() -> u64 {
    1024
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme_mode: "system".to_string(),
            font_preset: "system".to_string(),
            font_size: "normal".to_string(),
            accent_color: "emerald".to_string(),
            big_key_threshold_bytes: default_big_key_threshold(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSnapshot {
    pub profile_id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub db: u32,
    pub version: String,
    pub mode: String,
    pub role: String,
    pub db_size: u64,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyInfo {
    pub key: String,
    pub key_type: String,
    pub ttl: i64,
    pub size: Option<u64>,
    pub encoding: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyItem {
    pub name: String,
    pub key_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanPage {
    pub cursor: u64,
    pub keys: Vec<KeyItem>,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseOption {
    pub index: u32,
    pub keys: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabasesInfo {
    pub max_databases: u32,
    pub current: u32,
    pub databases: Vec<DatabaseOption>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseSelection {
    pub db: u32,
    pub db_size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseMemory {
    pub db: u32,
    pub keys: u64,
    pub memory: u64,
    pub memory_human: String,
    pub percent: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BigKeyInfo {
    pub db: u32,
    pub key: String,
    pub key_type: String,
    pub memory: u64,
    pub memory_human: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceReport {
    pub used_memory: u64,
    pub used_memory_human: String,
    pub databases_memory: u64,
    pub databases_memory_human: String,
    pub total_keys: u64,
    pub max_databases: u32,
    pub latency_ms: u64,
    pub server: ServerInfo,
    pub databases: Vec<DatabaseMemory>,
    pub big_keys: Vec<BigKeyInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: String,
    pub databases: u32,
    pub keys: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub path: String,
    pub databases: u32,
    pub keys: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueRow {
    pub field: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyValue {
    pub kind: String,
    pub value: String,
    pub rows: Vec<ValueRow>,
    pub cursor: u64,
    pub total: u64,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbInfo {
    pub db: u32,
    pub keys: u64,
    pub expires: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub version: String,
    pub mode: String,
    pub role: String,
    pub os: String,
    pub arch_bits: u64,
    pub connected_clients: u64,
    pub blocked_clients: u64,
    pub used_memory: u64,
    pub used_memory_human: String,
    pub maxmemory: u64,
    pub maxmemory_human: String,
    pub total_commands_processed: u64,
    pub instantaneous_ops_per_sec: u64,
    pub keyspace_hits: u64,
    pub keyspace_misses: u64,
    pub used_cpu_sys: f64,
    pub used_cpu_user: f64,
    pub uptime_in_seconds: u64,
    pub db_size: u64,
    pub keyspace: Vec<DbInfo>,
}
