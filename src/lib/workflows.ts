import { invoke } from '@tauri-apps/api/core';

export interface ApiResp<T> {
  ok: boolean;
  data: T;
  meta: { schema_version: number; revision: number };
}

export type WorkflowTool = 'claude' | 'codex' | 'gemini' | 'opencode';
export type WorkflowLaunchScope = 'shared' | 'strict';

export interface WorkflowPreset {
  id: string;
  name: string;
  tool: WorkflowTool;
  working_dir: string;
  provider_id?: string | null;
  mcp_server_ids: string[];
  required_skill_ids: string[];
  launch_prompt?: string | null;
  launch_scope: WorkflowLaunchScope;
  created_at: number;
  updated_at: number;
}

export interface WorkflowPresetInput {
  id?: string;
  name: string;
  tool?: WorkflowTool;
  working_dir?: string;
  provider_id?: string;
  mcp_server_ids?: string[];
  required_skill_ids?: string[];
  launch_prompt?: string;
  launch_scope?: WorkflowLaunchScope;
}

export interface WorkflowRun {
  id: string;
  preset_id: string;
  preset_name: string;
  tool: WorkflowTool;
  working_dir: string;
  launch_prompt?: string | null;
  launch_scope: WorkflowLaunchScope;
  session_id?: string | null;
  tool_session_id?: string | null;
  runtime_mode: WorkflowLaunchScope;
  runtime_profile_id?: string | null;
  prompt_apply_status: 'applied' | 'manual' | 'unsupported';
  dependency_apply_mode: 'shared-global' | 'strict-local' | 'global-compat';
  status: 'running' | 'success' | 'failed' | 'interrupted';
  summary?: string | null;
  error_message?: string | null;
  started_at: number;
  ended_at?: number | null;
  replay_of_run_id?: string | null;
}

export interface WorkflowDependencyState {
  active_provider_id?: string | null;
  active_provider_name?: string | null;
  missing_mcp_server_ids: string[];
  missing_mcp_names: string[];
  inactive_mcp_server_ids: string[];
  inactive_mcp_names: string[];
  missing_skill_ids: string[];
  missing_skill_names: string[];
  installable_skill_ids: string[];
  unresolved_skill_ids: string[];
}

export interface WorkflowDependencyApplyResult {
  preset_id: string;
  linked_mcp_count: number;
  enabled_mcp_switch_count: number;
  installed_skill_count: number;
  failed_skill_installs: string[];
  dependencies_after: WorkflowDependencyState;
}

export async function workflowsListPresets() {
  return invoke<ApiResp<WorkflowPreset[]>>('workflows_presets_list');
}

export async function workflowsUpsertPreset(input: WorkflowPresetInput) {
  return invoke<ApiResp<WorkflowPreset>>('workflows_preset_upsert', { input });
}

export async function workflowsDeletePreset(presetId: string) {
  return invoke<ApiResp<{ deleted: boolean }>>('workflows_preset_delete', { presetId });
}

export async function workflowsCheckDependencies(presetId: string) {
  return invoke<ApiResp<WorkflowDependencyState>>('workflows_check_dependencies', { presetId });
}

export async function workflowsApplyDependencies(presetId: string) {
  return invoke<ApiResp<WorkflowDependencyApplyResult>>('workflows_apply_dependencies', { presetId });
}

export async function workflowsLaunchPreset(input: {
  preset_id: string;
  session_name?: string;
  override_working_dir?: string;
}) {
  return invoke<ApiResp<{ preset: WorkflowPreset; session: unknown; run: WorkflowRun }>>('workflows_launch_preset', { input });
}

export async function workflowsReplayRun(input: { run_id: string; session_name?: string }) {
  return invoke<ApiResp<{ replay_of: WorkflowRun; session: unknown; run: WorkflowRun }>>('workflows_replay_run', { input });
}

export async function workflowsListRuns(input?: { preset_id?: string; limit?: number }) {
  return invoke<ApiResp<WorkflowRun[]>>('workflows_runs_list', { input });
}

export async function workflowsUpdateRun(input: {
  run_id: string;
  status: WorkflowRun['status'];
  summary?: string;
  error_message?: string;
}) {
  return invoke<ApiResp<WorkflowRun>>('workflows_run_update', { input });
}
