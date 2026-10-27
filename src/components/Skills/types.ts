import type { SkillModelId } from '../skillsModelOptions';

export type ModelType = SkillModelId;
export type InstallScope = 'global' | 'project';

export type ApiResp<T> = { ok: boolean; data: T; meta: { revision: number; ts: number } };

export interface SkillRecord {
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

export interface CatalogSkill {
  source_id: string;
  id: string;
  rel_path: string;
  dir_name?: string;
  name: string;
  description: string;
  models: ModelType[];
  first_seen_at?: number;
}

export interface StorageConfigLite {
  skills_new_badge_hours?: number;
  skills_sources?: Array<{ id?: string; name?: string }>;
}

export interface SkillDetail {
  skill: SkillRecord;
  markdown: string;
  local_path: string;
}

export interface CatalogSkillDetail {
  skill: CatalogSkill;
  markdown: string;
  source_path: string;
}

export interface CatalogOpenFolderResult {
  repo_key: string;
  opened_path: string;
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

export interface InstalledSkillTarget {
  model: ModelType;
  scope: InstallScope;
  project_root?: string | null;
  dir_name: string;
}

export interface ReloadPreview {
  before_label: string;
  after_label: string;
  changed_files: ReloadChangedFile[];
  text_diffs: ReloadTextDiff[];
  installed_models: ModelType[];
  installed_targets: InstalledSkillTarget[];
  has_changes: boolean;
}

export interface ReloadApplyResult {
  index_refreshed: boolean;
  synced_models: ModelType[];
  synced_targets: InstalledSkillTarget[];
  updated_files_count: number;
  applied_at: number;
}

export interface SourceSyncState {
  source_id: string;
  last_synced_at?: number;
  last_status: string;
  last_error?: string;
}

export interface SkillsSyncState {
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

export interface RepositorySkillView {
  repo_key: string;
  skill_id: string;
  dir_name?: string;
  source_id: string;
  source_rel_path: string;
  source_type: string;
  source_path?: string;
  name: string;
  description: string;
  models: ModelType[];
  icon_seed: string;
  hash?: string;
  created_at?: number;
  updated_at?: number;
  has_update: boolean;
  installed: RepoModelInstallState;
}

export interface RepoImportFolderResult {
  repo_key: string;
  skill_id: string;
  source_id: string;
  source_rel_path: string;
}

export interface InstallTargetSkill extends CatalogSkill {
  repo_key?: string;
  installed?: RepoModelInstallState;
}
