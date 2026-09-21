import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { UpstreamProviderList } from "./UpstreamProviderList";
import {
  type GatewayProviderTemplateView,
  type GatewayUpstreamProvider,
} from "@/lib/apiGateway";
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
        id: "p-auto-legacy",
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

    // Step 3: 底栏徽章只检查 provider.enabled，provider-level auto_disabled 不再控制；
    // enabled=true → Enabled 徽章，无琥珀色
    const legacyAutoBadge = screen.getByTestId("api-gateway-status-badge-p-auto-legacy");
    expect(legacyAutoBadge).toHaveTextContent("Enabled");
    expect(legacyAutoBadge.className).toContain("bg-emerald-500/10");

    // 旧的全局 "Auto-disabled" 文本不应出现在底栏中：
    expect(screen.queryByText("Auto-disabled")).not.toBeInTheDocument();
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
        onAdd={vi.fn()}
      />,
    );

    expect(screen.getByTestId("filter-status-all")).toHaveTextContent("全部");
    expect(screen.getByTestId("filter-status-enabled")).toHaveTextContent("已启用");
    expect(screen.getByTestId("filter-status-disabled")).toHaveTextContent("已禁用");

    expect(screen.getByTestId("api-gateway-status-badge-p1")).toHaveTextContent("已启用");
    expect(screen.getByTestId("api-gateway-status-badge-p2")).toHaveTextContent("已禁用");
  });

  it("提供 templateSection 时渲染在服务商列表上方，不提供时行为不变", () => {
    const providers: GatewayUpstreamProvider[] = [
      makeProvider({ id: "p1", name: "Provider 1", enabled: true }),
    ];

    const { unmount } = renderWithProviders(
      <UpstreamProviderList
        providers={providers}
        selectedProviderId={null}
        busy={false}
        onSelect={vi.fn()}
        onToggleEnabled={vi.fn()}
        onAdd={vi.fn()}
        templateSection={<div data-testid="api-gateway-template-slot">Templates</div>}
      />,
    );

    const slot = screen.getByTestId("api-gateway-template-slot");
    const providerCard = screen.getByTestId("api-gateway-provider-p1");
    expect(slot).toBeInTheDocument();
    expect(slot).toHaveTextContent("Templates");
    // 插槽必须出现在服务商卡片之前（列表上方）
    expect(
      slot.compareDocumentPosition(providerCard) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();

    // 不传 templateSection 时既有渲染不受影响，也不出现插槽
    unmount();
    renderWithProviders(
      <UpstreamProviderList
        providers={providers}
        selectedProviderId={null}
        busy={false}
        onSelect={vi.fn()}
        onToggleEnabled={vi.fn()}
        onAdd={vi.fn()}
      />,
    );

    expect(screen.queryByTestId("api-gateway-template-slot")).not.toBeInTheDocument();
    expect(screen.getByText("Provider 1")).toBeInTheDocument();
  });
});

describe("UpstreamProviderList 模板头像与退休映射提示", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  function makeTemplateView(
    overrides: {
      id?: string;
      name?: string;
      icon?: string | null;
      models?: string[];
    } = {},
  ): GatewayProviderTemplateView {
    const {
      id = "tpl-1",
      name = "OpenCode Zen",
      icon = "opencode",
      models = ["remote-a"],
    } = overrides;
    return {
      template: {
        id,
        name,
        description: "Curated provider template",
        base_url: "https://opencode.ai/zen/v1",
        protocol: "responses",
        source: "https://opencode.ai/zen/v1/models",
        models_url: null,
        models: models.map((upstream_model) => ({
          upstream_model,
          enabled: true,
        })),
        icon,
      },
      synced_at: null,
      source: "https://opencode.ai/zen/v1/models",
      from_snapshot: true,
    };
  }

  function providerListElement(
    providers: GatewayUpstreamProvider[],
    templates: GatewayProviderTemplateView[],
  ) {
    return (
      <UpstreamProviderList
        providers={providers}
        // Step 2 contract: the list accepts the loaded template views so cards
        // can resolve `template_id` to a template icon and retired-mapping hint.
        templates={templates}
        selectedProviderId={null}
        busy={false}
        onSelect={vi.fn()}
        onToggleEnabled={vi.fn()}
        onAdd={vi.fn()}
      />
    );
  }

  it("AC-006 绑定模板的卡片显示模板头像，手动与无法解析模板的卡片不显示", () => {
    const providers: GatewayUpstreamProvider[] = [
      makeProvider({
        id: "p-bound",
        name: "Template Bound",
        template_id: "tpl-opencode",
        mappings: [{ local_model: "l-a", upstream_model: "remote-a" }],
      }),
      makeProvider({ id: "p-manual", name: "Manual Provider" }),
      makeProvider({
        id: "p-unresolved",
        name: "Unresolved Provider",
        template_id: "tpl-missing",
        mappings: [{ local_model: "l-b", upstream_model: "remote-b" }],
      }),
    ];
    const templates = [
      makeTemplateView({
        id: "tpl-opencode",
        name: "OpenCode Zen",
        icon: "opencode",
      }),
    ];

    renderWithProviders(providerListElement(providers, templates));

    const boundIcon = screen.getByTestId(
      "api-gateway-provider-template-icon-p-bound",
    );
    expect(boundIcon).toHaveAttribute(
      "title",
      "Created from template OpenCode Zen",
    );
    expect(
      within(boundIcon).getByTestId("provider-icon-opencode"),
    ).toBeInTheDocument();
    const avatarBox = boundIcon.querySelector("div");
    expect(avatarBox).toHaveStyle({ width: "36px", height: "36px" });

    // 手动服务商卡片不得渲染模板头像。
    const manualCard = screen.getByTestId("api-gateway-provider-p-manual");
    expect(
      within(manualCard).queryByTestId(
        "api-gateway-provider-template-icon-p-manual",
      ),
    ).not.toBeInTheDocument();

    // 无法解析模板的服务商既不渲染头像，也不报错，名称照常展示。
    const unresolvedCard = screen.getByTestId(
      "api-gateway-provider-p-unresolved",
    );
    expect(
      within(unresolvedCard).queryByTestId(
        "api-gateway-provider-template-icon-p-unresolved",
      ),
    ).not.toBeInTheDocument();
    expect(screen.getByText("Unresolved Provider")).toBeInTheDocument();
  });

  it("AC-007 退休映射提示按 props 推导计数与 tooltip，并随 re-render 变化直至消失", () => {
    const template = makeTemplateView({
      id: "tpl-opencode",
      name: "OpenCode Zen",
      models: ["remote-a", "kept-model"],
    });
    const baseProvider = makeProvider({
      id: "p-bound",
      name: "Template Bound",
      template_id: "tpl-opencode",
      mappings: [
        { local_model: "l-gone-a", upstream_model: "gone-a", enabled: false },
        { local_model: "l-gone-b", upstream_model: "gone-b", enabled: false },
        // 正常启用映射不得计入退休计数。
        { local_model: "l-normal", upstream_model: "remote-a", enabled: true },
        // 模板仍包含的禁用映射同样不得计入。
        {
          local_model: "l-tpl-disabled",
          upstream_model: "kept-model",
          enabled: false,
        },
      ],
    });

    const view = renderWithProviders(
      providerListElement([baseProvider], [template]),
    );

    const hint = screen.getByTestId(
      "api-gateway-provider-retired-mappings-p-bound",
    );
    expect(hint).toHaveTextContent("2 mapping(s) removed from template");
    expect(hint).toHaveAttribute(
      "title",
      "Removed from the template and disabled: gone-a, gone-b",
    );
    expect(hint.getAttribute("title")).not.toContain("remote-a");
    expect(hint.getAttribute("title")).not.toContain("kept-model");

    // 重新启用其中一个映射：计数降为 1，tooltip 不再包含被启用模型。
    const oneEnabled = makeProvider({
      ...baseProvider,
      mappings: baseProvider.mappings.map((mapping) =>
        mapping.upstream_model === "gone-b"
          ? { ...mapping, enabled: true }
          : mapping,
      ),
    });
    view.rerender(providerListElement([oneEnabled], [template]));

    const singleHint = screen.getByTestId(
      "api-gateway-provider-retired-mappings-p-bound",
    );
    expect(singleHint).toHaveTextContent("1 mapping(s) removed from template");
    expect(singleHint).toHaveAttribute(
      "title",
      "Removed from the template and disabled: gone-a",
    );
    expect(singleHint.getAttribute("title")).not.toContain("gone-b");

    // 全部重新启用后提示完全消失。
    const allEnabled = makeProvider({
      ...baseProvider,
      mappings: baseProvider.mappings.map((mapping) => ({
        ...mapping,
        enabled: true,
      })),
    });
    view.rerender(providerListElement([allEnabled], [template]));

    expect(
      screen.queryByTestId("api-gateway-provider-retired-mappings-p-bound"),
    ).not.toBeInTheDocument();

    // 删除退役映射（而非重新启用）后提示同样消失：计数始终由当前 props 推导。
    const retiredRemoved = makeProvider({
      ...baseProvider,
      mappings: baseProvider.mappings.filter(
        (mapping) =>
          mapping.upstream_model !== "gone-a" &&
          mapping.upstream_model !== "gone-b",
      ),
    });
    view.rerender(providerListElement([retiredRemoved], [template]));

    expect(
      screen.queryByTestId("api-gateway-provider-retired-mappings-p-bound"),
    ).not.toBeInTheDocument();
  });

  it("AC-007 模板仍包含的禁用映射不计入退休提示", () => {
    const template = makeTemplateView({ models: ["kept-model"] });
    const provider = makeProvider({
      id: "p-bound",
      name: "Template Bound",
      template_id: "tpl-1",
      mappings: [
        { local_model: "l-kept", upstream_model: "kept-model", enabled: false },
      ],
    });

    renderWithProviders(providerListElement([provider], [template]));

    expect(
      screen.queryByTestId("api-gateway-provider-retired-mappings-p-bound"),
    ).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Step 3: per-model row-level auto-disable UI — read-only hint & footer badge
// ---------------------------------------------------------------------------

describe("UpstreamProviderList 逐行 auto-disabled 读提示", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("有 auto-disabled 行的服务商展示只读提示，含精确数量且无重启用按钮", () => {
    const providers: GatewayUpstreamProvider[] = [
      makeProvider({
        id: "p-hint",
        name: "Auto-disabled Provider",
        enabled: true,
        mappings: [
          {
            local_model: "m1",
            upstream_model: "ra",
            enabled: true,
          },
          {
            local_model: "m2",
            upstream_model: "rb",
            enabled: true,
            auto_disabled: true,
          },
          {
            local_model: "m3",
            upstream_model: "rc",
            enabled: true,
            auto_disabled: true,
          },
        ],
      }),
    ];

    renderWithProviders(
      <UpstreamProviderList
        providers={providers}
        selectedProviderId={null}
        busy={false}
        onSelect={vi.fn()}
        onToggleEnabled={vi.fn()}
        onAdd={vi.fn()}
      />,
    );

    // 只读提示应包含 data-testid
    const hintEl = screen.getByTestId(
      "api-gateway-provider-auto-disabled-models-p-hint",
    );
    expect(hintEl).toBeInTheDocument();

    // 提示文案应通过 i18n 键生成，包含 count 插值
    expect(hintEl.textContent).toContain("2");

    // 底栏徽章反映 provider.enabled 而非 auto_disabled
    const badge = screen.getByTestId("api-gateway-status-badge-p-hint");
    expect(badge).toHaveTextContent("Enabled");
    expect(badge.className).toContain("bg-emerald-500/10");

    // 不应有整个服务商的 re-enable 按钮
    const reenableBtn = screen.queryByRole("button", {
      name: /Re-enable provider|Re-enable/i,
    });
    expect(reenableBtn).not.toBeInTheDocument();
  });

  it("provider-level auto_disabled 为 true 但无 auto-disabled 行时不显示提示", () => {
    const providers: GatewayUpstreamProvider[] = [
      makeProvider({
        id: "p-legacy",
        name: "Legacy Only Flag",
        enabled: true,
        auto_disabled: true,
        mappings: [
          { local_model: "a", upstream_model: "ra", enabled: true },
        ],
      }),
    ];

    renderWithProviders(
      <UpstreamProviderList
        providers={providers}
        selectedProviderId={null}
        busy={false}
        onSelect={vi.fn()}
        onToggleEnabled={vi.fn()}
        onAdd={vi.fn()}
      />,
    );

    // 不应出现逐行 auto-disabled 提示
    expect(
      screen.queryByTestId("api-gateway-provider-auto-disabled-models-p-legacy"),
    ).not.toBeInTheDocument();
    // 底栏徽章反映 enabled=true → Enabled
    const badge = screen.getByTestId("api-gateway-status-badge-p-legacy");
    expect(badge).toHaveTextContent("Enabled");
  });

  it("所有行都 auto-disabled 时提示包含全量行数", () => {
    const providers: GatewayUpstreamProvider[] = [
      makeProvider({
        id: "p-full",
        name: "All Auto-disabled",
        enabled: true,
        mappings: [
          { local_model: "m1", upstream_model: "ra", auto_disabled: true },
          { local_model: "m2", upstream_model: "rb", auto_disabled: true },
        ],
      }),
    ];

    renderWithProviders(
      <UpstreamProviderList
        providers={providers}
        selectedProviderId={null}
        busy={false}
        onSelect={vi.fn()}
        onToggleEnabled={vi.fn()}
        onAdd={vi.fn()}
      />,
    );

    const hintEl = screen.getByTestId(
      "api-gateway-provider-auto-disabled-models-p-full",
    );
    expect(hintEl.textContent).toContain("2");
    // 底栏徽章仍然是 Enabled（enabled=true）
    const badge = screen.getByTestId("api-gateway-status-badge-p-full");
    expect(badge).toHaveTextContent("Enabled");
  });
});

describe("UpstreamProviderList 权重徽标展示 (AC-014)", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("当 provider 的 weight: 3 时渲染权重徽标且文本包含 Weight: 3，weight 未定义时默认显示 Weight: 1", () => {
    const providers: GatewayUpstreamProvider[] = [
      makeProvider({ id: "p-weighted", name: "Weighted Provider", weight: 3 } as any),
      makeProvider({ id: "p-default", name: "Default Provider" }),
    ];

    renderWithProviders(
      <UpstreamProviderList
        providers={providers}
        selectedProviderId={null}
        busy={false}
        onSelect={vi.fn()}
        onToggleEnabled={vi.fn()}
        onAdd={vi.fn()}
      />,
    );

    const weightedBadge = screen.getByTestId("api-gateway-weight-badge-p-weighted");
    expect(weightedBadge).toBeInTheDocument();
    expect(weightedBadge).toHaveTextContent(/Weight:\s*3/i);

    const defaultBadge = screen.getByTestId("api-gateway-weight-badge-p-default");
    expect(defaultBadge).toBeInTheDocument();
    expect(defaultBadge).toHaveTextContent(/Weight:\s*1/i);
  });

  it("中文环境下权重徽标显示 权重: 3 与默认 权重: 1", async () => {
    await i18n.changeLanguage("zh");

    const providers: GatewayUpstreamProvider[] = [
      makeProvider({ id: "p-weighted", name: "Weighted Provider", weight: 3 } as any),
      makeProvider({ id: "p-default", name: "Default Provider" }),
    ];

    renderWithProviders(
      <UpstreamProviderList
        providers={providers}
        selectedProviderId={null}
        busy={false}
        onSelect={vi.fn()}
        onToggleEnabled={vi.fn()}
        onAdd={vi.fn()}
      />,
    );

    const weightedBadge = screen.getByTestId("api-gateway-weight-badge-p-weighted");
    expect(weightedBadge).toBeInTheDocument();
    expect(weightedBadge).toHaveTextContent(/权重:\s*3/);

    const defaultBadge = screen.getByTestId("api-gateway-weight-badge-p-default");
    expect(defaultBadge).toBeInTheDocument();
    expect(defaultBadge).toHaveTextContent(/权重:\s*1/);
  });
});

