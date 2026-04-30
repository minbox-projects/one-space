export type TerminalPermissionMode = 'default' | 'full_access';
export type AiModelId = 'claude' | 'gemini' | 'codex' | 'opencode';

export interface AiModelPermissionModes {
  claude: TerminalPermissionMode;
  gemini: TerminalPermissionMode;
  codex: TerminalPermissionMode;
  opencode: TerminalPermissionMode;
}

export const DEFAULT_AI_MODEL_PERMISSION_MODES: AiModelPermissionModes = {
  claude: 'default',
  gemini: 'default',
  codex: 'default',
  opencode: 'default',
};

const VALID_MODES: Set<string> = new Set(['default', 'full_access']);

function coerceMode(value: unknown): TerminalPermissionMode {
  if (typeof value === 'string' && VALID_MODES.has(value)) {
    return value as TerminalPermissionMode;
  }
  return 'default';
}

export function normalizeAiModelPermissionModesForUi(
  raw?: Record<string, string> | AiModelPermissionModes,
): AiModelPermissionModes {
  if (!raw) {
    return { ...DEFAULT_AI_MODEL_PERMISSION_MODES };
  }
  return {
    claude: coerceMode(raw['claude']),
    gemini: coerceMode(raw['gemini']),
    codex: coerceMode(raw['codex']),
    opencode: coerceMode(raw['opencode']),
  };
}

export function getPermissionModeLabel(mode: TerminalPermissionMode): string {
  return mode === 'full_access' ? 'full_access' : 'default';
}

export function getFullAccessFlag(modelId: AiModelId): { flag?: string; env?: Record<string, string> } {
  switch (modelId) {
    case 'claude':
      return { flag: '--dangerously-skip-permissions' };
    case 'gemini':
      return { flag: '--approval-mode=yolo' };
    case 'codex':
      return { flag: '--dangerously-bypass-approvals-and-sandbox' };
    case 'opencode':
      return { env: { OPENCODE_PERMISSION: 'allow' } };
    default:
      return {};
  }
}

/** Extract the error code from a Tauri invoke error, if present. */
export function getInvokeErrorCode(err: unknown): string | null {
  if (err && typeof err === 'object') {
    const maybe = err as { code?: unknown };
    if (typeof maybe.code === 'string') return maybe.code;
  }
  return null;
}

/** Format a Tauri invoke error into a human-readable string. */
export function formatInvokeError(err: unknown): string {
  if (typeof err === 'string') return err;
  if (err && typeof err === 'object') {
    const maybe = err as { code?: unknown; message?: unknown; error?: unknown };
    const code = typeof maybe.code === 'string' ? maybe.code : null;
    const msg = typeof maybe.message === 'string' ? maybe.message : null;
    const errMsg = typeof maybe.error === 'string' ? maybe.error : null;
    if (msg) return code ? `[${code}] ${msg}` : msg;
    if (errMsg) return code ? `[${code}] ${errMsg}` : errMsg;
    if (code) return `[${code}]`;
    try {
      return JSON.stringify(err);
    } catch {
      return String(err);
    }
  }
  return String(err);
}
