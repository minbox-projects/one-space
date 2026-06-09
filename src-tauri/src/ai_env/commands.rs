#[tauri::command]
pub fn get_master_password() -> Result<String, String> {
    crate::crypto::get_or_init_master_password()
}

#[tauri::command]
pub async fn change_master_password(
    app: tauri::AppHandle,
    old_pass: String,
    new_pass: String,
) -> Result<(), String> {
    let current_pass = crate::crypto::get_or_init_master_password()?;
    if current_pass != old_pass {
        return Err("Old password incorrect".to_string());
    }

    // 1. Load decrypted config with old password, then rotate data files old->new.
    let storage_config = crate::config::get_storage_config()?;
    crate::app_store::rotate_master_password_data(&old_pass, &new_pass)?;
    crate::backup::rotate_backup_password(&old_pass, &new_pass)?;

    // 2. Switch active master password and re-save config with the new key.
    crate::crypto::set_master_password(&new_pass)?;
    crate::config::save_storage_config(app.clone(), storage_config).await?;

    Ok(())
}

#[tauri::command]
pub fn skip_claude_onboarding_login() -> Result<(), String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let claude_main_path = home_dir.join(".claude.json");

    let mut root = serde_json::Map::new();
    if claude_main_path.exists() {
        let content = fs::read_to_string(&claude_main_path).map_err(|e| e.to_string())?;
        if !content.trim().is_empty() {
            let parsed: serde_json::Value = serde_json::from_str(&content)
                .map_err(|e| format!("Failed to parse ~/.claude.json: {}", e))?;
            root = match parsed {
                serde_json::Value::Object(map) => map,
                _ => return Err("~/.claude.json must contain a JSON object".to_string()),
            };
        }
    }

    if root.get("hasCompletedOnboarding").and_then(|v| v.as_bool()) == Some(true) {
        return Ok(());
    }

    root.insert(
        "hasCompletedOnboarding".to_string(),
        serde_json::Value::Bool(true),
    );
    let content = serde_json::to_string_pretty(&serde_json::Value::Object(root))
        .map_err(|e| e.to_string())?;
    atomic_write(&claude_main_path, &content)?;
    Ok(())
}
