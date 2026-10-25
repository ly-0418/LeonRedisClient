use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use redis::aio::MultiplexedConnection;
use tokio::sync::RwLock;

use crate::models::ConnectionProfile;
use crate::storage;

pub struct RedisConnection {
    pub conn: MultiplexedConnection,
    pub read_only: AtomicBool,
    pub db: u32,
    pub session_password: Option<String>,
}

pub struct AppState {
    pub profiles: Mutex<Vec<ConnectionProfile>>,
    pub store_path: PathBuf,
    pub connections: RwLock<HashMap<String, RedisConnection>>,
}

impl AppState {
    pub fn new(store_path: PathBuf) -> Self {
        let profiles = storage::load_profiles(&store_path);
        Self {
            profiles: Mutex::new(profiles),
            store_path,
            connections: RwLock::new(HashMap::new()),
        }
    }

    pub fn profiles_snapshot(&self) -> Vec<ConnectionProfile> {
        self.profiles
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
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
}
