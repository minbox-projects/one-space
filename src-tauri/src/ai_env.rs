use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AiProvider {
    pub id: String,
    pub name: String,
    pub tool: String, // "claude", "codex", "gemini", "opencode"
    pub api_key: String,
    pub base_url: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AiProvidersState {
    pub active_claude: Option<String>,
    pub active_codex: Option<String>,
    pub active_gemini: Option<String>,
    pub active_opencode: Option<String>,
    pub providers: Vec<AiProvider>,
}

fn get_providers_path() -> Result<PathBuf, String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let config_dir = home_dir.join(".config").join("onespace");
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;
    }
    Ok(config_dir.join("ai_providers.json"))
}

#[tauri::command]
pub fn get_ai_providers() -> Result<AiProvidersState, String> {
    let path = get_providers_path()?;
    if !path.exists() {
        return Ok(AiProvidersState::default());
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let state: AiProvidersState = serde_json::from_str(&content).unwrap_or_default();
    Ok(state)
}

#[tauri::command]
pub fn save_ai_providers(state: AiProvidersState) -> Result<(), String> {
    let path = get_providers_path()?;
    let json = serde_json::to_string_pretty(&state).map_err(|e| e.to_string())?;
    let mut file = File::create(&path).map_err(|e| e.to_string())?;
    file.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    
    let temp_path = path.with_extension("tmp");
    let mut file = File::create(&temp_path).map_err(|e| e.to_string())?;
    file.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    fs::rename(&temp_path, path).map_err(|e| e.to_string())?;
    
    Ok(())
}

#[tauri::command]
pub fn apply_ai_environment(provider: AiProvider) -> Result<(), String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    
    match provider.tool.as_str() {
        "claude" => {
            let claude_dir = home_dir.join(".claude");
            let settings_path = claude_dir.join("settings.json");
            
            let mut settings = serde_json::Map::new();
            if settings_path.exists() {
                let content = fs::read_to_string(&settings_path).unwrap_or_else(|_| "{}".to_string());
                if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(&content) {
                    settings = map;
                }
            }
            
            if !settings.contains_key("env") {
                settings.insert("env".to_string(), serde_json::Value::Object(serde_json::Map::new()));
            }
            
            if let Some(serde_json::Value::Object(ref mut env)) = settings.get_mut("env") {
                env.insert("ANTHROPIC_API_KEY".to_string(), serde_json::Value::String(provider.api_key));
                
                if let Some(base_url) = provider.base_url {
                    if !base_url.is_empty() {
                        env.insert("ANTHROPIC_BASE_URL".to_string(), serde_json::Value::String(base_url));
                    } else {
                        env.remove("ANTHROPIC_BASE_URL");
                    }
                } else {
                    env.remove("ANTHROPIC_BASE_URL");
                }
            }
            
            atomic_write(&settings_path, &serde_json::to_string_pretty(&settings).unwrap())?;
        }
        "codex" => {
            let codex_dir = home_dir.join(".codex");
            
            let auth_path = codex_dir.join("auth.json");
            let mut auth = serde_json::Map::new();
            if auth_path.exists() {
                let content = fs::read_to_string(&auth_path).unwrap_or_else(|_| "{}".to_string());
                if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(&content) {
                    auth = map;
                }
            }
            auth.insert("OPENAI_API_KEY".to_string(), serde_json::Value::String(provider.api_key));
            atomic_write(&auth_path, &serde_json::to_string_pretty(&auth).unwrap())?;
            
            let config_path = codex_dir.join("config.toml");
            let mut toml_str = String::new();
            if config_path.exists() {
                toml_str = fs::read_to_string(&config_path).unwrap_or_default();
            }
            
            let mut doc = toml_str.parse::<toml_edit::DocumentMut>().map_err(|e| e.to_string())?;
            
            if let Some(base_url) = provider.base_url {
                if !base_url.is_empty() {
                    doc["base_url"] = toml_edit::value(base_url);
                } else {
                    doc.remove("base_url");
                }
            } else {
                doc.remove("base_url");
            }
            
            if let Some(model) = provider.model {
                if !model.is_empty() {
                    doc["model"] = toml_edit::value(model);
                }
            }
            
            atomic_write(&config_path, &doc.to_string())?;
        }
        "gemini" => {
            let gemini_dir = home_dir.join(".gemini");
            let env_path = gemini_dir.join(".env");
            
            let mut env_map = std::collections::BTreeMap::new();
            if env_path.exists() {
                let content = fs::read_to_string(&env_path).unwrap_or_default();
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some((k, v)) = line.split_once('=') {
                        env_map.insert(k.trim().to_string(), v.trim().to_string());
                    }
                }
            }
            
            env_map.insert("GEMINI_API_KEY".to_string(), provider.api_key);
            
            if let Some(base_url) = provider.base_url {
                if !base_url.is_empty() {
                    env_map.insert("GOOGLE_GEMINI_BASE_URL".to_string(), base_url);
                } else {
                    env_map.remove("GOOGLE_GEMINI_BASE_URL");
                }
            } else {
                env_map.remove("GOOGLE_GEMINI_BASE_URL");
            }
            
            if let Some(model) = provider.model {
                if !model.is_empty() {
                    env_map.insert("GEMINI_MODEL".to_string(), model);
                }
            }
            
            let mut env_content = String::new();
            for (k, v) in env_map {
                env_content.push_str(&format!("{}={}\n", k, v));
            }
            atomic_write(&env_path, &env_content)?;
        }
        "opencode" => {
            let opencode_dir = home_dir.join(".opencode");
            let settings_path = opencode_dir.join("settings.json");
            
            let mut settings = serde_json::Map::new();
            if settings_path.exists() {
                let content = fs::read_to_string(&settings_path).unwrap_or_else(|_| "{}".to_string());
                if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(&content) {
                    settings = map;
                }
            }
            
            settings.insert("api_key".to_string(), serde_json::Value::String(provider.api_key));
            if let Some(base_url) = provider.base_url {
                if !base_url.is_empty() {
                    settings.insert("base_url".to_string(), serde_json::Value::String(base_url));
                }
            }
            if let Some(model) = provider.model {
                if !model.is_empty() {
                    settings.insert("model".to_string(), serde_json::Value::String(model));
                }
            }
            
            atomic_write(&settings_path, &serde_json::to_string_pretty(&settings).unwrap())?;
        }
        _ => return Err(format!("Unknown tool: {}", provider.tool)),
    }
    
    Ok(())
}
