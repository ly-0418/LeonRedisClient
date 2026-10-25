mod commands;
mod models;
mod redis_manager;
mod security;
mod state;
mod storage;

use std::fs;
use std::path::{Path, PathBuf};

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

fn copy_dir_all(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn move_if_missing(source: &Path, destination: &Path) -> std::io::Result<()> {
    if !source.exists() || destination.exists() {
        return Ok(());
    }
    if destination.parent().is_some_and(|parent| !parent.exists()) {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
    }
    if fs::rename(source, destination).is_ok() {
        return Ok(());
    }
    fs::copy(source, destination)?;
    fs::remove_file(source)?;
    Ok(())
}

fn migrate_webview(old_local_dir: &Path, webview_dir: &Path) -> bool {
    let old_profile = old_local_dir.join("EBWebView");
    let new_profile = webview_dir.join("EBWebView");
    if !old_profile.exists() {
        return true;
    }
    if new_profile.exists() {
        return false;
    }
    if fs::create_dir_all(webview_dir).is_err() {
        return false;
    }
    if fs::rename(&old_profile, &new_profile).is_ok() {
        return true;
    }
    if copy_dir_all(&old_profile, &new_profile).is_ok() {
        let _ = fs::remove_dir_all(&old_profile);
        return true;
    }
    false
}

fn install_data_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let executable = std::env::current_exe()?;
    let install_dir = executable.parent().ok_or("无法确定安装目录")?.to_path_buf();
    let data_dir = install_dir.join("data");
    fs::create_dir_all(&data_dir)?;
    Ok(data_dir)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = install_data_dir()?;
            let store_path = data_dir.join("connections.json");

            // 将旧版 AppData 配置迁移到安装目录下的 data。
            if let Ok(legacy_dir) = app.path().app_data_dir() {
                let _ = move_if_missing(&legacy_dir.join("connections.json"), &store_path);
                let _ = move_if_missing(
                    &legacy_dir.join("secrets.json"),
                    &data_dir.join("secrets.json"),
                );
                if !legacy_dir.join("connections.json").exists()
                    && !legacy_dir.join("secrets.json").exists()
                {
                    let _ = fs::remove_dir_all(&legacy_dir);
                }
            }

            // 将 WebView2 缓存和 localStorage 迁移到安装目录。
            if let Ok(legacy_local_dir) = app.path().app_local_data_dir() {
                let webview_dir = data_dir.join("webview");
                if migrate_webview(&legacy_local_dir, &webview_dir) {
                    let _ = fs::remove_dir_all(&legacy_local_dir);
                }
            }

            app.manage(state::AppState::new(store_path));

            WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title("LeonRedis")
                .inner_size(1380.0, 880.0)
                .min_inner_size(1080.0, 680.0)
                .center()
                .resizable(true)
                .decorations(false)
                .shadow(true)
                .data_directory(data_dir.join("webview"))
                .build()?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_profiles,
            commands::load_settings,
            commands::save_settings,
            commands::save_profile,
            commands::delete_profile,
            commands::test_connection,
            commands::connect,
            commands::get_saved_password,
            commands::disconnect,
            commands::get_databases,
            commands::select_database,
            commands::flush_db,
            commands::analyze_performance,
            commands::export_data,
            commands::import_data,
            commands::set_read_only,
            commands::scan_keys,
            commands::get_key_info,
            commands::get_key_value,
            commands::create_key,
            commands::set_string,
            commands::hash_set,
            commands::hash_delete,
            commands::list_push,
            commands::list_set,
            commands::list_remove,
            commands::set_add,
            commands::set_remove,
            commands::zset_add,
            commands::zset_remove,
            commands::delete_key,
            commands::expire_key,
            commands::rename_key,
            commands::update_key,
            commands::get_server_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LeonRedis");
}
