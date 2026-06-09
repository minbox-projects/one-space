use super::{default_agents, default_assistant_settings, default_bearer, default_true};
use crate::get_data_dir;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

pub(in crate::ai_assistant) const ASSISTANT_STREAM_EVENT: &str = "assistant-stream";
pub(in crate::ai_assistant) const DEFAULT_STATE_FILE: &str = "ai_workspace_state.json";

pub(in crate::ai_assistant) static STATE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
pub(in crate::ai_assistant) static RUNNING_SCHEDULES: OnceLock<Mutex<HashSet<String>>> =
    OnceLock::new();
pub(in crate::ai_assistant) static SCHEDULER_STARTED: OnceLock<()> = OnceLock::new();

pub(in crate::ai_assistant) fn state_lock() -> &'static Mutex<()> {
    STATE_LOCK.get_or_init(|| Mutex::new(()))
}

pub(in crate::ai_assistant) fn running_schedules() -> &'static Mutex<HashSet<String>> {
    RUNNING_SCHEDULES.get_or_init(|| Mutex::new(HashSet::new()))
}

pub(in crate::ai_assistant) fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(in crate::ai_assistant) fn state_path() -> Result<PathBuf, String> {
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
    pub display_name: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
    #[serde(default)]
    pub server_id: Option<String>,
    #[serde(default)]
    pub server_name: Option<String>,
    #[serde(default)]
    pub original_tool_name: Option<String>,
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
pub(in crate::ai_assistant) struct AssistantState {
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
