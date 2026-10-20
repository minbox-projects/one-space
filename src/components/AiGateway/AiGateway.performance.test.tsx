/**
 * Dedicated AI Gateway UI work-count measurements.
 *
 * These are deterministic, measured work counts and jsdom proxy durations, not
 * native GUI acceptance timings. The Test-owned runner executes this file alone
 * and parses the `AGW_UI_METRIC` lines plus the one `AGW_UI_RESOURCE` line.
 *
 * Counts are recorded, never prescribed: an optimized implementation may change
 * any count without failing this file. Behavior assertions keep the equivalent
 * interactions honest (all tabs, explicit provider history, hidden ticks,
 * configuration bursts and the complete 3,650-day calendar).
 */
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterAll, afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { AiGateway } from "@/components/AiGateway";
import {
  AI_GATEWAY_CONFIG_UPDATED_EVENT,
  type GatewayConfig,
  type GatewayModelPool,
  type GatewayProviderKey,
  type GatewayStatus,
  type GatewayTerminalTarget,
  type GatewayUpstreamProvider,
  type ProviderDailyEntry,
  type ProviderUsageHistory,
  type UsageLogRecord,
  type UsageLogsPage,
  type UsageMetrics,
  type UsageModelBreakdown,
  type UsageStats,
} from "@/lib/aiGateway";
import { setNativeWindowVisible } from "@/lib/runtimeStatus";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, listenMock, resetTauriMocks } from "@/test/mocks/tauri";

type Store = {
  config: GatewayConfig;
  status: GatewayStatus;
  targets: GatewayTerminalTarget[];
  poolStatus: unknown[];
};

type ProviderWithKeys = GatewayUpstreamProvider & { keys: GatewayProviderKey[] };

const PROVIDERS = 50;
const MAPPINGS_PER_PROVIDER = 10;
const MODELS = 200;
const HISTORY_DAYS = 3_650;
const END_DAY_UTC = Date.UTC(2026, 8, 17);

function key(id: string, value: string): GatewayProviderKey {
  return {
    id,
    name: id,
    value,
    enabled: true,
    auto_marked: false,
    failure_kind: null,
    marked_at: null,
    reason: null,
  };
}

function dayString(offsetDays: number): string {
  return new Date(END_DAY_UTC - offsetDays * 86_400_000)
    .toISOString()
    .slice(0, 10);
}

function largeConfig(): GatewayConfig {
  const providers: ProviderWithKeys[] = Array.from(
    { length: PROVIDERS },
    (_, providerIndex) => ({
      id: `p${providerIndex}`,
      name: `Provider ${providerIndex}`,
      base_url: `https://api${providerIndex}.example/v1`,
      protocol: "chat_completions",
      enabled: true,
      default_model: null,
      keys: Array.from({ length: 4 }, (_, keyIndex) =>
        key(`k${providerIndex}-${keyIndex}`, `SAFE_FIXTURE_key_${providerIndex}_${keyIndex}`),
      ),
      mappings: Array.from({ length: MAPPINGS_PER_PROVIDER }, (_, mappingIndex) => ({
        local_model: `model-${providerIndex}-${mappingIndex}`,
        upstream_model: `upstream-${providerIndex}-${mappingIndex}`,
        protocol: null,
        display_name: null,
        enabled: true,
        reasoning_efforts: [],
        auto_disabled: false,
        disabled_reason: null,
        disabled_at: null,
        consecutive_failures: 0,
        last_error_at: null,
      })),
    }),
  );
  const modelPools: GatewayModelPool[] = Array.from({ length: 10 }, (_, poolIndex) => ({
    id: `pool-${poolIndex}`,
    name: `Pool ${poolIndex}`,
    model: `pool-model-${poolIndex}`,
    enabled: true,
    default_reasoning_effort: "auto",
    candidates: Array.from({ length: 20 }, (_, candidateIndex) => ({
      provider_id: `p${candidateIndex}`,
      local_model: `model-${candidateIndex}-0`,
      upstream_model: `upstream-${candidateIndex}-0`,
      protocol: "chat_completions",
      weight: 1,
    })),
  }));
  return {
    enabled: true,
    port: 17688,
    providers,
    keys: [
      {
        id: "local",
        label: "Local",
        value: "SAFE_FIXTURE_local_key",
        enabled: true,
        created_at: 1,
      },
    ],
    default_key_id: "local",
    terminal_syncs: [],
    model_pools: modelPools,
  };
}

function largeStatus(): GatewayStatus {
  return {
    running: true,
    enabled: true,
    port: 17688,
    local_base_url: "http://127.0.0.1:17688/v1",
    provider_count: PROVIDERS,
    auto_disabled_count: 0,
    key_count: PROVIDERS * 4,
    default_key_id: "local",
  };
}

function metrics(_index: number): UsageMetrics {
  return {
    request_count: 1,
    successful_request_count: 1,
    input_tokens: 10,
    cache_read_tokens: 1,
    cache_write_tokens: 0,
    output_tokens: 5,
    total_tokens: 16,
    amount: 0.001,
    unpriced_count: 0,
    cache_hit_tokens: 1,
    cache_eligible_tokens: 11,
    cache_hit_rate_percent: 9.09,
    cache_rate_eligible_count: 1,
  };
}

function largeStats(): UsageStats {
  const models: UsageModelBreakdown[] = Array.from(
    { length: MODELS },
    (_, index) => ({
      ...metrics(index),
      local_model: `model-${index}`,
      providers: [
        {
          ...metrics(index),
          provider_id: `p${index % PROVIDERS}`,
          provider_name: `Provider ${index % PROVIDERS}`,
        },
      ],
    }),
  );
  return {
    ...metrics(0),
    granularity: "hour",
    buckets: Array.from({ length: 24 }, (_, index) => ({
      ...metrics(index),
      label: `${String(index).padStart(2, "0")}:00`,
    })),
    models,
    unpriced_items: [],
  };
}

function logRecord(index: number): UsageLogRecord {
  return {
    timestamp_ms: 1_700_000_000_000 + index,
    local_model: `model-${index % MODELS}`,
    upstream_model: `upstream-${index % MODELS}`,
    provider_id: `p${index % PROVIDERS}`,
    provider_name: `Provider ${index % PROVIDERS}`,
    result: "success",
    status: 200,
    input_tokens: 10,
    cache_read_tokens: 1,
    cache_write_tokens: 0,
    output_tokens: 5,
    total_tokens: 16,
    amount: 0.001,
    cost_breakdown: null,
    duration_ms: 5,
    first_token_ms: 2,
    avg_tps: 10,
    error_message: null,
    terminal: true,
    reasoning_effort: null,
    resolved_reasoning_effort: null,
    native_reasoning: null,
  };
}

function logsPage(): UsageLogsPage {
  return {
    page: 1,
    page_size: 50,
    total: 1_000_000,
    total_pages: 20_000,
    group_by: null,
    records: Array.from({ length: 50 }, (_, index) => logRecord(index)),
    groups: [],
    models: Array.from({ length: MODELS }, (_, index) => `model-${index}`),
    providers: Array.from({ length: PROVIDERS }, (_, index) => `Provider ${index}`),
  };
}

function providerHistory(): ProviderUsageHistory {
  const days: ProviderDailyEntry[] = Array.from({ length: HISTORY_DAYS }, (_, index) => {
    if (index >= 3641 && index <= 3648) {
      return {
        day: dayString(index),
        coverage: "unavailable",
        ongoing: false,
        metrics: null,
      } as ProviderDailyEntry;
    }
    if (index === HISTORY_DAYS - 1) {
      return {
        day: dayString(index),
        coverage: "retained_historical",
        ongoing: false,
        metrics: metrics(index),
      } as ProviderDailyEntry;
    }
    return {
      day: dayString(index),
      coverage: "continuous",
      ongoing: index === 0,
      metrics: index % 97 === 0 ? {
        ...metrics(index),
        request_count: 0,
        successful_request_count: 0,
        total_tokens: 0,
        amount: 0,
      } : metrics(index),
    } as ProviderDailyEntry;
  });
  return {
    provider_id: "p0",
    earliest_available_day: dayString(HISTORY_DAYS - 1),
    continuity_start_day: dayString(3640),
    end_day: dayString(0),
    cumulative: metrics(0),
    days,
  };
}

function terminalTargets(): GatewayTerminalTarget[] {
  return [
    {
      provider_id: "gw-open",
      tool: "opencode",
      name: "OpenCode",
      base_url: "http://127.0.0.1:17688/v1",
      synced: true,
      pending_sync: false,
      synced_key_id: "local",
      synced_at: 1,
    },
    {
      provider_id: "gw-codex",
      tool: "codex",
      name: "Codex",
      base_url: null,
      synced: false,
      pending_sync: true,
      synced_key_id: null,
      synced_at: null,
    },
    {
      provider_id: "gw-claude",
      tool: "claude",
      name: "Claude Code",
      base_url: null,
      synced: false,
      pending_sync: true,
      synced_key_id: null,
      synced_at: null,
    },
  ];
}

function store(): Store {
  return {
    config: largeConfig(),
    status: largeStatus(),
    targets: terminalTargets(),
    poolStatus: Array.from({ length: 10 }, (_, poolIndex) => ({
      id: `pool-${poolIndex}`,
      name: `Pool ${poolIndex}`,
      model: `pool-model-${poolIndex}`,
      enabled: true,
      default_reasoning_effort: "auto",
      reasoning_efforts: [],
      reasoning_coverage: [],
      candidates: Array.from({ length: 20 }, (_, candidateIndex) => ({
        provider_id: `p${candidateIndex}`,
        provider_name: `Provider ${candidateIndex}`,
        local_model: `model-${candidateIndex}-0`,
        upstream_model: `upstream-${candidateIndex}-0`,
        protocol: "chat_completions",
        weight: 1,
        ready: true,
        availability: "ready",
        reason: null,
        cooling_until: null,
      })),
    })),
  };
}

function installMocks(current: Store) {
  invokeMock.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    switch (command) {
      case "ai_gateway_get_config":
        return current.config;
      case "ai_gateway_status":
        return current.status;
      case "ai_gateway_terminal_targets":
        return current.targets;
      case "ai_gateway_model_pools_status":
        return current.poolStatus;
      case "ai_gateway_provider_templates":
        return [];
      case "ai_gateway_template_auto_refresh_get":
        return { enabled: true, minutes: 60, last_run_at: null, next_run_at: null };
      case "ai_gateway_usage_stats":
        return largeStats();
      case "ai_gateway_request_logs":
        return logsPage();
      case "ai_gateway_usage_providers":
        return Array.from({ length: PROVIDERS }, (_, index) => ({
          provider_id: `p${index}`,
          provider_name: `Provider ${index}`,
          deleted: false,
          has_usage: true,
        }));
      case "ai_gateway_provider_usage_history":
        expect(args).toEqual({ providerId: "p0" });
        return providerHistory();
      case "ai_gateway_usage_retention_get":
        return 90;
      case "ai_gateway_upsert_provider":
      case "ai_gateway_upsert_key":
      case "ai_gateway_delete_provider":
      case "ai_gateway_sync_terminal":
      case "ai_gateway_configure_terminal":
        return current.config;
      default:
        return null;
    }
  });
}

function callCount(command: string) {
  return invokeMock.mock.calls.filter(([name]) => name === command).length;
}

function totalCalls() {
  return invokeMock.mock.calls.length;
}

function measuredCounters(): Record<string, number> {
  return {
    invoke_calls: totalCalls(),
    config_calls: callCount("ai_gateway_get_config"),
    status_calls: callCount("ai_gateway_status"),
    usage_stats_calls: callCount("ai_gateway_usage_stats"),
    request_logs_calls: callCount("ai_gateway_request_logs"),
    provider_history_calls: callCount("ai_gateway_provider_usage_history"),
    usage_providers_calls: callCount("ai_gateway_usage_providers"),
    pool_status_calls: callCount("ai_gateway_model_pools_status"),
  };
}

function emitUiMetric(payload: Record<string, unknown>) {
  process.stdout.write(`AGW_UI_METRIC ${JSON.stringify(payload)}\n`);
}

function emitUiResource() {
  const usage = process.resourceUsage();
  process.stdout.write(
    `AGW_UI_RESOURCE ${JSON.stringify({
      cpu_ms: (usage.userCPUTime + usage.systemCPUTime) / 1000,
      peak_rss_bytes: usage.maxRSS * 1024,
      scope: "vitest-worker",
      includes_preparation: true,
    })}\n`,
  );
}

async function flush() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

async function mountConsole() {
  const rendered = renderWithProviders(<AiGateway />);
  await screen.findByTestId("ai-gateway-console");
  await waitFor(() => expect(totalCalls()).toBeGreaterThan(0));
  await flush();
  return rendered;
}

async function visitAllTabs() {
  const { unmount } = await mountConsole();
  const tabs = screen.getAllByRole("tab");
  expect(tabs.length).toBeGreaterThanOrEqual(6);
  for (const tab of tabs.slice(0, 6)) {
    fireEvent.click(tab);
    await flush();
  }
  fireEvent.click(tabs[5] ?? tabs[tabs.length - 1]);
  await flush();
  for (const testId of [
    "ai-gateway-subtab-usage-stats",
    "ai-gateway-subtab-usage-logs",
    "ai-gateway-subtab-usage-history",
  ]) {
    fireEvent.click(screen.getByTestId(testId));
    await flush();
  }
  expect(screen.getByTestId("ai-gateway-console")).toBeInTheDocument();
  const locale = i18n.language === "zh" ? "zh" : "en";
  emitUiMetric({
    case: locale === "zh" ? "ui_all_tabs_zh" : "ui_all_tabs_en",
    locale,
    phase: passPhase.current,
    pass: passPhase.currentPass,
    operations: 1,
    counters: measuredCounters(),
    duration_ms: 0,
    proxy: true,
    dimensions: { locale, fixture: "gateway-large-v1", tabs: 6, subtabs: 3 },
  });
  unmount();
}

async function runVisibleTicks() {
  vi.useFakeTimers();
  installMocks(store());
  const { unmount } = renderWithProviders(<AiGateway />);
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
  await act(async () => {
    await vi.advanceTimersByTimeAsync(0);
  });
  invokeMock.mockClear();
  for (let tick = 0; tick < 30; tick += 1) {
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5_000);
    });
  }
  const counters: Record<string, number> = { ...measuredCounters(), ticks: 30 };
  expect(counters.usage_stats_calls).toBeGreaterThan(0);
  emitUiMetric({
    case: "ui_usage_ticks_visible",
    locale: "en",
    phase: passPhase.current,
    pass: passPhase.currentPass,
    operations: 30,
    counters,
    duration_ms: 0,
    proxy: true,
    dimensions: { locale: "en", fixture: "gateway-large-v1", ticks: 30 },
  });
  unmount();
}

async function runHiddenTicks() {
  vi.useFakeTimers();
  installMocks(store());
  const { unmount } = renderWithProviders(<AiGateway />);
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
  await act(async () => {
    await vi.advanceTimersByTimeAsync(0);
  });
  await act(async () => {
    setNativeWindowVisible(false);
  });
  invokeMock.mockClear();
  for (let tick = 0; tick < 30; tick += 1) {
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5_000);
    });
  }
  const hiddenCalls = totalCalls();
  expect(hiddenCalls).toBe(0);
  await act(async () => {
    setNativeWindowVisible(true);
  });
  await act(async () => {
    await vi.advanceTimersByTimeAsync(0);
  });
  const catchUpStats = callCount("ai_gateway_usage_stats");
  expect(catchUpStats).toBeGreaterThanOrEqual(1);
  emitUiMetric({
    case: "ui_hidden_ticks",
    locale: "en",
    phase: passPhase.current,
    pass: passPhase.currentPass,
    operations: 30,
    counters: {
      invoke_calls: hiddenCalls,
      catch_up_usage_stats_calls: catchUpStats,
      ticks: 30,
    },
    duration_ms: 0,
    proxy: true,
    dimensions: { locale: "en", fixture: "gateway-large-v1", ticks: 30 },
  });
  unmount();
}

async function runConfigBurst() {
  installMocks(store());
  const { unmount } = await mountConsole();
  invokeMock.mockClear();
  const configHandler = listenMock.mock.calls.find(
    ([event]) => event === AI_GATEWAY_CONFIG_UPDATED_EVENT,
  )?.[1] as ((event: { payload?: unknown }) => void) | undefined;
  expect(configHandler).toBeTypeOf("function");
  await act(async () => {
    for (let index = 0; index < 10; index += 1) {
      configHandler?.({ payload: undefined });
    }
  });
  await flush();
  await flush();
  const counters: Record<string, number> = { ...measuredCounters(), events: 10 };
  expect(counters.config_calls).toBeGreaterThan(0);
  expect(counters.usage_stats_calls).toBe(0);
  expect(counters.request_logs_calls).toBe(0);
  emitUiMetric({
    case: "ui_config_burst",
    locale: "en",
    phase: passPhase.current,
    pass: passPhase.currentPass,
    operations: 10,
    counters,
    duration_ms: 0,
    proxy: true,
    dimensions: { locale: "en", fixture: "gateway-large-v1", events: 10 },
  });
  unmount();
}

async function runFullHistory() {
  installMocks(store());
  const { unmount } = await mountConsole();
  const tabs = screen.getAllByRole("tab");
  fireEvent.click(tabs[5] ?? tabs[tabs.length - 1]);
  await flush();
  fireEvent.click(screen.getByTestId("ai-gateway-subtab-usage-history"));
  const panel = within(await screen.findByTestId("ai-gateway-provider-usage-history"));

  fireEvent.click(panel.getByTestId("ai-gateway-provider-history-provider-trigger"));
  fireEvent.click(panel.getByRole("option", { name: /Provider 0/ }));

  await waitFor(() =>
    expect(panel.getByTestId("ai-gateway-provider-history-days")).toBeInTheDocument(),
  );
  const rows = panel.getAllByTestId(/^ai-gateway-provider-history-day-\d{4}-\d{2}-\d{2}$/);
  expect(rows).toHaveLength(HISTORY_DAYS);
  expect(panel.getByTestId(`ai-gateway-provider-history-day-${dayString(HISTORY_DAYS - 1)}`)).toBeInTheDocument();
  expect(panel.getByTestId(`ai-gateway-provider-history-day-${dayString(0)}`)).toBeInTheDocument();

  const unavailable = panel
    .getAllByTestId(/^ai-gateway-provider-history-day-\d{4}-\d{2}-\d{2}$/)
    .filter((row) => row.getAttribute("data-coverage") === "unavailable");
  expect(unavailable).toHaveLength(8);
  expect(panel.getByTestId(`ai-gateway-provider-history-day-${dayString(0)}`)).toHaveAttribute(
    "data-ongoing",
    "true",
  );

  const counters: Record<string, number> = {
    ...measuredCounters(),
    history_days_rendered: rows.length,
    unavailable_rows: unavailable.length,
  };
  expect(counters.provider_history_calls).toBe(1);
  emitUiMetric({
    case: "ui_history_3650",
    locale: "en",
    phase: passPhase.current,
    pass: passPhase.currentPass,
    operations: 1,
    counters,
    duration_ms: 0,
    proxy: true,
    dimensions: {
      locale: "en",
      fixture: "gateway-large-v1",
      history_days: HISTORY_DAYS,
      end_day: dayString(0),
      earliest_day: dayString(HISTORY_DAYS - 1),
    },
  });
  unmount();
}

const passPhase = { current: "cold" as "cold" | "warm", currentPass: 0 };

async function runCase(
  name: string,
  interaction: () => Promise<void>,
  dimensions: Record<string, unknown>,
) {
  for (let pass = 0; pass <= 5; pass += 1) {
    passPhase.current = pass === 0 ? "cold" : "warm";
    passPhase.currentPass = pass;
    resetTauriMocks();
    installMocks(store());
    invokeMock.mockClear();
    await i18n.changeLanguage(name.endsWith("_zh") ? "zh" : "en");
    const started = process.uptime() * 1000;
    await interaction();
    const duration = process.uptime() * 1000 - started;
    vi.useRealTimers();
    // Replace the placeholder duration emitted by the interaction with the
    // actual measured jsdom proxy duration. Bypass the capture override.
    const lastMetric = latestMetric;
    expect(lastMetric, `${name} pass ${pass} emitted a metric`).not.toBeNull();
    originalWrite(
      `AGW_UI_METRIC ${JSON.stringify({
        ...lastMetric,
        duration_ms: Number(duration.toFixed(3)),
        dimensions: { ...dimensions, ...(lastMetric?.dimensions ?? {}) },
      })}\n`,
    );
    latestMetric = null;
  }
}

let latestMetric: Record<string, unknown> | null = null;
const originalWrite = process.stdout.write.bind(process.stdout);
process.stdout.write = ((chunk: string | Uint8Array, ...rest: unknown[]) => {
  if (typeof chunk === "string" && chunk.startsWith("AGW_UI_METRIC ")) {
    latestMetric = JSON.parse(chunk.slice("AGW_UI_METRIC ".length));
    return true;
  }
  return originalWrite(chunk as never, ...(rest as never[]));
}) as typeof process.stdout.write;

describe("AiGateway performance measurements", () => {
  beforeEach(async () => {
    resetTauriMocks();
    latestMetric = null;
    await i18n.changeLanguage("en");
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  afterAll(() => {
    emitUiResource();
    process.stdout.write = originalWrite;
  });

  it("measures all tabs in English", async () => {
    await runCase("ui_all_tabs_en", visitAllTabs, { locale: "en", tabs: 6, subtabs: 3 });
  }, 180_000);

  it("measures all tabs in Chinese", async () => {
    await runCase("ui_all_tabs_zh", visitAllTabs, { locale: "zh", tabs: 6, subtabs: 3 });
  }, 180_000);

  it("measures thirty visible five-second ticks", async () => {
    await runCase("ui_usage_ticks_visible", runVisibleTicks, {
      locale: "en",
      ticks: 30,
      interval_ms: 5000,
    });
  }, 180_000);

  it("measures thirty hidden ticks and the visible catch-up", async () => {
    await runCase("ui_hidden_ticks", runHiddenTicks, {
      locale: "en",
      ticks: 30,
      interval_ms: 5000,
    });
  }, 180_000);

  it("measures a ten-event configuration burst", async () => {
    await runCase("ui_config_burst", runConfigBurst, {
      locale: "en",
      events: 10,
    });
  }, 180_000);

  it("measures the complete 3,650-day provider calendar", async () => {
    await runCase("ui_history_3650", runFullHistory, {
      locale: "en",
      history_days: HISTORY_DAYS,
    });
  }, 180_000);
});
