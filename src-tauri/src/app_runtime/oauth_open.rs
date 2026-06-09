use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri_plugin_opener::OpenerExt;

#[derive(Serialize, Deserialize)]
pub(super) struct OAuthResult {
    code: String,
    redirect_uri: String,
}

#[tauri::command]
pub(super) async fn start_google_oauth(
    app: tauri::AppHandle,
    client_id: String,
    scope: String,
) -> Result<OAuthResult, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let port = tauri_plugin_oauth::start(move |url| {
        let _ = tx.send(url);
    })
    .map_err(|e| e.to_string())?;
    let redirect_uri = format!("http://localhost:{}", port);
    let mut url = reqwest::Url::parse("https://accounts.google.com/o/oauth2/v2/auth")
        .map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", &scope)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent");
    let auth_url = url.to_string();
    app.opener()
        .open_url(auth_url, None::<&str>)
        .map_err(|e| e.to_string())?;
    let url_str = rx
        .recv_timeout(std::time::Duration::from_secs(300))
        .map_err(|_| "OAuth login timed out after 5 minutes".to_string())?;
    let url = reqwest::Url::parse(&url_str).map_err(|e| e.to_string())?;
    let code = url
        .query_pairs()
        .find(|(key, _)| key == "code")
        .map(|(_, value)| value.to_string())
        .ok_or("No code found in redirect URL")?;
    Ok(OAuthResult { code, redirect_uri })
}

#[tauri::command]
pub(super) fn open_local_path(path: &str) -> Result<(), String> {
    open_path_with_system(path)
}

#[tauri::command]
pub(super) fn open_external_url(app: tauri::AppHandle, url: &str) -> Result<(), String> {
    let parsed = validate_external_url(url)?;
    app.opener()
        .open_url(parsed.to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

pub(crate) fn open_path_with_system(path: &str) -> Result<(), String> {
    let normalized = PathBuf::from(path);
    if !normalized.exists() {
        return Err(format!("Path does not exist: {}", normalized.display()));
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&normalized)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(&normalized)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        Command::new("xdg-open")
            .arg(&normalized)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(crate) fn atomic_write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp_path = temp_write_path(path);
    let mut file = File::create(&tmp_path).map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    fs::rename(&tmp_path, path).map_err(|e| e.to_string())
}

pub(crate) fn atomic_write_string(path: &Path, content: &str) -> Result<(), String> {
    atomic_write_bytes(path, content.as_bytes())
}

pub(crate) fn temp_write_path(path: &Path) -> PathBuf {
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    PathBuf::from(tmp)
}

pub(super) fn validate_external_url(url: &str) -> Result<reqwest::Url, String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
    match parsed.scheme() {
        "https" => Ok(parsed),
        "http" => {
            let host = parsed.host_str().unwrap_or_default();
            if host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" {
                Ok(parsed)
            } else {
                Err(format!("Only loopback http URLs are allowed: {}", url))
            }
        }
        _ => Err(format!("Unsupported URL scheme: {}", parsed.scheme())),
    }
}
