use std::collections::HashMap;
use std::time::{Duration, Instant};

use redis::aio::MultiplexedConnection;

use crate::models::{ConnectionProfile, ConnectionSnapshot, DbInfo, ServerInfo};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(6);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

fn connection_url(profile: &ConnectionProfile, password: Option<&str>) -> Result<String, String> {
    let host = if profile.host.contains(':') && !profile.host.starts_with('[') {
        format!("[{}]", profile.host)
    } else {
        profile.host.clone()
    };

    let mut url = url::Url::parse(&format!("redis://{}:{}", host, profile.port))
        .map_err(|err| format!("连接地址格式无效: {err}"))?;

    let username = profile
        .username
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let password = password.map(str::trim).filter(|value| !value.is_empty());

    if username.is_some() || password.is_some() {
        url.set_username(username.unwrap_or(""))
            .map_err(|_| "用户名包含非法字符".to_string())?;
        if let Some(password) = password {
            url.set_password(Some(password))
                .map_err(|_| "密码包含非法字符".to_string())?;
        }
    }

    url.set_path(&format!("/{}", profile.db));
    Ok(url.to_string())
}

async fn with_timeout<T, F>(duration: Duration, future: F, message: &str) -> Result<T, String>
where
    F: std::future::Future<Output = redis::RedisResult<T>>,
{
    match tokio::time::timeout(duration, future).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(err)) => Err(format!("{message}: {err}")),
        Err(_) => Err(message.to_string()),
    }
}

pub async fn open(
    profile: &ConnectionProfile,
    password: Option<String>,
) -> Result<(MultiplexedConnection, u64), String> {
    let url = connection_url(profile, password.as_deref())?;
    let client = redis::Client::open(url).map_err(|err| format!("连接参数无效: {err}"))?;

    let started = Instant::now();
    let mut conn = with_timeout(
        CONNECT_TIMEOUT,
        client.get_multiplexed_async_connection(),
        "连接 Redis 超时",
    )
    .await?;

    let pong: String = with_timeout(
        COMMAND_TIMEOUT,
        redis::cmd("PING").query_async(&mut conn),
        "PING 超时",
    )
    .await?;

    if pong != "PONG" {
        return Err(format!("Redis 返回了非预期结果: {pong}"));
    }

    Ok((conn, started.elapsed().as_millis() as u64))
}

pub async fn snapshot(
    profile: &ConnectionProfile,
    conn: &mut MultiplexedConnection,
    latency_ms: u64,
) -> Result<ConnectionSnapshot, String> {
    let info = info_map(conn).await;
    let db_size: u64 = with_timeout(
        COMMAND_TIMEOUT,
        redis::cmd("DBSIZE").query_async(conn),
        "读取 DBSIZE 超时",
    )
    .await
    .unwrap_or(0);

    Ok(ConnectionSnapshot {
        profile_id: profile.id.clone(),
        name: profile.name.clone(),
        host: profile.host.clone(),
        port: profile.port,
        db: profile.db,
        version: info
            .get("redis_version")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string()),
        mode: info
            .get("redis_mode")
            .cloned()
            .unwrap_or_else(|| "standalone".to_string()),
        role: info
            .get("role")
            .cloned()
            .unwrap_or_else(|| "master".to_string()),
        db_size,
        latency_ms,
    })
}

pub async fn server_info(conn: &mut MultiplexedConnection) -> Result<ServerInfo, String> {
    let info = info_map(conn).await;
    let db_size: u64 = with_timeout(
        COMMAND_TIMEOUT,
        redis::cmd("DBSIZE").query_async(conn),
        "读取 DBSIZE 超时",
    )
    .await
    .unwrap_or(0);

    let mut keyspace = Vec::new();
    for (key, value) in &info {
        if !key.starts_with("db") {
            continue;
        }
        let Ok(db) = key[2..].parse::<u32>() else {
            continue;
        };
        let mut keys = 0_u64;
        let mut expires = 0_u64;
        for part in value.split(',') {
            if let Some((name, raw)) = part.split_once('=') {
                if name == "keys" {
                    keys = raw.parse().unwrap_or(0);
                } else if name == "expires" {
                    expires = raw.parse().unwrap_or(0);
                }
            }
        }
        keyspace.push(DbInfo { db, keys, expires });
    }
    keyspace.sort_by_key(|item| item.db);

    Ok(ServerInfo {
        version: info
            .get("redis_version")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string()),
        mode: info
            .get("redis_mode")
            .cloned()
            .unwrap_or_else(|| "standalone".to_string()),
        role: info
            .get("role")
            .cloned()
            .unwrap_or_else(|| "master".to_string()),
        os: info
            .get("os")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string()),
        arch_bits: parse_u64(info.get("arch_bits")),
        connected_clients: parse_u64(info.get("connected_clients")),
        blocked_clients: parse_u64(info.get("blocked_clients")),
        used_memory: parse_u64(info.get("used_memory")),
        used_memory_human: info
            .get("used_memory_human")
            .cloned()
            .unwrap_or_else(|| "-".to_string()),
        maxmemory: parse_u64(info.get("maxmemory")),
        maxmemory_human: info
            .get("maxmemory_human")
            .cloned()
            .unwrap_or_else(|| "0B".to_string()),
        total_commands_processed: parse_u64(info.get("total_commands_processed")),
        instantaneous_ops_per_sec: parse_u64(info.get("instantaneous_ops_per_sec")),
        keyspace_hits: parse_u64(info.get("keyspace_hits")),
        keyspace_misses: parse_u64(info.get("keyspace_misses")),
        used_cpu_sys: parse_f64(info.get("used_cpu_sys")),
        used_cpu_user: parse_f64(info.get("used_cpu_user")),
        uptime_in_seconds: parse_u64(info.get("uptime_in_seconds")),
        db_size,
        keyspace,
    })
}

fn parse_u64(value: Option<&String>) -> u64 {
    value.and_then(|value| value.parse().ok()).unwrap_or(0)
}

fn parse_f64(value: Option<&String>) -> f64 {
    value.and_then(|value| value.parse().ok()).unwrap_or(0.0)
}

async fn info_map(conn: &mut MultiplexedConnection) -> HashMap<String, String> {
    let raw: String = match with_timeout(
        COMMAND_TIMEOUT,
        redis::cmd("INFO").query_async(conn),
        "读取 INFO 超时",
    )
    .await
    {
        Ok(raw) => raw,
        Err(_) => return HashMap::new(),
    };

    let mut map = HashMap::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            map.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    map
}
