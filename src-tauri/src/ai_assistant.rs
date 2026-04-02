use crate::get_data_dir;
use crate::mcp_runtime::{compose_mcp_tool_name, McpClient, McpToolCallOutput};
use chrono::{Datelike, Timelike, Weekday};
use regex::Regex;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Emitter;

const ASSISTANT_STREAM_EVENT: &str = "assistant-stream";
const DEFAULT_STATE_FILE: &str = "ai_workspace_state.json";

static STATE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static RUNNING_SCHEDULES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
static SCHEDULER_STARTED: OnceLock<()> = OnceLock::new();

fn state_lock() -> &'static Mutex<()> {
    STATE_LOCK.get_or_init(|| Mutex::new(()))
}

fn running_schedules() -> &'static Mutex<HashSet<String>> {
    RUNNING_SCHEDULES.get_or_init(|| Mutex::new(HashSet::new()))
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn state_path() -> Result<PathBuf, String> {
    Ok(get_data_dir()?.join(DEFAULT_STATE_FILE))
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AssistantProviderCapability {
    #[serde(default)]
    pub supports_reasoning: bool,
    #[serde(default = "default_true")]
    pub supports_streaming: bool,
    #[serde(default)]
    pub supports_web_search: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AiAssistantProvider {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub base_url: String,
    #[serde(default = "default_bearer")]
    pub auth_scheme: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub extra_headers: Vec<ProviderHeader>,
    #[serde(default)]
    pub capabilities: AssistantProviderCapability,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ProviderHeader {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AiAssistantModelProfile {
    pub id: String,
    pub name: String,
    pub provider_id: String,
    pub model_id: String,
    #[serde(default)]
    pub usage: String,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub frequency_penalty: Option<f32>,
    #[serde(default)]
    pub presence_penalty: Option<f32>,
    #[serde(default)]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(default)]
    pub enable_reasoning: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AiAssistantSettings {
    #[serde(default)]
    pub providers: Vec<AiAssistantProvider>,
    #[serde(default)]
    pub profiles: Vec<AiAssistantModelProfile>,
    #[serde(default)]
    pub model_catalog: Vec<ModelCatalogItem>,
    #[serde(default)]
    pub role_bindings: Vec<ModelRoleBinding>,
    #[serde(default)]
    pub runtime_presets: Vec<RuntimePreset>,
    #[serde(default)]
    pub default_chat_profile_id: Option<String>,
    #[serde(default)]
    pub default_agent_profile_id: Option<String>,
    #[serde(default)]
    pub default_summary_profile_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ModelCatalogItem {
    pub id: String,
    pub provider_id: String,
    pub model_id: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub supports_reasoning: bool,
    #[serde(default = "default_true")]
    pub supports_streaming: bool,
    #[serde(default)]
    pub supports_web_search: bool,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ModelRoleBinding {
    pub id: String,
    pub role: String,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub runtime_preset_id: Option<String>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub enable_reasoning: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct RuntimePreset {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub enable_reasoning: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AssistantCapabilitySnapshot {
    #[serde(default)]
    pub web_search: bool,
    #[serde(default)]
    pub workspace_read: bool,
    #[serde(default)]
    pub notes_search: bool,
    #[serde(default)]
    pub knowledge_base_ids: Vec<String>,
    #[serde(default)]
    pub mcp_server_ids: Vec<String>,
    #[serde(default)]
    pub memory_enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct QuickAssistantPreferences {
    #[serde(default)]
    pub preferred_assistant_id: Option<String>,
    #[serde(default)]
    pub preferred_role: String,
    #[serde(default = "default_true")]
    pub prefer_assistant_mode: bool,
    #[serde(default)]
    pub read_clipboard_on_open: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SelectionAssistantPreferences {
    #[serde(default)]
    pub preferred_assistant_id: Option<String>,
    #[serde(default)]
    pub preferred_role: String,
    #[serde(default = "default_true")]
    pub prefer_assistant_mode: bool,
    #[serde(default = "default_true")]
    pub read_clipboard_on_open: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AssistantMessageSource {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AssistantToolCall {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub arguments: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub started_at: u64,
    #[serde(default)]
    pub finished_at: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub parameters: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AssistantMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub reasoning: Option<String>,
    #[serde(default)]
    pub sources: Vec<AssistantMessageSource>,
    #[serde(default)]
    pub tool_calls: Vec<AssistantToolCall>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub schedule_draft: Option<AssistantScheduleDraft>,
    pub created_at: u64,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AssistantScheduleDraft {
    pub action: String,
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub schedule: Option<ScheduleJob>,
    #[serde(default)]
    pub target_schedule_id: Option<String>,
    #[serde(default)]
    pub target_schedule_name: Option<String>,
    #[serde(default)]
    pub desired_enabled: Option<bool>,
    #[serde(default)]
    pub agent_name: Option<String>,
    #[serde(default)]
    pub trigger_label: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AssistantConversation {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub archived: bool,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub assistant_id: Option<String>,
    #[serde(default)]
    pub model_profile_id: Option<String>,
    #[serde(default)]
    pub model_override_id: Option<String>,
    #[serde(default)]
    pub web_search_enabled: bool,
    #[serde(default)]
    pub capability_snapshot: Option<AssistantCapabilitySnapshot>,
    #[serde(default)]
    pub context_reset_count: u32,
    #[serde(default)]
    pub messages: Vec<AssistantMessage>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AssistantConversationListItem {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub archived: bool,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub message_count: usize,
    #[serde(default)]
    pub preview: String,
    #[serde(default)]
    pub search_text: String,
    #[serde(default)]
    pub assistant_id: Option<String>,
    #[serde(default)]
    pub model_profile_id: Option<String>,
    #[serde(default)]
    pub model_override_id: Option<String>,
    #[serde(default)]
    pub web_search_enabled: bool,
    #[serde(default)]
    pub context_reset_count: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AgentToolPolicy {
    #[serde(default)]
    pub web_search: bool,
    #[serde(default)]
    pub workspace_read: bool,
    #[serde(default)]
    pub notes_search: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentDefinition {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub avatar_emoji: Option<String>,
    #[serde(default)]
    pub description: String,
    pub system_prompt: String,
    #[serde(default)]
    pub primary_model_id: Option<String>,
    #[serde(default)]
    pub light_model_id: Option<String>,
    #[serde(default)]
    pub default_model_profile_id: Option<String>,
    #[serde(default)]
    pub light_model_profile_id: Option<String>,
    #[serde(default)]
    pub tool_policy: AgentToolPolicy,
    #[serde(default)]
    pub knowledge_base_ids: Vec<String>,
    #[serde(default)]
    pub mcp_server_ids: Vec<String>,
    #[serde(default)]
    pub memory_enabled: bool,
    #[serde(default)]
    pub output_contract: String,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScheduleTrigger {
    pub kind: String,
    #[serde(default)]
    pub interval_minutes: Option<u64>,
    #[serde(default)]
    pub time_of_day: Option<String>,
    #[serde(default)]
    pub weekdays: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScheduleJob {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub assistant_id: Option<String>,
    pub agent_id: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub model_profile_id: Option<String>,
    #[serde(default)]
    pub model_override_id: Option<String>,
    #[serde(default)]
    pub web_search_enabled: bool,
    pub trigger: ScheduleTrigger,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub output_target: String,
    #[serde(default)]
    pub conversation_id: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub next_run_at: Option<u64>,
    #[serde(default)]
    pub last_run_at: Option<u64>,
    #[serde(default)]
    pub last_status: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub misfire_policy: String,
    #[serde(default)]
    pub max_retries: u32,
    #[serde(default)]
    pub retry_count: u32,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScheduleRun {
    pub id: String,
    pub schedule_id: String,
    pub started_at: u64,
    #[serde(default)]
    pub ended_at: Option<u64>,
    pub status: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub conversation_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScheduleJobView {
    #[serde(flatten)]
    pub job: ScheduleJob,
    #[serde(default)]
    pub recent_runs: Vec<ScheduleRun>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AssistantState {
    #[serde(default)]
    pub settings: AiAssistantSettings,
    #[serde(default)]
    pub conversations: Vec<AssistantConversation>,
    #[serde(default)]
    pub agents: Vec<AgentDefinition>,
    #[serde(default)]
    pub schedules: Vec<ScheduleJob>,
    #[serde(default)]
    pub runs: Vec<ScheduleRun>,
    #[serde(default)]
    pub quick_assistant: QuickAssistantPreferences,
    #[serde(default)]
    pub selection_assistant: SelectionAssistantPreferences,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub is_encrypted: bool,
}

impl Default for AssistantState {
    fn default() -> Self {
        Self {
            settings: default_assistant_settings(),
            conversations: Vec::new(),
            agents: default_agents(),
            schedules: Vec::new(),
            runs: Vec::new(),
            quick_assistant: QuickAssistantPreferences {
                preferred_assistant_id: None,
                preferred_role: "quick_assistant".to_string(),
                prefer_assistant_mode: true,
                read_clipboard_on_open: false,
            },
            selection_assistant: SelectionAssistantPreferences {
                preferred_assistant_id: None,
                preferred_role: "selection_assistant".to_string(),
                prefer_assistant_mode: true,
                read_clipboard_on_open: true,
            },
            revision: now_ts(),
            is_encrypted: false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AssistantSendResult {
    pub conversation_id: String,
    pub user_message_id: String,
    pub assistant_message_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScheduleDraftResolveInput {
    pub conversation_id: String,
    pub message_id: String,
    pub approved: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AssistantStreamEvent {
    pub conversation_id: String,
    pub message_id: String,
    pub kind: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub sources: Option<Vec<AssistantMessageSource>>,
    #[serde(default)]
    pub tool: Option<AssistantToolCall>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentTestRunInput {
    pub agent_id: String,
    pub prompt: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentTestRunResult {
    pub conversation_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScheduleToggleInput {
    pub schedule_id: String,
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScheduleRunNowInput {
    pub schedule_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AssistantConnectionTestResult {
    pub ok: bool,
    pub message: String,
    pub latency_ms: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProviderConnectionTestInput {
    pub provider_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProviderModelsFetchInput {
    pub provider_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkspaceConversationCreateInput {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub assistant_id: Option<String>,
    #[serde(default)]
    pub model_override_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkspaceConversationUpdateInput {
    pub conversation_id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub pinned: Option<bool>,
    #[serde(default)]
    pub archived: Option<bool>,
    #[serde(default)]
    pub assistant_id: Option<String>,
    #[serde(default)]
    pub model_override_id: Option<String>,
    #[serde(default)]
    pub web_search_enabled: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkspaceConversationSendInput {
    pub conversation_id: String,
    pub content: String,
    #[serde(default)]
    pub assistant_id: Option<String>,
    #[serde(default)]
    pub model_override_id: Option<String>,
    #[serde(default)]
    pub web_search_enabled: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AiWorkspaceBootstrap {
    pub settings: AiAssistantSettings,
    pub assistants: Vec<AgentDefinition>,
    pub conversations: Vec<AssistantConversationListItem>,
    pub automations: Vec<ScheduleJobView>,
    pub quick_assistant: QuickAssistantPreferences,
    pub selection_assistant: SelectionAssistantPreferences,
}

fn default_true() -> bool {
    true
}

fn default_bearer() -> String {
    "bearer".to_string()
}

fn catalog_model_id(provider_id: &str, model_id: &str) -> String {
    format!("{}::{}", provider_id.trim(), model_id.trim())
}

fn workspace_roles() -> [&'static str; 8] {
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

fn legacy_profile_catalog_id(settings: &AiAssistantSettings, profile_id: Option<&str>) -> Option<String> {
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

fn build_model_catalog_from_profiles(settings: &AiAssistantSettings) -> Vec<ModelCatalogItem> {
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
            supports_reasoning: provider_capabilities.supports_reasoning || profile.enable_reasoning,
            supports_streaming: provider_capabilities.supports_streaming,
            supports_web_search: provider_capabilities.supports_web_search,
            created_at: now,
            updated_at: now,
        });
    }

    catalog
}

fn default_role_model_id(settings: &AiAssistantSettings, role: &str) -> Option<String> {
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

fn default_runtime_presets() -> Vec<RuntimePreset> {
    vec![
        RuntimePreset {
            id: "balanced".to_string(),
            name: "Balanced".to_string(),
            description: "General-purpose preset for chat, quick assistant, and routine work.".to_string(),
            temperature: Some(0.3),
            max_tokens: Some(2048),
            enable_reasoning: true,
        },
        RuntimePreset {
            id: "deep_reasoning".to_string(),
            name: "Deep Reasoning".to_string(),
            description: "Longer responses and stronger reasoning for assistants and automations.".to_string(),
            temperature: Some(0.2),
            max_tokens: Some(4096),
            enable_reasoning: true,
        },
        RuntimePreset {
            id: "lightweight".to_string(),
            name: "Lightweight".to_string(),
            description: "Fast, low-cost preset for summaries, translation, and topic naming.".to_string(),
            temperature: Some(0.1),
            max_tokens: Some(1024),
            enable_reasoning: false,
        },
    ]
}

fn build_default_role_bindings(settings: &AiAssistantSettings) -> Vec<ModelRoleBinding> {
    workspace_roles()
        .into_iter()
        .map(|role| ModelRoleBinding {
            id: role.to_string(),
            role: role.to_string(),
            model_id: default_role_model_id(settings, role),
            runtime_preset_id: Some(match role {
                "assistant" | "automation" | "selection_assistant" => "deep_reasoning",
                "summary" | "translate" | "topic_naming" => "lightweight",
                _ => "balanced",
            }
            .to_string()),
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

fn default_assistant_settings() -> AiAssistantSettings {
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

fn default_agents() -> Vec<AgentDefinition> {
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

fn normalize_state(mut state: AssistantState) -> AssistantState {
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
            agent.primary_model_id = legacy_profile_catalog_id(&state.settings, agent.default_model_profile_id.as_deref());
        }
        if agent.light_model_id.is_none() {
            agent.light_model_id = legacy_profile_catalog_id(&state.settings, agent.light_model_profile_id.as_deref());
        }
    }
    for conversation in &mut state.conversations {
        if conversation.model_override_id.is_none() {
            conversation.model_override_id =
                legacy_profile_catalog_id(&state.settings, conversation.model_profile_id.as_deref());
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

fn process_state_sensitive_data(state: &mut AssistantState, encrypt: bool) -> Result<(), String> {
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

fn load_state() -> Result<AssistantState, String> {
    let _guard = state_lock().lock().map_err(|_| "assistant state lock poisoned".to_string())?;
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

fn save_state(state: &AssistantState) -> Result<(), String> {
    let _guard = state_lock().lock().map_err(|_| "assistant state lock poisoned".to_string())?;
    let path = state_path()?;
    let mut state_to_save = normalize_state(state.clone());
    process_state_sensitive_data(&mut state_to_save, true)?;
    let content = serde_json::to_string_pretty(&state_to_save).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| e.to_string())
}

fn derive_title(content: &str) -> String {
    let first = content.lines().find(|line| !line.trim().is_empty()).unwrap_or("新会话");
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

fn build_model_params(profile: &AiAssistantModelProfile) -> Value {
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

fn chunk_text(text: &str, chunk_size: usize) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let chars = text.chars().collect::<Vec<_>>();
    chars
        .chunks(chunk_size.max(1))
        .map(|chunk| chunk.iter().collect::<String>())
        .collect()
}

fn conversation_list_item(conversation: &AssistantConversation) -> AssistantConversationListItem {
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

fn resolve_provider<'a>(
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

fn find_catalog_item<'a>(
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

fn find_role_binding<'a>(
    settings: &'a AiAssistantSettings,
    role: &str,
) -> Option<&'a ModelRoleBinding> {
    settings
        .role_bindings
        .iter()
        .find(|binding| binding.role == role)
}

fn find_runtime_preset<'a>(
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

fn runtime_profile_from_catalog(
    settings: &AiAssistantSettings,
    item: &ModelCatalogItem,
    binding: Option<&ModelRoleBinding>,
) -> AiAssistantModelProfile {
    let preset = binding.and_then(|value| find_runtime_preset(settings, value.runtime_preset_id.as_deref()));
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

fn resolve_runtime_profile(
    state: &AssistantState,
    explicit_model_id: Option<&str>,
    assistant: Option<&AgentDefinition>,
    role: &str,
) -> Result<AiAssistantModelProfile, String> {
    if let Some(model) = find_catalog_item(&state.settings, explicit_model_id) {
        let binding = find_role_binding(&state.settings, role)
            .filter(|binding| binding.model_id.as_deref() == Some(model.id.as_str()));
        return Ok(runtime_profile_from_catalog(&state.settings, model, binding));
    }

    if let Some(assistant) = assistant {
        let assistant_model_id = match role {
            "summary" | "translate" | "topic_naming" => {
                assistant.light_model_id.as_deref().or(assistant.primary_model_id.as_deref())
            }
            _ => assistant.primary_model_id.as_deref().or(assistant.light_model_id.as_deref()),
        };
        if let Some(model) = find_catalog_item(&state.settings, assistant_model_id) {
            return Ok(runtime_profile_from_catalog(&state.settings, model, None));
        }
    }

    if let Some(binding) = find_role_binding(&state.settings, role) {
        if let Some(model) = find_catalog_item(&state.settings, binding.model_id.as_deref()) {
            return Ok(runtime_profile_from_catalog(&state.settings, model, Some(binding)));
        }
    }

    if let Some(model) = state.settings.model_catalog.iter().find(|item| item.enabled) {
        return Ok(runtime_profile_from_catalog(&state.settings, model, None));
    }

    Err("No enabled AI workspace model found".to_string())
}

fn capability_snapshot_from_agent(
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

fn schedule_assistant_id(schedule: &ScheduleJob) -> Option<&str> {
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

fn build_context_messages(conversation: &AssistantConversation) -> Vec<(String, String)> {
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

fn build_reqwest_client(timeout_secs: Option<u64>) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder();
    if let Some(timeout) = timeout_secs {
        builder = builder.timeout(std::time::Duration::from_secs(timeout));
    }
    builder.build().map_err(|e| e.to_string())
}

fn interval_minutes_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"每\s*(\d+)\s*(分钟|小时)").expect("valid interval regex"))
}

fn time_of_day_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?P<hour>\d{1,2})[:：](?P<minute>\d{2})").expect("valid time regex"))
}

fn quoted_name_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?:名为|叫做|叫|标题为|任务名(?:称)?(?:为)?)[\s:：]*["“]?([^"”\n]+)["”]?"#)
            .expect("valid quoted name regex")
    })
}

fn apply_provider_headers(
    request: reqwest::RequestBuilder,
    provider: &AiAssistantProvider,
) -> Result<reqwest::RequestBuilder, String> {
    let mut request = request;
    if !provider.api_key.trim().is_empty() {
        match provider.auth_scheme.as_str() {
            "x-api-key" => {
                request = request.header("x-api-key", provider.api_key.clone());
            }
            "x-goog-api-key" => {
                request = request.header("x-goog-api-key", provider.api_key.clone());
            }
            _ => {
                request = request.header(AUTHORIZATION, format!("Bearer {}", provider.api_key));
            }
        }
    }
    let mut header_map = HeaderMap::new();
    for header in &provider.extra_headers {
        if header.key.trim().is_empty() || header.value.trim().is_empty() {
            continue;
        }
        let key = HeaderName::from_bytes(header.key.trim().as_bytes()).map_err(|e| e.to_string())?;
        let value = HeaderValue::from_str(header.value.trim()).map_err(|e| e.to_string())?;
        header_map.insert(key, value);
    }
    Ok(request.headers(header_map))
}

fn resolve_endpoint(base_url: &str, suffix: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with(suffix) {
        trimmed.to_string()
    } else {
        format!("{trimmed}/{suffix}")
    }
}

fn normalize_openai_compatible_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    let normalized = [
        "chat/completions",
        "responses",
        "completions",
        "embeddings",
        "audio/speech",
        "audio/transcriptions",
    ]
    .into_iter()
    .find_map(|suffix| trimmed.strip_suffix(suffix))
    .map(|prefix| prefix.trim_end_matches('/'))
    .unwrap_or(trimmed);

    normalized.to_string()
}

fn resolve_provider_endpoint(provider: &AiAssistantProvider, suffix: &str) -> String {
    match provider.protocol.as_str() {
        "openai-compatible" => {
            let normalized = normalize_openai_compatible_base_url(&provider.base_url);
            resolve_endpoint(&normalized, suffix)
        }
        _ => resolve_endpoint(&provider.base_url, suffix),
    }
}

fn build_builtin_tools(tool_policy: &AgentToolPolicy) -> Vec<ToolDefinition> {
    let mut tools = Vec::new();
    if tool_policy.workspace_read {
        tools.push(ToolDefinition {
            name: "workspace_read".to_string(),
            description: "Read a file from the workspace. Returns the file content.".to_string(),
            parameters: Some(json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The file path to read (relative to workspace root or absolute)"
                    }
                },
                "required": ["path"]
            })),
        });
    }
    if tool_policy.notes_search {
        tools.push(ToolDefinition {
            name: "notes_search".to_string(),
            description: "Search through user's notes. Returns matching note fragments.".to_string(),
            parameters: Some(json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query to find in notes"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return",
                        "default": 5
                    }
                },
                "required": ["query"]
            })),
        });
    }
    tools
}

#[derive(Debug, Clone)]
struct BoundMcpTool {
    assistant_tool_name: String,
    server_id: String,
    server_name: String,
    config_key: String,
    original_tool_name: String,
    category: crate::assistant_mcp::McpCategory,
    definition: ToolDefinition,
}

fn build_available_tools(
    tool_policy: &AgentToolPolicy,
    mcp_tools: &[BoundMcpTool],
) -> Vec<ToolDefinition> {
    let mut tools = build_builtin_tools(tool_policy);
    tools.extend(mcp_tools.iter().map(|item| item.definition.clone()));
    tools
}

async fn load_bound_mcp_tools(
    agent: Option<&AgentDefinition>,
    search_enabled: bool,
) -> Result<(HashMap<String, McpClient>, HashMap<String, BoundMcpTool>), String> {
    let Some(agent) = agent else {
        return Ok((HashMap::new(), HashMap::new()));
    };
    if agent.mcp_server_ids.is_empty() {
        return Ok((HashMap::new(), HashMap::new()));
    }

    let state = crate::mcp_servers::get_mcp_servers()?;
    let mut servers_by_id = HashMap::new();
    for server in state.servers {
        servers_by_id.insert(server.id.clone(), server);
    }

    let mut clients = HashMap::new();
    let mut tools = HashMap::new();
    let mut seen_servers = HashSet::new();

    for server_id in &agent.mcp_server_ids {
        if !seen_servers.insert(server_id.clone()) {
            continue;
        }
        let Some(server) = servers_by_id.get(server_id).cloned() else {
            continue;
        };
        let category = crate::assistant_mcp::category_for_server(&server);
        if matches!(category, crate::assistant_mcp::McpCategory::Search) && !search_enabled {
            continue;
        }

        let config_key = server
            .config_key
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| server.id.clone());

        let mut client = match McpClient::connect(&server).await {
            Ok(client) => client,
            Err(error) => {
                eprintln!("failed to initialize MCP server '{}': {}", server.name, error);
                continue;
            }
        };

        let listed_tools = match client.list_tools().await {
            Ok(tools) => tools,
            Err(error) => {
                eprintln!("failed to list tools for MCP server '{}': {}", server.name, error);
                client.close().await;
                continue;
            }
        };

        for tool in listed_tools {
            let assistant_tool_name = compose_mcp_tool_name(&config_key, &tool.name);
            tools.insert(
                assistant_tool_name.clone(),
                BoundMcpTool {
                    assistant_tool_name: assistant_tool_name.clone(),
                    server_id: server.id.clone(),
                    server_name: server.name.clone(),
                    config_key: config_key.clone(),
                    original_tool_name: tool.name.clone(),
                    category,
                    definition: ToolDefinition {
                        name: assistant_tool_name,
                        description: if tool.description.trim().is_empty() {
                            format!("MCP tool '{}' from {}", tool.name, server.name)
                        } else {
                            tool.description.clone()
                        },
                        parameters: Some(tool.input_schema.clone()),
                    },
                },
            );
        }

        clients.insert(server.id.clone(), client);
    }

    Ok((clients, tools))
}

async fn close_mcp_clients(clients: &mut HashMap<String, McpClient>) {
    let mut owned = clients.drain().map(|(_, client)| client).collect::<Vec<_>>();
    for client in &mut owned {
        client.close().await;
    }
}

fn is_exa_mcp_tool(binding: &BoundMcpTool) -> bool {
    binding.config_key == "exa"
        || binding.original_tool_name.contains("_exa")
        || (binding.server_name.to_lowercase().contains("exa")
            && matches!(binding.category, crate::assistant_mcp::McpCategory::Search))
}

fn extract_sources_from_mcp_output(
    binding: &BoundMcpTool,
    output: &McpToolCallOutput,
) -> Vec<AssistantMessageSource> {
    if !is_exa_mcp_tool(binding) {
        return Vec::new();
    }

    let from_value = output
        .structured_content
        .as_ref()
        .map(extract_sources_from_value)
        .filter(|items| !items.is_empty())
        .or_else(|| {
            let items = extract_sources_from_value(&output.raw_result);
            if items.is_empty() {
                None
            } else {
                Some(items)
            }
        });

    if let Some(items) = from_value {
        return items;
    }

    serde_json::from_str::<Value>(&output.text)
        .ok()
        .map(|value| extract_sources_from_value(&value))
        .unwrap_or_default()
}

fn extract_sources_from_value(value: &Value) -> Vec<AssistantMessageSource> {
    for pointer in [
        "/results",
        "/data/results",
        "/searchResults",
        "/data/searchResults",
        "/items",
        "/data/items",
    ] {
        if let Some(items) = value.pointer(pointer).and_then(|entry| entry.as_array()) {
            let collected = collect_sources_from_items(items);
            if !collected.is_empty() {
                return collected;
            }
        }
    }

    value.as_array()
        .map(|items| collect_sources_from_items(items))
        .unwrap_or_default()
}

fn collect_sources_from_items(items: &[Value]) -> Vec<AssistantMessageSource> {
    items.iter()
        .filter_map(|item| {
            let url = item
                .get("url")
                .or_else(|| item.get("id"))
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            if url.is_empty() {
                return None;
            }

            let title = item
                .get("title")
                .or_else(|| item.get("name"))
                .and_then(|value| value.as_str())
                .unwrap_or_else(|| url.as_str())
                .trim()
                .to_string();

            let snippet = item
                .get("snippet")
                .or_else(|| item.get("text"))
                .or_else(|| item.get("summary"))
                .or_else(|| item.get("content"))
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .or_else(|| {
                    item.get("highlights")
                        .and_then(|value| value.as_array())
                        .map(|highlights| {
                            highlights
                                .iter()
                                .filter_map(|highlight| highlight.as_str())
                                .take(2)
                                .collect::<Vec<_>>()
                                .join(" ")
                        })
                        .filter(|value| !value.trim().is_empty())
                })
                .unwrap_or_default();

            Some(AssistantMessageSource { title, url, snippet })
        })
        .take(6)
        .collect()
}

async fn execute_tool_call(
    app: &tauri::AppHandle,
    _state: &AssistantState,
    tool_name: &str,
    arguments: &Value,
    conversation_id: &str,
    message_id: &str,
    mcp_tools: &HashMap<String, BoundMcpTool>,
    mcp_clients: &mut HashMap<String, McpClient>,
) -> Result<(String, Vec<AssistantMessageSource>), String> {
    let start = now_ts();
    let tool_id = uuid::Uuid::new_v4().to_string();

    let pending_tool = AssistantToolCall {
        id: tool_id.clone(),
        name: tool_name.to_string(),
        arguments: Some(arguments.to_string()),
        status: "running".to_string(),
        summary: None,
        result: None,
        started_at: start,
        finished_at: None,
    };

    emit_stream_event(
        app,
        AssistantStreamEvent {
            conversation_id: conversation_id.to_string(),
            message_id: message_id.to_string(),
            kind: "tool.started".to_string(),
            text: None,
            sources: None,
            tool: Some(pending_tool.clone()),
            error: None,
        },
    );

    let result = match tool_name {
        "workspace_read" => {
            let path = arguments
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "workspace_read requires 'path' argument".to_string())?;

            let data_dir = crate::get_data_dir()?;
            let file_path = if path.starts_with('/') {
                path.to_string()
            } else if path.starts_with('~') {
                let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
                path.replacen('~', &home, 1)
            } else {
                data_dir.join(path).to_string_lossy().to_string()
            };

            fs::read_to_string(&file_path)
                .map(|content| (content, Vec::new()))
                .map_err(|e| format!("Failed to read file {}: {}", file_path, e))
        }
        "notes_search" => {
            let _query = arguments
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "notes_search requires 'query' argument".to_string())?;
            let _limit = arguments
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(5) as usize;

            // TODO: Implement actual notes search after notes module integration
            Err("Notes search is not yet implemented. Please enable this feature in future updates.".to_string())
        }
        _ => {
            let binding = mcp_tools
                .get(tool_name)
                .ok_or_else(|| format!("Unknown tool: {}", tool_name))?;
            let client = mcp_clients
                .get_mut(&binding.server_id)
                .ok_or_else(|| format!("MCP server unavailable for tool '{}'", tool_name))?;
            client
                .call_tool(&binding.original_tool_name, arguments.clone())
                .await
                .map(|output| {
                    let sources = extract_sources_from_mcp_output(binding, &output);
                    (output.text, sources)
                })
        }
    };

    match result {
        Ok((result_text, sources)) => {
            let done_tool = AssistantToolCall {
                id: tool_id.clone(),
                name: tool_name.to_string(),
                arguments: Some(arguments.to_string()),
                status: "success".to_string(),
                summary: Some(format!("Tool executed successfully")),
                result: Some(result_text.clone()),
                started_at: start,
                finished_at: Some(now_ts()),
            };

            emit_stream_event(
                app,
                AssistantStreamEvent {
                    conversation_id: conversation_id.to_string(),
                    message_id: message_id.to_string(),
                    kind: "tool.finished".to_string(),
                    text: None,
                    sources: Some(sources.clone()),
                    tool: Some(done_tool.clone()),
                    error: None,
                },
            );

            Ok((result_text, sources))
        }
        Err(error) => {
            let failed_tool = AssistantToolCall {
                id: tool_id.clone(),
                name: tool_name.to_string(),
                arguments: Some(arguments.to_string()),
                status: "failed".to_string(),
                summary: Some(error.clone()),
                result: None,
                started_at: start,
                finished_at: Some(now_ts()),
            };

            emit_stream_event(
                app,
                AssistantStreamEvent {
                    conversation_id: conversation_id.to_string(),
                    message_id: message_id.to_string(),
                    kind: "tool.finished".to_string(),
                    text: None,
                    sources: None,
                    tool: Some(failed_tool.clone()),
                    error: Some(error.clone()),
                },
            );

            Err(error)
        }
    }
}

#[derive(Debug)]
struct ProviderCatalogFetchError {
    message: String,
    unsupported_catalog_endpoint: bool,
}

fn is_unsupported_model_catalog_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 404 | 405 | 501)
}

fn catalog_tags_from_model_id(model_id: &str) -> Vec<String> {
    let lower = model_id.to_lowercase();
    let mut tags = Vec::new();
    if lower.contains("mini") || lower.contains("small") {
        tags.push("light".to_string());
    }
    if lower.contains("reason") || lower.contains("o1") || lower.contains("o3") {
        tags.push("reasoning".to_string());
    }
    if lower.contains("vision") || lower.contains("vl") {
        tags.push("vision".to_string());
    }
    tags
}

fn parse_provider_model_catalog(
    provider: &AiAssistantProvider,
    payload: &Value,
) -> Vec<ModelCatalogItem> {
    let now = now_ts();
    let items = payload
        .get("data")
        .and_then(|value| value.as_array())
        .cloned()
        .or_else(|| {
            payload
                .get("data")
                .and_then(|value| value.get("models"))
                .and_then(|value| value.as_array())
                .cloned()
        })
        .or_else(|| payload.get("models").and_then(|value| value.as_array()).cloned())
        .or_else(|| payload.get("result").and_then(|value| value.as_array()).cloned())
        .unwrap_or_default();

    let mut seen = HashSet::new();
    let mut catalog = Vec::new();
    for item in items {
        let raw_id = item
            .get("id")
            .and_then(|value| value.as_str())
            .or_else(|| item.get("name").and_then(|value| value.as_str()))
            .unwrap_or("")
            .trim()
            .trim_start_matches("models/");
        if raw_id.is_empty() {
            continue;
        }
        let id = catalog_model_id(&provider.id, raw_id);
        if !seen.insert(id.clone()) {
            continue;
        }
        let label = item
            .get("display_name")
            .and_then(|value| value.as_str())
            .or_else(|| item.get("displayName").and_then(|value| value.as_str()))
            .unwrap_or(raw_id)
            .trim()
            .to_string();
        catalog.push(ModelCatalogItem {
            id,
            provider_id: provider.id.clone(),
            model_id: raw_id.to_string(),
            label,
            description: item
                .get("description")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string(),
            enabled: true,
            tags: catalog_tags_from_model_id(raw_id),
            supports_reasoning: provider.capabilities.supports_reasoning,
            supports_streaming: provider.capabilities.supports_streaming,
            supports_web_search: provider.capabilities.supports_web_search,
            created_at: now,
            updated_at: now,
        });
    }
    catalog
}

async fn fetch_provider_model_catalog_detailed(
    provider: &AiAssistantProvider,
) -> Result<Vec<ModelCatalogItem>, ProviderCatalogFetchError> {
    if provider.api_key.trim().is_empty() {
        return Err(ProviderCatalogFetchError {
            message: "Provider API key is empty".to_string(),
            unsupported_catalog_endpoint: false,
        });
    }
    let client = build_reqwest_client(Some(12)).map_err(|message| ProviderCatalogFetchError {
        message,
        unsupported_catalog_endpoint: false,
    })?;
    let endpoint = resolve_provider_endpoint(provider, "models");
    let mut request = client.get(endpoint);
    if provider.protocol == "anthropic-messages" {
        request = request.header("anthropic-version", "2023-06-01");
    }
    let request = apply_provider_headers(request, provider).map_err(|message| ProviderCatalogFetchError {
        message,
        unsupported_catalog_endpoint: false,
    })?;
    let response = request.send().await.map_err(|e| ProviderCatalogFetchError {
        message: e.to_string(),
        unsupported_catalog_endpoint: false,
    })?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let details = body.trim();
        let unsupported_catalog_endpoint = is_unsupported_model_catalog_status(status);
        let message = if details.is_empty() {
            format!("Provider model fetch failed: {}", status)
        } else {
            format!("Provider model fetch failed: {} - {}", status, details)
        };
        let message = if unsupported_catalog_endpoint {
            format!(
                "{}. This provider does not expose a standard model catalog endpoint.",
                message
            )
        } else {
            message
        };
        return Err(ProviderCatalogFetchError {
            message,
            unsupported_catalog_endpoint,
        });
    }
    let payload = response.json::<Value>().await.map_err(|e| ProviderCatalogFetchError {
        message: e.to_string(),
        unsupported_catalog_endpoint: false,
    })?;
    let catalog = parse_provider_model_catalog(provider, &payload);
    if catalog.is_empty() {
        return Err(ProviderCatalogFetchError {
            message: "Provider returned no models".to_string(),
            unsupported_catalog_endpoint: false,
        });
    }
    Ok(catalog)
}

async fn fetch_provider_model_catalog(
    provider: &AiAssistantProvider,
) -> Result<Vec<ModelCatalogItem>, String> {
    fetch_provider_model_catalog_detailed(provider)
        .await
        .map_err(|error| error.message)
}

fn text_from_openai_message(message: &Value) -> String {
    if let Some(text) = message.get("content").and_then(|content| content.as_str()) {
        return text.to_string();
    }
    if let Some(items) = message.get("content").and_then(|content| content.as_array()) {
        return items
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .and_then(|value| value.as_str())
                    .or_else(|| item.get("content").and_then(|value| value.as_str()))
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    String::new()
}

fn reasoning_from_openai_message(message: &Value) -> Option<String> {
    message
        .get("reasoning")
        .and_then(value_to_text)
        .or_else(|| message.get("reasoning_content").and_then(value_to_text))
        .or_else(|| message.get("reasoning_summary").and_then(value_to_text))
}

fn value_to_text(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    if let Some(items) = value.as_array() {
        let joined = items
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .and_then(|val| val.as_str())
                    .or_else(|| item.get("content").and_then(|val| val.as_str()))
                    .or_else(|| item.as_str())
                    .map(|text| text.to_string())
            })
            .collect::<Vec<_>>()
            .join("\n");
        if !joined.trim().is_empty() {
            return Some(joined);
        }
    }
    None
}

fn bound_mcp_server_labels(agent: Option<&AgentDefinition>) -> Vec<String> {
    let Some(agent) = agent else {
        return Vec::new();
    };
    if agent.mcp_server_ids.is_empty() {
        return Vec::new();
    }
    let known = crate::mcp_servers::get_mcp_servers()
        .ok()
        .map(|state| state.servers)
        .unwrap_or_default();
    agent.mcp_server_ids
        .iter()
        .map(|server_id| {
            known.iter()
                .find(|server| server.id == *server_id)
                .map(|server| format!("{} ({})", server.name, server.id))
                .unwrap_or_else(|| server_id.clone())
        })
        .collect()
}

fn build_memory_summary(conversation: &AssistantConversation) -> Option<String> {
    let mut recent_points = conversation
        .messages
        .iter()
        .filter(|message| message.role == "user")
        .map(|message| message.content.trim())
        .filter(|content| !content.is_empty())
        .rev()
        .take(3)
        .map(|content| {
            let mut compact = String::new();
            for ch in content.chars().take(120) {
                compact.push(ch);
            }
            compact
        })
        .collect::<Vec<_>>();
    if recent_points.is_empty() {
        return None;
    }
    recent_points.reverse();
    Some(
        recent_points
            .into_iter()
            .map(|item| format!("- {}", item))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn build_system_prompt(
    conversation: &AssistantConversation,
    agent: Option<&AgentDefinition>,
    sources: &[AssistantMessageSource],
    available_tools: &[ToolDefinition],
) -> String {
    let mut sections = Vec::new();
    if let Some(agent) = agent {
        sections.push(agent.system_prompt.clone());
        if !agent.output_contract.trim().is_empty() {
            sections.push(format!("Output contract: {}", agent.output_contract.trim()));
        }
        let capability = capability_snapshot_from_agent(Some(agent), conversation.web_search_enabled);
        let mut capability_lines = Vec::new();
        if capability.workspace_read {
            capability_lines.push("Workspace reading is enabled for this assistant.".to_string());
        }
        if capability.notes_search {
            capability_lines.push("Notes search is enabled for this assistant.".to_string());
        }
        if !capability.knowledge_base_ids.is_empty() {
            capability_lines.push(format!(
                "Bound knowledge bases: {}.",
                capability.knowledge_base_ids.join(", ")
            ));
        }
        let mcp_labels = bound_mcp_server_labels(Some(agent));
        if !mcp_labels.is_empty() {
            capability_lines.push(format!("Bound MCP servers: {}.", mcp_labels.join(", ")));
        }
        if capability.memory_enabled {
            capability_lines.push(
                "Memory mode is enabled. Preserve stable preferences and continue prior intent when it helps."
                    .to_string(),
            );
            if let Some(summary) = build_memory_summary(conversation) {
                capability_lines.push(format!("Recent memory cues:\n{}", summary));
            }
        }
        if !capability_lines.is_empty() {
            sections.push(capability_lines.join("\n"));
        }
    } else {
        sections.push(
            "You are OneSpace AI Assistant. Be concise, practical, and cite provided web sources when they exist."
                .to_string(),
        );
    }

    let has_mcp_tools = available_tools.iter().any(|tool| tool.name.starts_with("mcp__"));
    if has_mcp_tools {
        sections.push(
            "Bound MCP tools are available. Use the most relevant MCP tool directly when it helps."
                .to_string(),
        );
    }
    if conversation.web_search_enabled {
        sections.push(
            "Search-class MCP tools are enabled for this conversation. Use them for current information when relevant."
                .to_string(),
        );
    } else if has_mcp_tools {
        sections.push(
            "Search-class MCP tools are disabled for this conversation. Documentation MCP tools may still be available."
                .to_string(),
        );
    }

    if !sources.is_empty() {
        let source_lines = sources
            .iter()
            .enumerate()
            .map(|(index, source)| {
                format!(
                    "[{}] {} - {} ({})",
                    index + 1,
                    source.title,
                    source.snippet,
                    source.url
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!(
            "Retrieved source context is available. Prefer these sources when relevant:\n{}",
            source_lines
        ));
    }
    sections.join("\n\n")
}

fn format_trigger_label(trigger: &ScheduleTrigger) -> String {
    match trigger.kind.as_str() {
        "interval" => {
            let minutes = trigger.interval_minutes.unwrap_or(30);
            if minutes % 60 == 0 {
                format!("每 {} 小时", minutes / 60)
            } else {
                format!("每 {} 分钟", minutes)
            }
        }
        "weekly" => {
            let weekdays = if trigger.weekdays.is_empty() {
                vec![1]
            } else {
                trigger.weekdays.clone()
            };
            let day_text = weekdays
                .into_iter()
                .map(|day| match day {
                    1 => "周一",
                    2 => "周二",
                    3 => "周三",
                    4 => "周四",
                    5 => "周五",
                    6 => "周六",
                    7 => "周日",
                    _ => "周一",
                })
                .collect::<Vec<_>>()
                .join("、");
            format!(
                "{} {}",
                day_text,
                trigger.time_of_day.clone().unwrap_or_else(|| "09:00".to_string())
            )
        }
        _ => format!(
            "每天 {}",
            trigger.time_of_day.clone().unwrap_or_else(|| "09:00".to_string())
        ),
    }
}

fn find_schedule_match<'a>(state: &'a AssistantState, text: &str) -> Option<&'a ScheduleJob> {
    let lower = text.to_lowercase();
    state
        .schedules
        .iter()
        .filter(|schedule| !schedule.name.trim().is_empty())
        .max_by_key(|schedule| {
            let name = schedule.name.to_lowercase();
            if lower.contains(&name) {
                name.len()
            } else {
                0
            }
        })
        .filter(|schedule| lower.contains(&schedule.name.to_lowercase()))
}

fn find_agent_match<'a>(state: &'a AssistantState, text: &str, web_search: bool) -> Option<&'a AgentDefinition> {
    let lower = text.to_lowercase();
    state
        .agents
        .iter()
        .find(|agent| lower.contains(&agent.name.to_lowercase()))
        .or_else(|| {
            state.agents.iter().find(|agent| {
                if web_search {
                    agent.tool_policy.web_search
                } else {
                    true
                }
            })
        })
        .or_else(|| state.agents.first())
}

fn parse_schedule_time(text: &str) -> Option<String> {
    let captures = time_of_day_regex().captures(text)?;
    let hour = captures.name("hour")?.as_str().parse::<u32>().ok()?;
    let minute = captures.name("minute")?.as_str().parse::<u32>().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(format!("{hour:02}:{minute:02}"))
}

fn parse_schedule_trigger(text: &str, existing: Option<&ScheduleJob>) -> Option<ScheduleTrigger> {
    let normalized = text.replace('：', ":");
    if let Some(captures) = interval_minutes_regex().captures(&normalized) {
        let value = captures.get(1)?.as_str().parse::<u64>().ok()?;
        let unit = captures.get(2)?.as_str();
        let interval_minutes = if unit.contains("小时") {
            value.saturating_mul(60)
        } else {
            value
        };
        return Some(ScheduleTrigger {
            kind: "interval".to_string(),
            interval_minutes: Some(interval_minutes.max(1)),
            time_of_day: None,
            weekdays: Vec::new(),
        });
    }

    let time = parse_schedule_time(&normalized)
        .or_else(|| existing.and_then(|schedule| schedule.trigger.time_of_day.clone()))
        .unwrap_or_else(|| "09:00".to_string());

    if normalized.contains("工作日") {
        return Some(ScheduleTrigger {
            kind: "weekly".to_string(),
            interval_minutes: None,
            time_of_day: Some(time),
            weekdays: vec![1, 2, 3, 4, 5],
        });
    }

    if normalized.contains("每周") {
        let mut weekdays = Vec::new();
        for (needle, value) in [
            ("一", 1_u8),
            ("二", 2_u8),
            ("三", 3_u8),
            ("四", 4_u8),
            ("五", 5_u8),
            ("六", 6_u8),
            ("日", 7_u8),
            ("天", 7_u8),
        ] {
            if normalized.contains(needle) && !weekdays.contains(&value) {
                weekdays.push(value);
            }
        }
        if weekdays.is_empty() {
            weekdays.push(1);
        }
        weekdays.sort_unstable();
        return Some(ScheduleTrigger {
            kind: "weekly".to_string(),
            interval_minutes: None,
            time_of_day: Some(time),
            weekdays,
        });
    }

    if normalized.contains("每天") || normalized.contains("每日") {
        return Some(ScheduleTrigger {
            kind: "daily".to_string(),
            interval_minutes: None,
            time_of_day: Some(time),
            weekdays: Vec::new(),
        });
    }

    existing.map(|schedule| schedule.trigger.clone())
}

fn derive_schedule_name(text: &str, existing: Option<&ScheduleJob>, agent: Option<&AgentDefinition>) -> String {
    if let Some(schedule) = existing {
        return schedule.name.clone();
    }
    if let Some(captures) = quoted_name_regex().captures(text) {
        if let Some(name) = captures.get(1) {
            let trimmed = name.as_str().trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    if let Some(agent) = agent {
        return format!("{} 定时任务", agent.name);
    }
    derive_title(text)
}

fn build_schedule_draft(state: &AssistantState, text: &str) -> Option<AssistantScheduleDraft> {
    let lower = text.to_lowercase();
    let matched_schedule = find_schedule_match(state, text);
    let wants_search = lower.contains("联网")
        || lower.contains("搜索")
        || lower.contains("新闻")
        || lower.contains("research");
    let agent = find_agent_match(state, text, wants_search);

    let action = if matched_schedule.is_some()
        && (text.contains("立即执行") || text.contains("马上执行") || lower.contains("run now"))
    {
        Some("run_now")
    } else if matched_schedule.is_some()
        && (text.contains("删除") || text.contains("移除"))
    {
        Some("delete")
    } else if matched_schedule.is_some()
        && (text.contains("暂停") || text.contains("停用"))
    {
        Some("toggle_off")
    } else if matched_schedule.is_some()
        && (text.contains("启用") || text.contains("恢复"))
    {
        Some("toggle_on")
    } else if matched_schedule.is_some()
        && (text.contains("修改")
            || text.contains("更新")
            || text.contains("调整")
            || text.contains("改成")
            || text.contains("改为")
            || text.contains("变更"))
    {
        Some("update")
    } else if text.contains("定时任务")
        || text.contains("提醒")
        || text.contains("每天")
        || text.contains("每周")
        || text.contains("每隔")
        || text.contains("工作日")
    {
        Some("create")
    } else {
        None
    }?;

    let action = action.to_string();
    if matches!(action.as_str(), "run_now" | "delete" | "toggle_off" | "toggle_on") {
        let target = matched_schedule?;
        let (title, summary, desired_enabled) = match action.as_str() {
            "run_now" => (
                "run schedule now".to_string(),
                format!("Assistant 想立即执行定时任务“{}”。", target.name),
                None,
            ),
            "delete" => (
                "delete schedule".to_string(),
                format!("Assistant 想删除定时任务“{}”。", target.name),
                None,
            ),
            "toggle_off" => (
                "pause schedule".to_string(),
                format!("Assistant 想暂停定时任务“{}”。", target.name),
                Some(false),
            ),
            _ => (
                "enable schedule".to_string(),
                format!("Assistant 想启用定时任务“{}”。", target.name),
                Some(true),
            ),
        };
        return Some(AssistantScheduleDraft {
            action,
            title,
            summary,
            schedule: None,
            target_schedule_id: Some(target.id.clone()),
            target_schedule_name: Some(target.name.clone()),
            desired_enabled,
            agent_name: None,
            trigger_label: Some(format_trigger_label(&target.trigger)),
        });
    }

    let base_schedule = matched_schedule.cloned();
    let trigger = parse_schedule_trigger(text, matched_schedule)?;
    let mut schedule = base_schedule.unwrap_or_else(|| ScheduleJob {
        id: String::new(),
        name: String::new(),
        assistant_id: None,
        agent_id: String::new(),
        prompt: String::new(),
        model_profile_id: None,
        model_override_id: None,
        web_search_enabled: false,
        trigger: ScheduleTrigger {
            kind: "daily".to_string(),
            interval_minutes: None,
            time_of_day: Some("09:00".to_string()),
            weekdays: Vec::new(),
        },
        timezone: Some("Asia/Shanghai".to_string()),
        output_target: "assistant_conversation".to_string(),
        conversation_id: None,
        enabled: true,
        next_run_at: None,
        last_run_at: None,
        last_status: None,
        last_error: None,
        misfire_policy: "skip".to_string(),
        max_retries: 0,
        retry_count: 0,
        created_at: 0,
        updated_at: 0,
    });
    if let Some(agent) = agent {
        schedule.assistant_id = Some(agent.id.clone());
        schedule.agent_id = agent.id.clone();
    }
    schedule.name = derive_schedule_name(text, matched_schedule, agent);
    schedule.prompt = text.trim().to_string();
    schedule.web_search_enabled = wants_search || schedule.web_search_enabled;
    schedule.trigger = trigger.clone();
    schedule.timezone = Some(
        schedule
            .timezone
            .clone()
            .unwrap_or_else(|| "Asia/Shanghai".to_string()),
    );
    schedule.enabled = true;

    Some(AssistantScheduleDraft {
        action,
        title: if matched_schedule.is_some() {
            "update schedule".to_string()
        } else {
            "create schedule".to_string()
        },
        summary: if matched_schedule.is_some() {
            format!("Assistant 想更新定时任务“{}”。", schedule.name)
        } else {
            format!("Assistant 想创建定时任务“{}”。", schedule.name)
        },
        schedule: Some(schedule.clone()),
        target_schedule_id: matched_schedule.map(|schedule| schedule.id.clone()),
        target_schedule_name: Some(schedule.name.clone()),
        desired_enabled: Some(true),
        agent_name: agent.map(|item| item.name.clone()),
        trigger_label: Some(format_trigger_label(&trigger)),
    })
}

async fn read_sse_response<F>(
    mut response: reqwest::Response,
    mut on_event: F,
) -> Result<(), String>
where
    F: FnMut(Option<&str>, &str) -> Result<(), String>,
{
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(if body.trim().is_empty() {
            format!("Request failed with status {}", status)
        } else {
            body
        });
    }

    let mut buffer = String::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        let normalized = String::from_utf8_lossy(&chunk).replace("\r\n", "\n");
        buffer.push_str(&normalized);
        while let Some(index) = buffer.find("\n\n") {
            let block = buffer[..index].to_string();
            buffer = buffer[index + 2..].to_string();
            let mut event_name: Option<String> = None;
            let mut data_lines = Vec::new();
            for line in block.lines() {
                let line = line.trim_end();
                if line.is_empty() || line.starts_with(':') {
                    continue;
                }
                if let Some(rest) = line.strip_prefix("event:") {
                    event_name = Some(rest.trim().to_string());
                } else if let Some(rest) = line.strip_prefix("data:") {
                    data_lines.push(rest.trim_start().to_string());
                }
            }
            if data_lines.is_empty() {
                continue;
            }
            let data = data_lines.join("\n");
            on_event(event_name.as_deref(), &data)?;
        }
    }

    if !buffer.trim().is_empty() {
        let block = buffer.trim().to_string();
        let mut event_name: Option<String> = None;
        let mut data_lines = Vec::new();
        for line in block.lines() {
            let line = line.trim_end();
            if line.is_empty() || line.starts_with(':') {
                continue;
            }
            if let Some(rest) = line.strip_prefix("event:") {
                event_name = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("data:") {
                data_lines.push(rest.trim_start().to_string());
            }
        }
        if !data_lines.is_empty() {
            let data = data_lines.join("\n");
            on_event(event_name.as_deref(), &data)?;
        }
    }

    Ok(())
}

fn text_from_openai_delta(delta: &Value) -> String {
    if let Some(text) = delta.get("content").and_then(|content| content.as_str()) {
        return text.to_string();
    }
    if let Some(items) = delta.get("content").and_then(|content| content.as_array()) {
        return items
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .and_then(|value| value.as_str())
                    .or_else(|| item.get("content").and_then(|value| value.as_str()))
            })
            .collect::<Vec<_>>()
            .join("");
    }
    String::new()
}

fn text_from_gemini_response(payload: &Value) -> String {
    payload
        .get("candidates")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|candidate| candidate.get("content"))
        .and_then(|content| content.get("parts"))
        .and_then(|parts| parts.as_array())
        .into_iter()
        .flatten()
        .filter_map(|part| part.get("text").and_then(|value| value.as_str()))
        .collect::<Vec<_>>()
        .join("")
}

async fn run_openai_compatible(
    provider: &AiAssistantProvider,
    profile: &AiAssistantModelProfile,
    context: &[(String, String)],
    system_prompt: &str,
) -> Result<(String, Option<String>), String> {
    let client = build_reqwest_client(Some(60))?;
    let endpoint = resolve_provider_endpoint(provider, "chat/completions");
    let mut messages = vec![json!({
        "role": "system",
        "content": system_prompt,
    })];
    for (role, content) in context {
        messages.push(json!({
            "role": role,
            "content": content,
        }));
    }
    let payload = json!({
        "model": profile.model_id,
        "messages": messages,
        "temperature": profile.temperature.unwrap_or(0.3),
        "max_tokens": profile.max_tokens.unwrap_or(2048),
    });
    let request = client.post(endpoint).json(&payload);
    let response = apply_provider_headers(request, provider)?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status();
    let body: Value = response.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(body.to_string());
    }
    let message = body
        .get("choices")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|choice| choice.get("message"))
        .ok_or_else(|| "Missing message in OpenAI-compatible response".to_string())?;
    Ok((text_from_openai_message(message), reasoning_from_openai_message(message)))
}

async fn run_openai_compatible_stream(
    app: &tauri::AppHandle,
    conversation_id: &str,
    message_id: &str,
    provider: &AiAssistantProvider,
    profile: &AiAssistantModelProfile,
    context: &[(String, String)],
    system_prompt: &str,
) -> Result<(String, Option<String>), String> {
    let client = build_reqwest_client(Some(60))?;
    let endpoint = resolve_provider_endpoint(provider, "chat/completions");
    let mut messages = vec![json!({
        "role": "system",
        "content": system_prompt,
    })];
    for (role, content) in context {
        messages.push(json!({
            "role": role,
            "content": content,
        }));
    }
    let payload = json!({
        "model": profile.model_id,
        "messages": messages,
        "temperature": profile.temperature.unwrap_or(0.3),
        "max_tokens": profile.max_tokens.unwrap_or(2048),
        "stream": true,
    });
    let request = client.post(endpoint).json(&payload);
    let response = apply_provider_headers(request, provider)?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let mut content = String::new();
    let mut reasoning = String::new();
    read_sse_response(response, |_, data| {
        if data.trim() == "[DONE]" {
            return Ok(());
        }
        let payload: Value = serde_json::from_str(data).map_err(|e| e.to_string())?;
        if let Some(delta) = payload
            .get("choices")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .and_then(|choice| choice.get("delta"))
        {
            let text_delta = text_from_openai_delta(delta);
            if !text_delta.is_empty() {
                content.push_str(&text_delta);
                emit_stream_event(
                    app,
                    AssistantStreamEvent {
                        conversation_id: conversation_id.to_string(),
                        message_id: message_id.to_string(),
                        kind: "message.delta".to_string(),
                        text: Some(text_delta),
                        sources: None,
                        tool: None,
                        error: None,
                    },
                );
            }
            if let Some(reasoning_delta) = reasoning_from_openai_message(delta) {
                if !reasoning_delta.is_empty() {
                    reasoning.push_str(&reasoning_delta);
                    emit_stream_event(
                        app,
                        AssistantStreamEvent {
                            conversation_id: conversation_id.to_string(),
                            message_id: message_id.to_string(),
                            kind: "reasoning.delta".to_string(),
                            text: Some(reasoning_delta),
                            sources: None,
                            tool: None,
                            error: None,
                        },
                    );
                }
            }
        }
        Ok(())
    })
    .await?;

    Ok((
        content,
        if reasoning.trim().is_empty() {
            None
        } else {
            Some(reasoning)
        },
    ))
}

async fn run_anthropic_messages(
    provider: &AiAssistantProvider,
    profile: &AiAssistantModelProfile,
    context: &[(String, String)],
    system_prompt: &str,
) -> Result<(String, Option<String>), String> {
    let client = build_reqwest_client(Some(60))?;
    let endpoint = resolve_endpoint(&provider.base_url, "messages");
    let messages = context
        .iter()
        .filter(|(role, _)| role == "user" || role == "assistant")
        .map(|(role, content)| {
            json!({
                "role": role,
                "content": content,
            })
        })
        .collect::<Vec<_>>();
    let request = client
        .post(endpoint)
        .header("anthropic-version", "2023-06-01")
        .json(&json!({
            "model": profile.model_id,
            "max_tokens": profile.max_tokens.unwrap_or(2048),
            "system": system_prompt,
            "messages": messages,
        }));
    let response = apply_provider_headers(request, provider)?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status();
    let body: Value = response.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(body.to_string());
    }
    let blocks = body
        .get("content")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let mut text = Vec::new();
    let mut reasoning = Vec::new();
    for block in blocks {
        match block.get("type").and_then(|value| value.as_str()).unwrap_or_default() {
            "text" => {
                if let Some(value) = block.get("text").and_then(|value| value.as_str()) {
                    text.push(value.to_string());
                }
            }
            "thinking" => {
                if let Some(value) = block.get("thinking").and_then(|value| value.as_str()) {
                    reasoning.push(value.to_string());
                }
            }
            _ => {}
        }
    }
    let reasoning = if reasoning.is_empty() {
        None
    } else {
        Some(reasoning.join("\n"))
    };
    Ok((text.join("\n"), reasoning))
}

async fn run_anthropic_messages_stream(
    app: &tauri::AppHandle,
    conversation_id: &str,
    message_id: &str,
    provider: &AiAssistantProvider,
    profile: &AiAssistantModelProfile,
    context: &[(String, String)],
    system_prompt: &str,
) -> Result<(String, Option<String>), String> {
    let client = build_reqwest_client(Some(60))?;
    let endpoint = resolve_endpoint(&provider.base_url, "messages");
    let messages = context
        .iter()
        .filter(|(role, _)| role == "user" || role == "assistant")
        .map(|(role, content)| {
            json!({
                "role": role,
                "content": content,
            })
        })
        .collect::<Vec<_>>();
    let request = client
        .post(endpoint)
        .header("anthropic-version", "2023-06-01")
        .json(&json!({
            "model": profile.model_id,
            "max_tokens": profile.max_tokens.unwrap_or(2048),
            "system": system_prompt,
            "messages": messages,
            "stream": true,
        }));
    let response = apply_provider_headers(request, provider)?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let mut content = String::new();
    let mut reasoning = String::new();
    read_sse_response(response, |event_name, data| {
        if data.trim() == "[DONE]" {
            return Ok(());
        }
        let payload: Value = serde_json::from_str(data).map_err(|e| e.to_string())?;
        let payload_type = payload
            .get("type")
            .and_then(|value| value.as_str())
            .or(event_name)
            .unwrap_or_default();

        let mut text_delta = String::new();
        let mut reasoning_delta = String::new();
        match payload_type {
            "content_block_start" => {
                if let Some(block) = payload.get("content_block") {
                    match block.get("type").and_then(|value| value.as_str()).unwrap_or_default() {
                        "text" => {
                            if let Some(text) = block.get("text").and_then(|value| value.as_str()) {
                                text_delta = text.to_string();
                            }
                        }
                        "thinking" => {
                            if let Some(text) =
                                block.get("thinking").and_then(|value| value.as_str())
                            {
                                reasoning_delta = text.to_string();
                            }
                        }
                        _ => {}
                    }
                }
            }
            "content_block_delta" => {
                if let Some(delta) = payload.get("delta") {
                    match delta.get("type").and_then(|value| value.as_str()).unwrap_or_default() {
                        "text_delta" => {
                            if let Some(text) = delta.get("text").and_then(|value| value.as_str()) {
                                text_delta = text.to_string();
                            }
                        }
                        "thinking_delta" => {
                            if let Some(text) =
                                delta.get("thinking").and_then(|value| value.as_str())
                            {
                                reasoning_delta = text.to_string();
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }

        if !reasoning_delta.is_empty() {
            reasoning.push_str(&reasoning_delta);
            emit_stream_event(
                app,
                AssistantStreamEvent {
                    conversation_id: conversation_id.to_string(),
                    message_id: message_id.to_string(),
                    kind: "reasoning.delta".to_string(),
                    text: Some(reasoning_delta),
                    sources: None,
                    tool: None,
                    error: None,
                },
            );
        }
        if !text_delta.is_empty() {
            content.push_str(&text_delta);
            emit_stream_event(
                app,
                AssistantStreamEvent {
                    conversation_id: conversation_id.to_string(),
                    message_id: message_id.to_string(),
                    kind: "message.delta".to_string(),
                    text: Some(text_delta),
                    sources: None,
                    tool: None,
                    error: None,
                },
            );
        }
        Ok(())
    })
    .await?;

    Ok((
        content,
        if reasoning.trim().is_empty() {
            None
        } else {
            Some(reasoning)
        },
    ))
}

async fn run_google_gemini(
    provider: &AiAssistantProvider,
    profile: &AiAssistantModelProfile,
    context: &[(String, String)],
    system_prompt: &str,
) -> Result<(String, Option<String>), String> {
    let client = build_reqwest_client(Some(60))?;
    let endpoint = if provider.base_url.contains(":generateContent") {
        provider.base_url.clone()
    } else {
        format!(
            "{}/models/{}:generateContent",
            provider.base_url.trim_end_matches('/'),
            profile.model_id
        )
    };
    let contents = context
        .iter()
        .filter(|(role, _)| role == "user" || role == "assistant")
        .map(|(role, content)| {
            let gemini_role = if role == "assistant" { "model" } else { "user" };
            json!({
                "role": gemini_role,
                "parts": [{ "text": content }],
            })
        })
        .collect::<Vec<_>>();
    let request = client.post(endpoint).json(&json!({
        "system_instruction": {
            "parts": [{ "text": system_prompt }]
        },
        "contents": contents,
        "generationConfig": {
            "temperature": profile.temperature.unwrap_or(0.3),
            "maxOutputTokens": profile.max_tokens.unwrap_or(2048),
        }
    }));
    let response = apply_provider_headers(request, provider)?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status();
    let body: Value = response.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(body.to_string());
    }
    let text = body
        .get("candidates")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|candidate| candidate.get("content"))
        .and_then(|content| content.get("parts"))
        .and_then(|parts| parts.as_array())
        .into_iter()
        .flatten()
        .filter_map(|part| part.get("text").and_then(|value| value.as_str()))
        .collect::<Vec<_>>()
        .join("\n");
    Ok((text, None))
}

async fn run_google_gemini_stream(
    app: &tauri::AppHandle,
    conversation_id: &str,
    message_id: &str,
    provider: &AiAssistantProvider,
    profile: &AiAssistantModelProfile,
    context: &[(String, String)],
    system_prompt: &str,
) -> Result<(String, Option<String>), String> {
    let client = build_reqwest_client(Some(60))?;
    let endpoint = if provider.base_url.contains(":streamGenerateContent") {
        provider.base_url.clone()
    } else if provider.base_url.contains(":generateContent") {
        provider
            .base_url
            .replace(":generateContent", ":streamGenerateContent?alt=sse")
    } else {
        format!(
            "{}/models/{}:streamGenerateContent?alt=sse",
            provider.base_url.trim_end_matches('/'),
            profile.model_id
        )
    };
    let contents = context
        .iter()
        .filter(|(role, _)| role == "user" || role == "assistant")
        .map(|(role, content)| {
            let gemini_role = if role == "assistant" { "model" } else { "user" };
            json!({
                "role": gemini_role,
                "parts": [{ "text": content }],
            })
        })
        .collect::<Vec<_>>();
    let request = client.post(endpoint).json(&json!({
        "system_instruction": {
            "parts": [{ "text": system_prompt }]
        },
        "contents": contents,
        "generationConfig": {
            "temperature": profile.temperature.unwrap_or(0.3),
            "maxOutputTokens": profile.max_tokens.unwrap_or(2048),
        }
    }));
    let response = apply_provider_headers(request, provider)?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let mut content = String::new();
    let mut last_rendered = String::new();
    read_sse_response(response, |_, data| {
        if data.trim() == "[DONE]" {
            return Ok(());
        }
        let payload: Value = serde_json::from_str(data).map_err(|e| e.to_string())?;
        let full_text = text_from_gemini_response(&payload);
        if full_text.is_empty() {
            return Ok(());
        }
        let delta = if full_text.starts_with(&last_rendered) {
            full_text[last_rendered.len()..].to_string()
        } else {
            full_text.clone()
        };
        if !delta.is_empty() {
            content.push_str(&delta);
            last_rendered = full_text;
            emit_stream_event(
                app,
                AssistantStreamEvent {
                    conversation_id: conversation_id.to_string(),
                    message_id: message_id.to_string(),
                    kind: "message.delta".to_string(),
                    text: Some(delta),
                    sources: None,
                    tool: None,
                    error: None,
                },
            );
        }
        Ok(())
    })
    .await?;

    Ok((content, None))
}

async fn run_model_request(
    provider: &AiAssistantProvider,
    profile: &AiAssistantModelProfile,
    context: &[(String, String)],
    system_prompt: &str,
) -> Result<(String, Option<String>), String> {
    match provider.protocol.as_str() {
        "anthropic-messages" => run_anthropic_messages(provider, profile, context, system_prompt).await,
        "google-gemini" => run_google_gemini(provider, profile, context, system_prompt).await,
        _ => run_openai_compatible(provider, profile, context, system_prompt).await,
    }
}

async fn run_model_request_with_tools(
    provider: &AiAssistantProvider,
    profile: &AiAssistantModelProfile,
    context: &[(String, String)],
    system_prompt: &str,
    tools: &[ToolDefinition],
) -> Result<(String, Option<String>, Vec<AssistantToolCall>), String> {
    let client = build_reqwest_client(Some(60))?;
    let endpoint = resolve_provider_endpoint(provider, "chat/completions");

    let mut messages = vec![json!({
        "role": "system",
        "content": system_prompt,
    })];

    for (role, content) in context {
        if role == "tool" {
            if let Ok(tool_msg) = serde_json::from_str::<Value>(content) {
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": tool_msg.get("tool_call_id").and_then(|v| v.as_str()).unwrap_or(""),
                    "content": tool_msg.get("content").and_then(|v| v.as_str()).unwrap_or(""),
                }));
            }
        } else {
            messages.push(json!({
                "role": role,
                "content": content,
            }));
        }
    }

    let model_params = build_model_params(profile);
    let mut payload = json!({
        "model": profile.model_id,
        "messages": messages,
    });
    if let Some(temp) = model_params.get("temperature") {
        payload["temperature"] = temp.clone();
    }
    if let Some(max_tokens) = model_params.get("max_tokens") {
        payload["max_tokens"] = max_tokens.clone();
    }
    if let Some(top_p) = model_params.get("top_p") {
        payload["top_p"] = top_p.clone();
    }
    if let Some(freq) = model_params.get("frequency_penalty") {
        payload["frequency_penalty"] = freq.clone();
    }
    if let Some(pres) = model_params.get("presence_penalty") {
        payload["presence_penalty"] = pres.clone();
    }
    if let Some(stop) = model_params.get("stop") {
        payload["stop"] = stop.clone();
    }

    if !tools.is_empty() {
        payload["tools"] = json!(tools.iter().map(|t| json!({
            "type": "function",
            "function": {
                "name": t.name,
                "description": t.description,
                "parameters": t.parameters,
            }
        })).collect::<Vec<_>>());
    }

    let request = client.post(endpoint).json(&payload);
    let response = apply_provider_headers(request, provider)?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status();
    let body: Value = response.json().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(body.to_string());
    }

    let message = body
        .get("choices")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|choice| choice.get("message"))
        .ok_or_else(|| "Missing message in response".to_string())?;

    let content = text_from_openai_message(message);
    let reasoning = reasoning_from_openai_message(message);

    let tool_calls = message
        .get("tool_calls")
        .and_then(|value| value.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|tc| {
                    let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let name = tc
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let arguments = tc
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    if !id.is_empty() && !name.is_empty() {
                        Some(AssistantToolCall {
                            id,
                            name,
                            arguments,
                            status: "pending".to_string(),
                            summary: None,
                            result: None,
                            started_at: now_ts(),
                            finished_at: None,
                        })
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    Ok((content, reasoning, tool_calls))
}

async fn run_model_request_with_tools_streaming(
    app: &tauri::AppHandle,
    conversation_id: &str,
    message_id: &str,
    provider: &AiAssistantProvider,
    profile: &AiAssistantModelProfile,
    context: &[(String, String)],
    system_prompt: &str,
    tools: &[ToolDefinition],
) -> Result<(String, Option<String>, Vec<AssistantToolCall>), String> {
    let client = build_reqwest_client(Some(60))?;
    let endpoint = resolve_provider_endpoint(provider, "chat/completions");

    let mut messages = vec![json!({
        "role": "system",
        "content": system_prompt,
    })];

    for (role, content) in context {
        if role == "tool" {
            if let Ok(tool_msg) = serde_json::from_str::<Value>(content) {
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": tool_msg.get("tool_call_id").and_then(|v| v.as_str()).unwrap_or(""),
                    "content": tool_msg.get("content").and_then(|v| v.as_str()).unwrap_or(""),
                }));
            }
        } else {
            messages.push(json!({
                "role": role,
                "content": content,
            }));
        }
    }

    let model_params = build_model_params(profile);
    let mut payload = json!({
        "model": profile.model_id,
        "messages": messages,
        "stream": true,
    });
    if let Some(temp) = model_params.get("temperature") {
        payload["temperature"] = temp.clone();
    }
    if let Some(max_tokens) = model_params.get("max_tokens") {
        payload["max_tokens"] = max_tokens.clone();
    }
    if let Some(top_p) = model_params.get("top_p") {
        payload["top_p"] = top_p.clone();
    }
    if let Some(freq) = model_params.get("frequency_penalty") {
        payload["frequency_penalty"] = freq.clone();
    }
    if let Some(pres) = model_params.get("presence_penalty") {
        payload["presence_penalty"] = pres.clone();
    }
    if let Some(stop) = model_params.get("stop") {
        payload["stop"] = stop.clone();
    }

    if !tools.is_empty() {
        payload["tools"] = json!(tools.iter().map(|t| json!({
            "type": "function",
            "function": {
                "name": t.name,
                "description": t.description,
                "parameters": t.parameters,
            }
        })).collect::<Vec<_>>());
    }

    let request = client.post(endpoint).json(&payload);
    let response = apply_provider_headers(request, provider)?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let mut content = String::new();
    let mut reasoning = String::new();
    let mut tool_calls_map: std::collections::HashMap<String, (String, String)> = std::collections::HashMap::new();
    let mut tool_call_order: Vec<String> = Vec::new();

    read_sse_response(response, |_, data| {
        if data.trim() == "[DONE]" {
            return Ok(());
        }
        let payload: Value = serde_json::from_str(data).map_err(|e| e.to_string())?;

        if let Some(delta) = payload
            .get("choices")
            .and_then(|value| value.as_array())
            .and_then(|items| items.first())
            .and_then(|choice| choice.get("delta"))
        {
            let text_delta = text_from_openai_delta(delta);
            if !text_delta.is_empty() {
                content.push_str(&text_delta);
                emit_stream_event(
                    app,
                    AssistantStreamEvent {
                        conversation_id: conversation_id.to_string(),
                        message_id: message_id.to_string(),
                        kind: "message.delta".to_string(),
                        text: Some(text_delta),
                        sources: None,
                        tool: None,
                        error: None,
                    },
                );
            }

            if let Some(reasoning_delta) = reasoning_from_openai_message(delta) {
                if !reasoning_delta.is_empty() {
                    reasoning.push_str(&reasoning_delta);
                    emit_stream_event(
                        app,
                        AssistantStreamEvent {
                            conversation_id: conversation_id.to_string(),
                            message_id: message_id.to_string(),
                            kind: "reasoning.delta".to_string(),
                            text: Some(reasoning_delta),
                            sources: None,
                            tool: None,
                            error: None,
                        },
                    );
                }
            }

            if let Some(tool_calls_delta) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                for tc in tool_calls_delta {
                    if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                        tool_call_order.push(id.to_string());
                    }
                    let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let id = tc.get("id")
                        .and_then(|v| v.as_str())
                        .or_else(|| tool_call_order.get(idx).map(|s| s.as_str()))
                        .unwrap_or("");
                    let name = tc
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let args = tc
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    if !id.is_empty() {
                        tool_calls_map
                            .entry(id.to_string())
                            .or_insert_with(|| (String::new(), String::new()));
                        let entry = tool_calls_map.get_mut(id).unwrap();
                        if !name.is_empty() {
                            entry.0 = name.to_string();
                        }
                        entry.1.push_str(args);
                    }
                }
            }
        }
        Ok(())
    })
    .await?;

    let tool_calls: Vec<AssistantToolCall> = tool_call_order
        .iter()
        .filter_map(|id| {
            tool_calls_map.get(id).map(|(name, args)| AssistantToolCall {
                id: id.clone(),
                name: name.clone(),
                arguments: Some(args.clone()),
                status: "pending".to_string(),
                summary: None,
                result: None,
                started_at: now_ts(),
                finished_at: None,
            })
        })
        .collect();

    let reasoning = if reasoning.trim().is_empty() {
        None
    } else {
        Some(reasoning)
    };

    Ok((content, reasoning, tool_calls))
}

async fn run_model_request_streaming(
    app: &tauri::AppHandle,
    conversation_id: &str,
    message_id: &str,
    provider: &AiAssistantProvider,
    profile: &AiAssistantModelProfile,
    context: &[(String, String)],
    system_prompt: &str,
) -> Result<(String, Option<String>), String> {
    match provider.protocol.as_str() {
        "anthropic-messages" => {
            run_anthropic_messages_stream(
                app,
                conversation_id,
                message_id,
                provider,
                profile,
                context,
                system_prompt,
            )
            .await
        }
        "google-gemini" => {
            run_google_gemini_stream(
                app,
                conversation_id,
                message_id,
                provider,
                profile,
                context,
                system_prompt,
            )
            .await
        }
        _ => {
            run_openai_compatible_stream(
                app,
                conversation_id,
                message_id,
                provider,
                profile,
                context,
                system_prompt,
            )
            .await
        }
    }
}

fn emit_stream_event(app: &tauri::AppHandle, payload: AssistantStreamEvent) {
    let _ = app.emit(ASSISTANT_STREAM_EVENT, payload);
}

fn save_message_result(
    conversation_id: &str,
    message_id: &str,
    content: &str,
    reasoning: Option<String>,
    sources: Vec<AssistantMessageSource>,
    tool_calls: Vec<AssistantToolCall>,
    status: &str,
) -> Result<(), String> {
    let mut state = load_state()?;
    let conversation = state
        .conversations
        .iter_mut()
        .find(|conversation| conversation.id == conversation_id)
        .ok_or_else(|| "Conversation not found".to_string())?;
    let message = conversation
        .messages
        .iter_mut()
        .find(|message| message.id == message_id)
        .ok_or_else(|| "Message not found".to_string())?;
    message.content = content.to_string();
    message.reasoning = reasoning;
    message.sources = sources;
    message.tool_calls = tool_calls;
    message.status = status.to_string();
    conversation.updated_at = now_ts();
    if conversation.title.trim().is_empty() {
        if let Some(user_message) = conversation.messages.iter().find(|item| item.role == "user") {
            conversation.title = derive_title(&user_message.content);
        }
    }
    save_state(&state)
}

async fn execute_workspace_conversation_run(
    app: tauri::AppHandle,
    conversation_id: String,
    assistant_message_id: String,
    explicit_model_id: Option<String>,
    explicit_assistant_id: Option<String>,
    _force_web_search: Option<bool>,
) -> Result<(), String> {
    let state = load_state()?;
    let conversation = state
        .conversations
        .iter()
        .find(|item| item.id == conversation_id)
        .cloned()
        .ok_or_else(|| "Conversation not found".to_string())?;
    let assistant = explicit_assistant_id
        .as_deref()
        .or(conversation.assistant_id.as_deref())
        .and_then(|id| state.agents.iter().find(|item| item.id == id))
        .cloned();
    let role = if assistant.is_some() { "assistant" } else { "chat" };
    let profile = resolve_runtime_profile(
        &state,
        explicit_model_id
            .as_deref()
            .or(conversation.model_override_id.as_deref()),
        assistant.as_ref(),
        role,
    )?;
    let provider = resolve_provider(&state, &profile)?.clone();
    if !provider.enabled {
        return Err(format!("Model provider is disabled: {}", provider.name));
    }
    if provider.api_key.trim().is_empty() {
        return Err(format!("Model provider API key is empty: {}", provider.name));
    }

    let mut tool_policy = assistant
        .as_ref()
        .map(|a| a.tool_policy.clone())
        .unwrap_or_default();
    tool_policy.web_search = conversation.web_search_enabled;

    let (mut mcp_clients, mcp_tools_by_name) =
        load_bound_mcp_tools(assistant.as_ref(), tool_policy.web_search).await?;
    let mut mcp_tools = mcp_tools_by_name
        .values()
        .cloned()
        .collect::<Vec<_>>();
    mcp_tools.sort_by(|a, b| a.assistant_tool_name.cmp(&b.assistant_tool_name));
    let available_tools = build_available_tools(&tool_policy, &mcp_tools);

    let mut all_tool_calls = Vec::new();
    let mut all_sources = Vec::new();
    let mut accumulated_content = String::new();
    let mut accumulated_reasoning: Option<String> = None;

    let max_tool_iterations = 5;
    let mut iteration = 0;

    let mut context = build_context_messages(&conversation);
    let initial_system_prompt = build_system_prompt(&conversation, assistant.as_ref(), &[], &available_tools);
    let mut system_prompt = initial_system_prompt.clone();

    let run_result = async {
        loop {
            iteration += 1;
            if iteration > max_tool_iterations {
                break;
            }

            let (content, reasoning, tool_calls_requested) =
                if provider.capabilities.supports_streaming {
                    run_model_request_with_tools_streaming(
                        &app,
                        &conversation_id,
                        &assistant_message_id,
                        &provider,
                        &profile,
                        &context,
                        &system_prompt,
                        &available_tools,
                    )
                    .await?
                } else {
                    run_model_request_with_tools(
                        &provider,
                        &profile,
                        &context,
                        &system_prompt,
                        &available_tools,
                    )
                    .await?
                };

            accumulated_content.push_str(&content);
            if let Some(r) = reasoning {
                accumulated_reasoning = Some(
                    accumulated_reasoning
                        .map(|existing| format!("{}\n{}", existing, r))
                        .unwrap_or(r),
                );
            }

            if tool_calls_requested.is_empty() {
                break;
            }

            for tool_call in &tool_calls_requested {
                let arguments: Value = tool_call
                    .arguments
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or(Value::Null);

                let result = execute_tool_call(
                    &app,
                    &state,
                    &tool_call.name,
                    &arguments,
                    &conversation_id,
                    &assistant_message_id,
                    &mcp_tools_by_name,
                    &mut mcp_clients,
                )
                .await;

                let tool_result_content = match result {
                    Ok((text, sources)) => {
                        all_sources.extend(sources);
                        let success_tool = AssistantToolCall {
                            id: tool_call.id.clone(),
                            name: tool_call.name.clone(),
                            arguments: tool_call.arguments.clone(),
                            status: "success".to_string(),
                            summary: Some("Tool executed successfully".to_string()),
                            result: Some(text.clone()),
                            started_at: tool_call.started_at,
                            finished_at: Some(now_ts()),
                        };
                        all_tool_calls.push(success_tool);
                        text
                    }
                    Err(error) => {
                        let failed_tool = AssistantToolCall {
                            id: tool_call.id.clone(),
                            name: tool_call.name.clone(),
                            arguments: tool_call.arguments.clone(),
                            status: "failed".to_string(),
                            summary: Some(error.clone()),
                            result: Some(format!("Error: {}", error)),
                            started_at: tool_call.started_at,
                            finished_at: Some(now_ts()),
                        };
                        all_tool_calls.push(failed_tool);
                        format!("Error: {}", error)
                    }
                };

                context.push(("assistant".to_string(), content.clone()));
                context.push((
                    "tool".to_string(),
                    json!({
                        "tool_call_id": tool_call.id,
                        "content": tool_result_content,
                    })
                    .to_string(),
                ));
            }

            system_prompt =
                build_system_prompt(&conversation, assistant.as_ref(), &all_sources, &available_tools);
        }

        save_message_result(
            &conversation_id,
            &assistant_message_id,
            &accumulated_content,
            accumulated_reasoning.clone(),
            all_sources.clone(),
            all_tool_calls.clone(),
            "done",
        )?;

        emit_stream_event(
            &app,
            AssistantStreamEvent {
                conversation_id,
                message_id: assistant_message_id,
                kind: "message.completed".to_string(),
                text: None,
                sources: Some(all_sources),
                tool: None,
                error: None,
            },
        );

        Ok(())
    }
    .await;

    close_mcp_clients(&mut mcp_clients).await;
    run_result
}

fn new_message(role: &str, content: String, status: &str) -> AssistantMessage {
    AssistantMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: role.to_string(),
        content,
        reasoning: None,
        sources: Vec::new(),
        tool_calls: Vec::new(),
        tool_call_id: None,
        schedule_draft: None,
        created_at: now_ts(),
        status: status.to_string(),
    }
}

fn upsert_agent(mut incoming: AgentDefinition) -> Result<AgentDefinition, String> {
    let mut state = load_state()?;
    let now = now_ts();
    if incoming.id.trim().is_empty() {
        incoming.id = uuid::Uuid::new_v4().to_string();
        incoming.created_at = now;
    }
    incoming.updated_at = now;
    if incoming.created_at == 0 {
        incoming.created_at = now;
    }
    if let Some(existing) = state.agents.iter_mut().find(|agent| agent.id == incoming.id) {
        *existing = incoming.clone();
    } else {
        state.agents.push(incoming.clone());
    }
    save_state(&state)?;
    Ok(incoming)
}

fn compute_next_run_at(trigger: &ScheduleTrigger, from_ts: u64, timezone: Option<&str>) -> Option<u64> {
    let tz: chrono_tz::Tz = timezone
        .and_then(|tz| tz.parse().ok())
        .unwrap_or(chrono_tz::Tz::Asia__Shanghai);

    match trigger.kind.as_str() {
        "interval" => trigger.interval_minutes.map(|minutes| from_ts + minutes.saturating_mul(60)),
        "daily" => {
            let time = trigger.time_of_day.as_deref().unwrap_or("09:00");
            let (hour, minute) = parse_time_of_day(time)?;
            let now_utc = chrono::Utc::now();
            let now_in_tz = now_utc.with_timezone(&tz);
            let today = now_in_tz
                .with_hour(hour)?
                .with_minute(minute)?
                .with_second(0)?
                .with_nanosecond(0)?;
            let next = if today.timestamp() as u64 > from_ts {
                today
            } else {
                today + chrono::Duration::days(1)
            };
            Some(next.timestamp() as u64)
        }
        "weekly" => {
            let days = if trigger.weekdays.is_empty() {
                vec![1]
            } else {
                trigger.weekdays.clone()
            };
            let time = trigger.time_of_day.as_deref().unwrap_or("09:00");
            let (hour, minute) = parse_time_of_day(time)?;
            let now_utc = chrono::Utc::now();
            let base = now_utc.with_timezone(&tz);
            for offset in 0..8 {
                let candidate = base + chrono::Duration::days(offset);
                let weekday = weekday_to_u8(candidate.weekday());
                if !days.contains(&weekday) {
                    continue;
                }
                let scheduled = candidate
                    .with_hour(hour)?
                    .with_minute(minute)?
                    .with_second(0)?
                    .with_nanosecond(0)?;
                if scheduled.timestamp() as u64 > from_ts {
                    return Some(scheduled.timestamp() as u64);
                }
            }
            None
        }
        _ => None,
    }
}

fn weekday_to_u8(weekday: Weekday) -> u8 {
    match weekday {
        Weekday::Mon => 1,
        Weekday::Tue => 2,
        Weekday::Wed => 3,
        Weekday::Thu => 4,
        Weekday::Fri => 5,
        Weekday::Sat => 6,
        Weekday::Sun => 7,
    }
}

fn parse_time_of_day(value: &str) -> Option<(u32, u32)> {
    let parts = value.split(':').collect::<Vec<_>>();
    if parts.len() != 2 {
        return None;
    }
    let hour = parts[0].trim().parse::<u32>().ok()?;
    let minute = parts[1].trim().parse::<u32>().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some((hour, minute))
}

fn schedule_view(job: &ScheduleJob, runs: &[ScheduleRun]) -> ScheduleJobView {
    let recent_runs = runs
        .iter()
        .filter(|run| run.schedule_id == job.id)
        .take(10)
        .cloned()
        .collect::<Vec<_>>();
    ScheduleJobView {
        job: job.clone(),
        recent_runs,
    }
}

async fn trigger_schedule_run(app: tauri::AppHandle, schedule_id: String) -> Result<(), String> {
    {
        let mut running = running_schedules()
            .lock()
            .map_err(|_| "running schedule lock poisoned".to_string())?;
        if !running.insert(schedule_id.clone()) {
            return Ok(());
        }
    }

    let result = trigger_schedule_run_inner(app.clone(), schedule_id.clone()).await;

    if let Ok(mut running) = running_schedules().lock() {
        running.remove(&schedule_id);
    }
    result
}

async fn trigger_schedule_run_inner(app: tauri::AppHandle, schedule_id: String) -> Result<(), String> {
    let mut state = load_state()?;
    let schedule_index = state
        .schedules
        .iter()
        .position(|schedule| schedule.id == schedule_id)
        .ok_or_else(|| "Schedule not found".to_string())?;
    let schedule_snapshot = state.schedules[schedule_index].clone();
    let agent = state
        .agents
        .iter()
        .find(|agent| agent.id == schedule_snapshot.agent_id)
        .cloned()
        .ok_or_else(|| "Schedule agent not found".to_string())?;

    let conversation_id = if let Some(existing_id) = schedule_snapshot.conversation_id.clone() {
        existing_id
    } else {
        let conversation = AssistantConversation {
            id: uuid::Uuid::new_v4().to_string(),
            title: schedule_snapshot.name.clone(),
            pinned: false,
            archived: false,
            created_at: now_ts(),
            updated_at: now_ts(),
            assistant_id: schedule_assistant_id(&schedule_snapshot).map(|value| value.to_string()),
            model_profile_id: schedule_snapshot.model_profile_id.clone(),
            model_override_id: schedule_snapshot
                .model_override_id
                .clone()
                .or_else(|| legacy_profile_catalog_id(&state.settings, schedule_snapshot.model_profile_id.as_deref())),
            web_search_enabled: schedule_snapshot.web_search_enabled,
            capability_snapshot: Some(capability_snapshot_from_agent(
                Some(&agent),
                schedule_snapshot.web_search_enabled,
            )),
            context_reset_count: 0,
            messages: Vec::new(),
        };
        let id = conversation.id.clone();
        state.schedules[schedule_index].conversation_id = Some(id.clone());
        state.conversations.push(conversation);
        id
    };
    let prompt = if schedule_snapshot.prompt.trim().is_empty() {
        format!("Run scheduled task for {}", schedule_snapshot.name)
    } else {
        schedule_snapshot.prompt.clone()
    };
    let user_message = new_message("user", prompt.clone(), "done");
    let assistant_message = new_message("assistant", String::new(), "streaming");

    if let Some(conversation) = state
        .conversations
        .iter_mut()
        .find(|conversation| conversation.id == conversation_id)
    {
        conversation.messages.push(user_message.clone());
        conversation.messages.push(assistant_message.clone());
        conversation.updated_at = now_ts();
    }

    let run = ScheduleRun {
        id: uuid::Uuid::new_v4().to_string(),
        schedule_id: schedule_snapshot.id.clone(),
        started_at: now_ts(),
        ended_at: None,
        status: "running".to_string(),
        summary: None,
        error_message: None,
        conversation_id: Some(conversation_id.clone()),
    };
    state.runs.insert(0, run.clone());
    state.schedules[schedule_index].last_run_at = Some(now_ts());
    state.schedules[schedule_index].last_status = Some("running".to_string());
    state.schedules[schedule_index].last_error = None;
    state.schedules[schedule_index].next_run_at =
        compute_next_run_at(&schedule_snapshot.trigger, now_ts(), schedule_snapshot.timezone.as_deref());
    state.schedules[schedule_index].updated_at = now_ts();
    save_state(&state)?;

    let execution = execute_workspace_conversation_run(
        app.clone(),
        conversation_id.clone(),
        assistant_message.id.clone(),
        schedule_snapshot
            .model_override_id
            .clone()
            .or_else(|| legacy_profile_catalog_id(&state.settings, schedule_snapshot.model_profile_id.as_deref())),
        schedule_assistant_id(&schedule_snapshot)
            .map(|value| value.to_string())
            .or_else(|| Some(agent.id.clone())),
        Some(schedule_snapshot.web_search_enabled),
    )
    .await;

    let mut latest_state = load_state()?;
    if let Some(latest_run) = latest_state.runs.iter_mut().find(|item| item.id == run.id) {
        latest_run.ended_at = Some(now_ts());
        match &execution {
            Ok(()) => {
                latest_run.status = "success".to_string();
                latest_run.summary = Some("Schedule run completed".to_string());
            }
            Err(error) => {
                latest_run.status = "failed".to_string();
                latest_run.error_message = Some(error.clone());
            }
        }
    }
    if let Some(latest_schedule) = latest_state
        .schedules
        .iter_mut()
        .find(|item| item.id == schedule_id)
    {
        match &execution {
            Ok(()) => {
                latest_schedule.last_status = Some("success".to_string());
                latest_schedule.last_error = None;
                latest_schedule.retry_count = 0;
            }
            Err(error) => {
                let should_retry = latest_schedule.retry_count < latest_schedule.max_retries;
                if should_retry {
                    latest_schedule.retry_count += 1;
                    latest_schedule.last_status = Some("retrying".to_string());
                    latest_schedule.last_error = Some(format!("{} (retry {}/{})", error, latest_schedule.retry_count, latest_schedule.max_retries));
                    // Schedule retry in 5 minutes
                    latest_schedule.next_run_at = Some(now_ts() + 300);
                } else {
                    latest_schedule.last_status = Some("failed".to_string());
                    latest_schedule.last_error = Some(error.clone());
                    latest_schedule.retry_count = 0;
                }
            }
        }
        latest_schedule.updated_at = now_ts();
    }
    save_state(&latest_state)?;
    execution
}

pub fn init_scheduler(app: tauri::AppHandle) {
    if SCHEDULER_STARTED.set(()).is_err() {
        return;
    }

    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        // Handle misfire on startup
        if let Ok(state) = load_state() {
            let now = now_ts();
            let misfired: Vec<(String, String)> = state
                .schedules
                .iter()
                .filter(|schedule| {
                    schedule.enabled
                        && schedule.next_run_at.unwrap_or(0) < now
                        && schedule.last_run_at.unwrap_or(0) < schedule.next_run_at.unwrap_or(0)
                })
                .map(|schedule| (schedule.id.clone(), schedule.misfire_policy.clone()))
                .collect();

            for (schedule_id, policy) in misfired {
                match policy.as_str() {
                    "immediate" => {
                        let _ = trigger_schedule_run(app_clone.clone(), schedule_id).await;
                    }
                    "next_window" => {
                        if let Ok(mut state) = load_state() {
                            if let Some(schedule) = state.schedules.iter_mut().find(|s| s.id == schedule_id) {
                                schedule.next_run_at = compute_next_run_at(
                                    &schedule.trigger,
                                    now,
                                    schedule.timezone.as_deref(),
                                );
                                schedule.last_status = Some("misfire_rescheduled".to_string());
                                let _ = save_state(&state);
                            }
                        }
                    }
                    _ => { /* skip - do nothing */ }
                }
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        loop {
            let now = now_ts();
            let due = load_state()
                .map(|state| {
                    state
                        .schedules
                        .iter()
                        .filter(|schedule| schedule.enabled && schedule.next_run_at.unwrap_or(0) <= now)
                        .map(|schedule| schedule.id.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            for schedule_id in due {
                let _ = trigger_schedule_run(app_clone.clone(), schedule_id).await;
            }
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
        }
    });
}

#[tauri::command]
pub fn ai_workspace_bootstrap() -> Result<AiWorkspaceBootstrap, String> {
    let state = load_state()?;
    let mut assistants = state.agents.clone();
    assistants.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    let mut conversations = state
        .conversations
        .iter()
        .map(conversation_list_item)
        .collect::<Vec<_>>();
    conversations.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });
    let mut automations = state
        .schedules
        .iter()
        .map(|job| schedule_view(job, &state.runs))
        .collect::<Vec<_>>();
    automations.sort_by(|a, b| {
        b.job.enabled
            .cmp(&a.job.enabled)
            .then_with(|| a.job.next_run_at.unwrap_or(u64::MAX).cmp(&b.job.next_run_at.unwrap_or(u64::MAX)))
    });
    Ok(AiWorkspaceBootstrap {
        settings: state.settings,
        assistants,
        conversations,
        automations,
        quick_assistant: state.quick_assistant,
        selection_assistant: state.selection_assistant,
    })
}

#[tauri::command]
pub fn workspace_settings_get() -> Result<AiAssistantSettings, String> {
    Ok(load_state()?.settings)
}

#[tauri::command]
pub fn workspace_settings_save(settings: AiAssistantSettings) -> Result<AiAssistantSettings, String> {
    let mut state = load_state()?;
    state.settings = settings.clone();
    state.revision = now_ts();
    save_state(&state)?;
    Ok(load_state()?.settings)
}

#[tauri::command]
pub fn workspace_model_roles_get() -> Result<Vec<ModelRoleBinding>, String> {
    Ok(load_state()?.settings.role_bindings)
}

#[tauri::command]
pub fn workspace_model_roles_save(role_bindings: Vec<ModelRoleBinding>) -> Result<Vec<ModelRoleBinding>, String> {
    let mut state = load_state()?;
    state.settings.role_bindings = role_bindings;
    state.revision = now_ts();
    save_state(&state)?;
    Ok(load_state()?.settings.role_bindings)
}

#[tauri::command]
pub async fn provider_connection_test(
    input: ProviderConnectionTestInput,
) -> Result<AssistantConnectionTestResult, String> {
    let state = load_state()?;
    let provider = state
        .settings
        .providers
        .iter()
        .find(|provider| provider.id == input.provider_id)
        .ok_or_else(|| "Provider not found".to_string())?
        .clone();
    if !provider.enabled {
        return Err(format!("Provider is disabled: {}", provider.name));
    }
    let existing_catalog_count = state
        .settings
        .model_catalog
        .iter()
        .filter(|item| item.provider_id == provider.id)
        .count();
    let started = std::time::Instant::now();
    match fetch_provider_model_catalog_detailed(&provider).await {
        Ok(catalog) => Ok(AssistantConnectionTestResult {
            ok: true,
            message: format!(
                "{} connected successfully. {} model(s) discovered.",
                provider.name,
                catalog.len()
            ),
            latency_ms: started.elapsed().as_millis() as u64,
        }),
        Err(error) if error.unsupported_catalog_endpoint => Ok(AssistantConnectionTestResult {
            ok: true,
            message: if existing_catalog_count > 0 {
                format!(
                    "{} connected successfully. This provider does not expose a standard model catalog endpoint, so detection verified connectivity and kept {} existing local catalog item(s).",
                    provider.name,
                    existing_catalog_count
                )
            } else {
                format!(
                    "{} connected successfully. This provider does not expose a standard model catalog endpoint, so detection verified connectivity only.",
                    provider.name
                )
            },
            latency_ms: started.elapsed().as_millis() as u64,
        }),
        Err(error) => Err(error.message),
    }
}

#[tauri::command]
pub async fn provider_models_fetch(
    input: ProviderModelsFetchInput,
) -> Result<Vec<ModelCatalogItem>, String> {
    let mut state = load_state()?;
    let provider = state
        .settings
        .providers
        .iter()
        .find(|provider| provider.id == input.provider_id)
        .ok_or_else(|| "Provider not found".to_string())?
        .clone();
    let catalog = fetch_provider_model_catalog(&provider).await?;
    state
        .settings
        .model_catalog
        .retain(|item| item.provider_id != provider.id);
    state.settings.model_catalog.extend(catalog.clone());
    if state.settings.role_bindings.is_empty() {
        state.settings.role_bindings = build_default_role_bindings(&state.settings);
    }
    state.revision = now_ts();
    save_state(&state)?;
    Ok(catalog)
}

#[tauri::command]
pub fn workspace_assistants_list() -> Result<Vec<AgentDefinition>, String> {
    let mut assistants = load_state()?.agents;
    assistants.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(assistants)
}

#[tauri::command]
pub fn workspace_assistant_upsert(mut assistant: AgentDefinition) -> Result<AgentDefinition, String> {
    let state = load_state()?;
    let is_new = assistant.id.trim().is_empty();
    if assistant.primary_model_id.is_none() {
        assistant.primary_model_id = default_role_model_id(&state.settings, "assistant");
    }
    if assistant.light_model_id.is_none() {
        assistant.light_model_id = default_role_model_id(&state.settings, "summary");
    }
    drop(state);
    if is_new && assistant.mcp_server_ids.is_empty() {
        assistant.mcp_server_ids = crate::assistant_mcp::ensure_default_assistant_mcp_server_ids()?;
    }
    upsert_agent(assistant)
}

#[tauri::command]
pub fn workspace_assistant_delete(assistant_id: String) -> Result<bool, String> {
    let mut state = load_state()?;
    let before = state.agents.len();
    state.agents.retain(|assistant| assistant.id != assistant_id);
    for conversation in &mut state.conversations {
        if conversation.assistant_id.as_deref() == Some(assistant_id.as_str()) {
            conversation.assistant_id = None;
        }
    }
    for schedule in &mut state.schedules {
        if schedule_assistant_id(schedule) == Some(assistant_id.as_str()) {
            schedule.assistant_id = None;
            schedule.agent_id.clear();
        }
    }
    if state.quick_assistant.preferred_assistant_id.as_deref() == Some(assistant_id.as_str()) {
        state.quick_assistant.preferred_assistant_id = None;
    }
    save_state(&state)?;
    Ok(before != state.agents.len())
}

#[tauri::command]
pub async fn workspace_assistant_test_run(
    app: tauri::AppHandle,
    input: AgentTestRunInput,
) -> Result<AgentTestRunResult, String> {
    let assistant = load_state()?
        .agents
        .into_iter()
        .find(|assistant| assistant.id == input.agent_id)
        .ok_or_else(|| "Assistant not found".to_string())?;
    let conversation = workspace_conversation_create(Some(WorkspaceConversationCreateInput {
        title: Some(format!("{} Topic", assistant.name)),
        assistant_id: Some(assistant.id.clone()),
        model_override_id: assistant.primary_model_id.clone(),
    }))?;
    let _ = workspace_conversation_send(
        app,
        WorkspaceConversationSendInput {
            conversation_id: conversation.id.clone(),
            content: input.prompt,
            assistant_id: Some(assistant.id),
            model_override_id: None,
            web_search_enabled: Some(assistant.tool_policy.web_search),
        },
    )
    .await?;
    Ok(AgentTestRunResult {
        conversation_id: conversation.id,
    })
}

#[tauri::command]
pub fn workspace_conversations_list() -> Result<Vec<AssistantConversationListItem>, String> {
    let mut items = load_state()?
        .conversations
        .iter()
        .map(conversation_list_item)
        .collect::<Vec<_>>();
    items.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });
    Ok(items)
}

#[tauri::command]
pub fn workspace_conversation_get(conversation_id: String) -> Result<AssistantConversation, String> {
    load_state()?
        .conversations
        .into_iter()
        .find(|conversation| conversation.id == conversation_id)
        .ok_or_else(|| "Conversation not found".to_string())
}

#[tauri::command]
pub fn workspace_conversation_create(
    input: Option<WorkspaceConversationCreateInput>,
) -> Result<AssistantConversation, String> {
    let mut state = load_state()?;
    let now = now_ts();
    let requested_assistant_id = input
        .as_ref()
        .and_then(|payload| payload.assistant_id.clone())
        .or_else(|| state.quick_assistant.preferred_assistant_id.clone());
    let assistant = requested_assistant_id
        .as_deref()
        .and_then(|id| state.agents.iter().find(|assistant| assistant.id == id))
        .cloned();
    let model_override_id = input
        .as_ref()
        .and_then(|payload| payload.model_override_id.clone())
        .or_else(|| assistant.as_ref().and_then(|item| item.primary_model_id.clone()))
        .or_else(|| default_role_model_id(&state.settings, if assistant.is_some() { "assistant" } else { "chat" }));
    let title = input
        .as_ref()
        .and_then(|payload| payload.title.clone())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "新主题".to_string());

    let conversation = AssistantConversation {
        id: uuid::Uuid::new_v4().to_string(),
        title,
        pinned: false,
        archived: false,
        created_at: now,
        updated_at: now,
        assistant_id: assistant.as_ref().map(|item| item.id.clone()),
        model_profile_id: None,
        model_override_id,
        web_search_enabled: assistant.as_ref().map(|item| item.tool_policy.web_search).unwrap_or(false),
        capability_snapshot: Some(capability_snapshot_from_agent(
            assistant.as_ref(),
            assistant.as_ref().map(|item| item.tool_policy.web_search).unwrap_or(false),
        )),
        context_reset_count: 0,
        messages: Vec::new(),
    };
    state.conversations.insert(0, conversation.clone());
    save_state(&state)?;
    Ok(conversation)
}

#[tauri::command]
pub fn workspace_conversation_update(
    input: WorkspaceConversationUpdateInput,
) -> Result<AssistantConversation, String> {
    let mut state = load_state()?;
    let assistant_override = input
        .assistant_id
        .as_deref()
        .and_then(|id| state.agents.iter().find(|assistant| assistant.id == id))
        .cloned();
    let conversation = state
        .conversations
        .iter_mut()
        .find(|conversation| conversation.id == input.conversation_id)
        .ok_or_else(|| "Conversation not found".to_string())?;
    if let Some(title) = input.title {
        conversation.title = title.trim().to_string();
    }
    if let Some(pinned) = input.pinned {
        conversation.pinned = pinned;
    }
    if let Some(archived) = input.archived {
        conversation.archived = archived;
    }
    if let Some(assistant_id) = input.assistant_id {
        conversation.assistant_id = if assistant_id.trim().is_empty() {
            None
        } else {
            Some(assistant_id.trim().to_string())
        };
    }
    if let Some(model_override_id) = input.model_override_id {
        conversation.model_override_id = if model_override_id.trim().is_empty() {
            None
        } else {
            Some(model_override_id.trim().to_string())
        };
    }
    if let Some(web_search_enabled) = input.web_search_enabled {
        conversation.web_search_enabled = web_search_enabled;
    }
    conversation.capability_snapshot = Some(capability_snapshot_from_agent(
        assistant_override
            .as_ref()
            .or_else(|| {
                conversation
                    .assistant_id
                    .as_deref()
                    .and_then(|id| state.agents.iter().find(|assistant| assistant.id == id))
            }),
        conversation.web_search_enabled,
    ));
    conversation.updated_at = now_ts();
    let updated = conversation.clone();
    save_state(&state)?;
    Ok(updated)
}

#[tauri::command]
pub fn workspace_conversation_delete(conversation_id: String) -> Result<bool, String> {
    let mut state = load_state()?;
    let before = state.conversations.len();
    state.conversations.retain(|conversation| conversation.id != conversation_id);
    save_state(&state)?;
    Ok(before != state.conversations.len())
}

#[tauri::command]
pub fn workspace_conversation_reset_context(conversation_id: String) -> Result<AssistantConversation, String> {
    let mut state = load_state()?;
    let conversation = state
        .conversations
        .iter_mut()
        .find(|conversation| conversation.id == conversation_id)
        .ok_or_else(|| "Conversation not found".to_string())?;
    conversation.messages.push(AssistantMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: "context_reset".to_string(),
        content: "上下文已重置".to_string(),
        reasoning: None,
        sources: Vec::new(),
        tool_calls: Vec::new(),
        tool_call_id: None,
        schedule_draft: None,
        created_at: now_ts(),
        status: "done".to_string(),
    });
    conversation.context_reset_count = conversation.context_reset_count.saturating_add(1);
    conversation.updated_at = now_ts();
    let updated = conversation.clone();
    save_state(&state)?;
    Ok(updated)
}

#[tauri::command]
pub async fn workspace_schedule_resolve_draft(
    app: tauri::AppHandle,
    input: ScheduleDraftResolveInput,
) -> Result<AssistantConversation, String> {
    assistant_schedule_resolve_draft(app, input).await
}

#[tauri::command]
pub async fn workspace_conversation_send(
    app: tauri::AppHandle,
    input: WorkspaceConversationSendInput,
) -> Result<AssistantSendResult, String> {
    let mut state = load_state()?;
    let conversation_index = state
        .conversations
        .iter()
        .position(|conversation| conversation.id == input.conversation_id)
        .ok_or_else(|| "Conversation not found".to_string())?;
    let assistant = input
        .assistant_id
        .as_deref()
        .or(state.conversations[conversation_index].assistant_id.as_deref())
        .and_then(|id| state.agents.iter().find(|assistant| assistant.id == id))
        .cloned();
    let mut schedule_draft = build_schedule_draft(&state, input.content.trim());
    if let Some(draft) = schedule_draft.as_mut() {
        if let Some(schedule) = draft.schedule.as_mut() {
            if schedule.assistant_id.as_deref().map(|value| value.trim().is_empty()).unwrap_or(true) {
                if let Some(assistant) = assistant.as_ref() {
                    schedule.assistant_id = Some(assistant.id.clone());
                    schedule.agent_id = assistant.id.clone();
                    if schedule.model_override_id.is_none() {
                        schedule.model_override_id = assistant.primary_model_id.clone();
                    }
                }
            }
            if schedule.model_override_id.is_none() {
                schedule.model_override_id = input
                    .model_override_id
                    .clone()
                    .or_else(|| {
                        state.conversations[conversation_index]
                            .model_override_id
                            .clone()
                    })
                    .or_else(|| default_role_model_id(&state.settings, "automation"));
            }
        }
        if draft.agent_name.is_none() {
            draft.agent_name = assistant.as_ref().map(|item| item.name.clone());
        }
    }

    let user_message = new_message("user", input.content.trim().to_string(), "done");
    let mut assistant_message = new_message("assistant", String::new(), "streaming");
    if let Some(draft) = schedule_draft.clone() {
        assistant_message.content = draft.summary.clone();
        assistant_message.status = "done".to_string();
        assistant_message.schedule_draft = Some(draft);
    }
    let conversation = &mut state.conversations[conversation_index];
    if let Some(assistant_id) = input.assistant_id.clone() {
        conversation.assistant_id = if assistant_id.trim().is_empty() {
            None
        } else {
            Some(assistant_id)
        };
    }
    if let Some(model_override_id) = input.model_override_id.clone() {
        conversation.model_override_id = if model_override_id.trim().is_empty() {
            None
        } else {
            Some(model_override_id)
        };
    }
    if let Some(web_search_enabled) = input.web_search_enabled {
        conversation.web_search_enabled = web_search_enabled;
    }
    conversation.capability_snapshot =
        Some(capability_snapshot_from_agent(assistant.as_ref(), conversation.web_search_enabled));
    conversation.messages.push(user_message.clone());
    conversation.messages.push(assistant_message.clone());
    conversation.updated_at = now_ts();
    if conversation.title.trim().is_empty() || conversation.title == "新主题" {
        conversation.title = derive_title(&user_message.content);
    }
    save_state(&state)?;

    if schedule_draft.is_some() {
        return Ok(AssistantSendResult {
            conversation_id: input.conversation_id,
            user_message_id: user_message.id,
            assistant_message_id: assistant_message.id,
        });
    }

    let app_handle = app.clone();
    let conversation_id = input.conversation_id.clone();
    let assistant_message_id = assistant_message.id.clone();
    let model_override_id = input.model_override_id.clone();
    let assistant_id = input.assistant_id.clone();
    let web_search_enabled = input.web_search_enabled;
    tauri::async_runtime::spawn(async move {
        if let Err(error) = execute_workspace_conversation_run(
            app_handle.clone(),
            conversation_id.clone(),
            assistant_message_id.clone(),
            model_override_id,
            assistant_id,
            web_search_enabled,
        )
        .await
        {
            let _ = save_message_result(
                &conversation_id,
                &assistant_message_id,
                "",
                None,
                Vec::new(),
                Vec::new(),
                "failed",
            );
            emit_stream_event(
                &app_handle,
                AssistantStreamEvent {
                    conversation_id,
                    message_id: assistant_message_id,
                    kind: "message.failed".to_string(),
                    text: None,
                    sources: None,
                    tool: None,
                    error: Some(error),
                },
            );
        }
    });

    Ok(AssistantSendResult {
        conversation_id: input.conversation_id,
        user_message_id: user_message.id,
        assistant_message_id: assistant_message.id,
    })
}

#[tauri::command]
pub fn workspace_automations_list() -> Result<Vec<ScheduleJobView>, String> {
    assistant_schedules_list()
}

#[tauri::command]
pub fn workspace_automation_upsert(mut schedule: ScheduleJob) -> Result<ScheduleJob, String> {
    if let Some(assistant_id) = schedule.assistant_id.clone().filter(|value| !value.trim().is_empty()) {
        schedule.agent_id = assistant_id;
    } else if !schedule.agent_id.trim().is_empty() {
        schedule.assistant_id = Some(schedule.agent_id.clone());
    }
    assistant_schedule_upsert(schedule)
}

#[tauri::command]
pub fn workspace_automation_delete(schedule_id: String) -> Result<bool, String> {
    assistant_schedule_delete(schedule_id)
}

#[tauri::command]
pub fn workspace_automation_toggle(input: ScheduleToggleInput) -> Result<ScheduleJob, String> {
    assistant_schedule_toggle(input)
}

#[tauri::command]
pub async fn workspace_automation_run_now(
    app: tauri::AppHandle,
    input: ScheduleRunNowInput,
) -> Result<bool, String> {
    assistant_schedule_run_now(app, input).await
}

#[tauri::command]
pub fn workspace_quick_assistant_get() -> Result<QuickAssistantPreferences, String> {
    Ok(load_state()?.quick_assistant)
}

#[tauri::command]
pub fn workspace_quick_assistant_save(
    preferences: QuickAssistantPreferences,
) -> Result<QuickAssistantPreferences, String> {
    let mut state = load_state()?;
    state.quick_assistant = preferences.clone();
    state.revision = now_ts();
    save_state(&state)?;
    Ok(preferences)
}

#[tauri::command]
pub fn workspace_selection_assistant_get() -> Result<SelectionAssistantPreferences, String> {
    Ok(load_state()?.selection_assistant)
}

#[tauri::command]
pub fn workspace_selection_assistant_save(
    preferences: SelectionAssistantPreferences,
) -> Result<SelectionAssistantPreferences, String> {
    let mut state = load_state()?;
    state.selection_assistant = preferences.clone();
    state.revision = now_ts();
    save_state(&state)?;
    Ok(preferences)
}

#[tauri::command]
pub async fn assistant_schedule_resolve_draft(
    app: tauri::AppHandle,
    input: ScheduleDraftResolveInput,
) -> Result<AssistantConversation, String> {
    let mut state = load_state()?;
    let conversation_index = state
        .conversations
        .iter()
        .position(|conversation| conversation.id == input.conversation_id)
        .ok_or_else(|| "Conversation not found".to_string())?;
    let message_index = state.conversations[conversation_index]
        .messages
        .iter()
        .position(|message| message.id == input.message_id)
        .ok_or_else(|| "Draft message not found".to_string())?;
    let draft = state.conversations[conversation_index].messages[message_index]
        .schedule_draft
        .clone()
        .ok_or_else(|| "No schedule draft found on this message".to_string())?;

    let now = now_ts();
    if !input.approved {
        let message = &mut state.conversations[conversation_index].messages[message_index];
        message.content = "已取消本次定时任务变更。".to_string();
        message.schedule_draft = None;
        message.tool_calls = vec![AssistantToolCall {
            id: uuid::Uuid::new_v4().to_string(),
            name: "schedule.cancel".to_string(),
            arguments: None,
            status: "cancelled".to_string(),
            summary: Some("Schedule draft was cancelled".to_string()),
            result: None,
            started_at: now,
            finished_at: Some(now),
        }];
        state.conversations[conversation_index].updated_at = now;
        let updated = state.conversations[conversation_index].clone();
        save_state(&state)?;
        return Ok(updated);
    }

    let mut tool_name = "schedule.update".to_string();
    let result_message = match draft.action.as_str() {
        "create" | "update" => {
            let mut schedule = draft
                .schedule
                .clone()
                .ok_or_else(|| "Draft schedule payload is missing".to_string())?;
            if schedule.id.trim().is_empty() {
                schedule.id = uuid::Uuid::new_v4().to_string();
                schedule.created_at = now;
                tool_name = "schedule.create".to_string();
            } else if schedule.created_at == 0 {
                schedule.created_at = now;
            }
            if schedule.output_target.trim().is_empty() {
                schedule.output_target = "assistant_conversation".to_string();
            }
            schedule.updated_at = now;
            schedule.next_run_at = if schedule.enabled {
                compute_next_run_at(&schedule.trigger, now, schedule.timezone.as_deref())
            } else {
                None
            };
            if let Some(existing) = state.schedules.iter_mut().find(|item| item.id == schedule.id) {
                *existing = schedule.clone();
                format!(
                    "已更新定时任务“{}”，计划：{}。",
                    schedule.name,
                    format_trigger_label(&schedule.trigger)
                )
            } else {
                state.schedules.push(schedule.clone());
                format!(
                    "已创建定时任务“{}”，计划：{}。",
                    schedule.name,
                    format_trigger_label(&schedule.trigger)
                )
            }
        }
        "toggle_off" | "toggle_on" => {
            let schedule_id = draft
                .target_schedule_id
                .clone()
                .ok_or_else(|| "Target schedule is missing".to_string())?;
            let enabled = draft.desired_enabled.unwrap_or(draft.action == "toggle_on");
            let schedule = state
                .schedules
                .iter_mut()
                .find(|item| item.id == schedule_id)
                .ok_or_else(|| "Target schedule not found".to_string())?;
            schedule.enabled = enabled;
            schedule.updated_at = now;
            schedule.next_run_at = if enabled {
                compute_next_run_at(&schedule.trigger, now, schedule.timezone.as_deref())
            } else {
                None
            };
            tool_name = if enabled {
                "schedule.enable".to_string()
            } else {
                "schedule.pause".to_string()
            };
            if enabled {
                format!("已启用定时任务“{}”。", schedule.name)
            } else {
                format!("已暂停定时任务“{}”。", schedule.name)
            }
        }
        "delete" => {
            let schedule_id = draft
                .target_schedule_id
                .clone()
                .ok_or_else(|| "Target schedule is missing".to_string())?;
            let schedule_name = draft
                .target_schedule_name
                .clone()
                .unwrap_or_else(|| "未命名任务".to_string());
            state.schedules.retain(|item| item.id != schedule_id);
            state.runs.retain(|run| run.schedule_id != schedule_id);
            tool_name = "schedule.delete".to_string();
            format!("已删除定时任务“{}”。", schedule_name)
        }
        "run_now" => {
            let schedule_id = draft
                .target_schedule_id
                .clone()
                .ok_or_else(|| "Target schedule is missing".to_string())?;
            let schedule_name = draft
                .target_schedule_name
                .clone()
                .unwrap_or_else(|| "未命名任务".to_string());
            tool_name = "schedule.run_now".to_string();
            let result_message = format!("已提交立即执行请求：定时任务“{}”。", schedule_name);
            save_state(&state)?;
            trigger_schedule_run(app, schedule_id).await?;
            let mut refreshed = load_state()?;
            let conversation = refreshed
                .conversations
                .iter_mut()
                .find(|conversation| conversation.id == input.conversation_id)
                .ok_or_else(|| "Conversation not found after run".to_string())?;
            let message = conversation
                .messages
                .iter_mut()
                .find(|message| message.id == input.message_id)
                .ok_or_else(|| "Draft message not found after run".to_string())?;
            message.content = result_message.clone();
            message.schedule_draft = None;
            message.tool_calls = vec![AssistantToolCall {
                id: uuid::Uuid::new_v4().to_string(),
                name: tool_name,
                arguments: None,
                status: "success".to_string(),
                summary: Some(result_message.clone()),
                result: None,
                started_at: now,
                finished_at: Some(now_ts()),
            }];
            conversation.updated_at = now_ts();
            let updated = conversation.clone();
            save_state(&refreshed)?;
            return Ok(updated);
        }
        _ => return Err("Unsupported draft action".to_string()),
    };

    let finished_at = now_ts();
    let conversation = &mut state.conversations[conversation_index];
    let message = &mut conversation.messages[message_index];
    message.content = result_message.clone();
    message.schedule_draft = None;
    message.tool_calls = vec![AssistantToolCall {
        id: uuid::Uuid::new_v4().to_string(),
        name: tool_name,
        arguments: None,
        status: "success".to_string(),
        summary: Some(result_message.clone()),
        result: None,
        started_at: now,
        finished_at: Some(finished_at),
    }];
    conversation.updated_at = finished_at;
    let updated = conversation.clone();
    save_state(&state)?;
    Ok(updated)
}

#[tauri::command]
pub fn assistant_schedules_list() -> Result<Vec<ScheduleJobView>, String> {
    let mut state = load_state()?;
    state.runs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    let mut schedules = state
        .schedules
        .iter()
        .map(|job| schedule_view(job, &state.runs))
        .collect::<Vec<_>>();
    schedules.sort_by(|a, b| {
        b.job.enabled
            .cmp(&a.job.enabled)
            .then_with(|| a.job.next_run_at.unwrap_or(u64::MAX).cmp(&b.job.next_run_at.unwrap_or(u64::MAX)))
    });
    Ok(schedules)
}

#[tauri::command]
pub fn assistant_schedule_upsert(mut schedule: ScheduleJob) -> Result<ScheduleJob, String> {
    let mut state = load_state()?;
    let now = now_ts();
    if schedule.id.trim().is_empty() {
        schedule.id = uuid::Uuid::new_v4().to_string();
        schedule.created_at = now;
    }
    if schedule.created_at == 0 {
        schedule.created_at = now;
    }
    schedule.updated_at = now;
    if schedule.output_target.trim().is_empty() {
        schedule.output_target = "assistant_conversation".to_string();
    }
    schedule.next_run_at = if schedule.enabled {
        compute_next_run_at(&schedule.trigger, now, schedule.timezone.as_deref())
    } else {
        None
    };

    if let Some(existing) = state.schedules.iter_mut().find(|item| item.id == schedule.id) {
        *existing = schedule.clone();
    } else {
        state.schedules.push(schedule.clone());
    }
    save_state(&state)?;
    Ok(schedule)
}

#[tauri::command]
pub fn assistant_schedule_delete(schedule_id: String) -> Result<bool, String> {
    let mut state = load_state()?;
    let before = state.schedules.len();
    state.schedules.retain(|schedule| schedule.id != schedule_id);
    state.runs.retain(|run| run.schedule_id != schedule_id);
    save_state(&state)?;
    Ok(before != state.schedules.len())
}

#[tauri::command]
pub fn assistant_schedule_toggle(input: ScheduleToggleInput) -> Result<ScheduleJob, String> {
    let mut state = load_state()?;
    let schedule = state
        .schedules
        .iter_mut()
        .find(|schedule| schedule.id == input.schedule_id)
        .ok_or_else(|| "Schedule not found".to_string())?;
    schedule.enabled = input.enabled;
    schedule.updated_at = now_ts();
    schedule.next_run_at = if input.enabled {
        compute_next_run_at(&schedule.trigger, now_ts(), schedule.timezone.as_deref())
    } else {
        None
    };
    let updated = schedule.clone();
    save_state(&state)?;
    Ok(updated)
}

#[tauri::command]
pub async fn assistant_schedule_run_now(
    app: tauri::AppHandle,
    input: ScheduleRunNowInput,
) -> Result<bool, String> {
    trigger_schedule_run(app, input.schedule_id).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;

    fn openai_provider(base_url: &str) -> AiAssistantProvider {
        AiAssistantProvider {
            id: "provider-test".to_string(),
            name: "Provider Test".to_string(),
            protocol: "openai-compatible".to_string(),
            base_url: base_url.to_string(),
            auth_scheme: default_bearer(),
            api_key: "sk-test".to_string(),
            enabled: true,
            extra_headers: Vec::new(),
            capabilities: AssistantProviderCapability {
                supports_reasoning: true,
                supports_streaming: true,
                supports_web_search: false,
            },
        }
    }

    fn test_agent() -> AgentDefinition {
        AgentDefinition {
            id: "agent-1".to_string(),
            name: "Search Assistant".to_string(),
            avatar_emoji: None,
            description: String::new(),
            system_prompt: "Be helpful.".to_string(),
            primary_model_id: None,
            light_model_id: None,
            default_model_profile_id: None,
            light_model_profile_id: None,
            tool_policy: AgentToolPolicy {
                web_search: true,
                workspace_read: true,
                notes_search: false,
            },
            knowledge_base_ids: vec!["kb-product".to_string()],
            mcp_server_ids: vec!["mcp-exa".to_string(), "mcp-context7".to_string()],
            memory_enabled: false,
            output_contract: String::new(),
            created_at: 1,
            updated_at: 1,
        }
    }

    fn exa_binding() -> BoundMcpTool {
        BoundMcpTool {
            assistant_tool_name: "mcp__exa__web_search_exa".to_string(),
            server_id: "mcp-exa".to_string(),
            server_name: "Exa MCP".to_string(),
            config_key: "exa".to_string(),
            original_tool_name: "web_search_exa".to_string(),
            category: crate::assistant_mcp::McpCategory::Search,
            definition: ToolDefinition {
                name: "mcp__exa__web_search_exa".to_string(),
                description: "Search the web".to_string(),
                parameters: Some(json!({"type": "object"})),
            },
        }
    }

    #[test]
    fn resolve_provider_endpoint_accepts_full_chat_completion_url() {
        let provider =
            openai_provider("https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions");

        let endpoint = resolve_provider_endpoint(&provider, "models");

        assert_eq!(
            endpoint,
            "https://dashscope.aliyuncs.com/compatible-mode/v1/models"
        );
    }

    #[test]
    fn parse_provider_model_catalog_supports_nested_data_models() {
        let provider = openai_provider("https://dashscope.aliyuncs.com/compatible-mode/v1");
        let payload = json!({
            "data": {
                "models": [
                    {
                        "name": "qwen-plus",
                        "display_name": "Qwen Plus",
                        "description": "Aliyun Bailian model"
                    }
                ]
            }
        });

        let catalog = parse_provider_model_catalog(&provider, &payload);

        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].model_id, "qwen-plus");
        assert_eq!(catalog[0].label, "Qwen Plus");
    }

    #[test]
    fn unsupported_model_catalog_statuses_are_treated_as_connectivity_only() {
        assert!(is_unsupported_model_catalog_status(StatusCode::METHOD_NOT_ALLOWED));
        assert!(is_unsupported_model_catalog_status(StatusCode::NOT_FOUND));
        assert!(is_unsupported_model_catalog_status(StatusCode::NOT_IMPLEMENTED));
        assert!(!is_unsupported_model_catalog_status(StatusCode::UNAUTHORIZED));
    }

    #[test]
    fn capability_snapshot_uses_conversation_search_toggle() {
        let agent = test_agent();

        let capability = capability_snapshot_from_agent(Some(&agent), false);

        assert!(!capability.web_search);
        assert!(capability.workspace_read);
        assert_eq!(
            capability.mcp_server_ids,
            vec!["mcp-exa".to_string(), "mcp-context7".to_string()]
        );
    }

    #[test]
    fn builtin_tools_do_not_include_legacy_web_search_tool() {
        let tools = build_builtin_tools(&AgentToolPolicy {
            web_search: true,
            workspace_read: true,
            notes_search: true,
        });
        let names = tools.into_iter().map(|tool| tool.name).collect::<Vec<_>>();

        assert!(names.contains(&"workspace_read".to_string()));
        assert!(names.contains(&"notes_search".to_string()));
        assert!(!names.contains(&"web_search".to_string()));
    }

    #[test]
    fn exa_source_extraction_maps_common_result_shape() {
        let output = McpToolCallOutput {
            text: String::new(),
            structured_content: Some(json!({
                "results": [
                    {
                        "title": "Exa Result",
                        "url": "https://example.com/article",
                        "snippet": "Latest facts"
                    }
                ]
            })),
            raw_result: Value::Null,
        };

        let sources = extract_sources_from_mcp_output(&exa_binding(), &output);

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].title, "Exa Result");
        assert_eq!(sources[0].url, "https://example.com/article");
        assert_eq!(sources[0].snippet, "Latest facts");
    }

    #[test]
    fn system_prompt_describes_docs_availability_when_search_tools_are_disabled() {
        let conversation = AssistantConversation {
            id: "conv-1".to_string(),
            title: "Docs".to_string(),
            pinned: false,
            archived: false,
            created_at: 1,
            updated_at: 1,
            assistant_id: None,
            model_profile_id: None,
            model_override_id: None,
            web_search_enabled: false,
            capability_snapshot: None,
            context_reset_count: 0,
            messages: Vec::new(),
        };
        let prompt = build_system_prompt(
            &conversation,
            None,
            &[],
            &[ToolDefinition {
                name: "mcp__context7__query_docs".to_string(),
                description: "Query docs".to_string(),
                parameters: Some(json!({"type": "object"})),
            }],
        );

        assert!(prompt.contains("Search-class MCP tools are disabled"));
        assert!(prompt.contains("Documentation MCP tools may still be available"));
    }
}
