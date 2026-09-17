import { beforeEach, describe, expect, it } from "vitest";
import {
  API_FUSION_DEFAULT_PORT,
  API_FUSION_KEY_MASK,
  apiFusionConfigureTerminal,
  apiFusionDeleteKey,
  apiFusionDeleteProvider,
  apiFusionGetConfig,
  apiFusionReenableProvider,
  apiFusionSaveConfig,
  apiFusionSetDefaultKey,
  apiFusionSetProviderEnabled,
  apiFusionStart,
  apiFusionStatus,
  apiFusionStop,
  apiFusionSyncTerminal,
  apiFusionTerminalTargets,
  apiFusionUpsertKey,
  apiFusionUpsertProvider,
  apiFusionModelPricesGet,
  apiFusionModelPricesSave,
  apiFusionRequestLogs,
  apiFusionUsageRetentionGet,
  apiFusionUsageRetentionSave,
  apiFusionUsageStats,
  clampUsagePage,
  formatFusionTimestamp,
  formatUsageAmount,
  formatUsageGroupLabel,
  formatUsageRowAmount,
  formatUtc8DateTime,
  formatUtc8Day,
  formatUtc8Hour,
  isUnpricedOnly,
  localBaseUrl,
  maskSecret,
  resolveDefaultKeyId,
  resolveMappingPreview,
  usageRangeToDays,
  usageStatusTranslationKey,
  type FusionConfig,
  type FusionKey,
  type FusionUpstreamProvider,
  type ModelPrice,
  type UsageMetrics,
} from "@/lib/apiFusion";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

function key(overrides: Partial<FusionKey> = {}): FusionKey {
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
  overrides: Partial<FusionUpstreamProvider> = {},
): FusionUpstreamProvider {
  return {
    id: "p1",
    name: "Provider",
    base_url: "https://upstream.example",
    api_key: API_FUSION_KEY_MASK,
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

function config(overrides: Partial<FusionConfig> = {}): FusionConfig {
  return {
    enabled: false,
    port: API_FUSION_DEFAULT_PORT,
    providers: [],
    keys: [],
    default_key_id: null,
    terminal_syncs: [],
    ...overrides,
  };
}

describe("apiFusion 命令封装", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  it("按逐字命令名与 camelCase 参数调用配置读写", async () => {
    const draft = config();
    await apiFusionGetConfig();
    await apiFusionSaveConfig(draft);
    await apiFusionUpsertProvider(provider());
    await apiFusionDeleteProvider("p1");
    await apiFusionSetProviderEnabled("p1", false);
    await apiFusionReenableProvider("p1");

    expect(invokeMock).toHaveBeenCalledWith("api_fusion_get_config");
    expect(invokeMock).toHaveBeenCalledWith("api_fusion_save_config", {
      config: draft,
    });
    expect(invokeMock).toHaveBeenCalledWith("api_fusion_upsert_provider", {
      provider: provider(),
    });
    expect(invokeMock).toHaveBeenCalledWith("api_fusion_delete_provider", {
      providerId: "p1",
    });
    expect(invokeMock).toHaveBeenCalledWith("api_fusion_set_provider_enabled", {
      providerId: "p1",
      enabled: false,
    });
    expect(invokeMock).toHaveBeenCalledWith("api_fusion_reenable_provider", {
      providerId: "p1",
    });
  });

  it("按 camelCase 参数调用 Key、启停与终端同步命令", async () => {
    await apiFusionUpsertKey(key());
    await apiFusionDeleteKey("k1");
    await apiFusionSetDefaultKey("k1");
    await apiFusionStart();
    await apiFusionStop();
    await apiFusionStatus();
    await apiFusionTerminalTargets();
    await apiFusionConfigureTerminal(["t-open"]);
    await apiFusionSyncTerminal(["t-open"]);

    expect(invokeMock).toHaveBeenCalledWith("api_fusion_upsert_key", {
      key: key(),
    });
    expect(invokeMock).toHaveBeenCalledWith("api_fusion_delete_key", {
      keyId: "k1",
    });
    expect(invokeMock).toHaveBeenCalledWith("api_fusion_set_default_key", {
      keyId: "k1",
    });
    expect(invokeMock).toHaveBeenCalledWith("api_fusion_start");
    expect(invokeMock).toHaveBeenCalledWith("api_fusion_stop");
    expect(invokeMock).toHaveBeenCalledWith("api_fusion_status");
    expect(invokeMock).toHaveBeenCalledWith("api_fusion_terminal_targets");
    expect(invokeMock).toHaveBeenCalledWith("api_fusion_configure_terminal", {
      targetTools: ["t-open"],
    });
    expect(invokeMock).toHaveBeenCalledWith("api_fusion_sync_terminal", {
      targetTools: ["t-open"],
    });
  });

  it("未指定目标时同步命令不携带目标载荷", async () => {
    await apiFusionSyncTerminal();
    expect(invokeMock).toHaveBeenCalledWith("api_fusion_sync_terminal", {});
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
    expect(localBaseUrl(API_FUSION_DEFAULT_PORT)).toBe("http://127.0.0.1:17688");
    expect(localBaseUrl(18000)).toBe("http://127.0.0.1:18000");
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

describe("formatFusionTimestamp", () => {
  it("空值返回 null", () => {
    expect(formatFusionTimestamp(null)).toBeNull();
  });

  it("以稳定格式展示时间", () => {
    const formatted = formatFusionTimestamp(1_700_000_000);
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
    expect(usageStatusTranslationKey("success")).toBe("apiFusionStatusSuccess");
    expect(usageStatusTranslationKey("failure")).toBe("apiFusionStatusFailure");
    expect(usageStatusTranslationKey("cancelled")).toBe(
      "apiFusionStatusCancelled",
    );
  });
});

describe("用量与日志命令封装", () => {
  beforeEach(() => {
    resetTauriMocks();
  });

  it("按 camelCase 参数调用六个用量/价格/保留天数命令", async () => {
    await apiFusionUsageStats(7);
    await apiFusionUsageStats(null);
    await apiFusionRequestLogs({
      days: 1,
      groupBy: "day",
      status: "failure",
      model: "gpt-4o",
      page: 2,
    });
    await apiFusionModelPricesGet();
    const prices: ModelPrice[] = [
      { upstream_model: "gpt-4o", input: 1, cache_read: 0.1, cache_write: 0.2, output: 2 },
    ];
    await apiFusionModelPricesSave(prices);
    await apiFusionUsageRetentionGet();
    await apiFusionUsageRetentionSave(30);

    expect(invokeMock).toHaveBeenCalledWith("api_fusion_usage_stats", {
      days: 7,
    });
    expect(invokeMock).toHaveBeenCalledWith("api_fusion_usage_stats", {
      days: null,
    });
    expect(invokeMock).toHaveBeenCalledWith("api_fusion_request_logs", {
      days: 1,
      groupBy: "day",
      status: "failure",
      model: "gpt-4o",
      page: 2,
    });
    expect(invokeMock).toHaveBeenCalledWith("api_fusion_model_prices_get");
    expect(invokeMock).toHaveBeenCalledWith("api_fusion_model_prices_save", {
      prices,
    });
    expect(invokeMock).toHaveBeenCalledWith("api_fusion_usage_retention_get");
    expect(invokeMock).toHaveBeenCalledWith("api_fusion_usage_retention_save", {
      days: 30,
    });
  });

  it("查询参数缺省时携带空筛选与第 1 页", async () => {
    await apiFusionRequestLogs({ days: null });
    expect(invokeMock).toHaveBeenCalledWith("api_fusion_request_logs", {
      days: null,
      groupBy: null,
      status: null,
      model: null,
      page: 1,
    });
  });
});
