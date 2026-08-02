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
      if (command === "ai_routing_gateway_key_create") return Promise.resolve({ key: { id: "key-1" }, plaintext: "osk_one_time_secret" });
      return Promise.resolve([]);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "网关密钥" }));
    await user.type(screen.getByPlaceholderText("密钥名称"), "CLI");
    await user.click(screen.getByRole("button", { name: "创建" }));
    expect(await screen.findByText("osk_one_time_secret")).toBeInTheDocument();
    await user.click(screen.getByTitle("关闭"));
    await waitFor(() => expect(screen.queryByText("osk_one_time_secret")).not.toBeInTheDocument());
  });
});
