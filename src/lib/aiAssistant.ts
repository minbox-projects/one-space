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

export interface AiAssistantProvider {
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

export interface AiAssistantModelProfile {
  id: string;
  name: string;
  provider_id: string;
  model_id: string;
  usage: string;
  temperature?: number | null;
  max_tokens?: number | null;
  enable_reasoning: boolean;
}

export interface WebSearchProvider {
  id: string;
  name: string;
  provider_type: string;
  base_url?: string | null;
  api_key: string;
  enabled: boolean;
  timeout_secs?: number | null;
  max_results?: number | null;
}

export interface AiAssistantSettings {
  providers: AiAssistantProvider[];
  profiles: AiAssistantModelProfile[];
  search_providers: WebSearchProvider[];
  default_chat_profile_id?: string | null;
  default_agent_profile_id?: string | null;
  default_summary_profile_id?: string | null;
  active_search_provider_id?: string | null;
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
  schedule?: ScheduleJob | null;
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
  model_profile_id?: string | null;
  web_search_enabled: boolean;
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
  model_profile_id?: string | null;
  web_search_enabled: boolean;
  context_reset_count: number;
}

export interface AgentToolPolicy {
  web_search: boolean;
  workspace_read: boolean;
  notes_search: boolean;
}

export interface AgentDefinition {
  id: string;
  name: string;
  description: string;
  system_prompt: string;
  default_model_profile_id?: string | null;
  light_model_profile_id?: string | null;
  tool_policy: AgentToolPolicy;
  output_contract: string;
  created_at: number;
  updated_at: number;
}

export interface ScheduleTrigger {
  kind: string;
  interval_minutes?: number | null;
  time_of_day?: string | null;
  weekdays: number[];
}

export interface ScheduleJob {
  id: string;
  name: string;
  agent_id: string;
  prompt: string;
  model_profile_id?: string | null;
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

export interface ScheduleRun {
  id: string;
  schedule_id: string;
  started_at: number;
  ended_at?: number | null;
  status: string;
  summary?: string | null;
  error_message?: string | null;
  conversation_id?: string | null;
}

export interface ScheduleJobView {
  recent_runs: ScheduleRun[];
  id: string;
  name: string;
  agent_id: string;
  prompt: string;
  model_profile_id?: string | null;
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

export async function assistantSettingsGet() {
  return invoke<AiAssistantSettings>('assistant_settings_get');
}

export async function assistantSettingsSave(settings: AiAssistantSettings) {
  return invoke<AiAssistantSettings>('assistant_settings_save', { settings });
}

export async function assistantConversationsList() {
  return invoke<AssistantConversationListItem[]>('assistant_conversations_list');
}

export async function assistantConversationGet(conversationId: string) {
  return invoke<AssistantConversation>('assistant_conversation_get', { conversationId });
}

export async function assistantConversationCreate(title?: string) {
  return invoke<AssistantConversation>('assistant_conversation_create', {
    input: title ? { title } : null,
  });
}

export async function assistantConversationUpdate(input: {
  conversation_id: string;
  title?: string;
  pinned?: boolean;
  archived?: boolean;
  model_profile_id?: string;
  web_search_enabled?: boolean;
}) {
  return invoke<AssistantConversation>('assistant_conversation_update', { input });
}

export async function assistantConversationDelete(conversationId: string) {
  return invoke<boolean>('assistant_conversation_delete', { conversationId });
}

export async function assistantConversationResetContext(conversationId: string) {
  return invoke<AssistantConversation>('assistant_conversation_reset_context', { conversationId });
}

export async function assistantScheduleResolveDraft(input: {
  conversation_id: string;
  message_id: string;
  approved: boolean;
}) {
  return invoke<AssistantConversation>('assistant_schedule_resolve_draft', { input });
}

export async function assistantMessageSend(input: {
  conversation_id: string;
  content: string;
  model_profile_id?: string;
  agent_id?: string;
  web_search_enabled?: boolean;
}) {
  return invoke<AssistantSendResult>('assistant_message_send', { input });
}

export async function assistantAgentsList() {
  return invoke<AgentDefinition[]>('assistant_agents_list');
}

export async function assistantAgentUpsert(agent: AgentDefinition) {
  return invoke<AgentDefinition>('assistant_agent_upsert', { agent });
}

export async function assistantAgentDelete(agentId: string) {
  return invoke<boolean>('assistant_agent_delete', { agentId });
}

export async function assistantAgentTestRun(input: { agent_id: string; prompt: string }) {
  return invoke<{ conversation_id: string }>('assistant_agent_test_run', { input });
}

export async function assistantModelTest(input: { profile_id: string }) {
  return invoke<AssistantConnectionTestResult>('assistant_model_test', { input });
}

export async function assistantSearchProviderTest(input: { provider_id: string }) {
  return invoke<AssistantConnectionTestResult>('assistant_search_provider_test', { input });
}

export async function assistantSchedulesList() {
  return invoke<ScheduleJobView[]>('assistant_schedules_list');
}

export async function assistantScheduleUpsert(schedule: ScheduleJob) {
  return invoke<ScheduleJob>('assistant_schedule_upsert', { schedule });
}

export async function assistantScheduleDelete(scheduleId: string) {
  return invoke<boolean>('assistant_schedule_delete', { scheduleId });
}

export async function assistantScheduleToggle(input: { schedule_id: string; enabled: boolean }) {
  return invoke<ScheduleJob>('assistant_schedule_toggle', { input });
}

export async function assistantScheduleRunNow(input: { schedule_id: string }) {
  return invoke<boolean>('assistant_schedule_run_now', { input });
}
