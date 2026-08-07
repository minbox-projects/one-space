import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
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
  base_url: "https://api.example.com/v1",
  auth_method: "bearer",
  upstream_protocol: "responses" as const,
  model_mappings: [{ account_id: "account-1", public_model_id: "gpt-test", upstream_model_id: "gpt-test", enabled: true }],
};

const richBootstrap = {
  ...bootstrap,
  accounts: [accountFixture],
  keys: [{ id: "key-1", name: "CLI", maskedKey: "osk_12******345678", enabled: true, createdAt: "2026-08-02T00:00:00Z", groupIds: ["default"], modelIds: ["gpt-test"], today: { requestCount: 2, totalTokens: 30, estimatedCostUsd: "0.1" }, last30Days: { requestCount: 5, totalTokens: 80, estimatedCostUsd: "0.3" } }],
  oauthReleaseBlockReason: null,
};

const groupedBootstrap = {
  ...bootstrap,
  groups: [
    { id: "team", name: "Team", sort_order: 1, is_default: false },
    { id: "default", name: "Default", sort_order: 0, is_default: true },
  ],
  accounts: [
    { ...accountFixture, name: "Default Visible", note: "primary note", tags: ["priority"], quota_threshold_override_percent: 75 },
    { ...accountFixture, id: "account-hidden", name: "Default Hidden", sort_order: 1, enabled: false, tags: ["secondary"], model_mappings: [] },
    { ...accountFixture, id: "account-team", name: "Team Account", group_id: "team", base_url: "https://team.example.com/a/very/long/api/path", tags: ["team"], model_mappings: [] },
  ],
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
    expect(screen.queryByRole("button", { name: /OAuth/ })).not.toBeInTheDocument();
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

  it("已有 OAuth 账号仅展示标签且没有新增或重新登录入口", async () => {
    const user = userEvent.setup();
    const oauthBootstrap = { ...richBootstrap, accounts: [{ ...accountFixture, id: "oauth-1", account_type: "oauth" as const, name: "OAuth Account" }] };
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve(oauthBootstrap);
      return Promise.resolve([]);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "账号池" }));
    expect(screen.getByRole("button", { name: /OAuth Account OAuth/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /浏览器 OAuth|手动回调|设备代码|重新登录/ })).not.toBeInTheDocument();
  });

  it("API Key 账号详情保存连接字段且空密钥表示保留", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve(richBootstrap);
      if (command === "ai_routing_gateway_mapping_list") return Promise.resolve(accountFixture.model_mappings);
      if (command === "ai_routing_gateway_quota_list" || command === "ai_routing_gateway_prices_list") return Promise.resolve([]);
      return Promise.resolve(accountFixture);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "账号池" }));
    await user.click(screen.getByRole("button", { name: /Third Party/ }));
    await user.clear(screen.getByLabelText("API 地址"));
    await user.type(screen.getByLabelText("API 地址"), "https://new.example.com/v1");
    expect(screen.getByLabelText("分组")).toBeInTheDocument();
    expect(screen.getByLabelText("标签")).toBeInTheDocument();
    expect(screen.getByLabelText("额度阈值（%）")).toBeInTheDocument();
    await user.selectOptions(screen.getByLabelText("认证方式"), "api_key_header");
    await user.selectOptions(screen.getByLabelText("上游协议"), "chat_completions");
    await user.click(screen.getAllByRole("button", { name: "保存" })[0]);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_account_update", { input: expect.objectContaining({ baseUrl: "https://new.example.com/v1", apiKey: null, authMethod: "api_key_header", upstreamProtocol: "chat_completions" }) }));
  });

  it("账号卡片进入独立详情，返回后刷新列表", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve(richBootstrap);
      if (command === "ai_routing_gateway_mapping_list") return Promise.resolve(accountFixture.model_mappings);
      return Promise.resolve([]);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "账号池" }));
    expect(screen.getByText("https://api.example.com/v1")).toBeInTheDocument();
    expect(screen.getByText("gpt-test → gpt-test")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Third Party/ }));
    expect(screen.getByTestId("account-edit-detail")).toBeInTheDocument();
    expect(screen.queryByPlaceholderText("新分组名称")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "返回" }));
    expect(await screen.findByRole("tab", { name: "Default" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByText("Third Party")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "管理分组" })).toBeInTheDocument();
    expect(screen.queryByPlaceholderText("新分组名称")).not.toBeInTheDocument();
    expect(invokeMock.mock.calls.filter(([command]) => command === "ai_routing_gateway_bootstrap").length).toBeGreaterThanOrEqual(2);
  });

  it("默认组固定首位，组内搜索与切组不会泄露其他组账号", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation((command: string) => command === "ai_routing_gateway_bootstrap" ? Promise.resolve(groupedBootstrap) : Promise.resolve([]));
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "账号池" }));

    const tabs = screen.getAllByRole("tab");
    expect(tabs.map((tab) => tab.textContent)).toEqual(["Default", "Team"]);
    expect(screen.queryByRole("tab", { name: /全部账号/i })).not.toBeInTheDocument();
    expect(screen.getByText("Default Visible")).toBeInTheDocument();
    expect(screen.getByText("Default Hidden")).toBeInTheDocument();
    expect(screen.queryByText("Team Account")).not.toBeInTheDocument();
    expect(screen.getByText("primary note")).toBeInTheDocument();
    expect(screen.getByText("priority")).toBeInTheDocument();

    await user.type(screen.getByRole("textbox", { name: "搜索账号" }), "visible");
    expect(screen.getByText("Default Visible")).toBeInTheDocument();
    expect(screen.queryByText("Default Hidden")).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Team" }));
    expect(screen.queryByText("Default Visible")).not.toBeInTheDocument();
    expect(screen.getByText("没有符合当前搜索条件的账号。")).toBeInTheDocument();
  });

  it("切组与刷新后当前组失效时回退默认组并清除选择", async () => {
    const user = userEvent.setup();
    let current = groupedBootstrap;
    invokeMock.mockImplementation((command: string) => command === "ai_routing_gateway_bootstrap" ? Promise.resolve(current) : Promise.resolve([]));
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "账号池" }));
    await user.click(screen.getByRole("checkbox", { name: "选择 Default Visible" }));
    expect(screen.getByTestId("ai-gateway-tab-accounts")).toHaveAttribute("data-selected-count", "1");
    await user.click(screen.getByRole("tab", { name: "Team" }));
    await waitFor(() => expect(screen.getByTestId("ai-gateway-tab-accounts")).toHaveAttribute("data-selected-count", "0"));
    await user.click(screen.getByRole("checkbox", { name: "选择 Team Account" }));

    current = { ...groupedBootstrap, groups: [groupedBootstrap.groups[1]], accounts: groupedBootstrap.accounts.slice(0, 2) };
    await user.click(screen.getByTitle("刷新"));
    await waitFor(() => expect(screen.queryByRole("tab", { name: "Team" })).not.toBeInTheDocument());
    expect(screen.getByRole("tab", { name: "Default" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByTestId("ai-gateway-tab-accounts")).toHaveAttribute("data-selected-count", "0");
  });

  it("删除当前组后展示迁移账号、回退默认组并清除选择", async () => {
    const user = userEvent.setup();
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    let current = groupedBootstrap;
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve(current);
      if (command === "ai_routing_gateway_group_delete") {
        current = {
          ...groupedBootstrap,
          groups: [groupedBootstrap.groups[1]],
          accounts: groupedBootstrap.accounts.map((account) => account.id === "account-team" ? { ...account, group_id: "default" } : account),
        };
        return Promise.resolve(undefined);
      }
      return Promise.resolve([]);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "账号池" }));
    await user.click(screen.getByRole("tab", { name: "Team" }));
    await user.click(screen.getByRole("checkbox", { name: "选择 Team Account" }));
    expect(screen.getByTestId("ai-gateway-tab-accounts")).toHaveAttribute("data-selected-count", "1");

    await user.click(screen.getByRole("button", { name: "管理分组" }));
    const dialog = screen.getByRole("dialog");
    await user.click(within(dialog).getByTitle("删除分组"));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_group_delete", { groupId: "team" }));
    await user.click(within(dialog).getByRole("button", { name: "关闭" }));
    expect(await screen.findByText("Team Account")).toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "Team" })).not.toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Default" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByTestId("ai-gateway-tab-accounts")).toHaveAttribute("data-selected-count", "0");
    confirm.mockRestore();
  });

  it("全选与批量禁用仅提交当前搜索可见账号，成功后清空选择", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve(groupedBootstrap);
      if (command === "ai_routing_gateway_accounts_disable") return Promise.resolve([]);
      return Promise.resolve([]);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "账号池" }));
    expect(screen.queryByRole("button", { name: "批量禁用" })).not.toBeInTheDocument();
    await user.type(screen.getByRole("textbox", { name: "搜索账号" }), "visible");
    await user.click(screen.getByRole("checkbox", { name: "全选当前可见账号" }));
    await user.click(screen.getByRole("button", { name: "批量禁用" }));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_accounts_disable", { input: { accountIds: ["account-1"] } }));
    await waitFor(() => expect(screen.getByTestId("ai-gateway-tab-accounts")).toHaveAttribute("data-selected-count", "0"));
    expect(screen.queryByRole("button", { name: "批量禁用" })).not.toBeInTheDocument();
  });

  it("批量失败保留选择，删除取消不请求确认令牌", async () => {
    const user = userEvent.setup();
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve(groupedBootstrap);
      if (command === "ai_routing_gateway_accounts_disable") return Promise.reject(new Error("storage_unavailable"));
      return Promise.resolve([]);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "账号池" }));
    await user.click(screen.getByRole("checkbox", { name: "选择 Default Visible" }));
    await user.click(screen.getByRole("button", { name: "批量禁用" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("storage_unavailable");
    expect(screen.getByRole("checkbox", { name: "选择 Default Visible" })).toBeChecked();

    await user.click(screen.getByRole("button", { name: "批量删除" }));
    expect(confirm).toHaveBeenCalledWith(expect.stringContaining("1 个账号"));
    expect(invokeMock.mock.calls.some(([command]) => command === "ai_routing_gateway_accounts_delete_confirmation" || command === "ai_routing_gateway_accounts_delete")).toBe(false);
    confirm.mockRestore();
  });

  it("批量删除绑定当前可见集合和确认令牌并在成功后清空选择", async () => {
    const user = userEvent.setup();
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve(groupedBootstrap);
      if (command === "ai_routing_gateway_accounts_delete_confirmation") return Promise.resolve("set-bound-token");
      return Promise.resolve([]);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "账号池" }));
    await user.type(screen.getByRole("textbox", { name: "搜索账号" }), "hidden");
    await user.click(screen.getByRole("checkbox", { name: "全选当前可见账号" }));
    await user.click(screen.getByRole("button", { name: "批量删除" }));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_accounts_delete_confirmation", { input: { accountIds: ["account-hidden"] } }));
    expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_accounts_delete", { input: { accountIds: ["account-hidden"], confirmationToken: "set-bound-token" } });
    await waitFor(() => expect(screen.getByTestId("ai-gateway-tab-accounts")).toHaveAttribute("data-selected-count", "0"));
    confirm.mockRestore();
  });

  it("分组管理覆盖新建、重命名、删除与默认组保护", async () => {
    const user = userEvent.setup();
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    invokeMock.mockImplementation((command: string) => command === "ai_routing_gateway_bootstrap" ? Promise.resolve(groupedBootstrap) : Promise.resolve({}));
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "账号池" }));
    await user.click(screen.getByRole("button", { name: "管理分组" }));
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getAllByText("Default")).toHaveLength(1);
    expect(within(dialog).getAllByTitle("重命名分组")).toHaveLength(1);
    expect(within(dialog).getAllByTitle("删除分组")).toHaveLength(1);

    await user.type(within(dialog).getByPlaceholderText("分组名称"), "Platform");
    await user.click(within(dialog).getByRole("button", { name: "创建分组" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_group_create", { input: { name: "Platform", sortOrder: 2 } }));

    await user.click(within(dialog).getByTitle("重命名分组"));
    const renameInput = within(dialog).getByDisplayValue("Team");
    await user.clear(renameInput);
    await user.type(renameInput, "Renamed Team");
    await user.click(within(dialog).getByRole("button", { name: "保存" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_group_rename", { input: { groupId: "team", name: "Renamed Team" } }));

    await user.click(within(dialog).getByTitle("删除分组"));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_group_delete", { groupId: "team" }));
    expect(confirm).toHaveBeenCalledWith(expect.stringContaining("Team"));
    confirm.mockRestore();
  });

  it("分组命令失败时保留输入并展示后端错误", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve(groupedBootstrap);
      if (command === "ai_routing_gateway_group_create") return Promise.reject(new Error("conflict:Platform"));
      return Promise.resolve([]);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "账号池" }));
    await user.click(screen.getByRole("button", { name: "管理分组" }));
    const dialog = screen.getByRole("dialog");
    const input = within(dialog).getByPlaceholderText("分组名称");
    await user.type(input, "Platform");
    await user.click(within(dialog).getByRole("button", { name: "创建分组" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("conflict:Platform");
    expect(input).toHaveValue("Platform");
  });

  it("重命名和删除分组失败时保留输入与分组状态", async () => {
    const user = userEvent.setup();
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve(groupedBootstrap);
      if (command === "ai_routing_gateway_group_rename") return Promise.reject(new Error("rename_failed"));
      if (command === "ai_routing_gateway_group_delete") return Promise.reject(new Error("delete_failed"));
      return Promise.resolve([]);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "账号池" }));
    await user.click(screen.getByRole("button", { name: "管理分组" }));
    const dialog = screen.getByRole("dialog");

    await user.click(within(dialog).getByTitle("重命名分组"));
    const renameInput = within(dialog).getByDisplayValue("Team");
    await user.clear(renameInput);
    await user.type(renameInput, "Failed Rename");
    await user.click(within(dialog).getByRole("button", { name: "保存" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("rename_failed");
    expect(within(dialog).getByDisplayValue("Failed Rename")).toBeInTheDocument();

    await user.click(within(dialog).getByRole("button", { name: "取消" }));
    await user.click(within(dialog).getByTitle("删除分组"));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("delete_failed");
    expect(within(dialog).getByText("Team")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "关闭" }));
    expect(screen.getByRole("tab", { name: "Team" })).toBeInTheDocument();
    confirm.mockRestore();
  });

  it("纵向列表保留移动、启停和单账号确认删除", async () => {
    const user = userEvent.setup();
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve(groupedBootstrap);
      if (command === "ai_routing_gateway_account_delete_confirmation") return Promise.resolve("single-token");
      return Promise.resolve(accountFixture);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "账号池" }));
    await user.click(screen.getAllByTitle("下移")[0]);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_account_move", { accountId: "account-1", direction: 1 }));
    await user.click(screen.getAllByRole("button", { name: "已启用" })[0]);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_account_update", { input: expect.objectContaining({ accountId: "account-1", enabled: false }) }));
    await user.click(screen.getAllByTitle("永久删除")[0]);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_account_delete_confirmation", { accountId: "account-1" }));
    expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_account_delete", { accountId: "account-1", confirmationToken: "single-token" });
    confirm.mockRestore();
  });

  it("新增详情初始化全部模型并只提交一次原子配置", async () => {
    const user = userEvent.setup();
    const twoModelBootstrap = { ...groupedBootstrap, accounts: [], models: [...bootstrap.models, { id: "gpt-second", displayName: "GPT Second", enabled: true }] };
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve(twoModelBootstrap);
      if (command === "ai_routing_gateway_account_create_api_key_with_configuration") return Promise.resolve(accountFixture);
      return Promise.resolve([]);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "账号池" }));
    const originalUrl = window.location.href;
    await user.click(screen.getByRole("button", { name: "添加第三方账号" }));
    expect(screen.getByTestId("account-create-detail")).toBeInTheDocument();
    expect(window.location.href).toBe(originalUrl);
    expect(screen.getByText("账号所属分组")).toBeInTheDocument();
    expect(screen.getByText("自定义标签")).toBeInTheDocument();
    expect(screen.getByText("账号额度阈值（%）")).toBeInTheDocument();
    expect(screen.getByLabelText("GPT Test 上游模型")).toHaveValue("gpt-test");
    expect(screen.getByLabelText("GPT Second 上游模型")).toHaveValue("gpt-second");
    expect(screen.getByLabelText("切换 GPT Test 的映射")).toBeChecked();
    expect(screen.getByLabelText("GPT Test 输入价格")).toHaveValue("");

    await user.type(screen.getByLabelText("账号名称"), "Atomic Account");
    await user.type(screen.getByLabelText("API 地址"), "https://atomic.example.com/v1");
    await user.type(screen.getByLabelText("第三方 API Key"), "SAFE_ATOMIC_KEY");
    await user.selectOptions(screen.getByText("账号所属分组").nextElementSibling as HTMLSelectElement, "team");
    await user.type(screen.getByPlaceholderText("多个标签使用逗号分隔"), "priority, team");
    await user.type(screen.getByPlaceholderText("继承全局阈值"), "75");
    await user.type(screen.getByText("备注").nextElementSibling as HTMLTextAreaElement, "atomic note");
    await user.clear(screen.getByLabelText("GPT Test 上游模型"));
    await user.type(screen.getByLabelText("GPT Test 上游模型"), "vendor-model");
    await user.click(screen.getByLabelText("切换 GPT Test 的映射"));
    await user.type(screen.getByLabelText("GPT Test 输入价格"), "1");
    await user.type(screen.getByLabelText("GPT Test 输出价格"), "2");
    await user.type(screen.getByLabelText("GPT Test 缓存读取价格"), "0.1");
    await user.type(screen.getByLabelText("GPT Test 缓存写入价格"), "0.2");
    await user.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_account_create_api_key_with_configuration", { input: {
      name: "Atomic Account", baseUrl: "https://atomic.example.com/v1", apiKey: "SAFE_ATOMIC_KEY", authMethod: "bearer", upstreamProtocol: "responses", groupId: "team", tags: ["priority", "team"], quotaThresholdOverridePercent: 75, note: "atomic note",
      mappings: [
        { publicModelId: "gpt-test", upstreamModelId: "vendor-model", enabled: false },
        { publicModelId: "gpt-second", upstreamModelId: "gpt-second", enabled: true },
      ],
      prices: [
        { publicModelId: "gpt-test", inputPerMillionUsd: "1", outputPerMillionUsd: "2", cacheReadPerMillionUsd: "0.1", cacheWritePerMillionUsd: "0.2" },
        { publicModelId: "gpt-second", inputPerMillionUsd: null, outputPerMillionUsd: null, cacheReadPerMillionUsd: null, cacheWritePerMillionUsd: null },
      ],
    } }));
    expect(invokeMock.mock.calls.filter(([command]) => command === "ai_routing_gateway_account_create_api_key_with_configuration")).toHaveLength(1);
    for (const command of ["ai_routing_gateway_account_create_api_key", "ai_routing_gateway_mapping_save", "ai_routing_gateway_price_save"]) {
      expect(invokeMock.mock.calls.some(([called]) => called === command)).toBe(false);
    }
    expect(await screen.findByText("当前视图没有账号。")).toBeInTheDocument();
  });

  it("创建详情按 is_default 初始化并把默认组保存为隐式默认组", async () => {
    const user = userEvent.setup();
    const creationBootstrap = { ...groupedBootstrap, accounts: [] };
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve(creationBootstrap);
      if (command === "ai_routing_gateway_account_create_api_key_with_configuration") return Promise.resolve(accountFixture);
      return Promise.resolve([]);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "账号池" }));
    await user.click(screen.getByRole("button", { name: "添加第三方账号" }));

    const groupSelect = screen.getByLabelText("账号所属分组") as HTMLSelectElement;
    expect([...groupSelect.options].map((option) => option.textContent)).toEqual(["Default", "Team"]);
    expect(groupSelect).toHaveValue("default");
    await user.type(screen.getByLabelText("账号名称"), "Default Account");
    await user.type(screen.getByLabelText("API 地址"), "https://default.example.com/v1");
    await user.type(screen.getByLabelText("第三方 API Key"), "SAFE_DEFAULT_KEY");
    await user.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_account_create_api_key_with_configuration", { input: expect.objectContaining({ name: "Default Account", baseUrl: "https://default.example.com/v1", apiKey: "SAFE_DEFAULT_KEY" }) }));
    const createCall = invokeMock.mock.calls.find(([command]) => command === "ai_routing_gateway_account_create_api_key_with_configuration") as [string, { input: Record<string, unknown> }] | undefined;
    expect(createCall?.[1].input.groupId).toBeUndefined();
  });

  it("新增取消不写入并保持模块 URL", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation((command: string) => command === "ai_routing_gateway_bootstrap" ? Promise.resolve(groupedBootstrap) : Promise.resolve([]));
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "账号池" }));
    const originalUrl = window.location.href;
    await user.click(screen.getByRole("button", { name: "添加第三方账号" }));
    await user.type(screen.getByLabelText("账号名称"), "Draft Only");
    await user.click(screen.getByRole("button", { name: "取消" }));
    expect(window.location.href).toBe(originalUrl);
    expect(await screen.findByText("Default Visible")).toBeInTheDocument();
    expect(invokeMock.mock.calls.some(([command]) => command === "ai_routing_gateway_account_create_api_key_with_configuration")).toBe(false);
  });

  it("原子新增失败后保留连接、密钥、映射和价格", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve(bootstrap);
      if (command === "ai_routing_gateway_account_create_api_key_with_configuration") return Promise.reject(new Error("invalid_input"));
      return Promise.resolve([]);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "账号池" }));
    await user.click(screen.getByRole("button", { name: "添加第三方账号" }));
    await user.type(screen.getByLabelText("账号名称"), "Retained");
    await user.type(screen.getByLabelText("API 地址"), "https://retained.example.com/v1");
    await user.type(screen.getByLabelText("第三方 API Key"), "SAFE_RETAINED_KEY");
    await user.type(screen.getByPlaceholderText("多个标签使用逗号分隔"), "retained, draft");
    await user.type(screen.getByPlaceholderText("继承全局阈值"), "60");
    await user.type(screen.getByText("备注").nextElementSibling as HTMLTextAreaElement, "retained note");
    await user.clear(screen.getByLabelText("GPT Test 上游模型"));
    await user.type(screen.getByLabelText("GPT Test 上游模型"), "retained-model");
    await user.type(screen.getByLabelText("GPT Test 输入价格"), "bad");
    await user.click(screen.getByRole("button", { name: "保存" }));
    expect(await screen.findByText("invalid_input")).toBeInTheDocument();
    expect(screen.getByLabelText("账号名称")).toHaveValue("Retained");
    expect(screen.getByLabelText("第三方 API Key")).toHaveValue("SAFE_RETAINED_KEY");
    expect(screen.getByPlaceholderText("多个标签使用逗号分隔")).toHaveValue("retained, draft");
    expect(screen.getByPlaceholderText("继承全局阈值")).toHaveValue(60);
    expect(screen.getByText("备注").nextElementSibling).toHaveValue("retained note");
    expect(screen.getByLabelText("GPT Test 上游模型")).toHaveValue("retained-model");
    expect(screen.getByLabelText("GPT Test 输入价格")).toHaveValue("bad");
  });

  it("OAuth 详情显示连接、映射和价格且没有写控件", async () => {
    const user = userEvent.setup();
    const oauth = { ...accountFixture, id: "oauth-1", account_type: "oauth" as const, name: "OAuth Account", model_mappings: [{ account_id: "oauth-1", public_model_id: "gpt-test", upstream_model_id: "oauth-model", enabled: true }] };
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve({ ...richBootstrap, accounts: [oauth] });
      if (command === "ai_routing_gateway_mapping_list") return Promise.resolve(oauth.model_mappings);
      if (command === "ai_routing_gateway_prices_list") return Promise.resolve([{ public_model_id: "gpt-test", account_id: "oauth-1", source: "account_override", effective_at: "2026-08-06T00:00:00Z", input_per_million_usd: "1" }]);
      return Promise.resolve([]);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "账号池" }));
    await user.click(screen.getByRole("button", { name: /OAuth Account/ }));
    expect(screen.getByText("https://api.example.com/v1")).toBeInTheDocument();
    expect(await screen.findByText("gpt-test → oauth-model")).toBeInTheDocument();
    expect(screen.getByText("1")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "切换 gpt-test 的映射" })).not.toBeInTheDocument();
    expect(screen.queryByLabelText("输入价格")).not.toBeInTheDocument();
    expect(screen.queryByPlaceholderText("上游模型")).not.toBeInTheDocument();
    expect(invokeMock.mock.calls.some(([command]) => command === "ai_routing_gateway_mapping_save" || command === "ai_routing_gateway_price_save")).toBe(false);
  });

  it("密钥列表只展示脱敏值并通过后端复制完整值和即时保存分组", async () => {
    const user = userEvent.setup();
    const twoGroups = { ...richBootstrap, groups: [...richBootstrap.groups, { id: "team", name: "Team", sort_order: 1, is_default: false }] };
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve(twoGroups);
      if (command === "ai_routing_gateway_key_copy") return Promise.resolve("osk_FULL_USABLE_SECRET_345678");
      if (command === "ai_routing_gateway_key_groups_update") return Promise.resolve(["default", "team"]);
      return Promise.resolve([]);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "网关密钥" }));
    expect(screen.getByText(/osk_12\*\*\*\*\*\*345678/)).toBeInTheDocument();
    expect(screen.queryByText("osk_FULL_USABLE_SECRET_345678")).not.toBeInTheDocument();
    await user.click(screen.getByTitle("复制"));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_key_copy", { keyId: "key-1" }));
    await user.click(screen.getAllByRole("checkbox", { name: "Team" })[1]);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_key_groups_update", { input: { keyId: "key-1", groupIds: ["default", "team"] } }));
  });

  it("分组即时保存失败时不把未持久化选择显示为已生效", async () => {
    const user = userEvent.setup();
    const twoGroups = { ...richBootstrap, groups: [...richBootstrap.groups, { id: "team", name: "Team", sort_order: 1, is_default: false }] };
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve(twoGroups);
      if (command === "ai_routing_gateway_key_groups_update") return Promise.reject(new Error("storage_unavailable"));
      return Promise.resolve([]);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "网关密钥" }));
    const persistedCheckbox = screen.getAllByRole("checkbox", { name: "Team" })[1];
    expect(persistedCheckbox).not.toBeChecked();
    await user.click(persistedCheckbox);
    expect(await screen.findByText("storage_unavailable")).toBeInTheDocument();
    expect(persistedCheckbox).not.toBeChecked();
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

  it("API Key 编辑详情继续使用旧价格保存接口", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation((command: string) => {
      if (command === "ai_routing_gateway_bootstrap") return Promise.resolve(richBootstrap);
      if (command === "ai_routing_gateway_mapping_list") return Promise.resolve(accountFixture.model_mappings);
      return Promise.resolve([]);
    });
    renderWithProviders(<AiRoutingGateway />);
    await user.click(await screen.findByRole("button", { name: "账号池" }));
    await user.click(screen.getByRole("button", { name: /Third Party/ }));
    await user.type(screen.getByLabelText("输入价格"), "1.5");
    await user.type(screen.getByLabelText("输出价格"), "6");
    await user.type(screen.getByLabelText("缓存读取价格"), "0.2");
    await user.type(screen.getByLabelText("缓存写入价格"), "0.3");
    await user.click(screen.getAllByRole("button", { name: "保存" })[1]);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("ai_routing_gateway_price_save", { input: expect.objectContaining({
      publicModelId: "gpt-test", accountId: "account-1", inputPerMillionUsd: "1.5", outputPerMillionUsd: "6", cacheReadPerMillionUsd: "0.2", cacheWritePerMillionUsd: "0.3",
    }) }));
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
