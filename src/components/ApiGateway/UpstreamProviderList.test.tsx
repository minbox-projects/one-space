import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { UpstreamProviderList } from "./UpstreamProviderList";
import { type GatewayUpstreamProvider } from "@/lib/apiGateway";
import { renderWithProviders } from "@/test/mocks/render";

function makeProvider(
  overrides: Partial<GatewayUpstreamProvider> = {},
): GatewayUpstreamProvider {
  return {
    id: "p1",
    name: "Provider 1",
    base_url: "https://api.example.com",
    api_key: "sk-test",
    default_model: "gpt-4o",
    protocol: "chat_completions",
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

describe("UpstreamProviderList 状态展示与过滤", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("已启用与已禁用服务商卡片分别使用绿色与红色徽章展示状态", () => {
    const providers: GatewayUpstreamProvider[] = [
      makeProvider({ id: "p-enabled", name: "Enabled Provider", enabled: true }),
      makeProvider({ id: "p-disabled", name: "Disabled Provider", enabled: false }),
      makeProvider({
        id: "p-auto",
        name: "Auto Disabled Provider",
        enabled: true,
        auto_disabled: true,
        disabled_reason: "rate limited",
      }),
    ];

    renderWithProviders(
      <UpstreamProviderList
        providers={providers}
        selectedProviderId={null}
        busy={false}
        onSelect={vi.fn()}
        onToggleEnabled={vi.fn()}
        onReenable={vi.fn()}
        onAdd={vi.fn()}
      />,
    );

    // 已启用状态徽章包含绿色类名
    const enabledBadge = screen.getByTestId("api-gateway-status-badge-p-enabled");
    expect(enabledBadge).toHaveTextContent("Enabled");
    expect(enabledBadge.className).toContain("bg-emerald-500/10");
    expect(enabledBadge.className).toContain("text-emerald-700");

    // 已禁用状态徽章包含红色类名
    const disabledBadge = screen.getByTestId("api-gateway-status-badge-p-disabled");
    expect(disabledBadge).toHaveTextContent("Disabled");
    expect(disabledBadge.className).toContain("bg-rose-500/10");
    expect(disabledBadge.className).toContain("text-rose-700");

    // 自动熔断服务商包含琥珀色徽章
    const autoBadge = screen.getByTestId("api-gateway-status-badge-p-auto");
    expect(autoBadge).toHaveTextContent("Auto-disabled");
    expect(autoBadge.className).toContain("bg-amber-500/10");
  });

  it("分段状态过滤器显示全部、已启用和已禁用选项及准确计数", () => {
    const providers: GatewayUpstreamProvider[] = [
      makeProvider({ id: "p1", name: "Provider 1", enabled: true }),
      makeProvider({ id: "p2", name: "Provider 2", enabled: true }),
      makeProvider({ id: "p3", name: "Provider 3", enabled: false }),
    ];

    renderWithProviders(
      <UpstreamProviderList
        providers={providers}
        selectedProviderId={null}
        busy={false}
        onSelect={vi.fn()}
        onToggleEnabled={vi.fn()}
        onReenable={vi.fn()}
        onAdd={vi.fn()}
      />,
    );

    const allBtn = screen.getByTestId("filter-status-all");
    const enabledBtn = screen.getByTestId("filter-status-enabled");
    const disabledBtn = screen.getByTestId("filter-status-disabled");

    expect(allBtn).toHaveTextContent("All");
    expect(allBtn).toHaveTextContent("3");

    expect(enabledBtn).toHaveTextContent("Enabled");
    expect(enabledBtn).toHaveTextContent("2");

    expect(disabledBtn).toHaveTextContent("Disabled");
    expect(disabledBtn).toHaveTextContent("1");
  });

  it("切换到已启用过滤条件时只显示已启用的服务商", async () => {
    const user = userEvent.setup();
    const providers: GatewayUpstreamProvider[] = [
      makeProvider({ id: "p1", name: "Active Service", enabled: true }),
      makeProvider({ id: "p2", name: "Inactive Service", enabled: false }),
    ];

    renderWithProviders(
      <UpstreamProviderList
        providers={providers}
        selectedProviderId={null}
        busy={false}
        onSelect={vi.fn()}
        onToggleEnabled={vi.fn()}
        onReenable={vi.fn()}
        onAdd={vi.fn()}
      />,
    );

    expect(screen.getByText("Active Service")).toBeInTheDocument();
    expect(screen.getByText("Inactive Service")).toBeInTheDocument();

    // 点击已启用
    await user.click(screen.getByTestId("filter-status-enabled"));

    expect(screen.getByText("Active Service")).toBeInTheDocument();
    expect(screen.queryByText("Inactive Service")).not.toBeInTheDocument();
  });

  it("切换到已禁用过滤条件时只显示已禁用的服务商", async () => {
    const user = userEvent.setup();
    const providers: GatewayUpstreamProvider[] = [
      makeProvider({ id: "p1", name: "Active Service", enabled: true }),
      makeProvider({ id: "p2", name: "Inactive Service", enabled: false }),
    ];

    renderWithProviders(
      <UpstreamProviderList
        providers={providers}
        selectedProviderId={null}
        busy={false}
        onSelect={vi.fn()}
        onToggleEnabled={vi.fn()}
        onReenable={vi.fn()}
        onAdd={vi.fn()}
      />,
    );

    // 点击已禁用
    await user.click(screen.getByTestId("filter-status-disabled"));

    expect(screen.queryByText("Active Service")).not.toBeInTheDocument();
    expect(screen.getByText("Inactive Service")).toBeInTheDocument();
  });

  it("过滤后无匹配项时显示微空态并通过重置按钮恢复全部", async () => {
    const user = userEvent.setup();
    // 只有已启用的服务商
    const providers: GatewayUpstreamProvider[] = [
      makeProvider({ id: "p1", name: "Active Service", enabled: true }),
    ];

    renderWithProviders(
      <UpstreamProviderList
        providers={providers}
        selectedProviderId={null}
        busy={false}
        onSelect={vi.fn()}
        onToggleEnabled={vi.fn()}
        onReenable={vi.fn()}
        onAdd={vi.fn()}
      />,
    );

    // 点击已禁用，过滤结果应为空
    await user.click(screen.getByTestId("filter-status-disabled"));

    expect(screen.getByTestId("api-gateway-providers-filter-empty")).toBeInTheDocument();
    expect(screen.getByText("No matching upstream providers")).toBeInTheDocument();

    // 点击重置过滤按钮
    const resetBtn = screen.getByRole("button", { name: /Show all providers/i });
    await user.click(resetBtn);

    // 恢复显示全部
    expect(screen.getByText("Active Service")).toBeInTheDocument();
    expect(screen.queryByTestId("api-gateway-providers-filter-empty")).not.toBeInTheDocument();
  });

  it("中文环境下状态徽章与分段过滤器展示中文标签", async () => {
    await i18n.changeLanguage("zh");

    const providers: GatewayUpstreamProvider[] = [
      makeProvider({ id: "p1", name: "上游服务商 1", enabled: true }),
      makeProvider({ id: "p2", name: "上游服务商 2", enabled: false }),
    ];

    renderWithProviders(
      <UpstreamProviderList
        providers={providers}
        selectedProviderId={null}
        busy={false}
        onSelect={vi.fn()}
        onToggleEnabled={vi.fn()}
        onReenable={vi.fn()}
        onAdd={vi.fn()}
      />,
    );

    expect(screen.getByTestId("filter-status-all")).toHaveTextContent("全部");
    expect(screen.getByTestId("filter-status-enabled")).toHaveTextContent("已启用");
    expect(screen.getByTestId("filter-status-disabled")).toHaveTextContent("已禁用");

    expect(screen.getByTestId("api-gateway-status-badge-p1")).toHaveTextContent("已启用");
    expect(screen.getByTestId("api-gateway-status-badge-p2")).toHaveTextContent("已禁用");
  });
});
