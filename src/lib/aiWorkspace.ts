import { invoke } from '@tauri-apps/api/core';

export interface ProviderHeader {
  key: string;
  value: string;
}

export interface AssistantProviderCapability {
  supports_reasoning: boolean;
  supports_streaming: boolean;
  supports_web_search: boolean;
}

export interface ProviderConnection {
  id: string;
  name: string;
  protocol: string;
  base_url: string;
  auth_scheme: string;
  api_key: string;
  enabled: boolean;
  extra_headers: ProviderHeader[];
  capabilities: AssistantProviderCapability;
}

export interface SearchProviderConnection {
  id: string;
  name: string;
  provider_type: string;
  base_url?: string | null;
  api_key: string;
  enabled: boolean;
  timeout_secs?: number | null;
  max_results?: number | null;
}

export interface ModelCatalogItem {
  id: string;
  provider_id: string;
  model_id: string;
  label: string;
  description: string;
  enabled: boolean;
  tags: string[];
  supports_reasoning: boolean;
  supports_streaming: boolean;
  supports_web_search: boolean;
  created_at: number;
  updated_at: number;
}

export interface ModelRoleBinding {
  id: string;
  role: string;
  model_id?: string | null;
  runtime_preset_id?: string | null;
  temperature?: number | null;
  max_tokens?: number | null;
  enable_reasoning: boolean;
  search_provider_id?: string | null;
}

export interface RuntimePreset {
  id: string;
  name: string;
  description: string;
  temperature?: number | null;
  max_tokens?: number | null;
  enable_reasoning: boolean;
}

export interface AiWorkspaceSettings {
  providers: ProviderConnection[];
  search_providers: SearchProviderConnection[];
  model_catalog: ModelCatalogItem[];
  role_bindings: ModelRoleBinding[];
  runtime_presets: RuntimePreset[];
  profiles?: unknown[];
  default_chat_profile_id?: string | null;
  default_agent_profile_id?: string | null;
  default_summary_profile_id?: string | null;
  active_search_provider_id?: string | null;
}

export interface AssistantCapabilitySnapshot {
  web_search: boolean;
  workspace_read: boolean;
  notes_search: boolean;
  knowledge_base_ids: string[];
  mcp_server_ids: string[];
  memory_enabled: boolean;
}

export interface AssistantPreset {
  id: string;
  name: string;
  avatar_emoji?: string | null;
  description: string;
  system_prompt: string;
  primary_model_id?: string | null;
  light_model_id?: string | null;
  default_model_profile_id?: string | null;
  light_model_profile_id?: string | null;
  tool_policy: {
    web_search: boolean;
    workspace_read: boolean;
    notes_search: boolean;
  };
  knowledge_base_ids: string[];
  mcp_server_ids: string[];
  memory_enabled: boolean;
  output_contract: string;
  created_at: number;
  updated_at: number;
}

export interface AssistantMessageSource {
  title: string;
  url: string;
  snippet: string;
}

export interface AssistantToolCall {
  name: string;
  status: string;
  summary?: string | null;
  started_at: number;
  finished_at?: number | null;
}

export interface AssistantScheduleDraft {
  action: string;
  title: string;
  summary: string;
  schedule?: AutomationJob | null;
  target_schedule_id?: string | null;
  target_schedule_name?: string | null;
  desired_enabled?: boolean | null;
  agent_name?: string | null;
  trigger_label?: string | null;
}

export interface AssistantMessage {
  id: string;
  role: string;
  content: string;
  reasoning?: string | null;
  sources: AssistantMessageSource[];
  tool_calls: AssistantToolCall[];
  schedule_draft?: AssistantScheduleDraft | null;
  created_at: number;
  status: string;
}

export interface AssistantConversation {
  id: string;
  title: string;
  pinned: boolean;
  archived: boolean;
  created_at: number;
  updated_at: number;
  assistant_id?: string | null;
  model_profile_id?: string | null;
  model_override_id?: string | null;
  web_search_enabled: boolean;
  capability_snapshot?: AssistantCapabilitySnapshot | null;
  context_reset_count: number;
  messages: AssistantMessage[];
}

export interface AssistantConversationListItem {
  id: string;
  title: string;
  pinned: boolean;
  archived: boolean;
  created_at: number;
  updated_at: number;
  message_count: number;
  preview: string;
  search_text: string;
  assistant_id?: string | null;
  model_profile_id?: string | null;
  model_override_id?: string | null;
  web_search_enabled: boolean;
  context_reset_count: number;
}

export interface ScheduleTrigger {
  kind: string;
  interval_minutes?: number | null;
  time_of_day?: string | null;
  weekdays: number[];
}

export interface AutomationJob {
  id: string;
  name: string;
  assistant_id?: string | null;
  agent_id: string;
  prompt: string;
  model_profile_id?: string | null;
  model_override_id?: string | null;
  web_search_enabled: boolean;
  trigger: ScheduleTrigger;
  timezone?: string | null;
  output_target: string;
  conversation_id?: string | null;
  enabled: boolean;
  next_run_at?: number | null;
  last_run_at?: number | null;
  last_status?: string | null;
  last_error?: string | null;
  created_at: number;
  updated_at: number;
}

export interface AutomationRun {
  id: string;
  schedule_id: string;
  started_at: number;
  ended_at?: number | null;
  status: string;
  summary?: string | null;
  error_message?: string | null;
  conversation_id?: string | null;
}

export interface AutomationJobView {
  recent_runs: AutomationRun[];
  job: AutomationJob;
}

export interface QuickAssistantPreferences {
  preferred_assistant_id?: string | null;
  preferred_role: string;
  prefer_assistant_mode: boolean;
  read_clipboard_on_open: boolean;
}

export interface SelectionAssistantPreferences {
  preferred_assistant_id?: string | null;
  preferred_role: string;
  prefer_assistant_mode: boolean;
  read_clipboard_on_open: boolean;
}

export interface AssistantSendResult {
  conversation_id: string;
  user_message_id: string;
  assistant_message_id: string;
}

export interface AssistantStreamEvent {
  conversation_id: string;
  message_id: string;
  kind: string;
  text?: string | null;
  sources?: AssistantMessageSource[] | null;
  tool?: AssistantToolCall | null;
  error?: string | null;
}

export interface AssistantConnectionTestResult {
  ok: boolean;
  message: string;
  latency_ms: number;
}

export interface AiWorkspaceBootstrap {
  settings: AiWorkspaceSettings;
  assistants: AssistantPreset[];
  conversations: AssistantConversationListItem[];
  automations: AutomationJobView[];
  quick_assistant: QuickAssistantPreferences;
  selection_assistant: SelectionAssistantPreferences;
}

export async function aiWorkspaceBootstrap() {
  return invoke<AiWorkspaceBootstrap>('ai_workspace_bootstrap');
}

export async function workspaceSettingsGet() {
  return invoke<AiWorkspaceSettings>('workspace_settings_get');
}

export async function workspaceSettingsSave(settings: AiWorkspaceSettings) {
  return invoke<AiWorkspaceSettings>('workspace_settings_save', { settings });
}

export async function workspaceModelRolesGet() {
  return invoke<ModelRoleBinding[]>('workspace_model_roles_get');
}

export async function workspaceModelRolesSave(roleBindings: ModelRoleBinding[]) {
  return invoke<ModelRoleBinding[]>('workspace_model_roles_save', { roleBindings });
}

export async function providerConnectionTest(input: { provider_id: string }) {
  return invoke<AssistantConnectionTestResult>('provider_connection_test', { input });
}

export async function providerModelsFetch(input: { provider_id: string }) {
  return invoke<ModelCatalogItem[]>('provider_models_fetch', { input });
}

export async function searchConnectionTest(input: { provider_id: string }) {
  return invoke<AssistantConnectionTestResult>('search_connection_test', { input });
}

export async function workspaceAssistantsList() {
  return invoke<AssistantPreset[]>('workspace_assistants_list');
}

export async function workspaceAssistantUpsert(assistant: AssistantPreset) {
  return invoke<AssistantPreset>('workspace_assistant_upsert', { assistant });
}

export async function workspaceAssistantDelete(assistantId: string) {
  return invoke<boolean>('workspace_assistant_delete', { assistantId });
}

export async function workspaceAssistantTestRun(input: { agent_id: string; prompt: string }) {
  return invoke<{ conversation_id: string }>('workspace_assistant_test_run', { input });
}

export async function workspaceConversationsList() {
  return invoke<AssistantConversationListItem[]>('workspace_conversations_list');
}

export async function workspaceConversationGet(conversationId: string) {
  return invoke<AssistantConversation>('workspace_conversation_get', { conversationId });
}

export async function workspaceConversationCreate(input?: {
  title?: string;
  assistant_id?: string;
  model_override_id?: string;
}) {
  return invoke<AssistantConversation>('workspace_conversation_create', {
    input: input || null,
  });
}

export async function workspaceConversationUpdate(input: {
  conversation_id: string;
  title?: string;
  pinned?: boolean;
  archived?: boolean;
  assistant_id?: string;
  model_override_id?: string;
  web_search_enabled?: boolean;
}) {
  return invoke<AssistantConversation>('workspace_conversation_update', { input });
}

export async function workspaceConversationDelete(conversationId: string) {
  return invoke<boolean>('workspace_conversation_delete', { conversationId });
}

export async function workspaceConversationResetContext(conversationId: string) {
  return invoke<AssistantConversation>('workspace_conversation_reset_context', { conversationId });
}

export async function workspaceScheduleResolveDraft(input: {
  conversation_id: string;
  message_id: string;
  approved: boolean;
}) {
  return invoke<AssistantConversation>('workspace_schedule_resolve_draft', { input });
}

export async function workspaceConversationSend(input: {
  conversation_id: string;
  content: string;
  assistant_id?: string;
  model_override_id?: string;
  web_search_enabled?: boolean;
}) {
  return invoke<AssistantSendResult>('workspace_conversation_send', { input });
}

export async function workspaceAutomationsList() {
  return invoke<AutomationJobView[]>('workspace_automations_list');
}

export async function workspaceAutomationUpsert(schedule: AutomationJob) {
  return invoke<AutomationJob>('workspace_automation_upsert', { schedule });
}

export async function workspaceAutomationDelete(scheduleId: string) {
  return invoke<boolean>('workspace_automation_delete', { scheduleId });
}

export async function workspaceAutomationToggle(input: { schedule_id: string; enabled: boolean }) {
  return invoke<AutomationJob>('workspace_automation_toggle', { input });
}

export async function workspaceAutomationRunNow(input: { schedule_id: string }) {
  return invoke<boolean>('workspace_automation_run_now', { input });
}

export async function workspaceQuickAssistantGet() {
  return invoke<QuickAssistantPreferences>('workspace_quick_assistant_get');
}

export async function workspaceQuickAssistantSave(preferences: QuickAssistantPreferences) {
  return invoke<QuickAssistantPreferences>('workspace_quick_assistant_save', { preferences });
}

export async function workspaceSelectionAssistantGet() {
  return invoke<SelectionAssistantPreferences>('workspace_selection_assistant_get');
}

export async function workspaceSelectionAssistantSave(preferences: SelectionAssistantPreferences) {
  return invoke<SelectionAssistantPreferences>('workspace_selection_assistant_save', { preferences });
}

export async function showQuickAssistantWindow() {
  return invoke('show_quick_assistant_window');
}

export async function hideQuickAssistantWindow() {
  return invoke('hide_quick_assistant_window');
}

export async function showSelectionAssistantWindow() {
  return invoke('show_selection_assistant_window');
}

export async function hideSelectionAssistantWindow() {
  return invoke('hide_selection_assistant_window');
}
