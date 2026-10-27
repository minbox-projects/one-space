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


export interface ContextWindowPresetItem {
  label: string;
  tokens: number;
}


export interface AggregateContextResult {
  /** Complete conservative context window across all unique sources; null if any source has unknown context or no sources exist. */
  conservative: number | null;
  /** Minimum context window among all sources that actually have a declared context window; null if no source has declared one. */
  known: number | null;
  /** True if there is at least one source without a declared context window (or no sources). */
  isIncomplete: boolean;
  /** Total number of unique sources. */
  totalCount: number;
  /** Number of sources with known context window. */
  knownCount: number;
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


/** Quick time ranges shared by the usage-stats and request-logs tabs. */
export type UsageRangeKey = "today" | "yesterday" | "7d" | "15d" | "30d" | "all";


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


/** One template whose latest backend automatic refresh failed, with its reason. */
export interface TemplateAutoRefreshFailure {
  template_id: string;
  reason: string;
}


/** Backend-owned automatic-refresh failure snapshot. */
export interface TemplateAutoRefreshStatus {
  failures: TemplateAutoRefreshFailure[];
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


/** One pool's configuration-derived static reasoning catalog. */
export interface ModelPoolReasoningCatalog {
  reasoning_efforts: string[];
  default_reasoning_effort: string;
  reasoning_coverage: ModelPoolReasoningCoverage[];
}


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


/** Payload for creating one upstream provider from a template. */
export interface CreateProviderFromTemplateRequest {
  templateId: string;
  name: string;
  baseUrl: string;
  protocol: GatewayUpstreamProtocol;
  apiKey: string;
}
