import type { ClaudeModelSelection, CreateProviderFromTemplateRequest, GatewayConfig, GatewayKey, GatewayModelPool, GatewayModelPoolStatus, GatewayProviderTemplate, GatewayProviderTemplateView, GatewaySettings, GatewayStatus, GatewayTerminalSyncRecord, GatewayTerminalTarget, GatewayUpstreamProvider, ModelPrice, ProviderGoUsage, ProviderQuota, ProviderUsageHistory, ProviderUsageIdentity, TemplateAutoRefreshStatus, UsageLogsPage, UsageLogsQuery, UsageRangeKey, UsageStats } from "./types";
import { invoke } from "@tauri-apps/api/core";

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


/**
 * Read the backend automatic-refresh failure snapshot without starting any
 * schedule; the process scheduler owns all template timing and syncing.
 */
export function aiGatewayTemplateAutoRefreshStatus() {
  return invoke<TemplateAutoRefreshStatus>(
    "ai_gateway_template_auto_refresh_status",
  );
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
