import { invoke } from "@tauri-apps/api/core";
import type { TFunction } from "i18next";

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
  /** Reasoning-effort identifiers the model advertises. */
  reasoning_efforts?: string[];
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
  /** Template this provider was created from; absent for manual providers. */
  template_id?: string | null;
  /** Template models the user deleted for this provider. */
  ignored_models?: string[];
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
  /** Provider-scoped price rows; optional so older fixtures stay valid. */
  model_prices?: ModelPrice[];
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
  /** Trimmed mapping `display_name`; absent for default-model sources or undeclared names. */
  displayName?: string;
}

/** A local model name and every enabled upstream source mapped to it. */
export interface AggregatedModel {
  model: string;
  providers: AggregatedModelProvider[];
}

/**
 * Aggregate the local models served by enabled, non-auto-disabled providers.
 *
 * A provider contributes every non-blank enabled local mapping. Results are
 * grouped by local model and sorted deterministically. Default models are fallback
 * targets for unmapped requests and are not listed here.
 */
export function aggregateModels(
  providers: GatewayUpstreamProvider[],
): AggregatedModel[] {
  const groups = new Map<string, AggregatedModelProvider[]>();

  providers.forEach((provider) => {
    if (!provider.enabled || provider.auto_disabled) return;

    provider.mappings.forEach((mapping) => {
      if (mapping.enabled === false) return;
      const localModel = mapping.local_model.trim();
      if (!localModel) return;
      const upstreamModel = mapping.upstream_model.trim();
      if (!upstreamModel) return;
      const entries = groups.get(localModel) ?? [];
      const displayName = mapping.display_name?.trim();
      entries.push({
        providerId: provider.id,
        providerName: provider.name,
        upstreamModel,
        endpoint: mapping.protocol ?? provider.protocol ?? "chat_completions",
        isDefault: false,
        ...(displayName ? { displayName } : {}),
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

export function apiGatewayUpsertProvider(
  provider: GatewayUpstreamProvider,
  prices?: ModelPrice[] | null,
) {
  return invoke<GatewayConfig>("api_gateway_upsert_provider", {
    provider,
    prices: prices ?? null,
  });
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
  off_peak?: OffPeakPrice | null;
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

  const rawOffPeaks =
    price.off_peaks && price.off_peaks.length > 0
      ? price.off_peaks
      : price.off_peak
        ? [price.off_peak]
        : [];

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
    row.off_peak = offPeaks[0] ?? null;
  }

  return row;
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

export function apiGatewayUsageRetentionGet() {
  return invoke<number>("api_gateway_usage_retention_get");
}

export function apiGatewayUsageRetentionSave(days: number) {
  return invoke<number>("api_gateway_usage_retention_save", { days });
}

// ---------------------------------------------------------------------------
// Provider templates
// ---------------------------------------------------------------------------

/** One model shipped by a provider template. */
export interface GatewayProviderTemplateModel {
  upstream_model: string;
  display_name?: string | null;
  protocol?: GatewayUpstreamProtocol | null;
  /** USD per million tokens. */
  input: number;
  cache_read: number;
  cache_write: number;
  output: number;
  off_peaks?: OffPeakPrice[];
  off_peak?: OffPeakPrice | null;
  reasoning_efforts?: string[];
}

/** A built-in provider template catalog entry. */
export interface GatewayProviderTemplate {
  id: string;
  name: string;
  description: string;
  base_url: string;
  protocol: GatewayUpstreamProtocol;
  source: string;
  snapshot_version: string;
  models_url?: string | null;
  models: GatewayProviderTemplateModel[];
}

/** A template plus the last-sync metadata surfaced to the UI. */
export interface GatewayProviderTemplateView {
  template: GatewayProviderTemplate;
  synced_at: number | null;
  source: string;
  from_snapshot: boolean;
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
  "apiGatewayWeekdaySun",
  "apiGatewayWeekdayMon",
  "apiGatewayWeekdayTue",
  "apiGatewayWeekdayWed",
  "apiGatewayWeekdayThu",
  "apiGatewayWeekdayFri",
  "apiGatewayWeekdaySat",
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
  if (unique.length === 0) return t("apiGatewayEveryDay");
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

export function apiGatewayProviderTemplates() {
  return invoke<GatewayProviderTemplateView[]>("api_gateway_provider_templates");
}

export function apiGatewaySyncProviderTemplate(templateId: string) {
  return invoke<GatewayProviderTemplateView>("api_gateway_sync_provider_template", {
    templateId,
  });
}

export function apiGatewayUpsertProviderTemplate(
  template: GatewayProviderTemplate,
) {
  return invoke<GatewayProviderTemplateView[]>(
    "api_gateway_upsert_provider_template",
    { template },
  );
}

export function apiGatewayDeleteProviderTemplate(templateId: string) {
  return invoke<GatewayProviderTemplateView[]>(
    "api_gateway_delete_provider_template",
    { templateId },
  );
}

export function apiGatewayResetProviderTemplates() {
  return invoke<GatewayProviderTemplateView[]>(
    "api_gateway_reset_provider_templates",
  );
}

export function apiGatewayFetchModels(url: string, apiKey?: string) {
  return invoke<string[]>("api_gateway_fetch_models", {
    url,
    apiKey: apiKey && apiKey.trim() ? apiKey.trim() : null,
  });
}

export function apiGatewayCreateProviderFromTemplate(
  request: CreateProviderFromTemplateRequest,
) {
  return invoke<GatewayConfig>("api_gateway_create_provider_from_template", {
    templateId: request.templateId,
    name: request.name,
    baseUrl: request.baseUrl,
    protocol: request.protocol,
    apiKey: request.apiKey,
  });
}

export function apiGatewayDeleteProviderModel(
  providerId: string,
  upstreamModel: string,
) {
  return invoke<GatewayConfig>("api_gateway_delete_provider_model", {
    providerId,
    upstreamModel,
  });
}

export function apiGatewayRestoreProviderModel(
  providerId: string,
  upstreamModel: string,
) {
  return invoke<GatewayConfig>("api_gateway_restore_provider_model", {
    providerId,
    upstreamModel,
  });
}
