import { invoke } from '@tauri-apps/api/core';
import type { ApiResp, CliTool } from './serviceProviders';
import type { TerminalPermissionMode } from './terminalPermissions';

export interface AiSession {
  id: string;
  name: string;
  working_dir: string;
  model_type: string;
  model_name?: string | null;
  tool_session_id: string;
  status?: string;
  created_at: number;
  last_used_at?: number;
  favorited_at?: number | null;
  provider_id?: string | null;
}

export type AiSessionListItem = AiSession;

export interface SessionInput {
  id?: string;
  name: string;
  working_dir: string;
  tool: string;
  tool_session_id?: string;
  runtime_mode?: string;
  runtime_profile_id?: string;
  preset_id?: string;
  status?: string;
  provider_id?: string;
  initial_prompt?: string;
  permission_mode?: TerminalPermissionMode;
}

export type AiModelLaunchCommands = Record<CliTool, string>;

export interface AiSessionStorageConfig {
  default_ai_dir?: string;
  default_ai_model?: CliTool;
  ai_model_launch_commands?: Partial<AiModelLaunchCommands>;
  ai_model_permission_modes?: Record<string, string>;
}

export function getAiSessionStorageConfig(): Promise<AiSessionStorageConfig> {
  return invoke<AiSessionStorageConfig>('get_storage_config');
}

export function sessionsList(): Promise<ApiResp<AiSession[]>> {
  return invoke<ApiResp<AiSession[]>>('sessions_list');
}

export function sessionsCreate(session: SessionInput): Promise<ApiResp<AiSession>> {
  return invoke<ApiResp<AiSession>>('sessions_create', { session });
}

export function sessionsUpdate(session: SessionInput): Promise<ApiResp<AiSession>> {
  return invoke<ApiResp<AiSession>>('sessions_update', { session });
}

export function sessionsLaunch(
  sessionId: string,
  permissionMode?: TerminalPermissionMode,
): Promise<ApiResp<AiSession>> {
  return invoke<ApiResp<AiSession>>('sessions_launch', {
    sessionId,
    ...(permissionMode !== undefined ? { permissionMode } : {}),
  });
}

export function sessionsDelete(sessionId: string): Promise<ApiResp<{ deleted: boolean }>> {
  return invoke<ApiResp<{ deleted: boolean }>>('sessions_delete', { sessionId });
}

export function sessionsSetFavorite(sessionId: string, favorite: boolean): Promise<ApiResp<AiSession>> {
  return invoke<ApiResp<AiSession>>('sessions_set_favorite', { sessionId, favorite });
}

export function checkCliInstalled(): Promise<boolean> {
  return invoke<boolean>('check_cli_installed');
}

export function installCli(): Promise<void> {
  return invoke<void>('install_cli');
}

export function hideQuickAiWindow(): Promise<void> {
  return invoke<void>('hide_quick_ai_window');
}

export function resizeQuickAiWindow(height: number): Promise<void> {
  return invoke<void>('resize_window', { height });
}
