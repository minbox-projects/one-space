import { invoke } from "@tauri-apps/api/core";

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

export async function listProfiles(
  homeOverride?: string,
): Promise<ProfileSummary[]> {
  return invoke<ProfileSummary[]>("ai_workflow_list_profiles", {
    homeOverride,
  });
}

export async function getProfileMatrix(
  name: string,
  homeOverride?: string,
): Promise<ProfileMatrix> {
  return invoke<ProfileMatrix>("ai_workflow_get_profile_matrix", {
    name,
    homeOverride,
  });
}

export async function getModelSources(
  homeOverride?: string,
): Promise<ModelSourcesResult> {
  return invoke<ModelSourcesResult>("ai_workflow_get_model_sources", {
    homeOverride,
  });
}

export async function saveAndActivateProfile(
  name: string,
  matrix: AgentMatrixRow[],
  homeOverride?: string,
): Promise<ProfileActivationReport> {
  return invoke<ProfileActivationReport>(
    "ai_workflow_save_and_activate_profile",
    {
      name,
      matrix,
      homeOverride,
    },
  );
}

export async function activateProfile(
  name: string,
  homeOverride?: string,
): Promise<ProfileActivationReport> {
  return invoke<ProfileActivationReport>("ai_workflow_activate_profile", {
    name,
    homeOverride,
  });
}

export async function createProfile(
  name: string,
  copyFrom?: string,
  homeOverride?: string,
): Promise<void> {
  return invoke<void>("ai_workflow_create_profile", {
    name,
    copyFrom,
    homeOverride,
  });
}

export async function deleteProfile(
  name: string,
  homeOverride?: string,
): Promise<void> {
  return invoke<void>("ai_workflow_delete_profile", {
    name,
    homeOverride,
  });
}

