import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { ProviderDetailDialog } from "@/components/ApiGateway/ProviderDetailDialog";
import {
  API_GATEWAY_KEY_MASK,
  type GatewayProviderTemplateView,
  type GatewayUpstreamProvider,
  type ModelPrice,
} from "@/lib/apiGateway";
import { renderWithProviders } from "@/test/mocks/render";

function makeProvider(
  overrides: Partial<GatewayUpstreamProvider> = {},
): GatewayUpstreamProvider {
  return {
    id: "p1",
    name: "Upstream A",
    base_url: "https://api.a.example",
    api_key: API_GATEWAY_KEY_MASK,
    default_model: null,
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

function renderProviderDialog({
  provider,
  prices,
  onSave = vi.fn(),
}: {
  provider: GatewayUpstreamProvider;
  prices?: ModelPrice[];
  onSave?: ReturnType<typeof vi.fn>;
}) {
  const view = renderWithProviders(
    <ProviderDetailDialog
      open
      provider={provider}
      prices={prices}
      busy={false}
      onSave={onSave}
      onDelete={vi.fn()}
      onOpenChange={vi.fn()}
    />,
  );
  return { ...view, onSave };
}

function valueOf(testId: string): string {
  return (screen.getByTestId(testId) as HTMLInputElement).value;
}

describe("ProviderDetailDialog 模型映射", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("映射行可编辑本地模型名称并随保存提交", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const onOpenChange = vi.fn();
    const provider = makeProvider({
      mappings: [
        {
          local_model: "gpt-4o",
          upstream_model: "gpt-4o-2024",
          display_name: "GPT-4o",
          protocol: null,
        },
      ],
    });

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={onSave}
        onDelete={vi.fn()}
        onOpenChange={onOpenChange}
      />,
    );

    const displayNameInput = screen.getByRole("textbox", {
      name: "Local model name 1",
    });
    expect(displayNameInput).toHaveValue("GPT-4o");

    await user.clear(displayNameInput);
    await user.type(displayNameInput, "GPT-4o Custom");
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledTimes(1);
    const saved = onSave.mock.calls[0][0] as GatewayUpstreamProvider;
    expect(saved.mappings[0]).toMatchObject({
      local_model: "gpt-4o",
      upstream_model: "gpt-4o-2024",
      display_name: "GPT-4o Custom",
    });
  });

  it("清空本地模型名称时保存为 undefined 且不改动其他映射字段", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const provider = makeProvider({
      mappings: [
        {
          local_model: "gpt-4o",
          upstream_model: "gpt-4o-2024",
          display_name: "GPT-4o",
          protocol: null,
        },
      ],
    });

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={onSave}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );

    const displayNameInput = screen.getByRole("textbox", {
      name: "Local model name 1",
    });
    await user.clear(displayNameInput);
    expect(displayNameInput).toHaveValue("");

    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledTimes(1);
    const saved = onSave.mock.calls[0][0] as GatewayUpstreamProvider;
    expect(saved.mappings[0].local_model).toBe("gpt-4o");
    expect(saved.mappings[0].upstream_model).toBe("gpt-4o-2024");
    expect(saved.mappings[0].display_name).toBeUndefined();
  });

  it("新增映射行包含空的本地模型名称输入", async () => {
    const user = userEvent.setup();
    const provider = makeProvider({ mappings: [] });

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("button", { name: /Add mapping/ }));

    const displayNameInput = screen.getByRole("textbox", {
      name: "Local model name 1",
    });
    expect(displayNameInput).toHaveValue("");
  });

  it("provider_dialog_round_trips_mapping_prices_after_reopen", async () => {
    const user = userEvent.setup();
    const provider = makeProvider({
      mappings: [
        { local_model: "local-a", upstream_model: "remote-a" },
        { local_model: "local-b", upstream_model: "remote-b" },
      ],
    });
    const storedZero: ModelPrice = {
      provider_id: "p1",
      upstream_model: "remote-b",
      input: 0,
      cache_read: 0,
      cache_write: 0,
      output: 0,
    };
    const { onSave, unmount } = renderProviderDialog({
      provider,
      prices: [storedZero],
    });

    expect(
      valueOf("api-gateway-price-1-input"),
      "显式存储 0 应回显为字符串 0 而不是空白",
    ).toBe("0");

    await user.type(screen.getByTestId("api-gateway-price-0-input"), "1.5");
    await user.type(screen.getByTestId("api-gateway-price-0-output"), "3");
    await user.click(screen.getByTestId("api-gateway-price-0-off-peak"));
    const start = screen.getByTestId("api-gateway-off-peak-0-start-0");
    await user.clear(start);
    await user.type(start, "23:00");
    const end = screen.getByTestId("api-gateway-off-peak-0-end-0");
    await user.clear(end);
    await user.type(end, "07:00");

    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledTimes(1);
    const [savedProvider, savedPrices] = onSave.mock.calls[0] as [
      GatewayUpstreamProvider,
      ModelPrice[],
    ];
    expect(savedProvider.mappings).toHaveLength(2);
    expect(savedProvider.mappings[0]).toMatchObject({
      local_model: "local-a",
      upstream_model: "remote-a",
    });

    const remoteA = savedPrices.find((row) => row.upstream_model === "remote-a");
    expect(remoteA).toMatchObject({
      input: 1.5,
      cache_read: 0,
      cache_write: 0,
      output: 3,
    });
    expect(remoteA?.off_peaks).toHaveLength(1);
    expect(remoteA?.off_peaks?.[0]).toMatchObject({
      start_time: "23:00",
      end_time: "07:00",
    });
    expect(remoteA?.off_peak).toMatchObject({
      start_time: "23:00",
      end_time: "07:00",
    });
    const remoteB = savedPrices.find((row) => row.upstream_model === "remote-b");
    expect(remoteB).toMatchObject({
      input: 0,
      cache_read: 0,
      cache_write: 0,
      output: 0,
    });

    unmount();
    renderProviderDialog({ provider, prices: savedPrices });
    expect(valueOf("api-gateway-price-0-input")).toBe("1.5");
    expect(valueOf("api-gateway-price-0-output")).toBe("3");
    expect(valueOf("api-gateway-off-peak-0-start-0")).toBe("23:00");
    expect(valueOf("api-gateway-off-peak-0-end-0")).toBe("07:00");
    expect(valueOf("api-gateway-price-1-input")).toBe("0");
  });

  it("provider_dialog_shared_upstream_model_uses_one_draft", async () => {
    const user = userEvent.setup();
    const provider = makeProvider({
      mappings: [
        { local_model: "local-a", upstream_model: "remote-a" },
        { local_model: "local-a-copy", upstream_model: "remote-a" },
      ],
    });
    const { onSave } = renderProviderDialog({ provider });

    await user.type(screen.getByTestId("api-gateway-price-0-input"), "2");
    expect(
      valueOf("api-gateway-price-1-input"),
      "共享同一上游模型的两行应编辑同一份草稿",
    ).toBe("2");

    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledTimes(1);
    const [, prices] = onSave.mock.calls[0] as [
      GatewayUpstreamProvider,
      ModelPrice[],
    ];
    const remoteRows = prices.filter((row) => row.upstream_model === "remote-a");
    expect(remoteRows).toHaveLength(1);
    expect(remoteRows[0]).toMatchObject({
      input: 2,
      cache_read: 0,
      cache_write: 0,
      output: 0,
    });
  });

  it("provider_dialog_blank_tiers_submit_no_price_rows", async () => {
    const user = userEvent.setup();
    const provider = makeProvider({
      mappings: [{ local_model: "local-a", upstream_model: "remote-a" }],
    });
    const { onSave } = renderProviderDialog({ provider });

    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledTimes(1);
    expect(onSave.mock.calls[0][0]).toMatchObject({ id: "p1" });
    expect(onSave.mock.calls[0][1]).toEqual([]);
  });

  it("provider_dialog_partial_tiers_submit_zeros", async () => {
    const user = userEvent.setup();
    const provider = makeProvider({
      mappings: [{ local_model: "local-a", upstream_model: "remote-a" }],
    });
    const first = renderProviderDialog({ provider });

    await user.type(screen.getByTestId("api-gateway-price-0-output"), "3");
    await user.click(screen.getByRole("button", { name: "Save" }));

    let prices = first.onSave.mock.calls[0][1] as ModelPrice[];
    expect(prices).toHaveLength(1);
    expect(prices[0]).toMatchObject({
      upstream_model: "remote-a",
      input: 0,
      cache_read: 0,
      cache_write: 0,
      output: 3,
    });
    first.unmount();

    const second = renderProviderDialog({ provider });
    for (const testId of [
      "api-gateway-price-0-input",
      "api-gateway-price-0-cache-read",
      "api-gateway-price-0-cache-write",
      "api-gateway-price-0-output",
    ]) {
      await user.type(screen.getByTestId(testId), "0");
    }
    await user.click(screen.getByRole("button", { name: "Save" }));

    prices = second.onSave.mock.calls[0][1] as ModelPrice[];
    expect(prices).toHaveLength(1);
    expect(prices[0]).toMatchObject({
      input: 0,
      cache_read: 0,
      cache_write: 0,
      output: 0,
    });
  });

  it("provider_dialog_writes_off_peak_only_when_enabled_with_windows", async () => {
    const user = userEvent.setup();
    const provider = makeProvider({
      mappings: [{ local_model: "local-a", upstream_model: "remote-a" }],
    });

    const enabled = renderProviderDialog({ provider });
    await user.type(screen.getByTestId("api-gateway-price-0-input"), "1");
    await user.click(screen.getByTestId("api-gateway-price-0-off-peak"));
    await user.click(screen.getByRole("button", { name: "Save" }));
    let prices = enabled.onSave.mock.calls[0][1] as ModelPrice[];
    expect(prices).toHaveLength(1);
    expect(prices[0].off_peaks).toHaveLength(1);
    expect(prices[0].off_peak).toMatchObject({
      start_time: "00:30",
      end_time: "08:30",
    });
    enabled.unmount();

    const disabled = renderProviderDialog({ provider });
    await user.type(screen.getByTestId("api-gateway-price-0-input"), "1");
    await user.click(screen.getByTestId("api-gateway-price-0-off-peak"));
    await user.click(screen.getByTestId("api-gateway-price-0-off-peak"));
    await user.click(screen.getByRole("button", { name: "Save" }));
    prices = disabled.onSave.mock.calls[0][1] as ModelPrice[];
    expect(prices).toHaveLength(1);
    expect(prices[0]).not.toHaveProperty("off_peaks");
    expect(prices[0]).not.toHaveProperty("off_peak");
    disabled.unmount();

    const noWindows = renderProviderDialog({ provider });
    await user.type(screen.getByTestId("api-gateway-price-0-input"), "1");
    await user.click(screen.getByTestId("api-gateway-price-0-off-peak"));
    await user.click(screen.getByTestId("api-gateway-off-peak-0-remove-0"));
    await user.click(screen.getByRole("button", { name: "Save" }));
    prices = noWindows.onSave.mock.calls[0][1] as ModelPrice[];
    expect(prices).toHaveLength(1);
    expect(prices[0]).not.toHaveProperty("off_peaks");
    expect(prices[0]).not.toHaveProperty("off_peak");
  });

  it("provider_dialog_default_model_lists_mapped_models_once", async () => {
    const user = userEvent.setup();
    const provider = makeProvider({
      mappings: [
        { local_model: "a1", upstream_model: "A" },
        { local_model: "b1", upstream_model: "B" },
        { local_model: "a2", upstream_model: "A" },
      ],
    });
    const { onSave } = renderProviderDialog({ provider });

    const select = screen.getByTestId("api-gateway-default-model-select");
    expect(select).toHaveAttribute(
      "aria-label",
      i18n.t("apiGatewayDefaultModel"),
    );
    const options = within(select).getAllByRole("option");
    const values = options.map(
      (option) => (option as HTMLOptionElement).value,
    );
    expect(values.filter((value) => value !== "")).toEqual(["A", "B"]);
    const emptyOption = options.find(
      (option) => (option as HTMLOptionElement).value === "",
    );
    expect(emptyOption).toBeDefined();
    expect(emptyOption!).toHaveTextContent(i18n.t("apiGatewayDefaultModelNone"));

    await user.selectOptions(select, "A");
    await user.click(screen.getByRole("button", { name: "Save" }));

    const [savedProvider] = onSave.mock.calls[0] as [
      GatewayUpstreamProvider,
      ModelPrice[],
    ];
    expect(savedProvider.default_model).toBe("A");
  });

  it("provider_dialog_materializes_legacy_default_as_marked_row_with_price", async () => {
    const user = userEvent.setup();
    const provider = makeProvider({ default_model: "X", mappings: [] });
    const prices: ModelPrice[] = [
      {
        provider_id: "p1",
        upstream_model: "X",
        input: 3,
        cache_read: 0,
        cache_write: 0,
        output: 0,
      },
    ];
    const { onSave } = renderProviderDialog({ provider, prices });

    const select = screen.getByTestId(
      "api-gateway-default-model-select",
    ) as HTMLSelectElement;
    expect(select.value).toBe("X");

    const autoRow = document.querySelector('[data-auto-added="true"]');
    expect(autoRow).not.toBeNull();
    const autoRowElement = autoRow as HTMLElement;
    expect(
      within(autoRowElement).getByText(
        i18n.t("apiGatewayDefaultModelAutoAdded"),
      ),
    ).toBeInTheDocument();
    expect(
      valueOf("api-gateway-price-0-input"),
      "自动补出的默认模型映射行应带出该服务商已有的价格行",
    ).toBe("3");

    await user.click(screen.getByRole("button", { name: "Save" }));

    const [savedProvider, savedPrices] = onSave.mock.calls[0] as [
      GatewayUpstreamProvider,
      ModelPrice[],
    ];
    expect(savedProvider.default_model).toBe("X");
    expect(
      savedProvider.mappings.find(
        (mapping) => mapping.upstream_model === "X",
      ),
    ).toMatchObject({
      local_model: "X",
      upstream_model: "X",
      enabled: true,
    });
    expect(
      savedPrices.find((row) => row.upstream_model === "X"),
    ).toMatchObject({ input: 3 });
  });

  it("provider_dialog_removing_last_default_mapping_clears_default_and_row", async () => {
    const user = userEvent.setup();
    const provider = makeProvider({
      default_model: "X",
      mappings: [{ local_model: "X", upstream_model: "X" }],
    });
    const prices: ModelPrice[] = [
      {
        provider_id: "p1",
        upstream_model: "X",
        input: 3,
        cache_read: 0,
        cache_write: 0,
        output: 0,
      },
    ];
    const { onSave } = renderProviderDialog({ provider, prices });

    expect(
      (screen.getByTestId("api-gateway-default-model-select") as HTMLSelectElement)
        .value,
    ).toBe("X");

    await user.click(screen.getByRole("button", { name: "Remove mapping 1" }));

    expect(
      (screen.getByTestId("api-gateway-default-model-select") as HTMLSelectElement)
        .value,
      "移除默认模型最后一个映射后应清空默认选择",
    ).toBe("");

    await user.click(screen.getByRole("button", { name: "Save" }));

    const [savedProvider, savedPrices] = onSave.mock.calls[0] as [
      GatewayUpstreamProvider,
      ModelPrice[],
    ];
    expect(savedProvider.default_model).toBeNull();
    expect(
      savedPrices.find((row) => row.upstream_model === "X"),
    ).toBeUndefined();
  });

  it("provider_dialog_blank_upstream_model_has_no_price_editor", () => {
    const provider = makeProvider({
      mappings: [
        { local_model: "local-a", upstream_model: "   " },
        { local_model: "local-b", upstream_model: "remote-a" },
      ],
    });
    renderProviderDialog({ provider });

    expect(
      screen.queryByTestId("api-gateway-mapping-price-0"),
      "上游模型为空的映射行不应渲染价格编辑器",
    ).not.toBeInTheDocument();
    expect(
      screen.getByTestId("api-gateway-mapping-price-1"),
      "非空上游模型的映射行应渲染价格编辑器",
    ).toBeInTheDocument();
  });

  it("映射行开关反映存储状态、关闭后保存并重开仍为禁用", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const provider = makeProvider({
      mappings: [
        { local_model: "local-a", upstream_model: "remote-a", enabled: true },
        { local_model: "local-b", upstream_model: "remote-b", enabled: false },
      ],
    });

    const { unmount } = renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={onSave}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );

    const enabledSwitch = screen.getByRole("switch", {
      name: "Enable mapping 1",
    });
    const disabledSwitch = screen.getByRole("switch", {
      name: "Enable mapping 2",
    });
    expect(enabledSwitch, "第 1 行启用映射开关应为开启").toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(disabledSwitch, "第 2 行禁用映射开关应为关闭").toHaveAttribute(
      "aria-checked",
      "false",
    );

    await user.click(enabledSwitch);
    expect(enabledSwitch, "点击后第 1 行开关应变为关闭").toHaveAttribute(
      "aria-checked",
      "false",
    );

    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledTimes(1);
    const saved = onSave.mock.calls[0][0] as GatewayUpstreamProvider;
    expect(saved.mappings[0].enabled, "保存载荷第 1 条映射应为禁用").toBe(false);
    expect(saved.mappings[1].enabled, "保存载荷第 2 条映射应为禁用").toBe(false);

    unmount();
    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={saved}
        busy={false}
        onSave={vi.fn()}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );
    expect(
      screen.getByRole("switch", { name: "Enable mapping 1" }),
      "重开后第 1 行开关应保持关闭",
    ).toHaveAttribute("aria-checked", "false");
    expect(
      screen.getByRole("switch", { name: "Enable mapping 2" }),
      "重开后第 2 行开关应保持关闭",
    ).toHaveAttribute("aria-checked", "false");
  });

  it("禁用映射行带 data-disabled 与弱化样式", () => {
    const provider = makeProvider({
      mappings: [
        { local_model: "local-a", upstream_model: "remote-a", enabled: true },
        { local_model: "local-b", upstream_model: "remote-b", enabled: false },
      ],
    });

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={vi.fn()}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );

    const disabledSwitch = screen.getByRole("switch", {
      name: "Enable mapping 2",
    });
    const disabledRow = disabledSwitch.closest("li");
    expect(disabledRow).not.toBeNull();
    expect(
      disabledRow!.getAttribute("data-disabled"),
      "禁用映射行应带 data-disabled=true",
    ).toBe("true");
    expect(
      disabledRow!.className,
      "禁用映射行应带弱化样式 opacity-60",
    ).toContain("opacity-60");

    const enabledSwitch = screen.getByRole("switch", {
      name: "Enable mapping 1",
    });
    const enabledRow = enabledSwitch.closest("li");
    expect(enabledRow).not.toBeNull();
    expect(
      enabledRow!.getAttribute("data-disabled"),
      "启用映射行不应带 data-disabled",
    ).toBeNull();
  });

  it("新增映射行默认启用", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const provider = makeProvider({ mappings: [] });

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={onSave}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("button", { name: /Add mapping/ }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledTimes(1);
    const saved = onSave.mock.calls[0][0] as GatewayUpstreamProvider;
    expect(saved.mappings).toHaveLength(1);
    expect(saved.mappings[0].enabled, "新增映射行应默认启用").toBe(true);
  });

  it("缺省 enabled 的既有映射行开关为启用", () => {
    const provider = makeProvider({
      mappings: [{ local_model: "local-a", upstream_model: "remote-a" }],
    });

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={vi.fn()}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );

    expect(
      screen.getByRole("switch", { name: "Enable mapping 1" }),
      "缺省 enabled 的既有映射行开关应视为启用",
    ).toHaveAttribute("aria-checked", "true");
  });
});

function makeTemplateView(
  models: string[],
  id = "t1",
): GatewayProviderTemplateView {
  return {
    template: {
      id,
      name: "OpenCode Zen",
      description: "Curated OpenCode models",
      base_url: "https://opencode.ai/zen/v1",
      protocol: "chat_completions",
      source: "snapshot:models.dev",
      snapshot_version: "2026.09.18",
      models: models.map((upstream_model) => ({
        upstream_model,
        display_name: upstream_model,
        protocol: "chat_completions" as const,
        input: 1,
        cache_read: 0,
        cache_write: 0,
        output: 1,
        off_peaks: [],
        reasoning_efforts: [],
      })),
    },
    synced_at: null,
    source: "snapshot:models.dev",
    from_snapshot: true,
  };
}

describe("ProviderDetailDialog 模板维护与推理档位", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("mappingRowExpandsToEditReasoningEffortsAndSaves", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const provider = makeProvider({
      mappings: [
        {
          local_model: "local-a",
          upstream_model: "remote-a",
          reasoning_efforts: ["low", "medium"],
        },
      ],
    });

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={onSave}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );

    const expand = screen.getByTestId("api-gateway-mapping-expand-0");
    expect(expand).toHaveAttribute("aria-expanded", "false");
    await user.click(expand);
    expect(expand).toHaveAttribute("aria-expanded", "true");

    const panel = screen.getByTestId("api-gateway-mapping-efforts-0");
    expect(
      within(panel).getByTestId("api-gateway-mapping-effort-0-low"),
    ).toBeInTheDocument();
    expect(
      within(panel).getByTestId("api-gateway-mapping-effort-0-medium"),
    ).toBeInTheDocument();

    const effortInput = screen.getByTestId("api-gateway-mapping-effort-input-0");
    await user.type(effortInput, "high");
    await user.keyboard("{Enter}");
    await user.click(
      screen.getByTestId("api-gateway-mapping-effort-remove-0-medium"),
    );

    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledTimes(1);
    const saved = onSave.mock.calls[0][0] as GatewayUpstreamProvider;
    expect(saved.mappings[0].reasoning_efforts).toEqual(["low", "high"]);
  });

  it("reasoningEffortsNormalizeTrimDuplicatesAndEmpty", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const provider = makeProvider({
      mappings: [
        {
          local_model: "local-a",
          upstream_model: "remote-a",
          reasoning_efforts: [],
        },
      ],
    });

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={onSave}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );

    await user.click(screen.getByTestId("api-gateway-mapping-expand-0"));
    const effortInput = screen.getByTestId("api-gateway-mapping-effort-input-0");

    await user.type(effortInput, " high ");
    await user.keyboard("{Enter}");
    await user.clear(effortInput);

    await user.type(effortInput, "high");
    await user.click(screen.getByTestId("api-gateway-mapping-effort-add-0"));
    await user.clear(effortInput);

    await user.type(effortInput, " ");
    await user.keyboard("{Enter}");

    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledTimes(1);
    const saved = onSave.mock.calls[0][0] as GatewayUpstreamProvider;
    expect(saved.mappings[0].reasoning_efforts).toEqual(["high"]);
  });

  it("templateBoundProviderMarksRetiredMappingsDeprecated", () => {
    const provider = makeProvider({
      id: "p1",
      template_id: "t1",
      mappings: [
        { local_model: "local-a", upstream_model: "remote-a" },
        { local_model: "local-b", upstream_model: "remote-b" },
      ],
    });

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={vi.fn()}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
        templates={[makeTemplateView(["remote-a"])]}
      />,
    );

    const deprecatedText = i18n.t("apiGatewayTemplateDeprecated");
    const rowA = screen
      .getByRole("switch", { name: "Enable mapping 1" })
      .closest("li");
    const rowB = screen
      .getByRole("switch", { name: "Enable mapping 2" })
      .closest("li");
    expect(rowA).not.toBeNull();
    expect(rowB).not.toBeNull();

    expect(
      rowA!.getAttribute("data-deprecated"),
      "模板中仍存在的映射行不应标记弃用",
    ).toBeNull();
    expect(
      rowB!.getAttribute("data-deprecated"),
      "模板已移除的映射行应标记弃用",
    ).toBe("true");
    expect(within(rowB!).getByText(deprecatedText)).toBeInTheDocument();
    expect(within(rowA!).queryByText(deprecatedText)).not.toBeInTheDocument();

    // 弃用行仍可启用与编辑
    expect(
      screen.getByRole("switch", { name: "Enable mapping 2" }),
    ).toBeEnabled();
    expect(
      screen.getByRole("textbox", { name: "Upstream model 2" }),
    ).toBeEnabled();
  });

  it("templateBoundProviderShowsIgnoredModelsAndRestores", async () => {
    const user = userEvent.setup();
    const onRestoreModel = vi.fn();
    const provider = makeProvider({
      id: "p1",
      template_id: "t1",
      ignored_models: ["remote-b"],
      mappings: [],
    });

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={vi.fn()}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
        templates={[makeTemplateView(["remote-a"])]}
        onRestoreModel={onRestoreModel}
      />,
    );

    const region = screen.getByTestId("api-gateway-ignored-models");
    expect(
      within(region).getByTestId("api-gateway-ignored-model-remote-b"),
    ).toBeInTheDocument();

    await user.click(screen.getByTestId("api-gateway-restore-model-remote-b"));

    expect(onRestoreModel).toHaveBeenCalledWith("p1", "remote-b");
  });

  it("manualProviderHasNoIgnoredSectionAndKeepsLocalRemove", async () => {
    const user = userEvent.setup();
    const onDeleteModel = vi.fn();
    const onSave = vi.fn();
    const provider = makeProvider({
      id: "p1",
      ignored_models: ["remote-b"],
      mappings: [
        { local_model: "local-a", upstream_model: "remote-a" },
        { local_model: "local-b", upstream_model: "remote-b" },
      ],
    });

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={onSave}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
        onDeleteModel={onDeleteModel}
      />,
    );

    expect(
      screen.queryByTestId("api-gateway-ignored-models"),
      "手动服务商不应展示忽略模型区域",
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Remove mapping 2" }));

    expect(onDeleteModel).not.toHaveBeenCalled();
    expect(onSave).not.toHaveBeenCalled();
    expect(
      screen.queryByRole("switch", { name: "Enable mapping 2" }),
    ).not.toBeInTheDocument();
  });

  it("templateBoundMappingDeleteCallsDeleteModelCallback", async () => {
    const user = userEvent.setup();
    const onDeleteModel = vi.fn();
    const onSave = vi.fn();
    const provider = makeProvider({
      id: "p1",
      template_id: "t1",
      mappings: [
        { local_model: "local-a", upstream_model: "remote-a" },
        { local_model: "local-b", upstream_model: "remote-b" },
      ],
    });

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={onSave}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
        templates={[makeTemplateView(["remote-a", "remote-b"])]}
        onDeleteModel={onDeleteModel}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Remove mapping 2" }));

    expect(onDeleteModel).toHaveBeenCalledWith("p1", "remote-b");
    expect(onSave).not.toHaveBeenCalled();
    expect(
      screen.queryByRole("switch", { name: "Enable mapping 2" }),
    ).not.toBeInTheDocument();
  });
});
