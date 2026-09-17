import { beforeEach, describe, expect, it } from "vitest";
import {
  API_FUSION_DEFAULT_PORT,
  API_FUSION_KEY_MASK,
  aggregateModels,
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
  formatFusionTimestamp,
  localBaseUrl,
  maskSecret,
  resolveDefaultKeyId,
  resolveMappingPreview,
  type FusionConfig,
  type FusionKey,
  type FusionUpstreamProvider,
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

describe("aggregateModels 聚合本地模型", () => {
  it("仅统计启用且未自动禁用的服务商，去重模型并保留全部上游来源与解析协议", () => {
    const providers: FusionUpstreamProvider[] = [
      provider({
        id: "pa",
        name: "Alpha",
        protocol: "responses",
        default_model: "gpt-4o",
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
      provider({ id: "pe", name: "Epsilon", default_model: "   ", mappings: [] }),
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
            upstreamModel: "gpt-4o",
            endpoint: "responses",
            isDefault: true,
          },
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
            upstreamModel: "gpt-4o",
            endpoint: "chat_completions",
            isDefault: true,
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
    const providers: FusionUpstreamProvider[] = [
      provider({
        id: "p-ghost",
        name: "Ghost",
        default_model: null,
        mappings: [{ local_model: "ghost", upstream_model: "   " }],
      }),
    ];

    expect(aggregateModels(providers)).toEqual([]);
  });

  it("默认模型与同名空白远端映射共存时只保留默认条目一次", () => {
    const providers: FusionUpstreamProvider[] = [
      provider({
        id: "p1",
        name: "Provider",
        default_model: "gpt-4o",
        mappings: [{ local_model: "gpt-4o", upstream_model: "" }],
      }),
    ];

    const aggregated = aggregateModels(providers);
    const gpt = aggregated.filter((entry) => entry.model === "gpt-4o");
    expect(gpt).toHaveLength(1);
    expect(gpt[0].providers).toHaveLength(1);
    expect(gpt[0].providers.filter((entry) => entry.isDefault)).toHaveLength(1);
    expect(gpt[0].providers[0]).toMatchObject({
      providerId: "p1",
      providerName: "Provider",
      upstreamModel: "gpt-4o",
      endpoint: "chat_completions",
      isDefault: true,
    });
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
});
