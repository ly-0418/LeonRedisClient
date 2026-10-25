use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::models::{AppSettings, ConnectionProfile};

const STORE_VERSION: u32 = 1;
const SECRETS_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct StoreFile {
    version: u32,
    profiles: Vec<ConnectionProfile>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SecretsFile {
    version: u32,
    secrets: HashMap<String, String>,
}

pub fn load_profiles(path: &Path) -> Vec<ConnectionProfile> {
    let Ok(raw) = fs::read_to_string(path) else {
        return Vec::new();
    };

    match serde_json::from_str::<StoreFile>(&raw) {
        Ok(store) if store.version == STORE_VERSION => store.profiles,
        Ok(_) => {
            eprintln!("连接配置文件版本不匹配，已忽略: {}", path.display());
            Vec::new()
        }
        Err(err) => {
            eprintln!("连接配置文件解析失败: {err}");
            Vec::new()
        }
    }
}

pub fn persist_profiles(path: &Path, profiles: &[ConnectionProfile]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("创建配置目录失败: {err}"))?;
    }

    let store = StoreFile {
        version: STORE_VERSION,
        profiles: profiles.to_vec(),
    };
    let json = serde_json::to_vec_pretty(&store).map_err(|err| format!("序列化配置失败: {err}"))?;

    // 先写临时文件，再替换，避免应用崩溃时留下半截 JSON。
    let tmp: PathBuf = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|err| format!("写入临时配置失败: {err}"))?;
    if path.exists() {
        fs::remove_file(path).map_err(|err| format!("替换旧配置失败: {err}"))?;
    }
    fs::rename(&tmp, path).map_err(|err| format!("保存配置失败: {err}"))?;
    Ok(())
}

fn secrets_path(store_path: &Path) -> PathBuf {
    store_path.with_file_name("secrets.json")
}

fn settings_path(store_path: &Path) -> PathBuf {
    store_path.with_file_name("settings.json")
}

pub fn load_settings(store_path: &Path) -> Option<AppSettings> {
    let raw = fs::read_to_string(settings_path(store_path)).ok()?;
    serde_json::from_str::<AppSettings>(&raw).ok()
}

pub fn persist_settings(store_path: &Path, settings: &AppSettings) -> Result<(), String> {
    let path = settings_path(store_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("创建配置目录失败: {err}"))?;
    }
    let json =
        serde_json::to_vec_pretty(settings).map_err(|err| format!("序列化设置失败: {err}"))?;
    let tmp: PathBuf = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|err| format!("写入临时设置失败: {err}"))?;
    if path.exists() {
        fs::remove_file(&path).map_err(|err| format!("替换旧设置失败: {err}"))?;
    }
    fs::rename(&tmp, &path).map_err(|err| format!("保存设置失败: {err}"))?;
    Ok(())
}

fn load_secrets(store_path: &Path) -> HashMap<String, String> {
    let path = secrets_path(store_path);
    let Ok(raw) = fs::read_to_string(path) else {
        return HashMap::new();
    };
    match serde_json::from_str::<SecretsFile>(&raw) {
        Ok(store) if store.version == SECRETS_VERSION => store.secrets,
        Ok(_) => {
            eprintln!("密码存储版本不匹配，已忽略");
            HashMap::new()
        }
        Err(err) => {
            eprintln!("密码存储解析失败: {err}");
            HashMap::new()
        }
    }
}

fn persist_secrets(store_path: &Path, secrets: &HashMap<String, String>) -> Result<(), String> {
    let path = secrets_path(store_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("创建密码目录失败: {err}"))?;
    }
    let store = SecretsFile {
        version: SECRETS_VERSION,
        secrets: secrets.clone(),
    };
    let json = serde_json::to_vec_pretty(&store).map_err(|err| format!("序列化密码失败: {err}"))?;
    let tmp: PathBuf = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|err| format!("写入临时密码失败: {err}"))?;
    if path.exists() {
        fs::remove_file(&path).map_err(|err| format!("替换旧密码失败: {err}"))?;
    }
    fs::rename(&tmp, &path).map_err(|err| format!("保存密码失败: {err}"))?;
    Ok(())
}

pub fn load_secret(store_path: &Path, profile_id: &str) -> Option<String> {
    load_secrets(store_path).get(profile_id).cloned()
}

pub fn save_secret(store_path: &Path, profile_id: &str, value: &str) -> Result<(), String> {
    let mut secrets = load_secrets(store_path);
    secrets.insert(profile_id.to_string(), value.to_string());
    persist_secrets(store_path, &secrets)
}

pub fn remove_secret(store_path: &Path, profile_id: &str) -> Result<(), String> {
    let mut secrets = load_secrets(store_path);
    if secrets.remove(profile_id).is_some() {
        persist_secrets(store_path, &secrets)?;
    }
    Ok(())
}
