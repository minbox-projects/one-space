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

export interface ModelCapability {
  efforts: string[];
  default_effort?: string;
}

export interface ColumnModelSource {
  models: string[];
  error?: string;
  // Additive capability metadata. An omitted map (or omitted entry) means the
  // exact host-local model capability is unknown; an explicit empty entry means
  // the capability is known to have no writable values.
  model_capabilities?: Record<string, ModelCapability>;
  // Independent diagnostic kept separate from the column `error`. A capability
  // read failure must not drop candidate models or degrade healthy columns.
  capability_warning?: string;
}

const DEFAULT_REASONING_EFFORT: ValidEffort = "medium";

export function isKnownEffort(value: string): value is ValidEffort {
  return (VALID_EFFORTS as readonly string[]).includes(value);
}

export function getModelCapability(
  source: ColumnModelSource | undefined,
  model: string,
): ModelCapability | undefined {
  if (!source?.model_capabilities) return undefined;
  return source.model_capabilities[model];
}

// Intersection of the declared raw efforts with the profile enum, deduplicated
// and returned in profile enum order. The raw list may include values the
// profile schema does not allow (for example `none` or `minimal`).
export function resolveWritableEfforts(
  capability: ModelCapability | undefined,
): ValidEffort[] {
  if (!capability) return [];
  const declared = new Set(capability.efforts);
  return VALID_EFFORTS.filter((effort) => declared.has(effort));
}

// Raw discovered tokens that the profile schema excludes, deduplicated in
// first-seen order and used for the visible schema-limit explanation.
export function resolveExcludedEfforts(
  capability: ModelCapability | undefined,
): string[] {
  if (!capability) return [];
  const seen = new Set<string>();
  const excluded: string[] = [];
  for (const raw of capability.efforts) {
    if (isKnownEffort(raw) || seen.has(raw)) continue;
    seen.add(raw);
    excluded.push(raw);
  }
  return excluded;
}

// Shared normalization for every model mutation path (single-cell model change,
// typed model entry, row fill and column fill).
// - Known model: keep a currently writable effort, else the declared default,
//   else `medium` when writable, else the first writable effort. A known model
//   with no writable values has no automatic default and keeps the raw value.
// - Unknown model: keep a non-empty entered effort, else start at `medium`
//   without claiming support.
export function normalizeEffortForModel(
  currentEffort: string,
  capability: ModelCapability | undefined,
): string {
  const trimmed = (currentEffort ?? "").trim();
  if (!capability) {
    return trimmed ? currentEffort : DEFAULT_REASONING_EFFORT;
  }
  const writable = resolveWritableEfforts(capability);
  if (writable.length === 0) return currentEffort;
  if (isKnownEffort(trimmed) && writable.includes(trimmed)) return trimmed;
  const declaredDefault = capability.default_effort;
  if (
    declaredDefault &&
    isKnownEffort(declaredDefault) &&
    writable.includes(declaredDefault)
  ) {
    return declaredDefault;
  }
  if (writable.includes("medium")) return "medium";
  return writable[0];
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
