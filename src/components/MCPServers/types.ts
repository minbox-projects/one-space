import type { WorkspaceCapabilityContext } from '../workspaceCapabilityContext';
import type { SkillModelId } from '../skillsModelOptions';

export type MCPModel = SkillModelId;

export interface MCPModelSwitchState {
  claude: boolean;
  codex: boolean;
  antigravity: boolean;
  opencode: boolean;
}

export interface MCPServer {
  id: string;
  name: string;
  config_key?: string;
  description?: string;
  transport: 'stdio' | 'http' | 'sse';
  command?: string;
  args?: string[];
  cwd?: string;
  url?: string;
  http_url?: string;
  env?: Record<string, string>;
  headers?: Record<string, string>;
  timeout?: number;
  trust?: boolean;
  linked_provider_ids: string[];
}

export interface MCPTemplate {
  id: string;
  name: string;
  description: string;
  transport: string;
  command?: string;
  args?: string[];
  url?: string;
  env_placeholders: string[];
  headers_placeholders: string[];
}

export type MCPUpdateStatus = 'up_to_date' | 'updatable' | 'floating_latest' | 'unsupported' | 'check_failed';

export interface MCPUpdateInfo {
  server_id: string;
  package_name?: string | null;
  current_version?: string | null;
  latest_version?: string | null;
  status: MCPUpdateStatus;
  message?: string | null;
  checked_at: number;
}

export interface MCPUpdatesState {
  status: string;
  last_error?: string | null;
  last_checked_at?: number | null;
  items: MCPUpdateInfo[];
}

export interface ApiResp<T> {
  ok: boolean;
  data: T;
  meta?: {
    revision: number;
    ts: number;
  };
}

export interface MCPServersProps {
  providers?: any[];
  onLinkedProvidersChange?: (serverId: string, providerIds: string[]) => void;
  isVisible?: boolean;
  workspaceContext?: WorkspaceCapabilityContext;
  onDismissWorkspaceContext?: () => void;
}
