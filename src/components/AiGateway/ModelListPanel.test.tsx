import { act, fireEvent, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { ConfirmDialogProvider } from "@/components/ConfirmDialogProvider";
import { ToastProvider } from "@/components/ToastProvider";
import type { GatewayUpstreamProvider } from "@/lib/aiGateway";
import { AI_GATEWAY_DEFAULT_PORT } from "@/lib/aiGateway";
import { renderWithProviders } from "@/test/mocks/render";
import { resetTauriMocks } from "@/test/mocks/tauri";
import { ModelListPanel } from "./ModelListPanel";

function makeProvider(
  overrides: Partial<GatewayUpstreamProvider> = {},
): GatewayUpstreamProvider {
  return {
    id: "provider-1",
    name: "Provider 1",
    base_url: "https://api.example.com",
    api_key: "********",
    default_model: null,
    mappings: [],
    enabled: true,
    ...overrides,
  } as GatewayUpstreamProvider;
}

function renderPanel(
  providers: GatewayUpstreamProvider[],
  onNavigateProviders?: () => void,
  port?: number,
) {
  return renderWithProviders(
    <ModelListPanel
      providers={providers}
      port={port}
      onNavigateProviders={onNavigateProviders}
    />,
  );
}

function rerenderPanel(
  rerender: ReturnType<typeof renderPanel>["rerender"],
  providers: GatewayUpstreamProvider[],
) {
  rerender(
    <ToastProvider>
      <ConfirmDialogProvider>
        <ModelListPanel providers={providers} />
      </ConfirmDialogProvider>
    </ToastProvider>,
  );
}

function searchBox(): HTMLInputElement {
  return screen.getByTestId("ai-gateway-model-list-search") as HTMLInputElement;
}

function rowModels(): Array<string | null> {
  return screen
    .queryAllByTestId("ai-gateway-model-list-row")
    .map((row) => row.getAttribute("data-model"));
}

describe("ModelListPanel 本地模型列表", () => {
  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
  });

  it("渲染三列模型 ID、模型名称与上游来源，且不再显示默认模型", () => {
    renderPanel([
      makeProvider({
        id: "p1",
        name: "Provider 1",
        default_model: "fallback-default",
        mappings: [{ local_model: "gpt-4o", upstream_model: "gpt-4o-2024" }],
      }),
    ]);

    expect(screen.getByTestId("ai-gateway-model-list-table")).toBeInTheDocument();

    const rows = screen.getAllByTestId("ai-gateway-model-list-row");
    expect(rows).toHaveLength(1);
    const row = rows[0];
    expect(row).toHaveAttribute("data-model", "gpt-4o");
    expect(within(row).getByTestId("ai-gateway-model-list-id").textContent).toBe(
      "gpt-4o",
    );
    expect(within(row).getByTestId("ai-gateway-model-list-name").textContent).toBe(
      "gpt-4o-2024",
    );

    const upstreams = within(row).getAllByTestId(
      "ai-gateway-model-list-upstream",
    );
    expect(upstreams).toHaveLength(1);

    const mappingEntry = upstreams[0];
    expect(mappingEntry).toHaveAttribute("data-default", "false");
    expect(
      within(mappingEntry).getByTestId(
        "ai-gateway-model-list-upstream-provider",
      ).textContent,
    ).toBe("Provider 1");
    expect(
      within(mappingEntry).getByTestId("ai-gateway-model-list-upstream-model")
        .textContent,
    ).toBe("gpt-4o-2024");
    expect(
      within(mappingEntry).queryByTestId(
        "ai-gateway-model-list-upstream-default",
      ),
    ).not.toBeInTheDocument();
  });

  it("同一本地模型被两个服务商映射时只渲染一行并按聚合顺序列出全部上游来源", () => {
    renderPanel([
      makeProvider({
        id: "pa",
        name: "Provider A",
        mappings: [
          { local_model: "shared-model", upstream_model: "upstream-a" },
        ],
      }),
      makeProvider({
        id: "pb",
        name: "Provider B",
        mappings: [
          { local_model: "shared-model", upstream_model: "upstream-b" },
        ],
      }),
    ]);

    const rows = screen.getAllByTestId("ai-gateway-model-list-row");
    expect(rows).toHaveLength(1);
    expect(rows[0]).toHaveAttribute("data-model", "shared-model");

    const upstreams = within(rows[0]).getAllByTestId(
      "ai-gateway-model-list-upstream",
    );
    expect(upstreams).toHaveLength(2);
    expect(
      upstreams.map(
        (entry) =>
          within(entry).getByTestId(
            "ai-gateway-model-list-upstream-provider",
          ).textContent,
      ),
    ).toEqual(["Provider A", "Provider B"]);
    expect(
      upstreams.map(
        (entry) =>
          within(entry).getByTestId(
            "ai-gateway-model-list-upstream-model",
          ).textContent,
      ),
    ).toEqual(["upstream-a", "upstream-b"]);
  });

  it("模型名称优先取映射 display_name 并去除首尾空白，不包含未映射的默认模型", () => {
    renderPanel([
      makeProvider({
        default_model: "default-only",
        mappings: [
          {
            local_model: "mapped-model",
            upstream_model: "upstream-mapped",
            display_name: "  Friendly  ",
          },
        ],
      }),
    ]);

    const rows = screen.getAllByTestId("ai-gateway-model-list-row");
    expect(rows).toHaveLength(1);

    const mapped = rows[0];
    expect(mapped).toHaveAttribute("data-model", "mapped-model");
    expect(
      within(mapped).getByTestId("ai-gateway-model-list-name").textContent,
    ).toBe("Friendly");
    expect(rowModels()).not.toContain("default-only");
  });

  it("启用且未自动禁用的映射才产生行", () => {
    renderPanel([
      makeProvider({
        id: "p-disabled",
        name: "Disabled provider",
        enabled: false,
        default_model: "disabled-default",
        mappings: [
          { local_model: "disabled-mapping", upstream_model: "up-disabled" },
        ],
      }),
      makeProvider({
        id: "p-auto",
        name: "Enabled provider with mapping",
        default_model: "auto-default",
        mappings: [{ local_model: "auto-mapping", upstream_model: "up-auto" }],
      }),
      makeProvider({
        id: "p-keep",
        name: "Keeper",
        default_model: null,
        mappings: [
          { local_model: "keep", upstream_model: "up-keep" },
          { local_model: "auto-row", upstream_model: "up-auto-row", auto_disabled: true },
          {
            local_model: "disabled-only",
            upstream_model: "up-disabled-only",
            enabled: false,
          },
        ],
      }),
    ]);

    // p-disabled（服务商禁用）不产出行；p-auto（服务商启用且行健康）产出行；p-keep 的 auto 行被跳过
    expect(rowModels()).toEqual(["auto-mapping", "keep"]);
    expect(screen.queryByTestId("ai-gateway-model-list-empty")).not.toBeInTheDocument();
  });

  it("providers 变更后重新启用的服务商行立即出现", () => {
    const disabledProvider = makeProvider({
      id: "p-late",
      name: "Late provider",
      enabled: false,
      default_model: "late-fallback",
      mappings: [{ local_model: "late-model", upstream_model: "up-late" }],
    });

    const { rerender } = renderPanel([disabledProvider]);
    expect(
      screen.getByTestId("ai-gateway-model-list-empty"),
    ).toBeInTheDocument();
    expect(rowModels()).toEqual([]);

    rerenderPanel(rerender, [
      { ...disabledProvider, enabled: true },
    ]);

    const rows = screen.getAllByTestId("ai-gateway-model-list-row");
    expect(rows).toHaveLength(1);
    expect(rows[0]).toHaveAttribute("data-model", "late-model");
    expect(
      screen.queryByTestId("ai-gateway-model-list-empty"),
    ).not.toBeInTheDocument();
  });

  it("搜索仅按模型 ID 与名称做大小写不敏感的子串匹配，上游字段与清空查询行为正确", () => {
    renderPanel([
      makeProvider({
        name: "Provider 1",
        mappings: [
          {
            local_model: "gpt-4o",
            upstream_model: "gpt-4o-2024",
            display_name: "GPT-4o",
          },
        ],
      }),
    ]);

    const upstreamText =
      screen
        .getByTestId("ai-gateway-model-list-upstreams")
        .textContent ?? "";
    expect(upstreamText).toContain("Provider 1");
    expect(upstreamText).toContain("gpt-4o-2024");

    const input = searchBox();

    fireEvent.change(input, { target: { value: "gpt" } });
    expect(rowModels()).toEqual(["gpt-4o"]);

    fireEvent.change(input, { target: { value: "4O" } });
    expect(rowModels()).toEqual(["gpt-4o"]);

    fireEvent.change(input, { target: { value: "pt-4" } });
    expect(rowModels()).toEqual(["gpt-4o"]);

    fireEvent.change(input, { target: { value: "Provider 1" } });
    expect(rowModels()).toEqual([]);
    expect(
      screen.getByTestId("ai-gateway-model-list-no-match"),
    ).toBeInTheDocument();

    fireEvent.change(input, { target: { value: "gpt-4o-2024" } });
    expect(rowModels()).toEqual([]);
    expect(
      screen.getByTestId("ai-gateway-model-list-no-match"),
    ).toBeInTheDocument();

    fireEvent.change(input, { target: { value: "" } });
    expect(rowModels()).toEqual(["gpt-4o"]);
    expect(
      screen.queryByTestId("ai-gateway-model-list-no-match"),
    ).not.toBeInTheDocument();
  });

  it("聚合为空时展示空态且不渲染表格与数据行", () => {
    renderPanel([
      makeProvider({
        enabled: false,
        default_model: "disabled-default",
        mappings: [
          { local_model: "disabled-mapping", upstream_model: "up-disabled" },
        ],
      }),
    ]);

    expect(screen.getByTestId("ai-gateway-model-list-empty")).toBeInTheDocument();
    expect(rowModels()).toEqual([]);
    expect(
      screen.queryByTestId("ai-gateway-model-list-table"),
    ).not.toBeInTheDocument();
  });

  it("非空查询无匹配时展示无匹配态且不渲染数据行，全空白查询展示全部行", () => {
    renderPanel([
      makeProvider({
        mappings: [
          { local_model: "alpha", upstream_model: "up-alpha" },
          { local_model: "beta", upstream_model: "up-beta" },
        ],
      }),
    ]);

    expect(rowModels()).toHaveLength(2);

    fireEvent.change(searchBox(), { target: { value: "zzz" } });
    expect(
      screen.getByTestId("ai-gateway-model-list-no-match"),
    ).toBeInTheDocument();
    expect(rowModels()).toEqual([]);
    expect(
      screen.queryByTestId("ai-gateway-model-list-empty"),
    ).not.toBeInTheDocument();

    fireEvent.change(searchBox(), { target: { value: "   " } });
    expect(
      screen.queryByTestId("ai-gateway-model-list-no-match"),
    ).not.toBeInTheDocument();
    expect(rowModels()).toHaveLength(2);
  });

  it("搜索分别命中模型名称与模型 ID：名称独有与 ID 独有的子串都保留行，仅上游独有的子串不命中", () => {
    renderPanel([
      makeProvider({
        id: "p1",
        name: "Provider 1",
        mappings: [
          {
            local_model: "local-only-model",
            upstream_model: "remote-only-model",
            display_name: "Friendly Label",
          },
        ],
      }),
    ]);

    expect(rowModels()).toEqual(["local-only-model"]);
    expect(
      within(
        screen.getByTestId("ai-gateway-model-list-row"),
      ).getByTestId("ai-gateway-model-list-name").textContent,
    ).toBe("Friendly Label");

    fireEvent.change(searchBox(), { target: { value: "friendly" } });
    expect(rowModels()).toEqual(["local-only-model"]);
    expect(
      screen.queryByTestId("ai-gateway-model-list-no-match"),
    ).not.toBeInTheDocument();

    fireEvent.change(searchBox(), { target: { value: "label" } });
    expect(rowModels()).toEqual(["local-only-model"]);

    fireEvent.change(searchBox(), { target: { value: "local-only" } });
    expect(rowModels()).toEqual(["local-only-model"]);

    fireEvent.change(searchBox(), { target: { value: "remote-only-model" } });
    expect(rowModels()).toEqual([]);
    expect(
      screen.getByTestId("ai-gateway-model-list-no-match"),
    ).toBeInTheDocument();
  });

  it("providers 为空数组时展示空态，不渲染表格且没有任何数据行", () => {
    renderPanel([]);

    expect(
      screen.getByTestId("ai-gateway-model-list-empty"),
    ).toBeInTheDocument();
    expect(
      screen.queryAllByTestId("ai-gateway-model-list-row"),
    ).toHaveLength(0);
    expect(
      screen.queryByTestId("ai-gateway-model-list-table"),
    ).not.toBeInTheDocument();
  });

  it("展示模型列表 API 获取地址并在指定端口时正确计算 URL", () => {
    const { unmount } = renderPanel([]);
    const defaultApiUrl = screen.getByTestId("ai-gateway-model-list-api-url");
    expect(defaultApiUrl.textContent).toBe(
      `http://127.0.0.1:${AI_GATEWAY_DEFAULT_PORT}/v1/models`,
    );
    unmount();

    renderPanel([], undefined, 18888);
    const customApiUrl = screen.getByTestId("ai-gateway-model-list-api-url");
    expect(customApiUrl.textContent).toBe("http://127.0.0.1:18888/v1/models");
  });

  it("点击 API 地址复制按钮调用剪贴板并复制对应的模型列表地址", async () => {
    const writeTextSpy = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, {
      clipboard: {
        writeText: writeTextSpy,
      },
    });

    renderPanel([], undefined, 18888);

    const copyBtn = screen.getByTestId("ai-gateway-model-list-copy-api-url-btn");
    expect(copyBtn).toHaveAttribute("aria-label", "Copy models API endpoint");
    await act(async () => {
      fireEvent.click(copyBtn);
    });

    expect(writeTextSpy).toHaveBeenCalledWith(
      "http://127.0.0.1:18888/v1/models",
    );
    expect(
      screen.getByText("Models API endpoint copied to clipboard"),
    ).toBeInTheDocument();
  });

  it("点击复制按钮调用剪贴板并复制对应的 Model ID", async () => {
    const writeTextSpy = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, {
      clipboard: {
        writeText: writeTextSpy,
      },
    });

    renderPanel([
      makeProvider({
        mappings: [{ local_model: "test-copy-model", upstream_model: "upstream-1" }],
      }),
    ]);

    const copyBtn = screen.getByTestId("ai-gateway-model-list-copy-btn");
    expect(copyBtn).toBeInTheDocument();
    await act(async () => {
      fireEvent.click(copyBtn);
    });

    expect(writeTextSpy).toHaveBeenCalledWith("test-copy-model");
  });

  it("点击上游模型复制按钮调用剪贴板并复制对应的上游模型 ID", async () => {
    const writeTextSpy = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, {
      clipboard: {
        writeText: writeTextSpy,
      },
    });

    renderPanel([
      makeProvider({
        mappings: [
          { local_model: "test-model", upstream_model: "upstream-gpt-4o" },
        ],
      }),
    ]);

    const upstreamCopyBtn = screen.getByTestId(
      "ai-gateway-model-list-upstream-copy-btn",
    );
    expect(upstreamCopyBtn).toBeInTheDocument();
    expect(upstreamCopyBtn).toHaveAttribute(
      "aria-label",
      "Copy upstream model ID",
    );

    await act(async () => {
      fireEvent.click(upstreamCopyBtn);
    });

    expect(writeTextSpy).toHaveBeenCalledWith("upstream-gpt-4o");
  });

  it("多个上游服务商时，每个上游条目的复制按钮分别复制各自的上游模型 ID", async () => {
    const writeTextSpy = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, {
      clipboard: {
        writeText: writeTextSpy,
      },
    });

    renderPanel([
      makeProvider({
        id: "p1",
        name: "Provider 1",
        mappings: [
          { local_model: "shared-model", upstream_model: "upstream-first" },
        ],
      }),
      makeProvider({
        id: "p2",
        name: "Provider 2",
        mappings: [
          { local_model: "shared-model", upstream_model: "upstream-second" },
        ],
      }),
    ]);

    const upstreamCopyBtns = screen.getAllByTestId(
      "ai-gateway-model-list-upstream-copy-btn",
    );
    expect(upstreamCopyBtns).toHaveLength(2);

    await act(async () => {
      fireEvent.click(upstreamCopyBtns[0]);
    });
    expect(writeTextSpy).toHaveBeenLastCalledWith("upstream-first");

    await act(async () => {
      fireEvent.click(upstreamCopyBtns[1]);
    });
    expect(writeTextSpy).toHaveBeenLastCalledWith("upstream-second");
  });

  it("支持按服务商筛选模型", () => {
    renderPanel([
      makeProvider({
        id: "p-openai",
        name: "OpenAI",
        mappings: [{ local_model: "gpt-model", upstream_model: "gpt-4o" }],
      }),
      makeProvider({
        id: "p-anthropic",
        name: "Anthropic",
        mappings: [{ local_model: "claude-model", upstream_model: "claude-3" }],
      }),
    ]);

    expect(rowModels()).toEqual(["claude-model", "gpt-model"]);

    const filterTrigger = screen.getByTestId("ai-gateway-model-list-provider-filter-trigger");
    fireEvent.click(filterTrigger);

    const openaiOption = screen.getByRole("option", { name: "OpenAI" });
    fireEvent.click(openaiOption);

    expect(rowModels()).toEqual(["gpt-model"]);
  });

  it("支持按协议筛选模型", () => {
    renderPanel([
      makeProvider({
        id: "p1",
        name: "Provider 1",
        protocol: "chat_completions",
        mappings: [
          {
            local_model: "chat-model",
            upstream_model: "chat-up",
            protocol: "chat_completions",
          },
          {
            local_model: "resp-model",
            upstream_model: "resp-up",
            protocol: "responses",
          },
        ],
      }),
    ]);

    expect(rowModels()).toEqual(["chat-model", "resp-model"]);

    const protocolTrigger = screen.getByTestId("ai-gateway-model-list-protocol-filter-trigger");
    fireEvent.click(protocolTrigger);

    const responsesOption = screen.getByRole("option", { name: "Responses" });
    fireEvent.click(responsesOption);

    expect(rowModels()).toEqual(["resp-model"]);
  });

  it("输入搜索词后支持点击清除按钮清空搜索", () => {
    renderPanel([
      makeProvider({
        mappings: [
          { local_model: "model-a", upstream_model: "up-a" },
          { local_model: "model-b", upstream_model: "up-b" },
        ],
      }),
    ]);

    const input = searchBox();
    fireEvent.change(input, { target: { value: "model-a" } });
    expect(rowModels()).toEqual(["model-a"]);

    const clearBtn = screen.getByLabelText("Clear search");
    expect(clearBtn).toBeInTheDocument();
    fireEvent.click(clearBtn);

    expect(rowModels()).toEqual(["model-a", "model-b"]);
  });

  it("空态下提供前往配置上游服务商按钮并可点击触发回调", () => {
    const onNavigateProviders = vi.fn();
    renderPanel([], onNavigateProviders);

    const goToBtn = screen.getByRole("button", {
      name: /Configure upstream providers/i,
    });
    expect(goToBtn).toBeInTheDocument();
    fireEvent.click(goToBtn);
    expect(onNavigateProviders).toHaveBeenCalledTimes(1);
  });

  it("未配置推理强度时不渲染推理强度标签容器", () => {
    renderPanel([
      makeProvider({
        mappings: [{ local_model: "gpt-4o", upstream_model: "gpt-4o-2024" }],
      }),
    ]);

    expect(
      screen.queryByTestId("ai-gateway-model-list-reasoning-efforts"),
    ).not.toBeInTheDocument();
  });

  it("配置推理强度时在第二列模型名称下方展示推理强度标签及各个档位", () => {
    renderPanel([
      makeProvider({
        mappings: [
          {
            local_model: "o3-mini",
            upstream_model: "o3-mini-2025",
            reasoning_efforts: ["low", "medium", "high"],
          },
        ],
      }),
    ]);

    const container = screen.getByTestId(
      "ai-gateway-model-list-reasoning-efforts",
    );
    expect(container).toBeInTheDocument();
    expect(container).toHaveTextContent("Reasoning efforts:");

    expect(
      screen.getByTestId("ai-gateway-model-list-effort-low"),
    ).toHaveTextContent("low");
    expect(
      screen.getByTestId("ai-gateway-model-list-effort-medium"),
    ).toHaveTextContent("medium");
    expect(
      screen.getByTestId("ai-gateway-model-list-effort-high"),
    ).toHaveTextContent("high");
  });

  it("多服务商映射同一模型时在第二列合并去重展示所有推理强度档位", () => {
    renderPanel([
      makeProvider({
        id: "p1",
        name: "Provider 1",
        mappings: [
          {
            local_model: "deepseek-r1",
            upstream_model: "deepseek-r1-a",
            reasoning_efforts: ["medium", "high"],
          },
        ],
      }),
      makeProvider({
        id: "p2",
        name: "Provider 2",
        mappings: [
          {
            local_model: "deepseek-r1",
            upstream_model: "deepseek-r1-b",
            reasoning_efforts: ["low", "high", "max"],
          },
        ],
      }),
    ]);

    const rows = screen.getAllByTestId("ai-gateway-model-list-row");
    expect(rows).toHaveLength(1);

    const effortsContainer = within(rows[0]).getByTestId(
      "ai-gateway-model-list-reasoning-efforts",
    );
    expect(effortsContainer).toBeInTheDocument();

    const efforts = within(effortsContainer)
      .getAllByTestId(/^ai-gateway-model-list-effort-/)
      .map((el) => el.textContent);
    expect(efforts).toEqual(["medium", "high", "low", "max"]);
  });

  it("中文环境下第二列正确展示推理档位标签文案", async () => {
    await act(async () => {
      await i18n.changeLanguage("zh");
    });
    renderPanel([
      makeProvider({
        mappings: [
          {
            local_model: "deepseek-r1",
            upstream_model: "deepseek-r1",
            reasoning_efforts: ["high"],
          },
        ],
      }),
    ]);

    const container = screen.getByTestId(
      "ai-gateway-model-list-reasoning-efforts",
    );
    expect(container).toHaveTextContent("推理档位:");
    expect(
      screen.getByTestId("ai-gateway-model-list-effort-high"),
    ).toHaveTextContent("high");
    await act(async () => {
      await i18n.changeLanguage("en");
    });
  });
});

const MODEL_LIST_I18N_KEYS: ReadonlyArray<
  readonly [key: string, en: string, zh: string]
> = [
  ["aiGatewayModelListTab", "Model list", "模型列表"],
  [
    "aiGatewayModelListDesc",
    "View local models exposed by the gateway, calling protocols, and mapped upstream routing sources.",
    "查看本地中继暴露的可用模型、调用协议及映射的上游路由来源。",
  ],
  ["aiGatewayModelListSearch", "Search models", "搜索模型"],
  [
    "aiGatewayModelListSearchPlaceholder",
    "Search by model ID or name",
    "按模型 ID 或名称搜索",
  ],
  ["aiGatewayModelListIdColumn", "Model ID", "模型 ID"],
  ["aiGatewayModelListNameColumn", "Model name", "模型名称"],
  ["aiGatewayModelListUpstreamColumn", "Upstream models", "上游模型"],
  [
    "aiGatewayModelListEmpty",
    "No local models are served by enabled upstream providers yet.",
    "还没有已启用上游服务商提供的本地模型。",
  ],
  [
    "aiGatewayModelListNoMatch",
    "No models match the current search.",
    "没有匹配当前搜索的模型。",
  ],
  ["aiGatewayModelListAllProviders", "All providers", "全部服务商"],
  ["aiGatewayModelListAllProtocols", "All protocols", "全部协议"],
  ["aiGatewayModelListFilterProvider", "Filter by provider", "按服务商筛选"],
  ["aiGatewayModelListFilterProtocol", "Filter by protocol", "按协议筛选"],
  ["aiGatewayModelListClearSearch", "Clear search", "清空搜索"],
  ["aiGatewayModelListClearFilters", "Clear filters", "清空筛选条件"],
  [
    "aiGatewayModelListCopySuccess",
    "Model ID copied to clipboard",
    "模型 ID 已复制到剪贴板",
  ],
  ["aiGatewayModelListCopyAria", "Copy model ID", "复制模型 ID"],
  [
    "aiGatewayModelListCopyUpstreamAria",
    "Copy upstream model ID",
    "复制上游模型 ID",
  ],
  [
    "aiGatewayModelListGoToProviders",
    "Configure upstream providers",
    "前往配置上游服务商",
  ],
  ["aiGatewayModelListApiUrl", "Models API endpoint", "模型列表 API 地址"],
  [
    "aiGatewayModelListCopyApiUrlSuccess",
    "Models API endpoint copied to clipboard",
    "模型列表 API 地址已复制到剪贴板",
  ],
  [
    "aiGatewayModelListCopyApiUrlAria",
    "Copy models API endpoint",
    "复制模型列表 API 地址",
  ],
];

describe("模型列表国际化键", () => {
  it("en 与 zh 都定义了全部键且文案与契约精确一致", async () => {
    for (const [key, en, zh] of MODEL_LIST_I18N_KEYS) {
      await i18n.changeLanguage("en");
      expect(i18n.t(key), `en:${key}`).toBe(en);

      await i18n.changeLanguage("zh");
      expect(i18n.t(key), `zh:${key}`).toBe(zh);
    }
    await i18n.changeLanguage("en");
  });
});
