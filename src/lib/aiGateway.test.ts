import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  AI_GATEWAY_DEFAULT_PORT,
  AI_GATEWAY_KEY_AUTH_FAILED_EVENT,
  AI_GATEWAY_KEY_AUTH_TOAST_WINDOW_MS,
  aggregateModels,
  parseContextWindowInput,
  formatContextWindowCompact,
  formatContextWindowDisplay,
  resolvePoolContextWindow,
  CONTEXT_WINDOW_PRESETS,
  DEFAULT_GATEWAY_SETTINGS,
  aiGatewayConfigureTerminal,
  aiGatewayDeleteKey,
  aiGatewayDeleteProvider,
  aiGatewayGetConfig,
  aiGatewayProviderGoUsage,
  aiGatewayProviderQuota,
  aiGatewayReenableProviderModel,
  aiGatewayReenableProviderModels,
  aiGatewaySetDefaultKey,
  aiGatewaySetProviderEnabled,
  aiGatewayStart,
  aiGatewayStatus,
  aiGatewayStop,
  aiGatewaySyncTerminal,
  aiGatewayTemplateAutoRefreshGet,
  aiGatewayTemplateAutoRefreshSave,
  aiGatewayTerminalTargets,
  aiGatewayUpsertKey,
  aiGatewayUpsertProvider,
  aiGatewayRequestLogs,
  aiGatewayUsageRetentionGet,
  aiGatewayUsageRetentionSave,
  aiGatewayUsageStats,
  clampUsagePage,
  formatGatewayTimestamp,
  formatGatewayTokens,
  formatCacheTokensK,
  formatGatewayDuration,
  getGatewayDurationColorClass,
  formatUsageAmount,
  formatUsageCurrency,
  formatUsageGroupLabel,
  formatUsageRowAmount,
  formatUtc8DateTime,
  formatUtc8Day,
  formatUtc8Hour,
  isUnpricedOnly,
  localAnthropicMessagesUrl,
  localBaseUrl,
  maskSecret,
  normalizeGatewaySettings,
  notifyTemplateAutoRefreshIntervalChanged,
  providerMarkedKeys,
  resolveAggregatedModelName,
  autoBindCandidateReasoning,
  CANONICAL_REASONING_EFFORTS,
  resolveAggregatedReasoningEfforts,
  resolveDefaultKeyId,
  subscribeTemplateAutoRefreshIntervalChanged,
  USAGE_RANGE_KEYS,
  usageStatusTranslationKey,
  type ClaudeModelSelection,
  type GatewayConfig,
  type GatewayKey,
  type GatewayModelPool,
  type GatewayProviderKey,
  type GatewaySettings,
  type GatewayUpstreamProtocol,
  type GatewayUpstreamProvider,
  type ModelPrice,
  type UsageLogsPage,
  type UsageMetrics,
  type UsageStats,
} from "@/lib/aiGateway";
// Namespace import for the Step 2 latency helpers: the module is always loaded,
// so a missing named export fails only its own callable assertion instead of
// aborting the whole file and hiding the rendered behavior RED.
import * as aiGatewayModule from "@/lib/aiGateway";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

function key(overrides: Partial<GatewayKey> = {}): GatewayKey {
  return {
    id: "k1",
    label: "Main",
    value: "secret",
    enabled: true,
    created_at: 1,
    ...overrides,
  };
}

/**
 * Raw ordered key-pool entry, built as a shape-only record so this suite keeps
 * compiling against the pre-Step-4 `GatewayUpstreamProvider` type while
 * pinning the delivered backend wire contract (field order included).
 */
type RawProviderKey = Record<string, unknown>;

function providerKey(overrides: RawProviderKey = {}): RawProviderKey {
  return {
    id: "k1",
    name: "Default",
    value: "sk-provider-key",
    enabled: true,
    daily_reset: false,
    auto_marked: false,
    failure_kind: null,
    marked_at: null,
    reason: null,
    ...overrides,
  };
}

function provider(
  overrides: Partial<GatewayUpstreamProvider> & { keys?: RawProviderKey[] } = {},
): GatewayUpstreamProvider {
  const { keys, ...rest } = overrides;
  return {
    id: "p1",
    name: "Provider",
    base_url: "https://upstream.example",
    keys: keys ?? [providerKey()],
    default_model: null,
    mappings: [],
    enabled: true,
    ...rest,
  } as unknown as GatewayUpstreamProvider;
}

function config(overrides: Partial<GatewayConfig> = {}): GatewayConfig {
  return {
    enabled: false,
    port: AI_GATEWAY_DEFAULT_PORT,
    providers: [],
    keys: [],
    default_key_id: null,
    terminal_syncs: [],
    ...overrides,
  };
}

describe("aiGateway 命令封装", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  it("按逐字命令名与 camelCase 参数调用配置读写", async () => {
    await aiGatewayGetConfig();
    await aiGatewayUpsertProvider(provider());
    await aiGatewayDeleteProvider("p1");
    await aiGatewaySetProviderEnabled("p1", false);

    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_get_config");
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_upsert_provider", {
      provider: provider(),
      prices: null,
    });
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_delete_provider", {
      providerId: "p1",
    });
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_set_provider_enabled", {
      providerId: "p1",
      enabled: false,
    });
    expect(invokeMock).not.toHaveBeenCalledWith(
      "ai_gateway_reenable_provider",
      expect.anything(),
    );
  });

  it("按 camelCase 参数调用 Key、启停与终端同步命令", async () => {
    await aiGatewayUpsertKey(key());
    await aiGatewayDeleteKey("k1");
    await aiGatewaySetDefaultKey("k1");
    await aiGatewayStart();
    await aiGatewayStop();
    await aiGatewayStatus();
    await aiGatewayTerminalTargets();
    await aiGatewayConfigureTerminal(["t-open"]);
    await aiGatewaySyncTerminal(["t-open"]);

    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_upsert_key", {
      key: key(),
    });
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_delete_key", {
      keyId: "k1",
    });
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_set_default_key", {
      keyId: "k1",
    });
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_start");
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_stop");
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_status");
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_terminal_targets");
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_configure_terminal", {
      targetTools: ["t-open"],
    });
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_sync_terminal", {
      targetTools: ["t-open"],
    });
  });

  it("逐行与批量重新启用命令使用新的独立包装函数", async () => {
    await aiGatewayReenableProviderModel("p1", "gpt-4o", "gpt-4o-2024");
    expect(invokeMock).toHaveBeenCalledWith(
      "ai_gateway_reenable_provider_model",
      {
        providerId: "p1",
        localModel: "gpt-4o",
        upstreamModel: "gpt-4o-2024",
      },
    );

    resetTauriMocks();
    await aiGatewayReenableProviderModels("p1");
    expect(invokeMock).toHaveBeenCalledWith(
      "ai_gateway_reenable_provider_models",
      {
        providerId: "p1",
      },
    );
  });

  it("未指定目标时同步命令不携带目标载荷", async () => {
    await aiGatewaySyncTerminal();
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_sync_terminal", {});
  });
});

describe("resolveDefaultKeyId 默认 Key 解析", () => {
  it("列表为空时没有默认 Key", () => {
    expect(resolveDefaultKeyId([], null)).toBeNull();
  });

  it("未手动指定时取第一个启用项", () => {
    const keys = [
      key({ id: "k1", enabled: false }),
      key({ id: "k2", enabled: true }),
      key({ id: "k3", enabled: true }),
    ];
    expect(resolveDefaultKeyId(keys, null)).toBe("k2");
  });

  it("手动指定的启用 Key 优先", () => {
    const keys = [key({ id: "k1" }), key({ id: "k2" })];
    expect(resolveDefaultKeyId(keys, "k2")).toBe("k2");
  });

  it("当前默认被禁用时顺延到下一个启用项", () => {
    const keys = [
      key({ id: "k1", enabled: false }),
      key({ id: "k2", enabled: true }),
      key({ id: "k3", enabled: true }),
    ];
    expect(resolveDefaultKeyId(keys, "k1")).toBe("k2");
  });

  it("顺延时环形回绕到列表开头之后的启用项", () => {
    const keys = [
      key({ id: "k1", enabled: true }),
      key({ id: "k2", enabled: false }),
      key({ id: "k3", enabled: false }),
    ];
    expect(resolveDefaultKeyId(keys, "k2")).toBe("k1");
  });

  it("指定的 id 不存在时回到第一个启用项", () => {
    const keys = [key({ id: "k1", enabled: false }), key({ id: "k2", enabled: true })];
    expect(resolveDefaultKeyId(keys, "missing")).toBe("k2");
  });

  it("没有启用项时默认 Key 为空", () => {
    const keys = [key({ id: "k1", enabled: false }), key({ id: "k2", enabled: false })];
    expect(resolveDefaultKeyId(keys, "k1")).toBeNull();
  });
});

describe("normalizeGatewaySettings 统一网络代理后的字段收敛", () => {
  it("忽略遗留 proxy_url 并保留所有其他字段", () => {
    const legacySettings = {
      bind_address: "0.0.0.0",
      enable_cors: false,
      require_api_key_for_lan: false,
      connect_timeout_secs: 20,
      read_timeout_secs: 90,
      request_budget_secs: 240,
      max_retries_per_provider: 5,
      retry_base_delay_ms: 1500,
      failure_threshold: 4,
      probe_cooldown_secs: 120,
      key_quota_ttl_secs: 600,
      session_affinity_timeout_secs: 900,
      context_hold_timeout_secs: 120,
      custom_session_headers: ["x-session-id", "x-trace-id"],
      daily_reset_offset_hours: 0,
      quota_cache_ttl_secs: 30,
      debug_logging: true,
      proxy_url: "socks5://127.0.0.1:7890",
    } as unknown as Partial<GatewaySettings>;

    const normalized = normalizeGatewaySettings(legacySettings);

    expect(normalized).not.toHaveProperty("proxy_url");
    expect(normalized).toEqual({
      bind_address: "0.0.0.0",
      enable_cors: false,
      require_api_key_for_lan: false,
      connect_timeout_secs: 20,
      read_timeout_secs: 90,
      request_budget_secs: 240,
      max_retries_per_provider: 5,
      retry_base_delay_ms: 1500,
      failure_threshold: 4,
      probe_cooldown_secs: 120,
      key_quota_ttl_secs: 600,
      session_affinity_timeout_secs: 900,
      context_hold_timeout_secs: 120,
      custom_session_headers: ["x-session-id", "x-trace-id"],
      daily_reset_offset_hours: 0,
      quota_cache_ttl_secs: 30,
      debug_logging: true,
    });
  });

  it("缺失或仅携带遗留 proxy_url 时回退全部默认值", () => {
    expect(normalizeGatewaySettings(undefined)).toEqual(
      DEFAULT_GATEWAY_SETTINGS,
    );
    expect(normalizeGatewaySettings(null)).toEqual(DEFAULT_GATEWAY_SETTINGS);

    const onlyLegacy = normalizeGatewaySettings({
      proxy_url: "http://127.0.0.1:8080",
    } as unknown as Partial<GatewaySettings>);

    expect(onlyLegacy).toEqual(DEFAULT_GATEWAY_SETTINGS);
    expect(onlyLegacy).not.toHaveProperty("proxy_url");
  });
});

describe("localBaseUrl 本地 Api 地址", () => {
  it("固定绑定回环地址与端口", () => {
    expect(localBaseUrl(AI_GATEWAY_DEFAULT_PORT)).toBe(
      `http://127.0.0.1:${AI_GATEWAY_DEFAULT_PORT}/v1`,
    );
    expect(localBaseUrl(18000)).toBe("http://127.0.0.1:18000/v1");
  });
});

describe("localAnthropicMessagesUrl 本地 Anthropic Messages 地址", () => {
  it("固定绑定回环地址、端口与 /v1/messages 路径", () => {
    expect(localAnthropicMessagesUrl(AI_GATEWAY_DEFAULT_PORT)).toBe(
      `http://127.0.0.1:${AI_GATEWAY_DEFAULT_PORT}/v1/messages`,
    );
    expect(localAnthropicMessagesUrl(18000)).toBe(
      "http://127.0.0.1:18000/v1/messages",
    );
  });
});

describe("maskSecret 掩码", () => {
  it("为空时返回空字符串", () => {
    expect(maskSecret("")).toBe("");
  });

  it("短值整体掩码", () => {
    expect(maskSecret("abc")).toBe("•••");
  });

  it("长值保留头尾便于识别且不含完整明文", () => {
    const masked = maskSecret("sk-live-secret-1234");
    expect(masked).toContain("sk-");
    expect(masked).toContain("1234");
    expect(masked).not.toBe("sk-live-secret-1234");
    expect(masked).not.toContain("live-secret");
  });
});

describe("formatGatewayTimestamp", () => {
  it("空值返回 null", () => {
    expect(formatGatewayTimestamp(null)).toBeNull();
  });

  it("以稳定格式展示时间", () => {
    const formatted = formatGatewayTimestamp(1_700_000_000);
    expect(formatted).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/);
  });
});

function metrics(overrides: Partial<UsageMetrics> = {}): UsageMetrics {
  return {
    request_count: 0,
    input_tokens: 0,
    cache_read_tokens: 0,
    cache_write_tokens: 0,
    output_tokens: 0,
    total_tokens: 0,
    amount: 0,
    unpriced_count: 0,
    cache_hit_tokens: 0,
    cache_eligible_tokens: 0,
    cache_hit_rate_percent: null,
    cache_rate_eligible_count: 0,
    successful_request_count: 0,
    ...overrides,
  };
}

describe("USAGE_RANGE_KEYS 快捷范围顺序", () => {
  it("按今日、昨天、近 N 天与全部的顺序暴露快捷范围", () => {
    expect(USAGE_RANGE_KEYS).toEqual([
      "today",
      "yesterday",
      "7d",
      "15d",
      "30d",
      "all",
    ]);
  });
});

describe("UTC+8 时间标签", () => {
  it("跨 UTC 日界时按 UTC+8 展示日期与时间", () => {
    // 2026-09-17T18:30:00Z == 2026-09-18 02:30 (UTC+8)
    expect(formatUtc8DateTime(Date.UTC(2026, 8, 17, 18, 30))).toBe(
      "2026-09-18 02:30:00",
    );
  });

  it("提供日与小时标签", () => {
    const noonUtc = Date.UTC(2026, 8, 16, 12, 0);
    expect(formatUtc8Day(noonUtc)).toBe("2026-09-16");
    expect(formatUtc8Hour(noonUtc)).toBe("20:00");
  });

  it("空值返回 null", () => {
    expect(formatUtc8DateTime(null)).toBeNull();
    expect(formatUtc8Day(undefined)).toBeNull();
    expect(formatUtc8Hour(null)).toBeNull();
  });
});

describe("金额格式化与未定价判定", () => {
  it("保留 4 位小数", () => {
    expect(formatUsageAmount(1.234567)).toBe("1.2346");
    expect(formatUsageAmount(0)).toBe("0.0000");
  });

  it("无金额时显示破折号", () => {
    expect(formatUsageAmount(null)).toBe("—");
    expect(formatUsageAmount(undefined)).toBe("—");
  });

  it("金额带美元货币单位格式化", () => {
    expect(formatUsageCurrency(1.234567)).toBe("$1.2346");
    expect(formatUsageCurrency(0)).toBe("$0.0000");
    expect(formatUsageCurrency(null)).toBe("—");
    expect(formatUsageCurrency(undefined)).toBe("—");
  });

  it("全部请求未定价时判定为未定价行", () => {
    expect(
      isUnpricedOnly(metrics({ request_count: 3, unpriced_count: 3 })),
    ).toBe(true);
  });

  it("存在已定价请求时不算未定价行", () => {
    expect(
      isUnpricedOnly(metrics({ request_count: 3, unpriced_count: 2 })),
    ).toBe(false);
    expect(
      isUnpricedOnly(metrics({ request_count: 0, unpriced_count: 0 })),
    ).toBe(false);
  });

  it("未定价行展示破折号，部分定价行展示金额", () => {
    expect(
      formatUsageRowAmount(
        metrics({ request_count: 2, unpriced_count: 2, amount: 0 }),
      ),
    ).toBe("—");
    expect(
      formatUsageRowAmount(
        metrics({ request_count: 2, unpriced_count: 1, amount: 0.5 }),
      ),
    ).toBe("0.5000");
  });
});

describe("formatGatewayDuration 秒格式化", () => {
  it("空值、非有限数字或负数返回破折号", () => {
    expect(formatGatewayDuration(null)).toBe("—");
    expect(formatGatewayDuration(undefined)).toBe("—");
    expect(formatGatewayDuration(Number.NaN)).toBe("—");
    expect(formatGatewayDuration(Number.POSITIVE_INFINITY)).toBe("—");
    expect(formatGatewayDuration(Number.NEGATIVE_INFINITY)).toBe("—");
    expect(formatGatewayDuration(-1)).toBe("—");
  });

  it("按毫秒除以 1000，保留三位小数（单位由调用方单独渲染）", () => {
    expect(formatGatewayDuration(0)).toBe("0.000");
    expect(formatGatewayDuration(120)).toBe("0.120");
    expect(formatGatewayDuration(1000)).toBe("1.000");
    expect(formatGatewayDuration(5000)).toBe("5.000");
    expect(formatGatewayDuration(62000)).toBe("62.000");
    expect(formatGatewayDuration(3661000)).toBe("3661.000");
  });

  it("三地小数舍入与原始毫秒分辨率无关，跨阈值显示值可相同", () => {
    // 2.9996s 与 3.0000s 显示相同，但颜色区间按原始毫秒判定。
    expect(formatGatewayDuration(2999.6)).toBe("3.000");
    expect(formatGatewayDuration(3000)).toBe("3.000");
    expect(formatGatewayDuration(15000.4)).toBe("15.000");
  });
});

/**
 * Resolve a Step 2 helper through the namespace so a missing export produces a
 * bounded message on its own assertion rather than a module-load failure.
 */
function latencyHelper(name: string): (value: number | null | undefined) => unknown {
  const candidate = (aiGatewayModule as Record<string, unknown>)[name];
  expect(
    typeof candidate,
    `aiGateway 未导出可调用的 ${name}（Steps 2 公共 helper）`,
  ).toBe("function");
  return candidate as (value: number | null | undefined) => unknown;
}

const GREEN_TONE = /(emerald|green)/;
const ORANGE_TONE = /(amber|orange)/;
const RED_TONE = /(rose|red)/;

const TONE_FAMILIES = {
  green: GREEN_TONE,
  orange: ORANGE_TONE,
  red: RED_TONE,
} as const;

/**
 * REQ-004 / Step 2: an available metric color must use a readable light-theme
 * shade (shade number >= 500) and carry a matching `dark:` variant in the SAME
 * color family. Exact shade numbers are intentionally not pinned, so
 * `text-emerald-600 dark:text-emerald-400` and `text-emerald-500 dark:text-emerald-400`
 * both pass.
 */
function expectReadableColorFamily(
  color: unknown,
  family: keyof typeof TONE_FAMILIES,
) {
  expect(typeof color, "颜色 helper 应返回类名字符串").toBe("string");
  const raw = String(color);
  const classes = raw.split(/\s+/).filter(Boolean);
  const tone = TONE_FAMILIES[family];
  const lightShades = classes.filter(
    (cls) => /^text-[a-z]+-\d{2,3}$/.test(cls) && tone.test(cls),
  );
  expect(
    lightShades.some((cls) => Number(cls.split("-").pop()) >= 500),
    `${family} 家族应提供 >= 500 的亮色主题色阶，实际类名：${raw}`,
  ).toBe(true);
  const darkShades = classes.filter(
    (cls) => /^dark:text-[a-z]+-\d{2,3}$/.test(cls) && tone.test(cls),
  );
  expect(
    darkShades.length,
    `${family} 家族应提供同色系 dark: 变体，实际类名：${raw}`,
  ).toBeGreaterThan(0);
}

describe("AI 网关延迟指标格式化与颜色 helper", () => {
  it("formatGatewayTps 以两位小数展示每秒 token，非法值返回破折号", () => {
    const formatGatewayTps = latencyHelper("formatGatewayTps");
    expect(formatGatewayTps(null)).toBe("—");
    expect(formatGatewayTps(undefined)).toBe("—");
    expect(formatGatewayTps(Number.NaN)).toBe("—");
    expect(formatGatewayTps(Number.POSITIVE_INFINITY)).toBe("—");
    expect(formatGatewayTps(-1)).toBe("—");
    expect(formatGatewayTps(0)).toBe("0.00");
    expect(formatGatewayTps(20)).toBe("20.00");
    expect(formatGatewayTps(50)).toBe("50.00");
    expect(formatGatewayTps(59.999)).toBe("60.00");
    expect(formatGatewayTps(200)).toBe("200.00");
  });

  it("getGatewayFirstTokenColorClass 按原始首字毫秒独立分档（<1s 绿，1~3s 橙，>3s 红）", () => {
    const color = latencyHelper("getGatewayFirstTokenColorClass");
    expect(color(null)).toBe("text-muted-foreground");
    expect(color(undefined)).toBe("text-muted-foreground");
    expect(color(Number.NaN)).toBe("text-muted-foreground");
    expect(color(Number.POSITIVE_INFINITY)).toBe("text-muted-foreground");
    expect(color(-1)).toBe("text-muted-foreground");
    expect(color(0)).toMatch(GREEN_TONE);
    expect(color(999)).toMatch(GREEN_TONE);
    // 999.6ms 显示为 1.000s，但原始值仍 < 1000ms，必须取绿色。
    expect(color(999.6)).toMatch(GREEN_TONE);
    expect(color(1000)).toMatch(ORANGE_TONE);
    expect(color(3000)).toMatch(ORANGE_TONE);
    expect(color(3000.4)).toMatch(RED_TONE);
    expect(color(3001)).toMatch(RED_TONE);
    // REQ-004: each available band must be readable in light and dark themes.
    expectReadableColorFamily(color(0), "green");
    expectReadableColorFamily(color(1000), "orange");
    expectReadableColorFamily(color(3001), "red");
  });

  it("getGatewayTpsColorClass 按原始 TPS 独立分档（>=60 绿，20~<60 橙，<20 红）", () => {
    const color = latencyHelper("getGatewayTpsColorClass");
    expect(color(null)).toBe("text-muted-foreground");
    expect(color(undefined)).toBe("text-muted-foreground");
    expect(color(Number.NaN)).toBe("text-muted-foreground");
    expect(color(Number.POSITIVE_INFINITY)).toBe("text-muted-foreground");
    expect(color(-1)).toBe("text-muted-foreground");
    expect(color(60)).toMatch(GREEN_TONE);
    expect(color(600)).toMatch(GREEN_TONE);
    // 59.999 显示为 60.00，但原始值仍 < 60，必须取橙色。
    expect(color(59.999)).toMatch(ORANGE_TONE);
    expect(color(20)).toMatch(ORANGE_TONE);
    expect(color(19.999)).toMatch(RED_TONE);
    expect(color(0)).toMatch(RED_TONE);
    // REQ-004: each available band must be readable in light and dark themes.
    expectReadableColorFamily(color(60), "green");
    expectReadableColorFamily(color(20), "orange");
    expectReadableColorFamily(color(0), "red");
  });
});

describe("getGatewayDurationColorClass 耗时颜色类名映射", () => {
  it("空值、非有限数字或负数返回 text-muted-foreground", () => {
    expect(getGatewayDurationColorClass(null)).toBe("text-muted-foreground");
    expect(getGatewayDurationColorClass(undefined)).toBe("text-muted-foreground");
    expect(getGatewayDurationColorClass(Number.NaN)).toBe("text-muted-foreground");
    expect(getGatewayDurationColorClass(Number.POSITIVE_INFINITY)).toBe("text-muted-foreground");
    expect(getGatewayDurationColorClass(-1)).toBe("text-muted-foreground");
  });

  it("小于 3 秒 (< 3000ms) 使用绿色系并带同色系亮/暗可读色阶", () => {
    expect(getGatewayDurationColorClass(0)).toMatch(GREEN_TONE);
    expect(getGatewayDurationColorClass(500)).toMatch(GREEN_TONE);
    expect(getGatewayDurationColorClass(2999)).toMatch(GREEN_TONE);
    expectReadableColorFamily(getGatewayDurationColorClass(0), "green");
    expectReadableColorFamily(getGatewayDurationColorClass(2999), "green");
  });

  it("3 秒至 15 秒 (3000ms <= ms <= 15000ms) 使用橙色系并带同色系亮/暗可读色阶", () => {
    expect(getGatewayDurationColorClass(3000)).toMatch(ORANGE_TONE);
    expect(getGatewayDurationColorClass(8000)).toMatch(ORANGE_TONE);
    expect(getGatewayDurationColorClass(15000)).toMatch(ORANGE_TONE);
    expectReadableColorFamily(getGatewayDurationColorClass(3000), "orange");
    expectReadableColorFamily(getGatewayDurationColorClass(15000), "orange");
  });

  it("超过 15 秒 (> 15000ms) 使用红色系并带同色系亮/暗可读色阶", () => {
    expect(getGatewayDurationColorClass(15001)).toMatch(RED_TONE);
    expect(getGatewayDurationColorClass(60000)).toMatch(RED_TONE);
    expectReadableColorFamily(getGatewayDurationColorClass(15001), "red");
    expectReadableColorFamily(getGatewayDurationColorClass(60000), "red");
  });
});

describe("formatGatewayTokens Token格式化", () => {
  it("0或无输入时返回0", () => {
    expect(formatGatewayTokens(0)).toBe("0");
    expect(formatGatewayTokens(null)).toBe("0");
    expect(formatGatewayTokens(undefined)).toBe("0");
    expect(formatGatewayTokens(Number.NaN)).toBe("0");
    expect(formatGatewayTokens(Number.POSITIVE_INFINITY)).toBe("0");
  });

  it("小于1万的数字展示千分位完整整数", () => {
    expect(formatGatewayTokens(1)).toBe("1");
    expect(formatGatewayTokens(340)).toBe("340");
    expect(formatGatewayTokens(1250)).toBe("1,250");
    expect(formatGatewayTokens(9999)).toBe("9,999");
  });

  it("1万至100万（含十万级）转换为万并最多保留1位小数", () => {
    expect(formatGatewayTokens(10000)).toBe("1万");
    expect(formatGatewayTokens(15000)).toBe("1.5万");
    expect(formatGatewayTokens(15400)).toBe("1.5万");
    expect(formatGatewayTokens(100000)).toBe("10万");
    expect(formatGatewayTokens(250000)).toBe("25万");
    expect(formatGatewayTokens(856000)).toBe("85.6万");
  });

  it("1百万至1千万转换为百万并最多保留1位小数", () => {
    expect(formatGatewayTokens(1000000)).toBe("1百万");
    expect(formatGatewayTokens(1500000)).toBe("1.5百万");
    expect(formatGatewayTokens(8000000)).toBe("8百万");
  });

  it("1千万至1亿转换为千万并最多保留1位小数", () => {
    expect(formatGatewayTokens(10000000)).toBe("1千万");
    expect(formatGatewayTokens(12000000)).toBe("1.2千万");
  });

  it("1亿以上转换为亿并最多保留1位小数", () => {
    expect(formatGatewayTokens(100000000)).toBe("1亿");
    expect(formatGatewayTokens(150000000)).toBe("1.5亿");
    expect(formatGatewayTokens(2300000000)).toBe("23亿");
  });
});

describe("formatCacheTokensK 缓存Token xxK格式化", () => {
  it("0或无输入时返回0K", () => {
    expect(formatCacheTokensK(0)).toBe("0K");
    expect(formatCacheTokensK(-10)).toBe("0K");
    expect(formatCacheTokensK(null)).toBe("0K");
    expect(formatCacheTokensK(undefined)).toBe("0K");
    expect(formatCacheTokensK(Number.NaN)).toBe("0K");
    expect(formatCacheTokensK(Number.POSITIVE_INFINITY)).toBe("0K");
  });

  it("小于1000的数字格式化为保留1位小数的K", () => {
    expect(formatCacheTokensK(500)).toBe("0.5K");
    expect(formatCacheTokensK(100)).toBe("0.1K");
  });

  it("以1000为单位格式化为xxK并最多保留1位小数", () => {
    expect(formatCacheTokensK(1000)).toBe("1K");
    expect(formatCacheTokensK(1500)).toBe("1.5K");
    expect(formatCacheTokensK(10000)).toBe("10K");
    expect(formatCacheTokensK(10100)).toBe("10.1K");
    expect(formatCacheTokensK(10120)).toBe("10.1K");
    expect(formatCacheTokensK(10180)).toBe("10.2K");
    expect(formatCacheTokensK(25400)).toBe("25.4K");
  });
});

describe("formatUsageGroupLabel 分组标签", () => {
  it("有分组值时原样展示", () => {
    expect(formatUsageGroupLabel("model", "gpt-4o")).toBe("gpt-4o");
    expect(formatUsageGroupLabel("day", "2026-09-17")).toBe("2026-09-17");
  });

  it("分组值为空时展示破折号", () => {
    expect(formatUsageGroupLabel("none", "")).toBe("—");
    expect(formatUsageGroupLabel("model", "   ")).toBe("—");
  });
});

describe("clampUsagePage 页码收敛", () => {
  it("低于 1 收敛到 1", () => {
    expect(clampUsagePage(0, 5)).toBe(1);
    expect(clampUsagePage(-3, 5)).toBe(1);
  });

  it("超过总页数收敛到最后一页", () => {
    expect(clampUsagePage(9, 3)).toBe(3);
  });

  it("无有效页时收敛到第 1 页", () => {
    expect(clampUsagePage(4, 0)).toBe(1);
    expect(clampUsagePage(4, -1)).toBe(1);
  });
});

describe("usageStatusTranslationKey 状态展示", () => {
  it("映射成功与失败结果到稳定文案键", () => {
    expect(usageStatusTranslationKey("success")).toBe("aiGatewayStatusSuccess");
    expect(usageStatusTranslationKey("failure")).toBe("aiGatewayStatusFailure");
  });
});

describe("用量与日志命令封装", () => {
  function deferred<T>() {
    let resolve!: (value: T | PromiseLike<T>) => void;
    let reject!: (reason?: unknown) => void;
    const promise = new Promise<T>((res, rej) => {
      resolve = res;
      reject = rej;
    });
    return { promise, resolve, reject };
  }

  beforeEach(() => {
    resetTauriMocks();
  });

  it("按 camelCase 参数调用用量与保留天数命令", async () => {
    await aiGatewayUsageStats("7d");
    await aiGatewayUsageStats("all");
    await aiGatewayRequestLogs({
      range: "today",
      groupBy: "day",
      status: "failure",
      model: "gpt-4o",
      provider: "OpenAI Direct",
      page: 2,
    });
    await aiGatewayUsageRetentionGet();
    await aiGatewayUsageRetentionSave(30);

    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_usage_stats", {
      range: "7d",
    });
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_usage_stats", {
      range: "all",
    });
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_request_logs", {
      range: "today",
      groupBy: "day",
      status: "failure",
      model: "gpt-4o",
      provider: "OpenAI Direct",
      page: 2,
    });
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_usage_retention_get");
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_usage_retention_save", {
      days: 30,
    });
  });

  it("removed_legacy_price_wrappers_are_not_exported", async () => {
    const gatewayModule = await import("@/lib/aiGateway");
    expect("aiGatewayModelPricesGet" in gatewayModule).toBe(false);
    expect("aiGatewayModelPricesSave" in gatewayModule).toBe(false);
    expect("getProviderAvailableModels" in gatewayModule).toBe(false);
  });

  it("查询参数缺省时携带空筛选与第 1 页", async () => {
    await aiGatewayRequestLogs({ range: "today" });
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_request_logs", {
      range: "today",
      groupBy: null,
      status: null,
      model: null,
      provider: null,
      page: 1,
    });
  });

  it("等价的在途用量读取共享一次查询，不同范围与已结算结果保持隔离", async () => {
    const pendingToday = deferred<UsageStats>();
    const todayResult = { request_count: 1 } as UsageStats;
    const weekResult = { request_count: 2 } as UsageStats;
    invokeMock.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command !== "ai_gateway_usage_stats") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return args?.range === "today"
        ? pendingToday.promise
        : Promise.resolve(weekResult);
    });
    const usageCallsFor = (range: string) =>
      invokeMock.mock.calls.filter(
        ([command, args]) =>
          command === "ai_gateway_usage_stats" &&
          (args as Record<string, unknown> | undefined)?.range === range,
      ).length;

    const first = aiGatewayUsageStats("today");
    const second = aiGatewayUsageStats("today");
    const otherRange = aiGatewayUsageStats("7d");

    expect(usageCallsFor("today")).toBe(1);
    expect(usageCallsFor("7d")).toBe(1);

    pendingToday.resolve(todayResult);
    await expect(first).resolves.toBe(todayResult);
    await expect(second).resolves.toBe(todayResult);
    await expect(otherRange).resolves.toBe(weekResult);

    // A settled result is not a TTL cache: the next explicit read queries again.
    await expect(aiGatewayUsageStats("today")).resolves.toBe(todayResult);
    expect(usageCallsFor("today")).toBe(2);
  });

  it("并发请求日志按完整参数共享在途读取，失败后释放以允许重试", async () => {
    const pending = deferred<UsageLogsPage>();
    const pageOne = { page: 1, records: [] } as unknown as UsageLogsPage;
    const pageTwo = { page: 2, records: [] } as unknown as UsageLogsPage;
    let attempts = 0;
    invokeMock.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      attempts += 1;
      if (attempts === 1) {
        return Promise.reject(new Error("first attempt failed"));
      }
      if (attempts === 2) {
        return pending.promise;
      }
      return Promise.resolve(args?.page === 2 ? pageTwo : pageOne);
    });

    await expect(
      aiGatewayRequestLogs({ range: "today", page: 1 }),
    ).rejects.toThrow("first attempt failed");

    // The rejected flight must be removed, so these two equivalent calls share
    // the next attempt instead of being permanently poisoned.
    const first = aiGatewayRequestLogs({ range: "today", page: 1 });
    const second = aiGatewayRequestLogs({ range: "today", page: 1 });
    const differentPage = aiGatewayRequestLogs({ range: "today", page: 2 });
    expect(attempts).toBe(3);

    pending.resolve(pageOne);
    await expect(first).resolves.toBe(pageOne);
    await expect(second).resolves.toBe(pageOne);
    await expect(differentPage).resolves.toBe(pageTwo);

    // Settled reads are not cached.
    await expect(
      aiGatewayRequestLogs({ range: "today", page: 2 }),
    ).resolves.toBe(pageTwo);
    expect(attempts).toBe(4);
  });
});

describe("aggregateModels 聚合本地模型", () => {
  it("仅统计启用的服务商并跳过自动禁用的映射行，去重模型并保留全部映射上游来源与解析协议，不包含默认模型", () => {
    const providers: GatewayUpstreamProvider[] = [
      provider({
        id: "pa",
        name: "Alpha",
        protocol: "responses",
        default_model: "fallback-default",
        mappings: [
          { local_model: "gpt-4o", upstream_model: "gpt-4o-2024" },
          {
            local_model: "claude-3-7-sonnet",
            upstream_model: "  claude-3-7  ",
            protocol: "chat_completions",
          },
          {
            local_model: "row-exclude",
            upstream_model: "row-exclude-up",
            auto_disabled: true,
          },
        ],
      }),
      provider({
        id: "pb",
        name: "Beta",
        default_model: "  gpt-4o  ",
        mappings: [
          { local_model: "gpt-4o", upstream_model: "gpt-4o-mini" },
          { local_model: "   ", upstream_model: "ignored" },
        ],
      }),
      provider({
        id: "pc",
        name: "Gamma",
        enabled: false,
        default_model: "disabled-default",
        mappings: [
          { local_model: "gpt-4o", upstream_model: "disabled-upstream" },
        ],
      }),
      provider({
        id: "pd",
        name: "Delta",
        default_model: "auto-disabled-default",
        mappings: [
          {
            local_model: "auto-disabled-local",
            upstream_model: "auto-disabled-upstream",
          },
        ],
      }),
      provider({ id: "pe", name: "Epsilon", default_model: "only-default", mappings: [] }),
    ];

    expect(aggregateModels(providers)).toEqual([
      {
        model: "auto-disabled-local",
        providers: [
          {
            providerId: "pd",
            providerName: "Delta",
            upstreamModel: "auto-disabled-upstream",
            endpoint: "chat_completions",
            isDefault: false,
          },
        ],
      },
      {
        model: "claude-3-7-sonnet",
        providers: [
          {
            providerId: "pa",
            providerName: "Alpha",
            upstreamModel: "claude-3-7",
            endpoint: "chat_completions",
            isDefault: false,
          },
        ],
      },
      {
        model: "gpt-4o",
        providers: [
          {
            providerId: "pa",
            providerName: "Alpha",
            upstreamModel: "gpt-4o-2024",
            endpoint: "responses",
            isDefault: false,
          },
          {
            providerId: "pb",
            providerName: "Beta",
            upstreamModel: "gpt-4o-mini",
            endpoint: "chat_completions",
            isDefault: false,
          },
        ],
      },
    ]);
  });

  it("远端模型去空格后为空且无默认模型时不产生聚合模型", () => {
    const providers: GatewayUpstreamProvider[] = [
      provider({
        id: "p-ghost",
        name: "Ghost",
        default_model: null,
        mappings: [{ local_model: "ghost", upstream_model: "   " }],
      }),
    ];

    expect(aggregateModels(providers)).toEqual([]);
  });

  it("服务商仅有默认模型而无有效远端映射时不产生聚合模型", () => {
    const providers: GatewayUpstreamProvider[] = [
      provider({
        id: "p1",
        name: "Provider",
        default_model: "gpt-4o",
        mappings: [{ local_model: "gpt-4o", upstream_model: "" }],
      }),
    ];

    const aggregated = aggregateModels(providers);
    expect(aggregated).toEqual([]);
  });

  it("没有有效服务商时返回空数组", () => {
    expect(aggregateModels([])).toEqual([]);
    expect(
      aggregateModels([
        provider({ id: "off", name: "Off", enabled: false, default_model: "x" }),
        provider({
          id: "auto",
          name: "Auto",
          default_model: "y",
        }),
      ]),
    ).toEqual([]);
  });

  it("聚合视图仅保留启用/缺省启用的映射，排除禁用映射与未映射的默认模型", () => {
    const p = provider({
      default_model: "d",
      mappings: [
        { local_model: "a", upstream_model: "ra", enabled: true },
        { local_model: "b", upstream_model: "rb", enabled: false },
        // 缺省 enabled 的映射仍应视为启用（回归）
        { local_model: "c", upstream_model: "rc" },
      ],
    });

    const models = aggregateModels([p]).map((entry) => entry.model);
    expect(
      models,
      "聚合视图仅保留启用/缺省启用的映射 a、c",
    ).toEqual(["a", "c"]);
    expect(models, "聚合视图不应包含禁用映射 b").not.toContain("b");
    expect(models, "聚合视图不应包含未映射的默认模型 d").not.toContain("d");
  });

  it("映射声明 display_name 时仅声明条目携带 trim 后的 displayName", () => {
    const providers: GatewayUpstreamProvider[] = [
      provider({
        id: "pa",
        name: "Alpha",
        mappings: [
          {
            local_model: "gpt-4o",
            upstream_model: "gpt-4o-2024",
            display_name: "  GPT-4o 旗舰  ",
          },
        ],
      }),
      provider({
        id: "pb",
        name: "Beta",
        mappings: [{ local_model: "gpt-4o", upstream_model: "gpt-4o-mini" }],
      }),
    ];

    expect(aggregateModels(providers)).toEqual([
      {
        model: "gpt-4o",
        providers: [
          {
            providerId: "pa",
            providerName: "Alpha",
            upstreamModel: "gpt-4o-2024",
            endpoint: "chat_completions",
            isDefault: false,
            displayName: "GPT-4o 旗舰",
          },
          {
            providerId: "pb",
            providerName: "Beta",
            upstreamModel: "gpt-4o-mini",
            endpoint: "chat_completions",
            isDefault: false,
          },
        ],
      },
    ]);
  });

  it("全空白 display_name 不产生 displayName，且不抑制同模型已声明的名称", () => {
    const providers: GatewayUpstreamProvider[] = [
      provider({
        id: "pa",
        name: "Alpha",
        mappings: [
          {
            local_model: "gpt-4o",
            upstream_model: "gpt-4o-2024",
            display_name: "   ",
          },
          {
            local_model: "gpt-4o",
            upstream_model: "gpt-4o-mini",
            display_name: "  mini  ",
          },
        ],
      }),
    ];

    expect(aggregateModels(providers)).toEqual([
      {
        model: "gpt-4o",
        providers: [
          {
            providerId: "pa",
            providerName: "Alpha",
            upstreamModel: "gpt-4o-2024",
            endpoint: "chat_completions",
            isDefault: false,
          },
          {
            providerId: "pa",
            providerName: "Alpha",
            upstreamModel: "gpt-4o-mini",
            endpoint: "chat_completions",
            isDefault: false,
            displayName: "mini",
          },
        ],
      },
    ]);
  });

  it("服务商配置了默认模型时不影响映射条目的解析和展示", () => {
    const providers: GatewayUpstreamProvider[] = [
      provider({
        id: "p1",
        name: "Provider",
        default_model: "gpt-4o",
        mappings: [
          {
            local_model: "gpt-4o",
            upstream_model: "gpt-4o-2024",
            display_name: "旗舰版",
          },
        ],
      }),
    ];

    expect(aggregateModels(providers)).toEqual([
      {
        model: "gpt-4o",
        providers: [
          {
            providerId: "p1",
            providerName: "Provider",
            upstreamModel: "gpt-4o-2024",
            endpoint: "chat_completions",
            isDefault: false,
            displayName: "旗舰版",
          },
        ],
      },
    ]);
  });

  // Step 3: per-model auto-disable filtering in aggregateModels
  it("共享模型的 healthy 行保留条目，auto-disabled 专属模型消失", () => {
    const providers: GatewayUpstreamProvider[] = [
      provider({
        id: "pa",
        name: "Alpha",
        mappings: [
          {
            local_model: "gpt-4o",
            upstream_model: "gpt-4o-healthy",
            auto_disabled: false,
          },
        ],
      }),
      provider({
        id: "pb",
        name: "Beta",
        mappings: [
          {
            local_model: "gpt-4o",
            upstream_model: "gpt-4o-auto-disabled",
            auto_disabled: true,
          },
        ],
      }),
    ];

    const models = aggregateModels(providers);
    expect(models).toHaveLength(1);
    expect(models[0].model).toBe("gpt-4o");
    expect(models[0].providers).toHaveLength(1);
    expect(models[0].providers[0].upstreamModel).toBe("gpt-4o-healthy");
  });

  it("仅由 auto-disabled 行提供的模型不产生聚合结果", () => {
    const providers: GatewayUpstreamProvider[] = [
      provider({
        id: "pa",
        name: "Alpha",
        mappings: [
          {
            local_model: "exclusive-model",
            upstream_model: "exclusive-upstream",
            auto_disabled: true,
          },
        ],
      }),
    ];

    expect(aggregateModels(providers)).toEqual([]);
  });
});

describe("resolveAggregatedModelName 聚合模型名称解析", () => {
  it("优先返回首个非空 displayName 的 trim 值", () => {
    const providers: GatewayUpstreamProvider[] = [
      provider({
        id: "pa",
        name: "Alpha",
        mappings: [
          {
            local_model: "gpt-4o",
            upstream_model: "gpt-4o-2024",
            display_name: "  GPT-4o 旗舰  ",
          },
        ],
      }),
      provider({
        id: "pb",
        name: "Beta",
        mappings: [{ local_model: "gpt-4o", upstream_model: "gpt-4o-mini" }],
      }),
    ];

    const entry = aggregateModels(providers).find(
      (item) => item.model === "gpt-4o",
    );
    expect(entry).toBeDefined();
    expect(resolveAggregatedModelName(entry!)).toBe("GPT-4o 旗舰");
  });

  it("无 displayName 时返回首个映射来源的 upstreamModel", () => {
    const providers: GatewayUpstreamProvider[] = [
      provider({
        id: "pa",
        name: "Alpha",
        default_model: "gpt-4o",
        mappings: [{ local_model: "gpt-4o", upstream_model: "gpt-4o-2024" }],
      }),
      provider({
        id: "pb",
        name: "Beta",
        mappings: [{ local_model: "gpt-4o", upstream_model: "gpt-4o-mini" }],
      }),
    ];

    const entry = aggregateModels(providers).find(
      (item) => item.model === "gpt-4o",
    );
    expect(entry).toBeDefined();
    expect(resolveAggregatedModelName(entry!)).toBe("gpt-4o-2024");
  });

  it("display_name 全空白时回退首个映射来源的 upstreamModel", () => {
    const providers: GatewayUpstreamProvider[] = [
      provider({
        id: "pa",
        name: "Alpha",
        mappings: [
          {
            local_model: "gpt-4o",
            upstream_model: "gpt-4o-2024",
            display_name: "   ",
          },
        ],
      }),
    ];

    const entry = aggregateModels(providers).find(
      (item) => item.model === "gpt-4o",
    );
    expect(entry).toBeDefined();
    expect(entry!.providers[0]).not.toHaveProperty("displayName");
    expect(resolveAggregatedModelName(entry!)).toBe("gpt-4o-2024");
  });

  it("providers 为空数组时返回 entry.model", () => {
    expect(
      resolveAggregatedModelName({ model: "orphan-model", providers: [] }),
    ).toBe("orphan-model");
  });
});

describe("resolveAggregatedReasoningEfforts 与推理强度聚合", () => {
  it("聚合单服务商映射的推理强度并规范化去重", () => {
    const providers: GatewayUpstreamProvider[] = [
      provider({
        id: "pa",
        name: "Alpha",
        mappings: [
          {
            local_model: "o3-mini",
            upstream_model: "o3-mini-2025",
            reasoning_efforts: [" low ", "high", "low", ""],
          },
        ],
      }),
    ];

    const entries = aggregateModels(providers);
    expect(entries).toHaveLength(1);
    expect(entries[0].reasoningEfforts).toEqual(["low", "high"]);
    expect(entries[0].providers[0].reasoningEfforts).toEqual(["low", "high"]);
    expect(resolveAggregatedReasoningEfforts(entries[0])).toEqual(["low", "high"]);
  });

  it("多服务商映射到同一模型时去重合并推理强度并保留首现顺序", () => {
    const providers: GatewayUpstreamProvider[] = [
      provider({
        id: "pa",
        name: "Alpha",
        mappings: [
          {
            local_model: "deepseek-r1",
            upstream_model: "deepseek-r1-a",
            reasoning_efforts: ["medium", "high"],
          },
        ],
      }),
      provider({
        id: "pb",
        name: "Beta",
        mappings: [
          {
            local_model: "deepseek-r1",
            upstream_model: "deepseek-r1-b",
            reasoning_efforts: ["low", "high", "max"],
          },
        ],
      }),
    ];

    const entries = aggregateModels(providers);
    expect(entries).toHaveLength(1);
    // Alpha sorted before Beta, efforts order: medium, high, low, max
    expect(entries[0].reasoningEfforts).toEqual(["medium", "high", "low", "max"]);
    expect(resolveAggregatedReasoningEfforts(entries[0])).toEqual([
      "medium",
      "high",
      "low",
      "max",
    ]);
  });

  it("未配置推理强度时不带 reasoningEfforts 属性且解析为空数组", () => {
    const providers: GatewayUpstreamProvider[] = [
      provider({
        id: "pa",
        name: "Alpha",
        mappings: [
          {
            local_model: "gpt-4o",
            upstream_model: "gpt-4o-2024",
          },
        ],
      }),
    ];

    const entries = aggregateModels(providers);
    expect(entries).toHaveLength(1);
    expect(entries[0]).not.toHaveProperty("reasoningEfforts");
    expect(entries[0].providers[0]).not.toHaveProperty("reasoningEfforts");
    expect(resolveAggregatedReasoningEfforts(entries[0])).toEqual([]);
  });

  it("providers 为空或无有效映射时返回空数组", () => {
    expect(
      resolveAggregatedReasoningEfforts({ model: "orphan-model", providers: [] }),
    ).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// Step 5: provider template types, command wrappers and display helpers.
// RED tests for the frozen interface contract. Only this test file is touched.
// ---------------------------------------------------------------------------

import type { TFunction } from "i18next";
import {
  aiGatewayCreateProviderFromTemplate,
  aiGatewayDeleteProviderModel,
  aiGatewayProviderTemplates,
  aiGatewayRestoreProviderModel,
  aiGatewaySyncProviderTemplate,
  aiGatewayUpsertProviderTemplate,
  formatOffPeakDays,
  gatewayWeekdayTranslationKey,
  GATEWAY_WEEKDAY_ORDER,
  isMappingDeprecated,
  normalizeReasoningEfforts,
  type CreateProviderFromTemplateRequest,
  type GatewayModelMapping,
  type GatewayProviderTemplate,
  type GatewayProviderTemplateModel,
  type GatewayProviderTemplateView,
  type OffPeakPrice,
} from "@/lib/aiGateway";

/** Identity translator so `formatOffPeakDays` assertions read the raw i18n keys. */
const identityT = ((key: string) => key) as unknown as TFunction;

function templateFixture(
  overrides: Partial<GatewayProviderTemplate> = {},
): GatewayProviderTemplate {
  return {
    id: "tpl-1",
    name: "Template",
    description: "Template description",
    base_url: "https://template.example",
    protocol: "chat_completions",
    source: "snapshot",
    models: [
      {
        upstream_model: "model-a",
        display_name: "Model A",
        protocol: "chat_completions",
        enabled: true,
      },
    ],
    ...overrides,
  };
}

describe("aiGateway provider template helpers", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  it("gatewayWeekdayOrderIsMondayFirstAndZeroIsSunday", () => {
    expect(GATEWAY_WEEKDAY_ORDER).toEqual([1, 2, 3, 4, 5, 6, 0]);

    expect(gatewayWeekdayTranslationKey(0)).toBe("aiGatewayWeekdaySun");
    expect(gatewayWeekdayTranslationKey(1)).toBe("aiGatewayWeekdayMon");
    expect(gatewayWeekdayTranslationKey(2)).toBe("aiGatewayWeekdayTue");
    expect(gatewayWeekdayTranslationKey(3)).toBe("aiGatewayWeekdayWed");
    expect(gatewayWeekdayTranslationKey(4)).toBe("aiGatewayWeekdayThu");
    expect(gatewayWeekdayTranslationKey(5)).toBe("aiGatewayWeekdayFri");
    expect(gatewayWeekdayTranslationKey(6)).toBe("aiGatewayWeekdaySat");

    expect(gatewayWeekdayTranslationKey(7)).toBe("");
    expect(gatewayWeekdayTranslationKey(-1)).toBe("");
  });

  it("formatOffPeakDaysTreatsAbsentAndEmptyAsEveryDay", () => {
    expect(formatOffPeakDays(undefined, identityT)).toBe("aiGatewayEveryDay");
    expect(formatOffPeakDays(null, identityT)).toBe("aiGatewayEveryDay");
    expect(formatOffPeakDays([], identityT)).toBe("aiGatewayEveryDay");
  });

  it("formatOffPeakDaysDeduplicatesFiltersAndOrdersDays", () => {
    expect(formatOffPeakDays([5, 1, 1, 9], identityT)).toBe(
      "aiGatewayWeekdayMon, aiGatewayWeekdayFri",
    );
    expect(formatOffPeakDays([0, 6], identityT)).toBe(
      "aiGatewayWeekdaySat, aiGatewayWeekdaySun",
    );
    expect(formatOffPeakDays([9, -1], identityT)).toBe("aiGatewayEveryDay");
  });

  it("normalizeReasoningEffortsTrimsDropsEmptyAndDeduplicates", () => {
    expect(
      normalizeReasoningEfforts([" low ", "", "low", "high", " high "]),
    ).toEqual(["low", "high"]);
    expect(normalizeReasoningEfforts(null)).toEqual([]);
    expect(normalizeReasoningEfforts(undefined)).toEqual([]);
  });

  it("isMappingDeprecatedDetectsRetiredTemplateModels", () => {
    const template = templateFixture();
    const installed: GatewayModelMapping = {
      local_model: "model-a",
      upstream_model: "model-a",
    };
    const retired: GatewayModelMapping = {
      local_model: "model-b",
      upstream_model: "model-b",
    };

    expect(isMappingDeprecated(installed, template)).toBe(false);
    expect(isMappingDeprecated(retired, template)).toBe(true);
    expect(isMappingDeprecated(installed, null)).toBe(false);
    expect(isMappingDeprecated(installed, undefined)).toBe(false);
  });

  it("removed_fetch_models_wrapper_is_not_exported", async () => {
    const gatewayModule = await import("@/lib/aiGateway");
    expect(
      (gatewayModule as Record<string, unknown>).aiGatewayFetchModels,
      "aiGatewayFetchModels 包装函数必须随获取模型面板一起删除",
    ).toBeUndefined();
  });

  it("gateway_template_model_has_no_price_or_snapshot_fields", () => {
    const model: GatewayProviderTemplateModel = {
      upstream_model: "model-a",
      display_name: "Model A",
      protocol: "chat_completions",
      enabled: true,
    };
    expect(Object.keys(model).sort()).toEqual(
      ["display_name", "enabled", "protocol", "upstream_model"].sort(),
    );

    const template = templateFixture();
    expect(template).not.toHaveProperty("snapshot_version");
  });

  it("upsert_provider_template_passes_enabled_models_through", async () => {
    const template = templateFixture({
      models: [
        {
          upstream_model: "model-a",
          display_name: "Model A",
          protocol: "chat_completions",
          enabled: true,
        },
        {
          upstream_model: "model-b",
          display_name: null,
          protocol: "responses",
          enabled: false,
        },
      ],
    });

    await aiGatewayUpsertProviderTemplate(template);

    expect(invokeMock).toHaveBeenCalledWith(
      "ai_gateway_upsert_provider_template",
      { template },
    );
    expect(template.models[0].enabled).toBe(true);
    expect(template.models[1].enabled).toBe(false);
  });

  it("providerTemplateWrappersPassExactCommandArguments", async () => {
    const request: CreateProviderFromTemplateRequest = {
      templateId: "tpl-1",
      name: "My Provider",
      baseUrl: "https://upstream.example",
      protocol: "responses",
      apiKey: "test-key",
    };

    const templatesCall = aiGatewayProviderTemplates();
    const syncCall = aiGatewaySyncProviderTemplate("tpl-1");
    const createCall = aiGatewayCreateProviderFromTemplate(request);
    const deleteCall = aiGatewayDeleteProviderModel("p1", "model-a");
    const restoreCall = aiGatewayRestoreProviderModel("p1", "model-a");

    expect(templatesCall).toBeInstanceOf(Promise);
    expect(syncCall).toBeInstanceOf(Promise);
    expect(createCall).toBeInstanceOf(Promise);
    expect(deleteCall).toBeInstanceOf(Promise);
    expect(restoreCall).toBeInstanceOf(Promise);

    await Promise.all([
      templatesCall,
      syncCall,
      createCall,
      deleteCall,
      restoreCall,
    ]);

    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_provider_templates");
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_sync_provider_template", {
      templateId: "tpl-1",
    });
    expect(invokeMock).toHaveBeenCalledWith(
      "ai_gateway_create_provider_from_template",
      {
        templateId: "tpl-1",
        name: "My Provider",
        baseUrl: "https://upstream.example",
        protocol: "responses",
        apiKey: "test-key",
      },
    );
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_delete_provider_model", {
      providerId: "p1",
      upstreamModel: "model-a",
    });
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_restore_provider_model", {
      providerId: "p1",
      upstreamModel: "model-a",
    });
  });

  it("newOptionalTemplateFieldsDoNotBreakExistingConstruction", () => {
    const legacyOffPeak: OffPeakPrice = {
      start_time: "00:00",
      end_time: "08:00",
      input: 1,
      cache_read: 0.1,
      cache_write: 0.2,
      output: 2,
    };
    expect(legacyOffPeak.days).toBeUndefined();
    const weekdayOffPeak: OffPeakPrice = { ...legacyOffPeak, days: [1, 3, 5] };
    expect(weekdayOffPeak.days).toEqual([1, 3, 5]);

    const legacyMapping: GatewayModelMapping = {
      local_model: "local-a",
      upstream_model: "remote-a",
    };
    expect(legacyMapping.reasoning_efforts).toBeUndefined();
    const effortMapping: GatewayModelMapping = {
      ...legacyMapping,
      reasoning_efforts: ["low"],
    };
    expect(effortMapping.reasoning_efforts).toEqual(["low"]);

    const legacyProvider = provider();
    expect(legacyProvider.template_id).toBeUndefined();
    expect(legacyProvider.ignored_models).toBeUndefined();
    const boundProvider = provider({
      template_id: "tpl-1",
      ignored_models: ["model-b"],
    });
    expect(boundProvider.template_id).toBe("tpl-1");
    expect(boundProvider.ignored_models).toEqual(["model-b"]);

    const view: GatewayProviderTemplateView = {
      template: templateFixture(),
      synced_at: null,
      source: "snapshot",
      from_snapshot: true,
    };
    expect(view.from_snapshot).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Step 2: provider-scoped price types, command wrapper and pure helpers.
// RED tests for the frozen interface contract. Only this test file is touched.
// ---------------------------------------------------------------------------

import {
  draftToPriceRow,
  isPriceDraftPriced,
  mappedUpstreamModels,
  normalizeDraftDays,
  parsePriceNumber,
  priceRowToDraft,
  resolveProviderPriceRow,
  type GatewayPriceDraft,
  type GatewayPriceDraftOffPeak,
} from "@/lib/aiGateway";

function priceDraftWindow(
  overrides: Partial<GatewayPriceDraftOffPeak> = {},
): GatewayPriceDraftOffPeak {
  return {
    id: "op-1",
    start_time: "00:30",
    end_time: "08:30",
    input: "",
    cache_read: "",
    cache_write: "",
    output: "",
    days: [],
    ...overrides,
  };
}

function priceDraft(
  overrides: Partial<GatewayPriceDraft> = {},
): GatewayPriceDraft {
  return {
    id: "draft-1",
    upstream_model: "model-a",
    input: "",
    cache_read: "",
    cache_write: "",
    output: "",
    enable_off_peak: false,
    off_peaks: [],
    ...overrides,
  };
}

function modelPrice(overrides: Partial<ModelPrice> = {}): ModelPrice {
  return {
    upstream_model: "model-a",
    input: 0,
    cache_read: 0,
    cache_write: 0,
    output: 0,
    ...overrides,
  };
}

describe("aiGateway provider price helpers", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  it("mapped_upstream_models_deduplicates_and_keeps_first_seen_order", () => {
    const p = provider({
      default_model: "not-a-mapping",
      mappings: [
        { local_model: "l1", upstream_model: "  model-b  " },
        { local_model: "l2", upstream_model: "model-a" },
        { local_model: "l3", upstream_model: "model-b" },
        { local_model: "l4", upstream_model: "   " },
        // 缺省 enabled 的映射仍应计入映射上游模型
        { local_model: "l5", upstream_model: "model-c" },
      ],
    });

    expect(mappedUpstreamModels(p)).toEqual(["model-b", "model-a", "model-c"]);
    expect(mappedUpstreamModels(p)).not.toContain("");
    expect(
      mappedUpstreamModels(p),
      "未映射的默认模型不应进入映射上游列表",
    ).not.toContain("not-a-mapping");
  });

  it("resolve_provider_price_row_is_provider_scoped_and_shared_model_safe", () => {
    const providerARow = modelPrice({
      provider_id: "provider-a",
      upstream_model: "X",
      input: 1,
      output: 1,
    });
    const providerBRow = modelPrice({
      provider_id: "provider-b",
      upstream_model: "X",
      input: 2,
      output: 2,
    });
    const globalRow = modelPrice({ upstream_model: "X", input: 9, output: 9 });
    const prices = [providerARow, providerBRow, globalRow];

    expect(resolveProviderPriceRow(prices, "provider-a", "X")).toBe(providerARow);
    expect(resolveProviderPriceRow(prices, "provider-b", "X")).toBe(providerBRow);
    expect(resolveProviderPriceRow(prices, "provider-a", "X")?.input).toBe(1);
    expect(resolveProviderPriceRow(prices, "provider-b", "X")?.input).toBe(2);
    expect(resolveProviderPriceRow(prices, "provider-c", "X")).toBeUndefined();
    expect(resolveProviderPriceRow(prices, "provider-a", "Y")).toBeUndefined();
    expect(
      resolveProviderPriceRow(prices, "provider-a", "x"),
      "上游模型名精确区分大小写",
    ).toBeUndefined();
    expect(resolveProviderPriceRow(null, "provider-a", "X")).toBeUndefined();
    expect(resolveProviderPriceRow(undefined, "provider-a", "X")).toBeUndefined();
  });

  it("price_draft_round_trip_echoes_explicit_zeros_and_off_peak", () => {
    const row = modelPrice({
      provider_id: "provider-a",
      upstream_model: "model-a",
      input: 0,
      cache_read: 0,
      cache_write: 0,
      output: 0,
      off_peaks: [
        {
          start_time: "00:30",
          end_time: "08:30",
          input: 0,
          cache_read: 0,
          cache_write: 0,
          output: 0,
          days: [1, 3],
        },
      ],
    });

    const draft = priceRowToDraft(row, "model-a", "row-1");

    expect(draft.upstream_model).toBe("model-a");
    expect(draft.input).toBe("0");
    expect(draft.cache_read).toBe("0");
    expect(draft.cache_write).toBe("0");
    expect(draft.output).toBe("0");
    expect(draft.enable_off_peak).toBe(true);
    expect(draft.off_peaks).toHaveLength(1);
    expect(draft.off_peaks[0].id).toBe("row-1-op-0");
    expect(draft.off_peaks[0].start_time).toBe("00:30");
    expect(draft.off_peaks[0].end_time).toBe("08:30");
    expect(draft.off_peaks[0].input).toBe("0");
    expect(draft.off_peaks[0].cache_read).toBe("0");
    expect(draft.off_peaks[0].cache_write).toBe("0");
    expect(draft.off_peaks[0].output).toBe("0");
    expect(draft.off_peaks[0].days).toEqual([1, 3]);
    expect(isPriceDraftPriced(draft)).toBe(true);

    const echoedRow = draftToPriceRow(draft);
    expect(echoedRow).not.toBeNull();
    expect(echoedRow!.provider_id).toBeUndefined();
    expect(echoedRow!.upstream_model).toBe("model-a");
    expect(echoedRow!.input).toBe(0);
    expect(echoedRow!.cache_read).toBe(0);
    expect(echoedRow!.cache_write).toBe(0);
    expect(echoedRow!.output).toBe(0);
    expect(echoedRow!.off_peaks).toHaveLength(1);
    expect(echoedRow!.off_peaks![0]).toEqual({
      start_time: "00:30",
      end_time: "08:30",
      input: 0,
      cache_read: 0,
      cache_write: 0,
      output: 0,
      days: [1, 3],
    });

    // 窗口某一档留空时应回退到草稿标准档位；用非零标准值证明是回退而非直接归零。
    const fallbackDraft = priceDraft({
      input: "4",
      cache_read: "1",
      cache_write: "2",
      output: "9",
      enable_off_peak: true,
      off_peaks: [
        priceDraftWindow({
          id: "w0",
          input: "",
          cache_read: "",
          output: "5",
          days: [5, 1, 1, 9],
        }),
      ],
    });
    const fallbackRow = draftToPriceRow(fallbackDraft);
    expect(fallbackRow!.off_peaks![0].input).toBe(4);
    expect(fallbackRow!.off_peaks![0].cache_read).toBe(1);
    expect(fallbackRow!.off_peaks![0].cache_write).toBe(2);
    expect(fallbackRow!.off_peaks![0].output).toBe(5);
    expect(fallbackRow!.off_peaks![0].days).toEqual([1, 5]);
  });

  it("blank_draft_is_unpriced_and_produces_no_row", () => {
    const blank = priceDraft();
    expect(isPriceDraftPriced(blank)).toBe(false);
    expect(draftToPriceRow(blank)).toBeNull();

    // 启用离峰也无法让全空档位变成已定价
    const blankWithOffPeak = priceDraft({
      enable_off_peak: true,
      off_peaks: [priceDraftWindow({ input: "5" })],
    });
    expect(isPriceDraftPriced(blankWithOffPeak)).toBe(false);
    expect(draftToPriceRow(blankWithOffPeak)).toBeNull();

    const partial = priceDraft({ output: "3" });
    expect(isPriceDraftPriced(partial)).toBe(true);
    expect(draftToPriceRow(partial)).toEqual({
      upstream_model: "model-a",
      input: 0,
      cache_read: 0,
      cache_write: 0,
      output: 3,
    });

    const zeros = priceDraft({
      input: "0",
      cache_read: "0",
      cache_write: "0",
      output: "0",
    });
    expect(isPriceDraftPriced(zeros)).toBe(true);
    expect(draftToPriceRow(zeros)).toEqual({
      upstream_model: "model-a",
      input: 0,
      cache_read: 0,
      cache_write: 0,
      output: 0,
    });

    const blankModel = priceDraft({ upstream_model: "   ", output: "3" });
    expect(isPriceDraftPriced(blankModel)).toBe(true);
    expect(draftToPriceRow(blankModel)).toBeNull();
  });

  it("draft_to_price_row_writes_off_peak_only_when_enabled_with_windows", () => {
    const noWindows = priceDraft({
      output: "3",
      enable_off_peak: true,
      off_peaks: [],
    });
    const noWindowsRow = draftToPriceRow(noWindows);
    expect(noWindowsRow).not.toBeNull();
    expect(noWindowsRow!).not.toHaveProperty("off_peaks");
    expect(noWindowsRow!).not.toHaveProperty("off_peak");

    const disabled = priceDraft({
      output: "3",
      enable_off_peak: false,
      off_peaks: [priceDraftWindow({ input: "5" })],
    });
    const disabledRow = draftToPriceRow(disabled);
    expect(disabledRow).not.toBeNull();
    expect(disabledRow!).not.toHaveProperty("off_peaks");
    expect(disabledRow!).not.toHaveProperty("off_peak");

    const enabled = priceDraft({
      output: "3",
      enable_off_peak: true,
      off_peaks: [
        priceDraftWindow({
          id: "w0",
          start_time: "  ",
          end_time: "",
          input: "1",
          output: "2",
          days: [],
        }),
        priceDraftWindow({ id: "w1", input: "4", days: [5, 1, 1, 9] }),
      ],
    });
    const enabledRow = draftToPriceRow(enabled);
    expect(enabledRow!.off_peaks).toHaveLength(2);
    expect(enabledRow!.off_peaks![0].start_time).toBe("00:30");
    expect(enabledRow!.off_peaks![0].end_time).toBe("08:30");
    expect(
      enabledRow!.off_peaks![0],
      "空 days 应省略该字段（旧数据形态）",
    ).not.toHaveProperty("days");
    expect(enabledRow!.off_peaks![1].days).toEqual([1, 5]);
  });

  // REQ-003 / AC-007 guard: the built row must not carry the removed singular key.
  it("draft_to_price_row_never_emits_the_singular_off_peak_key", () => {
    const row = draftToPriceRow(
      priceDraft({
        output: "3",
        enable_off_peak: true,
        off_peaks: [priceDraftWindow({ input: "1" })],
      }),
    );

    expect(row).not.toBeNull();
    expect(row!).not.toHaveProperty("off_peak");
    expect(row!.off_peaks).toHaveLength(1);
  });

  it("normalize_draft_days_filters_sorts_and_dedupes", () => {
    expect(normalizeDraftDays([3, 1, 1, 7, -1, 0])).toEqual([0, 1, 3]);
    expect(normalizeDraftDays([])).toEqual([]);
    expect(normalizeDraftDays(null)).toEqual([]);
    expect(normalizeDraftDays(undefined)).toEqual([]);
  });

  it("parse_price_number_trims_parses_and_guards_non_finite", () => {
    expect(parsePriceNumber("  1.25  ")).toBe(1.25);
    expect(parsePriceNumber("0")).toBe(0);
    expect(parsePriceNumber("")).toBe(0);
    expect(parsePriceNumber("   ")).toBe(0);
    expect(parsePriceNumber("abc")).toBe(0);
    expect(parsePriceNumber("Infinity")).toBe(0);
    expect(parsePriceNumber("-Infinity")).toBe(0);
    expect(parsePriceNumber("NaN")).toBe(0);
  });

  it("price_row_to_draft_handles_missing_price_and_ignores_singular_off_peak", () => {
    const blank = priceRowToDraft(null, "model-x", "row-9");
    expect(blank.upstream_model).toBe("model-x");
    expect(blank.input).toBe("");
    expect(blank.cache_read).toBe("");
    expect(blank.cache_write).toBe("");
    expect(blank.output).toBe("");
    expect(blank.enable_off_peak).toBe(false);
    expect(blank.off_peaks).toEqual([]);

    // REQ-003 / AC-007 guard: a stored row carrying only the removed singular
    // `off_peak` field must not seed an off-peak window anymore.
    const legacyOnly = priceRowToDraft(
      {
        upstream_model: "model-y",
        input: 0,
        cache_read: 0,
        cache_write: 0,
        output: 3,
        off_peak: {
          start_time: "00:30",
          end_time: "08:30",
          input: 1,
          cache_read: 0.5,
          cache_write: 0,
          output: 3,
        },
      } as unknown as ModelPrice,
      "model-y",
      "row-9",
    );
    expect(legacyOnly.upstream_model).toBe("model-y");
    expect(legacyOnly.output).toBe("3");
    expect(legacyOnly.enable_off_peak).toBe(false);
    expect(legacyOnly.off_peaks).toEqual([]);
  });

  it("upsert_provider_wrapper_passes_prices_through", async () => {
    const p = provider({ id: "p1" });
    const prices: ModelPrice[] = [
      {
        provider_id: "p1",
        upstream_model: "model-a",
        input: 1,
        cache_read: 0.1,
        cache_write: 0.2,
        output: 2,
      },
    ];
    const saved = config({ providers: [p] });
    invokeMock.mockResolvedValueOnce(saved);

    const withPrices = await aiGatewayUpsertProvider(p, prices);
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_upsert_provider", {
      provider: p,
      prices,
    });
    expect(withPrices).toBe(saved);

    invokeMock.mockClear();
    invokeMock.mockResolvedValueOnce(saved);
    const withoutPrices = await aiGatewayUpsertProvider(p);
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_upsert_provider", {
      provider: p,
      prices: null,
    });
    expect(withoutPrices).toBe(saved);
  });

  it("gateway_config_accepts_missing_model_prices", () => {
    const legacy = config();
    expect(legacy.model_prices).toBeUndefined();
    expect(
      resolveProviderPriceRow(legacy.model_prices, "p1", "model-a"),
    ).toBeUndefined();

    const configured = config({
      model_prices: [
        {
          provider_id: "p1",
          upstream_model: "model-a",
          input: 1,
          cache_read: 0,
          cache_write: 0,
          output: 2,
        },
      ],
    });
    expect(configured.model_prices).toHaveLength(1);
    expect(
      resolveProviderPriceRow(configured.model_prices, "p1", "model-a")?.input,
    ).toBe(1);
  });
});

// ---------------------------------------------------------------------------
// Step 2: template auto-refresh interval wrappers and subscriber notification.
// RED tests for the frozen interface contract. Only this test file is touched.
// ---------------------------------------------------------------------------

describe("aiGateway template auto refresh wrappers", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  it("invokes the exact get/save commands and returns the invoke result", async () => {
    invokeMock.mockResolvedValueOnce(60);
    const loaded = await aiGatewayTemplateAutoRefreshGet();
    expect(invokeMock).toHaveBeenCalledWith(
      "ai_gateway_template_auto_refresh_get",
    );
    expect(loaded).toBe(60);

    invokeMock.mockResolvedValueOnce(30);
    const saved = await aiGatewayTemplateAutoRefreshSave(30);
    expect(invokeMock).toHaveBeenCalledWith(
      "ai_gateway_template_auto_refresh_save",
      { minutes: 30 },
    );
    expect(saved).toBe(30);
  });

  it("passes disable and boundary values through unchanged", async () => {
    invokeMock.mockResolvedValueOnce(0);
    expect(await aiGatewayTemplateAutoRefreshSave(0)).toBe(0);
    expect(invokeMock).toHaveBeenCalledWith(
      "ai_gateway_template_auto_refresh_save",
      { minutes: 0 },
    );

    invokeMock.mockResolvedValueOnce(10);
    expect(await aiGatewayTemplateAutoRefreshSave(10)).toBe(10);
    expect(invokeMock).toHaveBeenCalledWith(
      "ai_gateway_template_auto_refresh_save",
      { minutes: 10 },
    );

    invokeMock.mockResolvedValueOnce(1440);
    expect(await aiGatewayTemplateAutoRefreshSave(1440)).toBe(1440);
    expect(invokeMock).toHaveBeenCalledWith(
      "ai_gateway_template_auto_refresh_save",
      { minutes: 1440 },
    );
  });

  it("notifies subscribers on every change and stops after unsubscribe", () => {
    const listener = vi.fn();
    const unsubscribe = subscribeTemplateAutoRefreshIntervalChanged(listener);
    expect(listener).not.toHaveBeenCalled();

    notifyTemplateAutoRefreshIntervalChanged();
    expect(listener).toHaveBeenCalledTimes(1);

    notifyTemplateAutoRefreshIntervalChanged();
    expect(listener).toHaveBeenCalledTimes(2);

    unsubscribe();
    notifyTemplateAutoRefreshIntervalChanged();
    expect(
      listener,
      "取消订阅后通知不应再触达旧监听器",
    ).toHaveBeenCalledTimes(2);
  });
});

// ---------------------------------------------------------------------------
// Step 3: ordered upstream key-pool wire contract and wrapper signatures.
// RED tests for the frozen interface contract. Only this test file is touched.
// ---------------------------------------------------------------------------

describe("AI Gateway 上游密钥池契约", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  it("provider 记录以 keys 数组承载有序密钥且字段顺序与后端契约一致", () => {
    const entry = providerKey();
    expect(Object.keys(entry)).toEqual([
      "id",
      "name",
      "value",
      "enabled",
      "daily_reset",
      "auto_marked",
      "failure_kind",
      "marked_at",
      "reason",
    ]);

    const p = provider({
      keys: [
        providerKey({ id: "k1", name: "Primary" }),
        providerKey({ id: "k2", name: "Backup" }),
      ],
    }) as unknown as { keys: RawProviderKey[] };
    expect(p.keys.map((item) => item.id)).toEqual(["k1", "k2"]);
  });

  it("upsert 包装函数原样传递有序 keys 数组与启用标记", async () => {
    const keys = [
      providerKey({
        id: "k1",
        name: "Primary",
        value: "sk-one",
        enabled: true,
        daily_reset: true,
      }),
      providerKey({
        id: "k2",
        name: "Backup",
        value: "sk-two",
        enabled: false,
        daily_reset: false,
      }),
    ];
    const p = provider({ id: "p1", keys });
    const saved = config({ providers: [p] });
    invokeMock.mockResolvedValueOnce(saved);

    const result = await aiGatewayUpsertProvider(p);

    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_upsert_provider", {
      provider: expect.objectContaining({
        keys: [keys[0], keys[1]],
      }),
      prices: null,
    });
    const invokedKeys = (
      invokeMock.mock.calls[0][1] as {
        provider: { keys: RawProviderKey[] };
      }
    ).provider.keys;
    expect(
      invokedKeys.map((entry) => entry.daily_reset),
      "包装函数应把字面量载荷中的 daily_reset 原样传递",
    ).toEqual([true, false]);
    expect(result).toBe(saved);
  });

  it("重新启用单个上游密钥包装为 ai_gateway_reenable_provider_key", async () => {
    const gatewayModule = await import("@/lib/aiGateway");
    const reenable = (gatewayModule as Record<string, unknown>)
      .aiGatewayReenableProviderKey as
      | ((providerId: string, keyId: string) => Promise<GatewayConfig>)
      | undefined;
    expect(
      typeof reenable,
      "aiGatewayReenableProviderKey 包装函数必须导出",
    ).toBe("function");

    const saved = config({
      providers: [provider({ keys: [providerKey({ id: "k1" })] })],
    });
    invokeMock.mockResolvedValueOnce(saved);

    await reenable!("p1", "k1");

    expect(invokeMock).toHaveBeenCalledWith(
      "ai_gateway_reenable_provider_key",
      { providerId: "p1", keyId: "k1" },
    );
  });

  it("配额与 Go 用量包装函数保持 providerId/forceRefresh 签名且不携带密钥参数", async () => {
    await aiGatewayProviderQuota("p1", true);
    await aiGatewayProviderGoUsage("p1", false);

    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_provider_quota", {
      providerId: "p1",
      forceRefresh: true,
    });
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_provider_go_usage", {
      providerId: "p1",
      forceRefresh: false,
    });
    for (const call of invokeMock.mock.calls) {
      const payload = (call[1] ?? {}) as Record<string, unknown>;
      expect(Object.keys(payload)).not.toContain("keyId");
      expect(Object.keys(payload)).not.toContain("key_id");
    }
  });
});

// ---------------------------------------------------------------------------
// Step 3: authentication-notification event literal, aggregation window and
// marked-key selector. RED tests for the frozen interface contract.
// ---------------------------------------------------------------------------

describe("providerMarkedKeys 已标记密钥筛选", () => {
  it("冻结鉴权失败事件字面量与 1500ms 聚合窗口", () => {
    expect(AI_GATEWAY_KEY_AUTH_FAILED_EVENT).toBe(
      "ai-gateway-key-auth-failed",
    );
    expect(AI_GATEWAY_KEY_AUTH_TOAST_WINDOW_MS).toBe(1500);
  });

  it("缺少密钥池或服务商为空时返回空数组", () => {
    expect(providerMarkedKeys(null)).toEqual([]);
    expect(providerMarkedKeys(undefined)).toEqual([]);

    const withoutPool = provider();
    delete (withoutPool as { keys?: unknown }).keys;
    expect(providerMarkedKeys(withoutPool)).toEqual([]);
  });

  it("仅保留 auto_marked 的密钥并保持密钥池顺序，鉴权与额度标记都被选中", () => {
    const p = provider({
      keys: [
        providerKey({ id: "k-unmarked", name: "Unmarked" }),
        providerKey({
          id: "k-quota",
          name: "Quota Key",
          auto_marked: true,
          failure_kind: "quota",
          marked_at: 1_700_000_000,
        }),
        providerKey({
          id: "k-disabled",
          name: "Disabled Unmarked",
          enabled: false,
        }),
        providerKey({
          id: "k-auth",
          name: "Auth Key",
          auto_marked: true,
          failure_kind: "authentication",
          marked_at: 1_700_000_100,
          reason: "invalid api key",
        }),
      ],
    });

    const marked = providerMarkedKeys(p);
    expect(marked.map((entry: GatewayProviderKey) => entry.id)).toEqual([
      "k-quota",
      "k-auth",
    ]);
    expect(
      marked.map((entry: GatewayProviderKey) => entry.failure_kind),
    ).toEqual(["quota", "authentication"]);
  });
});

// ---------------------------------------------------------------------------
// Step 2: provider-template policy DTOs and effective-policy helpers.
// RED tests for the frozen interface contract. Only this test file is touched.
// The helpers resolve from `template_id` only and must never infer a strategy
// from URL, name or icon (REQ-008 / AC-005 / AC-006 / AC-012).
// ---------------------------------------------------------------------------

import {
  isProviderTemplateBindingBroken,
  resolveEffectiveTemplatePolicy,
  resolveProviderTemplate,
  type GatewayEffectiveTemplatePolicy,
  type GatewayTemplateErrorStrategy,
  type GatewayTemplateQuotaStrategy,
  type GatewayTemplateSyncStrategy,
} from "@/lib/aiGateway";

const GENERIC_TEMPLATE_POLICY: GatewayEffectiveTemplatePolicy = {
  sync_strategy: "generic",
  quota_strategy: "none",
  error_strategy: "generic",
};

function policyTemplateView(
  overrides: Partial<GatewayProviderTemplate> = {},
): GatewayProviderTemplateView {
  return {
    template: {
      id: "tpl-cc",
      name: "CommandCode",
      description: "CommandCode provider template",
      base_url: "https://api.commandcode.ai/provider/v1",
      protocol: "chat_completions",
      source: "https://api.commandcode.ai/provider/v1/models",
      models_url: "https://api.commandcode.ai/provider/v1/models",
      models: [],
      sync_strategy: "generic",
      quota_strategy: "commandcode",
      error_strategy: "generic",
      ...overrides,
    },
    synced_at: null,
    source: "snapshot",
    from_snapshot: true,
  };
}

describe("AI Gateway 模板策略 DTO 与有效策略解析", () => {
  it("模板 DTO 携带闭合的同步/额度/错误策略字段", () => {
    const template = policyTemplateView({
      id: "tpl-custom",
      sync_strategy: "opencode_go",
      quota_strategy: "opencode_go",
      error_strategy: "generic",
    }).template;

    const sync: GatewayTemplateSyncStrategy = template.sync_strategy!;
    const quota: GatewayTemplateQuotaStrategy = template.quota_strategy!;
    const error: GatewayTemplateErrorStrategy = template.error_strategy!;

    expect(sync).toBe("opencode_go");
    expect(quota).toBe("opencode_go");
    expect(error).toBe("generic");
  });

  it("绑定服务商从其模板解析同步/额度/错误策略", () => {
    const views = [policyTemplateView()];
    const bound = { template_id: "tpl-cc" };

    expect(resolveProviderTemplate(views, bound)?.template.id).toBe("tpl-cc");
    expect(resolveEffectiveTemplatePolicy(views, bound)).toEqual({
      sync_strategy: "generic",
      quota_strategy: "commandcode",
      error_strategy: "generic",
    });
    expect(isProviderTemplateBindingBroken(views, bound)).toBe(false);
  });

  it("模板策略字段缺失时按 generic/none/generic 容错解析", () => {
    const legacyView = policyTemplateView({
      id: "tpl-legacy",
      sync_strategy: undefined,
      quota_strategy: undefined,
      error_strategy: undefined,
    });

    expect(
      resolveEffectiveTemplatePolicy([legacyView], { template_id: "tpl-legacy" }),
    ).toEqual(GENERIC_TEMPLATE_POLICY);
  });

  it("未绑定服务商解析为 generic 策略且绝不按 URL/名称推断", () => {
    const views = [policyTemplateView()];
    const commandCodeLike = {
      name: "CommandCode",
      base_url: "https://api.commandcode.ai/provider/v1",
      template_id: null,
    };
    const goLike = {
      name: "OpenCode Go",
      base_url: "https://opencode.ai/zen/go",
    };

    expect(resolveProviderTemplate(views, commandCodeLike)).toBeNull();
    expect(resolveEffectiveTemplatePolicy(views, commandCodeLike)).toEqual(
      GENERIC_TEMPLATE_POLICY,
    );
    expect(resolveEffectiveTemplatePolicy(views, goLike)).toEqual(
      GENERIC_TEMPLATE_POLICY,
    );
    expect(isProviderTemplateBindingBroken(views, commandCodeLike)).toBe(false);
    expect(isProviderTemplateBindingBroken(views, goLike)).toBe(false);
  });

  it("未知或已删除模板 id 回退 generic 并标记为损坏绑定", () => {
    const views = [policyTemplateView()];
    const unknown = { template_id: "deleted-or-unknown" };

    expect(resolveProviderTemplate(views, unknown)).toBeNull();
    expect(resolveEffectiveTemplatePolicy(views, unknown)).toEqual(
      GENERIC_TEMPLATE_POLICY,
    );
    expect(isProviderTemplateBindingBroken(views, unknown)).toBe(true);
  });

  it("空模板列表与空服务商都安全解析为 generic", () => {
    expect(resolveProviderTemplate(undefined, { template_id: "x" })).toBeNull();
    expect(resolveEffectiveTemplatePolicy(null, { template_id: "x" })).toEqual(
      GENERIC_TEMPLATE_POLICY,
    );
    expect(isProviderTemplateBindingBroken([], { template_id: "x" })).toBe(true);
    expect(isProviderTemplateBindingBroken([], null)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// Step 4: the third upstream protocol and the optional Claude model selection.
// RED before implementation: `GatewayUpstreamProtocol` only has two members and
// the terminal wrappers cannot carry `claude_models`.
// ---------------------------------------------------------------------------

describe("AI Gateway anthropic_messages 协议与 Claude 四选择 wrapper", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  it("GatewayUpstreamProtocol 联合接受 anthropic_messages", () => {
    const protocol: GatewayUpstreamProtocol = "anthropic_messages";
    expect(protocol).toBe("anthropic_messages");
  });

  it("服务商默认与映射行接受 anthropic_messages 并保持父默认继承", () => {
    const providers: GatewayUpstreamProvider[] = [
      provider({
        id: "p-anth",
        protocol: "anthropic_messages",
        mappings: [
          // No row protocol: inherits the provider's anthropic default.
          { local_model: "claude-inherit", upstream_model: "cc-opus" },
          // Explicit row protocol overrides the inherited default.
          {
            local_model: "chat-explicit",
            upstream_model: "gpt-4o",
            protocol: "chat_completions",
          },
        ],
      }),
    ];

    const models = aggregateModels(providers);
    expect(
      models.find((entry) => entry.model === "claude-inherit")?.providers[0]
        .endpoint,
    ).toBe("anthropic_messages");
    expect(
      models.find((entry) => entry.model === "chat-explicit")?.providers[0]
        .endpoint,
    ).toBe("chat_completions");
  });

  it("configure wrapper 携带可选 claude_models 并序列化为后端四选择载荷", async () => {
    const selection: ClaudeModelSelection = {
      default_model: "cc-opus",
      haiku: "cc-haiku",
      sonnet: "cc-sonnet",
      opus: "cc-opus",
    };

    await aiGatewayConfigureTerminal(["claude"], selection);

    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_configure_terminal", {
      targetTools: ["claude"],
      claudeModels: {
        default_model: "cc-opus",
        haiku: "cc-haiku",
        sonnet: "cc-sonnet",
        opus: "cc-opus",
      },
    });
  });

  it("sync wrapper 携带可选 claude_models 且载荷恰好四个本地模型 id", async () => {
    const selection: ClaudeModelSelection = {
      default_model: "cc-opus",
      haiku: "cc-haiku",
      sonnet: "cc-sonnet",
      opus: "cc-opus",
    };

    await aiGatewaySyncTerminal(["claude"], selection);

    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_sync_terminal", {
      targetTools: ["claude"],
      claudeModels: {
        default_model: "cc-opus",
        haiku: "cc-haiku",
        sonnet: "cc-sonnet",
        opus: "cc-opus",
      },
    });

    const payload = invokeMock.mock.calls.find(
      ([command]) => command === "ai_gateway_sync_terminal",
    )?.[1] as { claudeModels: Record<string, unknown> };
    expect(Object.keys(payload.claudeModels).sort()).toEqual([
      "default_model",
      "haiku",
      "opus",
      "sonnet",
    ]);
    // The payload is model ids only: no credential-shaped key leaks through.
    expect(JSON.stringify(payload)).not.toMatch(/api[_-]?key|"value"|sk-/i);
  });

  it("未提供 Claude 选择时 wrapper 不携带 claudeModels 键", async () => {
    await aiGatewayConfigureTerminal(["opencode"]);
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_configure_terminal", {
      targetTools: ["opencode"],
    });

    resetTauriMocks();
    await aiGatewaySyncTerminal();
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_sync_terminal", {});
  });
});

// ---------------------------------------------------------------------------
// Step 2 (20261014-ai-gateway-provider-usage-history): provider-history wrappers
// and the additive providerId/day request-log selectors. These assertions are
// the wrapper-level RED contract for the frontend implementation.
// ---------------------------------------------------------------------------

describe("服务商历史命令封装", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  it("provider-history wrappers call the exact new commands", async () => {
    const gateway = await import("@/lib/aiGateway");
    expect("aiGatewayUsageProviders" in gateway).toBe(true);
    expect("aiGatewayProviderUsageHistory" in gateway).toBe(true);

    const usageProviders = (
      gateway as unknown as {
        aiGatewayUsageProviders: () => Promise<unknown>;
      }
    ).aiGatewayUsageProviders;
    const providerUsageHistory = (
      gateway as unknown as {
        aiGatewayProviderUsageHistory: (query: {
          providerId: string;
        }) => Promise<unknown>;
      }
    ).aiGatewayProviderUsageHistory;

    await usageProviders();
    await providerUsageHistory({ providerId: "p-a" });

    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_usage_providers");
    expect(invokeMock).toHaveBeenCalledWith(
      "ai_gateway_provider_usage_history",
      { providerId: "p-a" },
    );
  });

  it("omitted providerId/day keep the exact existing six-key log payload", async () => {
    await aiGatewayRequestLogs({ range: "today" });
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_request_logs", {
      range: "today",
      groupBy: null,
      status: null,
      model: null,
      provider: null,
      page: 1,
    });
  });

  it("adds providerId/day only when set and never uses a separate log command", async () => {
    await aiGatewayRequestLogs({
      range: "today",
      providerId: "p-a",
      day: "2026-09-17",
    } as never);

    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_request_logs", {
      range: "today",
      groupBy: null,
      status: null,
      model: null,
      provider: null,
      page: 1,
      providerId: "p-a",
      day: "2026-09-17",
    });
    expect(
      invokeMock.mock.calls.some(
        ([command]) => command === "ai_gateway_provider_request_logs",
      ),
    ).toBe(false);
  });

  it("keeps each optional selector absent when only the other is set", async () => {
    await aiGatewayRequestLogs({ range: "today", providerId: "p-a" } as never);
    const providerIdOnly = invokeMock.mock.calls[
      invokeMock.mock.calls.length - 1
    ][1] as Record<string, unknown>;
    expect(providerIdOnly).toEqual({
      range: "today",
      groupBy: null,
      status: null,
      model: null,
      provider: null,
      page: 1,
      providerId: "p-a",
    });
    expect("day" in providerIdOnly).toBe(false);

    await aiGatewayRequestLogs({ range: "today", day: "2026-09-17" } as never);
    const dayOnly = invokeMock.mock.calls[
      invokeMock.mock.calls.length - 1
    ][1] as Record<string, unknown>;
    expect(dayOnly).toEqual({
      range: "today",
      groupBy: null,
      status: null,
      model: null,
      provider: null,
      page: 1,
      day: "2026-09-17",
    });
    expect("providerId" in dayOnly).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// Step 4 (20261015-pool-only-reasoning) REQ-002 / REQ-007 / REQ-005: the shared
// DTO helpers tolerate the new pool-local reasoning fields and keep ordinary
// provider reasoning identifiers exact. These are regression guards: they pass
// against the pre-Step-4 helpers and must keep passing after the frontend adds
// the pool DTO fields.
// ---------------------------------------------------------------------------

describe("AI Gateway 模型池推理 DTO 容错与普通档位隔离（Step 4 guard）", () => {
  it("普通推理档位联合去重保留原始标识而不规范化", () => {
    const [entry] = aggregateModels([
      provider({
        mappings: [
          {
            local_model: "m",
            upstream_model: "u",
            reasoning_efforts: ["max", "minimal", "AUTO", "max"],
          },
        ],
      }),
    ]);

    expect(resolveAggregatedReasoningEfforts(entry)).toEqual([
      "max",
      "minimal",
      "AUTO",
    ]);
  });

  it("候选状态解析忽略新增 reasoning 字段而保持服务商/映射/密钥判定", () => {
    const candidate = {
      provider_id: "p1",
      local_model: "l",
      upstream_model: "u",
      protocol: "chat_completions",
      weight: 1,
      reasoning: { profile: "openai_effort" },
    };

    const state = aiGatewayModule.resolveModelPoolCandidateState(
      candidate as never,
      [provider({ mappings: [{ local_model: "l", upstream_model: "u" }] })],
    );

    expect(state.availability).toBe("ready");
  });
});

// ---------------------------------------------------------------------------
// Step 4 repair (20261015-pool-only-reasoning) REQ-002 / AC-002 / AC-004: the
// exported candidate-local mapping helper is the one shared native-shape rule
// the pool editor guard, the catalog projection and the model list all reuse.
// It must reject the shapes the backend refuses (see `candidate_serves_effort`):
// an enum on a manual-budget profile, a positive preference mapped to the
// reserved `none`, a switch value other than `enabled`, and a budget action
// without a `budget_tokens` integer inside the declared range.
//
// RED: the pre-repair helper accepts every invalid row below because it only
// validates the declared range and never the mapped budget value.
// ---------------------------------------------------------------------------

describe("模型池推理映射有效性边界（Step 4 repair）", () => {
  function valid(
    reasoning: Record<string, unknown>,
    mapping: Record<string, unknown>,
  ): boolean {
    return aiGatewayModule.isModelPoolReasoningMappingValid(
      reasoning as never,
      mapping as never,
    );
  }

  const manualRange = { min: 1024, max: 8192 };

  it("manual-budget 配置拒绝 enum 正向动作", () => {
    expect(
      valid(
        {
          profile: "anthropic_manual_budget",
          values: ["high"],
          budget: manualRange,
          mappings: [],
        },
        { effort: "high", action: { kind: "enum", value: "high" } },
      ),
    ).toBe(false);
  });

  it("正向档位不得映射到保留字 none", () => {
    expect(
      valid(
        { profile: "openai_effort", values: ["none"], mappings: [] },
        { effort: "high", action: { kind: "enum", value: "none" } },
      ),
    ).toBe(false);
  });

  it("switch 配置只接受 enabled 开关取值", () => {
    const reasoning = {
      profile: "chat_thinking_switch",
      values: [],
      supports_disable: true,
      mappings: [],
    };
    expect(
      valid(reasoning, {
        effort: "high",
        action: { kind: "enum", value: "high" },
      }),
    ).toBe(false);
    expect(
      valid(reasoning, {
        effort: "high",
        action: { kind: "enum", value: "enabled" },
      }),
    ).toBe(true);
  });

  it("manual-budget 动作必须携带落在声明区间内的整数预算", () => {
    const reasoning = {
      profile: "anthropic_manual_budget",
      values: [],
      budget: manualRange,
      mappings: [],
    };
    const mapping = (budget_tokens?: number) => ({
      effort: "high",
      action: {
        kind: "budget",
        ...(budget_tokens === undefined ? {} : { budget_tokens }),
      },
    });

    expect(valid(reasoning, mapping(undefined))).toBe(false);
    expect(valid(reasoning, mapping(1023))).toBe(false);
    expect(valid(reasoning, mapping(8193))).toBe(false);
    expect(valid(reasoning, mapping(3072))).toBe(true);
  });

  it("非 manual-budget 配置不接受 budget 动作", () => {
    expect(
      valid(
        {
          profile: "anthropic_effort_adaptive",
          values: [],
          budget: manualRange,
          mappings: [],
        },
        { effort: "high", action: { kind: "budget", budget_tokens: 3072 } },
      ),
    ).toBe(false);
  });
});

describe("模型池统一对外推理强度与自动绑定 (autoBindCandidateReasoning)", () => {
  it("CANONICAL_REASONING_EFFORTS 包含 low, medium, high, xhigh, max, none", () => {
    expect(CANONICAL_REASONING_EFFORTS).toEqual([
      "low",
      "medium",
      "high",
      "xhigh",
      "max",
      "none",
    ]);
  });

  const providers: GatewayUpstreamProvider[] = [
    {
      id: "p1",
      name: "Provider 1",
      base_url: "https://example.com",
      default_model: null,
      enabled: true,
      protocol: "chat_completions",
      mappings: [
        {
          local_model: "m-all",
          upstream_model: "remote-all",
          reasoning_efforts: ["low", "medium", "high", "xhigh", "max", "none"],
        },
        {
          local_model: "m-case",
          upstream_model: "remote-case",
          reasoning_efforts: ["LOW", "Medium", "MAX"],
        },
        {
          local_model: "m-none-only",
          upstream_model: "remote-none-only",
          reasoning_efforts: ["none"],
        },
        {
          local_model: "m-unmatched",
          upstream_model: "remote-unmatched",
          reasoning_efforts: ["minimal", "ultra", "thinking"],
        },
        {
          local_model: "m-empty",
          upstream_model: "remote-empty",
          reasoning_efforts: [],
        },
      ],
    },
  ];

  it("当模型声明包含全部标准档位时，自动绑定到所有对应档位", () => {
    const candidate = {
      provider_id: "p1",
      local_model: "m-all",
      upstream_model: "remote-all",
      protocol: "chat_completions" as const,
      weight: 1,
    };
    const result = autoBindCandidateReasoning(candidate, providers);
    expect(result.hasMatch).toBe(true);
    expect(result.mappings).toEqual({
      low: "enum:low",
      medium: "enum:medium",
      high: "enum:high",
      xhigh: "enum:xhigh",
      max: "enum:max",
      none: "disable",
    });
  });

  it("支持不区分大小写匹配，并且绑定的 enum 值保留原生声明的原始大小写", () => {
    const candidate = {
      provider_id: "p1",
      local_model: "m-case",
      upstream_model: "remote-case",
      protocol: "chat_completions" as const,
      weight: 1,
    };
    const result = autoBindCandidateReasoning(candidate, providers);
    expect(result.hasMatch).toBe(true);
    expect(result.mappings).toEqual({
      low: "enum:LOW",
      medium: "enum:Medium",
      max: "enum:MAX",
    });
    expect(result.mappings.high).toBeUndefined();
    expect(result.mappings.none).toBeUndefined();
  });

  it("当模型仅支持 none（禁用）时，none 绑定为 disable", () => {
    const candidate = {
      provider_id: "p1",
      local_model: "m-none-only",
      upstream_model: "remote-none-only",
      protocol: "chat_completions" as const,
      weight: 1,
    };
    const result = autoBindCandidateReasoning(candidate, providers);
    expect(result.hasMatch).toBe(true);
    expect(result.mappings).toEqual({
      none: "disable",
    });
  });

  it("当模型原生声明了无法匹配的自定义档位时，hasMatch 为 false 且 mappings 为空", () => {
    const candidate = {
      provider_id: "p1",
      local_model: "m-unmatched",
      upstream_model: "remote-unmatched",
      protocol: "chat_completions" as const,
      weight: 1,
    };
    const result = autoBindCandidateReasoning(candidate, providers);
    expect(result.hasMatch).toBe(false);
    expect(result.mappings).toEqual({});
  });

  it("当模型未声明任何原生档位时，hasMatch 为 false 且 mappings 为空", () => {
    const candidate = {
      provider_id: "p1",
      local_model: "m-empty",
      upstream_model: "remote-empty",
      protocol: "chat_completions" as const,
      weight: 1,
    };
    const result = autoBindCandidateReasoning(candidate, providers);
    expect(result.hasMatch).toBe(false);
    expect(result.mappings).toEqual({});
  });
});

// ---------------------------------------------------------------------------
// Ordered fallback models: the shared static reasoning catalog spans the
// configured union of the regular candidates and every provider mapping that
// serves an ordered fallback gateway model. Every expanded source derives its
// native control from its exact mapping and effective protocol, and `none` is
// advertised only when that source declares it. Static support never depends on
// transient cooldown or key readiness.
//
// AC-004 / REQ-003.
// ---------------------------------------------------------------------------

describe("模型池静态推理目录按有序回退模型自动展开", () => {
  const regularProvider = provider({
    id: "p-a",
    name: "Provider A",
    protocol: "chat_completions",
    mappings: [{ local_model: "local-a1", upstream_model: "remote-a1" }],
  });

  const chatFallbackProvider = provider({
    id: "p-b",
    name: "Provider B",
    protocol: "chat_completions",
    mappings: [
      {
        local_model: "local-b1",
        upstream_model: "remote-b1",
        reasoning_efforts: ["high", "none"],
      },
    ],
  });

  const responsesFallbackProvider = provider({
    id: "p-c",
    name: "Provider C",
    protocol: "responses",
    mappings: [
      {
        local_model: "local-b1",
        upstream_model: "remote-b1-alt",
        reasoning_efforts: ["medium"],
      },
    ],
  });

  const anthropicFallbackProvider = provider({
    id: "p-d",
    name: "Provider D",
    protocol: "anthropic_messages",
    mappings: [
      {
        local_model: "local-b2",
        upstream_model: "remote-b2",
        reasoning_efforts: ["max"],
      },
    ],
  });

  const regularCandidate = {
    provider_id: "p-a",
    local_model: "local-a1",
    upstream_model: "remote-a1",
    protocol: "chat_completions" as const,
    weight: 1,
  };

  /** A pool with one regular candidate and an ordered fallback model list. */
  function poolWith(overrides: Record<string, unknown>): GatewayModelPool {
    return {
      id: "pool-fallback",
      name: "Pool One",
      model: "pool-model",
      enabled: true,
      default_reasoning_effort: "high",
      candidates: [regularCandidate],
      ...overrides,
    } as unknown as GatewayModelPool;
  }

  function coverageRow(
    catalog: ReturnType<typeof aiGatewayModule.poolReasoningCatalog>,
    effort: string,
  ) {
    return catalog.reasoning_coverage.find((item) => item.effort === effort);
  }

  it("每个回退模型展开到全部服务商映射，并按精确映射的协议自动推导档位", () => {
    const catalog = aiGatewayModule.poolReasoningCatalog(
      [
        regularProvider,
        chatFallbackProvider,
        responsesFallbackProvider,
        anthropicFallbackProvider,
      ],
      poolWith({ fallback_models: ["local-b1", "local-b2"] }),
    );

    // One regular candidate plus two mappings for local-b1 and one for local-b2.
    expect(catalog.reasoning_efforts).toEqual(["medium", "high", "max", "none"]);
    expect(coverageRow(catalog, "medium")).toEqual({
      effort: "medium",
      mapped_candidates: 1,
      total_candidates: 4,
    });
    expect(coverageRow(catalog, "high")).toEqual({
      effort: "high",
      mapped_candidates: 1,
      total_candidates: 4,
    });
    expect(coverageRow(catalog, "max")).toEqual({
      effort: "max",
      mapped_candidates: 1,
      total_candidates: 4,
    });
    // `none` is an explicit disable only because the Chat mapping declares it.
    expect(coverageRow(catalog, "none")).toEqual({
      effort: "none",
      mapped_candidates: 1,
      total_candidates: 4,
    });
  });

  it("只使用来源真实声明的档位，自动展开不会发明控制或借用其它来源", () => {
    const declaredHigh = provider({
      id: "p-b",
      name: "Provider B",
      protocol: "chat_completions",
      mappings: [
        {
          local_model: "local-b1",
          upstream_model: "remote-b1",
          reasoning_efforts: ["high"],
        },
      ],
    });
    const undeclared = provider({
      id: "p-e",
      name: "Provider E",
      protocol: "responses",
      mappings: [
        { local_model: "local-b2", upstream_model: "remote-b2" },
      ],
    });
    const explicitNone = provider({
      id: "p-f",
      name: "Provider F",
      protocol: "anthropic_messages",
      mappings: [
        {
          local_model: "local-b3",
          upstream_model: "remote-b3",
          reasoning_efforts: ["none"],
        },
      ],
    });

    const catalog = aiGatewayModule.poolReasoningCatalog(
      [regularProvider, declaredHigh, undeclared, explicitNone],
      poolWith({ fallback_models: ["local-b1", "local-b2", "local-b3"] }),
    );

    expect(catalog.reasoning_efforts).toEqual(["high", "none"]);
    expect(coverageRow(catalog, "high")?.mapped_candidates).toBe(1);
    expect(coverageRow(catalog, "none")?.mapped_candidates).toBe(1);
    expect(coverageRow(catalog, "max")?.mapped_candidates).toBe(0);
    // The undeclared source contributes a candidate row but no invented control.
    expect(catalog.reasoning_coverage.every((row) => row.total_candidates === 4)).toBe(
      true,
    );
  });

  it("没有服务商映射的已存回退模型不产生虚假来源", () => {
    const catalog = aiGatewayModule.poolReasoningCatalog(
      [regularProvider],
      poolWith({ fallback_models: ["local-missing"] }),
    );

    expect(catalog.reasoning_efforts).toEqual([]);
    expect(coverageRow(catalog, "high")).toEqual({
      effort: "high",
      mapped_candidates: 0,
      total_candidates: 1,
    });
  });

  it("没有回退列表的池保持常规候选静态目录", () => {
    const catalog = aiGatewayModule.poolReasoningCatalog(
      [regularProvider, chatFallbackProvider],
      poolWith({ fallback_models: [] }),
    );

    expect(catalog.reasoning_efforts).toEqual([]);
    expect(coverageRow(catalog, "high")).toEqual({
      effort: "high",
      mapped_candidates: 0,
      total_candidates: 1,
    });
  });

  describe("formatContextWindowCompact and parseContextWindowInput", () => {
    it("formatContextWindowDisplay 遵循分段格式化规则（<10000 保持千分位，>=10000 为 K，>=1000000 为 M）", () => {
      expect(formatContextWindowDisplay(null)).toBe("");
      expect(formatContextWindowDisplay(undefined)).toBe("");
      expect(formatContextWindowDisplay(0)).toBe("");
      expect(formatContextWindowDisplay(-1)).toBe("");

      // 小于 10,000 保持千分位
      expect(formatContextWindowDisplay(4096)).toBe("4,096");
      expect(formatContextWindowDisplay(8192)).toBe("8,192");
      expect(formatContextWindowDisplay(9999)).toBe("9,999");

      // 大于等于 10,000 格式化为 K
      expect(formatContextWindowDisplay(10000)).toBe("10K");
      expect(formatContextWindowDisplay(16000)).toBe("16K");
      expect(formatContextWindowDisplay(64000)).toBe("64K");
      expect(formatContextWindowDisplay(128000)).toBe("128K");
      expect(formatContextWindowDisplay(131072)).toBe("128K");
      expect(formatContextWindowDisplay(256000)).toBe("256K");
      expect(formatContextWindowDisplay(512000)).toBe("512K");

      // 大于等于 1,000,000 格式化为 M（四舍五入至多 1 位小数）
      expect(formatContextWindowDisplay(1000000)).toBe("1M");
      expect(formatContextWindowDisplay(1048576)).toBe("1M");
      expect(formatContextWindowDisplay(1040000)).toBe("1M");
      expect(formatContextWindowDisplay(1050000)).toBe("1.1M");
      expect(formatContextWindowDisplay(1060000)).toBe("1.1M");
      expect(formatContextWindowDisplay(1500000)).toBe("1.5M");
      expect(formatContextWindowDisplay(2000000)).toBe("2M");
      expect(formatContextWindowDisplay(2097152)).toBe("2M");
    });

    it("formatContextWindowCompact 格式化常用上下文长度单位", () => {
      expect(formatContextWindowCompact(null)).toBe("");
      expect(formatContextWindowCompact(undefined)).toBe("");
      expect(formatContextWindowCompact(0)).toBe("");
      expect(formatContextWindowCompact(-100)).toBe("");

      // K 级别十进制与二进制
      expect(formatContextWindowCompact(4000)).toBe("4K");
      expect(formatContextWindowCompact(4096)).toBe("4K");
      expect(formatContextWindowCompact(8000)).toBe("8K");
      expect(formatContextWindowCompact(8192)).toBe("8K");
      expect(formatContextWindowCompact(16000)).toBe("16K");
      expect(formatContextWindowCompact(16384)).toBe("16K");
      expect(formatContextWindowCompact(32000)).toBe("32K");
      expect(formatContextWindowCompact(32768)).toBe("32K");
      expect(formatContextWindowCompact(64000)).toBe("64K");
      expect(formatContextWindowCompact(65536)).toBe("64K");
      expect(formatContextWindowCompact(128000)).toBe("128K");
      expect(formatContextWindowCompact(131072)).toBe("128K");
      expect(formatContextWindowCompact(200000)).toBe("200K");
      expect(formatContextWindowCompact(256000)).toBe("256K");
      expect(formatContextWindowCompact(262144)).toBe("256K");
      expect(formatContextWindowCompact(400000)).toBe("400K");
      expect(formatContextWindowCompact(512000)).toBe("512K");

      // M 级别
      expect(formatContextWindowCompact(1000000)).toBe("1M");
      expect(formatContextWindowCompact(1048576)).toBe("1M");
      expect(formatContextWindowCompact(1500000)).toBe("1.5M");
      expect(formatContextWindowCompact(2000000)).toBe("2M");
      expect(formatContextWindowCompact(2097152)).toBe("2M");

      // 非整倍数四舍五入
      expect(formatContextWindowCompact(12345)).toBe("12.3K");
      expect(formatContextWindowCompact(1050000)).toBe("1.1M");
    });

    it("parseContextWindowInput 仅支持纯数字 tokens 输入，不使用单位转换", () => {
      // 空值
      expect(parseContextWindowInput("")).toEqual({ value: null, invalid: false });
      expect(parseContextWindowInput("   ")).toEqual({ value: null, invalid: false });

      // 纯数字支持
      expect(parseContextWindowInput("131072")).toEqual({ value: 131072, invalid: false });
      expect(parseContextWindowInput("128000")).toEqual({ value: 128000, invalid: false });
      expect(parseContextWindowInput("1000000")).toEqual({ value: 1000000, invalid: false });
      expect(parseContextWindowInput("64000")).toEqual({ value: 64000, invalid: false });
      expect(parseContextWindowInput("4096")).toEqual({ value: 4096, invalid: false });

      // 带单位与非法输入均判定为 invalid
      expect(parseContextWindowInput("1M")).toEqual({ value: null, invalid: true });
      expect(parseContextWindowInput("1m")).toEqual({ value: null, invalid: true });
      expect(parseContextWindowInput("2M")).toEqual({ value: null, invalid: true });
      expect(parseContextWindowInput("1.5M")).toEqual({ value: null, invalid: true });
      expect(parseContextWindowInput("256K")).toEqual({ value: null, invalid: true });
      expect(parseContextWindowInput("256k")).toEqual({ value: null, invalid: true });
      expect(parseContextWindowInput("128K")).toEqual({ value: null, invalid: true });
      expect(parseContextWindowInput("200K")).toEqual({ value: null, invalid: true });
      expect(parseContextWindowInput("64 K")).toEqual({ value: null, invalid: true });
      expect(parseContextWindowInput("abc")).toEqual({ value: null, invalid: true });
      expect(parseContextWindowInput("0")).toEqual({ value: null, invalid: true });
      expect(parseContextWindowInput("0K")).toEqual({ value: null, invalid: true });
      expect(parseContextWindowInput("-100")).toEqual({ value: null, invalid: true });
      expect(parseContextWindowInput("1.2.3")).toEqual({ value: null, invalid: true });
    });

    it("CONTEXT_WINDOW_PRESETS 包含常用上下文预设", () => {
      expect(CONTEXT_WINDOW_PRESETS).toContain("128K");
      expect(CONTEXT_WINDOW_PRESETS).toContain("200K");
      expect(CONTEXT_WINDOW_PRESETS).toContain("256K");
      expect(CONTEXT_WINDOW_PRESETS).toContain("1M");
      expect(CONTEXT_WINDOW_PRESETS).toContain("2M");
    });

    it("resolvePoolContextWindow 优先使用模型池显式配置的对外统一上下文限制", () => {
      const providers: any[] = [
        {
          id: "p1",
          protocol: "chat_completions",
          mappings: [
            { local_model: "m1", upstream_model: "m1", context_window: 128000 },
          ],
        },
      ];

      // 未配置 context_window 时，返回候选模型保守有效上下文
      const poolWithoutExplicit: any = {
        id: "pool-1",
        model: "free-model",
        candidates: [
          { provider_id: "p1", local_model: "m1", upstream_model: "m1", protocol: "chat_completions" },
        ],
      };
      expect(resolvePoolContextWindow(providers, poolWithoutExplicit)).toBe(128000);

      // 配置了 context_window 时（如 1050000），直接使用显式统一限制
      const poolWithExplicit: any = {
        ...poolWithoutExplicit,
        context_window: 1050000,
      };
      expect(resolvePoolContextWindow(providers, poolWithExplicit)).toBe(1050000);
    });
  });
});

