import { invoke } from "@tauri-apps/api/core";

export interface RemoteBotConfig {
  enabled: boolean;
  bot_token: string;
  allowed_chat_ids: number[];
  active_workspace?: string | null;
  auto_approve_permissions: boolean;
  enable_natural_dialogue: boolean;
  opencode_bin_path?: string | null;
  workspace_directories: string[];
  updated_at: number;
}

export interface RemoteBotStatus {
  running: boolean;
  bot_username?: string | null;
  bot_name?: string | null;
  last_poll_time?: number | null;
  last_error?: string | null;
  active_task_id?: string | null;
  pending_question_id?: string | null;
}

export interface TelegramUser {
  id: number;
  is_bot: boolean;
  first_name: string;
  username?: string | null;
}

export async function remoteBotGetConfig(): Promise<RemoteBotConfig> {
  return await invoke<RemoteBotConfig>("remote_bot_get_config");
}

export async function remoteBotSaveConfig(config: RemoteBotConfig): Promise<void> {
  await invoke("remote_bot_save_config", { config });
}

export async function remoteBotTestConnection(botToken: string): Promise<TelegramUser> {
  return await invoke<TelegramUser>("remote_bot_test_connection", { botToken });
}

export async function remoteBotGetStatus(): Promise<RemoteBotStatus> {
  return await invoke<RemoteBotStatus>("remote_bot_get_status");
}

export async function remoteBotRestart(): Promise<RemoteBotStatus> {
  return await invoke<RemoteBotStatus>("remote_bot_restart");
}

export async function remoteBotProbeOpencode(binPath?: string): Promise<string> {
  return await invoke<string>("remote_bot_probe_opencode", { binPath: binPath || null });
}
