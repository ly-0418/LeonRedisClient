use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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

/// 连接配置的读取结果。
///
/// 读取失败时不能静默返回空列表，否则用户下一次保存会把原文件覆盖掉。
pub struct ProfilesLoad {
    pub profiles: Vec<ConnectionProfile>,
    /// 已经尽力抢救（例如从 .bak 恢复、损坏文件已备份）时给出的提示。
    pub warning: Option<String>,
    /// 主文件损坏且无法备份，此时必须禁止写入，避免覆盖用户数据。
    pub blocked: bool,
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    match path.file_name() {
        Some(name) => path.with_file_name(format!("{}.{suffix}", name.to_string_lossy())),
        None => path.with_extension(suffix),
    }
}

/// 读取顺序：主文件 -> .tmp（崩溃时的待替换文件）-> .bak（上一次成功写入的版本）。
fn candidates(path: &Path) -> [PathBuf; 3] {
    [
        path.to_path_buf(),
        sibling(path, "tmp"),
        sibling(path, "bak"),
    ]
}

/// 读取 UTF-8 文本并去掉 BOM。
///
/// 记事本、部分编辑器和同步工具写出的文件带 BOM，而 serde_json 不接受 BOM，
/// 不去掉会导致整个配置被判定为损坏。
fn read_text(path: &Path) -> Option<String> {
    if !path.exists() {
        return None;
    }
    let raw = fs::read(path).ok()?;
    let text = String::from_utf8(raw).ok()?;
    Some(text.strip_prefix('\u{feff}').unwrap_or(&text).to_string())
}

/// 写入顺序：tmp -> 把旧文件改名成 .bak -> 把 tmp 改成正式文件。
///
/// 任意一步崩溃，下一次读取都能从 .bak 或 .tmp 拿到完整内容。
fn write_text_atomic(path: &Path, contents: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("创建配置目录失败: {err}"))?;
    }

    let tmp = sibling(path, "tmp");
    fs::write(&tmp, contents).map_err(|err| format!("写入临时配置文件失败: {err}"))?;

    let backup = sibling(path, "bak");
    if path.exists() {
        let _ = fs::remove_file(&backup);
        fs::rename(path, &backup).map_err(|err| format!("备份旧配置失败: {err}"))?;
    }

    if let Err(err) = fs::rename(&tmp, path) {
        // 回滚，保证正式文件不会凭空消失。
        if backup.exists() {
            let _ = fs::rename(&backup, path);
        }
        return Err(format!("保存配置失败: {err}"));
    }
    Ok(())
}

/// 把损坏的文件改名保留，成功返回备份路径。
fn quarantine(path: &Path) -> Result<PathBuf, String> {
    if !path.exists() {
        return Err("文件不存在".to_string());
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let target = sibling(path, &format!("corrupt-{stamp}"));
    fs::rename(path, &target).map_err(|err| err.to_string())?;
    Ok(target)
}

pub fn load_profiles(path: &Path) -> ProfilesLoad {
    let mut problem: Option<String> = None;

    for candidate in candidates(path) {
        if !candidate.exists() {
            continue;
        }
        let Some(raw) = read_text(&candidate) else {
            problem.get_or_insert(format!("{} 不是有效的 UTF-8 文本", candidate.display()));
            continue;
        };
        match serde_json::from_str::<StoreFile>(&raw) {
            Ok(store) if store.version == STORE_VERSION => {
                let warning = problem.map(|detail| {
                    format!(
                        "{detail}，已从备份文件 {} 恢复连接配置。",
                        candidate.display()
                    )
                });
                return ProfilesLoad {
                    profiles: store.profiles,
                    warning,
                    blocked: false,
                };
            }
            Ok(store) => {
                problem.get_or_insert(format!(
                    "连接配置版本为 {}，当前仅支持 {STORE_VERSION}",
                    store.version
                ));
            }
            Err(err) => {
                problem.get_or_insert(format!("连接配置文件解析失败: {err}"));
            }
        }
    }

    let Some(detail) = problem else {
        // 三个候选文件都不存在，属于首次启动。
        return ProfilesLoad {
            profiles: Vec::new(),
            warning: None,
            blocked: false,
        };
    };

    match quarantine(path) {
        Ok(backup) => ProfilesLoad {
            profiles: Vec::new(),
            warning: Some(format!(
                "{detail}。损坏的文件已备份为 {}，请重新添加连接。",
                backup.display()
            )),
            blocked: false,
        },
        Err(err) => ProfilesLoad {
            profiles: Vec::new(),
            warning: Some(format!(
                "{detail}，且无法备份（{err}）。为避免覆盖，已暂停写入；请手动修复或删除 {} 后重启应用。",
                path.display()
            )),
            blocked: true,
        },
    }
}

pub fn persist_profiles(path: &Path, profiles: &[ConnectionProfile]) -> Result<(), String> {
    let store = StoreFile {
        version: STORE_VERSION,
        profiles: profiles.to_vec(),
    };
    let json = serde_json::to_vec_pretty(&store).map_err(|err| format!("序列化配置失败: {err}"))?;
    write_text_atomic(path, &json)
}

fn secrets_path(store_path: &Path) -> PathBuf {
    store_path.with_file_name("secrets.json")
}

fn settings_path(store_path: &Path) -> PathBuf {
    store_path.with_file_name("settings.json")
}

pub fn load_settings(store_path: &Path) -> Option<AppSettings> {
    for candidate in candidates(&settings_path(store_path)) {
        let Some(raw) = read_text(&candidate) else {
            continue;
        };
        if let Ok(settings) = serde_json::from_str::<AppSettings>(&raw) {
            return Some(settings);
        }
    }
    None
}

pub fn persist_settings(store_path: &Path, settings: &AppSettings) -> Result<(), String> {
    let json =
        serde_json::to_vec_pretty(settings).map_err(|err| format!("序列化设置失败: {err}"))?;
    write_text_atomic(&settings_path(store_path), &json)
}

fn load_secrets(store_path: &Path) -> HashMap<String, String> {
    for candidate in candidates(&secrets_path(store_path)) {
        let Some(raw) = read_text(&candidate) else {
            continue;
        };
        match serde_json::from_str::<SecretsFile>(&raw) {
            Ok(store) if store.version == SECRETS_VERSION => return store.secrets,
            Ok(_) => {
                eprintln!("密码存储版本不匹配，已忽略: {}", candidate.display());
            }
            Err(err) => {
                eprintln!("密码存储解析失败: {err}");
            }
        }
    }
    HashMap::new()
}

fn persist_secrets(store_path: &Path, secrets: &HashMap<String, String>) -> Result<(), String> {
    let store = SecretsFile {
        version: SECRETS_VERSION,
        secrets: secrets.clone(),
    };
    let json = serde_json::to_vec_pretty(&store).map_err(|err| format!("序列化密码失败: {err}"))?;
    write_text_atomic(&secrets_path(store_path), &json)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "leonredis-test-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn sample_profile() -> ConnectionProfile {
        ConnectionProfile {
            id: "p1".to_string(),
            name: "本地".to_string(),
            host: "127.0.0.1".to_string(),
            port: 6379,
            db: 0,
            username: None,
            remember_password: false,
            last_used_at: None,
        }
    }

    #[test]
    fn reads_file_with_utf8_bom() {
        let dir = temp_dir("bom");
        let path = dir.join("connections.json");
        let json = serde_json::to_string(&StoreFile {
            version: STORE_VERSION,
            profiles: vec![sample_profile()],
        })
        .expect("serialize");

        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(json.as_bytes());
        fs::write(&path, bytes).expect("write");

        let loaded = load_profiles(&path);
        assert_eq!(loaded.profiles.len(), 1);
        assert!(loaded.warning.is_none());
        assert!(!loaded.blocked);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn quarantines_corrupt_file_and_reports_error() {
        let dir = temp_dir("corrupt");
        let path = dir.join("connections.json");
        fs::write(&path, b"{ this is not json").expect("write");

        let loaded = load_profiles(&path);
        assert!(loaded.profiles.is_empty());
        assert!(loaded.warning.is_some());
        assert!(!loaded.blocked);
        assert!(!path.exists(), "损坏文件应被改名保留");

        let backups: Vec<_> = fs::read_dir(&dir)
            .expect("read dir")
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().contains(".corrupt-"))
            .collect();
        assert_eq!(backups.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn recovers_from_backup_file() {
        let dir = temp_dir("recover");
        let path = dir.join("connections.json");
        fs::write(&path, b"broken").expect("write main");

        let json = serde_json::to_string(&StoreFile {
            version: STORE_VERSION,
            profiles: vec![sample_profile()],
        })
        .expect("serialize");
        fs::write(sibling(&path, "bak"), json).expect("write bak");

        let loaded = load_profiles(&path);
        assert_eq!(loaded.profiles.len(), 1);
        assert!(loaded.warning.is_some());
        assert!(!loaded.blocked);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn keeps_previous_version_as_backup() {
        let dir = temp_dir("atomic");
        let path = dir.join("connections.json");
        persist_profiles(&path, &[sample_profile()]).expect("first write");
        persist_profiles(&path, &[]).expect("second write");

        let backup = read_text(&sibling(&path, "bak")).expect("backup exists");
        assert!(backup.contains("\"p1\""), "备份应保留上一版内容");
        assert!(!sibling(&path, "tmp").exists(), "临时文件应被清理");
        let _ = fs::remove_dir_all(&dir);
    }
}
