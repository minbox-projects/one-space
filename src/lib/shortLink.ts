import { invoke } from "@tauri-apps/api/core";

export const SHORT_LINK_ERROR_CODES = [
  "not_configured",
  "invalid_url",
  "authentication_failed",
  "rate_limited",
  "request_rejected",
  "service_unavailable",
  "network_error",
  "invalid_response",
  "storage_error",
] as const;

export type ShortLinkErrorCode = (typeof SHORT_LINK_ERROR_CODES)[number];

export type ShortLinkConfigStatus = {
  configured: boolean;
};

export type ShortLinkCreateResponse = {
  longUrl: string;
  shortUrl: string;
};

type ShortLinkBackendError = {
  code: unknown;
  message?: unknown;
};

const SHORT_LINK_ERROR_CODE_SET = new Set<string>(SHORT_LINK_ERROR_CODES);

export class ShortLinkError extends Error {
  readonly code: ShortLinkErrorCode | "unknown";
  readonly diagnostic?: string;

  constructor(code: ShortLinkErrorCode | "unknown", diagnostic?: string) {
    super(diagnostic ?? "Short link command failed");
    this.name = "ShortLinkError";
    this.code = code;
    this.diagnostic = diagnostic;
  }
}

function isBackendError(error: unknown): error is ShortLinkBackendError {
  return typeof error === "object" && error !== null && "code" in error;
}

function normalizeError(error: unknown): ShortLinkError {
  if (error instanceof ShortLinkError) return error;

  if (
    isBackendError(error) &&
    typeof error.code === "string" &&
    SHORT_LINK_ERROR_CODE_SET.has(error.code)
  ) {
    const diagnostic = typeof error.message === "string" ? error.message : undefined;
    return new ShortLinkError(error.code as ShortLinkErrorCode, diagnostic);
  }

  return new ShortLinkError("unknown");
}

async function shortLinkInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return args ? await invoke<T>(command, args) : await invoke<T>(command);
  } catch (error) {
    throw normalizeError(error);
  }
}

export async function shortLinkConfigStatus(): Promise<ShortLinkConfigStatus> {
  const response = await shortLinkInvoke<ShortLinkConfigStatus>("short_link_config_status");
  return { configured: response.configured };
}

export async function shortLinkSaveToken(token: string): Promise<ShortLinkConfigStatus> {
  const response = await shortLinkInvoke<ShortLinkConfigStatus>("short_link_save_token", { token });
  return { configured: response.configured };
}

export async function shortLinkDeleteToken(): Promise<ShortLinkConfigStatus> {
  const response = await shortLinkInvoke<ShortLinkConfigStatus>("short_link_delete_token");
  return { configured: response.configured };
}

export async function shortLinkCreate(url: string): Promise<ShortLinkCreateResponse> {
  const response = await shortLinkInvoke<ShortLinkCreateResponse>("short_link_create", { url });
  return { longUrl: response.longUrl, shortUrl: response.shortUrl };
}
