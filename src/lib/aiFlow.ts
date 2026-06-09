import { invoke } from '@tauri-apps/api/core';

export interface ApiResp<T> {
  ok: boolean;
  data: T;
  meta: { schema_version: number; revision: number };
}

export interface AiFlowInstallStatus {
  repo_url: string;
  cache_dir: string;
  installed: boolean;
  commit?: string | null;
  branch?: string | null;
  log: string;
}

export interface AiFlowHealthItem {
  id: string;
  label: string;
  ok: boolean;
  status: string;
  path?: string | null;
  detail?: string | null;
}

export interface AiFlowHealthCheck {
  installed: boolean;
  repo_commit?: string | null;
  repo_branch?: string | null;
  items: AiFlowHealthItem[];
}

export interface AiFlowProjectSummary {
  id: string;
  name: string;
  root_path: string;
  ai_flow_dir: string;
  from_workspace: boolean;
  has_ai_flow: boolean;
  plan_count: number;
  pending_count: number;
  failed_count: number;
  done_count: number;
  invalid_state_count: number;
  queue_count: number;
  group_count: number;
  html_status_path?: string | null;
  updated_at?: string | null;
}

export interface AiFlowPlanTransition {
  seq?: number | null;
  at?: string | null;
  event?: string | null;
  from?: string | null;
  to?: string | null;
  actor?: string | null;
  note?: string | null;
}

export interface AiFlowPlanState {
  slug: string;
  title: string;
  current_status: string;
  plan_file?: string | null;
  plan_path?: string | null;
  review_files: string[];
  created_at?: string | null;
  updated_at?: string | null;
  transitions: AiFlowPlanTransition[];
  raw_state_path: string;
}

export interface AiFlowPlanGroupState {
  slug: string;
  title?: string | null;
  current_status?: string | null;
  current_child?: string | null;
  children: unknown[];
  dependencies: unknown[];
  raw_state_path: string;
}

export interface AiFlowQueueState {
  slug: string;
  title?: string | null;
  current_status?: string | null;
  items: unknown[];
  raw_state_path: string;
}

export interface AiFlowInvalidState {
  path: string;
  error: string;
}

export interface AiFlowConfigSummary {
  global_setting_exists: boolean;
  project_setting_exists: boolean;
  project_rule_exists: boolean;
  effective_setting?: unknown;
}

export interface AiFlowProjectStatus {
  project: AiFlowProjectSummary;
  plans: AiFlowPlanState[];
  groups: AiFlowPlanGroupState[];
  queues: AiFlowQueueState[];
  invalid_states: AiFlowInvalidState[];
  config_summary: AiFlowConfigSummary;
  html_status_path?: string | null;
}

export interface AiFlowConfigDocument {
  scope: string;
  format: 'json' | 'yaml';
  path: string;
  exists: boolean;
  content: string;
  parsed?: unknown;
}

export interface AiFlowConfigSaveResult {
  path: string;
  backup_path?: string | null;
}

export interface AiFlowLaunchAction {
  tool: 'claude' | 'codex' | 'gemini' | 'opencode';
  action:
    | 'plan-review'
    | 'coding'
    | 'review'
    | 'resume'
    | 'status'
    | 'reopen-current'
    | 'group-review'
    | 'group-final-review';
  slug: string;
  project_root: string;
  session_id?: string | null;
  permission_mode?: 'default' | 'full_access' | null;
}

export interface AiFlowLaunchPreview {
  tool: string;
  permission_confirmation_required: boolean;
  prompt: string;
}

export interface AiFlowQueueCreateResult {
  queue_slug: string;
  state_path: string;
  log: string;
}

export function aiFlowInstallLatest() {
  return invoke<ApiResp<AiFlowInstallStatus>>('ai_flow_install_latest');
}

export function aiFlowHealthCheck() {
  return invoke<ApiResp<AiFlowHealthCheck>>('ai_flow_health_check');
}

export function aiFlowProjectsList(extraPath?: string) {
  return invoke<ApiResp<AiFlowProjectSummary[]>>('ai_flow_projects_list', {
    extraPath: extraPath || null,
  });
}

export function aiFlowProjectStatus(projectRoot: string) {
  return invoke<ApiResp<AiFlowProjectStatus>>('ai_flow_project_status', {
    projectRoot,
  });
}

export function aiFlowConfigGet(scope: string, projectRoot?: string) {
  return invoke<ApiResp<AiFlowConfigDocument>>('ai_flow_config_get', {
    scope,
    projectRoot: projectRoot || null,
  });
}

export function aiFlowConfigSave(input: {
  scope: string;
  project_root?: string | null;
  format: 'json' | 'yaml';
  content: string;
}) {
  return invoke<ApiResp<AiFlowConfigSaveResult>>('ai_flow_config_save', { input });
}

export function aiFlowLaunchAction(input: AiFlowLaunchAction) {
  return invoke<ApiResp<unknown>>('ai_flow_launch_action', { input });
}

export function aiFlowLaunchPreview(input: AiFlowLaunchAction) {
  return invoke<ApiResp<AiFlowLaunchPreview>>('ai_flow_launch_preview', { input });
}

export function aiFlowQueueCreate(input: {
  project_root: string;
  queue_slug: string;
  plan_slugs: string[];
}) {
  return invoke<ApiResp<AiFlowQueueCreateResult>>('ai_flow_queue_create', { input });
}

export function aiFlowOpenPath(path: string) {
  return invoke<ApiResp<{ opened: boolean }>>('ai_flow_open_path', { path });
}

export function aiFlowFormatError(err: unknown): string {
  if (typeof err === 'string') return err;
  if (err && typeof err === 'object') {
    const maybe = err as { code?: unknown; message?: unknown; error?: unknown };
    const code = typeof maybe.code === 'string' ? maybe.code : null;
    const message =
      typeof maybe.message === 'string'
        ? maybe.message
        : typeof maybe.error === 'string'
          ? maybe.error
          : null;
    if (code && message) return `[${code}] ${message}`;
    if (message) return message;
    if (code) return `[${code}]`;
  }
  return String(err);
}
