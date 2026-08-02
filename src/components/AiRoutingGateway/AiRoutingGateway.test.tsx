import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { AiRoutingGateway } from "@/components/AiRoutingGateway";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

const bootstrap = {
  runtime: { state: "stopped", availability: "ready", port: 17688, run_enabled: true },
  settings: { port: 17688, globalQuotaThresholdPercent: 10, logRetentionDays: 90, runEnabled: true },
  groups: [{ id: "default", name: "Default", sort_order: 0, is_default: true }], accounts: [],
  models: [{ id: "gpt-test", displayName: "GPT Test", enabled: true }], keys: [],
  homepage: { accountCount: 0, availableCount: 0, unavailableCount: 0, staleCount: 0,
    today: { localDate: "2026-08-02", requestCount: 0, successCount: 0, failureCount: 0, usage: { inputTokens: 0, outputTokens: 0, cacheReadTokens: 0, cacheWriteTokens: 0, totalTokens: 0 }, estimatedCostUsd: "0", costCalculable: true },
    trend: Array.from({ length: 7 }, (_, index) => ({ localDate: `2026-07-${String(27 + index).padStart(2, "0")}`, requestCount: 0, successCount: 0, failureCount: 0, usage: { totalTokens: 0 }, estimatedCostUsd: "0", costCalculable: true })) },
  oauthReleaseBlockReason: "official_third_party_codex_oauth_contract_unavailable",
};

const accountFixture = {
  id: "account-1",
  account_type: "api_key" as const,
  name: "Third Party",
  group_id: "default",
  sort_order: 0,
  note: "",
  enabled: true,
  health_status: "healthy",
  tags: ["team"],
};

const richBootstrap = {
  ...bootstrap,
  accounts: [accountFixture],
  keys: [{ id: "key-1", name: "CLI", keyPrefix: "osk", enabled: true, createdAt: "2026-08-02T00:00:00Z", groupIds: ["default"], modelIds: ["gpt-test"] }],
  oauthReleaseBlockReason: null,
};

describe("AiRoutingGateway", () => {
  beforeEach(() => {
    resetTauriMocks();
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve(bootstrap);
      if (command === "ai_routing_gateway_logs_query") return Promise.resolve({ items: [], nextCursor: null });
      if (command === "ai_routing_gateway_prices_list") return Promise.resolve([]);
      return Promise.resolve([]);
    });
  });

  it("呈现五页签与加载后空状态工作区", async () => {
    const user = userEvent.setup();
    renderWithProviders(<AiRoutingGateway />);
    expect(await screen.findByTestId("ai-gateway-tab-home")).toBeInTheDocument();
    for (const label of ["账号池", "网关密钥", "请求日志", "设置"]) await user.click(screen.getByRole("button", { name: label }));
    expect(screen.getByTestId("ai-gateway-tab-settings")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "账号池" }));
    expect(screen.getByText("当前视图没有账号。")).toBeInTheDocument();
    expect(screen.getByText(/OAuth 录入仍处于发布门禁/)).toBeInTheDocument();
  });

  it("展示锁定与端口冲突状态", async () => {
    invokeMock.mockImplementation((command: string) => command === "ai_routing_gateway_bootstrap" ? Promise.resolve({ ...bootstrap, runtime: { state: "locked", availability: "locked", port: 17688, run_enabled: true, lock_reason: "root_key_missing" } }) : Promise.resolve([]));
    const { unmount } = renderWithProviders(<AiRoutingGateway />);
    expect(await screen.findByTestId("ai-gateway-state-locked")).toHaveTextContent("凭据存储已锁定");
    unmount();
    invokeMock.mockImplementation((command: string) => command === "ai_routing_gateway_bootstrap" ? Promise.resolve({ ...bootstrap, runtime: { state: "error", availability: "error", port: 17688, run_enabled: true, error_code: "port_conflict" } }) : Promise.resolve([]));
    renderWithProviders(<AiRoutingGateway />);
    expect(await screen.findByTestId("ai-gateway-state-error")).toHaveTextContent("端口冲突");
  });

  it("创建 Key 后只在一次性提示中展示明文并可关闭", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve(bootstrap);
       if (command === "ai_routing_gateway_key_create") return Promise.resolve({ key: { id: "key-1" }, plaintext: "osk_SAFE_FIXTURE_ONE_TIME_KEY" });
      return Promise.resolve([]);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "网关密钥" }));
    await user.type(screen.getByPlaceholderText("密钥名称"), "CLI");
    await user.click(screen.getByRole("button", { name: "创建" }));
     expect(await screen.findByText("osk_SAFE_FIXTURE_ONE_TIME_KEY")).toBeInTheDocument();
    await user.click(screen.getByTitle("关闭"));
     await waitFor(() => expect(screen.queryByText("osk_SAFE_FIXTURE_ONE_TIME_KEY")).not.toBeInTheDocument());
  });

  it("首页筛选组合会刷新 bootstrap 和 stats DTO，并保留四类 token 明细", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve({ ...richBootstrap, homepage: { ...bootstrap.homepage, today: { ...bootstrap.homepage.today, usage: { inputTokens: 10, outputTokens: 20, cacheReadTokens: 3, cacheWriteTokens: 4, totalTokens: 37 } } } });
      if (command === "ai_routing_gateway_stats_home") return Promise.resolve(richBootstrap.homepage);
      return Promise.resolve([]);
    });
    renderWithProviders(<AiRoutingGateway />);
    expect(await screen.findByTestId("ai-gateway-tab-home")).toBeInTheDocument();
    await user.selectOptions(screen.getByLabelText("账号"), "account-1");
    await user.selectOptions(screen.getByLabelText("分组"), "default");
    await user.selectOptions(screen.getByLabelText("公开模型"), "gpt-test");
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_bootstrap", expect.objectContaining({ filters: { accountId: "account-1", groupId: "default", publicModelId: "gpt-test" } })));
    await user.click(screen.getByRole("button", { name: "30天" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_stats_home", { days: 30, filters: { accountId: "account-1", groupId: "default", publicModelId: "gpt-test" } }));
    expect(screen.getByText(/缓存读取 3/)).toBeInTheDocument();
    expect(screen.getByText(/缓存写入 4/)).toBeInTheDocument();
  });

  it("OAuth 三路径维护 PKCE/manual/device 会话状态并完成或取消共享会话", async () => {
    const user = userEvent.setup();
    let beginCount = 0;
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve(richBootstrap);
      if (command === "ai_routing_gateway_oauth_begin") {
        beginCount += 1;
        return beginCount === 3
          ? Promise.resolve({ sessionId: "device-session", userCode: "DEVICE-123", verificationUrl: "http://127.0.0.1/device", intervalSeconds: 5, expiresInSeconds: 600 })
          : Promise.resolve({ sessionId: `oauth-session-${beginCount}`, authorizationUrl: "http://127.0.0.1/authorize?code_challenge=pkce", callbackUrl: "http://127.0.0.1:18222/oauth/callback" });
      }
      return Promise.resolve([]);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "账号池" }));
    await user.click(screen.getByRole("button", { name: "浏览器 OAuth" }));
    expect(await screen.findByTestId("ai-gateway-oauth-session")).toHaveTextContent("等待回调");
    expect(screen.getByRole("link", { name: "打开授权页面" })).toHaveAttribute("href", expect.stringContaining("code_challenge=pkce"));
    const callback = screen.getByLabelText("粘贴回调地址");
    await user.type(callback, "http://127.0.0.1:18222/oauth/callback?code=test&state=state");
    await user.click(screen.getByRole("button", { name: "完成 OAuth" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_oauth_complete", { sessionId: "oauth-session-1", callbackUrl: "http://127.0.0.1:18222/oauth/callback?code=test&state=state" }));
    await user.click(screen.getByRole("button", { name: "手动回调" }));
    expect(await screen.findByText("等待回调")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "设备代码" }));
    expect(await screen.findByText("DEVICE-123")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "取消 OAuth" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_oauth_cancel", { sessionId: "device-session" }));
    expect(screen.getByText("已取消")).toBeInTheDocument();
  });

  it("账号详情支持模型映射启用和禁用", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve(richBootstrap);
      if (command === "ai_routing_gateway_mapping_list") return Promise.resolve([{ account_id: "account-1", public_model_id: "gpt-test", upstream_model_id: "vendor-model", enabled: true }]);
      if (command === "ai_routing_gateway_quota_list") return Promise.resolve([]);
      return Promise.resolve([]);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "账号池" }));
    await user.click(screen.getByRole("button", { name: /Third Party/ }));
    const toggle = await screen.findByRole("button", { name: "切换 gpt-test 的映射" });
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    await user.click(toggle);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_mapping_save", { input: { accountId: "account-1", publicModelId: "gpt-test", upstreamModelId: "vendor-model", enabled: false } }));
  });

  it("账号详情展示每个额度窗口的范围、剩余量、重置和时长", async () => {
    const user = userEvent.setup();
    const oauthBootstrap = { ...richBootstrap, accounts: [{ ...accountFixture, id: "oauth-1", account_type: "oauth" as const, name: "OAuth Account" }] };
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve(oauthBootstrap);
      if (command === "ai_routing_gateway_quota_list") return Promise.resolve([{ id: "quota-1", account_id: "oauth-1", name: "5 hour", scope_type: "global", scope_value: null, used_percent: 20, remaining_percent: 80, resets_at: "2026-08-02T05:00:00Z", duration_seconds: 18000, is_stale: false, raw_kind: "five_hour" }]);
      if (command === "ai_routing_gateway_mapping_list") return Promise.resolve([]);
      return Promise.resolve([]);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "账号池" }));
    await user.click(screen.getByRole("button", { name: /OAuth Account/ }));
    expect(await screen.findByText(/已使用: 20%/)).toBeInTheDocument();
    expect(screen.getByText(/剩余: 80%/)).toBeInTheDocument();
    expect(screen.getByText(/窗口时长: 18000s/)).toBeInTheDocument();
  });

  it("日志支持组合筛选和可变分页大小", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve(richBootstrap);
      if (command === "ai_routing_gateway_logs_query") return Promise.resolve({ items: [], nextCursor: null });
      return Promise.resolve([]);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "请求日志" }));
    await user.type(screen.getByLabelText("开始日期"), "2026-08-01");
    await user.type(screen.getByLabelText("结束日期"), "2026-08-02");
    await user.selectOptions(screen.getByLabelText("账号"), "account-1");
    await user.selectOptions(screen.getByLabelText("分组"), "default");
    await user.selectOptions(screen.getByLabelText("公开模型"), "gpt-test");
    await user.type(screen.getByLabelText("上游模型"), "vendor-model");
    await user.selectOptions(screen.getByLabelText("状态"), "failed");
    await user.type(screen.getByLabelText("错误码"), "upstream_unavailable");
    await user.selectOptions(screen.getByLabelText("Key"), "key-1");
    await user.selectOptions(screen.getByLabelText("每页条数"), "50");
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_logs_query", expect.objectContaining({ input: expect.objectContaining({ accountId: "account-1", groupId: "default", publicModelId: "gpt-test", upstreamModelId: "vendor-model", status: "failed", errorCode: "upstream_unavailable", apiKeyId: "key-1", pageSize: 50 }) })));
  });

  it("设置支持官方价格与第三方 API-key 四项价格覆盖", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve(richBootstrap);
      if (command === "ai_routing_gateway_prices_list") return Promise.resolve([]);
      return Promise.resolve([]);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "设置" }));
    await user.selectOptions(screen.getByLabelText("价格账号"), "account-1");
    await user.type(screen.getByLabelText("输入价格"), "1");
    await user.type(screen.getByLabelText("输出价格"), "2");
    await user.type(screen.getByLabelText("缓存读取价格"), "0.1");
    await user.type(screen.getByLabelText("缓存写入价格"), "0.2");
    await user.click(screen.getByRole("button", { name: "添加价格快照" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_price_save", { input: expect.objectContaining({ publicModelId: "gpt-test", accountId: "account-1", inputPerMillionUsd: "1", outputPerMillionUsd: "2", cacheReadPerMillionUsd: "0.1", cacheWritePerMillionUsd: "0.2" }) }));
  });
});
