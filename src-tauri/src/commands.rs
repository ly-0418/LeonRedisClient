use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use redis::aio::MultiplexedConnection;
use redis::Value;
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::models::{
    AppSettings, BigKeyInfo, CommandResult, ConnectionProfile, ConnectionSnapshot, DatabaseMemory,
    DatabaseOption, DatabaseSelection, DatabasesInfo, ExportResult, ImportResult, KeyBytes,
    KeyDetailBundle, KeyInfo, KeyItem, KeyValue, PerformanceReport, PerformanceSummary,
    ProfileInput, ScanPage, ValueRow,
};
use crate::redis_manager;
use crate::security;
use crate::state::{AppState, RedisConnection};
use crate::storage;

const VALUE_LIMIT: usize = 200;
const MAX_STRING_BYTES: usize = 64 * 1024;
/// 普通读写命令的超时：避免服务器无响应时界面永久卡在加载状态。
const QUERY_TIMEOUT: Duration = Duration::from_secs(15);
/// 一页 SCAN 的超时。
const SCAN_TIMEOUT: Duration = Duration::from_secs(20);
/// 导入时单批 RESTORE 的超时。
const IMPORT_BATCH_TIMEOUT: Duration = Duration::from_secs(120);
const IMPORT_BATCH_SIZE: usize = 100;
/// 与前端 `TARGET_EXISTS_MESSAGE` 保持一致。
pub const TARGET_EXISTS_MESSAGE: &str = "目标键名已存在";

/// 带超时的单条命令，`what` 传入完整的中文动作描述（如「读取键类型失败」）。
async fn query<T, F>(future: F, timeout: Duration, what: &str) -> Result<T, String>
where
    T: redis::FromRedisValue,
    F: std::future::Future<Output = redis::RedisResult<T>>,
{
    match tokio::time::timeout(timeout, future).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(err)) => Err(format!("{what}: {err}")),
        Err(_) => Err(format!("{what}（超时，请检查网络或服务器状态）")),
    }
}

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
pub fn list_profiles(state: State<'_, AppState>) -> Result<Vec<ConnectionProfile>, String> {
    state.ensure_writable()?;
    Ok(state.profiles_snapshot())
}

/// 启动时读取配置可能出现的问题（已从备份恢复等），前端取一次并提示用户。
#[tauri::command]
pub fn take_store_warning(state: State<'_, AppState>) -> Option<String> {
    state.take_store_warning()
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
    state.ensure_writable()?;
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
    state.ensure_writable()?;
    state.connections.write().await.remove(&id);
    state.cleanup(&id).await;
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

    // 新会话：丢弃上一个会话残留的辅助连接与扫描令牌。
    state.cleanup(&profile_id).await;
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
    state.cleanup(&profile_id).await;
    Ok(())
}

/// 连接失效后就地重建，保留当前数据库、只读状态和会话密码。
#[tauri::command]
pub async fn reconnect(state: State<'_, AppState>, profile_id: String) -> Result<(), String> {
    state.reconnect(&profile_id).await
}

/// 轻量探活：前端在命令失败后用它区分「命令错误」和「连接已断」。
#[tauri::command]
pub async fn connection_alive(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<bool, String> {
    let Ok(mut conn) = state.connection(&profile_id).await else {
        return Ok(false);
    };
    Ok(matches!(
        tokio::time::timeout(
            Duration::from_millis(1500),
            redis::cmd("PING").query_async::<String>(&mut conn)
        )
        .await,
        Ok(Ok(pong)) if pong == "PONG"
    ))
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

    let raw: Value = query(
        redis::cmd("CONFIG")
            .arg("GET")
            .arg("databases")
            .query_async(&mut conn),
        QUERY_TIMEOUT,
        "读取数据库数量失败",
    )
    .await
    .unwrap_or(Value::Nil);
    let max_databases = as_array(&raw)
        .get(1)
        .and_then(value_as_string)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(16)
        .clamp(1, 1024);

    let info: String = query(
        redis::cmd("INFO").arg("keyspace").query_async(&mut conn),
        QUERY_TIMEOUT,
        "读取数据库键数失败",
    )
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
    query(
        redis::cmd("SELECT").arg(db).query_async::<Value>(&mut conn),
        QUERY_TIMEOUT,
        "切换数据库失败",
    )
    .await?;

    let db_size: u64 = query(
        redis::cmd("DBSIZE").query_async(&mut conn),
        QUERY_TIMEOUT,
        "读取键数量失败",
    )
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
    // ASYNC 版本不会在清空大库时长时间阻塞 Redis（Redis 4.0+）。
    let async_flush = query(
        redis::cmd("FLUSHDB")
            .arg("ASYNC")
            .query_async::<Value>(&mut conn),
        QUERY_TIMEOUT,
        "清空数据库失败",
    )
    .await;
    if async_flush.is_ok() {
        return Ok(());
    }
    query(
        redis::cmd("FLUSHDB").query_async::<Value>(&mut conn),
        QUERY_TIMEOUT,
        "清空数据库失败",
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn analyze_performance(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<PerformanceReport, String> {
    let mut conn = state.auxiliary_connection(&profile_id).await?;
    let cancel = state.begin_scan(&profile_id).await;
    let result = analyze_performance_inner(&mut conn, &cancel).await;
    if result.is_err() {
        // 辅助连接可能已失效，丢弃缓存让下一次重新建立。
        drop(conn);
        state.drop_auxiliary(&profile_id).await;
    }
    result
}

async fn analyze_performance_inner(
    conn: &mut MultiplexedConnection,
    cancel: &AtomicBool,
) -> Result<PerformanceReport, String> {
    let max_databases = configured_databases(conn).await;
    let (used_memory, used_memory_human) = memory_info(conn).await;

    let mut total_keys = 0u64;
    let mut databases = Vec::new();
    let mut big_keys: Vec<(u64, u32, Vec<u8>, String)> = Vec::new();

    for db in 0..max_databases {
        if cancel.load(Ordering::Relaxed) {
            return Err("扫描已取消".to_string());
        }
        query(
            redis::cmd("SELECT").arg(db).query_async::<Value>(conn),
            QUERY_TIMEOUT,
            &format!("切换到 db{db} 失败"),
        )
        .await?;

        let mut cursor = 0u64;
        let mut db_keys = 0u64;
        let mut db_memory = 0u64;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("扫描已取消".to_string());
            }
            let (next, keys) = scan_batch(conn, cursor).await?;
            if !keys.is_empty() {
                let mut pipe = redis::pipe();
                for key in &keys {
                    pipe.cmd("MEMORY")
                        .arg("USAGE")
                        .arg(key)
                        .cmd("TYPE")
                        .arg(key);
                }
                let values: Vec<Value> = query(
                    pipe.query_async(conn),
                    QUERY_TIMEOUT,
                    &format!("读取 db{db} 内存用量失败"),
                )
                .await?;
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
                // 只保留内存最大的 20 个，百万级键时也不会堆积几十 MB。
                if big_keys.len() > 200 {
                    big_keys.sort_unstable_by_key(|item| std::cmp::Reverse(item.0));
                    big_keys.truncate(20);
                }
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
    query(
        redis::cmd("PING").query_async::<Value>(conn),
        QUERY_TIMEOUT,
        "测量网络延迟失败",
    )
    .await?;
    let latency_ms = latency_started.elapsed().as_millis() as u64;
    let server = redis_manager::server_info(conn).await?;

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

/// 性能页的轻量轮询：只读 `INFO` + `PING`，不逐键统计内存。
#[tauri::command]
pub async fn get_performance_summary(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<PerformanceSummary, String> {
    let mut conn = state.connection(&profile_id).await?;

    let latency_started = std::time::Instant::now();
    query(
        redis::cmd("PING").query_async::<Value>(&mut conn),
        QUERY_TIMEOUT,
        "测量网络延迟失败",
    )
    .await?;
    let latency_ms = latency_started.elapsed().as_millis() as u64;

    let server = redis_manager::server_info(&mut conn).await?;
    let total_keys = server.keyspace.iter().map(|item| item.keys).sum();

    Ok(PerformanceSummary {
        server,
        latency_ms,
        total_keys,
    })
}

/// 取消正在进行的深度扫描。
#[tauri::command]
pub async fn cancel_performance_scan(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<(), String> {
    state.cancel_scan(&profile_id).await;
    Ok(())
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    let mut conn = state.auxiliary_connection(&profile_id).await?;
    let result = export_data_inner(&mut conn, &path).await;
    if result.is_err() {
        drop(conn);
        state.drop_auxiliary(&profile_id).await;
    }
    result
}

/// 边扫描边写入文件，避免把整个库先读进内存（大库会直接 OOM）。
async fn export_data_inner(
    conn: &mut MultiplexedConnection,
    path: &str,
) -> Result<ExportResult, String> {
    let max_databases = configured_databases(conn).await;
    let file = std::fs::File::create(path).map_err(|err| format!("创建导出文件失败: {err}"))?;
    let mut writer = std::io::BufWriter::new(file);

    let header = format!(
        "{{\"format\":\"leonredis-export\",\"version\":1,\"exported_at\":{},\"databases\":[",
        now_secs()
    );
    writer
        .write_all(header.as_bytes())
        .map_err(|err| format!("写入导出文件失败: {err}"))?;

    let mut databases = 0u32;
    let mut total_keys = 0u64;
    let mut first_database = true;

    for db in 0..max_databases {
        query(
            redis::cmd("SELECT").arg(db).query_async::<Value>(conn),
            QUERY_TIMEOUT,
            &format!("切换到 db{db} 失败"),
        )
        .await?;

        let mut cursor = 0u64;
        let mut database_open = false;
        loop {
            let (next, batch) = scan_batch(conn, cursor).await?;
            if !batch.is_empty() {
                let mut pipe = redis::pipe();
                for key in &batch {
                    pipe.cmd("DUMP").arg(key).cmd("PTTL").arg(key);
                }
                let values: Vec<Value> = query(
                    pipe.query_async(conn),
                    QUERY_TIMEOUT,
                    &format!("导出 db{db} 失败"),
                )
                .await?;
                for (index, key) in batch.iter().enumerate() {
                    let Some(dump) = values.get(index * 2).and_then(value_as_bytes) else {
                        continue;
                    };
                    if dump.is_empty() {
                        continue;
                    }
                    if database_open {
                        writer
                            .write_all(b",")
                            .map_err(|err| format!("写入导出文件失败: {err}"))?;
                    } else {
                        if !first_database {
                            writer
                                .write_all(b",")
                                .map_err(|err| format!("写入导出文件失败: {err}"))?;
                        }
                        first_database = false;
                        let head = format!("{{\"db\":{db},\"keys\":[");
                        writer
                            .write_all(head.as_bytes())
                            .map_err(|err| format!("写入导出文件失败: {err}"))?;
                        database_open = true;
                        databases += 1;
                    }
                    let entry = ExportKey {
                        key: STANDARD.encode(key),
                        ttl_ms: values
                            .get(index * 2 + 1)
                            .and_then(value_as_i64)
                            .unwrap_or(-1),
                        dump: STANDARD.encode(dump),
                    };
                    serde_json::to_writer(&mut writer, &entry)
                        .map_err(|err| format!("序列化导出数据失败: {err}"))?;
                    total_keys += 1;
                }
            }
            cursor = next;
            if cursor == 0 {
                break;
            }
        }

        if database_open {
            writer
                .write_all(b"]}")
                .map_err(|err| format!("写入导出文件失败: {err}"))?;
        }
    }

    writer
        .write_all(b"]}")
        .map_err(|err| format!("写入导出文件失败: {err}"))?;
    writer
        .flush()
        .map_err(|err| format!("写入导出文件失败: {err}"))?;

    let bytes = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);

    Ok(ExportResult {
        path: path.to_string(),
        databases,
        keys: total_keys,
        bytes,
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
            .map(|entry| entry.read_only.load(Ordering::Relaxed))
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

    let mut conn = state.auxiliary_connection(&profile_id).await?;
    let result = import_data_inner(&mut conn, &path, &document).await;
    if result.is_err() {
        drop(conn);
        state.drop_auxiliary(&profile_id).await;
    }
    result
}

async fn import_data_inner(
    conn: &mut MultiplexedConnection,
    path: &str,
    document: &ExportDocument,
) -> Result<ImportResult, String> {
    let mut imported_keys = 0u64;

    for database in &document.databases {
        query(
            redis::cmd("SELECT")
                .arg(database.db)
                .query_async::<Value>(conn),
            QUERY_TIMEOUT,
            &format!("切换到 db{} 失败", database.db),
        )
        .await?;

        for chunk in database.keys.chunks(IMPORT_BATCH_SIZE) {
            let mut pipe = redis::pipe();
            let mut decoded = Vec::with_capacity(chunk.len());
            for entry in chunk {
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
                pipe.cmd("RESTORE")
                    .arg(&key)
                    .arg(ttl)
                    .arg(&dump)
                    .arg("REPLACE");
                decoded.push((key, dump, ttl));
            }

            match query(
                pipe.query_async::<Vec<Value>>(conn),
                IMPORT_BATCH_TIMEOUT,
                "批量导入失败",
            )
            .await
            {
                Ok(_) => imported_keys += chunk.len() as u64,
                // 整批失败时逐条重做：RESTORE ... REPLACE 可重复执行，
                // 这样才能定位到具体哪个键失败并给出准确错误。
                Err(_) => {
                    for (key, dump, ttl) in decoded {
                        query(
                            redis::cmd("RESTORE")
                                .arg(&key)
                                .arg(ttl)
                                .arg(&dump)
                                .arg("REPLACE")
                                .query_async::<Value>(conn),
                            IMPORT_BATCH_TIMEOUT,
                            &format!(
                                "导入 db{} 的键 {} 失败",
                                database.db,
                                String::from_utf8_lossy(&key)
                            ),
                        )
                        .await?;
                        imported_keys += 1;
                    }
                }
            }
        }
    }

    Ok(ImportResult {
        path: path.to_string(),
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
        Err(err) if type_filter.is_some() && err.to_lowercase().contains("syntax") => {
            // Redis 5 及更早版本不支持 SCAN ... TYPE，退化为客户端过滤。
            scan_pages_by_type(
                &mut conn,
                cursor,
                &pattern,
                count,
                type_filter.as_deref().unwrap_or(""),
            )
            .await?
        }
        Err(err) => return Err(err),
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

    match query(
        pipe.query_async::<Vec<String>>(conn),
        QUERY_TIMEOUT,
        "读取键类型失败",
    )
    .await
    {
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
) -> Result<Value, String> {
    let mut cmd = redis::cmd("SCAN");
    cmd.arg(cursor);
    if pattern != "*" {
        cmd.arg("MATCH").arg(pattern);
    }
    cmd.arg("COUNT").arg(count);
    if let Some(type_filter) = type_filter {
        cmd.arg("TYPE").arg(type_filter);
    }
    query(cmd.query_async(conn), SCAN_TIMEOUT, "扫描键失败").await
}

/// 客户端类型过滤最多推进的页数，避免稀有的类型把一次请求拖成长时间扫描。
const MAX_FILTER_ROUNDS: usize = 20;

/// Redis < 6 的客户端类型过滤。
///
/// 单页可能一个匹配项都没有（例如库里绝大多数是 String，只筛 ZSet），
/// 只在当前页过滤会让界面误报「没有匹配的键」，所以有限次数地继续推进游标。
async fn scan_pages_by_type(
    conn: &mut MultiplexedConnection,
    start_cursor: u64,
    pattern: &str,
    count: u32,
    type_filter: &str,
) -> Result<Value, String> {
    let mut cursor = start_cursor;
    let mut cursor_value = Value::Int(0);
    let mut matched: Vec<Value> = Vec::new();
    let target = count as usize;

    for _ in 0..MAX_FILTER_ROUNDS {
        let raw = scan_once(conn, cursor, pattern, count, None).await?;
        let reply = as_array(&raw);
        cursor_value = reply.first().cloned().unwrap_or(Value::Int(0));
        cursor = value_as_u64(&cursor_value).unwrap_or(0);

        let filtered = filter_scan_by_type(conn, raw, type_filter).await?;
        if let Some(Value::Array(items)) = as_array(&filtered).get(1) {
            matched.extend(items.iter().cloned());
        }

        if cursor == 0 || matched.len() >= target {
            break;
        }
    }

    Ok(Value::Array(vec![cursor_value, Value::Array(matched)]))
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
    let types: Vec<String> = query(pipe.query_async(conn), QUERY_TIMEOUT, "读取键类型失败").await?;

    let filtered = keys
        .into_iter()
        .zip(types)
        .filter(|(_, key_type)| key_type.eq_ignore_ascii_case(type_filter))
        .map(|(key, _)| key)
        .collect();

    Ok(Value::Array(vec![cursor, Value::Array(filtered)]))
}

/// 一次 IPC 返回键信息 + 值：只发一次 TYPE，并把 TTL/MEMORY/OBJECT 合成一次往返。
#[tauri::command]
pub async fn get_key_detail(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
    cursor: Option<u64>,
    limit: Option<u32>,
) -> Result<KeyDetailBundle, String> {
    let mut conn = state.connection(&profile_id).await?;
    let limit = limit.unwrap_or(VALUE_LIMIT as u32).clamp(20, 500) as usize;
    let key_type: String = query(
        redis::cmd("TYPE").arg(&key).query_async(&mut conn),
        QUERY_TIMEOUT,
        "读取键类型失败",
    )
    .await?;

    let info = read_key_info(&mut conn, &key, &key_type).await?;
    let value = read_value(&mut conn, &key, &key_type, cursor.unwrap_or(0), limit).await?;
    Ok(KeyDetailBundle { info, value })
}

#[tauri::command]
pub async fn get_key_value(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
    cursor: Option<u64>,
    limit: Option<u32>,
    kind: Option<String>,
) -> Result<KeyValue, String> {
    let mut conn = state.connection(&profile_id).await?;
    let limit = limit.unwrap_or(VALUE_LIMIT as u32).clamp(20, 500) as usize;
    // 「加载更多」时前端已经知道键类型，省掉一次 TYPE 往返。
    let key_type = match clean_optional(kind) {
        Some(kind) => kind,
        None => {
            query(
                redis::cmd("TYPE").arg(&key).query_async(&mut conn),
                QUERY_TIMEOUT,
                "读取键类型失败",
            )
            .await?
        }
    };

    read_value(&mut conn, &key, &key_type, cursor.unwrap_or(0), limit).await
}

async fn read_key_info(
    conn: &mut MultiplexedConnection,
    key: &str,
    key_type: &str,
) -> Result<KeyInfo, String> {
    let mut pipe = redis::pipe();
    pipe.cmd("TTL")
        .arg(key)
        .cmd("MEMORY")
        .arg("USAGE")
        .arg(key)
        .cmd("OBJECT")
        .arg("ENCODING")
        .arg(key);

    let (ttl, size, encoding) = match query(
        pipe.query_async::<Vec<Value>>(conn),
        QUERY_TIMEOUT,
        "读取键信息失败",
    )
    .await
    {
        Ok(values) => (
            values.first().and_then(value_as_i64).unwrap_or(-2),
            values.get(1).and_then(value_as_u64),
            values.get(2).and_then(value_as_string),
        ),
        // 部分托管服务禁用了 MEMORY/OBJECT，退化为只读 TTL。
        Err(_) => (
            query(
                redis::cmd("TTL").arg(key).query_async(conn),
                QUERY_TIMEOUT,
                "读取过期时间失败",
            )
            .await
            .unwrap_or(-2),
            None,
            None,
        ),
    };

    Ok(KeyInfo {
        key: key.to_string(),
        key_type: key_type.to_string(),
        ttl,
        size,
        encoding,
    })
}

async fn read_value(
    conn: &mut MultiplexedConnection,
    key: &str,
    key_type: &str,
    cursor: u64,
    limit: usize,
) -> Result<KeyValue, String> {
    match key_type {
        "string" => read_string(conn, key).await,
        "hash" => read_hash(conn, key, cursor, limit).await,
        "list" => read_list(conn, key, cursor, limit).await,
        "set" => read_set(conn, key, cursor, limit).await,
        "zset" => read_zset(conn, key, cursor, limit).await,
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
    // 先取长度，再用 GETRANGE 只读前 64 KB：超大 String（Redis 上限 512 MB）
    // 不再整条读回内存。
    let total: u64 = query(
        redis::cmd("STRLEN").arg(key).query_async(conn),
        QUERY_TIMEOUT,
        "读取字符串长度失败",
    )
    .await
    .unwrap_or(0);

    if total == 0 {
        let exists: i64 = query(
            redis::cmd("EXISTS").arg(key).query_async(conn),
            QUERY_TIMEOUT,
            "检查键是否存在失败",
        )
        .await
        .unwrap_or(0);
        return Ok(KeyValue {
            kind: "string".to_string(),
            value: String::new(),
            rows: Vec::new(),
            cursor: 0,
            total: 0,
            truncated: false,
            message: if exists == 0 {
                Some("键不存在或已过期".to_string())
            } else {
                None
            },
        });
    }

    let truncated = total > MAX_STRING_BYTES as u64;
    let end = MAX_STRING_BYTES.saturating_sub(1);
    let raw: Value = query(
        redis::cmd("GETRANGE")
            .arg(key)
            .arg(0)
            .arg(end)
            .query_async(conn),
        QUERY_TIMEOUT,
        "读取字符串失败",
    )
    .await?;

    match raw {
        Value::BulkString(bytes) => Ok(KeyValue {
            kind: "string".to_string(),
            value: String::from_utf8_lossy(&bytes).into_owned(),
            rows: Vec::new(),
            cursor: 0,
            total,
            truncated,
            message: None,
        }),
        Value::Nil => Ok(KeyValue {
            kind: "string".to_string(),
            value: String::new(),
            rows: Vec::new(),
            cursor: 0,
            total: 0,
            truncated: false,
            message: Some("键不存在或已过期".to_string()),
        }),
        _ => Err("该键不是字符串类型".to_string()),
    }
}

async fn read_hash(
    conn: &mut MultiplexedConnection,
    key: &str,
    cursor: u64,
    limit: usize,
) -> Result<KeyValue, String> {
    let total: u64 = query(
        redis::cmd("HLEN").arg(key).query_async(conn),
        QUERY_TIMEOUT,
        "读取 Hash 长度失败",
    )
    .await
    .unwrap_or(0);

    let raw: Value = query(
        redis::cmd("HSCAN")
            .arg(key)
            .arg(cursor)
            .arg("COUNT")
            .arg(limit)
            .query_async(conn),
        QUERY_TIMEOUT,
        "扫描 Hash 失败",
    )
    .await?;

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
    let total: u64 = query(
        redis::cmd("LLEN").arg(key).query_async(conn),
        QUERY_TIMEOUT,
        "读取 List 长度失败",
    )
    .await
    .unwrap_or(0);
    let start = cursor;
    let stop = start.saturating_add(limit as u64).saturating_sub(1);

    let raw: Value = query(
        redis::cmd("LRANGE")
            .arg(key)
            .arg(start)
            .arg(stop)
            .query_async(conn),
        QUERY_TIMEOUT,
        "读取 List 失败",
    )
    .await?;

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
    let total: u64 = query(
        redis::cmd("SCARD").arg(key).query_async(conn),
        QUERY_TIMEOUT,
        "读取 Set 长度失败",
    )
    .await
    .unwrap_or(0);

    let raw: Value = query(
        redis::cmd("SSCAN")
            .arg(key)
            .arg(cursor)
            .arg("COUNT")
            .arg(limit)
            .query_async(conn),
        QUERY_TIMEOUT,
        "扫描 Set 失败",
    )
    .await?;

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
    let total: u64 = query(
        redis::cmd("ZCARD").arg(key).query_async(conn),
        QUERY_TIMEOUT,
        "读取 ZSet 长度失败",
    )
    .await
    .unwrap_or(0);
    let start = cursor;
    let stop = start.saturating_add(limit as u64).saturating_sub(1);

    let raw: Value = query(
        redis::cmd("ZRANGE")
            .arg(key)
            .arg(start)
            .arg(stop)
            .arg("WITHSCORES")
            .query_async(conn),
        QUERY_TIMEOUT,
        "读取 ZSet 失败",
    )
    .await?;

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
    // 写命令同样带超时，避免网络黑洞时界面一直停在保存中。
    let exists: i64 = query(
        redis::cmd("EXISTS").arg(&key).query_async(&mut conn),
        QUERY_TIMEOUT,
        "检查键是否存在失败",
    )
    .await?;
    if exists > 0 {
        return Err("键已存在，请换一个键名或直接编辑原键".to_string());
    }

    let key_type = key_type.to_ascii_lowercase();
    match key_type.as_str() {
        "string" => {
            query(
                redis::cmd("SET")
                    .arg(&key)
                    .arg(value.unwrap_or_default())
                    .query_async::<Value>(&mut conn),
                QUERY_TIMEOUT,
                "创建 String 失败",
            )
            .await?;
        }
        "hash" => {
            let field = clean_optional(field).ok_or_else(|| "Hash 字段不能为空".to_string())?;
            query(
                redis::cmd("HSET")
                    .arg(&key)
                    .arg(field)
                    .arg(value.unwrap_or_default())
                    .query_async::<Value>(&mut conn),
                QUERY_TIMEOUT,
                "创建 Hash 失败",
            )
            .await?;
        }
        "list" => {
            query(
                redis::cmd("RPUSH")
                    .arg(&key)
                    .arg(value.unwrap_or_default())
                    .query_async::<Value>(&mut conn),
                QUERY_TIMEOUT,
                "创建 List 失败",
            )
            .await?;
        }
        "set" => {
            query(
                redis::cmd("SADD")
                    .arg(&key)
                    .arg(value.unwrap_or_default())
                    .query_async::<Value>(&mut conn),
                QUERY_TIMEOUT,
                "创建 Set 失败",
            )
            .await?;
        }
        "zset" => {
            query(
                redis::cmd("ZADD")
                    .arg(&key)
                    .arg(score.unwrap_or(0.0))
                    .arg(value.unwrap_or_default())
                    .query_async::<Value>(&mut conn),
                QUERY_TIMEOUT,
                "创建 ZSet 失败",
            )
            .await?;
        }
        other => return Err(format!("暂不支持创建 {other} 类型")),
    }

    if let Some(ttl) = ttl_seconds {
        query(
            redis::cmd("EXPIRE")
                .arg(&key)
                .arg(ttl)
                .query_async::<Value>(&mut conn),
            QUERY_TIMEOUT,
            "键已创建，但设置过期时间失败",
        )
        .await?;
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
    query(
        redis::cmd("SET")
            .arg(&key)
            .arg(value)
            .query_async::<Value>(&mut conn),
        QUERY_TIMEOUT,
        "保存失败",
    )
    .await?;
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
    query(
        redis::cmd("HSET")
            .arg(&key)
            .arg(field)
            .arg(value)
            .query_async::<Value>(&mut conn),
        QUERY_TIMEOUT,
        "写入 Hash 失败",
    )
    .await?;
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
    query(
        redis::cmd("HDEL")
            .arg(&key)
            .arg(field)
            .query_async::<Value>(&mut conn),
        QUERY_TIMEOUT,
        "删除 Hash 字段失败",
    )
    .await?;
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
    query(
        redis::cmd(command)
            .arg(&key)
            .arg(value)
            .query_async::<Value>(&mut conn),
        QUERY_TIMEOUT,
        "写入 List 失败",
    )
    .await?;
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
    query(
        redis::cmd("LSET")
            .arg(&key)
            .arg(index)
            .arg(value)
            .query_async::<Value>(&mut conn),
        QUERY_TIMEOUT,
        "修改 List 元素失败",
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn list_remove_at(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
    index: i64,
) -> Result<u64, String> {
    let mut conn = state.writable_connection(&profile_id).await?;
    list_remove_at_inner(&mut conn, &key, index).await
}

async fn list_remove_at_inner(
    conn: &mut MultiplexedConnection,
    key: &str,
    index: i64,
) -> Result<u64, String> {
    // Redis 没有「按索引删除 List 元素」的命令。LREM 是按值删除，
    // 列表里存在重复元素时会删错行；这里先用一次性令牌替换目标位置，
    // 再按令牌删除，保证只影响这一行。
    // 先记下原值：万一 LREM 失败，尽量把令牌还原回原内容。
    let original: Option<Vec<u8>> = query(
        redis::cmd("LINDEX")
            .arg(key)
            .arg(index)
            .query_async(&mut *conn),
        QUERY_TIMEOUT,
        "读取 List 元素失败",
    )
    .await?;
    let Some(original) = original else {
        return Err("该元素已不存在，列表可能已被修改，请刷新后重试".to_string());
    };

    let token = format!("\u{0}leonredis-remove-{}", Uuid::new_v4());
    match query(
        redis::cmd("LSET")
            .arg(key)
            .arg(index)
            .arg(&token)
            .query_async::<Value>(&mut *conn),
        QUERY_TIMEOUT,
        "定位 List 元素失败",
    )
    .await
    {
        Ok(_) => {}
        Err(err) if err.contains("index out of range") => {
            return Err("该元素已不存在，列表可能已被修改，请刷新后重试".to_string());
        }
        Err(err) => return Err(err),
    }

    match query(
        redis::cmd("LREM")
            .arg(key)
            .arg(1)
            .arg(&token)
            .query_async(&mut *conn),
        QUERY_TIMEOUT,
        "删除 List 元素失败",
    )
    .await
    {
        Ok(removed) => Ok(removed),
        Err(err) => {
            // 尽力还原，避免把内部令牌遗留在用户的列表里。
            let _: Result<Value, String> = query(
                redis::cmd("LSET")
                    .arg(key)
                    .arg(index)
                    .arg(&original)
                    .query_async(&mut *conn),
                QUERY_TIMEOUT,
                "还原 List 元素失败",
            )
            .await;
            Err(err)
        }
    }
}

#[tauri::command]
pub async fn set_add(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
    member: String,
) -> Result<(), String> {
    let mut conn = state.writable_connection(&profile_id).await?;
    query(
        redis::cmd("SADD")
            .arg(&key)
            .arg(member)
            .query_async::<Value>(&mut conn),
        QUERY_TIMEOUT,
        "写入 Set 失败",
    )
    .await?;
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
    query(
        redis::cmd("SREM")
            .arg(&key)
            .arg(member)
            .query_async::<Value>(&mut conn),
        QUERY_TIMEOUT,
        "删除 Set 成员失败",
    )
    .await?;
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
    query(
        redis::cmd("ZADD")
            .arg(&key)
            .arg(score)
            .arg(member)
            .query_async::<Value>(&mut conn),
        QUERY_TIMEOUT,
        "写入 ZSet 失败",
    )
    .await?;
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
    query(
        redis::cmd("ZREM")
            .arg(&key)
            .arg(member)
            .query_async::<Value>(&mut conn),
        QUERY_TIMEOUT,
        "删除 ZSet 成员失败",
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn delete_key(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
) -> Result<u64, String> {
    let mut conn = state.writable_connection(&profile_id).await?;
    // 优先用 UNLINK（Redis 4.0+）：大集合在后台线程释放，不会阻塞主线程。
    let linked: Result<u64, String> = query(
        redis::cmd("UNLINK").arg(&key).query_async(&mut conn),
        QUERY_TIMEOUT,
        "删除键失败",
    )
    .await;
    if let Ok(removed) = linked {
        return Ok(removed);
    }

    let deleted: u64 = query(
        redis::cmd("DEL").arg(&key).query_async(&mut conn),
        QUERY_TIMEOUT,
        "删除键失败",
    )
    .await?;
    Ok(deleted)
}

#[tauri::command]
pub async fn update_key(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
    new_key: String,
    ttl_seconds: Option<i64>,
    overwrite: Option<bool>,
) -> Result<String, String> {
    let mut conn = state.writable_connection(&profile_id).await?;
    update_key_inner(
        &mut conn,
        &key,
        &new_key,
        ttl_seconds,
        overwrite.unwrap_or(false),
    )
    .await
}

async fn update_key_inner(
    conn: &mut MultiplexedConnection,
    key: &str,
    new_key: &str,
    ttl_seconds: Option<i64>,
    overwrite: bool,
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

    let exists: i64 = query(
        redis::cmd("EXISTS").arg(key).query_async(&mut *conn),
        QUERY_TIMEOUT,
        "检查键是否存在失败",
    )
    .await?;
    if exists == 0 {
        return Err("键已经不存在，可能已被删除或过期".to_string());
    }

    if key != target {
        // RENAME 会直接覆盖同名键，先拦住让用户确认，避免静默删掉另一个键。
        if !overwrite {
            let target_exists: i64 = query(
                redis::cmd("EXISTS").arg(&target).query_async(&mut *conn),
                QUERY_TIMEOUT,
                "检查目标键是否存在失败",
            )
            .await?;
            if target_exists > 0 {
                return Err(TARGET_EXISTS_MESSAGE.to_string());
            }
        }
        query(
            redis::cmd("RENAME")
                .arg(key)
                .arg(&target)
                .query_async::<Value>(&mut *conn),
            QUERY_TIMEOUT,
            "重命名失败",
        )
        .await?;
    }

    if let Some(ttl) = ttl_seconds {
        let changed: i64 = query(
            redis::cmd("EXPIRE")
                .arg(&target)
                .arg(ttl)
                .query_async(&mut *conn),
            QUERY_TIMEOUT,
            "设置过期时间失败",
        )
        .await?;
        if changed == 0 {
            return Err("键已更新，但设置过期时间失败".to_string());
        }
    } else {
        query(
            redis::cmd("PERSIST")
                .arg(&target)
                .query_async::<Value>(&mut *conn),
            QUERY_TIMEOUT,
            "移除过期时间失败",
        )
        .await?;
    }

    Ok(target)
}

/// 会话内禁止的命令：会阻塞连接或影响服务器稳定性。
const BLOCKED_COMMANDS: &[&str] = &[
    "SUBSCRIBE",
    "PSUBSCRIBE",
    "SSUBSCRIBE",
    "MONITOR",
    "BLPOP",
    "BRPOP",
    "BLMOVE",
    "BRPOPLPUSH",
    "BLMPOP",
    "BZPOPMIN",
    "BZPOPMAX",
    "BZMPOP",
    "WAIT",
    "SHUTDOWN",
    "DEBUG",
    "SLAVEOF",
    "REPLICAOF",
    "MIGRATE",
    "CLIENT",
];

/// 把命令行拆成参数，支持单引号/双引号包裹带空格的参数。
fn parse_command_line(line: &str) -> Result<Vec<String>, String> {
    let mut args: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut has_token = false;
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        match quote {
            Some(mark) => {
                if ch == mark {
                    quote = None;
                } else if ch == '\\' && mark == '"' {
                    if let Some(next) = chars.next() {
                        current.push(next);
                    }
                } else {
                    current.push(ch);
                }
            }
            None => {
                if ch == '"' || ch == '\'' {
                    quote = Some(ch);
                    has_token = true;
                } else if ch.is_whitespace() {
                    if has_token {
                        args.push(std::mem::take(&mut current));
                        has_token = false;
                    }
                } else {
                    current.push(ch);
                    has_token = true;
                }
            }
        }
    }

    if quote.is_some() {
        return Err("引号没有闭合".to_string());
    }
    if has_token {
        args.push(current);
    }
    Ok(args)
}

fn describe_bytes(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text)
            if !text
                .chars()
                .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t')) =>
        {
            if text.is_empty() {
                "(empty string)".to_string()
            } else {
                text.to_string()
            }
        }
        _ => format!("(binary, {} bytes) {}", bytes.len(), hex_preview(bytes)),
    }
}

fn hex_preview(bytes: &[u8]) -> String {
    let shown: Vec<String> = bytes
        .iter()
        .take(32)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let mut output = shown.join(" ");
    if bytes.len() > 32 {
        output.push_str(" …");
    }
    output
}

fn format_reply(value: &Value) -> String {
    match value {
        Value::Nil => "(nil)".to_string(),
        Value::Int(number) => number.to_string(),
        Value::Double(number) => number.to_string(),
        Value::Boolean(flag) => flag.to_string(),
        Value::Okay => "OK".to_string(),
        Value::SimpleString(text) => text.clone(),
        Value::BulkString(bytes) => describe_bytes(bytes),
        Value::VerbatimString { text, .. } => text.clone(),
        Value::BigNumber(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        Value::Array(items) => format_reply_list(items),
        Value::Set(items) => format_reply_list(items),
        Value::Map(pairs) => {
            if pairs.is_empty() {
                return "(empty map)".to_string();
            }
            pairs
                .iter()
                .enumerate()
                .map(|(index, (key, value))| {
                    format!(
                        "{}) {} => {}",
                        index + 1,
                        format_reply(key),
                        format_reply(value)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        Value::Attribute { data, .. } => format_reply(data),
        _ => "(不支持的返回类型)".to_string(),
    }
}

fn format_reply_list(items: &[Value]) -> String {
    if items.is_empty() {
        return "(empty array)".to_string();
    }
    items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let text = format_reply(item);
            let mut lines = text.lines();
            let mut output = format!("{}) {}", index + 1, lines.next().unwrap_or(""));
            for line in lines {
                output.push_str("\n   ");
                output.push_str(line);
            }
            output
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 命令行面板：直接执行用户输入的 Redis 命令，返回格式化后的回复。
///
/// 只读模式下整条通道都不可用；会阻塞连接或影响服务器稳定性的命令被拒绝。
#[tauri::command]
pub async fn run_command(
    state: State<'_, AppState>,
    profile_id: String,
    command: String,
) -> Result<CommandResult, String> {
    let mut conn = state
        .writable_connection(&profile_id)
        .await
        .map_err(|err| {
            if err.contains("只读模式") {
                "只读模式已开启，命令行不可用".to_string()
            } else {
                err
            }
        })?;
    run_command_inner(&mut conn, &command).await
}

async fn run_command_inner(
    conn: &mut MultiplexedConnection,
    command: &str,
) -> Result<CommandResult, String> {
    let args = parse_command_line(command.trim())?;
    if args.is_empty() {
        return Err("请输入要执行的命令".to_string());
    }
    let name = args[0].to_ascii_uppercase();
    if BLOCKED_COMMANDS.contains(&name.as_str()) {
        return Err(format!(
            "{name} 会阻塞连接或影响服务器稳定性，命令行中已禁用"
        ));
    }

    let mut cmd = redis::cmd(&args[0]);
    for arg in &args[1..] {
        cmd.arg(arg);
    }

    let started = std::time::Instant::now();
    let reply: Value = query(cmd.query_async(&mut *conn), QUERY_TIMEOUT, "执行命令失败").await?;
    Ok(CommandResult {
        output: format_reply(&reply),
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

/// 读取 String 键的原始字节，用于十六进制查看（避免 UTF-8 有损转换）。
#[tauri::command]
pub async fn get_key_bytes(
    state: State<'_, AppState>,
    profile_id: String,
    key: String,
    offset: Option<u64>,
    limit: Option<u32>,
) -> Result<KeyBytes, String> {
    let mut conn = state.connection(&profile_id).await?;
    read_key_bytes_inner(&mut conn, &key, offset.unwrap_or(0), limit).await
}

async fn read_key_bytes_inner(
    conn: &mut MultiplexedConnection,
    key: &str,
    offset: u64,
    limit: Option<u32>,
) -> Result<KeyBytes, String> {
    let limit = limit.unwrap_or(4096).clamp(16, 16384) as u64;

    let total: u64 = query(
        redis::cmd("STRLEN").arg(key).query_async(&mut *conn),
        QUERY_TIMEOUT,
        "读取字符串长度失败",
    )
    .await
    .unwrap_or(0);

    let start = offset.min(total.saturating_sub(1));
    let end = start.saturating_add(limit).saturating_sub(1);
    let raw: Value = query(
        redis::cmd("GETRANGE")
            .arg(key)
            .arg(start)
            .arg(end)
            .query_async(&mut *conn),
        QUERY_TIMEOUT,
        "读取字符串字节失败",
    )
    .await?;

    let bytes = match raw {
        Value::BulkString(bytes) => bytes,
        _ => Vec::new(),
    };

    Ok(KeyBytes {
        base64: STANDARD.encode(&bytes),
        total,
        offset: start,
        truncated: end.saturating_add(1) < total,
    })
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

async fn configured_databases(conn: &mut MultiplexedConnection) -> u32 {
    let raw: Value = query(
        redis::cmd("CONFIG")
            .arg("GET")
            .arg("databases")
            .query_async(conn),
        QUERY_TIMEOUT,
        "读取数据库数量失败",
    )
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
    let raw: String = query(
        redis::cmd("INFO").arg("memory").query_async(conn),
        QUERY_TIMEOUT,
        "读取内存信息失败",
    )
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
    let raw: Value = query(
        redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg("*")
            .arg("COUNT")
            .arg(500)
            .query_async(conn),
        SCAN_TIMEOUT,
        "扫描数据库失败",
    )
    .await?;
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
    fn splits_command_line_with_quotes() {
        assert_eq!(
            parse_command_line("GET user:1:name").expect("parse"),
            vec!["GET".to_string(), "user:1:name".to_string()]
        );
        assert_eq!(
            parse_command_line("SET greeting \"hello world\"").expect("parse"),
            vec![
                "SET".to_string(),
                "greeting".to_string(),
                "hello world".to_string()
            ]
        );
        assert_eq!(
            parse_command_line("SET key 'a b'").expect("parse"),
            vec!["SET".to_string(), "key".to_string(), "a b".to_string()]
        );
        assert!(parse_command_line("SET key \"unclosed").is_err());
        assert!(parse_command_line("   ").expect("parse").is_empty());
    }

    #[test]
    fn formats_replies_for_the_console() {
        assert_eq!(format_reply(&Value::Int(7)), "7");
        assert_eq!(format_reply(&Value::Nil), "(nil)");
        assert_eq!(format_reply(&Value::Okay), "OK");
        assert_eq!(
            format_reply(&Value::Array(vec![
                Value::BulkString(b"a".to_vec()),
                Value::BulkString(b"b".to_vec()),
            ])),
            "1) a\n2) b"
        );
        assert!(format_reply(&Value::BulkString(vec![0, 1, 255])).contains("binary"));
    }

    #[test]
    fn reads_array_reply() {
        let value = Value::Array(vec![Value::BulkString(b"a".to_vec())]);
        assert_eq!(as_array(&value).len(), 1);
        assert!(as_array(&Value::Nil).is_empty());
    }
}

/// 需要本地 Redis（db9）的集成测试；连不上时自动跳过，
/// 只使用 `__leonreview:` 前缀的键并在结束时清理。
#[cfg(test)]
mod live_tests {
    use super::*;

    /// live 测试共用本地 db9，串行执行避免互相删除对方的键。
    static LIVE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    async fn test_connection() -> Option<MultiplexedConnection> {
        let client = redis::Client::open("redis://127.0.0.1:6379/9").ok()?;
        let mut conn = client.get_multiplexed_async_connection().await.ok()?;
        sweep_test_keys(&mut conn).await;
        Some(conn)
    }

    fn unique_key(label: &str) -> String {
        format!("__leonreview:{label}:{}", Uuid::new_v4())
    }

    async fn select_db9(conn: &mut MultiplexedConnection) {
        let _: Result<Value, _> = redis::cmd("SELECT").arg(9).query_async(&mut *conn).await;
    }

    /// 清掉历史遗留的测试键，避免测试中断后越积越多。
    async fn sweep_test_keys(conn: &mut MultiplexedConnection) {
        select_db9(conn).await;
        let mut cursor = 0u64;
        loop {
            let raw: Result<Value, _> = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg("__leonreview*")
                .arg("COUNT")
                .arg(200)
                .query_async(&mut *conn)
                .await;
            let Ok(raw) = raw else { return };
            let parts = as_array(&raw);
            cursor = parts.first().and_then(value_as_u64).unwrap_or(0);
            let keys: Vec<Vec<u8>> = parts
                .get(1)
                .map(as_array)
                .unwrap_or(&[])
                .iter()
                .filter_map(value_as_bytes)
                .collect();
            for key in keys {
                let _: Result<Value, _> = redis::cmd("DEL").arg(key).query_async(&mut *conn).await;
            }
            if cursor == 0 {
                break;
            }
        }
    }

    async fn cleanup(conn: &mut MultiplexedConnection, keys: &[String]) {
        select_db9(conn).await;
        for key in keys {
            let _: Result<Value, _> = redis::cmd("DEL").arg(key).query_async(&mut *conn).await;
        }
    }

    #[tokio::test]
    async fn list_remove_at_deletes_the_requested_index() {
        let _guard = LIVE_LOCK.lock().await;
        let Some(mut conn) = test_connection().await else {
            return;
        };
        let key = unique_key("list");
        let _: Value = redis::cmd("RPUSH")
            .arg(&key)
            .arg("a")
            .arg("b")
            .arg("a")
            .query_async(&mut conn)
            .await
            .expect("rpush");

        // 重复元素：删除下标 2 的 "a"，下标 0 的 "a" 必须保留。
        let removed = list_remove_at_inner(&mut conn, &key, 2)
            .await
            .expect("remove index 2");
        assert_eq!(removed, 1);
        let left: Vec<String> = redis::cmd("LRANGE")
            .arg(&key)
            .arg(0)
            .arg(-1)
            .query_async(&mut conn)
            .await
            .expect("lrange");
        assert_eq!(left, vec!["a".to_string(), "b".to_string()]);

        let err = list_remove_at_inner(&mut conn, &key, 9)
            .await
            .expect_err("out of range");
        assert!(err.contains("已不存在"), "{err}");

        cleanup(&mut conn, &[key]).await;
    }

    #[tokio::test]
    async fn client_side_type_filter_keeps_scanning_until_it_finds_a_match() {
        let _guard = LIVE_LOCK.lock().await;
        let Some(mut conn) = test_connection().await else {
            return;
        };
        // Redis 5 不支持 SCAN ... TYPE，构造「大量 string + 1 个 zset」，
        // 首屏很可能全是 string，验证会继续推进游标直到找到 zset。
        let prefix = format!("__leonreview:filter:{}:", Uuid::new_v4());
        let mut keys = Vec::new();
        for index in 0..24 {
            let key = format!("{prefix}string:{index}");
            let _: Value = redis::cmd("SET")
                .arg(&key)
                .arg("v")
                .query_async(&mut conn)
                .await
                .expect("set");
            keys.push(key);
        }
        let target = format!("{prefix}zset");
        let _: Value = redis::cmd("ZADD")
            .arg(&target)
            .arg(1)
            .arg("member")
            .query_async(&mut conn)
            .await
            .expect("zadd");
        keys.push(target.clone());

        let raw = scan_pages_by_type(&mut conn, 0, &format!("{prefix}*"), 5, "zset")
            .await
            .expect("client side filter");
        let reply = as_array(&raw);
        let found: Vec<String> = reply
            .get(1)
            .map(as_array)
            .unwrap_or(&[])
            .iter()
            .filter_map(value_as_string)
            .collect();
        assert_eq!(found, vec![target]);

        cleanup(&mut conn, &keys).await;
    }

    #[tokio::test]
    async fn runs_console_commands_and_blocks_dangerous_ones() {
        let _guard = LIVE_LOCK.lock().await;
        let Some(mut conn) = test_connection().await else {
            return;
        };
        let key = unique_key("console");
        let _: Value = redis::cmd("SET")
            .arg(&key)
            .arg("console-value")
            .query_async(&mut conn)
            .await
            .expect("set");

        let get = run_command_inner(&mut conn, &format!("GET {key}"))
            .await
            .expect("get");
        assert_eq!(get.output, "console-value");

        let set = run_command_inner(&mut conn, &format!("SET {key} \"two words\""))
            .await
            .expect("set via console");
        assert_eq!(set.output, "OK");
        let value: String = redis::cmd("GET")
            .arg(&key)
            .query_async(&mut conn)
            .await
            .expect("get value");
        assert_eq!(value, "two words");

        let blocked = run_command_inner(&mut conn, "SUBSCRIBE channel")
            .await
            .expect_err("subscribe blocked");
        assert!(blocked.contains("已禁用"), "{blocked}");

        let missing = run_command_inner(&mut conn, &format!("GET {key}:missing"))
            .await
            .expect("missing key");
        assert_eq!(missing.output, "(nil)");

        cleanup(&mut conn, &[key]).await;
    }

    #[tokio::test]
    async fn reads_raw_bytes_for_hex_view() {
        let _guard = LIVE_LOCK.lock().await;
        let Some(mut conn) = test_connection().await else {
            return;
        };
        let key = unique_key("bytes");
        // 含 NUL 与 0xFF 的非 UTF-8 内容，验证不会被有损转换。
        let payload: Vec<u8> = (0u8..40).map(|byte| byte.wrapping_mul(3)).collect();
        let _: Value = redis::cmd("SET")
            .arg(&key)
            .arg(&payload[..])
            .query_async(&mut conn)
            .await
            .expect("set binary");

        let bytes = read_key_bytes_inner(&mut conn, &key, 0, None)
            .await
            .expect("read bytes");
        assert_eq!(bytes.total, payload.len() as u64);
        assert!(!bytes.truncated);
        assert_eq!(STANDARD.decode(&bytes.base64).expect("base64"), payload);

        // 窗口读取：跳过前 8 字节，取 16 字节，后面还有内容因此标记为截断。
        let window = read_key_bytes_inner(&mut conn, &key, 8, Some(16))
            .await
            .expect("read window");
        assert_eq!(window.offset, 8);
        assert_eq!(
            STANDARD.decode(&window.base64).expect("base64"),
            payload[8..24].to_vec()
        );
        assert!(window.truncated);

        cleanup(&mut conn, &[key]).await;
    }

    #[tokio::test]
    async fn reads_large_string_without_loading_everything() {
        let _guard = LIVE_LOCK.lock().await;
        let Some(mut conn) = test_connection().await else {
            return;
        };
        let key = unique_key("string");
        let big = "x".repeat(MAX_STRING_BYTES + 4096);
        let _: Value = redis::cmd("SET")
            .arg(&key)
            .arg(&big)
            .query_async(&mut conn)
            .await
            .expect("set");

        let value = read_string(&mut conn, &key).await.expect("read big string");
        assert_eq!(value.total, big.len() as u64);
        assert!(value.truncated);
        assert_eq!(value.value.len(), MAX_STRING_BYTES);

        let empty_key = unique_key("empty");
        let _: Value = redis::cmd("SET")
            .arg(&empty_key)
            .arg("")
            .query_async(&mut conn)
            .await
            .expect("set empty");
        let empty = read_string(&mut conn, &empty_key)
            .await
            .expect("read empty string");
        assert_eq!(empty.total, 0);
        assert!(empty.message.is_none(), "空字符串不应提示键不存在");

        let missing = read_string(&mut conn, &unique_key("missing"))
            .await
            .expect("read missing");
        assert!(missing.message.is_some());

        cleanup(&mut conn, &[key, empty_key]).await;
    }

    #[tokio::test]
    async fn export_streams_valid_json_and_import_restores_keys() {
        let _guard = LIVE_LOCK.lock().await;
        let Some(mut conn) = test_connection().await else {
            return;
        };
        let key = unique_key("export-source");
        let _: Value = redis::cmd("SET")
            .arg(&key)
            .arg("hello-leon")
            .query_async(&mut conn)
            .await
            .expect("set");

        let dir = std::env::temp_dir().join(format!("leonredis-export-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("export.json");
        let path_str = path.to_string_lossy().to_string();

        let result = export_data_inner(&mut conn, &path_str)
            .await
            .expect("export");
        assert!(result.keys >= 1);
        assert!(result.bytes > 0);

        // 流式写出的文件必须是合法 JSON。
        let raw = std::fs::read_to_string(&path).expect("read export file");
        let document: ExportDocument = serde_json::from_str(&raw).expect("valid json");
        assert_eq!(document.format, "leonredis-export");
        assert_eq!(document.version, 1);
        let db9 = document
            .databases
            .iter()
            .find(|db| db.db == 9)
            .expect("db9 exported");
        let encoded = STANDARD.encode(key.as_bytes());
        assert!(db9.keys.iter().any(|entry| entry.key == encoded));

        // 只把 db9 的数据导回去，避免影响开发机上其他数据库。
        select_db9(&mut conn).await;
        let _: Value = redis::cmd("DEL")
            .arg(&key)
            .query_async(&mut conn)
            .await
            .expect("del");
        let subset = ExportDocument {
            format: document.format.clone(),
            version: document.version,
            exported_at: document.exported_at,
            databases: vec![ExportDatabase {
                db: 9,
                keys: db9.keys.clone(),
            }],
        };
        let imported = import_data_inner(&mut conn, &path_str, &subset)
            .await
            .expect("import");
        assert_eq!(imported.keys as usize, db9.keys.len());

        let restored: Option<String> = redis::cmd("GET")
            .arg(&key)
            .query_async(&mut conn)
            .await
            .expect("get restored");
        assert_eq!(restored.as_deref(), Some("hello-leon"));

        let _ = std::fs::remove_dir_all(&dir);
        cleanup(&mut conn, &[key]).await;
    }

    #[tokio::test]
    async fn update_key_refuses_to_overwrite_until_confirmed() {
        let _guard = LIVE_LOCK.lock().await;
        let Some(mut conn) = test_connection().await else {
            return;
        };
        let source = unique_key("rename-source");
        let target = unique_key("rename-target");
        let _: Value = redis::cmd("SET")
            .arg(&source)
            .arg("1")
            .query_async(&mut conn)
            .await
            .expect("set source");
        let _: Value = redis::cmd("SET")
            .arg(&target)
            .arg("2")
            .query_async(&mut conn)
            .await
            .expect("set target");

        let err = update_key_inner(&mut conn, &source, &target, None, false)
            .await
            .expect_err("should refuse overwrite");
        assert_eq!(err, TARGET_EXISTS_MESSAGE);
        let untouched: Option<String> = redis::cmd("GET")
            .arg(&source)
            .query_async(&mut conn)
            .await
            .expect("get source");
        assert_eq!(untouched.as_deref(), Some("1"), "未确认时源键必须保留");

        let renamed = update_key_inner(&mut conn, &source, &target, None, true)
            .await
            .expect("rename with overwrite");
        assert_eq!(renamed, target);
        let overwritten: Option<String> = redis::cmd("GET")
            .arg(&target)
            .query_async(&mut conn)
            .await
            .expect("get target");
        assert_eq!(overwritten.as_deref(), Some("1"));

        cleanup(&mut conn, &[target]).await;
    }
}
