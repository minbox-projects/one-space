use super::{
    now_ts, state_lock, state_path, AgentDefinition, AgentToolPolicy, AiAssistantModelProfile,
    AiAssistantProvider, AiAssistantSettings, AssistantProviderCapability, AssistantState,
    ModelCatalogItem, ModelRoleBinding, RuntimePreset,
};
use std::collections::HashSet;
use std::fs;

pub(in crate::ai_assistant) fn default_true() -> bool {
    true
}

pub(in crate::ai_assistant) fn default_bearer() -> String {
    "bearer".to_string()
}

pub(in crate::ai_assistant) fn catalog_model_id(provider_id: &str, model_id: &str) -> String {
    format!("{}::{}", provider_id.trim(), model_id.trim())
}

pub(in crate::ai_assistant) fn workspace_roles() -> [&'static str; 8] {
    [
        "chat",
        "assistant",
        "summary",
        "automation",
        "quick_assistant",
        "selection_assistant",
        "translate",
        "topic_naming",
    ]
}

pub(in crate::ai_assistant) fn legacy_profile_catalog_id(
    settings: &AiAssistantSettings,
    profile_id: Option<&str>,
) -> Option<String> {
    let profile_id = profile_id?.trim();
    if profile_id.is_empty() {
        return None;
    }
    settings
        .profiles
        .iter()
        .find(|profile| profile.id == profile_id)
        .map(|profile| catalog_model_id(&profile.provider_id, &profile.model_id))
}

pub(in crate::ai_assistant) fn build_model_catalog_from_profiles(
    settings: &AiAssistantSettings,
) -> Vec<ModelCatalogItem> {
    let mut seen = HashSet::new();
    let mut catalog = Vec::new();
    let now = now_ts();

    for profile in &settings.profiles {
        let item_id = catalog_model_id(&profile.provider_id, &profile.model_id);
        if !seen.insert(item_id.clone()) {
            continue;
        }
        let provider_capabilities = settings
            .providers
            .iter()
            .find(|provider| provider.id == profile.provider_id)
            .map(|provider| provider.capabilities.clone())
            .unwrap_or_default();
        catalog.push(ModelCatalogItem {
            id: item_id,
            provider_id: profile.provider_id.clone(),
            model_id: profile.model_id.clone(),
            label: if profile.name.trim().is_empty() {
                profile.model_id.clone()
            } else {
                profile.name.clone()
            },
            description: profile.usage.clone(),
            enabled: true,
            tags: if profile.usage.trim().is_empty() {
                Vec::new()
            } else {
                vec![profile.usage.clone()]
            },
            supports_reasoning: provider_capabilities.supports_reasoning
                || profile.enable_reasoning,
            supports_streaming: provider_capabilities.supports_streaming,
            supports_web_search: provider_capabilities.supports_web_search,
            created_at: now,
            updated_at: now,
        });
    }

    catalog
}

pub(in crate::ai_assistant) fn default_role_model_id(
    settings: &AiAssistantSettings,
    role: &str,
) -> Option<String> {
    let explicit = match role {
        "assistant" | "automation" | "selection_assistant" => {
            legacy_profile_catalog_id(settings, settings.default_agent_profile_id.as_deref())
        }
        "summary" | "translate" | "topic_naming" => {
            legacy_profile_catalog_id(settings, settings.default_summary_profile_id.as_deref())
        }
        _ => legacy_profile_catalog_id(settings, settings.default_chat_profile_id.as_deref()),
    };

    explicit.or_else(|| settings.model_catalog.first().map(|item| item.id.clone()))
}

pub(in crate::ai_assistant) fn default_runtime_presets() -> Vec<RuntimePreset> {
    vec![
        RuntimePreset {
            id: "balanced".to_string(),
            name: "Balanced".to_string(),
            description: "General-purpose preset for chat, quick assistant, and routine work."
                .to_string(),
            temperature: Some(0.3),
            max_tokens: Some(2048),
            enable_reasoning: true,
        },
        RuntimePreset {
            id: "deep_reasoning".to_string(),
            name: "Deep Reasoning".to_string(),
            description: "Longer responses and stronger reasoning for assistants and automations."
                .to_string(),
            temperature: Some(0.2),
            max_tokens: Some(4096),
            enable_reasoning: true,
        },
        RuntimePreset {
            id: "lightweight".to_string(),
            name: "Lightweight".to_string(),
            description: "Fast, low-cost preset for summaries, translation, and topic naming."
                .to_string(),
            temperature: Some(0.1),
            max_tokens: Some(1024),
            enable_reasoning: false,
        },
    ]
}

pub(in crate::ai_assistant) fn build_default_role_bindings(
    settings: &AiAssistantSettings,
) -> Vec<ModelRoleBinding> {
    workspace_roles()
        .into_iter()
        .map(|role| ModelRoleBinding {
            id: role.to_string(),
            role: role.to_string(),
            model_id: default_role_model_id(settings, role),
            runtime_preset_id: Some(
                match role {
                    "assistant" | "automation" | "selection_assistant" => "deep_reasoning",
                    "summary" | "translate" | "topic_naming" => "lightweight",
                    _ => "balanced",
                }
                .to_string(),
            ),
            temperature: match role {
                "summary" | "translate" | "topic_naming" => Some(0.1),
                "assistant" | "automation" | "selection_assistant" => Some(0.2),
                _ => Some(0.3),
            },
            max_tokens: match role {
                "summary" | "translate" | "topic_naming" => Some(1024),
                _ => Some(2048),
            },
            enable_reasoning: role != "summary" && role != "translate" && role != "topic_naming",
        })
        .collect()
}

pub(in crate::ai_assistant) fn default_assistant_settings() -> AiAssistantSettings {
    let mut settings = AiAssistantSettings {
        providers: vec![
            AiAssistantProvider {
                id: "openai-compatible".to_string(),
                name: "OpenAI Compatible".to_string(),
                protocol: "openai-compatible".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                auth_scheme: default_bearer(),
                api_key: String::new(),
                enabled: false,
                extra_headers: Vec::new(),
                capabilities: AssistantProviderCapability {
                    supports_reasoning: true,
                    supports_streaming: true,
                    supports_web_search: false,
                },
            },
            AiAssistantProvider {
                id: "anthropic-direct".to_string(),
                name: "Anthropic Direct".to_string(),
                protocol: "anthropic-messages".to_string(),
                base_url: "https://api.anthropic.com/v1".to_string(),
                auth_scheme: "x-api-key".to_string(),
                api_key: String::new(),
                enabled: false,
                extra_headers: Vec::new(),
                capabilities: AssistantProviderCapability {
                    supports_reasoning: true,
                    supports_streaming: true,
                    supports_web_search: false,
                },
            },
            AiAssistantProvider {
                id: "gemini-direct".to_string(),
                name: "Gemini Direct".to_string(),
                protocol: "google-gemini".to_string(),
                base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
                auth_scheme: "x-goog-api-key".to_string(),
                api_key: String::new(),
                enabled: false,
                extra_headers: Vec::new(),
                capabilities: AssistantProviderCapability {
                    supports_reasoning: false,
                    supports_streaming: true,
                    supports_web_search: false,
                },
            },
        ],
        profiles: vec![
            AiAssistantModelProfile {
                id: "chat-default".to_string(),
                name: "chat-default".to_string(),
                provider_id: "openai-compatible".to_string(),
                model_id: "gpt-4.1".to_string(),
                usage: "chat".to_string(),
                temperature: Some(0.3),
                top_p: None,
                max_tokens: Some(2048),
                frequency_penalty: None,
                presence_penalty: None,
                stop_sequences: None,
                enable_reasoning: true,
            },
            AiAssistantModelProfile {
                id: "agent-main".to_string(),
                name: "agent-main".to_string(),
                provider_id: "openai-compatible".to_string(),
                model_id: "gpt-4.1".to_string(),
                usage: "agent".to_string(),
                temperature: Some(0.2),
                top_p: None,
                max_tokens: Some(2048),
                frequency_penalty: None,
                presence_penalty: None,
                stop_sequences: None,
                enable_reasoning: true,
            },
            AiAssistantModelProfile {
                id: "summarizer".to_string(),
                name: "summarizer".to_string(),
                provider_id: "openai-compatible".to_string(),
                model_id: "gpt-4.1-mini".to_string(),
                usage: "summary".to_string(),
                temperature: Some(0.1),
                top_p: None,
                max_tokens: Some(1024),
                frequency_penalty: None,
                presence_penalty: None,
                stop_sequences: None,
                enable_reasoning: false,
            },
        ],
        model_catalog: Vec::new(),
        role_bindings: Vec::new(),
        runtime_presets: default_runtime_presets(),
        default_chat_profile_id: Some("chat-default".to_string()),
        default_agent_profile_id: Some("agent-main".to_string()),
        default_summary_profile_id: Some("summarizer".to_string()),
    };
    settings.model_catalog = build_model_catalog_from_profiles(&settings);
    settings.role_bindings = build_default_role_bindings(&settings);
    settings
}

pub(in crate::ai_assistant) fn default_agents() -> Vec<AgentDefinition> {
    let now = now_ts();
    vec![
        AgentDefinition {
            id: "release-agent".to_string(),
            name: "Release Agent".to_string(),
            avatar_emoji: Some("🚀".to_string()),
            description: "Focus on release notes, regression risk, and launch checklists.".to_string(),
            system_prompt: "You are OneSpace Release Agent. Produce concise release checklists, risk summaries, and action items.".to_string(),
            primary_model_id: Some(catalog_model_id("openai-compatible", "gpt-4.1")),
            light_model_id: Some(catalog_model_id("openai-compatible", "gpt-4.1-mini")),
            default_model_profile_id: Some("agent-main".to_string()),
            light_model_profile_id: Some("summarizer".to_string()),
            tool_policy: AgentToolPolicy {
                web_search: true,
                workspace_read: true,
                notes_search: true,
            },
            knowledge_base_ids: Vec::new(),
            mcp_server_ids: Vec::new(),
            memory_enabled: false,
            output_contract: "summary + risks + action_items".to_string(),
            created_at: now,
            updated_at: now,
        },
        AgentDefinition {
            id: "research-agent".to_string(),
            name: "Research Agent".to_string(),
            avatar_emoji: Some("🔎".to_string()),
            description: "Focus on multi-source synthesis and evidence-backed summaries.".to_string(),
            system_prompt: "You are OneSpace Research Agent. Prefer sourced answers with clear assumptions.".to_string(),
            primary_model_id: Some(catalog_model_id("openai-compatible", "gpt-4.1")),
            light_model_id: Some(catalog_model_id("openai-compatible", "gpt-4.1-mini")),
            default_model_profile_id: Some("agent-main".to_string()),
            light_model_profile_id: Some("summarizer".to_string()),
            tool_policy: AgentToolPolicy {
                web_search: true,
                workspace_read: false,
                notes_search: true,
            },
            knowledge_base_ids: Vec::new(),
            mcp_server_ids: Vec::new(),
            memory_enabled: false,
            output_contract: "summary + references + next_steps".to_string(),
            created_at: now,
            updated_at: now,
        },
    ]
}

pub(in crate::ai_assistant) fn normalize_state(mut state: AssistantState) -> AssistantState {
    if state.settings.providers.is_empty() {
        state.settings = default_assistant_settings();
    }
    if state.settings.model_catalog.is_empty() {
        state.settings.model_catalog = build_model_catalog_from_profiles(&state.settings);
    }
    if state.settings.runtime_presets.is_empty() {
        state.settings.runtime_presets = default_runtime_presets();
    }
    if state.settings.role_bindings.is_empty() {
        state.settings.role_bindings = build_default_role_bindings(&state.settings);
    }
    for binding in &mut state.settings.role_bindings {
        if binding.runtime_preset_id.is_none() {
            binding.runtime_preset_id = Some(
                match binding.role.as_str() {
                    "assistant" | "automation" | "selection_assistant" => "deep_reasoning",
                    "summary" | "translate" | "topic_naming" => "lightweight",
                    _ => "balanced",
                }
                .to_string(),
            );
        }
    }
    if state.agents.is_empty() {
        state.agents = default_agents();
    }
    for agent in &mut state.agents {
        if agent.primary_model_id.is_none() {
            agent.primary_model_id = legacy_profile_catalog_id(
                &state.settings,
                agent.default_model_profile_id.as_deref(),
            );
        }
        if agent.light_model_id.is_none() {
            agent.light_model_id =
                legacy_profile_catalog_id(&state.settings, agent.light_model_profile_id.as_deref());
        }
    }
    for conversation in &mut state.conversations {
        if conversation.model_override_id.is_none() {
            conversation.model_override_id = legacy_profile_catalog_id(
                &state.settings,
                conversation.model_profile_id.as_deref(),
            );
        }
    }
    for schedule in &mut state.schedules {
        if schedule.assistant_id.is_none() && !schedule.agent_id.trim().is_empty() {
            schedule.assistant_id = Some(schedule.agent_id.clone());
        }
    }
    if state.quick_assistant.preferred_role.trim().is_empty() {
        state.quick_assistant.preferred_role = "quick_assistant".to_string();
    }
    if state.selection_assistant.preferred_role.trim().is_empty() {
        state.selection_assistant.preferred_role = "selection_assistant".to_string();
    }
    state.revision = state.revision.max(now_ts());
    state
}

pub(in crate::ai_assistant) fn process_state_sensitive_data(
    state: &mut AssistantState,
    encrypt: bool,
) -> Result<(), String> {
    let password = crate::crypto::get_or_init_master_password()?;

    for provider in &mut state.settings.providers {
        if provider.api_key.trim().is_empty() {
            continue;
        }
        if encrypt {
            provider.api_key = crate::crypto::encrypt(&provider.api_key, &password)?;
        } else if let Ok(decrypted) = crate::crypto::decrypt(&provider.api_key, &password) {
            provider.api_key = decrypted;
        }
    }

    state.is_encrypted = encrypt;
    Ok(())
}

pub(in crate::ai_assistant) fn load_state() -> Result<AssistantState, String> {
    let _guard = state_lock()
        .lock()
        .map_err(|_| "assistant state lock poisoned".to_string())?;
    let path = state_path()?;
    if !path.exists() {
        return Ok(AssistantState::default());
    }
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if raw.trim().is_empty() {
        return Ok(AssistantState::default());
    }
    let mut state = serde_json::from_str::<AssistantState>(&raw).map_err(|e| e.to_string())?;
    if state.is_encrypted {
        let _ = process_state_sensitive_data(&mut state, false);
    }
    Ok(normalize_state(state))
}

pub(in crate::ai_assistant) fn save_state(state: &AssistantState) -> Result<(), String> {
    let _guard = state_lock()
        .lock()
        .map_err(|_| "assistant state lock poisoned".to_string())?;
    let path = state_path()?;
    let mut state_to_save = normalize_state(state.clone());
    process_state_sensitive_data(&mut state_to_save, true)?;
    let content = serde_json::to_string_pretty(&state_to_save).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| e.to_string())
}
