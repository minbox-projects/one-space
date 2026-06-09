use super::{
    AgentDefinition, AiAssistantModelProfile, AiAssistantProvider, AiAssistantSettings,
    AssistantCapabilitySnapshot, AssistantConversation, AssistantConversationListItem,
    AssistantState, ModelCatalogItem, ModelRoleBinding, RuntimePreset, ScheduleJob,
};
use serde_json::{json, Value};

pub(in crate::ai_assistant) fn derive_title(content: &str) -> String {
    let first = content
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("新会话");
    let trimmed = first.trim();
    let mut out = String::new();
    for ch in trimmed.chars().take(28) {
        out.push(ch);
    }
    if out.is_empty() {
        "新会话".to_string()
    } else {
        out
    }
}

pub(in crate::ai_assistant) fn build_model_params(profile: &AiAssistantModelProfile) -> Value {
    let mut params = json!({
        "temperature": profile.temperature.unwrap_or(0.3),
        "max_tokens": profile.max_tokens.unwrap_or(2048),
    });

    if let Some(top_p) = profile.top_p {
        params["top_p"] = json!(top_p);
    }
    if let Some(freq_penalty) = profile.frequency_penalty {
        params["frequency_penalty"] = json!(freq_penalty);
    }
    if let Some(pres_penalty) = profile.presence_penalty {
        params["presence_penalty"] = json!(pres_penalty);
    }
    if let Some(ref stop) = profile.stop_sequences {
        if !stop.is_empty() {
            params["stop"] = json!(stop);
        }
    }

    params
}

pub(in crate::ai_assistant) fn conversation_list_item(
    conversation: &AssistantConversation,
) -> AssistantConversationListItem {
    let preview = conversation
        .messages
        .iter()
        .rev()
        .find(|message| message.role != "context_reset")
        .map(|message| {
            let mut value = message.content.trim().to_string();
            if value.chars().count() > 80 {
                value = value.chars().take(80).collect::<String>();
            }
            value
        })
        .unwrap_or_default();
    let search_text = conversation
        .messages
        .iter()
        .filter(|message| message.role != "context_reset")
        .map(|message| {
            let reasoning = message.reasoning.clone().unwrap_or_default();
            format!("{} {}", message.content, reasoning)
        })
        .collect::<Vec<_>>()
        .join("\n");

    AssistantConversationListItem {
        id: conversation.id.clone(),
        title: conversation.title.clone(),
        pinned: conversation.pinned,
        archived: conversation.archived,
        created_at: conversation.created_at,
        updated_at: conversation.updated_at,
        message_count: conversation.messages.len(),
        preview,
        search_text,
        assistant_id: conversation.assistant_id.clone(),
        model_profile_id: conversation.model_profile_id.clone(),
        model_override_id: conversation.model_override_id.clone(),
        web_search_enabled: conversation.web_search_enabled,
        context_reset_count: conversation.context_reset_count,
    }
}

pub(in crate::ai_assistant) fn resolve_provider<'a>(
    state: &'a AssistantState,
    profile: &AiAssistantModelProfile,
) -> Result<&'a AiAssistantProvider, String> {
    state
        .settings
        .providers
        .iter()
        .find(|provider| provider.id == profile.provider_id)
        .ok_or_else(|| format!("Model provider not found: {}", profile.provider_id))
}

pub(in crate::ai_assistant) fn find_catalog_item<'a>(
    settings: &'a AiAssistantSettings,
    model_id: Option<&str>,
) -> Option<&'a ModelCatalogItem> {
    let model_id = model_id?.trim();
    if model_id.is_empty() {
        return None;
    }
    settings
        .model_catalog
        .iter()
        .find(|item| item.id == model_id && item.enabled)
}

pub(in crate::ai_assistant) fn find_role_binding<'a>(
    settings: &'a AiAssistantSettings,
    role: &str,
) -> Option<&'a ModelRoleBinding> {
    settings
        .role_bindings
        .iter()
        .find(|binding| binding.role == role)
}

pub(in crate::ai_assistant) fn find_runtime_preset<'a>(
    settings: &'a AiAssistantSettings,
    preset_id: Option<&str>,
) -> Option<&'a RuntimePreset> {
    let preset_id = preset_id?.trim();
    if preset_id.is_empty() {
        return None;
    }
    settings
        .runtime_presets
        .iter()
        .find(|preset| preset.id == preset_id)
}

pub(in crate::ai_assistant) fn runtime_profile_from_catalog(
    settings: &AiAssistantSettings,
    item: &ModelCatalogItem,
    binding: Option<&ModelRoleBinding>,
) -> AiAssistantModelProfile {
    let preset =
        binding.and_then(|value| find_runtime_preset(settings, value.runtime_preset_id.as_deref()));
    AiAssistantModelProfile {
        id: binding
            .map(|value| format!("binding::{}", value.id))
            .unwrap_or_else(|| format!("model::{}", item.id)),
        name: item.label.clone(),
        provider_id: item.provider_id.clone(),
        model_id: item.model_id.clone(),
        usage: binding
            .map(|value| value.role.clone())
            .unwrap_or_else(|| "assistant".to_string()),
        temperature: binding
            .and_then(|value| value.temperature)
            .or_else(|| preset.and_then(|value| value.temperature)),
        top_p: None,
        max_tokens: binding
            .and_then(|value| value.max_tokens)
            .or_else(|| preset.and_then(|value| value.max_tokens)),
        frequency_penalty: None,
        presence_penalty: None,
        stop_sequences: None,
        enable_reasoning: binding
            .map(|value| value.enable_reasoning)
            .or_else(|| preset.map(|value| value.enable_reasoning))
            .unwrap_or(item.supports_reasoning),
    }
}

pub(in crate::ai_assistant) fn resolve_runtime_profile(
    state: &AssistantState,
    explicit_model_id: Option<&str>,
    assistant: Option<&AgentDefinition>,
    role: &str,
) -> Result<AiAssistantModelProfile, String> {
    if let Some(model) = find_catalog_item(&state.settings, explicit_model_id) {
        let binding = find_role_binding(&state.settings, role)
            .filter(|binding| binding.model_id.as_deref() == Some(model.id.as_str()));
        return Ok(runtime_profile_from_catalog(
            &state.settings,
            model,
            binding,
        ));
    }

    if let Some(assistant) = assistant {
        let assistant_model_id = match role {
            "summary" | "translate" | "topic_naming" => assistant
                .light_model_id
                .as_deref()
                .or(assistant.primary_model_id.as_deref()),
            _ => assistant
                .primary_model_id
                .as_deref()
                .or(assistant.light_model_id.as_deref()),
        };
        if let Some(model) = find_catalog_item(&state.settings, assistant_model_id) {
            return Ok(runtime_profile_from_catalog(&state.settings, model, None));
        }
    }

    if let Some(binding) = find_role_binding(&state.settings, role) {
        if let Some(model) = find_catalog_item(&state.settings, binding.model_id.as_deref()) {
            return Ok(runtime_profile_from_catalog(
                &state.settings,
                model,
                Some(binding),
            ));
        }
    }

    if let Some(model) = state
        .settings
        .model_catalog
        .iter()
        .find(|item| item.enabled)
    {
        return Ok(runtime_profile_from_catalog(&state.settings, model, None));
    }

    Err("No enabled AI workspace model found".to_string())
}

pub(in crate::ai_assistant) fn capability_snapshot_from_agent(
    agent: Option<&AgentDefinition>,
    web_search_enabled: bool,
) -> AssistantCapabilitySnapshot {
    match agent {
        Some(agent) => AssistantCapabilitySnapshot {
            web_search: web_search_enabled,
            workspace_read: agent.tool_policy.workspace_read,
            notes_search: agent.tool_policy.notes_search,
            knowledge_base_ids: agent.knowledge_base_ids.clone(),
            mcp_server_ids: agent.mcp_server_ids.clone(),
            memory_enabled: agent.memory_enabled,
        },
        None => AssistantCapabilitySnapshot {
            web_search: web_search_enabled,
            ..AssistantCapabilitySnapshot::default()
        },
    }
}

pub(in crate::ai_assistant) fn schedule_assistant_id(schedule: &ScheduleJob) -> Option<&str> {
    schedule
        .assistant_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            if schedule.agent_id.trim().is_empty() {
                None
            } else {
                Some(schedule.agent_id.as_str())
            }
        })
}

pub(in crate::ai_assistant) fn build_context_messages(
    conversation: &AssistantConversation,
) -> Vec<(String, String)> {
    let reset_index = conversation
        .messages
        .iter()
        .rposition(|message| message.role == "context_reset");
    let start_index = reset_index.map(|idx| idx + 1).unwrap_or(0);
    conversation.messages[start_index..]
        .iter()
        .filter_map(|message| {
            if message.role == "user" || message.role == "assistant" {
                Some((message.role.clone(), message.content.clone()))
            } else {
                None
            }
        })
        .collect()
}

pub(in crate::ai_assistant) fn latest_user_message_text(
    state: &AssistantState,
    conversation_id: &str,
) -> Option<String> {
    let conversation = state
        .conversations
        .iter()
        .find(|item| item.id == conversation_id)?;
    let reset_index = conversation
        .messages
        .iter()
        .rposition(|message| message.role == "context_reset");
    let start_index = reset_index.map(|idx| idx + 1).unwrap_or(0);
    conversation.messages[start_index..]
        .iter()
        .rev()
        .find(|message| message.role == "user" && !message.content.trim().is_empty())
        .map(|message| message.content.trim().to_string())
}
