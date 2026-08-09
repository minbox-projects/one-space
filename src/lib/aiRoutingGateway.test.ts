import { describe, expect, it, vi } from "vitest";
import {
  aiRoutingGatewayAccountCreateApiKey,
  aiRoutingGatewayAccountCreateApiKeyWithConfiguration,
  aiRoutingGatewayAccountUpdate,
  aiRoutingGatewayAccountsDelete,
  aiRoutingGatewayAccountsDeleteConfirmation,
  aiRoutingGatewayAccountsDisable,
  aiRoutingGatewayBootstrap,
  aiRoutingGatewayGroupCreate,
  aiRoutingGatewayGroupDelete,
  aiRoutingGatewayGroupRename,
  aiRoutingGatewayKeyCreate,
  aiRoutingGatewayKeyCopy,
  aiRoutingGatewayKeyConvertToProviders,
  aiRoutingGatewayKeyConvertibleTools,
  aiRoutingGatewayKeyDelete,
  aiRoutingGatewayKeyDisplayGroupCreate,
  aiRoutingGatewayKeyDisplayGroupDelete,
  aiRoutingGatewayKeyDisplayGroupRename,
  aiRoutingGatewayKeyDisplayGroupsList,
  aiRoutingGatewayKeyGroupsUpdate,
  aiRoutingGatewayKeyList,
  aiRoutingGatewayKeyUpdate,
  aiRoutingGatewayLogsQuery,
  aiRoutingGatewayMappingSave,
  aiRoutingGatewayPriceSave,
  aiRoutingGatewaySettingsSave,
  aiRoutingGatewayStatsHome,
  subscribeAiRoutingGatewayEvents,
  type GatewayAccountEvent,
  type GatewayMaintenanceEvent,
  type GatewayOAuthEvent,
  type GatewayRuntime,
} from "@/lib/aiRoutingGateway";
import { invokeMock, listenMock, resetTauriMocks } from "@/test/mocks/tauri";

describe("AI routing gateway typed IPC facade", () => {
  it("matches the Rust runtime event payload fields", () => {
    const runtime: GatewayRuntime = {
      state: "error",
      availability: "error",
      port: 17689,
      run_enabled: true,
      error_code: "port_conflict",
      lock_reason: null,
    };

    expect(runtime).toMatchObject({ run_enabled: true, error_code: "port_conflict" });
  });

  it("matches the Rust account, OAuth, and maintenance event payloads", () => {
    const account: GatewayAccountEvent = {
      id: "account-1",
      stable_external_id: null,
      account_type: "api_key",
      name: "Fixture Account",
      group_id: "default",
      sort_order: 0,
      note: "safe fixture",
      enabled: true,
      health_status: "healthy",
      quota_threshold_override_percent: null,
      base_url: "http://127.0.0.1:18000/v1",
      auth_method: "bearer",
      upstream_protocol: "responses",
      tags: [],
      model_mappings: [],
    };
    const oauth: GatewayOAuthEvent = {
      sessionId: "fixture-session",
      state: "completed",
    };
    const maintenance: GatewayMaintenanceEvent = {
      operation: "cleanup",
      state: "completed",
      affectedRows: 0,
    };

    expect(account).toHaveProperty("account_type", "api_key");
    expect(oauth).toEqual({ sessionId: "fixture-session", state: "completed" });
    expect(maintenance).toEqual({ operation: "cleanup", state: "completed", affectedRows: 0 });
  });

  it("集中使用独立命令前缀和 camelCase 输入 DTO", async () => {
    resetTauriMocks();
    invokeMock.mockResolvedValue({});
    await aiRoutingGatewayBootstrap(15);
    await aiRoutingGatewayAccountCreateApiKey({ name: "Local", baseUrl: "http://127.0.0.1:18000/v1", apiKey: "SAFE_FIXTURE_API_KEY", authMethod: "bearer", upstreamProtocol: "responses", note: "SAFE_FIXTURE_NOTE" });
    await aiRoutingGatewayKeyCreate({ name: "CLI", groupIds: ["default"], modelIds: ["gpt-test"], expiresAt: null });
    await aiRoutingGatewayKeyCopy("key-1");
    await aiRoutingGatewayKeyGroupsUpdate("key-1", ["default", "team"]);
    await aiRoutingGatewayLogsQuery({ status: "failed", pageSize: 25 });
    await aiRoutingGatewaySettingsSave({ port: 17688, globalQuotaThresholdPercent: 10, logRetentionDays: 90, runEnabled: true });

    expect(invokeMock.mock.calls).toEqual([
      ["ai_routing_gateway_bootstrap", { days: 15 }],
      ["ai_routing_gateway_account_create_api_key", { input: { name: "Local", baseUrl: "http://127.0.0.1:18000/v1", apiKey: "SAFE_FIXTURE_API_KEY", authMethod: "bearer", upstreamProtocol: "responses", note: "SAFE_FIXTURE_NOTE" } }],
      ["ai_routing_gateway_key_create", { input: { name: "CLI", groupIds: ["default"], modelIds: ["gpt-test"], expiresAt: null } }],
      ["ai_routing_gateway_key_copy", { keyId: "key-1" }],
      ["ai_routing_gateway_key_groups_update", { input: { keyId: "key-1", groupIds: ["default", "team"] } }],
      ["ai_routing_gateway_logs_query", { input: { status: "failed", pageSize: 25 } }],
      ["ai_routing_gateway_settings_save", { input: { port: 17688, globalQuotaThresholdPercent: 10, logRetentionDays: 90, runEnabled: true } }],
    ]);
    expect((invokeMock.mock.calls as unknown[][]).every(([command]) => String(command).startsWith("ai_routing_gateway_") && !String(command).startsWith("protocol_router_"))).toBe(true);
  });

  it("原子创建使用固定命令和完整 camelCase 配置，并保留旧编辑 facade", async () => {
    resetTauriMocks();
    const account = { id: "account-new", account_type: "api_key" };
    invokeMock.mockResolvedValue(account);
    const input = {
      name: "Atomic",
      baseUrl: "https://api.example.com/v1",
      apiKey: "SAFE_ATOMIC_FIXTURE_KEY",
      authMethod: "api_key_header" as const,
      upstreamProtocol: "chat_completions" as const,
      groupId: "team",
      tags: ["priority", "team"],
      quotaThresholdOverridePercent: 75,
      note: "fixture",
      mappings: [{ publicModelId: "gpt-test", upstreamModelId: "vendor-model", enabled: false }],
      prices: [{
        publicModelId: "gpt-test",
        inputPerMillionUsd: "1",
        outputPerMillionUsd: "2",
        cacheReadPerMillionUsd: "0.1",
        cacheWritePerMillionUsd: "0.2",
      }],
    };

    await expect(aiRoutingGatewayAccountCreateApiKeyWithConfiguration(input)).resolves.toBe(account);
    await aiRoutingGatewayAccountUpdate({ accountId: "account-1", name: "Edited", groupId: "default", sortOrder: 0, note: "", enabled: true, tags: [] });
    await aiRoutingGatewayMappingSave({ accountId: "account-1", publicModelId: "gpt-test", upstreamModelId: "vendor", enabled: true });
    await aiRoutingGatewayPriceSave({ publicModelId: "gpt-test", accountId: "account-1", effectiveAt: "2026-08-06T00:00:00Z", inputPerMillionUsd: "1" });

    expect(invokeMock.mock.calls).toEqual([
      ["ai_routing_gateway_account_create_api_key_with_configuration", { input }],
      ["ai_routing_gateway_account_update", { input: { accountId: "account-1", name: "Edited", groupId: "default", sortOrder: 0, note: "", enabled: true, tags: [] } }],
      ["ai_routing_gateway_mapping_save", { input: { accountId: "account-1", publicModelId: "gpt-test", upstreamModelId: "vendor", enabled: true } }],
      ["ai_routing_gateway_price_save", { input: { publicModelId: "gpt-test", accountId: "account-1", effectiveAt: "2026-08-06T00:00:00Z", inputPerMillionUsd: "1" } }],
    ]);
  });

  it("分组与批量 facade 精确包装命令、集合和确认令牌", async () => {
    resetTauriMocks();
    invokeMock.mockResolvedValue({});

    await aiRoutingGatewayGroupCreate({ name: "Team", sortOrder: 2 });
    await aiRoutingGatewayGroupRename({ groupId: "team", name: "Platform" });
    await aiRoutingGatewayGroupDelete("team");
    await aiRoutingGatewayAccountsDisable(["account-1", "account-2"]);
    await aiRoutingGatewayAccountsDeleteConfirmation(["account-2", "account-1"]);
    await aiRoutingGatewayAccountsDelete(["account-1", "account-2"], "confirmation-token");

    expect(invokeMock.mock.calls).toEqual([
      ["ai_routing_gateway_group_create", { input: { name: "Team", sortOrder: 2 } }],
      ["ai_routing_gateway_group_rename", { input: { groupId: "team", name: "Platform" } }],
      ["ai_routing_gateway_group_delete", { groupId: "team" }],
      ["ai_routing_gateway_accounts_disable", { input: { accountIds: ["account-1", "account-2"] } }],
      ["ai_routing_gateway_accounts_delete_confirmation", { input: { accountIds: ["account-2", "account-1"] } }],
      ["ai_routing_gateway_accounts_delete", { input: { accountIds: ["account-1", "account-2"], confirmationToken: "confirmation-token" } }],
    ]);
  });

  it("密钥管理和转换只提交 camelCase 业务输入，不接受派生服务商字段", async () => {
    resetTauriMocks();
    invokeMock.mockResolvedValue({});
    await aiRoutingGatewayKeyDisplayGroupsList();
    await aiRoutingGatewayKeyDisplayGroupCreate("Team Keys");
    await aiRoutingGatewayKeyDisplayGroupRename("key-group-1", "Platform Keys");
    await aiRoutingGatewayKeyDisplayGroupDelete("key-group-1");
    await aiRoutingGatewayKeyList({
      groupId: "gateway-key-default",
      text: "fixture",
      status: "active",
      page: 1,
      pageSize: 20,
      sort: "createdNewest",
    });
    await aiRoutingGatewayKeyUpdate({
      keyId: "key-1",
      name: "Updated",
      displayGroupId: "gateway-key-default",
      groupIds: ["default"],
      modelIds: ["gpt-test"],
      expiresAt: null,
    });
    await aiRoutingGatewayKeyConvertibleTools("key-1");
    await aiRoutingGatewayKeyConvertToProviders({
      keyId: "key-1",
      tools: ["claude", "opencode"],
    });
    await aiRoutingGatewayKeyDelete("key-1");

    expect(invokeMock.mock.calls).toEqual([
      ["ai_routing_gateway_key_display_groups_list", undefined],
      ["ai_routing_gateway_key_display_group_create", { input: { name: "Team Keys" } }],
      ["ai_routing_gateway_key_display_group_rename", { input: { groupId: "key-group-1", name: "Platform Keys" } }],
      ["ai_routing_gateway_key_display_group_delete", { groupId: "key-group-1" }],
      ["ai_routing_gateway_key_list", { input: { groupId: "gateway-key-default", text: "fixture", status: "active", page: 1, pageSize: 20, sort: "createdNewest" } }],
      ["ai_routing_gateway_key_update", { input: { keyId: "key-1", name: "Updated", displayGroupId: "gateway-key-default", groupIds: ["default"], modelIds: ["gpt-test"], expiresAt: null } }],
      ["ai_routing_gateway_key_convertible_tools", { keyId: "key-1" }],
      ["ai_routing_gateway_key_convert_to_providers", { input: { keyId: "key-1", tools: ["claude", "opencode"], activate: false } }],
      ["ai_routing_gateway_key_delete", { keyId: "key-1" }],
    ]);
    const conversionInput = (invokeMock.mock.calls[7]?.[1] as { input: Record<string, unknown> }).input;
    expect(conversionInput).toEqual({ keyId: "key-1", tools: ["claude", "opencode"], activate: false });
    expect(conversionInput).not.toHaveProperty("baseUrl");
    expect(conversionInput).not.toHaveProperty("apiKey");
    expect(conversionInput).not.toHaveProperty("serviceProviderId");
  });

  it("新增账号池 facade 失败时统一归一化错误", async () => {
    resetTauriMocks();
    invokeMock.mockRejectedValue({ category: "not_found", entityId: "account-missing" });

    await expect(aiRoutingGatewayAccountsDisable(["account-missing"])).rejects.toMatchObject({
      name: "AiRoutingGatewayError",
      message: JSON.stringify({ category: "not_found", entityId: "account-missing" }),
    });
  });

  it("归一化错误并释放全部已订阅事件", async () => {
    resetTauriMocks();
    invokeMock.mockRejectedValue("gateway_locked");
    await expect(aiRoutingGatewayBootstrap()).rejects.toMatchObject({ name: "AiRoutingGatewayError", message: "gateway_locked" });
    const unlisteners = [vi.fn(), vi.fn(), vi.fn(), vi.fn()];
    listenMock.mockResolvedValueOnce(unlisteners[0]).mockResolvedValueOnce(unlisteners[1]).mockResolvedValueOnce(unlisteners[2]).mockResolvedValueOnce(unlisteners[3]);
    const cleanup = await subscribeAiRoutingGatewayEvents({ runtime: vi.fn(), account: vi.fn(), oauth: vi.fn(), maintenance: vi.fn() });
    expect((listenMock.mock.calls as unknown[][]).map(([event]) => event)).toEqual(["ai-routing-gateway-runtime", "ai-routing-gateway-account", "ai-routing-gateway-oauth", "ai-routing-gateway-maintenance"]);
    cleanup();
    for (const unlisten of unlisteners) expect(unlisten).toHaveBeenCalledOnce();
  });

  it("部分事件注册失败时释放已经成功注册的 listener", async () => {
    resetTauriMocks();
    const unlisteners = [vi.fn(), vi.fn(), vi.fn()];
    listenMock
      .mockResolvedValueOnce(unlisteners[0])
      .mockRejectedValueOnce(new Error("oauth_listener_failed"))
      .mockResolvedValueOnce(unlisteners[1])
      .mockResolvedValueOnce(unlisteners[2]);

    await expect(
      subscribeAiRoutingGatewayEvents({
        runtime: vi.fn(),
        account: vi.fn(),
        oauth: vi.fn(),
        maintenance: vi.fn(),
      }),
    ).rejects.toThrow("oauth_listener_failed");
    for (const unlisten of unlisteners) expect(unlisten).toHaveBeenCalledOnce();
  });

  it("把首页组合筛选传入 bootstrap 和 stats DTO", async () => {
    resetTauriMocks();
    invokeMock.mockResolvedValue({});
    const filters = { accountId: "account-1", groupId: "group-1", publicModelId: "model-1" };
    await aiRoutingGatewayBootstrap(7, filters);
    await aiRoutingGatewayStatsHome(30, filters);
    expect(invokeMock.mock.calls).toEqual([
      ["ai_routing_gateway_bootstrap", { days: 7, filters }],
      ["ai_routing_gateway_stats_home", { days: 30, filters }],
    ]);
  });
});
