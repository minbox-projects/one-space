import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type GatewayRuntimeState = "stopped" | "running" | "error" | "locked";
export type GatewayAvailability = "ready" | "locked" | "error";
export type AccountType = "oauth" | "api_key";
export type UpstreamProtocol = "responses" | "chat_completions";

export interface GatewayRuntime {
  state: GatewayRuntimeState;
  availability: GatewayAvailability;
  port: number;
  run_enabled: boolean;
  error_code?: string | null;
  lock_reason?: string | null;
}

export interface GatewaySettings {
  port: number;
  globalQuotaThresholdPercent: number;
  logRetentionDays: 7 | 30 | 90 | 180 | null;
  runEnabled: boolean;
}

export interface GatewayGroup {
  id: string;
  name: string;
  sort_order: number;
  is_default: boolean;
}

export interface GatewayAccount {
  id: string;
  stable_external_id?: string | null;
  account_type: AccountType;
  name: string;
  group_id: string;
  sort_order: number;
  note: string;
  enabled: boolean;
  health_status: string;
  quota_threshold_override_percent?: number | null;
  base_url?: string | null;
  auth_method?: string | null;
  upstream_protocol?: UpstreamProtocol | null;
  tags: string[];
  model_mappings: ModelMapping[];
}

export interface PublicModel {
  id: string;
  displayName: string;
  enabled: boolean;
}

export interface GatewayKeyRecord {
  id: string;
  name: string;
  maskedKey: string;
  enabled: boolean;
  expiresAt?: string | null;
  revokedAt?: string | null;
  lastUsedAt?: string | null;
  createdAt: string;
  groupIds: string[];
  modelIds: string[];
  today: GatewayKeyUsage;
  last30Days: GatewayKeyUsage;
}

export interface GatewayKeyUsage {
  requestCount: number;
  totalTokens: number;
  estimatedCostUsd?: string | null;
}

export interface OneTimeGatewayKey {
  key: GatewayKeyRecord;
  plaintext: string;
}

export interface GatewayKeyDisplayGroup {
  id: string;
  name: string;
  isDefault: boolean;
  createdAt: string;
  updatedAt: string;
}

export type GatewayKeyStatus = "active" | "disabled" | "expired";
export type GatewayKeyStatusFilter = "all" | GatewayKeyStatus;
export type GatewayKeySort =
  | "createdNewest"
  | "createdOldest"
  | "nameAscending"
  | "nameDescending";
export type GatewayProviderTool = "claude" | "codex" | "gemini" | "opencode";

export interface GatewayKeyWindowUsage {
  totalTokens: number;
  estimatedCostUsd?: string | null;
  costCalculable: boolean;
}

export interface GatewayKeyListItem {
  id: string;
  name: string;
  maskedKey: string;
  displayGroupId: string;
  displayGroupName: string;
  status: GatewayKeyStatus;
  expiresAt?: string | null;
  createdAt: string;
  groupIds: string[];
  modelIds: string[];
  today: GatewayKeyWindowUsage;
  last30Days: GatewayKeyWindowUsage;
}

export interface GatewayKeyListPage {
  items: GatewayKeyListItem[];
  total: number;
}

export interface GatewayKeyConversionToolState {
  tool: GatewayProviderTool;
  converted: boolean;
  serviceProviderId?: string | null;
}

export interface ConvertedGatewayProviderSummary {
  tool: GatewayProviderTool;
  serviceProviderId: string;
  name: string;
  activated: boolean;
}

export interface GatewayKeyConversionResult {
  keyId: string;
  providers: ConvertedGatewayProviderSummary[];
  tools: GatewayKeyConversionToolState[];
}

export interface TokenUsage {
  inputTokens?: number | null;
  outputTokens?: number | null;
  cacheReadTokens?: number | null;
  cacheWriteTokens?: number | null;
  totalTokens?: number | null;
}

export interface TrendPoint {
  localDate: string;
  requestCount: number;
  successCount: number;
  failureCount: number;
  usage: TokenUsage;
  estimatedCostUsd?: string | null;
  costCalculable: boolean;
}

export interface GatewayHomepage {
  accountCount: number;
  availableCount: number;
  unavailableCount: number;
  staleCount: number;
  today: TrendPoint;
  trend: TrendPoint[];
}

export interface HomepageFilters {
  accountId?: string;
  groupId?: string;
  publicModelId?: string;
}

export interface GatewayBootstrap {
  runtime: GatewayRuntime;
  settings: GatewaySettings;
  groups: GatewayGroup[];
  accounts: GatewayAccount[];
  models: PublicModel[];
  keys: GatewayKeyRecord[];
  homepage: GatewayHomepage;
  oauthReleaseBlockReason?: string | null;
}

export interface QuotaWindow {
  id: string;
  account_id: string;
  name: string;
  scope_type: "global" | "model" | "endpoint" | "capability" | "unknown";
  scope_value?: string | null;
  upstream_window_id?: string | null;
  used_percent?: number | null;
  remaining_percent?: number | null;
  resets_at?: string | null;
  duration_seconds?: number | null;
  last_succeeded_at?: string | null;
  is_stale: boolean;
  raw_kind?: string | null;
}

export interface ModelMapping {
  account_id: string;
  public_model_id: string;
  upstream_model_id: string;
  enabled: boolean;
}

export interface CreateApiKeyModelMappingInput {
  publicModelId: string;
  upstreamModelId: string;
  enabled: boolean;
}

export interface CreateApiKeyModelPriceInput {
  publicModelId: string;
  inputPerMillionUsd?: string | null;
  outputPerMillionUsd?: string | null;
  cacheReadPerMillionUsd?: string | null;
  cacheWritePerMillionUsd?: string | null;
}

export interface CreateApiKeyAccountWithConfigurationInput {
  name: string;
  baseUrl: string;
  apiKey: string;
  authMethod: "bearer" | "api_key_header";
  upstreamProtocol: UpstreamProtocol;
  groupId?: string | null;
  tags?: string[];
  quotaThresholdOverridePercent?: number | null;
  note: string;
  mappings?: CreateApiKeyModelMappingInput[];
  prices?: CreateApiKeyModelPriceInput[];
}

export interface RequestLog {
  id: string;
  request_id: string;
  started_at: string;
  completed_at?: string | null;
  endpoint: string;
  public_model_id: string;
  upstream_model_id_snapshot?: string | null;
  api_key_name_snapshot?: string | null;
  account_name_snapshot?: string | null;
  group_name_snapshot?: string | null;
  status: string;
  error_code?: string | null;
  usage: {
    input_tokens?: number | null;
    output_tokens?: number | null;
    cache_read_tokens?: number | null;
    cache_write_tokens?: number | null;
    total_tokens?: number | null;
  };
  estimated_cost_usd?: string | null;
  cost_calculable: boolean;
}

export interface RequestAttempt {
  id: string;
  attempt_number: number;
  account_name_snapshot: string;
  upstream_model_id_snapshot?: string | null;
  started_at: string;
  completed_at?: string | null;
  status: string;
  error_code?: string | null;
  emitted_client_bytes: boolean;
  affected_health: boolean;
}

export interface LogPage {
  items: RequestLog[];
  nextCursor?: string | null;
}

export interface PriceRecord {
  public_model_id: string;
  account_id?: string | null;
  source: string;
  effective_at: string;
  input_per_million_usd?: string | null;
  output_per_million_usd?: string | null;
  cache_read_per_million_usd?: string | null;
  cache_write_per_million_usd?: string | null;
}

export interface OAuthBeginResult {
  sessionId: string;
  authorizationUrl?: string;
  callbackUrl?: string;
  userCode?: string;
  verificationUrl?: string;
  intervalSeconds?: number;
  expiresInSeconds?: number;
}

export interface GatewayAccountDeletedEvent {
  accountId: string;
  deleted: true;
}

export type GatewayAccountEvent = GatewayAccount | GatewayAccountDeletedEvent;

export interface GatewayOAuthStateEvent {
  sessionId: string;
  state: "completed" | "cancelled";
}

export type GatewayOAuthEvent = OAuthBeginResult | GatewayOAuthStateEvent;

export class AiRoutingGatewayError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "AiRoutingGatewayError";
  }
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    const message =
      typeof error === "string"
        ? error
        : error instanceof Error
          ? error.message
          : JSON.stringify(error);
    throw new AiRoutingGatewayError(message || "ai_routing_gateway_error");
  }
}

function homepageFilterArgs(filters?: HomepageFilters) {
  if (!filters || !Object.values(filters).some((value) => value)) return {};
  return { filters };
}

export const aiRoutingGatewayBootstrap = (days: 7 | 15 | 30 = 7, filters?: HomepageFilters) =>
  call<GatewayBootstrap>("ai_routing_gateway_bootstrap", { days, ...homepageFilterArgs(filters) });
export const aiRoutingGatewayRuntimeStatus = () =>
  call<GatewayRuntime>("ai_routing_gateway_runtime_status");
export const aiRoutingGatewayRuntimeStart = () =>
  call<GatewayRuntime>("ai_routing_gateway_runtime_start");
export const aiRoutingGatewayRuntimeStop = () =>
  call<GatewayRuntime>("ai_routing_gateway_runtime_stop");
export const aiRoutingGatewaySettingsGet = () =>
  call<GatewaySettings>("ai_routing_gateway_settings_get");
export const aiRoutingGatewaySettingsSave = (input: GatewaySettings) =>
  call<GatewaySettings>("ai_routing_gateway_settings_save", { input });
export const aiRoutingGatewayGroupCreate = (input: { name: string; sortOrder: number }) =>
  call<GatewayGroup>("ai_routing_gateway_group_create", { input });
export const aiRoutingGatewayGroupRename = (input: { groupId: string; name: string }) =>
  call<GatewayGroup>("ai_routing_gateway_group_rename", { input });
export const aiRoutingGatewayGroupDelete = (groupId: string) =>
  call<void>("ai_routing_gateway_group_delete", { groupId });
export const aiRoutingGatewayAccountCreateApiKey = (input: {
  name: string;
  baseUrl: string;
  apiKey: string;
  authMethod: "bearer" | "api_key_header";
  upstreamProtocol: UpstreamProtocol;
  note: string;
}) => call<GatewayAccount>("ai_routing_gateway_account_create_api_key", { input });
export const aiRoutingGatewayAccountCreateApiKeyWithConfiguration = (
  input: CreateApiKeyAccountWithConfigurationInput,
) => call<GatewayAccount>("ai_routing_gateway_account_create_api_key_with_configuration", { input });
export const aiRoutingGatewayAccountUpdate = (input: {
  accountId: string;
  name: string;
  groupId: string;
  sortOrder: number;
  note: string;
  enabled: boolean;
  quotaThresholdOverridePercent?: number | null;
  tags: string[];
  baseUrl?: string;
  apiKey?: string | null;
  authMethod?: "bearer" | "api_key_header";
  upstreamProtocol?: UpstreamProtocol;
}) => call<GatewayAccount>("ai_routing_gateway_account_update", { input });
export const aiRoutingGatewayAccountMove = (accountId: string, direction: -1 | 1) =>
  call<GatewayAccount>("ai_routing_gateway_account_move", { accountId, direction });
export const aiRoutingGatewayAccountDeleteConfirmation = (accountId: string) =>
  call<string>("ai_routing_gateway_account_delete_confirmation", { accountId });
export const aiRoutingGatewayAccountDelete = (accountId: string, confirmationToken: string) =>
  call<void>("ai_routing_gateway_account_delete", { accountId, confirmationToken });
export const aiRoutingGatewayAccountsDisable = (accountIds: string[]) =>
  call<GatewayAccount[]>("ai_routing_gateway_accounts_disable", { input: { accountIds } });
export const aiRoutingGatewayAccountsDeleteConfirmation = (accountIds: string[]) =>
  call<string>("ai_routing_gateway_accounts_delete_confirmation", { input: { accountIds } });
export const aiRoutingGatewayAccountsDelete = (accountIds: string[], confirmationToken: string) =>
  call<void>("ai_routing_gateway_accounts_delete", { input: { accountIds, confirmationToken } });
export const aiRoutingGatewayQuotaList = (accountId: string) =>
  call<QuotaWindow[]>("ai_routing_gateway_quota_list", { accountId });
export const aiRoutingGatewayQuotaRefresh = (accountId: string) =>
  call<void>("ai_routing_gateway_quota_refresh", { accountId });
export const aiRoutingGatewayMappingList = (accountId: string) =>
  call<ModelMapping[]>("ai_routing_gateway_mapping_list", { accountId });
export const aiRoutingGatewayMappingSave = (input: {
  accountId: string;
  publicModelId: string;
  upstreamModelId: string;
  enabled: boolean;
}) => call<void>("ai_routing_gateway_mapping_save", { input });
export const aiRoutingGatewayKeyCreate = (input: {
  name: string;
  displayGroupId?: string;
  groupIds: string[];
  modelIds: string[];
  expiresAt?: string | null;
}) => call<OneTimeGatewayKey>("ai_routing_gateway_key_create", { input });
export const aiRoutingGatewayKeyDisplayGroupsList = () =>
  call<GatewayKeyDisplayGroup[]>("ai_routing_gateway_key_display_groups_list");
export const aiRoutingGatewayKeyDisplayGroupCreate = (name: string) =>
  call<GatewayKeyDisplayGroup>("ai_routing_gateway_key_display_group_create", { input: { name } });
export const aiRoutingGatewayKeyDisplayGroupRename = (groupId: string, name: string) =>
  call<GatewayKeyDisplayGroup>("ai_routing_gateway_key_display_group_rename", {
    input: { groupId, name },
  });
export const aiRoutingGatewayKeyDisplayGroupDelete = (groupId: string) =>
  call<void>("ai_routing_gateway_key_display_group_delete", { groupId });
export const aiRoutingGatewayKeyList = (input: {
  groupId: string;
  text?: string;
  status: GatewayKeyStatusFilter;
  page: number;
  pageSize: number;
  sort: GatewayKeySort;
}) => call<GatewayKeyListPage>("ai_routing_gateway_key_list", { input });
export const aiRoutingGatewayKeyUpdate = (input: {
  keyId: string;
  name: string;
  displayGroupId: string;
  groupIds: string[];
  modelIds: string[];
  expiresAt?: string | null;
}) => call<void>("ai_routing_gateway_key_update", { input });
export const aiRoutingGatewayKeyRegenerate = (keyId: string) =>
  call<OneTimeGatewayKey>("ai_routing_gateway_key_regenerate", { keyId });
export const aiRoutingGatewayKeyCopy = (keyId: string) =>
  call<string>("ai_routing_gateway_key_copy", { keyId });
export const aiRoutingGatewayKeyGroupsUpdate = (keyId: string, groupIds: string[]) =>
  call<string[]>("ai_routing_gateway_key_groups_update", { input: { keyId, groupIds } });
export const aiRoutingGatewayKeySetEnabled = (keyId: string, enabled: boolean) =>
  call<void>("ai_routing_gateway_key_set_enabled", { keyId, enabled });
export const aiRoutingGatewayKeyRevoke = (keyId: string) =>
  call<void>("ai_routing_gateway_key_revoke", { keyId });
export const aiRoutingGatewayKeyDelete = (keyId: string) =>
  call<void>("ai_routing_gateway_key_delete", { keyId });
export const aiRoutingGatewayKeyConvertibleTools = (keyId: string) =>
  call<GatewayKeyConversionToolState[]>("ai_routing_gateway_key_convertible_tools", { keyId });
export const aiRoutingGatewayKeyConvertToProviders = (input: {
  keyId: string;
  tools: GatewayProviderTool[];
  activate?: boolean;
}) =>
  call<GatewayKeyConversionResult>("ai_routing_gateway_key_convert_to_providers", {
    input: { ...input, activate: input.activate ?? false },
  });
export const aiRoutingGatewayLogsQuery = (input: {
  startedAtOrAfter?: string;
  startedBefore?: string;
  accountId?: string;
  groupId?: string;
  publicModelId?: string;
  upstreamModelId?: string;
  status?: string;
  errorCode?: string;
  apiKeyId?: string;
  cursor?: string;
  pageSize: number;
}) => call<LogPage>("ai_routing_gateway_logs_query", { input });
export const aiRoutingGatewayLogAttempts = (requestLogId: string) =>
  call<RequestAttempt[]>("ai_routing_gateway_log_attempts", { requestLogId });
export const aiRoutingGatewayLogsClear = () =>
  call<number>("ai_routing_gateway_logs_clear");
export const aiRoutingGatewayPricesList = () =>
  call<PriceRecord[]>("ai_routing_gateway_prices_list");
export const aiRoutingGatewayPriceSave = (input: {
  publicModelId: string;
  accountId?: string | null;
  effectiveAt: string;
  inputPerMillionUsd?: string | null;
  outputPerMillionUsd?: string | null;
  cacheReadPerMillionUsd?: string | null;
  cacheWritePerMillionUsd?: string | null;
}) => call<string>("ai_routing_gateway_price_save", { input });
export const aiRoutingGatewayStatsHome = (days: 7 | 15 | 30, filters?: HomepageFilters) =>
  call<GatewayHomepage>("ai_routing_gateway_stats_home", { days, ...homepageFilterArgs(filters) });
export const aiRoutingGatewayRetentionSave = (days: 7 | 30 | 90 | 180 | null) =>
  call<void>("ai_routing_gateway_retention_save", { days });
export type GatewayEventHandlers = {
  runtime?: (runtime: GatewayRuntime) => void;
  account?: (payload: GatewayAccountEvent) => void;
  oauth?: (payload: GatewayOAuthEvent) => void;
};

export async function subscribeAiRoutingGatewayEvents(
  handlers: GatewayEventHandlers,
): Promise<UnlistenFn> {
  const registrations = [
    handlers.runtime
      ? listen<GatewayRuntime>("ai-routing-gateway-runtime", (event) => handlers.runtime?.(event.payload))
      : null,
    handlers.account
      ? listen<GatewayAccountEvent>("ai-routing-gateway-account", (event) => handlers.account?.(event.payload))
      : null,
    handlers.oauth
      ? listen<GatewayOAuthEvent>("ai-routing-gateway-oauth", (event) => handlers.oauth?.(event.payload))
      : null,
  ];
  const settled = await Promise.allSettled(registrations);
  const listeners = settled.flatMap((result) =>
    result.status === "fulfilled" && result.value ? [result.value] : [],
  );
  const failure = settled.find((result): result is PromiseRejectedResult => result.status === "rejected");
  if (failure) {
    for (const unlisten of listeners) {
      try {
        unlisten();
      } catch {
        // 保留最初的 listener 注册错误。
      }
    }
    throw failure.reason;
  }
  return () => {
    for (const unlisten of listeners) unlisten?.();
  };
}
