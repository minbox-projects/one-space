import { invoke } from "@tauri-apps/api/core";

export const API_GATEWAY_DEFAULT_PORT = 17688;
export const API_GATEWAY_STATUS_UPDATED_EVENT = "api-gateway-status-update";

/** Sentinel mask the frontend submits to keep a stored provider api key or local key; the backend reads it as "preserve existing value" (or generate a new one) and does not echo masked secrets. */
export const API_GATEWAY_KEY_MASK = "********";

/** Terminal tools API Gateway is allowed to configure; claude/antigravity are excluded. */
export const API_GATEWAY_SUPPORTED_TERMINAL_TOOLS = ["opencode", "codex"] as const;

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
}

/** Upstream endpoint family a provider exposes; request bodies are not translated. */
export type GatewayUpstreamProtocol = "chat_completions" | "responses";

export interface GatewayUpstreamProvider {
  id: string;
  name: string;
  base_url: string;
  api_key: string;
  default_model: string | null;
  protocol?: GatewayUpstreamProtocol;
  mappings: GatewayModelMapping[];
  enabled: boolean;
  auto_disabled: boolean;
  disabled_reason: string | null;
  disabled_at: number | null;
  consecutive_failures: number;
  last_error_at: number | null;
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

export interface GatewayConfig {
  enabled: boolean;
  port: number;
  providers: GatewayUpstreamProvider[];
  keys: GatewayKey[];
  default_key_id: string | null;
  terminal_syncs: GatewayTerminalSyncRecord[];
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

export interface GatewayTerminalTarget {
  tool: string;
  name: string;
  provider_id: string | null;
  base_url: string | null;
  synced: boolean;
  pending_sync: boolean;
  synced_key_id: string | null;
  synced_at: number | null;
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

/** Resolved upstream model plus the endpoint family the request is sent to. */
export interface GatewayMappingPreview {
  upstreamModel: string;
  endpoint: GatewayUpstreamProtocol;
}

/**
 * Resolve the upstream model and target endpoint for a requested local model.
 *
 * An exact, non-blank mapping match wins and may override the protocol per row;
 * disabled rows are skipped. When every matching row is disabled the request is
 * not served (no default-model fallback); otherwise the provider default model
 * is used with the provider protocol.
 * `null` means the provider cannot serve the requested model.
 */
export function resolveMappingPreview(
  provider: {
    protocol?: GatewayUpstreamProtocol;
    mappings: GatewayModelMapping[];
    default_model: string | null;
  },
  localModel: string,
): GatewayMappingPreview | null {
  const candidates = provider.mappings.filter(
    (entry) =>
      entry.local_model === localModel && entry.upstream_model.trim() !== "",
  );
  if (candidates.length > 0) {
    const mapping = candidates.find((entry) => entry.enabled !== false);
    if (mapping) {
      return {
        upstreamModel: mapping.upstream_model,
        endpoint: mapping.protocol ?? provider.protocol ?? "chat_completions",
      };
    }
    return null;
  }
  if (provider.default_model) {
    return {
      upstreamModel: provider.default_model,
      endpoint: provider.protocol ?? "chat_completions",
    };
  }
  return null;
}

/** One upstream source that can serve an aggregated local model. */
export interface AggregatedModelProvider {
  providerId: string;
  providerName: string;
  upstreamModel: string;
  endpoint: GatewayUpstreamProtocol;
  isDefault: boolean;
}

/** A local model name and every enabled upstream source mapped to it. */
export interface AggregatedModel {
  model: string;
  providers: AggregatedModelProvider[];
}

/**
 * Aggregate the local models served by enabled, non-auto-disabled providers.
 *
 * A provider contributes its default model (as `isDefault`) plus every non-blank
 * enabled local mapping. Results are grouped by local model and sorted
 * deterministically.
 */
export function aggregateModels(
  providers: GatewayUpstreamProvider[],
): AggregatedModel[] {
  const groups = new Map<string, AggregatedModelProvider[]>();

  providers.forEach((provider) => {
    if (!provider.enabled || provider.auto_disabled) return;

    const defaultModel = (provider.default_model ?? "").trim();
    if (defaultModel) {
      const entries = groups.get(defaultModel) ?? [];
      entries.push({
        providerId: provider.id,
        providerName: provider.name,
        upstreamModel: defaultModel,
        endpoint: provider.protocol ?? "chat_completions",
        isDefault: true,
      });
      groups.set(defaultModel, entries);
    }

    provider.mappings.forEach((mapping) => {
      if (mapping.enabled === false) return;
      const localModel = mapping.local_model.trim();
      if (!localModel) return;
      const upstreamModel = mapping.upstream_model.trim();
      if (!upstreamModel) return;
      const entries = groups.get(localModel) ?? [];
      entries.push({
        providerId: provider.id,
        providerName: provider.name,
        upstreamModel,
        endpoint: mapping.protocol ?? provider.protocol ?? "chat_completions",
        isDefault: false,
      });
      groups.set(localModel, entries);
    });
  });

  return Array.from(groups.entries())
    .map(([model, entries]) => ({
      model,
      providers: [...entries].sort((a, b) => {
        const byName = a.providerName.localeCompare(b.providerName);
        return byName !== 0 ? byName : a.upstreamModel.localeCompare(b.upstreamModel);
      }),
    }))
    .sort((a, b) => a.model.localeCompare(b.model));
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

export function apiGatewayGetConfig() {
  return invoke<GatewayConfig>("api_gateway_get_config");
}

export function apiGatewaySaveConfig(config: GatewayConfig) {
  return invoke<GatewayConfig>("api_gateway_save_config", { config });
}

export function apiGatewayUpsertProvider(provider: GatewayUpstreamProvider) {
  return invoke<GatewayConfig>("api_gateway_upsert_provider", { provider });
}

export function apiGatewayDeleteProvider(providerId: string) {
  return invoke<GatewayConfig>("api_gateway_delete_provider", { providerId });
}

export function apiGatewaySetProviderEnabled(
  providerId: string,
  enabled: boolean,
) {
  return invoke<GatewayConfig>("api_gateway_set_provider_enabled", {
    providerId,
    enabled,
  });
}

export function apiGatewayReenableProvider(providerId: string) {
  return invoke<GatewayConfig>("api_gateway_reenable_provider", { providerId });
}

export function apiGatewayUpsertKey(key: GatewayKey) {
  return invoke<GatewayConfig>("api_gateway_upsert_key", { key });
}

export function apiGatewayDeleteKey(keyId: string) {
  return invoke<GatewayConfig>("api_gateway_delete_key", { keyId });
}

export function apiGatewaySetDefaultKey(keyId: string) {
  return invoke<GatewayConfig>("api_gateway_set_default_key", { keyId });
}

export function apiGatewayStart() {
  return invoke<GatewayStatus>("api_gateway_start");
}

export function apiGatewayStop() {
  return invoke<GatewayStatus>("api_gateway_stop");
}

export function apiGatewayStatus() {
  return invoke<GatewayStatus>("api_gateway_status");
}

export function apiGatewayTerminalTargets() {
  return invoke<GatewayTerminalTarget[]>("api_gateway_terminal_targets");
}

export function apiGatewayConfigureTerminal(targetTools: string[]) {
  return invoke<GatewayTerminalSyncRecord[]>("api_gateway_configure_terminal", {
    targetTools,
  });
}

export function apiGatewaySyncTerminal(targetTools?: string[]) {
  return invoke<GatewayTerminalSyncRecord[]>(
    "api_gateway_sync_terminal",
    targetTools ? { targetTools } : {},
  );
}

/** Quick time ranges shared by the usage-stats and request-logs tabs. */
export type UsageRangeKey = "today" | "7d" | "15d" | "30d" | "all";

export const USAGE_RANGE_KEYS: readonly UsageRangeKey[] = [
  "today",
  "7d",
  "15d",
  "30d",
  "all",
];

/**
 * Map a quick-range selection to the backend `days` argument.
 * `null` means all time, `1` means today, otherwise the last N calendar days.
 */
export function usageRangeToDays(range: UsageRangeKey): number | null {
  switch (range) {
    case "today":
      return 1;
    case "7d":
      return 7;
    case "15d":
      return 15;
    case "30d":
      return 30;
    case "all":
      return null;
  }
}

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
}

export interface UsageBucket extends UsageMetrics {
  label: string;
}

export interface UsageProviderBreakdown extends UsageMetrics {
  provider_id: string;
  provider_name: string;
}

export interface UsageModelBreakdown extends UsageMetrics {
  local_model: string;
  providers: UsageProviderBreakdown[];
}

export interface UsageStats extends UsageMetrics {
  granularity: "hour" | "day";
  buckets: UsageBucket[];
  models: UsageModelBreakdown[];
}

export type UsageLogResult = "success" | "failure" | "cancelled";

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
  duration_ms: number;
}

export interface UsageLogGroup {
  group: string;
  request_count: number;
  /** Failure records only; cancelled is excluded. */
  error_count: number;
  last_request_at_ms: number;
}

export type UsageGroupBy = "none" | "model" | "day";

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
}

export interface UsageLogsQuery {
  days: number | null;
  groupBy?: UsageGroupBy | null;
  status?: UsageLogResult | null;
  model?: string | null;
  page?: number;
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
}

export interface ModelPrice {
  provider_id?: string | null;
  upstream_model: string;
  /** USD per million tokens. */
  input: number;
  cache_read: number;
  cache_write: number;
  output: number;
  off_peak?: OffPeakPrice | null;
}

export interface ProviderAvailableModel {
  upstream_model: string;
  display_name?: string | null;
  is_default?: boolean;
}

/**
 * Extract unique upstream models configured for a provider (from default_model and mappings).
 */
export function getProviderAvailableModels(
  provider: GatewayUpstreamProvider,
): ProviderAvailableModel[] {
  const map = new Map<string, ProviderAvailableModel>();
  const defaultModel = (provider.default_model ?? "").trim();
  if (defaultModel) {
    map.set(defaultModel, {
      upstream_model: defaultModel,
      is_default: true,
    });
  }
  for (const m of provider.mappings) {
    const upstream = m.upstream_model.trim();
    if (!upstream) continue;
    const existing = map.get(upstream);
    if (!existing) {
      map.set(upstream, {
        upstream_model: upstream,
        display_name: m.display_name?.trim() || m.local_model.trim(),
        is_default: false,
      });
    } else if (!existing.display_name && (m.display_name?.trim() || m.local_model.trim())) {
      existing.display_name = m.display_name?.trim() || m.local_model.trim();
    }
  }
  return Array.from(map.values()).sort((a, b) =>
    a.upstream_model.localeCompare(b.upstream_model),
  );
}

const UTC8_OFFSET_MS = 8 * 60 * 60 * 1000;

function pad2(value: number): string {
  return String(value).padStart(2, "0");
}

/** Format a millisecond timestamp as `YYYY-MM-DD HH:mm` in UTC+8. */
export function formatUtc8DateTime(ms: number | null | undefined): string | null {
  if (ms === null || ms === undefined) return null;
  const date = new Date(ms + UTC8_OFFSET_MS);
  return `${date.getUTCFullYear()}-${pad2(date.getUTCMonth() + 1)}-${pad2(
    date.getUTCDate(),
  )} ${pad2(date.getUTCHours())}:${pad2(date.getUTCMinutes())}`;
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
      return "apiGatewayStatusSuccess";
    case "failure":
      return "apiGatewayStatusFailure";
    case "cancelled":
      return "apiGatewayStatusCancelled";
  }
}

/** Card totals, time buckets and model/provider breakdown for a time range. */
export function apiGatewayUsageStats(days: number | null) {
  return invoke<UsageStats>("api_gateway_usage_stats", { days });
}

/** Paged request logs, optionally grouped by model or UTC+8 day. */
export function apiGatewayRequestLogs(query: UsageLogsQuery) {
  return invoke<UsageLogsPage>("api_gateway_request_logs", {
    days: query.days,
    groupBy: query.groupBy ?? null,
    status: query.status ?? null,
    model: query.model ?? null,
    page: query.page ?? 1,
  });
}

export function apiGatewayModelPricesGet() {
  return invoke<ModelPrice[]>("api_gateway_model_prices_get");
}

export function apiGatewayModelPricesSave(prices: ModelPrice[]) {
  return invoke<ModelPrice[]>("api_gateway_model_prices_save", { prices });
}

export function apiGatewayUsageRetentionGet() {
  return invoke<number>("api_gateway_usage_retention_get");
}

export function apiGatewayUsageRetentionSave(days: number) {
  return invoke<number>("api_gateway_usage_retention_save", { days });
}
