use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use redis::aio::MultiplexedConnection;
use tokio::sync::{OwnedMutexGuard, RwLock};

use crate::models::ConnectionProfile;
use crate::redis_manager;
use crate::storage;

pub struct RedisConnection {
    pub conn: MultiplexedConnection,
    pub read_only: AtomicBool,
    pub db: u32,
    pub session_password: Option<String>,
}

/// 复用的辅助连接，避免性能分析/导入导出每次调用都新建一条 TCP 连接。
///
/// 辅助连接会通过 `SELECT` 遍历所有数据库，因此必须独占使用：
/// `OwnedMutexGuard` 保证同一 profile 的辅助操作串行执行。
pub struct AuxConnection {
    guard: OwnedMutexGuard<MultiplexedConnection>,
}

impl Deref for AuxConnection {
    type Target = MultiplexedConnection;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl DerefMut for AuxConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

pub struct AppState {
    pub profiles: Mutex<Vec<ConnectionProfile>>,
    pub store_path: PathBuf,
    store_warning: Mutex<Option<String>>,
    store_blocked: AtomicBool,
    pub connections: RwLock<HashMap<String, RedisConnection>>,
    auxiliary: RwLock<HashMap<String, Arc<tokio::sync::Mutex<MultiplexedConnection>>>>,
    scan_tokens: RwLock<HashMap<String, Arc<AtomicBool>>>,
}

impl AppState {
    pub fn new(store_path: PathBuf) -> Self {
        let loaded = storage::load_profiles(&store_path);
        Self {
            profiles: Mutex::new(loaded.profiles),
            store_path,
            store_warning: Mutex::new(loaded.warning),
            store_blocked: AtomicBool::new(loaded.blocked),
            connections: RwLock::new(HashMap::new()),
            auxiliary: RwLock::new(HashMap::new()),
            scan_tokens: RwLock::new(HashMap::new()),
        }
    }

    pub fn profiles_snapshot(&self) -> Vec<ConnectionProfile> {
        self.profiles
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// 配置损坏且无法备份时禁止写入，避免把用户的原文件覆盖成空列表。
    pub fn ensure_writable(&self) -> Result<(), String> {
        if self.store_blocked.load(Ordering::Relaxed) {
            return Err(
                "连接配置读取失败且无法备份，已暂停写入。请修复或删除 data\\connections.json 后重启应用。"
                    .to_string(),
            );
        }
        Ok(())
    }

    pub fn take_store_warning(&self) -> Option<String> {
        self.store_warning
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }

    pub async fn connection(&self, profile_id: &str) -> Result<MultiplexedConnection, String> {
        self.connections
            .read()
            .await
            .get(profile_id)
            .map(|entry| entry.conn.clone())
            .ok_or_else(|| "连接已断开，请重新连接".to_string())
    }

    /// 写操作必须经过这里，确保只读模式不能只靠前端按钮绕过。
    pub async fn writable_connection(
        &self,
        profile_id: &str,
    ) -> Result<MultiplexedConnection, String> {
        let connections = self.connections.read().await;
        let entry = connections
            .get(profile_id)
            .ok_or_else(|| "连接已断开，请重新连接".to_string())?;
        if entry.read_only.load(Ordering::Relaxed) {
            return Err("只读模式已开启，后端已拦截写操作".to_string());
        }
        Ok(entry.conn.clone())
    }

    pub async fn set_read_only(&self, profile_id: &str, read_only: bool) -> Result<(), String> {
        let connections = self.connections.read().await;
        let entry = connections
            .get(profile_id)
            .ok_or_else(|| "连接已断开，请重新连接".to_string())?;
        entry.read_only.store(read_only, Ordering::Relaxed);
        Ok(())
    }

    /// 连接级别凭证：当前库、会话密码，用于新建辅助连接和重连。
    async fn connection_credentials(
        &self,
        profile_id: &str,
    ) -> Result<(ConnectionProfile, Option<String>), String> {
        let mut profile = {
            let profiles = self
                .profiles
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            profiles
                .iter()
                .find(|profile| profile.id == profile_id)
                .cloned()
                .ok_or_else(|| "连接配置不存在".to_string())?
        };

        let (password, db) = {
            let connections = self.connections.read().await;
            let entry = connections
                .get(profile_id)
                .ok_or_else(|| "连接已断开，请重新连接".to_string())?;
            (entry.session_password.clone(), entry.db)
        };

        profile.db = db;
        Ok((profile, password))
    }

    /// 复用辅助连接；不存在或已失效时新建一条。
    pub async fn auxiliary_connection(&self, profile_id: &str) -> Result<AuxConnection, String> {
        let cached = self.auxiliary.read().await.get(profile_id).cloned();

        let handle = match cached {
            Some(handle) => handle,
            None => {
                let (profile, password) = self.connection_credentials(profile_id).await?;
                let (conn, _) = redis_manager::open(&profile, password).await?;
                let fresh = Arc::new(tokio::sync::Mutex::new(conn));
                // 并发首次调用时只保留先写入的那一条，多余连接被丢弃。
                self.auxiliary
                    .write()
                    .await
                    .entry(profile_id.to_string())
                    .or_insert_with(|| fresh.clone())
                    .clone()
            }
        };

        let guard = handle.lock_owned().await;
        Ok(AuxConnection { guard })
    }

    /// 丢弃辅助连接，让下一次调用重新建立（连接失效或断开时使用）。
    pub async fn drop_auxiliary(&self, profile_id: &str) {
        self.auxiliary.write().await.remove(profile_id);
    }

    /// 开始一次深度扫描前取一个取消令牌。
    pub async fn begin_scan(&self, profile_id: &str) -> Arc<AtomicBool> {
        let mut map = self.scan_tokens.write().await;
        let token = map
            .entry(profile_id.to_string())
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone();
        token.store(false, Ordering::Relaxed);
        token
    }

    pub async fn cancel_scan(&self, profile_id: &str) {
        let map = self.scan_tokens.read().await;
        if let Some(token) = map.get(profile_id) {
            token.store(true, Ordering::Relaxed);
        }
    }

    /// 断开连接时清理辅助连接与扫描令牌。
    pub async fn cleanup(&self, profile_id: &str) {
        self.auxiliary.write().await.remove(profile_id);
        self.scan_tokens.write().await.remove(profile_id);
    }

    /// 连接失效后就地重建，保留当前库、只读状态和会话密码。
    pub async fn reconnect(&self, profile_id: &str) -> Result<(), String> {
        let (profile, password) = self.connection_credentials(profile_id).await?;
        let (conn, _) = redis_manager::open(&profile, password.clone()).await?;

        // 旧连接可能仍然活着但已不可用，先把辅助连接丢掉。
        self.drop_auxiliary(profile_id).await;

        let mut connections = self.connections.write().await;
        let entry = connections
            .get_mut(profile_id)
            .ok_or_else(|| "连接已断开，请重新连接".to_string())?;
        entry.conn = conn;
        entry.db = profile.db;
        entry.session_password = password;
        Ok(())
    }
}
