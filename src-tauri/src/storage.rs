use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use crate::{get_data_dir, crypto, git};

#[derive(Serialize, Deserialize)]
struct EncryptedStorage {
    #[serde(default)]
    pub is_encrypted: bool,
    pub data: String,
}

#[tauri::command]
pub fn read_snippets() -> Result<String, String> {
    let data_dir = get_data_dir()?;
    let snippets_path = data_dir.join("snippets.json");
    if !snippets_path.exists() { return Ok("[]".to_string()); }
    let content = fs::read_to_string(snippets_path).map_err(|e| e.to_string())?;
    if let Ok(storage) = serde_json::from_str::<EncryptedStorage>(&content) {
        if storage.is_encrypted {
            let password = crypto::get_or_init_master_password()?;
            return crypto::decrypt(&storage.data, &password);
        }
        return Ok(storage.data);
    }
    Ok(content)
}

#[tauri::command]
pub fn save_snippets(app: tauri::AppHandle, snippets_json: &str) -> Result<(), String> {
    let data_dir = get_data_dir()?;
    let snippets_path = data_dir.join("snippets.json");
    let password = crypto::get_or_init_master_password()?;
    let encrypted_data = crypto::encrypt(snippets_json, &password)?;
    let storage = EncryptedStorage { is_encrypted: true, data: encrypted_data };
    let content = serde_json::to_string_pretty(&storage).map_err(|e| e.to_string())?;
    let mut file = File::create(snippets_path).map_err(|e| e.to_string())?;
    file.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn(async move { let _ = git::sync_git(app).await; });
    Ok(())
}

#[tauri::command]
pub fn read_bookmarks() -> Result<String, String> {
    let data_dir = get_data_dir()?;
    let bookmarks_path = data_dir.join("bookmarks.json");
    if !bookmarks_path.exists() { return Ok("[]".to_string()); }
    let content = fs::read_to_string(bookmarks_path).map_err(|e| e.to_string())?;
    if let Ok(storage) = serde_json::from_str::<EncryptedStorage>(&content) {
        if storage.is_encrypted {
            let password = crypto::get_or_init_master_password()?;
            return crypto::decrypt(&storage.data, &password);
        }
        return Ok(storage.data);
    }
    Ok(content)
}

#[tauri::command]
pub fn save_bookmarks(app: tauri::AppHandle, bookmarks_json: &str) -> Result<(), String> {
    let data_dir = get_data_dir()?;
    let bookmarks_path = data_dir.join("bookmarks.json");
    let password = crypto::get_or_init_master_password()?;
    let encrypted_data = crypto::encrypt(bookmarks_json, &password)?;
    let storage = EncryptedStorage { is_encrypted: true, data: encrypted_data };
    let content = serde_json::to_string_pretty(&storage).map_err(|e| e.to_string())?;
    let mut file = File::create(bookmarks_path).map_err(|e| e.to_string())?;
    file.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn(async move { let _ = git::sync_git(app).await; });
    Ok(())
}

#[tauri::command]
pub fn read_notes() -> Result<String, String> {
    let data_dir = get_data_dir()?;
    let notes_path = data_dir.join("notes.json");
    if !notes_path.exists() { return Ok("[]".to_string()); }
    let content = fs::read_to_string(notes_path).map_err(|e| e.to_string())?;
    if let Ok(storage) = serde_json::from_str::<EncryptedStorage>(&content) {
        if storage.is_encrypted {
            let password = crypto::get_or_init_master_password()?;
            return crypto::decrypt(&storage.data, &password);
        }
        return Ok(storage.data);
    }
    Ok(content)
}

#[tauri::command]
pub fn save_notes(app: tauri::AppHandle, notes_json: &str) -> Result<(), String> {
    let data_dir = get_data_dir()?;
    let notes_path = data_dir.join("notes.json");
    let password = crypto::get_or_init_master_password()?;
    let encrypted_data = crypto::encrypt(notes_json, &password)?;
    let storage = EncryptedStorage { is_encrypted: true, data: encrypted_data };
    let content = serde_json::to_string_pretty(&storage).map_err(|e| e.to_string())?;
    let mut file = File::create(notes_path).map_err(|e| e.to_string())?;
    file.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn(async move { let _ = git::sync_git(app).await; });
    Ok(())
}
