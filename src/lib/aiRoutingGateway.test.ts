import { describe, expect, it, vi } from "vitest";
import {
  aiRoutingGatewayAccountCreateApiKey,
  aiRoutingGatewayBootstrap,
  aiRoutingGatewayKeyCreate,
  aiRoutingGatewayLogsQuery,
  aiRoutingGatewaySettingsSave,
  subscribeAiRoutingGatewayEvents,
} from "@/lib/aiRoutingGateway";
import { invokeMock, listenMock, resetTauriMocks } from "@/test/mocks/tauri";

describe("AI routing gateway typed IPC facade", () => {
  it("集中使用独立命令前缀和 camelCase 输入 DTO", async () => {
    resetTauriMocks();
    invokeMock.mockResolvedValue({});
    await aiRoutingGatewayBootstrap(15);
    await aiRoutingGatewayAccountCreateApiKey({ name: "Local", baseUrl: "http://127.0.0.1:18000/v1", apiKey: "secret", authMethod: "bearer", upstreamProtocol: "responses", note: "fixture" });
    await aiRoutingGatewayKeyCreate({ name: "CLI", groupIds: ["default"], modelIds: ["gpt-test"], expiresAt: null });
    await aiRoutingGatewayLogsQuery({ status: "failed", pageSize: 25 });
    await aiRoutingGatewaySettingsSave({ port: 17688, globalQuotaThresholdPercent: 10, logRetentionDays: 90, runEnabled: true });

    expect(invokeMock.mock.calls).toEqual([
      ["ai_routing_gateway_bootstrap", { days: 15 }],
      ["ai_routing_gateway_account_create_api_key", { input: { name: "Local", baseUrl: "http://127.0.0.1:18000/v1", apiKey: "secret", authMethod: "bearer", upstreamProtocol: "responses", note: "fixture" } }],
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
});
