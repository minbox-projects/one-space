use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use crate::{get_data_dir, crypto, git};

#[derive(Serialize, Deserialize)]
struct EncryptedStorage {
    #[serde(default)]
    pub is_encrypted: bool,
    pub data: String,
}

fn content_path(name: &str) -> Result<PathBuf, String> {
    let data_dir = get_data_dir()?;
    let dir = data_dir.join("data").join("content");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join(format!("{}.enc.json", name)))
}

fn legacy_path(name: &str) -> Result<PathBuf, String> {
    let data_dir = get_data_dir()?;
    Ok(data_dir.join(format!("{}.json", name)))
}

fn read_content(name: &str) -> Result<String, String> {
    let new_path = content_path(name)?;
    let legacy = legacy_path(name)?;
    let target = if new_path.exists() { new_path } else { legacy };
    if !target.exists() {
        return Ok("[]".to_string());
    }
    let content = fs::read_to_string(&target).map_err(|e| e.to_string())?;
    if let Ok(storage) = serde_json::from_str::<EncryptedStorage>(&content) {
        if storage.is_encrypted {
            let password = crypto::get_or_init_master_password()?;
            return crypto::decrypt(&storage.data, &password);
        }
        return Ok(storage.data);
    }
    Ok(content)
}

fn save_content(app: tauri::AppHandle, name: &str, raw_json: &str) -> Result<(), String> {
    let target = content_path(name)?;
    let password = crypto::get_or_init_master_password()?;
    let encrypted_data = crypto::encrypt(raw_json, &password)?;
    let storage = EncryptedStorage { is_encrypted: true, data: encrypted_data };
    let content = serde_json::to_string_pretty(&storage).map_err(|e| e.to_string())?;
    let mut file = File::create(&target).map_err(|e| e.to_string())?;
    file.write_all(content.as_bytes()).map_err(|e| e.to_string())?;

    let legacy = legacy_path(name)?;
    if legacy.exists() {
        let _ = fs::remove_file(legacy);
    }

    tauri::async_runtime::spawn(async move { let _ = git::sync_git(app).await; });
    Ok(())
}

#[tauri::command]
pub fn read_snippets() -> Result<String, String> {
    read_content("snippets")
}

#[tauri::command]
pub fn save_snippets(app: tauri::AppHandle, snippets_json: &str) -> Result<(), String> {
    save_content(app, "snippets", snippets_json)
}

#[tauri::command]
pub fn read_bookmarks() -> Result<String, String> {
    read_content("bookmarks")
}

#[tauri::command]
pub fn save_bookmarks(app: tauri::AppHandle, bookmarks_json: &str) -> Result<(), String> {
    save_content(app, "bookmarks", bookmarks_json)
}

#[tauri::command]
pub fn read_notes() -> Result<String, String> {
    read_content("notes")
}

#[tauri::command]
pub fn save_notes(app: tauri::AppHandle, notes_json: &str) -> Result<(), String> {
    save_content(app, "notes", notes_json)
}

#[tauri::command]
pub fn read_game_data() -> Result<String, String> {
    read_content("game_data")
}

#[tauri::command]
pub fn save_game_data(app: tauri::AppHandle, data_json: &str) -> Result<(), String> {
    save_content(app, "game_data", data_json)
}
