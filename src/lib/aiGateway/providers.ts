import type { GatewayKey, GatewayProviderKey, GatewayProviderKeyState, GatewaySettings, GatewayUpstreamProvider, GatewayUpstreamProviderWithKeys } from "./types";

/** i18n keys for each derived key state. */
export const PROVIDER_KEY_STATE_TRANSLATION_KEYS: Record<
  GatewayProviderKeyState,
  string
> = {
  usable: "aiGatewayProviderKeyStateUsable",
  disabled: "aiGatewayProviderKeyStateDisabled",
  quota: "aiGatewayProviderKeyStateQuota",
  auth: "aiGatewayProviderKeyStateAuth",
};


/**
 * Derive a key's display state. The user's `enabled` flag wins over any runtime
 * mark; an auto-marked key is quota-exhausted unless it failed authentication.
 */
export function providerKeyState(key: GatewayProviderKey): GatewayProviderKeyState {
  if (!key.enabled) return "disabled";
  if (key.auto_marked) {
    return key.failure_kind === "authentication" ? "auth" : "quota";
  }
  return "usable";
}


/** Read a provider's ordered key pool; a legacy or missing field yields an empty list. */
export function providerKeyPool(
  provider: GatewayUpstreamProvider | null | undefined,
): GatewayProviderKey[] {
  return (
    (provider as GatewayUpstreamProviderWithKeys | null | undefined)?.keys ?? []
  );
}


/** The runtime-marked keys of a provider, preserving the pool order. */
export function providerMarkedKeys(
  provider: GatewayUpstreamProvider | null | undefined,
): GatewayProviderKey[] {
  return providerKeyPool(provider).filter((key) => key.auto_marked);
}


export const DEFAULT_GATEWAY_SETTINGS: GatewaySettings = {
  bind_address: "127.0.0.1",
  enable_cors: true,
  require_api_key_for_lan: true,
  connect_timeout_secs: 10,
  read_timeout_secs: 60,
  request_budget_secs: 120,
  max_retries_per_provider: 3,
  retry_base_delay_ms: 1000,
  failure_threshold: 3,
  probe_cooldown_secs: 60,
  key_quota_ttl_secs: 1800,
  session_affinity_timeout_secs: 1800,
  context_hold_timeout_secs: 300,
  custom_session_headers: [],
  daily_reset_offset_hours: 8,
  quota_cache_ttl_secs: 60,
  debug_logging: false,
};


export function normalizeGatewaySettings(
  settings?: Partial<GatewaySettings> | null,
): GatewaySettings {
  if (!settings || typeof settings !== "object") {
    return { ...DEFAULT_GATEWAY_SETTINGS };
  }
  return {
    bind_address:
      typeof settings.bind_address === "string"
        ? settings.bind_address
        : DEFAULT_GATEWAY_SETTINGS.bind_address,
    enable_cors:
      typeof settings.enable_cors === "boolean"
        ? settings.enable_cors
        : DEFAULT_GATEWAY_SETTINGS.enable_cors,
    require_api_key_for_lan:
      typeof settings.require_api_key_for_lan === "boolean"
        ? settings.require_api_key_for_lan
        : DEFAULT_GATEWAY_SETTINGS.require_api_key_for_lan,

    connect_timeout_secs: Number.isFinite(settings.connect_timeout_secs)
      ? Number(settings.connect_timeout_secs)
      : DEFAULT_GATEWAY_SETTINGS.connect_timeout_secs,
    read_timeout_secs: Number.isFinite(settings.read_timeout_secs)
      ? Number(settings.read_timeout_secs)
      : DEFAULT_GATEWAY_SETTINGS.read_timeout_secs,
    request_budget_secs: Number.isFinite(settings.request_budget_secs)
      ? Number(settings.request_budget_secs)
      : DEFAULT_GATEWAY_SETTINGS.request_budget_secs,
    max_retries_per_provider: Number.isFinite(settings.max_retries_per_provider)
      ? Number(settings.max_retries_per_provider)
      : DEFAULT_GATEWAY_SETTINGS.max_retries_per_provider,
    retry_base_delay_ms: Number.isFinite(settings.retry_base_delay_ms)
      ? Number(settings.retry_base_delay_ms)
      : DEFAULT_GATEWAY_SETTINGS.retry_base_delay_ms,

    failure_threshold: Number.isFinite(settings.failure_threshold)
      ? Number(settings.failure_threshold)
      : DEFAULT_GATEWAY_SETTINGS.failure_threshold,
    probe_cooldown_secs: Number.isFinite(settings.probe_cooldown_secs)
      ? Number(settings.probe_cooldown_secs)
      : DEFAULT_GATEWAY_SETTINGS.probe_cooldown_secs,
    key_quota_ttl_secs: Number.isFinite(settings.key_quota_ttl_secs)
      ? Number(settings.key_quota_ttl_secs)
      : DEFAULT_GATEWAY_SETTINGS.key_quota_ttl_secs,

    session_affinity_timeout_secs: Number.isFinite(
      settings.session_affinity_timeout_secs,
    )
      ? Number(settings.session_affinity_timeout_secs)
      : DEFAULT_GATEWAY_SETTINGS.session_affinity_timeout_secs,
    context_hold_timeout_secs: Number.isFinite(settings.context_hold_timeout_secs)
      ? Number(settings.context_hold_timeout_secs)
      : DEFAULT_GATEWAY_SETTINGS.context_hold_timeout_secs,
    custom_session_headers: Array.isArray(settings.custom_session_headers)
      ? settings.custom_session_headers
      : [],

    daily_reset_offset_hours: Number.isFinite(settings.daily_reset_offset_hours)
      ? Number(settings.daily_reset_offset_hours)
      : DEFAULT_GATEWAY_SETTINGS.daily_reset_offset_hours,
    quota_cache_ttl_secs: Number.isFinite(settings.quota_cache_ttl_secs)
      ? Number(settings.quota_cache_ttl_secs)
      : DEFAULT_GATEWAY_SETTINGS.quota_cache_ttl_secs,

    debug_logging: Boolean(settings.debug_logging),
  };
}


/**
 * Resolve the effective default local key id.
 *
 * Mirrors the backend rule: a manual choice wins while it points at an enabled
 * key; otherwise the search advances to the next enabled key in list order
 * (wrapping), and empty means no enabled key is available.
 */
export function resolveDefaultKeyId(
  keys: GatewayKey[],
  stored: string | null | undefined,
): string | null {
  if (keys.length === 0) return null;
  const storedIndex = stored
    ? keys.findIndex((key) => key.id === stored)
    : -1;

  if (storedIndex >= 0) {
    if (keys[storedIndex].enabled) return keys[storedIndex].id;
    for (let offset = 1; offset <= keys.length; offset += 1) {
      const candidate = (storedIndex + offset) % keys.length;
      if (keys[candidate].enabled) return keys[candidate].id;
    }
    return null;
  }

  return keys.find((key) => key.enabled)?.id ?? null;
}


/** Trim, drop blanks and deduplicate reasoning efforts preserving first-seen order. */
export function normalizeReasoningEfforts(
  efforts: string[] | null | undefined,
): string[] {
  const normalized: string[] = [];
  for (const effort of efforts ?? []) {
    const trimmed = effort.trim();
    if (trimmed !== "" && !normalized.includes(trimmed)) normalized.push(trimmed);
  }
  return normalized;
}


/** Redact a secret for display while keeping head/tail recognizable. */
export function maskSecret(value: string): string {
  if (!value) return "";
  if (value.length <= 8) return "•".repeat(value.length);
  return `${value.slice(0, 3)}${"•".repeat(6)}${value.slice(-4)}`;
}


/** Render a unix-seconds timestamp as a stable `YYYY-MM-DD HH:mm` string. */
export function formatGatewayTimestamp(ts: number | null | undefined): string | null {
  if (ts === null || ts === undefined) return null;
  const date = new Date(ts * 1000);
  const pad = (part: number) => String(part).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(
    date.getHours(),
  )}:${pad(date.getMinutes())}`;
}
