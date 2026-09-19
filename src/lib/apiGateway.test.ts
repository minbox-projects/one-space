import { beforeEach, describe, expect, it } from "vitest";
import {
  API_GATEWAY_DEFAULT_PORT,
  API_GATEWAY_KEY_MASK,
  aggregateModels,
  apiGatewayConfigureTerminal,
  apiGatewayDeleteKey,
  apiGatewayDeleteProvider,
  apiGatewayGetConfig,
  apiGatewayReenableProvider,
  apiGatewaySaveConfig,
  apiGatewaySetDefaultKey,
  apiGatewaySetProviderEnabled,
  apiGatewayStart,
  apiGatewayStatus,
  apiGatewayStop,
  apiGatewaySyncTerminal,
  apiGatewayTerminalTargets,
  apiGatewayUpsertKey,
  apiGatewayUpsertProvider,
  apiGatewayRequestLogs,
  apiGatewayUsageRetentionGet,
  apiGatewayUsageRetentionSave,
  apiGatewayUsageStats,
  clampUsagePage,
  formatGatewayTimestamp,
  formatUsageAmount,
  formatUsageGroupLabel,
  formatUsageRowAmount,
  formatUtc8DateTime,
  formatUtc8Day,
  formatUtc8Hour,
  isUnpricedOnly,
  localBaseUrl,
  maskSecret,
  resolveAggregatedModelName,
  resolveDefaultKeyId,
  resolveMappingPreview,
  usageRangeToDays,
  usageStatusTranslationKey,
  type GatewayConfig,
  type GatewayKey,
  type GatewayUpstreamProvider,
  type ModelPrice,
  type UsageMetrics,
} from "@/lib/apiGateway";
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

function provider(
  overrides: Partial<GatewayUpstreamProvider> = {},
): GatewayUpstreamProvider {
  return {
    id: "p1",
    name: "Provider",
    base_url: "https://upstream.example",
    api_key: API_GATEWAY_KEY_MASK,
    default_model: null,
    mappings: [],
    enabled: true,
    auto_disabled: false,
    disabled_reason: null,
    disabled_at: null,
    consecutive_failures: 0,
    last_error_at: null,
    ...overrides,
  };
}

function config(overrides: Partial<GatewayConfig> = {}): GatewayConfig {
  return {
    enabled: false,
    port: API_GATEWAY_DEFAULT_PORT,
    providers: [],
    keys: [],
    default_key_id: null,
    terminal_syncs: [],
    ...overrides,
  };
}

describe("apiGateway 命令封装", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  it("按逐字命令名与 camelCase 参数调用配置读写", async () => {
    const draft = config();
    await apiGatewayGetConfig();
    await apiGatewaySaveConfig(draft);
    await apiGatewayUpsertProvider(provider());
    await apiGatewayDeleteProvider("p1");
    await apiGatewaySetProviderEnabled("p1", false);
    await apiGatewayReenableProvider("p1");

    expect(invokeMock).toHaveBeenCalledWith("api_gateway_get_config");
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_save_config", {
      config: draft,
    });
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_upsert_provider", {
      provider: provider(),
      prices: null,
    });
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_delete_provider", {
      providerId: "p1",
    });
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_set_provider_enabled", {
      providerId: "p1",
      enabled: false,
    });
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_reenable_provider", {
      providerId: "p1",
    });
  });

  it("按 camelCase 参数调用 Key、启停与终端同步命令", async () => {
    await apiGatewayUpsertKey(key());
    await apiGatewayDeleteKey("k1");
    await apiGatewaySetDefaultKey("k1");
    await apiGatewayStart();
    await apiGatewayStop();
    await apiGatewayStatus();
    await apiGatewayTerminalTargets();
    await apiGatewayConfigureTerminal(["t-open"]);
    await apiGatewaySyncTerminal(["t-open"]);

    expect(invokeMock).toHaveBeenCalledWith("api_gateway_upsert_key", {
      key: key(),
    });
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_delete_key", {
      keyId: "k1",
    });
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_set_default_key", {
      keyId: "k1",
    });
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_start");
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_stop");
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_status");
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_terminal_targets");
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_configure_terminal", {
      targetTools: ["t-open"],
    });
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_sync_terminal", {
      targetTools: ["t-open"],
    });
  });

  it("未指定目标时同步命令不携带目标载荷", async () => {
    await apiGatewaySyncTerminal();
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_sync_terminal", {});
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

describe("localBaseUrl 本地 Api 地址", () => {
  it("固定绑定回环地址与端口", () => {
    expect(localBaseUrl(API_GATEWAY_DEFAULT_PORT)).toBe("http://127.0.0.1:17688/v1");
    expect(localBaseUrl(18000)).toBe("http://127.0.0.1:18000/v1");
  });
});

describe("resolveMappingPreview 模型解析预览", () => {
  it("映射行协议优先于服务商协议", () => {
    const p = provider({
      protocol: "chat_completions",
      mappings: [
        { local_model: "local-a", upstream_model: "remote-a", protocol: "responses" },
      ],
      default_model: "remote-default",
    });
    expect(resolveMappingPreview(p, "local-a")).toEqual({
      upstreamModel: "remote-a",
      endpoint: "responses",
    });
  });

  it("映射行未声明协议时继承服务商协议", () => {
    const p = provider({
      protocol: "responses",
      mappings: [{ local_model: "local-a", upstream_model: "remote-a" }],
      default_model: "remote-default",
    });
    expect(resolveMappingPreview(p, "local-a")).toEqual({
      upstreamModel: "remote-a",
      endpoint: "responses",
    });
  });

  it("服务商与映射行都未声明协议时回落 chat_completions", () => {
    const p = provider({
      mappings: [{ local_model: "local-a", upstream_model: "remote-a" }],
      default_model: "remote-default",
    });
    expect(resolveMappingPreview(p, "local-a")).toEqual({
      upstreamModel: "remote-a",
      endpoint: "chat_completions",
    });
  });

  it("未命中映射时回退默认模型并使用服务商协议", () => {
    const p = provider({
      protocol: "responses",
      mappings: [{ local_model: "local-a", upstream_model: "remote-a" }],
      default_model: "remote-default",
    });
    expect(resolveMappingPreview(p, "local-unknown")).toEqual({
      upstreamModel: "remote-default",
      endpoint: "responses",
    });
  });

  it("既无匹配映射也无默认模型时不可解析", () => {
    const p = provider({ mappings: [], default_model: null });
    expect(resolveMappingPreview(p, "local-a")).toBeNull();
  });

  it("远端模型为空白的映射行不构成命中", () => {
    const p = provider({
      protocol: "responses",
      mappings: [
        { local_model: "local-a", upstream_model: "   ", protocol: "chat_completions" },
      ],
      default_model: "remote-default",
    });
    expect(resolveMappingPreview(p, "local-a")).toEqual({
      upstreamModel: "remote-default",
      endpoint: "responses",
    });

    const noDefault = provider({
      mappings: [{ local_model: "local-a", upstream_model: "   " }],
      default_model: null,
    });
    expect(resolveMappingPreview(noDefault, "local-a")).toBeNull();
  });

  it("禁用映射行既不成命中也不回退默认模型", () => {
    const p = provider({
      mappings: [
        { local_model: "local-a", upstream_model: "remote-a", enabled: false },
      ],
      default_model: "remote-default",
    });
    expect(
      resolveMappingPreview(p, "local-a"),
      "禁用映射不应命中，也不应回退到默认模型",
    ).toBeNull();
  });

  it("显式启用映射行正常命中", () => {
    const p = provider({
      mappings: [
        { local_model: "local-a", upstream_model: "remote-a", enabled: true },
      ],
      default_model: "remote-default",
    });
    expect(resolveMappingPreview(p, "local-a")).toEqual({
      upstreamModel: "remote-a",
      endpoint: "chat_completions",
    });
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
    ...overrides,
  };
}

describe("usageRangeToDays 快捷范围映射", () => {
  it("今日映射为 1、近 N 天映射为 N", () => {
    expect(usageRangeToDays("today")).toBe(1);
    expect(usageRangeToDays("7d")).toBe(7);
    expect(usageRangeToDays("15d")).toBe(15);
    expect(usageRangeToDays("30d")).toBe(30);
  });

  it("全部范围映射为 null", () => {
    expect(usageRangeToDays("all")).toBeNull();
  });
});

describe("UTC+8 时间标签", () => {
  it("跨 UTC 日界时按 UTC+8 展示日期与时间", () => {
    // 2026-09-17T18:30:00Z == 2026-09-18 02:30 (UTC+8)
    expect(formatUtc8DateTime(Date.UTC(2026, 8, 17, 18, 30))).toBe(
      "2026-09-18 02:30",
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
  it("映射三种结果到稳定文案键", () => {
    expect(usageStatusTranslationKey("success")).toBe("apiGatewayStatusSuccess");
    expect(usageStatusTranslationKey("failure")).toBe("apiGatewayStatusFailure");
    expect(usageStatusTranslationKey("cancelled")).toBe(
      "apiGatewayStatusCancelled",
    );
  });
});

describe("用量与日志命令封装", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  it("按 camelCase 参数调用用量与保留天数命令", async () => {
    await apiGatewayUsageStats(7);
    await apiGatewayUsageStats(null);
    await apiGatewayRequestLogs({
      days: 1,
      groupBy: "day",
      status: "failure",
      model: "gpt-4o",
      page: 2,
    });
    await apiGatewayUsageRetentionGet();
    await apiGatewayUsageRetentionSave(30);

    expect(invokeMock).toHaveBeenCalledWith("api_gateway_usage_stats", {
      days: 7,
    });
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_usage_stats", {
      days: null,
    });
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_request_logs", {
      days: 1,
      groupBy: "day",
      status: "failure",
      model: "gpt-4o",
      page: 2,
    });
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_usage_retention_get");
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_usage_retention_save", {
      days: 30,
    });
  });

  it("removed_legacy_price_wrappers_are_not_exported", async () => {
    const gatewayModule = await import("@/lib/apiGateway");
    expect("apiGatewayModelPricesGet" in gatewayModule).toBe(false);
    expect("apiGatewayModelPricesSave" in gatewayModule).toBe(false);
    expect("getProviderAvailableModels" in gatewayModule).toBe(false);
  });

  it("查询参数缺省时携带空筛选与第 1 页", async () => {
    await apiGatewayRequestLogs({ days: null });
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_request_logs", {
      days: null,
      groupBy: null,
      status: null,
      model: null,
      page: 1,
    });
  });
});

describe("aggregateModels 聚合本地模型", () => {
  it("仅统计启用且未自动禁用的服务商，去重模型并保留全部映射上游来源与解析协议，不包含默认模型", () => {
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
        auto_disabled: true,
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
          auto_disabled: true,
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

// ---------------------------------------------------------------------------
// Step 5: provider template types, command wrappers and display helpers.
// RED tests for the frozen interface contract. Only this test file is touched.
// ---------------------------------------------------------------------------

import type { TFunction } from "i18next";
import {
  apiGatewayCreateProviderFromTemplate,
  apiGatewayDeleteProviderModel,
  apiGatewayProviderTemplates,
  apiGatewayRestoreProviderModel,
  apiGatewaySyncProviderTemplate,
  formatOffPeakDays,
  gatewayWeekdayTranslationKey,
  GATEWAY_WEEKDAY_ORDER,
  isMappingDeprecated,
  normalizeReasoningEfforts,
  type CreateProviderFromTemplateRequest,
  type GatewayModelMapping,
  type GatewayProviderTemplate,
  type GatewayProviderTemplateView,
  type OffPeakPrice,
} from "@/lib/apiGateway";

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
    snapshot_version: "2026-09-18",
    models: [
      {
        upstream_model: "model-a",
        input: 1,
        cache_read: 0.1,
        cache_write: 0.2,
        output: 2,
      },
    ],
    ...overrides,
  };
}

describe("apiGateway provider template helpers", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  it("gatewayWeekdayOrderIsMondayFirstAndZeroIsSunday", () => {
    expect(GATEWAY_WEEKDAY_ORDER).toEqual([1, 2, 3, 4, 5, 6, 0]);

    expect(gatewayWeekdayTranslationKey(0)).toBe("apiGatewayWeekdaySun");
    expect(gatewayWeekdayTranslationKey(1)).toBe("apiGatewayWeekdayMon");
    expect(gatewayWeekdayTranslationKey(2)).toBe("apiGatewayWeekdayTue");
    expect(gatewayWeekdayTranslationKey(3)).toBe("apiGatewayWeekdayWed");
    expect(gatewayWeekdayTranslationKey(4)).toBe("apiGatewayWeekdayThu");
    expect(gatewayWeekdayTranslationKey(5)).toBe("apiGatewayWeekdayFri");
    expect(gatewayWeekdayTranslationKey(6)).toBe("apiGatewayWeekdaySat");

    expect(gatewayWeekdayTranslationKey(7)).toBe("");
    expect(gatewayWeekdayTranslationKey(-1)).toBe("");
  });

  it("formatOffPeakDaysTreatsAbsentAndEmptyAsEveryDay", () => {
    expect(formatOffPeakDays(undefined, identityT)).toBe("apiGatewayEveryDay");
    expect(formatOffPeakDays(null, identityT)).toBe("apiGatewayEveryDay");
    expect(formatOffPeakDays([], identityT)).toBe("apiGatewayEveryDay");
  });

  it("formatOffPeakDaysDeduplicatesFiltersAndOrdersDays", () => {
    expect(formatOffPeakDays([5, 1, 1, 9], identityT)).toBe(
      "apiGatewayWeekdayMon, apiGatewayWeekdayFri",
    );
    expect(formatOffPeakDays([0, 6], identityT)).toBe(
      "apiGatewayWeekdaySat, apiGatewayWeekdaySun",
    );
    expect(formatOffPeakDays([9, -1], identityT)).toBe("apiGatewayEveryDay");
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

  it("providerTemplateWrappersPassExactCommandArguments", async () => {
    const request: CreateProviderFromTemplateRequest = {
      templateId: "tpl-1",
      name: "My Provider",
      baseUrl: "https://upstream.example",
      protocol: "responses",
      apiKey: "test-key",
    };

    const templatesCall = apiGatewayProviderTemplates();
    const syncCall = apiGatewaySyncProviderTemplate("tpl-1");
    const createCall = apiGatewayCreateProviderFromTemplate(request);
    const deleteCall = apiGatewayDeleteProviderModel("p1", "model-a");
    const restoreCall = apiGatewayRestoreProviderModel("p1", "model-a");

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

    expect(invokeMock).toHaveBeenCalledWith("api_gateway_provider_templates");
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_sync_provider_template", {
      templateId: "tpl-1",
    });
    expect(invokeMock).toHaveBeenCalledWith(
      "api_gateway_create_provider_from_template",
      {
        templateId: "tpl-1",
        name: "My Provider",
        baseUrl: "https://upstream.example",
        protocol: "responses",
        apiKey: "test-key",
      },
    );
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_delete_provider_model", {
      providerId: "p1",
      upstreamModel: "model-a",
    });
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_restore_provider_model", {
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
} from "@/lib/apiGateway";

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

describe("apiGateway provider price helpers", () => {
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
    expect(echoedRow!.off_peak).toEqual(echoedRow!.off_peaks![0]);

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
    expect(enabledRow!.off_peak).toEqual(enabledRow!.off_peaks![0]);
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

  it("price_row_to_draft_handles_missing_and_legacy_off_peak", () => {
    const blank = priceRowToDraft(null, "model-x", "row-9");
    expect(blank.upstream_model).toBe("model-x");
    expect(blank.input).toBe("");
    expect(blank.cache_read).toBe("");
    expect(blank.cache_write).toBe("");
    expect(blank.output).toBe("");
    expect(blank.enable_off_peak).toBe(false);
    expect(blank.off_peaks).toEqual([]);

    const legacy = priceRowToDraft(
      modelPrice({
        upstream_model: "model-y",
        output: 3,
        off_peak: {
          start_time: undefined as unknown as string,
          end_time: undefined as unknown as string,
          input: 1,
          cache_read: 0.5,
          cache_write: undefined as unknown as number,
          output: 3,
        },
      }),
      "model-y",
      "row-9",
    );
    expect(legacy.upstream_model).toBe("model-y");
    expect(legacy.output).toBe("3");
    expect(legacy.enable_off_peak).toBe(true);
    expect(legacy.off_peaks).toHaveLength(1);
    expect(legacy.off_peaks[0].id).toBe("row-9-op-0");
    expect(legacy.off_peaks[0].start_time).toBe("00:30");
    expect(legacy.off_peaks[0].end_time).toBe("08:30");
    expect(legacy.off_peaks[0].input).toBe("1");
    expect(legacy.off_peaks[0].cache_read).toBe("0.5");
    expect(legacy.off_peaks[0].cache_write).toBe("");
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

    const withPrices = await apiGatewayUpsertProvider(p, prices);
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_upsert_provider", {
      provider: p,
      prices,
    });
    expect(withPrices).toBe(saved);

    invokeMock.mockClear();
    invokeMock.mockResolvedValueOnce(saved);
    const withoutPrices = await apiGatewayUpsertProvider(p);
    expect(invokeMock).toHaveBeenCalledWith("api_gateway_upsert_provider", {
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
