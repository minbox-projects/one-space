import { invoke } from "@tauri-apps/api/core";

export type AiWorkFlowInstallOperation = "install" | "update";
export type AiWorkFlowInstallState =
  | "idle"
  | "running"
  | "succeeded"
  | "failed"
  | "cancelled";
export type AiWorkFlowInstallStage =
  | "preparing"
  | "clone"
  | "verify_repository"
  | "pull"
  | "npm_ci"
  | "install"
  | "validate"
  | "complete";
export type AiWorkFlowLogSource = "system" | "stdout" | "stderr";

export type AiWorkFlowErrorPayload = {
  code: string;
  message: string;
};

export class AiWorkFlowError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = "AiWorkFlowError";
    this.code = code;
  }
}

export type AiWorkFlowInstallStatus = {
  state: AiWorkFlowInstallState;
  operation: AiWorkFlowInstallOperation | null;
  stage: AiWorkFlowInstallStage | null;
  started_at: string | null;
  finished_at: string | null;
  version: string | null;
  error: AiWorkFlowErrorPayload | null;
};

export type AiWorkFlowInstallVersion = {
  installed: boolean;
  version: string | null;
  error: AiWorkFlowErrorPayload | null;
};

export type AiWorkFlowInstallLog = {
  sequence: number;
  timestamp: string;
  stage: AiWorkFlowInstallStage;
  source: AiWorkFlowLogSource;
  message: string;
};

export type AiWorkFlowCancelResult = {
  accepted: boolean;
  status: AiWorkFlowInstallStatus;
};

export type AiWorkFlowEnvironmentSummary = {
  name: string;
  current: boolean;
  valid: boolean;
};

export type AiWorkFlowEnvironmentDocument = {
  name: string;
  content: string;
  value: unknown | null;
  current: boolean;
  valid: boolean;
  validation_error: AiWorkFlowErrorPayload | null;
};

export type AiWorkFlowEnvironmentStatus = {
  current: string;
  exists: boolean;
  valid: boolean;
};

function normalizeError(error: unknown): AiWorkFlowError {
  if (error instanceof AiWorkFlowError) return error;
  if (error && typeof error === "object") {
    const value = error as { code?: unknown; message?: unknown };
    if (typeof value.code === "string") {
      return new AiWorkFlowError(
        value.code,
        typeof value.message === "string" ? value.message : value.code,
      );
    }
  }
  return new AiWorkFlowError(
    "unknown",
    typeof error === "string" ? error : "AI Work Flow operation failed",
  );
}

async function aiWorkFlowInvoke<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    return args ? await invoke<T>(command, args) : await invoke<T>(command);
  } catch (error) {
    throw normalizeError(error);
  }
}

export const aiWorkFlowInstallStatus = () =>
  aiWorkFlowInvoke<AiWorkFlowInstallStatus>("ai_work_flow_install_status_get");
export const aiWorkFlowInstallVersion = () =>
  aiWorkFlowInvoke<AiWorkFlowInstallVersion>("ai_work_flow_install_version_get");
export const aiWorkFlowInstallOrUpdate = () =>
  aiWorkFlowInvoke<AiWorkFlowInstallStatus>("ai_work_flow_install_or_update");
export const aiWorkFlowInstallCancel = () =>
  aiWorkFlowInvoke<AiWorkFlowCancelResult>("ai_work_flow_install_cancel");
export const aiWorkFlowInstallLogs = () =>
  aiWorkFlowInvoke<AiWorkFlowInstallLog[]>("ai_work_flow_install_logs_get");

export const aiWorkFlowEnvironmentList = () =>
  aiWorkFlowInvoke<AiWorkFlowEnvironmentSummary[]>(
    "ai_work_flow_environment_list",
  );
export const aiWorkFlowEnvironmentCreate = (name: string, content: string) =>
  aiWorkFlowInvoke<AiWorkFlowEnvironmentDocument>(
    "ai_work_flow_environment_create",
    { name, content },
  );
export const aiWorkFlowEnvironmentRead = (name: string) =>
  aiWorkFlowInvoke<AiWorkFlowEnvironmentDocument>(
    "ai_work_flow_environment_read",
    { name },
  );
export const aiWorkFlowEnvironmentUpdate = (name: string, content: string) =>
  aiWorkFlowInvoke<AiWorkFlowEnvironmentDocument>(
    "ai_work_flow_environment_update",
    { name, content },
  );
export const aiWorkFlowEnvironmentDelete = (name: string) =>
  aiWorkFlowInvoke<AiWorkFlowEnvironmentStatus>(
    "ai_work_flow_environment_delete",
    { name },
  );
export const aiWorkFlowEnvironmentUse = (name: string) =>
  aiWorkFlowInvoke<AiWorkFlowEnvironmentStatus>(
    "ai_work_flow_environment_use",
    { name },
  );
export const aiWorkFlowEnvironmentStatus = () =>
  aiWorkFlowInvoke<AiWorkFlowEnvironmentStatus>(
    "ai_work_flow_environment_status",
  );
