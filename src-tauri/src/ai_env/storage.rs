use super::{AiProvider, AiProvidersState};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use uuid::Uuid;

pub(in crate::ai_env) fn process_providers_sensitive_data(
    state: &mut AiProvidersState,
    encrypt: bool,
) -> Result<(), String> {
    let password = crate::crypto::get_or_init_master_password()?;

    for p in state.providers.iter_mut() {
        if encrypt {
            if !p.api_key.is_empty() {
                p.api_key = crate::crypto::encrypt(&p.api_key, &password)?;
            }
        } else {
            if !p.api_key.is_empty() {
                // Try to decrypt, if fails (maybe it was plain text), keep as is
                if let Ok(decrypted) = crate::crypto::decrypt(&p.api_key, &password) {
                    p.api_key = decrypted;
                }
            }
        }

        // Handle OpenCode extra fields (options.apiKey)
        if let Some(options) = p.extra_fields.get_mut("options") {
            if let Some(opts_obj) = options.as_object_mut() {
                if let Some(api_key_val) = opts_obj.get_mut("apiKey") {
                    if let Some(key_str) = api_key_val.as_str() {
                        if !key_str.is_empty() {
                            if encrypt {
                                *api_key_val = serde_json::Value::String(crate::crypto::encrypt(
                                    key_str, &password,
                                )?);
                            } else {
                                if let Ok(dec) = crate::crypto::decrypt(key_str, &password) {
                                    *api_key_val = serde_json::Value::String(dec);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    state.is_encrypted = encrypt;
    Ok(())
}

pub(in crate::ai_env) fn get_providers_path() -> Result<PathBuf, String> {
    let data_dir = crate::get_data_dir()?;
    Ok(data_dir.join("ai_providers.json"))
}

pub fn get_ai_providers() -> Result<AiProvidersState, String> {
    let path = get_providers_path()?;
    let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;

    let mut state = if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if content.trim().is_empty() {
                AiProvidersState::default()
            } else {
                match serde_json::from_str::<AiProvidersState>(&content) {
                    Ok(mut s) => {
                        if s.is_encrypted {
                            let _ = process_providers_sensitive_data(&mut s, false);
                        }
                        s
                    }
                    Err(e) => {
                        println!("Failed to parse ai_providers.json at {:?}: {}", path, e);
                        // Fallback: try to read as the old format or return error
                        AiProvidersState::default()
                    }
                }
            }
        } else {
            return Err("Failed to read ai_providers.json".to_string());
        }
    } else {
        // Fallback for transition: check old path
        let old_config_dir = home_dir.join(".config").join("onespace");
        let old_path = old_config_dir.join("ai_providers.json");
        if old_path.exists() {
            if let Ok(content) = fs::read_to_string(&old_path) {
                serde_json::from_str(&content).unwrap_or_default()
            } else {
                AiProvidersState::default()
            }
        } else {
            AiProvidersState::default()
        }
    };

    // Only import defaults if the state is truly empty (e.g., first run or file missing)
    if state.providers.is_empty() {
        // 1. 提取 Claude Code 配置
        let mut claude_provider = AiProvider {
            id: Uuid::new_v4().to_string(),
            name: "Imported Claude Config".to_string(),
            tool: "claude".to_string(),
            api_key: "".to_string(),
            ..Default::default()
        };

        let claude_settings_path = home_dir.join(".claude").join("settings.json");
        if claude_settings_path.exists() {
            if let Ok(content) = fs::read_to_string(&claude_settings_path) {
                if let Ok(serde_json::Value::Object(settings)) = serde_json::from_str(&content) {
                    let normalized_default_model =
                        crate::app_store::resolve_claude_default_model_from_settings(&settings);
                    if let Some(serde_json::Value::Bool(skip)) =
                        settings.get("dangerouslySkipPermissions")
                    {
                        claude_provider.dangerously_skip_permissions = Some(*skip);
                    }
                    if let Some(serde_json::Value::Bool(memory)) =
                        settings.get("enableAllMemoryFeatures")
                    {
                        claude_provider.enable_all_memory_features = Some(*memory);
                    }
                    if let Some(serde_json::Value::Bool(mcp)) = settings.get("enableMcp") {
                        claude_provider.enable_mcp = Some(*mcp);
                    }
                    if let Some(serde_json::Value::Array(allowed)) = settings.get("allowedTools") {
                        claude_provider.allowed_tools = Some(
                            allowed
                                .iter()
                                .filter_map(|v| v.as_str())
                                .map(|s| s.to_string())
                                .collect(),
                        );
                    }
                    if let Some(serde_json::Value::Array(blocked)) = settings.get("blockedTools") {
                        claude_provider.blocked_tools = Some(
                            blocked
                                .iter()
                                .filter_map(|v| v.as_str())
                                .map(|s| s.to_string())
                                .collect(),
                        );
                    }
                    if let Some(serde_json::Value::Number(turns)) = settings.get("maxSessionTurns")
                    {
                        claude_provider.max_session_turns = turns.as_u64().map(|n| n as u32);
                    }

                    if let Some(serde_json::Value::Object(env)) = settings.get("env") {
                        // Prefer ANTHROPIC_API_KEY over AUTH_TOKEN
                        if let Some(serde_json::Value::String(key)) = env.get("ANTHROPIC_API_KEY") {
                            claude_provider.api_key = key.clone();
                        } else if let Some(serde_json::Value::String(key)) =
                            env.get("ANTHROPIC_AUTH_TOKEN")
                        {
                            // Fallback to AUTH_TOKEN if API_KEY not set
                            claude_provider.api_key = key.clone();
                        }
                        if let Some(serde_json::Value::String(url)) = env.get("ANTHROPIC_BASE_URL")
                        {
                            claude_provider.base_url = Some(url.clone());
                        }
                        if let Some(serde_json::Value::String(v)) =
                            env.get("CLAUDE_CODE_EFFORT_LEVEL")
                        {
                            claude_provider.claude_reasoning_effort = Some(v.clone());
                        }
                        let mut claude_model_mappings = Vec::new();
                        for family in ["haiku", "sonnet", "opus"] {
                            let Some((model_key, name_key, capabilities_key)) =
                                crate::app_store::claude_model_env_keys_for_family(family)
                            else {
                                continue;
                            };
                            let raw_model =
                                env.get(model_key).and_then(|v| v.as_str()).unwrap_or("");
                            let (upstream_model, supports_1m) =
                                crate::app_store::split_claude_1m_suffix(raw_model);
                            let display_name = env
                                .get(name_key)
                                .and_then(|v| v.as_str())
                                .unwrap_or(match family {
                                    "haiku" => "Haiku",
                                    "sonnet" => "Sonnet",
                                    "opus" => "Opus",
                                    _ => "",
                                })
                                .to_string();
                            let supported_capabilities = env
                                .get(capabilities_key)
                                .and_then(|v| v.as_str())
                                .and_then(crate::app_store::parse_supported_capabilities_csv);
                            if !upstream_model.is_empty()
                                || !display_name.is_empty()
                                || supported_capabilities.is_some()
                            {
                                claude_model_mappings.push(crate::app_store::ClaudeModelMapping {
                                    family: family.to_string(),
                                    display_name,
                                    upstream_model,
                                    supports_1m: Some(supports_1m && family != "haiku"),
                                    supported_capabilities,
                                });
                            }
                        }
                        if !claude_model_mappings.is_empty() {
                            claude_provider.claude_model_mappings = Some(claude_model_mappings);
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
                    claude_provider.model = normalized_default_model.clone();
                    claude_provider.claude_default_model = normalized_default_model;
                }
            }
        }
        if !claude_provider.api_key.is_empty()
            && claude_provider
                .base_url
                .as_ref()
                .map_or(false, |url| !url.is_empty())
        {
            state.active_claude = Some(claude_provider.id.clone());
        }
        state.providers.push(claude_provider);

        // 2. 提取 Codex 配置
        let mut codex_provider = AiProvider {
            id: Uuid::new_v4().to_string(),
            name: "Imported Codex Config".to_string(),
            tool: "codex".to_string(),
            api_key: "".to_string(),
            ..Default::default()
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
                    if let Some(disable) = doc
                        .get("disable_response_storage")
                        .and_then(|v| v.as_bool())
                    {
                        codex_provider.disable_response_storage = Some(disable);
                    }
                    if let Some(personality_val) = doc.get("personality").and_then(|v| v.as_str()) {
                        codex_provider.personality = Some(personality_val.to_string());
                    }
                    if let Some(model_providers) =
                        doc.get("model_providers").and_then(|v| v.as_table())
                    {
                        for (_key, val) in model_providers.iter() {
                            if let Some(url) = val.get("base_url").and_then(|v| v.as_str()) {
                                codex_provider.base_url = Some(url.to_string());
                            }
                            if let Some(wire_api_val) = val.get("wire_api").and_then(|v| v.as_str())
                            {
                                codex_provider.wire_api = Some(wire_api_val.to_string());
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
        if !codex_provider.api_key.is_empty()
            && codex_provider
                .base_url
                .as_ref()
                .map_or(false, |url| !url.is_empty())
        {
            state.active_codex = Some(codex_provider.id.clone());
        }
        state.providers.push(codex_provider);

        // 3. 提取 Gemini 配置
        let mut gemini_provider = AiProvider {
            id: Uuid::new_v4().to_string(),
            name: "Imported Gemini Config".to_string(),
            tool: "gemini".to_string(),
            api_key: "".to_string(),
            ..Default::default()
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

        let gemini_settings_path = home_dir.join(".gemini").join("settings.json");
        if gemini_settings_path.exists() {
            if let Ok(content) = fs::read_to_string(&gemini_settings_path) {
                if let Ok(serde_json::Value::Object(settings)) = serde_json::from_str(&content) {
                    if let Some(security) = settings.get("security").and_then(|v| v.as_object()) {
                        if let Some(auth) = security.get("auth").and_then(|v| v.as_object()) {
                            if let Some(serde_json::Value::String(auth_type)) =
                                auth.get("selectedType")
                            {
                                gemini_provider.gemini_auth_type = Some(auth_type.clone());
                            }
                        }
                    }
                }
            }
        }
        if !gemini_provider.api_key.is_empty()
            && gemini_provider
                .base_url
                .as_ref()
                .map_or(false, |url| !url.is_empty())
        {
            state.active_gemini = Some(gemini_provider.id.clone());
        }
        state.providers.push(gemini_provider);
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
                            let provider_id = state
                                .providers
                                .iter()
                                .find(|provider| {
                                    provider.tool == "opencode"
                                        && provider.provider_key.as_deref() == Some(id.as_str())
                                })
                                .map(|provider| provider.id.clone())
                                .unwrap_or_else(|| Uuid::new_v4().to_string());
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
                                if p_existing.tool == "opencode"
                                    && p_existing.provider_key.as_deref()
                                        == Some(provider_key.as_str())
                                {
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
                                    id: provider_id.clone(),
                                    name: p
                                        .get("name")
                                        .and_then(|v| v.as_str())
                                        .map(|name| name.to_string())
                                        .unwrap_or_else(|| {
                                            format!("Imported OpenCode Config ({})", provider_key)
                                        }),
                                    tool: "opencode".to_string(),
                                    api_key: "".to_string(),
                                    is_enabled: Some(true),
                                    provider_key: Some(provider_key.clone()),
                                    extra_fields: extra_fields.clone(),
                                    ..Default::default()
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
            id: Uuid::new_v4().to_string(),
            name: "Imported OpenCode Config".to_string(),
            tool: "opencode".to_string(),
            api_key: "".to_string(),
            is_enabled: Some(false),
            provider_key: Some("onespace_provider".to_string()),
            ..Default::default()
        });
    }

    Ok(state)
}

#[allow(dead_code)]
pub(in crate::ai_env) fn save_ai_providers_internal(
    state: &AiProvidersState,
) -> Result<(), String> {
    let path = get_providers_path()?;
    let mut state_to_save = state.clone();

    // Always encrypt when saving to file
    process_providers_sensitive_data(&mut state_to_save, true)?;

    let json = serde_json::to_string_pretty(&state_to_save).map_err(|e| e.to_string())?;
    let mut file = File::create(&path).map_err(|e| e.to_string())?;
    file.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

#[allow(dead_code)]
pub async fn save_ai_providers(
    app: tauri::AppHandle,
    state: AiProvidersState,
) -> Result<(), String> {
    save_ai_providers_internal(&state)?;
    let _ = crate::app_store::sync_enqueue(app, "ai_env_save_providers".to_string()).await;

    Ok(())
}
