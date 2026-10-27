import type { SubagentModelId } from '../subagentsModelOptions';

export type ModelType = SubagentModelId;
export type InstallScope = 'global' | 'project';

export type ApiResp<T> = { ok: boolean; data: T; meta: { revision: number; ts: number } };

export interface SubagentRecord {
  id: string;
  dir_name?: string;
  model: ModelType;
  models: ModelType[];
  name: string;
  description: string;
  source_id: string;
  source_rel_path: string;
  installed_at: number;
  updated_at?: number;
  has_update: boolean;
  icon_seed: string;
  scope?: InstallScope;
  project_root?: string | null;
  target_path?: string | null;
}

export interface CatalogSubagent {
  source_id: string;
  id: string;
  rel_path: string;
  dir_name?: string;
  name: string;
  description: string;
  models: ModelType[];
  model?: string;
  tools?: string[];
  first_seen_at?: number;
}

export interface StorageConfigLite {
  subagents_new_badge_hours?: number;
  subagents_sources?: Array<{ id?: string; name?: string }>;
}

export interface SubagentDetail {
  subagent: SubagentRecord;
  markdown: string;
  local_path: string;
}

export interface CatalogSubagentDetail {
  subagent: CatalogSubagent;
  markdown: string;
  source_path: string;
}

export interface CatalogOpenFolderResult {
  repo_key: string;
  opened_path: string;
}

export interface UpdateDiff {
  local_markdown: string;
  remote_markdown: string;
  local_changed_lines: number[];
  remote_changed_lines: number[];
  local_changed_blocks: { start_line: number; end_line: number; content: string }[];
  remote_changed_blocks: { start_line: number; end_line: number; content: string }[];
}

export interface ReloadChangedFile {
  path: string;
  status: 'added' | 'modified' | 'deleted' | string;
  is_binary: boolean;
}

export interface ReloadTextDiff {
  path: string;
  before_content: string;
  after_content: string;
  before_changed_lines: number[];
  after_changed_lines: number[];
}

export interface ReloadPreview {
  before_label: string;
  after_label: string;
  changed_files: ReloadChangedFile[];
  text_diffs: ReloadTextDiff[];
  installed_models: ModelType[];
  has_changes: boolean;
}

export interface ReloadApplyResult {
  index_refreshed: boolean;
  synced_models: ModelType[];
  updated_files_count: number;
  applied_at: number;
}

export interface SourceSyncState {
  source_id: string;
  last_synced_at?: number;
  last_status: string;
  last_error?: string;
}

export interface SubagentsSyncState {
  status: string;
  last_error?: string;
  last_sync_at?: number;
  sources: SourceSyncState[];
}

export interface RepoModelInstallState {
  claude: boolean;
  antigravity: boolean;
  codex: boolean;
  opencode: boolean;
}

export interface RepositorySubagentView {
  repo_key: string;
  subagent_id: string;
  dir_name?: string;
  source_id: string;
  source_rel_path: string;
  source_type: string;
  source_path?: string;
  name: string;
  description: string;
  models: ModelType[];
  model?: string;
  tools?: string[];
  icon_seed: string;
  hash?: string;
  created_at?: number;
  updated_at?: number;
  has_update: boolean;
  installed: RepoModelInstallState;
}

export interface RepoImportFolderResult {
  repo_key: string;
  subagent_id: string;
  source_id: string;
  source_rel_path: string;
}

export interface InstallTargetSubagent extends CatalogSubagent {
  repo_key?: string;
  installed?: RepoModelInstallState;
}
