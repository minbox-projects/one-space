use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HistoryEntry {
    pub timestamp: u64,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AiProvider {
    pub id: String,
    pub name: String,
    pub tool: String, // "claude", "codex", "gemini", "opencode"
    pub api_key: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,

    // 通用模型字段
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    // Claude 专属模型路由映射
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_reasoning_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_haiku_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_sonnet_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_opus_model: Option<String>,

    // Claude 高级选项
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dangerously_skip_permissions: Option<bool>,

    // 是否同步到 CLI 配置文件 (针对 OpenCode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_enabled: Option<bool>,

    // 供应商标识，作为 opencode.json 中的 key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_key: Option<String>,

    // 历史记录
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<HistoryEntry>>,

    // 存储 OpenCode 特有的所有其他字段，确保 JSON 编辑时不丢失数据
    #[serde(flatten)]
    pub extra_fields: std::collections::HashMap<String, serde_json::Value>,
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
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;

    let mut state = if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            AiProvidersState::default()
        }
    } else {
        AiProvidersState::default()
    };

    // 如果是第一次启动（没有 ai_providers.json），或者 providers 列表为空，则进行导入
    if state.providers.is_empty() {
        // 1. 提取 Claude Code 配置
        let mut claude_provider = AiProvider {
            id: "default-claude".to_string(),
            name: "Imported Claude Config".to_string(),
            tool: "claude".to_string(),
            api_key: "".to_string(),
            base_url: None,
            model: None,
            claude_reasoning_model: None,
            claude_haiku_model: None,
            claude_sonnet_model: None,
            claude_opus_model: None,
            dangerously_skip_permissions: None,
            is_enabled: None,
            provider_key: None,
            history: None,
            extra_fields: std::collections::HashMap::new(),
        };

        let claude_settings_path = home_dir.join(".claude").join("settings.json");
        if claude_settings_path.exists() {
            if let Ok(content) = fs::read_to_string(&claude_settings_path) {
                if let Ok(serde_json::Value::Object(settings)) = serde_json::from_str(&content) {
                    if let Some(serde_json::Value::Bool(skip)) =
                        settings.get("dangerouslySkipPermissions")
                    {
                        claude_provider.dangerously_skip_permissions = Some(*skip);
                    }

                    if let Some(serde_json::Value::Object(env)) = settings.get("env") {
                        if let Some(serde_json::Value::String(key)) = env.get("ANTHROPIC_API_KEY") {
                            claude_provider.api_key = key.clone();
                        } else if let Some(serde_json::Value::String(key)) =
                            env.get("ANTHROPIC_AUTH_TOKEN")
                        {
                            claude_provider.api_key = key.clone();
                        }
                        if let Some(serde_json::Value::String(url)) = env.get("ANTHROPIC_BASE_URL")
                        {
                            claude_provider.base_url = Some(url.clone());
                        }
                        if let Some(serde_json::Value::String(m)) =
                            env.get("ANTHROPIC_REASONING_MODEL")
                        {
                            claude_provider.claude_reasoning_model = Some(m.clone());
                        }
                        if let Some(serde_json::Value::String(m)) =
                            env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL")
                        {
                            claude_provider.claude_haiku_model = Some(m.clone());
                        }
                        if let Some(serde_json::Value::String(m)) =
                            env.get("ANTHROPIC_DEFAULT_SONNET_MODEL")
                        {
                            claude_provider.claude_sonnet_model = Some(m.clone());
                        }
                        if let Some(serde_json::Value::String(m)) =
                            env.get("ANTHROPIC_DEFAULT_OPUS_MODEL")
                        {
                            claude_provider.claude_opus_model = Some(m.clone());
                        }
                    }
                }
            }
        }
        state.providers.push(claude_provider);
        state.active_claude = Some("default-claude".to_string());

        // 2. 提取 Codex 配置
        let mut codex_provider = AiProvider {
            id: "default-codex".to_string(),
            name: "Imported Codex Config".to_string(),
            tool: "codex".to_string(),
            api_key: "".to_string(),
            base_url: None,
            model: None,
            claude_reasoning_model: None,
            claude_haiku_model: None,
            claude_sonnet_model: None,
            claude_opus_model: None,
            dangerously_skip_permissions: None,
            is_enabled: None,
            provider_key: None,
            history: None,
            extra_fields: std::collections::HashMap::new(),
        };

        let codex_auth_path = home_dir.join(".codex").join("auth.json");
        if codex_auth_path.exists() {
            if let Ok(content) = fs::read_to_string(&codex_auth_path) {
                if let Ok(serde_json::Value::Object(auth)) = serde_json::from_str(&content) {
                    if let Some(serde_json::Value::String(key)) = auth.get("OPENAI_API_KEY") {
                        codex_provider.api_key = key.clone();
                    }
                }
            }
        }
        let codex_config_path = home_dir.join(".codex").join("config.toml");
        if codex_config_path.exists() {
            if let Ok(content) = fs::read_to_string(&codex_config_path) {
                if let Ok(doc) = content.parse::<toml_edit::DocumentMut>() {
                    // 提取 model_providers 的 base_url
                    if let Some(providers) = doc.get("model_providers").and_then(|v| v.as_table()) {
                        for (_key, val) in providers.iter() {
                            if let Some(url) = val.get("base_url").and_then(|v| v.as_str()) {
                                codex_provider.base_url = Some(url.to_string());
                                break;
                            }
                        }
                    }

                    if codex_provider.base_url.is_none() {
                        if let Some(url) = doc.get("base_url").and_then(|v| v.as_str()) {
                            codex_provider.base_url = Some(url.to_string());
                        }
                    }
                    if let Some(model) = doc.get("model").and_then(|v| v.as_str()) {
                        codex_provider.model = Some(model.to_string());
                    }
                }
            }
        }
        state.providers.push(codex_provider);
        state.active_codex = Some("default-codex".to_string());

        // 3. 提取 Gemini 配置
        let mut gemini_provider = AiProvider {
            id: "default-gemini".to_string(),
            name: "Imported Gemini Config".to_string(),
            tool: "gemini".to_string(),
            api_key: "".to_string(),
            base_url: None,
            model: None,
            claude_reasoning_model: None,
            claude_haiku_model: None,
            claude_sonnet_model: None,
            claude_opus_model: None,
            dangerously_skip_permissions: None,
            is_enabled: None,
            provider_key: None,
            history: None,
            extra_fields: std::collections::HashMap::new(),
        };

        let gemini_env_path = home_dir.join(".gemini").join(".env");
        if gemini_env_path.exists() {
            if let Ok(content) = fs::read_to_string(&gemini_env_path) {
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some((k, v)) = line.split_once('=') {
                        let key = k.trim();
                        let val = v.trim();
                        match key {
                            "GEMINI_API_KEY" => gemini_provider.api_key = val.to_string(),
                            "GOOGLE_GEMINI_BASE_URL" => {
                                gemini_provider.base_url = Some(val.to_string())
                            }
                            "GEMINI_MODEL" => gemini_provider.model = Some(val.to_string()),
                            _ => {}
                        }
                    }
                }
            }
        }
        state.providers.push(gemini_provider);
        state.active_gemini = Some("default-gemini".to_string());
    }

    // 4. 提取 OpenCode 配置 - 始终与 opencode.json 同步
    let opencode_settings_path = home_dir
        .join(".config")
        .join("opencode")
        .join("opencode.json");
    let mut opencode_ids_in_json = std::collections::HashSet::new();

    if opencode_settings_path.exists() {
        if let Ok(content) = fs::read_to_string(&opencode_settings_path) {
            if let Ok(serde_json::Value::Object(settings)) = serde_json::from_str(&content) {
                if let Some(serde_json::Value::Object(providers)) = settings.get("provider") {
                    for (id, val) in providers.iter() {
                        if let Some(p) = val.as_object() {
                            let provider_id = if id == "onespace_provider" {
                                "default-opencode".to_string()
                            } else {
                                format!("opencode-{}", id)
                            };
                            opencode_ids_in_json.insert(provider_id.clone());
                            let provider_key = id.clone();

                            // 将所有字段存入 extra_fields
                            let mut extra_fields = std::collections::HashMap::new();
                            for (k, v) in p.iter() {
                                extra_fields.insert(k.clone(), v.clone());
                            }

                            // 如果 onespace 已经有了，更新它并标记为 is_enabled
                            let mut found = false;
                            for p_existing in state.providers.iter_mut() {
                                if p_existing.id == provider_id {
                                    p_existing.is_enabled = Some(true);
                                    p_existing.provider_key = Some(provider_key.clone());
                                    p_existing.extra_fields = extra_fields.clone();

                                    if let Some(serde_json::Value::Object(options)) =
                                        p.get("options")
                                    {
                                        if let Some(serde_json::Value::String(key)) =
                                            options.get("apiKey")
                                        {
                                            p_existing.api_key = key.clone();
                                        }
                                        if let Some(serde_json::Value::String(url)) =
                                            options.get("baseURL")
                                        {
                                            p_existing.base_url = Some(url.clone());
                                        }
                                    }
                                    if let Some(serde_json::Value::Object(models)) = p.get("models")
                                    {
                                        if let Some((model_id, _)) = models.iter().next() {
                                            p_existing.model = Some(model_id.clone());
                                        }
                                    }
                                    found = true;
                                    break;
                                }
                            }

                            if !found {
                                let mut opencode_provider = AiProvider {
                                    id: provider_id,
                                    name: p
                                        .get("name")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or(id)
                                        .to_string(),
                                    tool: "opencode".to_string(),
                                    api_key: "".to_string(),
                                    base_url: None,
                                    model: None,
                                    claude_reasoning_model: None,
                                    claude_haiku_model: None,
                                    claude_sonnet_model: None,
                                    claude_opus_model: None,
                                    dangerously_skip_permissions: None,
                                    is_enabled: Some(true),
                                    provider_key: Some(provider_key),
                                    history: None,
                                    extra_fields,
                                };

                                if let Some(serde_json::Value::Object(options)) = p.get("options") {
                                    if let Some(serde_json::Value::String(key)) =
                                        options.get("apiKey")
                                    {
                                        opencode_provider.api_key = key.clone();
                                    }
                                    if let Some(serde_json::Value::String(url)) =
                                        options.get("baseURL")
                                    {
                                        opencode_provider.base_url = Some(url.clone());
                                    }
                                }

                                if let Some(serde_json::Value::Object(models)) = p.get("models") {
                                    if let Some((model_id, _)) = models.iter().next() {
                                        opencode_provider.model = Some(model_id.clone());
                                    }
                                }

                                if state.active_opencode.is_none() {
                                    state.active_opencode = Some(opencode_provider.id.clone());
                                }
                                state.providers.push(opencode_provider);
                            }
                        }
                    }
                }
            }
        }
    }

    // 标记 onespace 中存在但 opencode.json 中不存在的为 is_enabled: false
    for p in state.providers.iter_mut() {
        if p.tool == "opencode" {
            p.is_enabled = Some(opencode_ids_in_json.contains(&p.id));
        }
    }

    let opencode_has_providers = state.providers.iter().any(|p| p.tool == "opencode");

    if !opencode_has_providers {
        state.providers.push(AiProvider {
            id: "default-opencode".to_string(),
            name: "Imported OpenCode Config".to_string(),
            tool: "opencode".to_string(),
            api_key: "".to_string(),
            base_url: None,
            model: None,
            claude_reasoning_model: None,
            claude_haiku_model: None,
            claude_sonnet_model: None,
            claude_opus_model: None,
            dangerously_skip_permissions: None,
            is_enabled: Some(false),
            provider_key: Some("onespace_provider".to_string()),
            history: None,
            extra_fields: std::collections::HashMap::new(),
        });
        state.active_opencode = Some("default-opencode".to_string());
    }

    let _ = save_ai_providers_internal(&state);
    Ok(state)
}

fn save_ai_providers_internal(state: &AiProvidersState) -> Result<(), String> {
    let path = get_providers_path()?;
    let json = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    let mut file = File::create(&path).map_err(|e| e.to_string())?;
    file.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn save_ai_providers(state: AiProvidersState) -> Result<(), String> {
    save_ai_providers_internal(&state)
}

fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let temp_path = path.with_extension("tmp");
    let mut file = File::create(&temp_path).map_err(|e| e.to_string())?;
    file.write_all(content.as_bytes())
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    fs::rename(&temp_path, path).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn remove_ai_environment(provider: AiProvider) -> Result<(), String> {
    if provider.tool != "opencode" {
        return Ok(());
    }
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    let opencode_dir = home_dir.join(".config").join("opencode");
    let settings_path = opencode_dir.join("opencode.json");
    if !settings_path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(&settings_path).map_err(|e| e.to_string())?;
    let mut settings: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| e.to_string())?;
    if let Some(providers) = settings.get_mut("provider").and_then(|v| v.as_object_mut()) {
        let target_id = if let Some(key) = &provider.provider_key {
            key.as_str()
        } else if provider.id == "default-opencode" {
            "onespace_provider"
        } else if provider.id.starts_with("opencode-") {
            &provider.id[9..]
        } else {
            &provider.id
        };
        providers.remove(target_id);
        atomic_write(
            &settings_path,
            &serde_json::to_string_pretty(&settings).unwrap(),
        )?;
    }
    Ok(())
}

#[tauri::command]
pub async fn apply_ai_environment(provider: AiProvider) -> Result<(), String> {
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
    if provider.tool == "opencode" && provider.is_enabled == Some(false) {
        return remove_ai_environment(provider);
    }
    match provider.tool.as_str() {
        "claude" => {
            let claude_dir = home_dir.join(".claude");
            let settings_path = claude_dir.join("settings.json");
            let mut settings = serde_json::Map::new();
            if settings_path.exists() {
                let content =
                    fs::read_to_string(&settings_path).unwrap_or_else(|_| "{}".to_string());
                if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(&content) {
                    settings = map;
                }
            }
            if let Some(skip) = provider.dangerously_skip_permissions {
                settings.insert(
                    "dangerouslySkipPermissions".to_string(),
                    serde_json::Value::Bool(skip),
                );
            } else {
                settings.remove("dangerouslySkipPermissions");
            }
            if !settings.contains_key("env") {
                settings.insert(
                    "env".to_string(),
                    serde_json::Value::Object(serde_json::Map::new()),
                );
            }
            if let Some(serde_json::Value::Object(ref mut env)) = settings.get_mut("env") {
                env.insert(
                    "ANTHROPIC_API_KEY".to_string(),
                    serde_json::Value::String(provider.api_key),
                );
                if let Some(base_url) = provider.base_url {
                    if !base_url.is_empty() {
                        env.insert(
                            "ANTHROPIC_BASE_URL".to_string(),
                            serde_json::Value::String(base_url),
                        );
                    } else {
                        env.remove("ANTHROPIC_BASE_URL");
                    }
                } else {
                    env.remove("ANTHROPIC_BASE_URL");
                }
                let model_mappings = vec![
                    (
                        "ANTHROPIC_REASONING_MODEL",
                        &provider.claude_reasoning_model,
                    ),
                    (
                        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
                        &provider.claude_haiku_model,
                    ),
                    (
                        "ANTHROPIC_DEFAULT_SONNET_MODEL",
                        &provider.claude_sonnet_model,
                    ),
                    ("ANTHROPIC_DEFAULT_OPUS_MODEL", &provider.claude_opus_model),
                ];
                for (key, val) in model_mappings {
                    if let Some(model_name) = val {
                        if !model_name.is_empty() {
                            env.insert(
                                key.to_string(),
                                serde_json::Value::String(model_name.clone()),
                            );
                        } else {
                            env.remove(key);
                        }
                    } else {
                        env.remove(key);
                    }
                }
            }
            atomic_write(
                &settings_path,
                &serde_json::to_string_pretty(&settings).unwrap(),
            )?;
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
            auth.insert(
                "OPENAI_API_KEY".to_string(),
                serde_json::Value::String(provider.api_key),
            );
            atomic_write(&auth_path, &serde_json::to_string_pretty(&auth).unwrap())?;
            let config_path = codex_dir.join("config.toml");
            let mut toml_str = String::new();
            if config_path.exists() {
                toml_str = fs::read_to_string(&config_path).unwrap_or_default();
            }
            let mut doc = toml_str
                .parse::<toml_edit::DocumentMut>()
                .map_err(|e| e.to_string())?;
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
                } else {
                    doc.remove("model");
                }
            } else {
                doc.remove("model");
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
                } else {
                    env_map.remove("GEMINI_MODEL");
                }
            } else {
                env_map.remove("GEMINI_MODEL");
            }
            let mut env_content = String::new();
            for (k, v) in env_map {
                env_content.push_str(&format!("{}={}\n", k, v));
            }
            atomic_write(&env_path, &env_content)?;
        }
        "opencode" => {
            let opencode_dir = home_dir.join(".config").join("opencode");
            let settings_path = opencode_dir.join("opencode.json");
            let mut settings = serde_json::Map::new();
            if settings_path.exists() {
                let content =
                    fs::read_to_string(&settings_path).unwrap_or_else(|_| "{}".to_string());
                if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(&content) {
                    settings = map;
                }
            }
            if !settings.contains_key("$schema") {
                settings.insert(
                    "$schema".to_string(),
                    serde_json::Value::String("https://opencode.ai/config.json".to_string()),
                );
            }
            if !settings.contains_key("provider") {
                settings.insert(
                    "provider".to_string(),
                    serde_json::Value::Object(serde_json::Map::new()),
                );
            }
            if let Some(serde_json::Value::Object(ref mut providers)) = settings.get_mut("provider")
            {
                let target_id = if let Some(key) = &provider.provider_key {
                    key.as_str()
                } else if provider.id == "default-opencode" {
                    "onespace_provider"
                } else if provider.id.starts_with("opencode-") {
                    &provider.id[9..]
                } else {
                    &provider.id
                };
                let mut full_provider_json = serde_json::to_value(&provider).unwrap();
                if let Some(obj) = full_provider_json.as_object_mut() {
                    obj.remove("id");
                    obj.remove("tool");
                    obj.remove("is_enabled");
                    obj.remove("provider_key");
                    obj.remove("history");
                    obj.remove("api_key");
                    obj.remove("base_url");
                    obj.remove("model");
                    obj.remove("claude_reasoning_model");
                    obj.remove("claude_haiku_model");
                    obj.remove("claude_sonnet_model");
                    obj.remove("claude_opus_model");
                    obj.remove("dangerously_skip_permissions");
                }
                providers.insert(target_id.to_string(), full_provider_json);
            }
            atomic_write(
                &settings_path,
                &serde_json::to_string_pretty(&settings).unwrap(),
            )?;
        }
        _ => return Err(format!("Unknown tool: {}", provider.tool)),
    }
    Ok(())
}
