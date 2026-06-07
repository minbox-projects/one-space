import type { AiSessionListItem, AiSessionsQueryState } from "../AiSessionsList";
import type { SkillModelId } from "../skillsModelOptions";
import type { SubagentModelId } from "../subagentsModelOptions";

export type ApiResp<T> = {
  ok: boolean;
  data: T;
  meta: { schema_version?: number; revision: number; ts?: number };
};

export type ModelId = "claude" | "gemini" | "codex" | "opencode";
export type WorkspaceTab = "sessions" | "mcp" | "skills" | "subagents";
export type DialogMode = "create" | "edit";

export type WorkspaceRecord = {
  id: string;
  name: string;
  root_path: string;
  description?: string | null;
  tags: string[];
  source: string;
  created_at: number;
  updated_at: number;
  last_activity_at: number;
};

export type WorkspaceView = {
  workspace: WorkspaceRecord;
  session_count: number;
};

export type WorkspaceMcpBinding = {
  workspace_id: string;
  server_id: string;
  enabled_models: string[];
  created_at: number;
  updated_at: number;
};

export type WorkspaceDetail = {
  workspace: WorkspaceView;
  mcp_bindings: WorkspaceMcpBinding[];
};

export type MCPServer = {
  id: string;
  name: string;
  config_key?: string;
  description?: string;
  transport?: "stdio" | "http" | "sse";
  command?: string;
  args?: string[];
  url?: string;
  http_url?: string;
};

export type MCPStateResp = {
  servers?: MCPServer[];
};

export type MCPModelSwitchState = Record<ModelId, boolean>;

export type WorkspaceFormState = {
  id?: string;
  name: string;
  root_path: string;
  description: string;
  tags: string;
};

export type WorkspaceMcpScope = "global" | "project";

export type WorkspaceMcpEntry = {
  server: MCPServer;
  binding: WorkspaceMcpBinding | null;
  scope: WorkspaceMcpScope;
  enabled_models: ModelId[];
};

export type WorkspaceMcpCatalogEntry = WorkspaceMcpEntry & {
  status: "enabled_for_model" | "enabled_user_level" | "bound_other_models" | "not_bound";
};

export type WorkspaceSessionsListData = {
  items: AiSessionListItem[];
  total: number;
  tool_options: string[];
  model_options: string[];
};

export type WorkspaceMessage = {
  type: "success" | "error";
  text: string;
};

export type InstalledSkill = {
  id: string;
  model: ModelId;
  name: string;
  description?: string;
  source_id: string;
  source_rel_path: string;
  scope?: "global" | "project";
  project_root?: string | null;
};

export type InstalledSubagent = {
  id: string;
  model: ModelId;
  name: string;
  description?: string;
  source_id: string;
  source_rel_path: string;
  scope?: "global" | "project";
  project_root?: string | null;
};

export type CopyableSkill = InstalledSkill & { selection_key: string };
export type CopyableSubagent = InstalledSubagent & { selection_key: string };

export type WorkspaceCollectionState = {
  loading: boolean;
  workspacesInitialized: boolean;
  workspaces: WorkspaceView[];
  allTags: string[];
  selectedTags: string[];
  visibleWorkspaces: WorkspaceView[];
  selectedWorkspaceTags: Set<string>;
};

export const DEFAULT_WORKSPACE_SESSIONS_QUERY: AiSessionsQueryState = {
  toolFilter: "all",
  modelFilter: "all",
  nameFilter: "",
};

export const TOOL_OPTIONS: Array<{ id: ModelId; label: string }> = [
  { id: "claude", label: "Claude Code" },
  { id: "gemini", label: "Gemini" },
  { id: "codex", label: "Codex" },
  { id: "opencode", label: "OpenCode" },
];

export const DEFAULT_MCP_MODEL_SWITCH_STATE: MCPModelSwitchState = {
  claude: false,
  gemini: false,
  codex: false,
  opencode: false,
};

export const TAB_LOADING_MIN_MS = 200;

export type CapabilityRepoModelInstallState = Record<ModelId, boolean>;

export type WorkspaceCapabilityPanelMessage = {
  type: "success" | "error";
  text: string;
} | null;

export type WorkspaceDiscoveryMode = "recommended" | "repository";

export type WorkspaceInstalledCapabilityBase<TModel extends string> = {
  id: string;
  dir_name?: string;
  model: TModel;
  models: TModel[];
  name: string;
  description: string;
  source_id: string;
  source_rel_path: string;
  installed_at: number;
  updated_at?: number;
  has_update: boolean;
  icon_seed: string;
  scope?: "global" | "project";
  project_root?: string | null;
};

export type WorkspaceCatalogCapabilityBase<TModel extends string> = {
  source_id: string;
  id: string;
  rel_path: string;
  dir_name?: string;
  name: string;
  description: string;
  models: TModel[];
  first_seen_at?: number;
};

export type WorkspaceRepositoryCapabilityBase<TModel extends string> = {
  repo_key: string;
  dir_name?: string;
  source_id: string;
  source_rel_path: string;
  source_type: string;
  name: string;
  description: string;
  models: TModel[];
  icon_seed: string;
  created_at?: number;
  updated_at?: number;
  has_update: boolean;
  installed: CapabilityRepoModelInstallState;
};

export type WorkspaceInstallTargetBase<TModel extends string> = {
  source_id: string;
  id: string;
  rel_path: string;
  dir_name?: string;
  name: string;
  description: string;
  models: TModel[];
  repo_key?: string;
  installed?: CapabilityRepoModelInstallState;
};

export type WorkspaceStorageConfigLite = {
  skills_sources?: Array<{ id?: string; name?: string }>;
  subagents_sources?: Array<{ id?: string; name?: string }>;
};

export type WorkspaceSkillModel = SkillModelId;
export type WorkspaceSubagentModel = SubagentModelId;
