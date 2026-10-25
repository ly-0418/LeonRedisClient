use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use redis::aio::MultiplexedConnection;
use redis::Value;
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::models::{
    AppSettings, BigKeyInfo, ConnectionProfile, ConnectionSnapshot, DatabaseMemory, DatabaseOption,
    DatabaseSelection, DatabasesInfo, ExportResult, ImportResult, KeyInfo, KeyItem, KeyValue,
    PerformanceReport, ProfileInput, ScanPage, ServerInfo, ValueRow,
};
use crate::redis_manager;
use crate::security;
use crate::state::{AppState, RedisConnection};
use crate::storage;

const VALUE_LIMIT: usize = 200;
const MAX_STRING_BYTES: usize = 64 * 1024;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn profile_from_input(input: ProfileInput, id: String) -> Result<ConnectionProfile, String> {
    let name = input.name.trim();
    let host = input.host.trim();
    if name.is_empty() {
        return Err("连接名称不能为空".to_string());
    }
    if host.is_empty() {
        return Err("主机地址不能为空".to_string());
    }
    if input.port == 0 {
        return Err("端口必须大于 0".to_string());
    }

    Ok(ConnectionProfile {
        id,
        name: name.to_string(),
        host: host.to_string(),
        port: input.port,
        db: input.db,
        username: clean_optional(input.username),
        remember_password: input.remember_password,
        last_used_at: None,
    })
}

#[tauri::command]
pub fn list_profiles(state: State<'_, AppState>) -> Vec<ConnectionProfile> {
    state.profiles_snapshot()
}

#[tauri::command]
pub fn load_settings(state: State<'_, AppState>) -> Option<AppSettings> {
    storage::load_settings(&state.store_path)
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: AppSettings) -> Result<(), String> {
    storage::persist_settings(&state.store_path, &settings)
}

#[tauri::command]
pub fn save_profile(
    state: State<'_, AppState>,
    input: ProfileInput,
    password: Option<String>,
) -> Result<ConnectionProfile, String> {
    let requested_id = clean_optional(input.id.clone());
    let id = requested_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let profile = profile_from_input(input, id)?;

    let mut profiles = state
        .profiles
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let saved = if let Some(existing) = profiles.iter_mut().find(|item| item.id == profile.id) {
        let mut updated = profile.clone();
        updated.last_used_at = existing.last_used_at;
        *existing = updated.clone();
        updated
    } else {
        profiles.push(profile.clone());
        profile
    };

    storage::persist_profiles(&state.store_path, &profiles)?;

    if !saved.remember_password {
        storage::remove_secret(&state.store_path, &saved.id)?;
    } else if let Some(password) = password.filter(|value| !value.trim().is_empty()) {
        let encrypted = security::protect(&password)?;
        storage::save_secret(&state.store_path, &saved.id, &encrypted)?;
    }

    Ok(saved)
}

#[tauri::command]
pub async fn delete_profile(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.connections.write().await.remove(&id);
    storage::remove_secret(&state.store_path, &id)?;

    let mut profiles = state
        .profiles
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    profiles.retain(|profile| profile.id != id);
    storage::persist_profiles(&state.store_path, &profiles)?;
    Ok(())
}

#[tauri::command]
pub async fn test_connection(
    input: ProfileInput,
    password: Option<String>,
) -> Result<ConnectionSnapshot, String> {
    let profile = profile_from_input(input, "test".to_string())?;
    let (mut conn, latency_ms) = redis_manager::open(&profile, password).await?;
    redis_manager::snapshot(&profile, &mut conn, latency_ms).await
}

#[tauri::command]
pub async fn connect(
    state: State<'_, AppState>,
    profile_id: String,
    password: Option<String>,
) -> Result<ConnectionSnapshot, String> {
    let mut profile = {
        let profiles = state
            .profiles
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        profiles
            .iter()
            .find(|profile| profile.id == profile_id)
            .cloned()
            .ok_or_else(|| "连接配置不存在".to_string())?
    };
    // 每次重新连接默认进入 db0；连接后仍可通过数据库下拉切换。
    profile.db = 0;

    let supplied_password = password.filter(|value| !value.trim().is_empty());
    let password = match supplied_password {
        Some(password) => Some(password),
        None if profile.remember_password => storage::load_secret(&state.store_path, &profile_id)
            .map(|encrypted| security::unprotect(&encrypted))
            .transpose()?,
        None => None,
    };

    let (mut conn, latency_ms) = redis_manager::open(&profile, password.clone()).await?;
    let snapshot = redis_manager::snapshot(&profile, &mut conn, latency_ms).await?;

    {
        let mut profiles = state
            .profiles
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(existing) = profiles.iter_mut().find(|item| item.id == profile_id) {
            existing.last_used_at = Some(now_secs());
            existing.db = 0;
        }
        if let Err(err) = storage::persist_profiles(&state.store_path, &profiles) {
            eprintln!("更新最近连接时间失败: {err}");
        }
    }

    state.connections.write().await.insert(
        profile_id,
        RedisConnection {
            conn,
            read_only: AtomicBool::new(false),
            db: profile.db,
            session_password: password,
        },
    );

    Ok(snapshot)
}

#[tauri::command]
pub fn get_saved_password(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<Option<String>, String> {
    let remember = {
        let profiles = state
            .profiles
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        profiles
            .iter()
            .find(|profile| profile.id == profile_id)
            .map(|profile| profile.remember_password)
            .unwrap_or(false)
    };
    if !remember {
        return Ok(None);
    }

    match storage::load_secret(&state.store_path, &profile_id) {
        Some(encrypted) => security::unprotect(&encrypted).map(Some),
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn set_read_only(
    state: State<'_, AppState>,
    profile_id: String,
    read_only: bool,
) -> Result<(), String> {
    state.set_read_only(&profile_id, read_only).await
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>, profile_id: String) -> Result<(), String> {
    state.connections.write().await.remove(&profile_id);
    Ok(())
}

#[tauri::command]
pub async fn get_databases(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<DatabasesInfo, String> {
    let (mut conn, current) = {
        let connections = state.connections.read().await;
        let entry = connections
            .get(&profile_id)
            .ok_or_else(|| "连接已断开，请重新连接".to_string())?;
        (entry.conn.clone(), entry.db)
    };

    let raw: Value = redis::cmd("CONFIG")
        .arg("GET")
        .arg("databases")
        .query_async(&mut conn)
        .await
        .unwrap_or(Value::Nil);
    let max_databases = as_array(&raw)
        .get(1)
        .and_then(value_as_string)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(16)
        .clamp(1, 1024);

    let info: String = redis::cmd("INFO")
        .arg("keyspace")
        .query_async(&mut conn)
        .await
        .unwrap_or_default();
    let counts = parse_keyspace_counts(&info);

    let databases = (0..max_databases)
        .map(|index| DatabaseOption {
            index,
            keys: counts.get(&index).copied().unwrap_or(0),
        })
        .collect();

    Ok(DatabasesInfo {
        max_databases,
        current,
        databases,
    })
}

fn parse_keyspace_counts(info: &str) -> HashMap<u32, u64> {
    let mut counts = HashMap::new();
    for line in info.lines() {
        let Some((name, values)) = line.trim().split_once(':') else {
            continue;
        };
        let Some(db) = name
            .strip_prefix("db")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        for part in values.split(',') {
            if let Some(raw) = part.strip_prefix("keys=") {
                if let Ok(keys) = raw.parse::<u64>() {
                    counts.insert(db, keys);
                }
            }
        }
    }
    counts
}

#[tauri::command]
pub async fn select_database(
    state: State<'_, AppState>,
    profile_id: String,
    db: u32,
) -> Result<DatabaseSelection, String> {
    let mut conn = state.connection(&profile_id).await?;
    redis::cmd("SELECT")
        .arg(db)
        .query_async::<Value>(&mut conn)
        .await
        .map_err(|err| format!("切换数据库失败: {err}"))?;

    let db_size: u64 = redis::cmd("DBSIZE")
        .query_async(&mut conn)
        .await
        .unwrap_or(0);

    {
        let mut connections = state.connections.write().await;
        if let Some(entry) = connections.get_mut(&profile_id) {
            entry.db = db;
        }
    }

    {
        let mut profiles = state
            .profiles
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(profile) = profiles.iter_mut().find(|item| item.id == profile_id) {
            profile.db = db;
        }
        if let Err(err) = storage::persist_profiles(&state.store_path, &profiles) {
            eprintln!("保存当前数据库失败: {err}");
        }
    }

    Ok(DatabaseSelection { db, db_size })
}

#[tauri::command]
pub async fn flush_db(state: State<'_, AppState>, profile_id: String) -> Result<(), String> {
    let mut conn = state.writable_connection(&profile_id).await?;
    redis::cmd("FLUSHDB")
        .query_async::<Value>(&mut conn)
        .await
        .map_err(|err| format!("清空数据库失败: {err}"))?;
    Ok(())
}

#[tauri::command]
pub async fn analyze_performance(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<PerformanceReport, String> {
    let mut conn = auxiliary_connection(&state, &profile_id).await?;
    let max_databases = configured_databases(&mut conn).await;
    let (used_memory, used_memory_human) = memory_info(&mut conn).await;

    let mut total_keys = 0u64;
    let mut databases = Vec::new();
    let mut big_keys: Vec<(u64, u32, Vec<u8>, String)> = Vec::new();

    for db in 0..max_databases {
        redis::cmd("SELECT")
            .arg(db)
            .query_async::<Value>(&mut conn)
            .await
            .map_err(|err| format!("切换到 db{db} 失败: {err}"))?;

        let mut cursor = 0u64;
        let mut db_keys = 0u64;
        let mut db_memory = 0u64;
        loop {
            let (next, keys) = scan_batch(&mut conn, cursor).await?;
            if !keys.is_empty() {
                let mut pipe = redis::pipe();
                for key in &keys {
                    pipe.cmd("MEMORY")
                        .arg("USAGE")
                        .arg(key)
                        .cmd("TYPE")
                        .arg(key);
                }
                let values: Vec<Value> = pipe
                    .query_async(&mut conn)
                    .await
                    .map_err(|err| format!("读取 db{db} 内存用量失败: {err}"))?;
                for (index, key) in keys.iter().enumerate() {
                    let memory = values.get(index * 2).and_then(value_as_u64).unwrap_or(0);
                    let key_type = values
                        .get(index * 2 + 1)
                        .and_then(value_as_string)
                        .unwrap_or_else(|| "unknown".to_string());
                    db_memory += memory;
                    big_keys.push((memory, db, key.clone(), key_type));
                }
                db_keys += keys.len() as u64;
            }
            cursor = next;
            if cursor == 0 {
                break;
            }
        }

        total_keys += db_keys;
        if db_keys > 0 {
            databases.push((db, db_keys, db_memory));
        }
    }

    let databases_memory: u64 = databases.iter().map(|(_, _, memory)| memory).sum();
    let database_rows = databases
        .into_iter()
        .map(|(db, keys, memory)| DatabaseMemory {
            db,
            keys,
            memory,
            memory_human: human_bytes(memory),
            percent: if databases_memory == 0 {
                0.0
            } else {
                memory as f64 / databases_memory as f64 * 100.0
            },
        })
        .collect();

    big_keys.sort_by_key(|item| std::cmp::Reverse(item.0));
    big_keys.truncate(20);
    let big_key_rows = big_keys
        .into_iter()
        .map(|(memory, db, key, key_type)| BigKeyInfo {
            db,
            key: String::from_utf8_lossy(&key).into_owned(),
            key_type,
            memory,
            memory_human: human_bytes(memory),
        })
        .collect();

    let latency_started = std::time::Instant::now();
    let _: Value = redis::cmd("PING")
        .query_async(&mut conn)
        .await
        .map_err(|err| format!("测量网络延迟失败: {err}"))?;
    let latency_ms = latency_started.elapsed().as_millis() as u64;
    let server = redis_manager::server_info(&mut conn).await?;

    Ok(PerformanceReport {
        used_memory: if used_memory == 0 {
            databases_memory
        } else {
            used_memory
        },
        used_memory_human: if used_memory == 0 {
            human_bytes(databases_memory)
        } else {
            used_memory_human
        },
        databases_memory,
        databases_memory_human: human_bytes(databases_memory),
        total_keys,
        max_databases,
        latency_ms,
        server,
        databases: database_rows,
        big_keys: big_key_rows,
    })
}

#[derive(Debug, Serialize, Deserialize)]
struct ExportDocument {
    format: String,
    version: u32,
    exported_at: u64,
    databases: Vec<ExportDatabase>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExportDatabase {
    db: u32,
    keys: Vec<ExportKey>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExportKey {
    key: String,
    ttl_ms: i64,
    dump: String,
}

#[tauri::command]
pub async fn export_data(
    state: State<'_, AppState>,
    profile_id: String,
    path: String,
) -> Result<ExportResult, String> {
    if path.trim().is_empty() {
        return Err("导出路径不能为空".to_string());
    }
    let mut conn = auxiliary_connection(&state, &profile_id).await?;
    let max_databases = configured_databases(&mut conn).await;
    let mut document = ExportDocument {
        format: "leonredis-export".to_string(),
        version: 1,
        exported_at: now_secs(),
        databases: Vec::new(),
    };
    let mut total_keys = 0u64;

    for db in 0..max_databases {
        redis::cmd("SELECT")
            .arg(db)
            .query_async::<Value>(&mut conn)
            .await
            .map_err(|err| format!("切换到 db{db} 失败: {err}"))?;

        let mut cursor = 0u64;
        let mut keys = Vec::new();
        loop {
            let (next, batch) = scan_batch(&mut conn, cursor).await?;
            if !batch.is_empty() {
                let mut pipe = redis::pipe();
                for key in &batch {
                    pipe.cmd("DUMP").arg(key).cmd("PTTL").arg(key);
                }
                let values: Vec<Value> = pipe
                    .query_async(&mut conn)
                    .await
                    .map_err(|err| format!("导出 db{db} 失败: {err}"))?;
                for (index, key) in batch.iter().enumerate() {
                    let Some(dump) = values.get(index * 2).and_then(value_as_bytes) else {
                        continue;
                    };
                    if dump.is_empty() {
                        continue;
                    }
                    keys.push(ExportKey {
                        key: STANDARD.encode(key),
                        ttl_ms: values
                            .get(index * 2 + 1)
                            .and_then(value_as_i64)
                            .unwrap_or(-1),
                        dump: STANDARD.encode(dump),
                    });
                }
            }
            cursor = next;
            if cursor == 0 {
                break;
            }
        }

        total_keys += keys.len() as u64;
        if !keys.is_empty() {
            document.databases.push(ExportDatabase { db, keys });
        }
    }

    let json =
        serde_json::to_vec_pretty(&document).map_err(|err| format!("序列化导出文件失败: {err}"))?;
    std::fs::write(&path, &json).map_err(|err| format!("写入导出文件失败: {err}"))?;

    Ok(ExportResult {
        path,
        databases: document.databases.len() as u32,
        keys: total_keys,
        bytes: json.len() as u64,
    })
}

#[tauri::command]
pub async fn import_data(
    state: State<'_, AppState>,
    profile_id: String,
    path: String,
) -> Result<ImportResult, String> {
    let read_only = {
        let connections = state.connections.read().await;
        connections
            .get(&profile_id)
            .map(|entry| entry.read_only.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(false)
    };
    if read_only {
        return Err("只读模式已开启，无法导入数据".to_string());
    }

    let raw = std::fs::read(&path).map_err(|err| format!("读取导入文件失败: {err}"))?;
    let document: ExportDocument =
        serde_json::from_slice(&raw).map_err(|err| format!("解析导入文件失败: {err}"))?;
    if document.format != "leonredis-export" || document.version != 1 {
        return Err("不是有效的 LeonRedis 导出文件".to_string());
    }

    let mut conn = auxiliary_connection(&state, &profile_id).await?;
    let mut imported_keys = 0u64;

    for database in &document.databases {
        redis::cmd("SELECT")
            .arg(database.db)
            .query_async::<Value>(&mut conn)
            .await
            .map_err(|err| format!("切换到 db{} 失败: {err}", database.db))?;
        for entry in &database.keys {
            let key = STANDARD
                .decode(&entry.key)
                .map_err(|_| "导入文件中的键名编码无效".to_string())?;
            let dump = STANDARD
                .decode(&entry.dump)
                .map_err(|_| "导入文件中的键值编码无效".to_string())?;
            let ttl = if entry.ttl_ms > 0 {
                entry.ttl_ms as u64
            } else {
                0
            };
            redis::cmd("RESTORE")
                .arg(&key)
                .arg(ttl)
                .arg(dump)
                .arg("REPLACE")
                .query_async::<Value>(&mut conn)
                .await
                .map_err(|err| {
                    format!(
                        "导入 db{} 的键 {} 失败: {err}",
                        database.db,
                        String::from_utf8_lossy(&key)
                    )
                })?;
            imported_keys += 1;
        }
    }

    Ok(ImportResult {
        path,
        databases: document.databases.len() as u32,
        keys: imported_keys,
    })
}

#[tauri::command]
pub async fn scan_keys(
    state: State<'_, AppState>,
    profile_id: String,
    cursor: u64,
    pattern: Option<String>,
    count: Option<u32>,
    type_filter: Option<String>,
) -> Result<ScanPage, String> {
    let mut conn = state.connection(&profile_id).await?;
    let pattern = clean_optional(pattern).unwrap_or_else(|| "*".to_string());
    let count = count.unwrap_or(200).clamp(10, 1000);
    let type_filter = clean_optional(type_filter);

    let raw = match scan_once(&mut conn, cursor, &pattern, count, type_filter.as_deref()).await {
        Ok(raw) => raw,
        Err(err) if type_filter.is_some() && err.to_string().to_lowercase().contains("syntax") => {
            // Redis 5 及更早版本不支持 SCAN ... TYPE，退化为客户端过滤。
            let raw = scan_once(&mut conn, cursor, &pattern, count, None)
                .await
                .map_err(|err| format!("扫描键失败: {err}"))?;
            filter_scan_by_type(&mut conn, raw, type_filter.as_deref().unwrap_or("")).await?
        }
        Err(err) => return Err(format!("扫描键失败: {err}")),
    };

    let reply = as_array(&raw);
    let next = reply.first().and_then(value_as_u64).unwrap_or(0);
    let names: Vec<String> = reply
        .get(1)
        .map(as_array)
        .unwrap_or(&[])
        .iter()
        .filter_map(value_as_string)
        .collect();
    let key_types = key_types(&mut conn, &names).await;
    let keys = names
        .into_iter()
        .zip(key_types)
        .map(|(name, key_type)| KeyItem { name, key_type })
        .collect();

    Ok(ScanPage {
        cursor: next,
        keys,
        done: next == 0,
    })
}

async fn key_types(conn: &mut MultiplexedConnection, keys: &[String]) -> Vec<String> {
    if keys.is_empty() {
        return Vec::new();
    }

    let mut pipe = redis::pipe();
    for key in keys {
        pipe.cmd("TYPE").arg(key);
    }

    match pipe.query_async::<Vec<String>>(conn).await {
        Ok(types) if types.len() == keys.len() => types,
        _ => vec!["unknown".to_string(); keys.len()],
    }
}

async fn scan_once(
    conn: &mut MultiplexedConnection,
    cursor: u64,
    pattern: &str,
    count: u32,
    type_filter: Option<&str>,
) -> redis::RedisResult<Value> {
    let mut cmd = redis::cmd("SCAN");
    cmd.arg(cursor);
    if pattern != "*" {
        cmd.arg("MATCH").arg(pattern);
    }
    cmd.arg("COUNT").arg(count);
    if let Some(type_filter) = type_filter {
        cmd.arg("TYPE").arg(type_filter);
    }
    cmd.query_async(conn).await
}

async fn filter_scan_by_type(
    conn: &mut MultiplexedConnection,
    raw: Value,
    type_filter: &str,
) -> Result<Value, String> {
    let reply = as_array(&raw);
    let cursor = reply.first().cloned().unwrap_or(Value::Int(0));
    let keys: Vec<Value> = reply
        .get(1)
        .map(as_array)
        .unwrap_or(&[])
        .iter()
        .filter(|value| matches!(value, Value::BulkString(_)))
        .cloned()
        .collect();

    if keys.is_empty() {
        return Ok(Value::Array(vec![cursor, Value::Array(Vec::new())]));
    }

    let mut pipe = redis::pipe();
    for key in &keys {
        if let Value::BulkString(bytes) = key {
            pipe.cmd("TYPE").arg(bytes);
        }
    }
    let types: Vec<String> = pipe
        .query_async(conn)
        .await
        .map_err(|err| format!("读取键类型失败: {err}"))?;

    let filtered = keys
        .into_iter()
        .zip(types)
        .filter(|(_, key_type)| key_type.eq_ignore_ascii_case(type_filter))
        .map(|(key, _)| key)
        .collect();

    Ok(Value::Array(vec![cursor, Value::Array(filtered)]))
}

#[tauri::command]
pub async fn get_key_info(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
) -> Result<KeyInfo, String> {
    let mut conn = state.connection(&profile_id).await?;

    let key_type: String = redis::cmd("TYPE")
        .arg(&key)
        .query_async(&mut conn)
        .await
        .map_err(|err| format!("读取键类型失败: {err}"))?;

    let ttl: i64 = redis::cmd("TTL")
        .arg(&key)
        .query_async(&mut conn)
        .await
        .unwrap_or(-2);

    let size: Option<u64> = redis::cmd("MEMORY")
        .arg("USAGE")
        .arg(&key)
        .query_async(&mut conn)
        .await
        .ok()
        .flatten();

    let encoding: Option<String> = redis::cmd("OBJECT")
        .arg("ENCODING")
        .arg(&key)
        .query_async(&mut conn)
        .await
        .ok();

    Ok(KeyInfo {
        key,
        key_type,
        ttl,
        size,
        encoding,
    })
}

#[tauri::command]
pub async fn get_key_value(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
    cursor: Option<u64>,
    limit: Option<u32>,
) -> Result<KeyValue, String> {
    let mut conn = state.connection(&profile_id).await?;
    let limit = limit.unwrap_or(VALUE_LIMIT as u32).clamp(20, 500) as usize;
    let key_type: String = redis::cmd("TYPE")
        .arg(&key)
        .query_async(&mut conn)
        .await
        .map_err(|err| format!("读取键类型失败: {err}"))?;

    match key_type.as_str() {
        "string" => read_string(&mut conn, &key).await,
        "hash" => read_hash(&mut conn, &key, cursor.unwrap_or(0), limit).await,
        "list" => read_list(&mut conn, &key, cursor.unwrap_or(0), limit).await,
        "set" => read_set(&mut conn, &key, cursor.unwrap_or(0), limit).await,
        "zset" => read_zset(&mut conn, &key, cursor.unwrap_or(0), limit).await,
        "stream" => Ok(KeyValue {
            kind: "stream".to_string(),
            value: String::new(),
            rows: Vec::new(),
            cursor: 0,
            total: 0,
            truncated: false,
            message: Some("Stream 的可视化编辑将在后续版本提供。".to_string()),
        }),
        "none" => Ok(KeyValue {
            kind: "none".to_string(),
            value: String::new(),
            rows: Vec::new(),
            cursor: 0,
            total: 0,
            truncated: false,
            message: Some("键不存在或已过期".to_string()),
        }),
        other => Ok(KeyValue {
            kind: other.to_string(),
            value: String::new(),
            rows: Vec::new(),
            cursor: 0,
            total: 0,
            truncated: false,
            message: Some(format!("暂不支持可视化查看 {other} 类型")),
        }),
    }
}

async fn read_string(conn: &mut MultiplexedConnection, key: &str) -> Result<KeyValue, String> {
    let raw: Value = redis::cmd("GET")
        .arg(key)
        .query_async(conn)
        .await
        .map_err(|err| format!("读取字符串失败: {err}"))?;

    match raw {
        Value::Nil => Ok(KeyValue {
            kind: "string".to_string(),
            value: String::new(),
            rows: Vec::new(),
            cursor: 0,
            total: 0,
            truncated: false,
            message: Some("键不存在或已过期".to_string()),
        }),
        Value::BulkString(bytes) => {
            let total = bytes.len() as u64;
            let truncated = bytes.len() > MAX_STRING_BYTES;
            let slice = if truncated {
                &bytes[..MAX_STRING_BYTES]
            } else {
                &bytes
            };
            Ok(KeyValue {
                kind: "string".to_string(),
                value: String::from_utf8_lossy(slice).into_owned(),
                rows: Vec::new(),
                cursor: 0,
                total,
                truncated,
                message: None,
            })
        }
        _ => Err("该键不是字符串类型".to_string()),
    }
}

async fn read_hash(
    conn: &mut MultiplexedConnection,
    key: &str,
    cursor: u64,
    limit: usize,
) -> Result<KeyValue, String> {
    let total: u64 = redis::cmd("HLEN")
        .arg(key)
        .query_async(conn)
        .await
        .unwrap_or(0);

    let raw: Value = redis::cmd("HSCAN")
        .arg(key)
        .arg(cursor)
        .arg("COUNT")
        .arg(limit)
        .query_async(conn)
        .await
        .map_err(|err| format!("扫描 Hash 失败: {err}"))?;

    let reply = as_array(&raw);
    let next = reply.first().and_then(value_as_u64).unwrap_or(0);
    let entries = reply.get(1).map(as_array).unwrap_or(&[]);
    let rows = entries
        .chunks(2)
        .map(|chunk| ValueRow {
            field: chunk.first().and_then(value_as_string).unwrap_or_default(),
            value: chunk.get(1).and_then(value_as_string).unwrap_or_default(),
            score: None,
        })
        .collect();

    Ok(KeyValue {
        kind: "hash".to_string(),
        value: String::new(),
        rows,
        cursor: next,
        total,
        truncated: false,
        message: None,
    })
}

async fn read_list(
    conn: &mut MultiplexedConnection,
    key: &str,
    cursor: u64,
    limit: usize,
) -> Result<KeyValue, String> {
    let total: u64 = redis::cmd("LLEN")
        .arg(key)
        .query_async(conn)
        .await
        .unwrap_or(0);
    let start = cursor;
    let stop = start.saturating_add(limit as u64).saturating_sub(1);

    let raw: Value = redis::cmd("LRANGE")
        .arg(key)
        .arg(start)
        .arg(stop)
        .query_async(conn)
        .await
        .map_err(|err| format!("读取 List 失败: {err}"))?;

    let items = as_array(&raw);
    let rows = items
        .iter()
        .enumerate()
        .map(|(offset, value)| ValueRow {
            field: (start as usize + offset).to_string(),
            value: value_as_string(value).unwrap_or_default(),
            score: None,
        })
        .collect();
    let next = start.saturating_add(items.len() as u64);
    let next = if next >= total { 0 } else { next };

    Ok(KeyValue {
        kind: "list".to_string(),
        value: String::new(),
        rows,
        cursor: next,
        total,
        truncated: false,
        message: None,
    })
}

async fn read_set(
    conn: &mut MultiplexedConnection,
    key: &str,
    cursor: u64,
    limit: usize,
) -> Result<KeyValue, String> {
    let total: u64 = redis::cmd("SCARD")
        .arg(key)
        .query_async(conn)
        .await
        .unwrap_or(0);

    let raw: Value = redis::cmd("SSCAN")
        .arg(key)
        .arg(cursor)
        .arg("COUNT")
        .arg(limit)
        .query_async(conn)
        .await
        .map_err(|err| format!("扫描 Set 失败: {err}"))?;

    let reply = as_array(&raw);
    let next = reply.first().and_then(value_as_u64).unwrap_or(0);
    let members = reply.get(1).map(as_array).unwrap_or(&[]);
    let rows = members
        .iter()
        .filter_map(value_as_string)
        .map(|member| ValueRow {
            field: member,
            value: String::new(),
            score: None,
        })
        .collect();

    Ok(KeyValue {
        kind: "set".to_string(),
        value: String::new(),
        rows,
        cursor: next,
        total,
        truncated: false,
        message: None,
    })
}

async fn read_zset(
    conn: &mut MultiplexedConnection,
    key: &str,
    cursor: u64,
    limit: usize,
) -> Result<KeyValue, String> {
    let total: u64 = redis::cmd("ZCARD")
        .arg(key)
        .query_async(conn)
        .await
        .unwrap_or(0);
    let start = cursor;
    let stop = start.saturating_add(limit as u64).saturating_sub(1);

    let raw: Value = redis::cmd("ZRANGE")
        .arg(key)
        .arg(start)
        .arg(stop)
        .arg("WITHSCORES")
        .query_async(conn)
        .await
        .map_err(|err| format!("读取 ZSet 失败: {err}"))?;

    let items = as_array(&raw);
    let rows = items
        .chunks(2)
        .map(|chunk| ValueRow {
            field: chunk.first().and_then(value_as_string).unwrap_or_default(),
            value: String::new(),
            score: chunk
                .get(1)
                .and_then(value_as_string)
                .and_then(|score| score.parse().ok()),
        })
        .collect();
    let next = start.saturating_add(items.len() as u64 / 2);
    let next = if next >= total { 0 } else { next };

    Ok(KeyValue {
        kind: "zset".to_string(),
        value: String::new(),
        rows,
        cursor: next,
        total,
        truncated: false,
        message: None,
    })
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn create_key(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
    key_type: String,
    value: Option<String>,
    field: Option<String>,
    score: Option<f64>,
    ttl_seconds: Option<i64>,
) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("键名不能为空".to_string());
    }
    if let Some(ttl) = ttl_seconds {
        if ttl <= 0 {
            return Err("过期时间必须大于 0 秒".to_string());
        }
    }

    let mut conn = state.writable_connection(&profile_id).await?;
    let exists: i64 = redis::cmd("EXISTS")
        .arg(&key)
        .query_async(&mut conn)
        .await
        .map_err(|err| format!("检查键是否存在失败: {err}"))?;
    if exists > 0 {
        return Err("键已存在，请换一个键名或直接编辑原键".to_string());
    }

    let key_type = key_type.to_ascii_lowercase();
    match key_type.as_str() {
        "string" => {
            redis::cmd("SET")
                .arg(&key)
                .arg(value.unwrap_or_default())
                .query_async::<Value>(&mut conn)
                .await
                .map_err(|err| format!("创建 String 失败: {err}"))?;
        }
        "hash" => {
            let field = clean_optional(field).ok_or_else(|| "Hash 字段不能为空".to_string())?;
            redis::cmd("HSET")
                .arg(&key)
                .arg(field)
                .arg(value.unwrap_or_default())
                .query_async::<Value>(&mut conn)
                .await
                .map_err(|err| format!("创建 Hash 失败: {err}"))?;
        }
        "list" => {
            redis::cmd("RPUSH")
                .arg(&key)
                .arg(value.unwrap_or_default())
                .query_async::<Value>(&mut conn)
                .await
                .map_err(|err| format!("创建 List 失败: {err}"))?;
        }
        "set" => {
            redis::cmd("SADD")
                .arg(&key)
                .arg(value.unwrap_or_default())
                .query_async::<Value>(&mut conn)
                .await
                .map_err(|err| format!("创建 Set 失败: {err}"))?;
        }
        "zset" => {
            redis::cmd("ZADD")
                .arg(&key)
                .arg(score.unwrap_or(0.0))
                .arg(value.unwrap_or_default())
                .query_async::<Value>(&mut conn)
                .await
                .map_err(|err| format!("创建 ZSet 失败: {err}"))?;
        }
        other => return Err(format!("暂不支持创建 {other} 类型")),
    }

    if let Some(ttl) = ttl_seconds {
        redis::cmd("EXPIRE")
            .arg(&key)
            .arg(ttl)
            .query_async::<Value>(&mut conn)
            .await
            .map_err(|err| format!("键已创建，但设置过期时间失败: {err}"))?;
    }

    Ok(())
}

#[tauri::command]
pub async fn set_string(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
    value: String,
) -> Result<(), String> {
    let mut conn = state.writable_connection(&profile_id).await?;
    redis::cmd("SET")
        .arg(&key)
        .arg(value)
        .query_async::<Value>(&mut conn)
        .await
        .map_err(|err| format!("保存失败: {err}"))?;
    Ok(())
}

#[tauri::command]
pub async fn hash_set(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
    field: String,
    value: String,
) -> Result<(), String> {
    let mut conn = state.writable_connection(&profile_id).await?;
    redis::cmd("HSET")
        .arg(&key)
        .arg(field)
        .arg(value)
        .query_async::<Value>(&mut conn)
        .await
        .map_err(|err| format!("写入 Hash 失败: {err}"))?;
    Ok(())
}

#[tauri::command]
pub async fn hash_delete(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
    field: String,
) -> Result<(), String> {
    let mut conn = state.writable_connection(&profile_id).await?;
    redis::cmd("HDEL")
        .arg(&key)
        .arg(field)
        .query_async::<Value>(&mut conn)
        .await
        .map_err(|err| format!("删除 Hash 字段失败: {err}"))?;
    Ok(())
}

#[tauri::command]
pub async fn list_push(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
    value: String,
    side: Option<String>,
) -> Result<(), String> {
    let mut conn = state.writable_connection(&profile_id).await?;
    let command = if side.as_deref() == Some("left") {
        "LPUSH"
    } else {
        "RPUSH"
    };
    redis::cmd(command)
        .arg(&key)
        .arg(value)
        .query_async::<Value>(&mut conn)
        .await
        .map_err(|err| format!("写入 List 失败: {err}"))?;
    Ok(())
}

#[tauri::command]
pub async fn list_set(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
    index: i64,
    value: String,
) -> Result<(), String> {
    let mut conn = state.writable_connection(&profile_id).await?;
    redis::cmd("LSET")
        .arg(&key)
        .arg(index)
        .arg(value)
        .query_async::<Value>(&mut conn)
        .await
        .map_err(|err| format!("修改 List 元素失败: {err}"))?;
    Ok(())
}

#[tauri::command]
pub async fn list_remove(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
    value: String,
) -> Result<u64, String> {
    let mut conn = state.writable_connection(&profile_id).await?;
    let removed: u64 = redis::cmd("LREM")
        .arg(&key)
        .arg(1)
        .arg(value)
        .query_async(&mut conn)
        .await
        .map_err(|err| format!("删除 List 元素失败: {err}"))?;
    Ok(removed)
}

#[tauri::command]
pub async fn set_add(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
    member: String,
) -> Result<(), String> {
    let mut conn = state.writable_connection(&profile_id).await?;
    redis::cmd("SADD")
        .arg(&key)
        .arg(member)
        .query_async::<Value>(&mut conn)
        .await
        .map_err(|err| format!("写入 Set 失败: {err}"))?;
    Ok(())
}

#[tauri::command]
pub async fn set_remove(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
    member: String,
) -> Result<(), String> {
    let mut conn = state.writable_connection(&profile_id).await?;
    redis::cmd("SREM")
        .arg(&key)
        .arg(member)
        .query_async::<Value>(&mut conn)
        .await
        .map_err(|err| format!("删除 Set 成员失败: {err}"))?;
    Ok(())
}

#[tauri::command]
pub async fn zset_add(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
    member: String,
    score: f64,
) -> Result<(), String> {
    let mut conn = state.writable_connection(&profile_id).await?;
    redis::cmd("ZADD")
        .arg(&key)
        .arg(score)
        .arg(member)
        .query_async::<Value>(&mut conn)
        .await
        .map_err(|err| format!("写入 ZSet 失败: {err}"))?;
    Ok(())
}

#[tauri::command]
pub async fn zset_remove(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
    member: String,
) -> Result<(), String> {
    let mut conn = state.writable_connection(&profile_id).await?;
    redis::cmd("ZREM")
        .arg(&key)
        .arg(member)
        .query_async::<Value>(&mut conn)
        .await
        .map_err(|err| format!("删除 ZSet 成员失败: {err}"))?;
    Ok(())
}

#[tauri::command]
pub async fn delete_key(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
) -> Result<u64, String> {
    let mut conn = state.writable_connection(&profile_id).await?;
    let deleted: u64 = redis::cmd("DEL")
        .arg(&key)
        .query_async(&mut conn)
        .await
        .map_err(|err| format!("删除键失败: {err}"))?;
    Ok(deleted)
}

#[tauri::command]
pub async fn expire_key(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
    seconds: i64,
) -> Result<bool, String> {
    let mut conn = state.writable_connection(&profile_id).await?;
    let changed: i64 = redis::cmd("EXPIRE")
        .arg(&key)
        .arg(seconds)
        .query_async(&mut conn)
        .await
        .map_err(|err| format!("设置过期时间失败: {err}"))?;
    Ok(changed == 1)
}

#[tauri::command]
pub async fn rename_key(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
    new_key: String,
) -> Result<(), String> {
    let mut conn = state.writable_connection(&profile_id).await?;
    redis::cmd("RENAME")
        .arg(&key)
        .arg(new_key)
        .query_async::<Value>(&mut conn)
        .await
        .map_err(|err| format!("重命名失败: {err}"))?;
    Ok(())
}

#[tauri::command]
pub async fn update_key(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
    new_key: String,
    ttl_seconds: Option<i64>,
) -> Result<String, String> {
    let target = new_key.trim().to_string();
    if target.is_empty() {
        return Err("键名不能为空".to_string());
    }
    if let Some(ttl) = ttl_seconds {
        if ttl <= 0 {
            return Err("过期时间必须大于 0 秒".to_string());
        }
    }

    let mut conn = state.writable_connection(&profile_id).await?;
    let exists: i64 = redis::cmd("EXISTS")
        .arg(&key)
        .query_async(&mut conn)
        .await
        .map_err(|err| format!("检查键是否存在失败: {err}"))?;
    if exists == 0 {
        return Err("键已经不存在，可能已被删除或过期".to_string());
    }

    if key != target {
        redis::cmd("RENAME")
            .arg(&key)
            .arg(&target)
            .query_async::<Value>(&mut conn)
            .await
            .map_err(|err| format!("重命名失败: {err}"))?;
    }

    if let Some(ttl) = ttl_seconds {
        let changed: i64 = redis::cmd("EXPIRE")
            .arg(&target)
            .arg(ttl)
            .query_async(&mut conn)
            .await
            .map_err(|err| format!("设置过期时间失败: {err}"))?;
        if changed == 0 {
            return Err("键已更新，但设置过期时间失败".to_string());
        }
    } else {
        redis::cmd("PERSIST")
            .arg(&target)
            .query_async::<Value>(&mut conn)
            .await
            .map_err(|err| format!("移除过期时间失败: {err}"))?;
    }

    Ok(target)
}

#[tauri::command]
pub async fn get_server_info(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<ServerInfo, String> {
    let mut conn = state.connection(&profile_id).await?;
    redis_manager::server_info(&mut conn).await
}

fn as_array(value: &Value) -> &[Value] {
    match value {
        Value::Array(items) => items,
        _ => &[],
    }
}

fn value_as_string(value: &Value) -> Option<String> {
    match value {
        Value::BulkString(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        Value::SimpleString(text) => Some(text.clone()),
        Value::Int(value) => Some(value.to_string()),
        Value::Double(value) => Some(value.to_string()),
        Value::Boolean(value) => Some(value.to_string()),
        Value::Okay => Some("OK".to_string()),
        Value::BigNumber(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        Value::VerbatimString { text, .. } => Some(text.clone()),
        _ => None,
    }
}

fn value_as_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Int(value) => u64::try_from(*value).ok(),
        Value::BulkString(bytes) => std::str::from_utf8(bytes).ok()?.parse().ok(),
        Value::SimpleString(text) => text.parse().ok(),
        _ => None,
    }
}

fn value_as_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Int(value) => Some(*value),
        Value::BulkString(bytes) => std::str::from_utf8(bytes).ok()?.parse().ok(),
        Value::SimpleString(text) => text.parse().ok(),
        _ => None,
    }
}

fn value_as_bytes(value: &Value) -> Option<Vec<u8>> {
    match value {
        Value::BulkString(bytes) => Some(bytes.clone()),
        _ => value_as_string(value).map(String::into_bytes),
    }
}

fn human_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    if bytes < 1024 * 1024 {
        return format!("{:.1} KB", bytes as f64 / 1024.0);
    }
    if bytes < 1024 * 1024 * 1024 {
        return format!("{:.2} MB", bytes as f64 / 1024.0 / 1024.0);
    }
    format!("{:.2} GB", bytes as f64 / 1024.0 / 1024.0 / 1024.0)
}

async fn auxiliary_connection(
    state: &AppState,
    profile_id: &str,
) -> Result<MultiplexedConnection, String> {
    let profile = {
        let profiles = state
            .profiles
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        profiles
            .iter()
            .find(|profile| profile.id == profile_id)
            .cloned()
            .ok_or_else(|| "连接配置不存在".to_string())?
    };
    let password = {
        let connections = state.connections.read().await;
        connections
            .get(profile_id)
            .and_then(|entry| entry.session_password.clone())
    };
    let (conn, _) = redis_manager::open(&profile, password).await?;
    Ok(conn)
}

async fn configured_databases(conn: &mut MultiplexedConnection) -> u32 {
    let raw: Value = redis::cmd("CONFIG")
        .arg("GET")
        .arg("databases")
        .query_async(conn)
        .await
        .unwrap_or(Value::Nil);
    as_array(&raw)
        .get(1)
        .and_then(value_as_string)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(16)
        .clamp(1, 1024)
}

async fn memory_info(conn: &mut MultiplexedConnection) -> (u64, String) {
    let raw: String = redis::cmd("INFO")
        .arg("memory")
        .query_async(conn)
        .await
        .unwrap_or_default();
    let mut used = 0;
    let mut human = String::new();
    for line in raw.lines() {
        if let Some(value) = line.strip_prefix("used_memory:") {
            used = value.trim().parse().unwrap_or(0);
        } else if let Some(value) = line.strip_prefix("used_memory_human:") {
            human = value.trim().to_string();
        }
    }
    if human.is_empty() {
        human = human_bytes(used);
    }
    (used, human)
}

async fn scan_batch(
    conn: &mut MultiplexedConnection,
    cursor: u64,
) -> Result<(u64, Vec<Vec<u8>>), String> {
    let raw: Value = redis::cmd("SCAN")
        .arg(cursor)
        .arg("MATCH")
        .arg("*")
        .arg("COUNT")
        .arg(500)
        .query_async(conn)
        .await
        .map_err(|err| format!("扫描数据库失败: {err}"))?;
    let parts = as_array(&raw);
    let next = parts.first().and_then(value_as_u64).unwrap_or(0);
    let keys = parts
        .get(1)
        .map(as_array)
        .unwrap_or(&[])
        .iter()
        .filter_map(value_as_bytes)
        .collect();
    Ok((next, keys))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_scan_cursor_from_bulk_string() {
        assert_eq!(value_as_u64(&Value::BulkString(b"42".to_vec())), Some(42));
    }

    #[test]
    fn converts_scan_values_to_strings() {
        assert_eq!(
            value_as_string(&Value::BulkString("键名".as_bytes().to_vec())),
            Some("键名".to_string())
        );
        assert_eq!(value_as_string(&Value::Int(7)), Some("7".to_string()));
    }

    #[test]
    fn reads_array_reply() {
        let value = Value::Array(vec![Value::BulkString(b"a".to_vec())]);
        assert_eq!(as_array(&value).len(), 1);
        assert!(as_array(&Value::Nil).is_empty());
    }
}
