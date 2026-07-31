use crate::crypto;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

const TINYURL_API_TOKEN_KEY: &str = "tinyurl_api_token";
const RESERVED_SECRET_KEY_ERROR: &str = "This secret is managed by a dedicated command.";

#[derive(Serialize, Deserialize, Default)]
pub struct Secrets {
    #[serde(default)]
    pub is_encrypted: bool,
    pub values: HashMap<String, String>,
}

#[derive(Serialize, Deserialize)]
struct EncryptedBlob {
    #[serde(default)]
    is_encrypted: bool,
    data: String,
}

fn get_secrets_path() -> Result<PathBuf, String> {
    let data_dir = crate::get_data_dir()?;
    let dir = data_dir.join("data").join("secrets");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("state.enc.json"))
}

fn get_legacy_secrets_path() -> Result<PathBuf, String> {
    let data_dir = crate::get_data_dir()?;
    Ok(data_dir.join("secrets.json"))
}

fn load_secrets() -> Result<Secrets, String> {
    let new_path = get_secrets_path()?;
    let legacy_path = get_legacy_secrets_path()?;
    let target = if new_path.exists() {
        new_path
    } else {
        legacy_path
    };
    if !target.exists() {
        return Ok(Secrets::default());
    }
    let content = fs::read_to_string(target).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(Secrets::default());
    }
    parse_secrets_content(&content)
}

fn decode_blob_payload(blob: EncryptedBlob) -> Result<Secrets, String> {
    let plain_payload = if blob.is_encrypted {
        let password = crypto::get_or_init_master_password()?;
        crypto::decrypt(&blob.data, &password)?
    } else {
        blob.data
    };
    let value: Value = serde_json::from_str(&plain_payload).map_err(|e| e.to_string())?;
    let obj = value
        .as_object()
        .ok_or("Invalid secrets blob payload".to_string())?;

    let mut values = HashMap::new();
    for (k, v) in obj {
        if let Some(s) = v.as_str() {
            values.insert(k.clone(), s.to_string());
        }
    }

    Ok(Secrets {
        // Keep in-memory values as plaintext; file encryption is handled at blob level.
        is_encrypted: false,
        values,
    })
}

fn normalize_legacy_secrets(secrets: Secrets) -> Result<Secrets, String> {
    if !secrets.is_encrypted {
        return Ok(secrets);
    }

    let password = crypto::get_or_init_master_password()?;
    let mut values = HashMap::new();
    for (k, v) in secrets.values {
        let plain = crypto::decrypt(&v, &password).unwrap_or(v);
        values.insert(k, plain);
    }

    Ok(Secrets {
        is_encrypted: false,
        values,
    })
}

fn parse_secrets_content(content: &str) -> Result<Secrets, String> {
    if let Ok(secrets) = serde_json::from_str::<Secrets>(content) {
        return normalize_legacy_secrets(secrets);
    }
    if let Ok(blob) = serde_json::from_str::<EncryptedBlob>(content) {
        return decode_blob_payload(blob);
    }
    Err("Invalid secrets format".to_string())
}

fn write_secrets(secrets: &Secrets) -> Result<(), String> {
    let path = get_secrets_path()?;
    let mut obj = Map::new();
    for (k, v) in &secrets.values {
        obj.insert(k.clone(), Value::String(v.clone()));
    }

    let password = crypto::get_or_init_master_password()?;
    let encrypted = crypto::encrypt(&Value::Object(obj).to_string(), &password)?;
    let blob = EncryptedBlob {
        is_encrypted: true,
        data: encrypted,
    };
    let json = serde_json::to_string_pretty(&blob).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())?;

    let legacy_path = get_legacy_secrets_path()?;
    if legacy_path.exists() {
        let _ = fs::remove_file(legacy_path);
    }
    Ok(())
}

#[tauri::command]
pub fn get_secret(key: &str) -> Result<Option<String>, String> {
    get_generic_secret(key)
}

fn get_generic_secret(key: &str) -> Result<Option<String>, String> {
    ensure_generic_secret_key_allowed(key)?;
    get_secret_value(key)
}

#[tauri::command]
pub async fn save_secret(app: tauri::AppHandle, key: String, value: String) -> Result<(), String> {
    let _ = app;
    save_generic_secret(key, value)
}

fn save_generic_secret(key: String, value: String) -> Result<(), String> {
    ensure_generic_secret_key_allowed(&key)?;
    save_secret_value(key, value)
}

fn ensure_generic_secret_key_allowed(key: &str) -> Result<(), String> {
    if key == TINYURL_API_TOKEN_KEY {
        return Err(RESERVED_SECRET_KEY_ERROR.to_string());
    }
    Ok(())
}

pub(crate) fn get_secret_value(key: &str) -> Result<Option<String>, String> {
    let secrets = load_secrets()?;
    Ok(secrets.values.get(key).cloned())
}

pub(crate) fn save_secret_value(key: String, value: String) -> Result<(), String> {
    let mut secrets = load_secrets()?;
    secrets.values.insert(key, value);
    write_secrets(&secrets)
}

pub(crate) fn get_tinyurl_api_token() -> Result<Option<String>, String> {
    get_secret_value(TINYURL_API_TOKEN_KEY)
}

pub(crate) fn save_tinyurl_api_token(token: String) -> Result<(), String> {
    save_secret_value(TINYURL_API_TOKEN_KEY.to_string(), token)
}

pub(crate) fn delete_tinyurl_api_token() -> Result<(), String> {
    let mut secrets = load_secrets()?;
    if secrets.values.remove(TINYURL_API_TOKEN_KEY).is_some() {
        write_secrets(&secrets)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_secret(app: tauri::AppHandle, key: String) -> Result<(), String> {
    let _ = app;
    delete_generic_secret(key)
}

fn delete_generic_secret(key: String) -> Result<(), String> {
    ensure_generic_secret_key_allowed(&key)?;
    let mut secrets = load_secrets()?;

    if secrets.values.remove(&key).is_some() {
        write_secrets(&secrets)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const TEST_TOKEN: &str = "tinyurl-test-sensitive-token";

    fn with_temp_home<T>(name: &str, test: impl FnOnce(&Path) -> T) -> T {
        let _guard = crate::lock_test_home_env();
        let temp_home = std::env::temp_dir().join(format!(
            "onespace-secrets-{}-{}",
            name,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&temp_home).expect("create temporary home");
        let original_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &temp_home);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| test(&temp_home)));
        restore_home(original_home);
        let _ = fs::remove_dir_all(&temp_home);
        match result {
            Ok(value) => value,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }

    fn restore_home(original_home: Option<std::ffi::OsString>) {
        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
    }

    fn seed_tinyurl_token() {
        save_tinyurl_api_token(TEST_TOKEN.to_string()).expect("seed TinyURL token");
    }

    fn assert_reserved_key_error(error: &str) {
        assert_eq!(error, RESERVED_SECRET_KEY_ERROR);
        assert!(!error.contains(TINYURL_API_TOKEN_KEY));
        assert!(!error.contains(TEST_TOKEN));
    }

    fn assert_stored_tinyurl_token(expected: Option<&str>) {
        assert_eq!(
            get_tinyurl_api_token().expect("read TinyURL token through dedicated storage"),
            expected.map(str::to_string)
        );
    }

    #[test]
    fn get_secret_command_cannot_read_tinyurl_token() {
        with_temp_home("reserved-read", |_| {
            seed_tinyurl_token();

            let error = get_secret(TINYURL_API_TOKEN_KEY)
                .expect_err("generic command must reject TinyURL token reads");

            assert_reserved_key_error(&error);
            assert_stored_tinyurl_token(Some(TEST_TOKEN));
        });
    }

    #[test]
    fn save_secret_command_cannot_overwrite_tinyurl_token() {
        with_temp_home("reserved-write", |_| {
            seed_tinyurl_token();

            let error = save_generic_secret(
                TINYURL_API_TOKEN_KEY.to_string(),
                "attacker-controlled-replacement".to_string(),
            )
            .expect_err("generic command must reject TinyURL token writes");

            assert_reserved_key_error(&error);
            assert_stored_tinyurl_token(Some(TEST_TOKEN));
        });
    }

    #[test]
    fn delete_secret_command_cannot_delete_tinyurl_token() {
        with_temp_home("reserved-delete", |_| {
            seed_tinyurl_token();

            let error = delete_generic_secret(TINYURL_API_TOKEN_KEY.to_string())
                .expect_err("generic command must reject TinyURL token deletion");

            assert_reserved_key_error(&error);
            assert_stored_tinyurl_token(Some(TEST_TOKEN));
        });
    }

    #[test]
    fn tinyurl_dedicated_commands_still_manage_reserved_token() {
        with_temp_home("dedicated-commands", |_| {
            let saved = crate::short_link::short_link_save_token(format!("  {TEST_TOKEN}  "))
                .expect("save TinyURL token through dedicated command");
            assert_eq!(
                serde_json::to_string(&saved).expect("serialize saved status"),
                r#"{"configured":true}"#
            );
            assert_stored_tinyurl_token(Some(TEST_TOKEN));

            let status = crate::short_link::short_link_config_status()
                .expect("read TinyURL status through dedicated command");
            assert_eq!(
                serde_json::to_string(&status).expect("serialize configuration status"),
                r#"{"configured":true}"#
            );

            let deleted = crate::short_link::short_link_delete_token()
                .expect("delete TinyURL token through dedicated command");
            assert_eq!(
                serde_json::to_string(&deleted).expect("serialize deleted status"),
                r#"{"configured":false}"#
            );
            assert_stored_tinyurl_token(None);
        });
    }
}
