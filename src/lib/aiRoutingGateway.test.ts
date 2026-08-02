import { describe, expect, it, vi } from "vitest";
import {
  aiRoutingGatewayAccountCreateApiKey,
  aiRoutingGatewayBootstrap,
  aiRoutingGatewayKeyCreate,
  aiRoutingGatewayLogsQuery,
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
    await aiRoutingGatewayLogsQuery({ status: "failed", pageSize: 25 });
    await aiRoutingGatewaySettingsSave({ port: 17688, globalQuotaThresholdPercent: 10, logRetentionDays: 90, runEnabled: true });

    expect(invokeMock.mock.calls).toEqual([
      ["ai_routing_gateway_bootstrap", { days: 15 }],
      ["ai_routing_gateway_account_create_api_key", { input: { name: "Local", baseUrl: "http://127.0.0.1:18000/v1", apiKey: "SAFE_FIXTURE_API_KEY", authMethod: "bearer", upstreamProtocol: "responses", note: "SAFE_FIXTURE_NOTE" } }],
      ["ai_routing_gateway_key_create", { input: { name: "CLI", groupIds: ["default"], modelIds: ["gpt-test"], expiresAt: null } }],
      ["ai_routing_gateway_logs_query", { input: { status: "failed", pageSize: 25 } }],
      ["ai_routing_gateway_settings_save", { input: { port: 17688, globalQuotaThresholdPercent: 10, logRetentionDays: 90, runEnabled: true } }],
    ]);
    expect((invokeMock.mock.calls as unknown[][]).every(([command]) => String(command).startsWith("ai_routing_gateway_") && !String(command).startsWith("protocol_router_"))).toBe(true);
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
