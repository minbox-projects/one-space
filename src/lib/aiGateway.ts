import { invoke } from "@tauri-apps/api/core";
import type { TFunction } from "i18next";

export const AI_GATEWAY_RELEASE_PORT = 17688;
export const AI_GATEWAY_DEV_PORT = 17689;
export const AI_GATEWAY_DEFAULT_PORT = import.meta.env.DEV
  ? AI_GATEWAY_DEV_PORT
  : AI_GATEWAY_RELEASE_PORT;
export const AI_GATEWAY_STATUS_UPDATED_EVENT = "ai-gateway-status-update";
export const AI_GATEWAY_CONFIG_UPDATED_EVENT = "ai-gateway-config-update";
/** Runtime broadcast emitted when a key transitions into the authentication-failed mark. */
export const AI_GATEWAY_KEY_AUTH_FAILED_EVENT = "ai-gateway-key-auth-failed";
/** Per-provider aggregation window for authentication-failed toasts. */
export const AI_GATEWAY_KEY_AUTH_TOAST_WINDOW_MS = 1500;
/** Delay before an unacknowledged provider-card warning pill auto-dismisses. */
export const AI_GATEWAY_ALERT_AUTO_DISMISS_MS = 8000;

/** Terminal tools AI Gateway is allowed to configure; Antigravity is excluded. */
export const AI_GATEWAY_SUPPORTED_TERMINAL_TOOLS = [
  "opencode",
  "codex",
  "claude",
] as const;

export interface GatewayModelMapping {
  local_model: string;
  upstream_model: string;
  /** Optional name surfaced by the gateway for this model. */
  display_name?: string | null;
  /** `null`/absent means this row inherits the provider protocol. */
  protocol?: GatewayUpstreamProtocol | null;
  /**
   * Whether this mapping participates in routing. Absent/`undefined` is treated
   * as enabled for backwards compatibility with configs written before the flag
   * existed; only an explicit `false` disables the row.
   */
  enabled?: boolean;
  /** Reasoning-effort identifiers the model advertises. */
  reasoning_efforts?: string[];
  /**
   * Declared total context window in tokens. Absent/`null` means unspecified;
   * a bound mapping inherits this value from its template instead.
   */
  context_window?: number | null;
  /**
   * Row-scoped runtime health state. Absent/`undefined` means healthy; the
   * backend only sets these after a real upstream failure and clears them
   * through an explicit row-level re-enable.
   */
  auto_disabled?: boolean;
  disabled_reason?: string | null;
  disabled_at?: number | null;
  consecutive_failures?: number;
  last_error_at?: number | null;
}

/**
 * Upstream endpoint family a provider exposes. Same-protocol requests are
 * forwarded unchanged; an `anthropic_messages` mapping can serve
 * Chat/Responses ingress through the conversion service.
 */
export type GatewayUpstreamProtocol =
  | "chat_completions"
  | "responses"
  | "anthropic_messages";

/**
 * The four explicit model choices an operator submits for a Claude terminal
 * sync: the default model plus the haiku/sonnet/opus family aliases.
 *
 * Values are local mapping ids (the ids Claude Code sends), never vendor ids
 * guessed from a name, and the same id may fill several slots. The shape is
 * shared with the backend terminal-target snapshot; it never carries a
 * credential value.
 */
export interface ClaudeModelSelection {
  default_model: string;
  haiku: string;
  sonnet: string;
  opus: string;
}

/** Key-scoped failure kinds that persist a runtime mark on one upstream key. */
export type GatewayProviderKeyFailureKind = "authentication" | "quota";

/**
 * One entry of a provider's ordered upstream credential pool.
 *
 * `enabled` is the user's intent; `auto_marked` with `failure_kind` is runtime
 * state the backend owns. The list order is the persisted priority order.
 */
export interface GatewayProviderKey {
  id: string;
  /** Required non-blank name that also serves as the key's remark. */
  name: string;
  value: string;
  enabled: boolean;
  /**
   * Independent per-key daily-reset preference. Optional so the pre-feature
   * fixtures and records stay assignable; absent/`undefined` reads as `false`.
   * A `true` key whose quota mark predates the current UTC+8 day becomes
   * eligible again at the next UTC+8 midnight instead of the legacy 1800s TTL.
   */
  daily_reset?: boolean;
  auto_marked: boolean;
  failure_kind: GatewayProviderKeyFailureKind | null;
  marked_at: number | null;
  reason: string | null;
}

/** Derived per-key display state shared by the editor and the provider card. */
export type GatewayProviderKeyState = "usable" | "disabled" | "quota" | "auth";

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

export interface GatewayUpstreamProvider {
  id: string;
  name: string;
  base_url: string;
  default_model: string | null;
  protocol?: GatewayUpstreamProtocol;
  mappings: GatewayModelMapping[];
  enabled: boolean;
  /** Template this provider was created from; absent for manual providers. */
  template_id?: string | null;
  /** Template models the user deleted for this provider. */
  ignored_models?: string[];
  /** Routing weight for weighted round-robin (1-100, default 1). */
  weight?: number;
  /** Tags for categorizing / filtering providers. */
  tags?: string[];
  /** Optional custom icon override; if not set, defaults to inheriting from template. */
  icon?: string | null;
}

/**
 * A provider record carrying its ordered upstream key pool.
 *
 * The pool is a separate optional extension of the base record so the
 * pre-pool fixture shapes used across the suites stay assignable; the backend
 * always serializes `keys`, including an empty array.
 */
export type GatewayUpstreamProviderWithKeys = GatewayUpstreamProvider & {
  keys?: GatewayProviderKey[];
};

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

/**
 * Payload of the authentication-failed runtime event.
 *
 * It never carries a key value; `key_name` is the operator-visible label.
 */
export interface GatewayKeyAuthFailedEvent {
  provider_id: string;
  provider_name: string;
  key_id: string;
  key_name: string;
  reason: string;
  marked_at: number;
}

export interface QuotaCredits {
  monthlyCredits: number;
  purchasedCredits: number;
  freeCredits: number;
  belowThreshold: boolean;
}

export interface QuotaWindow {
  used: number;
  cap: number;
  exceeded: boolean;
  resetAt?: string | number | null;
}

export interface QuotaWindowLimits {
  limited: boolean;
  fiveHour?: QuotaWindow | null;
  weekly?: QuotaWindow | null;
}

export interface ProviderQuota {
  credits: QuotaCredits;
  windowLimits?: QuotaWindowLimits | null;
}

export interface GoUsageWindow {
  status: "ok" | "rate-limited";
  percent: number;
  resetsAt: string;
}

export interface GoUsage {
  rolling: GoUsageWindow;
  weekly: GoUsageWindow;
  monthly: GoUsageWindow;
}

export interface ProviderGoUsage {
  usage: GoUsage;
}

export interface GatewayKey {
  id: string;
  label: string;
  value: string;
  enabled: boolean;
  created_at: number;
}

export interface GatewayTerminalSyncRecord {
  provider_id: string;
  tool: string;
  synced_key_id: string;
  synced_base_url: string;
  synced_at: number;
}

export interface GatewaySettings {
  bind_address: string;
  enable_cors: boolean;
  proxy_url?: string | null;
  require_api_key_for_lan: boolean;

  connect_timeout_secs: number;
  read_timeout_secs: number;
  request_budget_secs: number;
  max_retries_per_provider: number;
  retry_base_delay_ms: number;

  failure_threshold: number;
  probe_cooldown_secs: number;
  key_quota_ttl_secs: number;

  session_affinity_timeout_secs: number;
  context_hold_timeout_secs: number;
  custom_session_headers: string[];

  daily_reset_offset_hours: number;
  quota_cache_ttl_secs: number;

  debug_logging: boolean;
}

export const DEFAULT_GATEWAY_SETTINGS: GatewaySettings = {
  bind_address: "127.0.0.1",
  enable_cors: true,
  proxy_url: null,
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
    proxy_url:
      typeof settings.proxy_url === "string"
        ? settings.proxy_url
        : settings.proxy_url === null
        ? null
        : DEFAULT_GATEWAY_SETTINGS.proxy_url,
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

export interface GatewayConfig {
  enabled: boolean;
  port: number;
  providers: GatewayUpstreamProvider[];
  keys: GatewayKey[];
  default_key_id: string | null;
  terminal_syncs: GatewayTerminalSyncRecord[];
  /** Provider-scoped price rows; optional so older fixtures stay valid. */
  model_prices?: ModelPrice[];
  /**
   * Operator-defined model pools; absent/empty means no pools (default for
   * configs written before the feature). Pool records never carry source keys
   * or endpoints — only stable identity and exact source references.
   */
  model_pools?: GatewayModelPool[];
  /**
   * Minimal alias reservation: pool model ids previously/currently owned by a
   * pool. It is never advertised as callable and only blocks ordinary routing
   * from reusing a retired alias.
   */
  model_pool_reserved_models?: string[];
  /** Dynamic gateway operational settings */
  settings?: GatewaySettings;
}

export interface GatewayStatus {
  running: boolean;
  enabled: boolean;
  port: number;
  local_base_url: string;
  provider_count: number;
  auto_disabled_count: number;
  key_count: number;
  default_key_id: string | null;
}

/** How many applicable models or Claude choices declare a known context window. */
export interface GatewayTerminalContextCoverage {
  total: number;
  known: number;
  unknown: number;
}

/**
 * A target's expected native context configuration. It describes what a new or
 * rematerialized session receives; it never claims that a running CLI consumed it.
 */
export interface GatewayTerminalContextSummary {
  mode: "per_model" | "selected_model" | "shared_compaction";
  scope: "generated_models" | "activation" | "global_and_isolated_settings";
  coverage: GatewayTerminalContextCoverage;
  selected_model?: string | null;
  raw_context?: number | null;
  applied_context?: number | null;
  /** Stable reason codes such as `unknown_context` or `below_minimum`. */
  reasons: string[];
}

export interface GatewayTerminalTarget {
  tool: string;
  name: string;
  provider_id: string | null;
  base_url: string | null;
  synced: boolean;
  pending_sync: boolean;
  synced_key_id: string | null;
  synced_at: number | null;
  /**
   * Model-only Claude selection snapshot (the four local mapping ids) for the
   * `claude` target; absent for the OpenAI tools and never a credential.
   */
  claude_models?: ClaudeModelSelection | null;
  /** Expected native context configuration; absent on pre-field snapshots. */
  context?: GatewayTerminalContextSummary | null;
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

/** Build the local OpenAI-compatible base address (with `/v1` suffix) the listener binds to. */
export function localBaseUrl(port: number): string {
  return `http://127.0.0.1:${port}/v1`;
}

/** Build the local Anthropic Messages-compatible API endpoint address. */
export function localAnthropicMessagesUrl(port: number): string {
  return `http://127.0.0.1:${port}/v1/messages`;
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

/** Declared context-window bounds in total tokens. */
export const CONTEXT_WINDOW_MIN = 1;
export const CONTEXT_WINDOW_MAX = 4294967295;
/** Claude's shared compaction threshold boundaries. */
export const CLAUDE_COMPACTION_MIN = 100000;
export const CLAUDE_COMPACTION_CAP = 1000000;

/** Commonly used context-window presets for operator convenience. */
export const CONTEXT_WINDOW_PRESETS = [
  "32K",
  "64K",
  "128K",
  "200K",
  "256K",
  "512K",
  "1M",
  "2M",
] as const;

/** A declared value usable as a context window; `null`/zero means unspecified. */
export function knownContextWindow(
  value: number | null | undefined,
): number | null {
  return typeof value === "number" &&
    Number.isInteger(value) &&
    value >= CONTEXT_WINDOW_MIN &&
    value <= CONTEXT_WINDOW_MAX
    ? value
    : null;
}

/**
 * Parse an operator's context-window draft. Blank means unspecified, while a
 * nonnumeric, fractional or out-of-range value stays invalid so the caller can
 * keep the draft visible and block the save.
 * Supports compact units such as "1M", "256K", "128k", "1.5M" as well as raw integers.
 */
export function parseContextWindowInput(raw: string): {
  value: number | null;
  invalid: boolean;
} {
  const trimmed = raw.trim();
  if (trimmed === "") return { value: null, invalid: false };

  // Raw positive integer check
  if (/^\d+$/.test(trimmed)) {
    const value = Number(trimmed);
    if (
      !Number.isSafeInteger(value) ||
      value < CONTEXT_WINDOW_MIN ||
      value > CONTEXT_WINDOW_MAX
    ) {
      return { value: null, invalid: true };
    }
    return { value, invalid: false };
  }

  // Match compact human-friendly units: e.g. "1M", "256K", "128k", "1.5M"
  const match = trimmed.match(/^(\d+(?:\.\d+)?)\s*([kmKM])$/);
  if (match) {
    const num = Number(match[1]);
    const unit = match[2].toUpperCase();
    if (!Number.isFinite(num) || num <= 0) {
      return { value: null, invalid: true };
    }
    const multiplier = unit === "M" ? 1_000_000 : 1_000;
    const value = Math.round(num * multiplier);
    if (
      !Number.isSafeInteger(value) ||
      value < CONTEXT_WINDOW_MIN ||
      value > CONTEXT_WINDOW_MAX
    ) {
      return { value: null, invalid: true };
    }
    return { value, invalid: false };
  }

  return { value: null, invalid: true };
}

/** Group a token count with deterministic separators for display. */
export function formatContextWindow(value: number): string {
  return String(value).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/**
 * Formats a context-window token count into commonly used compact units like "1M", "256K".
 * Handles both decimal (1000) and binary (1024) thresholds gracefully.
 * Returns an empty string for null, undefined, or non-positive values.
 */
export function formatContextWindowCompact(
  value: number | null | undefined,
): string {
  if (value == null || !Number.isFinite(value) || value <= 0) {
    return "";
  }

  // Megatoken boundaries (>= 1,000,000)
  if (value >= 1_000_000) {
    // Prefer clean decimal millions first: 1,000,000 -> 1M, 2,000,000 -> 2M
    if (value % 1_000_000 === 0) {
      return `${value / 1_000_000}M`;
    }
    // Binary megatokens: 1,048,576 -> 1M, 2,097,152 -> 2M
    if (value % 1_048_576 === 0) {
      return `${value / 1_048_576}M`;
    }
    // Round to 1 decimal place with proper rounding: e.g. 1,050,000 -> 1.1M, 1,500,000 -> 1.5M
    const roundedMillions = Math.round((value / 1_000_000) * 10) / 10;
    return `${roundedMillions}M`;
  }

  // Kilotoken boundaries (>= 1,000)
  if (value >= 1_000) {
    // Prefer clean decimal thousands first: 128,000 -> 128K, 256,000 -> 256K, 64,000 -> 64K
    if (value % 1000 === 0) {
      return `${value / 1000}K`;
    }
    // Binary kilotokens: 131,072 -> 128K, 65,536 -> 64K, 262,144 -> 256K, 4096 -> 4K
    if (value % 1024 === 0) {
      return `${value / 1024}K`;
    }
    // Round to 1 decimal place with proper rounding
    const roundedKilos = Math.round((value / 1000) * 10) / 10;
    if (roundedKilos >= 1000) {
      return "1M";
    }
    return `${roundedKilos}K`;
  }

  return String(value);
}

/**
 * Formats a context-window token count for standard model list and dialog displays:
 * - Below 10,000 tokens: keeps raw comma-formatted number (e.g. 4,096, 8,192).
 * - 10,000 to 999,999 tokens: compact kilotokens (e.g. 16K, 64K, 128K, 256K).
 * - 1,000,000+ tokens: compact megatokens (e.g. 1M, 1.5M, 2M).
 * Returns empty string when value is null, undefined, or non-positive.
 */
export function formatContextWindowDisplay(
  value: number | null | undefined,
): string {
  if (value == null || !Number.isFinite(value) || value <= 0) {
    return "";
  }
  if (value < 10_000) {
    return formatContextWindow(value);
  }
  return formatContextWindowCompact(value);
}

/**
 * A template model's declared context, matched by trimmed, case-sensitive
 * upstream model id. The first matching model wins, including when its value is
 * unspecified; a missing model is also unspecified.
 */
export function resolveTemplateContextWindow(
  template: GatewayProviderTemplate | null | undefined,
  upstreamModel: string,
): number | null {
  if (!template) return null;
  const target = upstreamModel.trim();
  if (target === "") return null;
  const model = template.models.find(
    (entry) => entry.upstream_model.trim() === target,
  );
  return knownContextWindow(model?.context_window);
}

function effectiveMappingProtocol(
  provider: GatewayUpstreamProvider,
  mapping: GatewayModelMapping,
): GatewayUpstreamProtocol {
  return mapping.protocol ?? provider.protocol ?? "chat_completions";
}

interface ContextSourceRef {
  providerId: string;
  localModel: string;
  upstreamModel: string;
  protocol: GatewayUpstreamProtocol;
}

/** Resolve one exact source reference's declared context; missing is unknown. */
function resolveContextSource(
  providers: GatewayUpstreamProvider[],
  ref: ContextSourceRef,
): number | null {
  const targetProviderId = ref.providerId?.trim() ?? "";
  const targetLocal = ref.localModel?.trim() ?? "";
  const targetUpstream = ref.upstreamModel?.trim() ?? "";
  if (targetProviderId === "" || targetUpstream === "") return null;

  const provider = providers.find(
    (entry) => (entry.id?.trim() ?? "") === targetProviderId,
  );
  if (!provider) return null;
  const mapping = provider.mappings.find((entry) => {
    const entryLocal = entry.local_model?.trim() ?? "";
    const entryUpstream = entry.upstream_model?.trim() ?? "";
    const localMatches = targetLocal === "" || entryLocal === targetLocal;
    return localMatches && entryUpstream === targetUpstream;
  });
  if (!mapping) return null;
  if (ref.protocol && effectiveMappingProtocol(provider, mapping) !== ref.protocol) return null;
  return knownContextWindow(mapping.context_window);
}

/**
 * Conservative aggregate over deduplicated exact source identities: every
 * source must declare a known value and the advertised bound is their minimum.
 */
function conservativeSourceContext(
  providers: GatewayUpstreamProvider[],
  refs: ContextSourceRef[],
): number | null {
  const seen = new Set<string>();
  let minimum: number | null = null;
  let count = 0;
  for (const ref of refs) {
    const key = `${ref.providerId}\u0000${ref.localModel}\u0000${ref.upstreamModel}\u0000${ref.protocol}`;
    if (seen.has(key)) continue;
    seen.add(key);
    count += 1;
    const value = resolveContextSource(providers, ref);
    if (value === null) return null;
    minimum = minimum === null ? value : Math.min(minimum, value);
  }
  return count === 0 ? null : minimum;
}

/**
 * Conservative ordinary-model context across every configured mapping that
 * serves `model`, plus same-provider default-model declarations. Runtime
 * readiness, keys and enabled state never raise the advertised bound.
 */
export function resolveOrdinaryContextWindow(
  providers: GatewayUpstreamProvider[],
  model: string,
): number | null {
  const publicModel = model.trim();
  if (publicModel === "") return null;
  const refs: ContextSourceRef[] = [];
  for (const provider of providers) {
    for (const mapping of provider.mappings) {
      if (mapping.local_model.trim() !== publicModel) continue;
      refs.push({
        providerId: provider.id,
        localModel: publicModel,
        upstreamModel: mapping.upstream_model.trim(),
        protocol: effectiveMappingProtocol(provider, mapping),
      });
    }
  }
  if (refs.length === 0) return null;
  for (const provider of providers) {
    const defaultModel = provider.default_model?.trim();
    if (!defaultModel || defaultModel !== publicModel) continue;
    const declared = provider.mappings.find(
      (mapping) => mapping.upstream_model.trim() === defaultModel,
    );
    if (!declared) return null;
    refs.push({
      providerId: provider.id,
      localModel: publicModel,
      upstreamModel: defaultModel,
      protocol: provider.protocol ?? "chat_completions",
    });
  }
  return conservativeSourceContext(providers, refs);
}

function expandFallbackModelRefs(
  providers: GatewayUpstreamProvider[],
  model: string,
): ContextSourceRef[] {
  const target = model.trim();
  if (target === "") return [];
  const refs: ContextSourceRef[] = [];
  for (const provider of providers) {
    for (const mapping of provider.mappings) {
      if (mapping.local_model.trim() !== target) continue;
      const upstreamModel = mapping.upstream_model.trim();
      if (upstreamModel === "") continue;
      refs.push({
        providerId: provider.id,
        localModel: target,
        upstreamModel,
        protocol: effectiveMappingProtocol(provider, mapping),
      });
    }
  }
  return refs;
}

/**
 * Conservative pool context across every candidate plus expanded ordered
 * fallback model. Unresolved references make the aggregate unknown; a pool has
 * no independent editable override.
 */
export function resolvePoolContextWindow(
  providers: GatewayUpstreamProvider[],
  pool:
    | (Pick<GatewayModelPool, "candidates"> & {
        context_window?: number | null;
        fallback_models?: string[];
      })
    | null
    | undefined,
): number | null {
  if (!pool) return null;
  if (pool.context_window != null && pool.context_window > 0) {
    return pool.context_window;
  }
  const refs: ContextSourceRef[] = pool.candidates.map((candidate) => ({
    providerId: candidate.provider_id,
    localModel: candidate.local_model,
    upstreamModel: candidate.upstream_model,
    protocol: candidate.protocol,
  }));
  for (const fallback of pool.fallback_models ?? []) {
    const expanded = expandFallbackModelRefs(providers, fallback);
    if (expanded.length === 0) return null;
    refs.push(...expanded);
  }
  return conservativeSourceContext(providers, refs);
}

/** Context of one exact provider mapping, used for per-source display. */
export function resolveMappingContextWindow(
  providers: GatewayUpstreamProvider[],
  providerId: string,
  localModel: string,
  upstreamModel: string,
): number | null {
  const targetProviderId = providerId?.trim() ?? "";
  const targetLocal = localModel?.trim() ?? "";
  const targetUpstream = upstreamModel?.trim() ?? "";
  if (targetProviderId === "" || targetUpstream === "") return null;

  const provider = providers.find(
    (entry) => (entry.id?.trim() ?? "") === targetProviderId,
  );
  if (!provider) return null;
  const mapping = provider.mappings.find((entry) => {
    const entryLocal = entry.local_model?.trim() ?? "";
    const entryUpstream = entry.upstream_model?.trim() ?? "";
    const localMatches = targetLocal === "" || entryLocal === targetLocal;
    return localMatches && entryUpstream === targetUpstream;
  });
  if (!mapping) return null;
  return resolveContextSource(providers, {
    providerId: targetProviderId,
    localModel: targetLocal,
    upstreamModel: targetUpstream,
    protocol: effectiveMappingProtocol(provider, mapping),
  });
}

/** One upstream source that can serve an aggregated local model. */
export interface AggregatedModelProvider {
  providerId: string;
  providerName: string;
  upstreamModel: string;
  endpoint: GatewayUpstreamProtocol;
  isDefault: boolean;
  /** Trimmed mapping `display_name`; absent for default-model sources or undeclared names. */
  displayName?: string;
  /** Normalized reasoning efforts advertised by this upstream mapping; absent when empty. */
  reasoningEfforts?: string[];
  /** Declared total context window on this mapping; absent means unknown. */
  contextWindow?: number | null;
}

/** A local model name and every enabled upstream source mapped to it. */
export interface AggregatedModel {
  model: string;
  providers: AggregatedModelProvider[];
  /** Unified deduplicated list of reasoning efforts across all enabled upstream providers; absent when empty. */
  reasoningEfforts?: string[];
  /** Conservative effective context across every configured source; absent when unknown. */
  contextWindow?: number | null;
}

/**
 * Aggregate the local models served by enabled providers.
 *
 * A provider contributes every non-blank mapping that is not user-disabled and
 * not auto-disabled. Results are grouped by local model and sorted
 * deterministically. Default models are fallback targets for unmapped requests
 * and are not listed here.
 */
export function aggregateModels(
  providers: GatewayUpstreamProvider[],
): AggregatedModel[] {
  const groups = new Map<string, AggregatedModelProvider[]>();

  providers.forEach((provider) => {
    if (!provider.enabled) return;

    provider.mappings.forEach((mapping) => {
      if (mapping.enabled === false || mapping.auto_disabled) return;
      const localModel = mapping.local_model.trim();
      if (!localModel) return;
      const upstreamModel = mapping.upstream_model.trim();
      if (!upstreamModel) return;
      const entries = groups.get(localModel) ?? [];
      const displayName = mapping.display_name?.trim();
      const reasoningEfforts = normalizeReasoningEfforts(mapping.reasoning_efforts);
      entries.push({
        providerId: provider.id,
        providerName: provider.name,
        upstreamModel,
        endpoint: mapping.protocol ?? provider.protocol ?? "chat_completions",
        isDefault: false,
        ...(displayName ? { displayName } : {}),
        ...(reasoningEfforts.length > 0 ? { reasoningEfforts } : {}),
      });
      groups.set(localModel, entries);
    });
  });

  return Array.from(groups.entries())
    .map(([model, entries]) => {
      const sortedProviders = [...entries].sort((a, b) => {
        const byName = a.providerName.localeCompare(b.providerName);
        return byName !== 0 ? byName : a.upstreamModel.localeCompare(b.upstreamModel);
      });
      const unifiedEfforts = normalizeReasoningEfforts(
        sortedProviders.flatMap((p) => p.reasoningEfforts ?? []),
      );
      return {
        model,
        providers: sortedProviders,
        ...(unifiedEfforts.length > 0 ? { reasoningEfforts: unifiedEfforts } : {}),
      };
    })
    .sort((a, b) => a.model.localeCompare(b.model));
}

/**
 * Resolve the display name for an aggregated local model.
 *
 * The first mapping source declaring a non-empty `displayName` wins; otherwise
 * the name falls back to the first mapping source's `upstreamModel`, and finally
 * to the local model itself when there are no sources.
 */
export function resolveAggregatedModelName(entry: AggregatedModel): string {
  for (const provider of entry.providers) {
    const displayName = provider.displayName?.trim();
    if (displayName) return displayName;
  }
  const source = entry.providers[0];
  return source ? source.upstreamModel.trim() : entry.model;
}

/**
 * Resolve the aggregated list of reasoning efforts supported by an aggregated model.
 *
 * Returns the unified list from all enabled sources; falls back to an empty list.
 */
export function resolveAggregatedReasoningEfforts(entry: AggregatedModel): string[] {
  if (entry.reasoningEfforts && entry.reasoningEfforts.length > 0) {
    return entry.reasoningEfforts;
  }
  return normalizeReasoningEfforts(
    entry.providers.flatMap((p) => p.reasoningEfforts ?? []),
  );
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

export function aiGatewayGetConfig() {
  return invoke<GatewayConfig>("ai_gateway_get_config");
}

export function aiGatewaySaveSettings(settings: GatewaySettings, port?: number) {
  return invoke<GatewayConfig>("ai_gateway_save_settings", { settings, port: port ?? null });
}

export function aiGatewayUpsertProvider(
  provider: GatewayUpstreamProvider,
  prices?: ModelPrice[] | null,
) {
  return invoke<GatewayConfig>("ai_gateway_upsert_provider", {
    provider,
    prices: prices ?? null,
  });
}

export function aiGatewayDeleteProvider(providerId: string) {
  return invoke<GatewayConfig>("ai_gateway_delete_provider", { providerId });
}

export function aiGatewaySetProviderEnabled(
  providerId: string,
  enabled: boolean,
) {
  return invoke<GatewayConfig>("ai_gateway_set_provider_enabled", {
    providerId,
    enabled,
  });
}

/** Clear the runtime state of one mapping row (`providerId`, trimmed key) and return the updated config. */
export function aiGatewayReenableProviderModel(
  providerId: string,
  localModel: string,
  upstreamModel: string,
) {
  return invoke<GatewayConfig>("ai_gateway_reenable_provider_model", {
    providerId,
    localModel,
    upstreamModel,
  });
}

/** Clear the runtime state of every auto-disabled row of one provider and return the updated config. */
export function aiGatewayReenableProviderModels(providerId: string) {
  return invoke<GatewayConfig>("ai_gateway_reenable_provider_models", {
    providerId,
  });
}

/** Clear one upstream key's runtime state (`providerId`, `keyId`) and return the updated config. */
export function aiGatewayReenableProviderKey(providerId: string, keyId: string) {
  return invoke<GatewayConfig>("ai_gateway_reenable_provider_key", {
    providerId,
    keyId,
  });
}

export function aiGatewayUpsertKey(key: GatewayKey) {
  return invoke<GatewayConfig>("ai_gateway_upsert_key", { key });
}

export function aiGatewayDeleteKey(keyId: string) {
  return invoke<GatewayConfig>("ai_gateway_delete_key", { keyId });
}

export function aiGatewaySetDefaultKey(keyId: string) {
  return invoke<GatewayConfig>("ai_gateway_set_default_key", { keyId });
}

export function aiGatewayStart() {
  return invoke<GatewayStatus>("ai_gateway_start");
}

export function aiGatewayStop() {
  return invoke<GatewayStatus>("ai_gateway_stop");
}

export function aiGatewayStatus() {
  return invoke<GatewayStatus>("ai_gateway_status");
}

export function aiGatewayProviderQuota(
  providerId: string,
  forceRefresh = false,
): Promise<ProviderQuota> {
  return invoke<ProviderQuota>("ai_gateway_provider_quota", {
    providerId,
    forceRefresh,
  });
}

export function aiGatewayProviderGoUsage(
  providerId: string,
  forceRefresh = false,
): Promise<ProviderGoUsage> {
  return invoke<ProviderGoUsage>("ai_gateway_provider_go_usage", {
    providerId,
    forceRefresh,
  });
}

export function aiGatewayTerminalTargets() {
  return invoke<GatewayTerminalTarget[]>("ai_gateway_terminal_targets");
}

export function aiGatewayConfigureTerminal(
  targetTools: string[],
  claudeModels?: ClaudeModelSelection,
) {
  return invoke<GatewayTerminalSyncRecord[]>("ai_gateway_configure_terminal", {
    targetTools,
    ...(claudeModels ? { claudeModels } : {}),
  });
}

export function aiGatewaySyncTerminal(
  targetTools?: string[],
  claudeModels?: ClaudeModelSelection,
) {
  return invoke<GatewayTerminalSyncRecord[]>("ai_gateway_sync_terminal", {
    ...(targetTools ? { targetTools } : {}),
    ...(claudeModels ? { claudeModels } : {}),
  });
}

/** Quick time ranges shared by the usage-stats and request-logs tabs. */
export type UsageRangeKey = "today" | "yesterday" | "7d" | "15d" | "30d" | "all";

export const USAGE_RANGE_KEYS: readonly UsageRangeKey[] = [
  "today",
  "yesterday",
  "7d",
  "15d",
  "30d",
  "all",
];

export interface UsageMetrics {
  request_count: number;
  input_tokens: number;
  cache_read_tokens: number;
  cache_write_tokens: number;
  output_tokens: number;
  total_tokens: number;
  /** Sum of already-priced record amounts in scope. */
  amount: number;
  /** Records with no matching price row in scope. */
  unpriced_count: number;
  /** Backend-owned cache hit numerator in scope. */
  cache_hit_tokens: number;
  /** Backend-owned cache rate denominator in scope. */
  cache_eligible_tokens: number;
  /** Backend-owned token cache hit rate in `0..=100`; `null` when there is no valid positive denominator. */
  cache_hit_rate_percent: number | null;
  /** Successful requests with a valid positive cache denominator in scope. */
  cache_rate_eligible_count: number;
  /** Successful terminal requests in scope (coverage denominator). */
  successful_request_count: number;
}

export interface UsageBucket extends UsageMetrics {
  label: string;
}

export interface UsageProviderBreakdown extends UsageMetrics {
  provider_id: string;
  provider_name: string;
}

export interface UsageProviderModelBreakdown extends UsageMetrics {
  local_model: string;
}

export interface UsageProviderStatsRow extends UsageMetrics {
  provider_id: string;
  provider_name: string;
  models: UsageProviderModelBreakdown[];
}

export interface UsageModelBreakdown extends UsageMetrics {
  local_model: string;
  providers: UsageProviderBreakdown[];
}

export interface UnpricedUsageItem {
  provider_id: string;
  provider_name: string;
  local_model: string;
  upstream_model: string;
  count: number;
}

export interface UsageStats extends UsageMetrics {
  granularity: "hour" | "day";
  buckets: UsageBucket[];
  models: UsageModelBreakdown[];
  providers?: UsageProviderStatsRow[];
  unpriced_items?: UnpricedUsageItem[];
}

export type UsageLogResult = "success" | "failure";

export interface UsageCostBreakdown {
  input_price: number;
  output_price: number;
  cache_read_price: number;
  cache_write_price: number;
  input_cost: number;
  output_cost: number;
  cache_read_cost: number;
  cache_write_cost: number;
}

export interface UsageLogRecord {
  timestamp_ms: number;
  local_model: string;
  upstream_model: string;
  provider_id: string;
  provider_name: string;
  result: UsageLogResult;
  status: number;
  input_tokens: number;
  cache_read_tokens: number;
  cache_write_tokens: number;
  output_tokens: number;
  total_tokens: number;
  amount: number | null;
  /** Record-time pricing details; absent for legacy or unpriced rows. */
  cost_breakdown?: UsageCostBreakdown | null;
  duration_ms: number;
  /**
   * Per-attempt milliseconds from the attempt's monotonic start to its first
   * meaningful upstream output. Absent or `null` when unmeasured (non-streaming,
   * no output or a pre-output failure); a measured `0` is a valid observation.
   */
  first_token_ms?: number | null;
  /**
   * Gateway-observed average output tokens per second over the attempt's
   * generation window. Absent or `null` when unavailable; the frontend renders
   * the stored value directly and never reconstructs it from displayed times.
   */
  avg_tps?: number | null;
  /** Sanitized upstream error text of a failed attempt; `null`/absent when none was recorded. */
  error_message?: string | null;
  /**
   * Whether this row is the terminal row of its request. Absent means terminal,
   * mirroring the backend serde default; only an explicit `false` marks a
   * non-terminal attempt row.
   */
  terminal?: boolean;
  /** Reasoning effort level (e.g. "low", "medium", "high") requested by client; null/absent when none was specified. */
  reasoning_effort?: string | null;
  /**
   * Pool-only resolved preference (`low`..`none` or `auto`). Absent/`null` for
   * ordinary or historical rows; never inferred from the requested value.
   */
  resolved_reasoning_effort?: string | null;
  /** Pool-only allowlisted native control snapshot; absent/`null` when unavailable. */
  native_reasoning?: ModelPoolNativeReasoning | null;
}

/**
 * The allowlisted native control evidence recorded for one pool attempt. It
 * carries only the target protocol, the resolved mode and the small set of
 * native parameters actually sent; never a request/response body or credential.
 */
export interface ModelPoolNativeReasoning {
  protocol: string;
  mode: string;
  parameters?: Record<string, unknown> | null;
}

export interface UsageLogGroup {
  group: string;
  request_count: number;
  /** Failure records only. */
  error_count: number;
  last_request_at_ms: number;
}

export type UsageGroupBy = "none" | "model" | "provider" | "day";

export interface UsageLogsPage {
  page: number;
  page_size: number;
  total: number;
  total_pages: number;
  group_by: string | null;
  records: UsageLogRecord[];
  groups: UsageLogGroup[];
  /**
   * Distinct non-empty in-range local models, independent of page and model
   * filter. Optional for responses/fixtures that predate the facet.
   */
  models?: string[];
  /**
   * Distinct non-empty in-range provider names, independent of page and
   * provider filter. Optional for responses/fixtures that predate the facet.
   */
  providers?: string[];
}

export interface UsageLogsQuery {
  range: UsageRangeKey;
  groupBy?: UsageGroupBy | null;
  status?: UsageLogResult | null;
  model?: string | null;
  provider?: string | null;
  page?: number;
  /**
   * Optional exact provider-ID focus (Step 2). It is only sent when it holds a
   * non-null value, so callers that omit it keep the exact existing six-key
   * invoke payload.
   */
  providerId?: string | null;
  /**
   * Optional strict `YYYY-MM-DD` UTC+8 day focus (Step 2). Like `providerId`,
   * the key is only added when set.
   */
  day?: string | null;
}

/**
 * One selectable provider identity for the history selector, keyed by exact
 * nonblank provider ID. `deleted` marks a historical identity absent from the
 * current configuration; `has_usage` is false for a configured-but-unused
 * provider that must not be given a fabricated first-use date.
 */
export interface ProviderUsageIdentity {
  provider_id: string;
  provider_name: string;
  deleted: boolean;
  has_usage: boolean;
}

/**
 * Coverage classification of one UTC+8 calendar day in a provider history:
 * `retainedHistorical` may be incomplete, `continuous` makes a no-request day a
 * genuine zero, and `unavailable` has no metrics at all.
 */
export type ProviderDayCoverage =
  | "retained_historical"
  | "continuous"
  | "unavailable";

/**
 * One calendar day of a provider history; `metrics` is `null` only when the
 * date is unavailable (never a fabricated zero).
 */
export interface ProviderDailyEntry {
  day: string;
  coverage: ProviderDayCoverage;
  ongoing: boolean;
  metrics: UsageMetrics | null;
}

/**
 * A provider's complete UTC+8 daily calendar, coverage and cumulative values
 * through the current reporting day, as returned by the backend.
 */
export interface ProviderUsageHistory {
  provider_id: string;
  earliest_available_day: string | null;
  continuity_start_day: string | null;
  end_day: string | null;
  cumulative: UsageMetrics | null;
  days: ProviderDailyEntry[];
}

export interface OffPeakPrice {
  /** "HH:mm" 24-hour string in UTC+8, e.g. "00:30" */
  start_time: string;
  /** "HH:mm" 24-hour string in UTC+8, e.g. "08:30" */
  end_time: string;
  /** USD per million tokens in off-peak hours */
  input: number;
  cache_read: number;
  cache_write: number;
  output: number;
  /** UTC+8 weekdays the window applies to (`0` Sunday–`6` Saturday); absent/empty means every day. */
  days?: number[] | null;
}

export interface ModelPrice {
  provider_id?: string | null;
  upstream_model: string;
  /** USD per million tokens. */
  input: number;
  cache_read: number;
  cache_write: number;
  output: number;
  off_peaks?: OffPeakPrice[];
}

/** Editable string-form off-peak window for one mapping-row price draft. */
export type GatewayPriceDraftOffPeak = {
  id: string;
  start_time: string;
  end_time: string;
  input: string;
  cache_read: string;
  cache_write: string;
  output: string;
  days: number[];
};

/** Editable string-form price for one upstream model. */
export type GatewayPriceDraft = {
  id: string;
  upstream_model: string;
  input: string;
  cache_read: string;
  cache_write: string;
  output: string;
  enable_off_peak: boolean;
  off_peaks: GatewayPriceDraftOffPeak[];
};

/** Distinct non-blank mapping upstream models in first-seen order. */
export function mappedUpstreamModels(provider: GatewayUpstreamProvider): string[] {
  const models: string[] = [];
  for (const mapping of provider.mappings) {
    const upstream = mapping.upstream_model.trim();
    if (upstream !== "" && !models.includes(upstream)) models.push(upstream);
  }
  return models;
}

/** First price row owned by exactly this provider and upstream model; a global row never matches. */
export function resolveProviderPriceRow(
  prices: ModelPrice[] | null | undefined,
  providerId: string,
  upstreamModel: string,
): ModelPrice | undefined {
  return (prices ?? []).find(
    (price) =>
      price.provider_id === providerId && price.upstream_model === upstreamModel,
  );
}

/** Parse a price input, treating blank and non-finite values as 0. */
export function parsePriceNumber(value: string): number {
  const parsed = Number(value.trim());
  return Number.isFinite(parsed) ? parsed : 0;
}

/** Keep only UTC+8 weekdays (0–6), deduplicated and ascending. */
export function normalizeDraftDays(days: number[] | null | undefined): number[] {
  return Array.from(
    new Set((days ?? []).filter((day) => day >= 0 && day <= 6)),
  ).sort((a, b) => a - b);
}

/** Seed one price draft from a stored row, or a blank draft when none exists. */
export function priceRowToDraft(
  price: ModelPrice | null | undefined,
  upstreamModel: string,
  idPrefix: string,
): GatewayPriceDraft {
  if (price == null) {
    return {
      id: idPrefix,
      upstream_model: upstreamModel,
      input: "",
      cache_read: "",
      cache_write: "",
      output: "",
      enable_off_peak: false,
      off_peaks: [],
    };
  }

  const rawOffPeaks = price.off_peaks ?? [];

  const stringifyTier = (value: number | null | undefined): string =>
    value !== undefined && value !== null ? String(value) : "";

  return {
    id: idPrefix,
    upstream_model: price.upstream_model,
    input: String(price.input),
    cache_read: String(price.cache_read),
    cache_write: String(price.cache_write),
    output: String(price.output),
    enable_off_peak: rawOffPeaks.length > 0,
    off_peaks: rawOffPeaks.map((op, index) => ({
      id: `${idPrefix}-op-${index}`,
      start_time: op.start_time ?? "00:30",
      end_time: op.end_time ?? "08:30",
      input: stringifyTier(op.input),
      cache_read: stringifyTier(op.cache_read),
      cache_write: stringifyTier(op.cache_write),
      output: stringifyTier(op.output),
      days: op.days ?? [],
    })),
  };
}

/** A draft is priced when at least one standard tier holds a non-blank value. */
export function isPriceDraftPriced(draft: GatewayPriceDraft): boolean {
  return [draft.input, draft.cache_read, draft.cache_write, draft.output].some(
    (value) => value.trim() !== "",
  );
}

/** Build the stored row for a priced draft; blank drafts or blank models produce null. */
export function draftToPriceRow(draft: GatewayPriceDraft): ModelPrice | null {
  if (!isPriceDraftPriced(draft)) return null;
  const upstreamModel = draft.upstream_model.trim();
  if (upstreamModel === "") return null;

  const row: ModelPrice = {
    upstream_model: upstreamModel,
    input: parsePriceNumber(draft.input),
    cache_read: parsePriceNumber(draft.cache_read),
    cache_write: parsePriceNumber(draft.cache_write),
    output: parsePriceNumber(draft.output),
  };

  if (draft.enable_off_peak && draft.off_peaks.length > 0) {
    const offPeaks: OffPeakPrice[] = draft.off_peaks.map((op) => {
      const days = normalizeDraftDays(op.days);
      return {
        start_time: op.start_time.trim() || "00:30",
        end_time: op.end_time.trim() || "08:30",
        input: parsePriceNumber(op.input || draft.input),
        cache_read: parsePriceNumber(op.cache_read || draft.cache_read),
        cache_write: parsePriceNumber(op.cache_write || draft.cache_write),
        output: parsePriceNumber(op.output || draft.output),
        ...(days.length > 0 ? { days } : {}),
      };
    });
    row.off_peaks = offPeaks;
  }

  return row;
}

const UTC8_OFFSET_MS = 8 * 60 * 60 * 1000;

function pad2(value: number): string {
  return String(value).padStart(2, "0");
}

/** Format a millisecond timestamp as `YYYY-MM-DD HH:mm:ss` in UTC+8. */
export function formatUtc8DateTime(ms: number | null | undefined): string | null {
  if (ms === null || ms === undefined) return null;
  const date = new Date(ms + UTC8_OFFSET_MS);
  return `${date.getUTCFullYear()}-${pad2(date.getUTCMonth() + 1)}-${pad2(
    date.getUTCDate(),
  )} ${pad2(date.getUTCHours())}:${pad2(date.getUTCMinutes())}:${pad2(date.getUTCSeconds())}`;
}

/** Format a millisecond timestamp as `YYYY-MM-DD` in UTC+8. */
export function formatUtc8Day(ms: number | null | undefined): string | null {
  if (ms === null || ms === undefined) return null;
  const date = new Date(ms + UTC8_OFFSET_MS);
  return `${date.getUTCFullYear()}-${pad2(date.getUTCMonth() + 1)}-${pad2(
    date.getUTCDate(),
  )}`;
}

/** Format a millisecond timestamp as `HH:00` in UTC+8. */
export function formatUtc8Hour(ms: number | null | undefined): string | null {
  if (ms === null || ms === undefined) return null;
  const date = new Date(ms + UTC8_OFFSET_MS);
  return `${pad2(date.getUTCHours())}:00`;
}

/** Format an amount with 4 decimals; missing amounts render as `—`. */
export function formatUsageAmount(amount: number | null | undefined): string {
  if (amount === null || amount === undefined || Number.isNaN(amount)) return "—";
  return amount.toFixed(4);
}

/** Format an amount with 4 decimals and USD currency symbol ($); missing amounts render as `—`. */
export function formatUsageCurrency(amount: number | null | undefined): string {
  const formatted = formatUsageAmount(amount);
  return formatted === "—" ? "—" : `$${formatted}`;
}

/**
 * Format a per-attempt latency in milliseconds as value-only seconds with
 * three decimals (e.g. `5.000`); the unit is rendered separately by the caller
 * so it can stay muted and independent of the value color.
 * - Returns "—" for null, undefined, non-finite or negative values.
 */
export function formatGatewayDuration(ms: number | null | undefined): string {
  if (ms === null || ms === undefined || !Number.isFinite(ms) || ms < 0) {
    return "—";
  }
  return (ms / 1000).toFixed(3);
}

/**
 * Returns Tailwind text color classes for request duration based on latency.
 * Each band carries a readable light-theme shade plus a matching dark: variant.
 * - < 3s (< 3000ms): green (text-emerald-600 dark:text-emerald-400)
 * - 3s ~ 15s (3000ms ~ 15000ms): amber/yellow (text-amber-600 dark:text-amber-400)
 * - > 15s (> 15000ms): red/rose (text-rose-600 dark:text-rose-400)
 * - null/undefined/negative: muted foreground (text-muted-foreground)
 */
export function getGatewayDurationColorClass(ms: number | null | undefined): string {
  if (ms === null || ms === undefined || !Number.isFinite(ms) || ms < 0) {
    return "text-muted-foreground";
  }
  if (ms < 3000) {
    return "text-emerald-600 dark:text-emerald-400";
  }
  if (ms <= 15000) {
    return "text-amber-600 dark:text-amber-400";
  }
  return "text-rose-600 dark:text-rose-400";
}

/**
 * Format an average output rate in tokens per second as value-only two-decimal
 * text (e.g. `50.00`); the `t/s` unit is rendered separately by the caller.
 * - Returns "—" for null, undefined, non-finite or negative values, so an
 *   explicit `0` stays a measured value distinct from unavailable.
 */
export function formatGatewayTps(tps: number | null | undefined): string {
  if (tps === null || tps === undefined || !Number.isFinite(tps) || tps < 0) {
    return "—";
  }
  return tps.toFixed(2);
}

/**
 * Returns Tailwind text color classes for first-token latency, classified on
 * the raw millisecond value (never the display-rounded seconds).
 * - < 1000ms: green (text-emerald-600 dark:text-emerald-400)
 * - 1000ms ~ 3000ms inclusive: amber (text-amber-600 dark:text-amber-400)
 * - > 3000ms: rose (text-rose-600 dark:text-rose-400)
 * - null/undefined/non-finite/negative: muted foreground
 */
export function getGatewayFirstTokenColorClass(ms: number | null | undefined): string {
  if (ms === null || ms === undefined || !Number.isFinite(ms) || ms < 0) {
    return "text-muted-foreground";
  }
  if (ms < 1000) {
    return "text-emerald-600 dark:text-emerald-400";
  }
  if (ms <= 3000) {
    return "text-amber-600 dark:text-amber-400";
  }
  return "text-rose-600 dark:text-rose-400";
}

/**
 * Returns Tailwind text color classes for average TPS, classified on the raw
 * value (never the display-rounded text).
 * - >= 60: green (text-emerald-600 dark:text-emerald-400)
 * - 20 ~ < 60: amber (text-amber-600 dark:text-amber-400)
 * - < 20: rose (text-rose-600 dark:text-rose-400)
 * - null/undefined/non-finite/negative: muted foreground
 */
export function getGatewayTpsColorClass(tps: number | null | undefined): string {
  if (tps === null || tps === undefined || !Number.isFinite(tps) || tps < 0) {
    return "text-muted-foreground";
  }
  if (tps >= 60) {
    return "text-emerald-600 dark:text-emerald-400";
  }
  if (tps >= 20) {
    return "text-amber-600 dark:text-amber-400";
  }
  return "text-rose-600 dark:text-rose-400";
}

/**
 * Format a token count using compact Chinese units (万, 百万, 千万, 亿),
 * matching the formatting used in AI Usage Stats.
 * Numbers below 10,000 are formatted with thousand separators.
 * Null, undefined, NaN, or non-finite values default to "0".
 */
export function formatGatewayTokens(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) {
    return "0";
  }
  const absValue = Math.abs(value);
  const units = [
    { threshold: 100_000_000, suffix: "亿" },
    { threshold: 10_000_000, suffix: "千万" },
    { threshold: 1_000_000, suffix: "百万" },
    { threshold: 10_000, suffix: "万" },
  ];
  const unit = units.find((item) => absValue >= item.threshold);
  if (!unit) {
    return new Intl.NumberFormat().format(value);
  }

  const formatted = new Intl.NumberFormat(undefined, {
    maximumFractionDigits: 1,
  }).format(value / unit.threshold);
  return `${formatted}${unit.suffix}`;
}

/**
 * Formats cache token counts into "xxK" format (e.g. 10100 -> "10.1K", 10000 -> "10K", 500 -> "0.5K", 0 -> "0K").
 * Null, undefined, NaN, or non-finite values default to "0K".
 */
export function formatCacheTokensK(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value) || value <= 0) {
    return "0K";
  }
  const inK = value / 1000;
  const formatted = new Intl.NumberFormat("en-US", {
    minimumFractionDigits: 0,
    maximumFractionDigits: 1,
  }).format(inK);
  return `${formatted}K`;
}

/**
 * A row is "unpriced" only when every request in scope lacks a price row.
 * A partially priced row still shows its priced amount.
 */
export function isUnpricedOnly(
  metrics: Pick<UsageMetrics, "request_count" | "unpriced_count">,
): boolean {
  return (
    metrics.request_count > 0 &&
    metrics.unpriced_count > 0 &&
    metrics.unpriced_count === metrics.request_count
  );
}

/** Amount cell for a usage-analysis row (`—` when fully unpriced). */
export function formatUsageRowAmount(metrics: UsageMetrics): string {
  if (isUnpricedOnly(metrics)) return "—";
  return formatUsageAmount(metrics.amount);
}

/** Display label for a grouped request-log row. */
export function formatUsageGroupLabel(
  _groupBy: UsageGroupBy,
  group: string,
): string {
  return group && group.trim() !== "" ? group : "—";
}

/** Clamp a 1-based page number into the valid range for the given page count. */
export function clampUsagePage(page: number, totalPages: number): number {
  if (!Number.isFinite(page)) return 1;
  if (!Number.isFinite(totalPages) || totalPages < 1) return 1;
  const floored = Math.floor(page);
  if (floored < 1) return 1;
  return Math.min(floored, Math.floor(totalPages));
}

/** Stable i18n key for a request-log status result. */
export function usageStatusTranslationKey(result: UsageLogResult): string {
  switch (result) {
    case "success":
      return "aiGatewayStatusSuccess";
    case "failure":
      return "aiGatewayStatusFailure";
  }
}

type InFlightRead<T> = { promise: Promise<T> };

const usageStatsInFlight = new Map<string, InFlightRead<UsageStats>>();
const requestLogsInFlight = new Map<string, InFlightRead<UsageLogsPage>>();

/**
 * Join an equivalent in-flight read for the same complete parameter set.
 * Settled results are never cached: the entry is removed as soon as this exact
 * source promise settles, and failure releases the slot for a later retry.
 */
function shareInFlight<T>(
  inFlight: Map<string, InFlightRead<T>>,
  key: string,
  read: () => Promise<T>,
): Promise<T> {
  const existing = inFlight.get(key);
  if (existing) return existing.promise;
  const entry = {} as InFlightRead<T>;
  entry.promise = read().then(
    (value) => {
      if (inFlight.get(key) === entry) inFlight.delete(key);
      return value;
    },
    (error: unknown) => {
      if (inFlight.get(key) === entry) inFlight.delete(key);
      throw error;
    },
  );
  inFlight.set(key, entry);
  return entry.promise;
}

type GatewayFreshReadOptions = { fresh?: boolean };

/** Card totals, time buckets and model/provider breakdown for a quick range. */
export function aiGatewayUsageStats(
  range: UsageRangeKey,
  options: GatewayFreshReadOptions = {},
) {
  const read = () => invoke<UsageStats>("ai_gateway_usage_stats", { range });
  if (options.fresh) return read();
  return shareInFlight(usageStatsInFlight, JSON.stringify([range]), read);
}

/** Paged request logs, optionally grouped by model, provider or UTC+8 day. */
export function aiGatewayRequestLogs(
  query: UsageLogsQuery,
  options: GatewayFreshReadOptions = {},
) {
  const payload = {
    range: query.range,
    groupBy: query.groupBy ?? null,
    status: query.status ?? null,
    model: query.model ?? null,
    provider: query.provider ?? null,
    page: query.page ?? 1,
    // Add the exact focus selectors only when set, so ordinary callers keep the
    // byte-identical six-key payload they had before this feature.
    ...(query.providerId !== undefined && query.providerId !== null
      ? { providerId: query.providerId }
      : {}),
    ...(query.day !== undefined && query.day !== null ? { day: query.day } : {}),
  };
  const read = () =>
    invoke<UsageLogsPage>("ai_gateway_request_logs", payload);
  if (options.fresh) return read();
  const key = JSON.stringify([
    query.range,
    query.groupBy ?? null,
    query.status ?? null,
    query.model ?? null,
    query.provider ?? null,
    query.page ?? 1,
    query.providerId ?? null,
    query.day ?? null,
  ]);
  return shareInFlight(requestLogsInFlight, key, read);
}

/**
 * The union of configured and historical provider identities for the history
 * selector. It takes no payload.
 */
export function aiGatewayUsageProviders() {
  return invoke<ProviderUsageIdentity[]>("ai_gateway_usage_providers");
}

/** One provider's complete UTC+8 daily calendar, coverage and cumulative stats. */
export function aiGatewayProviderUsageHistory(query: { providerId: string }) {
  return invoke<ProviderUsageHistory>("ai_gateway_provider_usage_history", {
    providerId: query.providerId,
  });
}

export function aiGatewayUsageRetentionGet() {
  return invoke<number>("ai_gateway_usage_retention_get");
}

export function aiGatewayUsageRetentionSave(days: number) {
  return invoke<number>("ai_gateway_usage_retention_save", { days });
}

/** Template auto-refresh interval in minutes; `0` disables the refresh loop. */
export function aiGatewayTemplateAutoRefreshGet() {
  return invoke<number>("ai_gateway_template_auto_refresh_get");
}

export function aiGatewayTemplateAutoRefreshSave(minutes: number) {
  return invoke<number>("ai_gateway_template_auto_refresh_save", { minutes });
}

// Subscribers notified after the template auto-refresh interval is persisted so
// long-lived views can re-read the value instead of polling or prop drilling.
const templateAutoRefreshIntervalListeners = new Set<() => void>();

export function notifyTemplateAutoRefreshIntervalChanged(): void {
  for (const listener of Array.from(templateAutoRefreshIntervalListeners)) {
    listener();
  }
}

export function subscribeTemplateAutoRefreshIntervalChanged(
  listener: () => void,
): () => void {
  templateAutoRefreshIntervalListeners.add(listener);
  return () => {
    templateAutoRefreshIntervalListeners.delete(listener);
  };
}

/** One template whose latest backend automatic refresh failed, with its reason. */
export interface TemplateAutoRefreshFailure {
  template_id: string;
  reason: string;
}

/** Backend-owned automatic-refresh failure snapshot. */
export interface TemplateAutoRefreshStatus {
  failures: TemplateAutoRefreshFailure[];
}

/**
 * Tauri event literal emitted after the backend automatic-refresh failure set
 * changes; its payload is a {@link TemplateAutoRefreshStatus}.
 */
export const AI_GATEWAY_TEMPLATE_AUTO_REFRESH_UPDATED_EVENT =
  "ai-gateway-template-auto-refresh-updated";

/**
 * Read the backend automatic-refresh failure snapshot without starting any
 * schedule; the process scheduler owns all template timing and syncing.
 */
export function aiGatewayTemplateAutoRefreshStatus() {
  return invoke<TemplateAutoRefreshStatus>(
    "ai_gateway_template_auto_refresh_status",
  );
}

// ---------------------------------------------------------------------------
// Model pools
// ---------------------------------------------------------------------------

/**
 * The public canonical reasoning preferences a pool client can request, in
 * canonical order. `none` is an explicit disable and `auto` is never a request.
 */
export const CANONICAL_REASONING_EFFORTS = [
  "low",
  "medium",
  "high",
  "xhigh",
  "max",
  "none",
] as const;

export type CanonicalReasoningEffort = (typeof CANONICAL_REASONING_EFFORTS)[number];

/** The bounded native reasoning profile identifiers a pool candidate declares. */
export type ModelPoolReasoningProfile =
  | "openai_effort"
  | "anthropic_effort_inherit"
  | "anthropic_effort_adaptive"
  | "anthropic_manual_budget"
  | "chat_thinking_switch";

const MODEL_POOL_REASONING_PROFILES: ReadonlySet<string> = new Set([
  "openai_effort",
  "anthropic_effort_inherit",
  "anthropic_effort_adaptive",
  "anthropic_manual_budget",
  "chat_thinking_switch",
]);

/** Profiles whose positive action is a native enum value rather than a budget. */
const MODEL_POOL_ENUM_REASONING_PROFILES: ReadonlySet<string> = new Set([
  "openai_effort",
  "anthropic_effort_inherit",
  "anthropic_effort_adaptive",
  "chat_thinking_switch",
]);

/** Internal minimum budget an Anthropic manual-budget mapping may declare. */
export const MODEL_POOL_MIN_REASONING_BUDGET = 1024;

/** Whether a stored profile identifier is one of the bounded supported shapes. */
export function isKnownModelPoolReasoningProfile(
  profile: string | null | undefined,
): profile is ModelPoolReasoningProfile {
  return typeof profile === "string" && MODEL_POOL_REASONING_PROFILES.has(profile);
}

/**
 * Whether a declared profile may target one stored effective protocol. The
 * bounded profiles are protocol-family specific, so a profile on the wrong
 * ingress family never contributes a mapping.
 */
export function isModelPoolReasoningProfileSupportedOnProtocol(
  profile: string | null | undefined,
  protocol: GatewayUpstreamProtocol,
): boolean {
  switch (profile) {
    case "openai_effort":
      return protocol === "chat_completions" || protocol === "responses";
    case "chat_thinking_switch":
      return protocol === "chat_completions";
    case "anthropic_effort_inherit":
    case "anthropic_effort_adaptive":
    case "anthropic_manual_budget":
      return protocol === "anthropic_messages";
    default:
      return false;
  }
}

/** An inclusive integer budget range declared by an Anthropic manual-budget candidate. */
export interface ModelPoolReasoningBudget {
  min?: number | null;
  max?: number | null;
}

/** A typed native action a public preference maps to; shape depends on `kind`. */
export interface ModelPoolReasoningAction {
  kind: string;
  value?: string;
  budget_tokens?: number;
}

/** One public-preference -> typed-native-action mapping owned by a pool candidate. */
export interface ModelPoolReasoningMapping {
  effort: string;
  action: ModelPoolReasoningAction;
}

/**
 * A pool candidate's declared native reasoning capability. Every field is
 * optional so legacy pools and old payloads stay assignable; a missing
 * declaration stays unknown and no mapping is inferred from source metadata.
 */
export interface GatewayModelPoolReasoning {
  profile?: string | null;
  values?: string[] | null;
  budget?: ModelPoolReasoningBudget | null;
  supports_disable?: boolean;
  mappings?: ModelPoolReasoningMapping[] | null;
}

/**
 * One source candidate of a model pool: an exact reference to a real provider
 * mapping. It never carries a source key or endpoint — the referenced mapping
 * remains authoritative for endpoint, credential and health. The optional
 * pool-local `reasoning` declaration is the only place new policy lives.
 */
export interface GatewayModelPoolCandidate {
  provider_id: string;
  local_model: string;
  upstream_model: string;
  protocol: GatewayUpstreamProtocol;
  /** Integer 1-100; default 1. */
  weight: number;
  /** Pool-local native reasoning declaration; absent/`null` stays unknown. */
  reasoning?: GatewayModelPoolReasoning | null;
}

/**
 * An operator-defined pool exposed through one public model id. The default
 * display name is the user's `Free Modle` and the default id is `free-model`.
 * `default_reasoning_effort` is one of the five canonical tokens or `auto`
 * (absent/`null` reads as `auto`); it never lives on the source provider.
 */
export interface GatewayModelPool {
  id: string;
  name: string;
  model: string;
  enabled: boolean;
  default_reasoning_effort?: string | null;
  /** Configured unified public context window limit in tokens. Overrides conservative candidate minimum when set. */
  context_window?: number | null;
  candidates: GatewayModelPoolCandidate[];
  /**
   * Ordered gateway local model ids used after every regular candidate is
   * exhausted. Each id expands to every provider mapping serving that model, so
   * the list stores no provider, upstream model, protocol or weight. An
   * absent/empty list means no configured fallback.
   */
  fallback_models?: string[];
}

/** Nonsecret source-availability code reported per candidate by the backend. */
export type GatewayModelPoolAvailability =
  | "ready"
  | "cooling"
  | "source_disabled"
  | "mapping_unavailable"
  | "missing_provider"
  | "no_usable_key"
  | "unsupported_protocol";

/** Nonsecret per-candidate status; never carries a key value or endpoint. */
export interface GatewayModelPoolCandidateStatus extends GatewayModelPoolCandidate {
  provider_name: string | null;
  ready: boolean;
  availability: GatewayModelPoolAvailability | string;
  reason: string | null;
  /** Wall-clock epoch SECONDS when cooldown ends; `null` when not cooling. */
  cooling_until: number | null;
}

/** Nonsecret readiness of one provider mapping serving a fallback gateway model. */
export interface GatewayModelPoolFallbackSourceStatus {
  provider_id: string;
  provider_name: string | null;
  local_model: string;
  upstream_model: string;
  protocol: GatewayUpstreamProtocol;
  ready: boolean;
  availability: GatewayModelPoolAvailability | string;
  reason: string | null;
  /** Wall-clock epoch SECONDS when cooldown ends; `null` when not cooling. */
  cooling_until: number | null;
}

/** Nonsecret readiness of one stored fallback gateway model and its sources. */
export interface GatewayModelPoolFallbackModelStatus {
  /** The trimmed gateway local model id stored in the ordered list. */
  model: string;
  ready: boolean;
  ready_sources: number;
  total_sources: number;
  sources: GatewayModelPoolFallbackSourceStatus[];
  /** Bounded nonsecret explanation when no expanded source is ready. */
  reason?: string | null;
}

/** One public preference's static pool coverage, independent of readiness. */
export interface ModelPoolReasoningCoverage {
  effort: string;
  mapped_candidates: number;
  total_candidates: number;
}

/** Nonsecret pool status snapshot derived from current source config. */
export interface GatewayModelPoolStatus {
  id: string;
  name: string;
  model: string;
  enabled: boolean;
  /** Pool default preference; absent/`null` reads as `auto`. */
  default_reasoning_effort?: string | null;
  /** Canonical preferences currently supported by valid pool mappings. */
  reasoning_efforts?: string[];
  /** Per-preference static coverage; absent for legacy pools. */
  reasoning_coverage?: ModelPoolReasoningCoverage[];
  /** Regular candidate readiness; always regular-only. */
  candidates: GatewayModelPoolCandidateStatus[];
  /**
   * Ordered per-model readiness of the stored fallback list. Missing on legacy
   * payloads and empty when the pool configures no fallback model.
   */
  fallback_models?: GatewayModelPoolFallbackModelStatus[];
}

/**
 * Whether one candidate-local mapping is structurally valid for its declared
 * profile. Unknown profiles and unsupported action shapes never produce a
 * mapping, so no support is inferred from source metadata.
 */
export function isModelPoolReasoningMappingValid(
  reasoning: GatewayModelPoolReasoning | null | undefined,
  mapping: ModelPoolReasoningMapping,
): boolean {
  const profile = reasoning?.profile;
  if (!isKnownModelPoolReasoningProfile(profile)) return false;
  const effort = mapping.effort;
  if (!(CANONICAL_REASONING_EFFORTS as readonly string[]).includes(effort)) {
    return false;
  }
  const action = mapping.action;
  if (!action || typeof action.kind !== "string") return false;

  if (action.kind === "disable") {
    return effort === "none" && reasoning?.supports_disable === true;
  }
  if (effort === "none") return false;

  if (action.kind === "enum") {
    // Only an enum-capable profile may carry a positive native value; a
    // manual-budget candidate never does.
    if (!MODEL_POOL_ENUM_REASONING_PROFILES.has(profile)) return false;
    if (typeof action.value !== "string") return false;
    const trimmed = action.value.trim();
    if (trimmed === "") return false;
    // A positive preference must never target the reserved disable/omission
    // value.
    if (trimmed.toLowerCase() === "none") return false;
    // A switch profile selects one intensity: its positive value is `enabled`.
    if (profile === "chat_thinking_switch") return trimmed === "enabled";
    // An effort-profile value must stay inside the candidate's declared set.
    return (reasoning?.values ?? []).includes(action.value);
  }
  if (action.kind === "budget") {
    if (profile !== "anthropic_manual_budget") return false;
    const min = reasoning?.budget?.min;
    const max = reasoning?.budget?.max;
    if (typeof min !== "number" || typeof max !== "number") return false;
    if (!Number.isInteger(min) || !Number.isInteger(max)) return false;
    if (min < MODEL_POOL_MIN_REASONING_BUDGET || max < min) return false;
    const tokens = action.budget_tokens;
    if (typeof tokens !== "number" || !Number.isInteger(tokens)) return false;
    return tokens >= min && tokens <= max;
  }
  return false;
}

/** The concrete pool default preference, mapping a missing value to `auto`. */
export function resolveModelPoolDefaultEffort(
  pool: Pick<GatewayModelPool, "default_reasoning_effort"> | null | undefined,
): string {
  const value = pool?.default_reasoning_effort;
  return typeof value === "string" && value.trim() !== "" ? value : "auto";
}

/**
 * Whether one candidate's exact source reference currently resolves against the
 * given providers: the provider id, the original trimmed mapping pair and the
 * mapping's effective protocol. Availability and key state never enter.
 */
export function modelPoolCandidateResolves(
  providers: GatewayUpstreamProvider[] | null | undefined,
  candidate: Pick<
    GatewayModelPoolCandidate,
    "provider_id" | "local_model" | "upstream_model" | "protocol"
  >,
): boolean {
  return (providers ?? []).some(
    (provider) =>
      provider.id === candidate.provider_id &&
      (provider.mappings ?? []).some(
        (mapping) =>
          mapping.local_model.trim() === candidate.local_model &&
          mapping.upstream_model.trim() === candidate.upstream_model &&
          (mapping.protocol ?? provider.protocol ?? "chat_completions") ===
            candidate.protocol,
      ),
  );
}

/**
 * Whether one candidate statically serves `effort`: its exact source reference
 * resolves, its profile is representable on its effective protocol and it
 * declares a structurally valid mapping for the preference. Runtime readiness
 * and key state are deliberately excluded.
 */
export function modelPoolCandidateServesEffort(
  providers: GatewayUpstreamProvider[] | null | undefined,
  candidate: GatewayModelPoolCandidate,
  effort: string,
): boolean {
  const reasoning = candidate.reasoning;
  if (!reasoning) return false;
  const mapping = (reasoning.mappings ?? []).find(
    (item) => item.effort === effort,
  );
  if (!mapping) return false;
  if (!modelPoolCandidateResolves(providers, candidate)) return false;
  if (
    !isModelPoolReasoningProfileSupportedOnProtocol(
      reasoning.profile,
      candidate.protocol,
    )
  ) {
    return false;
  }
  return isModelPoolReasoningMappingValid(reasoning, mapping);
}

/** Map one source-advertised native token to a canonical pool preference. */
function fallbackCanonicalEffort(raw: string): CanonicalReasoningEffort | null {
  const normalized = raw.trim().toLowerCase();
  return (CANONICAL_REASONING_EFFORTS as readonly string[]).includes(normalized)
    ? (normalized as CanonicalReasoningEffort)
    : null;
}

/**
 * Derive one expanded fallback source's native reasoning declaration from its
 * exact provider mapping: the OpenAI effort profile for Chat/Responses and the
 * Anthropic effort-inherit profile for Messages. Only declared canonical
 * efforts become mappings, and `none` becomes an explicit disable only when the
 * mapping declares it. No control is inferred from a model name.
 */
export function fallbackModelReasoning(
  mapping: Pick<GatewayModelMapping, "reasoning_efforts"> | null | undefined,
  protocol: GatewayUpstreamProtocol,
): GatewayModelPoolReasoning | null {
  const profile =
    protocol === "anthropic_messages"
      ? "anthropic_effort_inherit"
      : "openai_effort";
  const efforts: CanonicalReasoningEffort[] = [];
  const values: string[] = [];
  for (const raw of mapping?.reasoning_efforts ?? []) {
    const trimmed = raw.trim();
    const effort = fallbackCanonicalEffort(trimmed);
    if (!effort || efforts.includes(effort)) continue;
    efforts.push(effort);
    if (effort !== "none") values.push(effort);
  }
  if (efforts.length === 0) return null;
  return {
    profile,
    values,
    supports_disable: efforts.includes("none"),
    mappings: efforts.map((effort) => ({
      effort,
      action:
        effort === "none"
          ? { kind: "disable" as const }
          : { kind: "enum" as const, value: effort },
    })),
  };
}

/**
 * Expand one stored fallback gateway model to every provider mapping that serves
 * its trimmed local model, preserving provider and mapping order. Disabled
 * providers and mappings stay included so status can report them; the runtime
 * eligibility contract is applied separately. Each source derives its reasoning
 * automatically from its exact mapping and effective protocol.
 */
export function expandModelPoolFallbackModel(
  providers: GatewayUpstreamProvider[] | null | undefined,
  localModel: string,
): GatewayModelPoolCandidate[] {
  const model = localModel.trim();
  if (!model) return [];
  const expanded: GatewayModelPoolCandidate[] = [];
  const seen = new Set<string>();
  for (const provider of providers ?? []) {
    const providerId = provider.id.trim();
    for (const mapping of provider.mappings ?? []) {
      if (mapping.local_model.trim() !== model) continue;
      const upstreamModel = mapping.upstream_model.trim();
      if (!upstreamModel) continue;
      const protocol =
        mapping.protocol ?? provider.protocol ?? "chat_completions";
      const candidate: GatewayModelPoolCandidate = {
        provider_id: providerId,
        local_model: model,
        upstream_model: upstreamModel,
        protocol,
        weight: 1,
      };
      const key = modelPoolCandidateKey(candidate);
      if (seen.has(key)) continue;
      seen.add(key);
      const reasoning = fallbackModelReasoning(mapping, protocol);
      expanded.push(reasoning ? { ...candidate, reasoning } : candidate);
    }
  }
  return expanded;
}

/** One pool's configuration-derived static reasoning catalog. */
export interface ModelPoolReasoningCatalog {
  reasoning_efforts: string[];
  default_reasoning_effort: string;
  reasoning_coverage: ModelPoolReasoningCoverage[];
}

/**
 * Derive one pool's static reasoning catalog from the current provider list,
 * mirroring the backend `pool_reasoning_catalog`: the supported canonical
 * preferences in canonical order, the pool default and the five canonical
 * coverage rows. Readiness, cooldown, key and source-enable state never move the
 * counts; a dangling or protocol-changed reference stops being advertised.
 */
export function poolReasoningCatalog(
  providers: GatewayUpstreamProvider[] | null | undefined,
  pool: Pick<
    GatewayModelPool,
    "candidates" | "default_reasoning_effort" | "fallback_models"
  >,
): ModelPoolReasoningCatalog {
  // The configured union: regular candidates plus every provider mapping that
  // serves an ordered fallback gateway model. Static support ignores transient
  // readiness for both roles.
  const candidates = [
    ...pool.candidates,
    ...(pool.fallback_models ?? []).flatMap((model) =>
      expandModelPoolFallbackModel(providers, model),
    ),
  ];
  const total = candidates.length;
  const reasoning_coverage = CANONICAL_REASONING_EFFORTS.map((effort) => ({
    effort,
    mapped_candidates: candidates.filter((candidate) =>
      modelPoolCandidateServesEffort(providers, candidate, effort),
    ).length,
    total_candidates: total,
  }));
  return {
    reasoning_efforts: reasoning_coverage
      .filter((row) => row.mapped_candidates > 0)
      .map((row) => row.effort),
    default_reasoning_effort: resolveModelPoolDefaultEffort(pool),
    reasoning_coverage,
  };
}

/**
 * The coverage row a pool capability block surfaces: the concrete default's row
 * when one is set, otherwise the first advertised labeled preference. It never
 * silently falls back to an unlabeled canonical `low` row while a labeled tier
 * is mapped, and only falls back to the first canonical row when nothing at all
 * is advertised (so the denominator stays visible).
 */
export function selectPoolReasoningCoverageRow(
  coverage: ModelPoolReasoningCoverage[] | null | undefined,
  defaultEffort: string,
  supportedEfforts: string[],
): ModelPoolReasoningCoverage | null {
  const rows = coverage ?? [];
  if (rows.length === 0) return null;
  if (defaultEffort && defaultEffort !== "auto") {
    const exact = rows.find((row) => row.effort === defaultEffort);
    if (exact) return exact;
  }
  const advertised = rows.find((row) => supportedEfforts.includes(row.effort));
  return advertised ?? rows[0] ?? null;
}

/**
 * Automatically maps a candidate model's native reasoning levels to the
 * pool's unified canonical reasoning efforts (low, medium, high, xhigh, max, none).
 * If the candidate model declares no native levels or none match the unified
 * levels, it returns hasMatch = false and an empty mappings dictionary.
 */
export function autoBindCandidateReasoning(
  candidate: Pick<
    GatewayModelPoolCandidate,
    "provider_id" | "local_model" | "upstream_model" | "protocol" | "reasoning"
  >,
  providers: GatewayUpstreamProvider[],
): {
  mappings: Record<string, string>;
  hasMatch: boolean;
} {
  const provider = providers.find((item) => item.id === candidate.provider_id);
  const mapping = (provider?.mappings ?? []).find(
    (m) =>
      m.local_model.trim() === candidate.local_model &&
      m.upstream_model.trim() === candidate.upstream_model &&
      (m.protocol ?? provider?.protocol ?? "chat_completions") === candidate.protocol,
  );

  const rawEfforts = [
    ...(mapping?.reasoning_efforts ?? []),
    ...(candidate.reasoning?.values ?? []),
  ];
  const nativeEfforts: string[] = [];
  for (const v of rawEfforts) {
    const trimmed = v.trim();
    if (trimmed !== "" && !nativeEfforts.includes(trimmed)) {
      nativeEfforts.push(trimmed);
    }
  }

  const supportsDisable =
    nativeEfforts.some((v) => v.toLowerCase() === "none") ||
    candidate.reasoning?.supports_disable === true;

  const positiveNativeEfforts = nativeEfforts.filter(
    (v) => v.toLowerCase() !== "none",
  );

  const mappings: Record<string, string> = {};
  for (const effort of CANONICAL_REASONING_EFFORTS) {
    if (effort === "none") {
      if (supportsDisable) {
        mappings["none"] = "disable";
      }
    } else {
      const matched = positiveNativeEfforts.find(
        (v) => v.toLowerCase() === effort.toLowerCase(),
      );
      if (matched) {
        mappings[effort] = `enum:${matched}`;
      }
    }
  }

  return {
    mappings,
    hasMatch: Object.keys(mappings).length > 0,
  };
}

/**
 * Stable candidate identity key. It is not a secret; it only folds the exact
 * source tuple (provider, local/upstream model, protocol) into one string.
 */
export function modelPoolCandidateKey(
  candidate: Pick<
    GatewayModelPoolCandidate,
    "provider_id" | "local_model" | "upstream_model" | "protocol"
  >,
): string {
  return [
    candidate.provider_id,
    candidate.local_model,
    candidate.upstream_model,
    candidate.protocol,
  ].join("\u0001");
}

/** Generate a fresh opaque pool id; the backend stores it verbatim. */
export function newModelPoolId(): string {
  return `pool-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

export function aiGatewayUpsertModelPool(pool: GatewayModelPool) {
  return invoke<GatewayConfig>("ai_gateway_upsert_model_pool", { pool });
}

export function aiGatewayDeleteModelPool(poolId: string) {
  return invoke<GatewayConfig>("ai_gateway_delete_model_pool", { poolId });
}

export function aiGatewaySetModelPoolEnabled(poolId: string, enabled: boolean) {
  return invoke<GatewayConfig>("ai_gateway_set_model_pool_enabled", {
    poolId,
    enabled,
  });
}

export function aiGatewayModelPoolsStatus() {
  return invoke<GatewayModelPoolStatus[]>("ai_gateway_model_pools_status");
}

/** i18n keys for every nonsecret candidate availability code. */
export const MODEL_POOL_AVAILABILITY_LABEL_KEYS: Record<string, string> = {
  ready: "aiGatewayModelPoolStatusReady",
  cooling: "aiGatewayModelPoolStatusCooling",
  source_disabled: "aiGatewayModelPoolStatusSourceDisabled",
  mapping_unavailable: "aiGatewayModelPoolStatusMappingUnavailable",
  missing_provider: "aiGatewayModelPoolStatusMissingProvider",
  no_usable_key: "aiGatewayModelPoolStatusNoUsableKey",
  unsupported_protocol: "aiGatewayModelPoolStatusUnsupportedProtocol",
};

/**
 * A candidate's resolved availability: the localized-code state is authoritative
 * for known current-config conditions, while the raw backend `reason` (if any) is
 * retained as additional detail and never substitutes the code.
 */
export interface ResolvedModelPoolCandidateState {
  availability: string;
  reason: string | null;
  cooling_until: number | null;
  provider_name: string | null;
  ready: boolean;
}

/** States derived from current intent that must never be masked by a stale snapshot. */
const HARD_CURRENT_POOL_STATES = new Set([
  "missing_provider",
  "source_disabled",
  "mapping_unavailable",
  "no_usable_key",
]);

/** A quota-marked key recovers automatically once its mark is older than 1800s. */
export const POOL_KEY_QUOTA_MARK_TTL_SECONDS = 1800;

function poolKeyUsable(key: GatewayProviderKey, nowSeconds: number): boolean {
  if (!key.enabled) return false;
  if (!key.auto_marked) return true;
  // Authentication marks are manual-only and never expire on their own.
  if (key.failure_kind === "authentication") return false;
  const markedAt = typeof key.marked_at === "number" ? key.marked_at : 0;
  if (markedAt <= 0) return false;
  return nowSeconds - markedAt > POOL_KEY_QUOTA_MARK_TTL_SECONDS;
}

/**
 * Derive a candidate's availability from the CURRENT config (provider intent,
 * exact mapping/protocol reference and key availability), optionally augmented by
 * this pool's own nonsecret status for transient signals (cooldown, live health).
 *
 * The own-pool status may add a cooling/health state over an otherwise-ready source,
 * but a stale snapshot can never turn a currently disabled/removed/mapping-mismatched
 * or key-less source back into `ready`. Another pool's status is never consulted.
 */
export function resolveModelPoolCandidateState(
  candidate: GatewayModelPoolCandidate,
  providers: GatewayUpstreamProvider[] | null | undefined,
  statusCandidate?: GatewayModelPoolCandidateStatus | null,
  nowSeconds: number = Math.floor(Date.now() / 1000),
): ResolvedModelPoolCandidateState {
  const provider =
    (providers ?? []).find((item) => item.id === candidate.provider_id) ?? null;
  const statusReason = statusCandidate?.reason?.trim()
    ? statusCandidate.reason.trim()
    : null;
  const providerName =
    provider?.name?.trim() ||
    statusCandidate?.provider_name?.trim() ||
    candidate.provider_id;

  let current = "ready";
  if (!provider) {
    current = "missing_provider";
  } else if (provider.enabled === false) {
    current = "source_disabled";
  } else {
    const mapping = (provider.mappings ?? []).find(
      (item) =>
        item.local_model.trim() === candidate.local_model &&
        item.upstream_model.trim() === candidate.upstream_model &&
        (item.protocol ?? provider.protocol ?? "chat_completions") ===
          candidate.protocol,
    );
    if (!mapping) {
      current = "mapping_unavailable";
    } else {
      const rawKeys = (provider as GatewayUpstreamProviderWithKeys).keys;
      if (Array.isArray(rawKeys)) {
        current = rawKeys.some((key) => poolKeyUsable(key, nowSeconds))
          ? "ready"
          : "no_usable_key";
      }
      // A missing/legacy `keys` field is unknown, never an automatic denial.
    }
  }

  if (HARD_CURRENT_POOL_STATES.has(current)) {
    return {
      availability: current,
      reason: statusReason,
      cooling_until: null,
      provider_name: providerName,
      ready: false,
    };
  }

  const statusAvailability = statusCandidate?.availability ?? null;
  if (statusAvailability) {
    return {
      availability: statusAvailability,
      reason: statusReason,
      cooling_until: statusCandidate?.cooling_until ?? null,
      provider_name: providerName,
      ready: statusAvailability === "ready",
    };
  }

  return {
    availability: current,
    reason: null,
    cooling_until: null,
    provider_name: providerName,
    ready: current === "ready",
  };
}

// ---------------------------------------------------------------------------
// Provider templates
// ---------------------------------------------------------------------------

/** One model shipped by a provider template. */
export interface GatewayProviderTemplateModel {
  upstream_model: string;
  local_model?: string;
  display_name?: string | null;
  protocol?: GatewayUpstreamProtocol | null;
  enabled: boolean;
  input?: number;
  cache_read?: number;
  cache_write?: number;
  output?: number;
  off_peaks?: OffPeakPrice[];
  reasoning_efforts?: string[];
  /** Declared total context window in tokens; absent/null means unspecified. */
  context_window?: number | null;
}

/** Model-synchronization strategy a provider template declares (closed set). */
export type GatewayTemplateSyncStrategy = "generic" | "opencode_go";

/** Quota/usage monitoring strategy a provider template declares (closed set). */
export type GatewayTemplateQuotaStrategy = "none" | "commandcode" | "opencode_go";

/** Error-handling strategy a provider template declares (closed set). */
export type GatewayTemplateErrorStrategy = "generic";

/**
 * The effective policy a provider resolves from its bound template, or the
 * generic fallback when it is unbound or its binding cannot be resolved.
 */
export interface GatewayEffectiveTemplatePolicy {
  sync_strategy: GatewayTemplateSyncStrategy;
  quota_strategy: GatewayTemplateQuotaStrategy;
  error_strategy: GatewayTemplateErrorStrategy;
}

/** The generic fallback used by every unbound or unresolved provider binding. */
export const GATEWAY_GENERIC_TEMPLATE_POLICY: GatewayEffectiveTemplatePolicy = {
  sync_strategy: "generic",
  quota_strategy: "none",
  error_strategy: "generic",
};

/** A built-in provider template catalog entry. */
export interface GatewayProviderTemplate {
  id: string;
  name: string;
  description: string;
  base_url: string;
  protocol: GatewayUpstreamProtocol;
  /** Model-synchronization strategy; a missing/unknown value reads as `generic`. */
  sync_strategy?: GatewayTemplateSyncStrategy;
  /** Quota/usage monitoring strategy; a missing/unknown value reads as `none`. */
  quota_strategy?: GatewayTemplateQuotaStrategy;
  /** Error-handling strategy; a missing/unknown value reads as `generic`. */
  error_strategy?: GatewayTemplateErrorStrategy;
  source: string;
  models_url?: string | null;
  models: GatewayProviderTemplateModel[];
  icon?: string | null;
}

/** A template plus the last-sync metadata surfaced to the UI. */
export interface GatewayProviderTemplateView {
  template: GatewayProviderTemplate;
  synced_at: number | null;
  source: string;
  from_snapshot: boolean;
}

/** A provider reference whose binding is resolved strictly from `template_id`. */
export interface GatewayTemplateBoundProvider {
  template_id?: string | null;
}

/**
 * A provider argument accepted by the binding helpers: the typed reference, any
 * other object (whose `template_id` is read when present) or `null`/`undefined`.
 * The wider shape keeps callers that pass partial provider fixtures assignable.
 */
export type GatewayTemplateProviderRef =
  | GatewayTemplateBoundProvider
  | Record<string, unknown>
  | null
  | undefined;

/** Read a non-blank `template_id` from a loosely typed provider reference. */
function boundTemplateId(provider: GatewayTemplateProviderRef): string | null {
  if (!provider || typeof provider !== "object") return null;
  const raw = (provider as { template_id?: unknown }).template_id;
  return typeof raw === "string" && raw !== "" ? raw : null;
}

function normalizeTemplateSyncStrategy(value: unknown): GatewayTemplateSyncStrategy {
  return value === "opencode_go" ? "opencode_go" : "generic";
}

function normalizeTemplateQuotaStrategy(value: unknown): GatewayTemplateQuotaStrategy {
  if (value === "commandcode") return "commandcode";
  if (value === "opencode_go") return "opencode_go";
  return "none";
}

function normalizeTemplateErrorStrategy(): GatewayTemplateErrorStrategy {
  return "generic";
}

/**
 * Resolve the template a provider is bound to, strictly from its `template_id`.
 *
 * An unbound provider, an unknown or deleted id and an empty template list all
 * resolve to `null`; URL, name and icon are never consulted (AC-005 / AC-006).
 */
export function resolveProviderTemplate(
  templates: GatewayProviderTemplateView[] | null | undefined,
  provider: GatewayTemplateProviderRef,
): GatewayProviderTemplateView | null {
  const templateId = boundTemplateId(provider);
  if (!templateId) return null;
  return (templates ?? []).find((view) => view.template.id === templateId) ?? null;
}

/**
 * Resolve a provider's effective policy from its bound template, falling back to
 * the generic policy for an unbound, unknown or deleted binding. Missing or
 * unknown stored strategy values resolve to `generic`/`none`/`generic`.
 */
export function resolveEffectiveTemplatePolicy(
  templates: GatewayProviderTemplateView[] | null | undefined,
  provider: GatewayTemplateProviderRef,
): GatewayEffectiveTemplatePolicy {
  const view = resolveProviderTemplate(templates, provider);
  if (!view) return { ...GATEWAY_GENERIC_TEMPLATE_POLICY };
  return {
    sync_strategy: normalizeTemplateSyncStrategy(view.template.sync_strategy),
    quota_strategy: normalizeTemplateQuotaStrategy(view.template.quota_strategy),
    error_strategy: normalizeTemplateErrorStrategy(),
  };
}

/**
 * Whether a provider carries a `template_id` that no longer resolves. An unbound
 * provider is never broken.
 */
export function isProviderTemplateBindingBroken(
  templates: GatewayProviderTemplateView[] | null | undefined,
  provider: GatewayTemplateProviderRef,
): boolean {
  const templateId = boundTemplateId(provider);
  if (!templateId) return false;
  return !(templates ?? []).some((view) => view.template.id === templateId);
}

/** Payload for creating one upstream provider from a template. */
export interface CreateProviderFromTemplateRequest {
  templateId: string;
  name: string;
  baseUrl: string;
  protocol: GatewayUpstreamProtocol;
  apiKey: string;
}

/** Display order for weekday chips: Monday first, Sunday last. */
export const GATEWAY_WEEKDAY_ORDER: readonly number[] = [1, 2, 3, 4, 5, 6, 0];

const GATEWAY_WEEKDAY_KEYS = [
  "aiGatewayWeekdaySun",
  "aiGatewayWeekdayMon",
  "aiGatewayWeekdayTue",
  "aiGatewayWeekdayWed",
  "aiGatewayWeekdayThu",
  "aiGatewayWeekdayFri",
  "aiGatewayWeekdaySat",
] as const;

/** i18n key for a UTC+8 weekday (`0` Sunday–`6` Saturday); `""` when out of range. */
export function gatewayWeekdayTranslationKey(day: number): string {
  return GATEWAY_WEEKDAY_KEYS[day] ?? "";
}

/**
 * Render a weekday set as localized labels. Invalid entries are dropped and
 * duplicates collapsed; an empty result means every day.
 */
export function formatOffPeakDays(
  days: number[] | null | undefined,
  t: TFunction,
): string {
  const unique = Array.from(
    new Set((days ?? []).filter((day) => day >= 0 && day <= 6)),
  );
  if (unique.length === 0) return t("aiGatewayEveryDay");
  return GATEWAY_WEEKDAY_ORDER.filter((day) => unique.includes(day))
    .map((day) => t(gatewayWeekdayTranslationKey(day)))
    .join(", ");
}

/** Whether a mapping's upstream model is absent from the template's current models. */
export function isMappingDeprecated(
  mapping: GatewayModelMapping,
  template: GatewayProviderTemplate | null | undefined,
): boolean {
  if (!template) return false;
  return !template.models.some(
    (model) => model.upstream_model === mapping.upstream_model,
  );
}

export function aiGatewayProviderTemplates() {
  return invoke<GatewayProviderTemplateView[]>("ai_gateway_provider_templates");
}

export function aiGatewaySyncProviderTemplate(templateId: string) {
  return invoke<GatewayProviderTemplateView>("ai_gateway_sync_provider_template", {
    templateId,
  });
}

export function aiGatewayUpsertProviderTemplate(
  template: GatewayProviderTemplate,
) {
  return invoke<GatewayProviderTemplateView[]>(
    "ai_gateway_upsert_provider_template",
    { template },
  );
}

export function aiGatewayDeleteProviderTemplate(templateId: string) {
  return invoke<GatewayProviderTemplateView[]>(
    "ai_gateway_delete_provider_template",
    { templateId },
  );
}

export function aiGatewayCreateProviderFromTemplate(
  request: CreateProviderFromTemplateRequest,
) {
  return invoke<GatewayConfig>("ai_gateway_create_provider_from_template", {
    templateId: request.templateId,
    name: request.name,
    baseUrl: request.baseUrl,
    protocol: request.protocol,
    apiKey: request.apiKey,
  });
}

export function aiGatewayDeleteProviderModel(
  providerId: string,
  upstreamModel: string,
) {
  return invoke<GatewayConfig>("ai_gateway_delete_provider_model", {
    providerId,
    upstreamModel,
  });
}

export function aiGatewayRestoreProviderModel(
  providerId: string,
  upstreamModel: string,
) {
  return invoke<GatewayConfig>("ai_gateway_restore_provider_model", {
    providerId,
    upstreamModel,
  });
}
