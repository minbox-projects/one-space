import { invokeToolboxCommand } from "@/toolbox/invoke";

export const SUPPORTED_ROLES = [
  "backend",
  "documentation-maintainer",
  "file-explorer",
  "frontend",
  "git-operator",
  "researcher",
  "spec-review",
  "standards-review",
  "test",
] as const;

export type SupportedRole = (typeof SUPPORTED_ROLES)[number];

export const SUPPORTED_TOOLS = ["codex", "claude", "opencode"] as const;
export type SupportedTool = (typeof SUPPORTED_TOOLS)[number];

export const AI_WORKFLOW_PROFILE_UPDATED_EVENT = "ai-workflow-profile-updated";

export const VALID_EFFORTS = [
  "low",
  "medium",
  "high",
  "xhigh",
  "max",
  "ultra",
] as const;
export type ValidEffort = (typeof VALID_EFFORTS)[number];

export interface ProfileSummary {
  name: string;
  active: boolean;
  error?: string;
}

export interface ModelEffort {
  model: string;
  reasoning_effort: string;
}

export interface AgentMatrixRow {
  role: string;
  codex?: ModelEffort;
  claude?: ModelEffort;
  opencode?: ModelEffort;
}

export interface ProfileMatrix {
  name: string;
  rows: AgentMatrixRow[];
}

export interface ColumnModelSource {
  models: string[];
  error?: string;
}

export interface ModelSourcesResult {
  opencode: ColumnModelSource;
  codex: ColumnModelSource;
  claude: ColumnModelSource;
}

export interface AgentInstallationInfo {
  name: string;
  path: string;
  model?: string;
  reasoning_effort?: string;
}

export interface HostInstallation {
  host: string;
  agents_directory: string;
  agents: AgentInstallationInfo[];
}

export interface ProfileActivationReport {
  active_profile: string;
  hosts: string[];
  installations: HostInstallation[];
  message?: string;
}

export async function listProfiles(): Promise<ProfileSummary[]> {
  return invokeToolboxCommand<ProfileSummary[]>("ai_workflow_list_profiles");
}

export async function getProfileMatrix(name: string): Promise<ProfileMatrix> {
  return invokeToolboxCommand<ProfileMatrix>("ai_workflow_get_profile_matrix", {
    name,
  });
}

export async function getActiveModels(): Promise<ProfileMatrix | null> {
  return invokeToolboxCommand<ProfileMatrix | null>("ai_workflow_get_active_models");
}

export async function getModelSources(): Promise<ModelSourcesResult> {
  return invokeToolboxCommand<ModelSourcesResult>("ai_workflow_get_model_sources");
}

export async function saveProfile(
  name: string,
  matrix: AgentMatrixRow[],
): Promise<void> {
  return invokeToolboxCommand<void>("ai_workflow_save_profile", {
    name,
    matrix,
  });
}

export async function activateProfile(
  name: string,
): Promise<ProfileActivationReport> {
  const report = await invokeToolboxCommand<ProfileActivationReport>("ai_workflow_activate_profile", {
    name,
  });
  window.dispatchEvent(new Event(AI_WORKFLOW_PROFILE_UPDATED_EVENT));
  return report;
}

export async function createProfile(
  name: string,
  copyFrom?: string,
): Promise<void> {
  return invokeToolboxCommand<void>("ai_workflow_create_profile", {
    name,
    copyFrom,
  });
}

export async function deleteProfile(name: string): Promise<void> {
  return invokeToolboxCommand<void>("ai_workflow_delete_profile", {
    name,
  });
}

export async function renameProfile(
  oldName: string,
  newName: string,
): Promise<void> {
  return invokeToolboxCommand<void>("ai_workflow_rename_profile", {
    oldName,
    newName,
  });
}
