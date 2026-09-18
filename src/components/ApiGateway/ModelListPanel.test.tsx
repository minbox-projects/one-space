import { fireEvent, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import i18n from "@/i18n";
import { ConfirmDialogProvider } from "@/components/ConfirmDialogProvider";
import { ToastProvider } from "@/components/ToastProvider";
import type { GatewayUpstreamProvider } from "@/lib/apiGateway";
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
    auto_disabled: false,
    disabled_reason: null,
    disabled_at: null,
    consecutive_failures: 0,
    last_error_at: null,
    ...overrides,
  };
}

function renderPanel(providers: GatewayUpstreamProvider[]) {
  return renderWithProviders(<ModelListPanel providers={providers} />);
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
  return screen.getByTestId("api-gateway-model-list-search") as HTMLInputElement;
}

function rowModels(): Array<string | null> {
  return screen
    .queryAllByTestId("api-gateway-model-list-row")
    .map((row) => row.getAttribute("data-model"));
}

describe("ModelListPanel 本地模型列表", () => {
  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
  });

  it("渲染三列模型 ID、模型名称与上游来源，并只给默认来源加 Default 徽标", () => {
    renderPanel([
      makeProvider({
        id: "p1",
        name: "Provider 1",
        default_model: "gpt-4o",
        mappings: [{ local_model: "gpt-4o", upstream_model: "gpt-4o-2024" }],
      }),
    ]);

    expect(screen.getByTestId("api-gateway-model-list-table")).toBeInTheDocument();

    const rows = screen.getAllByTestId("api-gateway-model-list-row");
    expect(rows).toHaveLength(1);
    const row = rows[0];
    expect(row).toHaveAttribute("data-model", "gpt-4o");
    expect(within(row).getByTestId("api-gateway-model-list-id").textContent).toBe(
      "gpt-4o",
    );
    expect(within(row).getByTestId("api-gateway-model-list-name").textContent).toBe(
      "gpt-4o-2024",
    );

    const upstreams = within(row).getAllByTestId(
      "api-gateway-model-list-upstream",
    );
    expect(upstreams).toHaveLength(2);

    const defaultEntry = upstreams[0];
    expect(defaultEntry).toHaveAttribute("data-default", "true");
    expect(
      within(defaultEntry).getByTestId(
        "api-gateway-model-list-upstream-provider",
      ).textContent,
    ).toBe("Provider 1");
    expect(
      within(defaultEntry).getByTestId("api-gateway-model-list-upstream-model")
        .textContent,
    ).toBe("gpt-4o");
    expect(
      within(defaultEntry).getByTestId(
        "api-gateway-model-list-upstream-default",
      ).textContent,
    ).toBe("Default");

    const mappingEntry = upstreams[1];
    expect(mappingEntry).toHaveAttribute("data-default", "false");
    expect(
      within(mappingEntry).getByTestId(
        "api-gateway-model-list-upstream-provider",
      ).textContent,
    ).toBe("Provider 1");
    expect(
      within(mappingEntry).getByTestId("api-gateway-model-list-upstream-model")
        .textContent,
    ).toBe("gpt-4o-2024");
    expect(
      within(mappingEntry).queryByTestId(
        "api-gateway-model-list-upstream-default",
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

    const rows = screen.getAllByTestId("api-gateway-model-list-row");
    expect(rows).toHaveLength(1);
    expect(rows[0]).toHaveAttribute("data-model", "shared-model");

    const upstreams = within(rows[0]).getAllByTestId(
      "api-gateway-model-list-upstream",
    );
    expect(upstreams).toHaveLength(2);
    expect(
      upstreams.map(
        (entry) =>
          within(entry).getByTestId(
            "api-gateway-model-list-upstream-provider",
          ).textContent,
      ),
    ).toEqual(["Provider A", "Provider B"]);
    expect(
      upstreams.map(
        (entry) =>
          within(entry).getByTestId(
            "api-gateway-model-list-upstream-model",
          ).textContent,
      ),
    ).toEqual(["upstream-a", "upstream-b"]);
  });

  it("模型名称优先取映射 display_name 并去除首尾空白，仅有默认模型时取 default_model", () => {
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

    const rows = screen.getAllByTestId("api-gateway-model-list-row");
    expect(rows).toHaveLength(2);

    const mapped = rows.find(
      (row) => row.getAttribute("data-model") === "mapped-model",
    );
    expect(mapped).not.toBeUndefined();
    expect(
      within(mapped!).getByTestId("api-gateway-model-list-name").textContent,
    ).toBe("Friendly");

    const defaultOnly = rows.find(
      (row) => row.getAttribute("data-model") === "default-only",
    );
    expect(defaultOnly).not.toBeUndefined();
    expect(
      within(defaultOnly!).getByTestId("api-gateway-model-list-name").textContent,
    ).toBe("default-only");
  });

  it("禁用服务商、自动禁用服务商与禁用映射都不产生行", () => {
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
        name: "Auto disabled provider",
        auto_disabled: true,
        default_model: "auto-default",
        mappings: [{ local_model: "auto-mapping", upstream_model: "up-auto" }],
      }),
      makeProvider({
        id: "p-keep",
        name: "Keeper",
        default_model: null,
        mappings: [
          { local_model: "keep", upstream_model: "up-keep" },
          {
            local_model: "disabled-only",
            upstream_model: "up-disabled-only",
            enabled: false,
          },
        ],
      }),
    ]);

    expect(rowModels()).toEqual(["keep"]);
    expect(screen.queryByTestId("api-gateway-model-list-empty")).not.toBeInTheDocument();
  });

  it("providers 变更后重新启用的服务商行立即出现", () => {
    const disabledProvider = makeProvider({
      id: "p-late",
      name: "Late provider",
      enabled: false,
      default_model: "late-model",
    });

    const { rerender } = renderPanel([disabledProvider]);
    expect(
      screen.getByTestId("api-gateway-model-list-empty"),
    ).toBeInTheDocument();
    expect(rowModels()).toEqual([]);

    rerenderPanel(rerender, [
      { ...disabledProvider, enabled: true },
    ]);

    const rows = screen.getAllByTestId("api-gateway-model-list-row");
    expect(rows).toHaveLength(1);
    expect(rows[0]).toHaveAttribute("data-model", "late-model");
    expect(
      screen.queryByTestId("api-gateway-model-list-empty"),
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
        .getByTestId("api-gateway-model-list-upstreams")
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
      screen.getByTestId("api-gateway-model-list-no-match"),
    ).toBeInTheDocument();

    fireEvent.change(input, { target: { value: "gpt-4o-2024" } });
    expect(rowModels()).toEqual([]);
    expect(
      screen.getByTestId("api-gateway-model-list-no-match"),
    ).toBeInTheDocument();

    fireEvent.change(input, { target: { value: "" } });
    expect(rowModels()).toEqual(["gpt-4o"]);
    expect(
      screen.queryByTestId("api-gateway-model-list-no-match"),
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

    expect(screen.getByTestId("api-gateway-model-list-empty")).toBeInTheDocument();
    expect(rowModels()).toEqual([]);
    expect(
      screen.queryByTestId("api-gateway-model-list-table"),
    ).not.toBeInTheDocument();
  });

  it("非空查询无匹配时展示无匹配态且不渲染数据行，全空白查询展示全部行", () => {
    renderPanel([
      makeProvider({
        default_model: "alpha",
        mappings: [{ local_model: "beta", upstream_model: "up-beta" }],
      }),
    ]);

    expect(rowModels()).toHaveLength(2);

    fireEvent.change(searchBox(), { target: { value: "zzz" } });
    expect(
      screen.getByTestId("api-gateway-model-list-no-match"),
    ).toBeInTheDocument();
    expect(rowModels()).toEqual([]);
    expect(
      screen.queryByTestId("api-gateway-model-list-empty"),
    ).not.toBeInTheDocument();

    fireEvent.change(searchBox(), { target: { value: "   " } });
    expect(
      screen.queryByTestId("api-gateway-model-list-no-match"),
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
        screen.getByTestId("api-gateway-model-list-row"),
      ).getByTestId("api-gateway-model-list-name").textContent,
    ).toBe("Friendly Label");

    fireEvent.change(searchBox(), { target: { value: "friendly" } });
    expect(rowModels()).toEqual(["local-only-model"]);
    expect(
      screen.queryByTestId("api-gateway-model-list-no-match"),
    ).not.toBeInTheDocument();

    fireEvent.change(searchBox(), { target: { value: "label" } });
    expect(rowModels()).toEqual(["local-only-model"]);

    fireEvent.change(searchBox(), { target: { value: "local-only" } });
    expect(rowModels()).toEqual(["local-only-model"]);

    fireEvent.change(searchBox(), { target: { value: "remote-only-model" } });
    expect(rowModels()).toEqual([]);
    expect(
      screen.getByTestId("api-gateway-model-list-no-match"),
    ).toBeInTheDocument();
  });

  it("providers 为空数组时展示空态，不渲染表格且没有任何数据行", () => {
    renderPanel([]);

    expect(
      screen.getByTestId("api-gateway-model-list-empty"),
    ).toBeInTheDocument();
    expect(
      screen.queryAllByTestId("api-gateway-model-list-row"),
    ).toHaveLength(0);
    expect(
      screen.queryByTestId("api-gateway-model-list-table"),
    ).not.toBeInTheDocument();
  });
});

const MODEL_LIST_I18N_KEYS: ReadonlyArray<
  readonly [key: string, en: string, zh: string]
> = [
  ["apiGatewayModelListTab", "Model list", "模型列表"],
  ["apiGatewayModelListSearch", "Search models", "搜索模型"],
  [
    "apiGatewayModelListSearchPlaceholder",
    "Search by model ID or name",
    "按模型 ID 或名称搜索",
  ],
  ["apiGatewayModelListIdColumn", "Model ID", "模型 ID"],
  ["apiGatewayModelListNameColumn", "Model name", "模型名称"],
  ["apiGatewayModelListUpstreamColumn", "Upstream models", "上游模型"],
  [
    "apiGatewayModelListEmpty",
    "No local models are served by enabled upstream providers yet.",
    "还没有已启用上游服务商提供的本地模型。",
  ],
  [
    "apiGatewayModelListNoMatch",
    "No models match the current search.",
    "没有匹配当前搜索的模型。",
  ],
];

describe("模型列表国际化键", () => {
  it("en 与 zh 都定义了 8 个键且文案与契约精确一致", async () => {
    for (const [key, en, zh] of MODEL_LIST_I18N_KEYS) {
      await i18n.changeLanguage("en");
      expect(i18n.t(key), `en:${key}`).toBe(en);

      await i18n.changeLanguage("zh");
      expect(i18n.t(key), `zh:${key}`).toBe(zh);
    }
    await i18n.changeLanguage("en");
  });
});
