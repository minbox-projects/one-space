import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState, type ComponentProps } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { ProviderDetailDialog } from "@/components/AiGateway/ProviderDetailDialog";
import {
  formatGatewayTimestamp,
  type GatewayProviderTemplateView,
  type GatewayUpstreamProvider,
  type ModelPrice,
} from "@/lib/aiGateway";
import { renderWithProviders } from "@/test/mocks/render";

/**
 * Raw ordered key-pool entry, built as a shape-only record so this suite keeps
 * compiling against the pre-Step-4 `GatewayUpstreamProvider` type while pinning
 * the delivered backend wire contract (field order and camel/snake naming).
 */
type RawProviderKey = Record<string, unknown>;

function providerKey(overrides: RawProviderKey = {}): RawProviderKey {
  return {
    id: "k1",
    name: "Default",
    value: "sk-default",
    enabled: true,
    auto_marked: false,
    failure_kind: null,
    marked_at: null,
    reason: null,
    ...overrides,
  };
}

function makeProvider(
  overrides: Partial<GatewayUpstreamProvider> & { keys?: RawProviderKey[] } = {},
): GatewayUpstreamProvider {
  const { keys, ...rest } = overrides;
  return {
    id: "p1",
    name: "Upstream A",
    base_url: "https://api.a.example",
    keys: keys ?? [providerKey()],
    default_model: null,
    protocol: "chat_completions",
    mappings: [],
    enabled: true,
    ...rest,
  } as unknown as GatewayUpstreamProvider;
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

  it("mappingRowStartsWithDetailsCollapsedAndArrowTogglesPriceAndEfforts", async () => {
    const user = userEvent.setup();
    const provider = makeProvider({
      mappings: [{ local_model: "local-a", upstream_model: "remote-a" }],
    });
    const prices: ModelPrice[] = [
      {
        provider_id: "p1",
        upstream_model: "remote-a",
        input: 1.5,
        cache_read: 0,
        cache_write: 0,
        output: 0,
      },
    ];
    renderProviderDialog({ provider, prices });

    const expand = screen.getByTestId("ai-gateway-mapping-expand-0");
    expect(
      expand,
      "默认收起时展开箭头应为 aria-expanded=false",
    ).toHaveAttribute("aria-expanded", "false");
    expect(
      screen.queryByTestId("ai-gateway-mapping-price-0"),
      "默认收起时不渲染计价配置",
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("ai-gateway-mapping-efforts-0"),
      "默认收起时不渲染推理档位面板",
    ).not.toBeInTheDocument();

    await user.click(expand);
    expect(
      expand,
      "点击展开后箭头应为 aria-expanded=true",
    ).toHaveAttribute("aria-expanded", "true");
    expect(
      screen.getByTestId("ai-gateway-mapping-price-0"),
      "展开后应渲染计价配置",
    ).toBeInTheDocument();
    expect(
      screen.getByTestId("ai-gateway-mapping-efforts-0"),
      "展开后应渲染推理档位面板",
    ).toBeInTheDocument();
    expect(
      valueOf("ai-gateway-price-0-input"),
      "展开后应回显已存价格",
    ).toBe("1.5");

    await user.click(expand);
    expect(
      expand,
      "再次点击后箭头应回到 aria-expanded=false",
    ).toHaveAttribute("aria-expanded", "false");
    expect(
      screen.queryByTestId("ai-gateway-mapping-price-0"),
      "再次点击应收起计价配置",
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("ai-gateway-mapping-efforts-0"),
      "再次点击应收起推理档位面板",
    ).not.toBeInTheDocument();
  });

  it("saveWithoutExpandingPersistsStoredMappingPrice", async () => {
    const user = userEvent.setup();
    const provider = makeProvider({
      mappings: [{ local_model: "local-a", upstream_model: "remote-a" }],
    });
    const prices: ModelPrice[] = [
      {
        provider_id: "p1",
        upstream_model: "remote-a",
        input: 1.5,
        cache_read: 0,
        cache_write: 0,
        output: 0,
      },
    ];
    const { onSave } = renderProviderDialog({ provider, prices });

    expect(
      screen.queryByTestId("ai-gateway-mapping-price-0"),
      "未展开时不应渲染计价配置",
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledTimes(1);
    const [, savedPrices] = onSave.mock.calls[0] as [
      GatewayUpstreamProvider,
      ModelPrice[],
    ];
    expect(
      savedPrices.find((row) => row.upstream_model === "remote-a"),
      "未展开直接保存仍应提交已存价格行",
    ).toMatchObject({ upstream_model: "remote-a", input: 1.5 });
  });

  it("mappingExpandButtonExposesMappingDetailsAccessibleName", () => {
    const provider = makeProvider({
      mappings: [{ local_model: "local-a", upstream_model: "remote-a" }],
    });
    renderProviderDialog({ provider });

    expect(
      screen.getByTestId("ai-gateway-mapping-expand-0"),
      "展开箭头可访问名称应为 Mapping details",
    ).toHaveAccessibleName("Mapping details");
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

    await user.click(screen.getByTestId("ai-gateway-mapping-expand-1"));
    expect(
      valueOf("ai-gateway-price-1-input"),
      "显式存储 0 应回显为字符串 0 而不是空白",
    ).toBe("0");

    await user.click(screen.getByTestId("ai-gateway-mapping-expand-0"));
    await user.type(screen.getByTestId("ai-gateway-price-0-input"), "1.5");
    await user.type(screen.getByTestId("ai-gateway-price-0-output"), "3");
    await user.click(screen.getByTestId("ai-gateway-price-0-off-peak"));
    const start = screen.getByTestId("ai-gateway-off-peak-0-start-0");
    await user.clear(start);
    await user.type(start, "23:00");
    const end = screen.getByTestId("ai-gateway-off-peak-0-end-0");
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
    const remoteB = savedPrices.find((row) => row.upstream_model === "remote-b");
    expect(remoteB).toMatchObject({
      input: 0,
      cache_read: 0,
      cache_write: 0,
      output: 0,
    });

    unmount();
    renderProviderDialog({ provider, prices: savedPrices });
    await user.click(screen.getByTestId("ai-gateway-mapping-expand-0"));
    await user.click(screen.getByTestId("ai-gateway-mapping-expand-1"));
    expect(valueOf("ai-gateway-price-0-input")).toBe("1.5");
    expect(valueOf("ai-gateway-price-0-output")).toBe("3");
    expect(valueOf("ai-gateway-off-peak-0-start-0")).toBe("23:00");
    expect(valueOf("ai-gateway-off-peak-0-end-0")).toBe("07:00");
    expect(valueOf("ai-gateway-price-1-input")).toBe("0");
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

    await user.click(screen.getByTestId("ai-gateway-mapping-expand-0"));
    await user.click(screen.getByTestId("ai-gateway-mapping-expand-1"));
    await user.type(screen.getByTestId("ai-gateway-price-0-input"), "2");
    expect(
      valueOf("ai-gateway-price-1-input"),
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

    await user.click(screen.getByTestId("ai-gateway-mapping-expand-0"));
    await user.type(screen.getByTestId("ai-gateway-price-0-output"), "3");
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
    await user.click(screen.getByTestId("ai-gateway-mapping-expand-0"));
    for (const testId of [
      "ai-gateway-price-0-input",
      "ai-gateway-price-0-cache-read",
      "ai-gateway-price-0-cache-write",
      "ai-gateway-price-0-output",
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
    await user.click(screen.getByTestId("ai-gateway-mapping-expand-0"));
    await user.type(screen.getByTestId("ai-gateway-price-0-input"), "1");
    await user.click(screen.getByTestId("ai-gateway-price-0-off-peak"));
    await user.click(screen.getByRole("button", { name: "Save" }));
    let prices = enabled.onSave.mock.calls[0][1] as ModelPrice[];
    expect(prices).toHaveLength(1);
    expect(prices[0].off_peaks).toHaveLength(1);
    enabled.unmount();

    const disabled = renderProviderDialog({ provider });
    await user.click(screen.getByTestId("ai-gateway-mapping-expand-0"));
    await user.type(screen.getByTestId("ai-gateway-price-0-input"), "1");
    await user.click(screen.getByTestId("ai-gateway-price-0-off-peak"));
    await user.click(screen.getByTestId("ai-gateway-price-0-off-peak"));
    await user.click(screen.getByRole("button", { name: "Save" }));
    prices = disabled.onSave.mock.calls[0][1] as ModelPrice[];
    expect(prices).toHaveLength(1);
    expect(prices[0]).not.toHaveProperty("off_peaks");
    expect(prices[0]).not.toHaveProperty("off_peak");
    disabled.unmount();

    const noWindows = renderProviderDialog({ provider });
    await user.click(screen.getByTestId("ai-gateway-mapping-expand-0"));
    await user.type(screen.getByTestId("ai-gateway-price-0-input"), "1");
    await user.click(screen.getByTestId("ai-gateway-price-0-off-peak"));
    await user.click(screen.getByTestId("ai-gateway-off-peak-0-remove-0"));
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

    const select = screen.getByTestId("ai-gateway-default-model-select");
    expect(select).toHaveAttribute(
      "aria-label",
      i18n.t("aiGatewayDefaultModel"),
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
    expect(emptyOption!).toHaveTextContent(i18n.t("aiGatewayDefaultModelNone"));

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
      "ai-gateway-default-model-select",
    ) as HTMLSelectElement;
    expect(select.value).toBe("X");

    const autoRow = document.querySelector('[data-auto-added="true"]');
    expect(autoRow).not.toBeNull();
    const autoRowElement = autoRow as HTMLElement;
    expect(
      within(autoRowElement).getByText(
        i18n.t("aiGatewayDefaultModelAutoAdded"),
      ),
    ).toBeInTheDocument();
    await user.click(screen.getByTestId("ai-gateway-mapping-expand-0"));
    expect(
      valueOf("ai-gateway-price-0-input"),
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
      (screen.getByTestId("ai-gateway-default-model-select") as HTMLSelectElement)
        .value,
    ).toBe("X");

    await user.click(screen.getByRole("button", { name: "Remove mapping 1" }));

    expect(
      (screen.getByTestId("ai-gateway-default-model-select") as HTMLSelectElement)
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

  it("provider_dialog_blank_upstream_model_has_no_price_editor", async () => {
    const user = userEvent.setup();
    const provider = makeProvider({
      mappings: [
        { local_model: "local-a", upstream_model: "   " },
        { local_model: "local-b", upstream_model: "remote-a" },
      ],
    });
    renderProviderDialog({ provider });

    await user.click(screen.getByTestId("ai-gateway-mapping-expand-0"));
    await user.click(screen.getByTestId("ai-gateway-mapping-expand-1"));

    expect(
      screen.queryByTestId("ai-gateway-mapping-price-0"),
      "上游模型为空的映射行不应渲染价格编辑器",
    ).not.toBeInTheDocument();
    expect(
      screen.getByTestId("ai-gateway-mapping-price-1"),
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
      source: "https://opencode.ai/zen/v1/models",
      models: models.map((upstream_model) => ({
        upstream_model,
        display_name: upstream_model,
        protocol: "chat_completions" as const,
        enabled: true,
      })),
    },
    synced_at: null,
    source: "https://opencode.ai/zen/v1/models",
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

    const expand = screen.getByTestId("ai-gateway-mapping-expand-0");
    expect(expand).toHaveAttribute("aria-expanded", "false");
    await user.click(expand);
    expect(expand).toHaveAttribute("aria-expanded", "true");

    const panel = screen.getByTestId("ai-gateway-mapping-efforts-0");
    expect(
      within(panel).getByTestId("ai-gateway-mapping-effort-0-low"),
    ).toBeInTheDocument();
    expect(
      within(panel).getByTestId("ai-gateway-mapping-effort-0-medium"),
    ).toBeInTheDocument();

    const effortInput = screen.getByTestId("ai-gateway-mapping-effort-input-0");
    await user.type(effortInput, "high");
    await user.keyboard("{Enter}");
    await user.click(
      screen.getByTestId("ai-gateway-mapping-effort-remove-0-medium"),
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

    await user.click(screen.getByTestId("ai-gateway-mapping-expand-0"));
    const effortInput = screen.getByTestId("ai-gateway-mapping-effort-input-0");

    await user.type(effortInput, " high ");
    await user.keyboard("{Enter}");
    await user.clear(effortInput);

    await user.type(effortInput, "high");
    await user.click(screen.getByTestId("ai-gateway-mapping-effort-add-0"));
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

    const deprecatedText = i18n.t("aiGatewayTemplateDeprecated");
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

    const region = screen.getByTestId("ai-gateway-ignored-models");
    expect(
      within(region).getByTestId("ai-gateway-ignored-model-remote-b"),
    ).toBeInTheDocument();

    await user.click(screen.getByTestId("ai-gateway-restore-model-remote-b"));

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
      screen.queryByTestId("ai-gateway-ignored-models"),
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

  it("templateBoundProviderRendersAssociatedTemplateBannerAndTitleBadge", () => {
    const provider = makeProvider({
      id: "p1",
      template_id: "t1",
      mappings: [
        { local_model: "local-a", upstream_model: "remote-a" },
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
        templates={[makeTemplateView(["remote-a", "remote-b"])]}
      />,
    );

    // 1. 表单顶部渲染关联服务商模板横幅卡片
    const banner = screen.getByTestId("ai-gateway-bound-template-banner");
    expect(banner).toBeInTheDocument();
    expect(within(banner).getByText("OpenCode Zen")).toBeInTheDocument();
    expect(within(banner).getByTestId("provider-icon-opencode")).toBeInTheDocument();
    expect(within(banner).getByText("2 preset models")).toBeInTheDocument();

    // 完整显示模板 API 地址，不被截断
    const apiUrlEl = within(banner).getByText("https://opencode.ai/zen/v1");
    expect(apiUrlEl).toBeInTheDocument();
    expect(apiUrlEl.className).not.toContain("truncate");

    // 默认展示尚未同步文案
    expect(within(banner).getByText(/Not synced yet/)).toBeInTheDocument();

    // 2. 弹窗顶部标题徽章展示具体模板名称
    expect(screen.getByText("Template: OpenCode Zen")).toBeInTheDocument();

    // 3. 模型映射列表标题展示已配置模型数量徽章
    const countBadge = screen.getByTestId("ai-gateway-mappings-count-badge");
    expect(countBadge).toHaveTextContent("1 configured");
  });

  it("templateBoundProviderShowsFormattedLastSyncTimeWhenSynced", () => {
    const provider = makeProvider({
      id: "p1",
      template_id: "t1",
      mappings: [],
    });
    const templateView = makeTemplateView(["remote-a"]);
    templateView.synced_at = 1700000000;

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={vi.fn()}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
        templates={[templateView]}
      />,
    );

    const banner = screen.getByTestId("ai-gateway-bound-template-banner");
    // 包含时间戳转换后的格式化时间文本
    expect(within(banner).getByText(/2023-11-15|2023\/11\/15/)).toBeInTheDocument();
  });

  it("mappingsCountBadgeUpdatesWhenAddingOrRemovingMappings", async () => {
    const user = userEvent.setup();
    const provider = makeProvider({
      id: "p1",
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
      />,
    );

    const countBadge = screen.getByTestId("ai-gateway-mappings-count-badge");
    expect(countBadge).toHaveTextContent("2 configured");

    // 点击添加映射
    await user.click(screen.getByRole("button", { name: "Add mapping" }));
    expect(countBadge).toHaveTextContent("3 configured");

    // 删除第 1 条映射
    await user.click(screen.getByRole("button", { name: "Remove mapping 1" }));
    expect(countBadge).toHaveTextContent("2 configured");
  });

  it("manualProviderDoesNotRenderAssociatedTemplateBanner", () => {
    const provider = makeProvider({
      id: "p-manual",
      template_id: null,
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
      />,
    );

    expect(
      screen.queryByTestId("ai-gateway-bound-template-banner"),
    ).not.toBeInTheDocument();
    expect(screen.queryByText(/Template:/)).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Step 3: per-model row-level auto-disable UI — data-auto-disabled, re-enable
// ---------------------------------------------------------------------------

describe("ProviderDetailDialog 逐行 auto-disabled 与重新启用", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("auto_disabled 映射行带 data-auto-disabled 且不与 data-disabled 冲突", () => {
    const provider = makeProvider({
      mappings: [
        { local_model: "healthy", upstream_model: "ra", enabled: true },
        {
          local_model: "user-disabled",
          upstream_model: "rb",
          enabled: false,
        },
        {
          local_model: "auto-disabled",
          upstream_model: "rc",
          enabled: true,
          auto_disabled: true,
        },
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

    // User-disabled 行：data-disabled=true，无 data-auto-disabled
    const userDisabledSwitch = screen.getByRole("switch", {
      name: "Enable mapping 2",
    });
    const userDisabledRow = userDisabledSwitch.closest("li");
    expect(userDisabledRow!.getAttribute("data-disabled")).toBe("true");
    expect(userDisabledRow!.getAttribute("data-auto-disabled")).toBeNull();

    // Auto-disabled 行：data-auto-disabled=true；data-disabled 可能不存在（实现可省略）
    const autoDisabledSwitch = screen.getByRole("switch", {
      name: "Enable mapping 3",
    });
    const autoDisabledRow = autoDisabledSwitch.closest("li");
    expect(autoDisabledRow!.getAttribute("data-auto-disabled")).toBe("true");
  });

  it("自动禁用行通过 data-auto-disabled 与 re-enable 控制区分用户禁用行", () => {
    const provider = makeProvider({
      mappings: [
        { local_model: "ud", upstream_model: "ub", enabled: false },
        { local_model: "ad", upstream_model: "ac", enabled: true, auto_disabled: true },
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

    // User-disabled 行：有 data-disabled，无 data-auto-disabled，无重新启用按钮
    const udSwitch = screen.getByRole("switch", { name: "Enable mapping 1" });
    const udRow = udSwitch.closest("li") as HTMLElement;
    expect(udRow!.getAttribute("data-disabled")).toBe("true");
    expect(udRow!.getAttribute("data-auto-disabled")).toBeNull();
    expect(
      within(udRow!).queryByTestId("ai-gateway-reenable-mapping-ud"),
    ).not.toBeInTheDocument();

    // Auto-disabled 行：有 data-auto-disabled，且有 per-row 重新启用按钮
    const adSwitch = screen.getByRole("switch", { name: "Enable mapping 2" });
    const adRow = adSwitch.closest("li") as HTMLElement;
    expect(adRow!.getAttribute("data-auto-disabled")).toBe("true");
    // 行为层：auto-disabled 行必须包含 per-row re-enable 控件
    expect(
      within(adRow!).getByTestId("ai-gateway-reenable-mapping-ad"),
    ).toBeInTheDocument();
  });

  it("自动禁用映射行的第一列开关按钮显示为关闭，且点击开关可触发重新启用", async () => {
    const user = userEvent.setup();
    const onReenableModel = vi.fn();
    const provider = makeProvider({
      mappings: [
        { local_model: "healthy", upstream_model: "rh", enabled: true },
        { local_model: "auto-off", upstream_model: "ra", enabled: true, auto_disabled: true },
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
        onReenableModel={onReenableModel}
      />,
    );

    const healthySwitch = screen.getByRole("switch", { name: "Enable mapping 1" });
    const autoDisabledSwitch = screen.getByRole("switch", { name: "Enable mapping 2" });

    // 状态层断言：健康行开关开启，自动禁用行开关关闭
    expect(healthySwitch).toHaveAttribute("aria-checked", "true");
    expect(autoDisabledSwitch).toHaveAttribute("aria-checked", "false");

    // 行为层断言：点击自动禁用行的开关，应触发 onReenableModel，并且开关状态变为开启
    await user.click(autoDisabledSwitch);
    expect(onReenableModel).toHaveBeenCalledTimes(1);
    expect(onReenableModel).toHaveBeenCalledWith("p1", "auto-off", "ra");
    expect(autoDisabledSwitch).toHaveAttribute("aria-checked", "true");
  });

  it("自动禁用映射的重新启用按钮调用 onReenableModel 而不触发 onSave", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const onReenableModel = vi.fn();
    const provider = makeProvider({
      mappings: [
        {
          local_model: "gpt-4o",
          upstream_model: "gpt-4o-2024",
          enabled: true,
          auto_disabled: true,
        },
      ],
    });

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={onSave}
        onReenableModel={onReenableModel}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );

    const reenableBtn = screen.getByTestId(
      "ai-gateway-reenable-mapping-gpt-4o",
    );
    expect(reenableBtn).toBeInTheDocument();
    expect(reenableBtn).toHaveAccessibleName(/Re-enable|重新启用/i);

    await user.click(reenableBtn);
    expect(onReenableModel).toHaveBeenCalledTimes(1);
    expect(onReenableModel).toHaveBeenCalledWith(
      "p1",
      "gpt-4o",
      "gpt-4o-2024",
    );
    // Save-based re-enable must NOT be called: backend preserves stored runtime state
    expect(onSave).not.toHaveBeenCalled();
  });

  it("仅用户禁用的行不展示重新启用按钮", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const provider = makeProvider({
      mappings: [
        { local_model: "ud-only", upstream_model: "rub", enabled: false },
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

    // 用户禁用行不应有 ai-gateway-reenable-mapping-* 按钮
    // （只保留普通的 Enable switch）
    const udSwitch = screen.getByRole("switch", {
      name: "Enable mapping 1",
    });
    expect(udSwitch).toBeEnabled();
    // 不应存在重新启用按钮 testid
    expect(
      screen.queryByTestId("ai-gateway-reenable-mapping-ud-only"),
    ).not.toBeInTheDocument();

    // 点击 switch 应该 toggle
    await user.click(udSwitch);
    expect(udSwitch).toHaveAttribute("aria-checked", "true");
  });

  it("健康映射行不展示任何 auto 相关 badge 或提示", () => {
    const provider = makeProvider({
      mappings: [
        { local_model: "healthy-row", upstream_model: "rh" },
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

    // 不应出现 auto-disabled 或 re-enable 相关内容
    expect(
      screen.queryByTestId("ai-gateway-reenable-mapping-healthy-row"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("ai-gateway-provider-auto-disabled-models-p1"),
    ).not.toBeInTheDocument();
  });

  it("存在自动禁用映射的行触发时重新启用所有映射按钮并调用 onReenableModels", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const onReenableModels = vi.fn();
    const provider = makeProvider({
      mappings: [
        {
          local_model: "gpt-4o",
          upstream_model: "gpt-4o-2024",
          enabled: true,
          auto_disabled: true,
        },
        { local_model: "claude-3", upstream_model: "rc", enabled: true },
      ],
    });

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={onSave}
        onReenableModels={onReenableModels}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );

    const reenableAllBtn = screen.getByTestId(
      "ai-gateway-reenable-models-p1",
    );
    expect(reenableAllBtn).toBeInTheDocument();
    expect(reenableAllBtn).toHaveAccessibleName(/Re-enable all|重新启用所有/i);

    await user.click(reenableAllBtn);
    expect(onReenableModels).toHaveBeenCalledTimes(1);
    expect(onReenableModels).toHaveBeenCalledWith("p1");
    // Save-based re-enable must NOT be called
    expect(onSave).not.toHaveBeenCalled();
  });

  it("无自动禁用行的服务商不展示重新启用所有映射按钮", () => {
    const provider = makeProvider({
      mappings: [
        { local_model: "healthy", upstream_model: "rh", enabled: true },
        { local_model: "ud", upstream_model: "rub", enabled: false },
      ],
    });

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={vi.fn()}
        onReenableModels={vi.fn()}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );

    expect(
      screen.queryByTestId("ai-gateway-reenable-models-p1"),
    ).not.toBeInTheDocument();
  });
});

describe("ProviderDetailDialog 服务商路由权重 (AC-012, AC-013)", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("AC-012: 渲染权重输入框且默认值为 1，限制范围为 1-100", () => {
    const provider = makeProvider();

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

    const weightInput = screen.getByTestId("ai-gateway-provider-weight-input");
    expect(weightInput).toBeInTheDocument();
    expect(weightInput).toHaveValue(1);
    expect(weightInput).toHaveAttribute("min", "1");
    expect(weightInput).toHaveAttribute("max", "100");
  });

  it("AC-012: 若服务商已有 weight 配置，则初始值显示对应权重", () => {
    const provider = makeProvider({
      weight: 10,
    } as any);

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

    const weightInput = screen.getByTestId("ai-gateway-provider-weight-input");
    expect(weightInput).toBeInTheDocument();
    expect(weightInput).toHaveValue(10);
  });

  it("AC-013: 用户将权重修改为 5 并点击保存时，onSave 回调接收到的 provider 对象包含 weight: 5", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const provider = makeProvider();

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

    const weightInput = screen.getByTestId("ai-gateway-provider-weight-input");
    await user.clear(weightInput);
    await user.type(weightInput, "5");

    const saveButton = screen.getByRole("button", { name: "Save" });
    await user.click(saveButton);

    expect(onSave).toHaveBeenCalledTimes(1);
    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        weight: 5,
      }),
      expect.anything(),
    );
  });
});

// ---------------------------------------------------------------------------
// Step 4: runtime-field merge from the live provider snapshot (AC-010, REQ-007)
// ---------------------------------------------------------------------------

describe("ProviderDetailDialog 运行时字段合并", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  function runtimeSnapshot(
    overrides: Partial<GatewayUpstreamProvider> = {},
  ): GatewayUpstreamProvider {
    return makeProvider({
      id: "p1",
      name: "Runtime Name",
      base_url: "https://runtime.example",
      ...overrides,
    });
  }

  // The dialog must stay mounted while `runtimeProvider` changes, so the live
  // snapshot is driven through a harness that owns it as state instead of
  // re-rendering (which would remount the dialog and reset the draft).
  function RuntimeDialogHarness({
    provider,
    onSave,
    onRuntimeReady,
  }: {
    provider: GatewayUpstreamProvider;
    onSave: ReturnType<typeof vi.fn>;
    onRuntimeReady: (setRuntime: (next: GatewayUpstreamProvider) => void) => void;
  }) {
    const [runtimeProvider, setRuntimeProvider] =
      useState<GatewayUpstreamProvider | null>(null);
    onRuntimeReady(setRuntimeProvider);
    return (
      <ProviderDetailDialog
        open
        provider={provider}
        runtimeProvider={runtimeProvider}
        busy={false}
        onSave={onSave}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
        onReenableModel={vi.fn()}
        onReenableModels={vi.fn()}
      />
    );
  }

  function renderRuntimeDialog({
    provider,
    onSave = vi.fn(),
  }: {
    provider: GatewayUpstreamProvider;
    onSave?: ReturnType<typeof vi.fn>;
  }) {
    let setRuntimeProvider: (next: GatewayUpstreamProvider) => void = () => {};
    const view = renderWithProviders(
      <RuntimeDialogHarness
        provider={provider}
        onSave={onSave}
        onRuntimeReady={(setter) => {
          setRuntimeProvider = setter;
        }}
      />,
    );
    return {
      ...view,
      applyRuntime: async (next: GatewayUpstreamProvider) => {
        await act(async () => {
          setRuntimeProvider(next);
        });
      },
    };
  }

  it("runtime_provider_merge_updates_auto_disabled_presentation_without_losing_unsaved_edits", async () => {
    const user = userEvent.setup();
    const provider = makeProvider({
      id: "p1",
      name: "Upstream A",
      base_url: "https://api.a.example",
      mappings: [
        {
          local_model: "gpt-4o",
          upstream_model: "gpt-4o-2024",
          display_name: "GPT-4o",
          enabled: true,
        },
        {
          local_model: "claude-3",
          upstream_model: "claude-3-2024",
          display_name: "Claude 3",
          enabled: true,
        },
      ],
    });

    const { applyRuntime } = renderRuntimeDialog({ provider });

    // 未保存编辑：重命名服务商、修改第 1 条映射的本地模型名。
    await user.clear(screen.getByLabelText("Name"));
    await user.type(screen.getByLabelText("Name"), "Unsaved Name");
    await user.clear(screen.getByLabelText("Local model 1"));
    await user.type(screen.getByLabelText("Local model 1"), "gpt-4o-custom");

    expect(
      (
        screen
          .getByRole("switch", { name: "Enable mapping 2" })
          .closest("li") as HTMLElement
      ).getAttribute("data-auto-disabled"),
    ).toBeNull();

    // 运行时快照：第 2 行与草稿键匹配并被自动禁用；第 1 行键因改名已不匹配，
    // 且两个匹配候选都携带不同的用户可编辑字段以证明字段隔离。
    const runtimeProvider = runtimeSnapshot({
      name: "Runtime Name",
      base_url: "https://runtime.example",
      mappings: [
        {
          local_model: "gpt-4o",
          upstream_model: "gpt-4o-2024",
          display_name: "Runtime Display",
          enabled: true,
          auto_disabled: true,
          disabled_reason: "HTTP 500",
          consecutive_failures: 3,
        },
        {
          local_model: "claude-3",
          upstream_model: "claude-3-2024",
          display_name: "Runtime Claude",
          enabled: true,
          auto_disabled: true,
          disabled_reason: "HTTP 500",
          disabled_at: 1_700_000_000,
          consecutive_failures: 3,
          last_error_at: 1_700_000_100,
        },
      ],
    });

    await applyRuntime(runtimeProvider);

    // 匹配行的 auto-disabled 呈现更新：标记与行内重新启用控件。
    const row2 = screen
      .getByRole("switch", { name: "Enable mapping 2" })
      .closest("li") as HTMLElement;
    expect(row2.getAttribute("data-auto-disabled")).toBe("true");
    expect(
      within(row2).getByTestId("ai-gateway-reenable-mapping-claude-3"),
    ).toBeInTheDocument();

    // 未保存编辑保留，运行时快照的非运行时字段没有覆盖用户字段。
    expect(screen.getByLabelText("Name")).toHaveValue("Unsaved Name");
    expect(screen.getByLabelText("API base URL")).toHaveValue(
      "https://api.a.example",
    );
    expect(screen.getByLabelText("Local model 1")).toHaveValue("gpt-4o-custom");
    expect(screen.getByLabelText("Local model name 1")).toHaveValue("GPT-4o");
    expect(screen.getByLabelText("Local model name 2")).toHaveValue("Claude 3");

    // 键不匹配的第 1 行不得被自动禁用。
    const row1 = screen
      .getByRole("switch", { name: "Enable mapping 1" })
      .closest("li") as HTMLElement;
    expect(row1.getAttribute("data-auto-disabled")).toBeNull();
    expect(
      within(row1).queryByTestId("ai-gateway-reenable-mapping-gpt-4o-custom"),
    ).not.toBeInTheDocument();
  });

  it("runtime_provider_merge_drives_the_auto_disabled_hint_from_the_draft", async () => {
    const provider = makeProvider({
      id: "p1",
      name: "Upstream A",
      mappings: [
        {
          local_model: "gpt-4o",
          upstream_model: "gpt-4o-2024",
          enabled: true,
        },
        {
          local_model: "claude-3",
          upstream_model: "claude-3-2024",
          enabled: true,
        },
      ],
    });

    const { applyRuntime } = renderRuntimeDialog({ provider });

    // 干净快照：草稿没有自动禁用行，因此没有批量重新启用控件。
    expect(
      screen.queryByTestId("ai-gateway-reenable-models-p1"),
    ).not.toBeInTheDocument();

    const runtimeProvider = runtimeSnapshot({
      mappings: [
        {
          local_model: "gpt-4o",
          upstream_model: "gpt-4o-2024",
          enabled: true,
          auto_disabled: true,
          consecutive_failures: 3,
        },
        {
          local_model: "claude-3",
          upstream_model: "claude-3-2024",
          enabled: true,
        },
      ],
    });

    await applyRuntime(runtimeProvider);

    // provider 快照本身仍是健康的；只有从合并后的草稿派生才会出现该控件。
    const reenableAll = screen.getByTestId("ai-gateway-reenable-models-p1");
    expect(reenableAll).toBeInTheDocument();
    expect(reenableAll).toHaveAccessibleName(/Re-enable all|重新启用所有/i);

    const row1 = screen
      .getByRole("switch", { name: "Enable mapping 1" })
      .closest("li") as HTMLElement;
    expect(row1.getAttribute("data-auto-disabled")).toBe("true");
    expect(
      within(row1).getByTestId("ai-gateway-reenable-mapping-gpt-4o"),
    ).toBeInTheDocument();
  });

  it("runtime_provider_does_not_reset_the_draft_when_unrelated_fields_change", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const provider = makeProvider({
      id: "p1",
      name: "Upstream A",
      base_url: "https://api.a.example",
      mappings: [
        {
          local_model: "gpt-4o",
          upstream_model: "gpt-4o-2024",
          display_name: "GPT-4o",
          enabled: true,
        },
        {
          local_model: "claude-3",
          upstream_model: "claude-3-2024",
          display_name: "Claude 3",
          enabled: true,
        },
      ],
    });

    const { applyRuntime } = renderRuntimeDialog({ provider, onSave });

    await user.clear(screen.getByLabelText("Name"));
    await user.type(screen.getByLabelText("Name"), "Unsaved Name");
    await user.clear(screen.getByLabelText("Local model name 1"));
    await user.type(screen.getByLabelText("Local model name 1"), "Unsaved Display");

    // 运行时快照只改非运行时字段，另含一条草稿中不存在的键：
    // 按索引合并（而非按键）会污染草稿。
    const runtimeProvider = runtimeSnapshot({
      name: "Runtime Name",
      base_url: "https://runtime.example",
      mappings: [
        {
          local_model: "gpt-4o",
          upstream_model: "gpt-4o-2024",
          display_name: "Runtime Display",
          enabled: true,
        },
        {
          local_model: "no-match",
          upstream_model: "no-match",
          enabled: true,
          auto_disabled: true,
          consecutive_failures: 3,
        },
      ],
    });

    await applyRuntime(runtimeProvider);

    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledTimes(1);
    const [saved] = onSave.mock.calls[0] as [GatewayUpstreamProvider, ModelPrice[]];
    expect(saved.name).toBe("Unsaved Name");
    expect(saved.base_url).toBe("https://api.a.example");
    expect(saved.mappings).toHaveLength(2);
    expect(saved.mappings[0]).toMatchObject({
      local_model: "gpt-4o",
      upstream_model: "gpt-4o-2024",
      display_name: "Unsaved Display",
    });
    expect(saved.mappings[0].auto_disabled).toBeFalsy();
    expect(saved.mappings[1].auto_disabled).toBeFalsy();
    expect(saved.mappings.some((m) => m.local_model === "no-match")).toBe(false);
  });

  it("runtime_provider_refresh_updates_key_marks_without_replacing_unsaved_key_drafts", async () => {
    const user = userEvent.setup();
    const provider = makeProvider({
      id: "p1",
      keys: [
        providerKey({ id: "k1", name: "Primary", value: "sk-one", enabled: true }),
        providerKey({ id: "k2", name: "Backup", value: "sk-two", enabled: true }),
      ],
    });

    const { applyRuntime } = renderRuntimeDialog({ provider });

    // Unsaved edits across user-owned fields: edit key via modal, and fast switches on card.
    await user.click(screen.getByTestId("ai-gateway-key-0"));
    const nameInput = screen.getByTestId("ai-gateway-key-dialog-name");
    await user.clear(nameInput);
    await user.type(nameInput, "Primary Renamed");
    const valInput = screen.getByTestId("ai-gateway-key-dialog-value");
    await user.clear(valInput);
    await user.type(valInput, "sk-renamed");
    await user.click(screen.getByTestId("ai-gateway-key-dialog-daily-reset"));
    await user.click(screen.getByTestId("ai-gateway-key-dialog-confirm"));

    await user.click(screen.getByRole("switch", { name: "Enable key 1" }));

    await user.click(screen.getByTestId("ai-gateway-key-1"));
    await user.click(screen.getByTestId("ai-gateway-key-dialog-daily-reset"));
    await user.click(screen.getByTestId("ai-gateway-key-dialog-confirm"));

    // Event-equivalent snapshot refresh: same ids, runtime marks only.
    await applyRuntime(
      makeProvider({
        id: "p1",
        keys: [
          providerKey({
            id: "k1",
            name: "Runtime Primary",
            value: "sk-runtime-1",
            enabled: true,
            daily_reset: false,
            auto_marked: true,
            failure_kind: "quota",
            marked_at: 1_700_000_000,
          }),
          providerKey({
            id: "k2",
            name: "Runtime Backup",
            value: "sk-runtime-2",
            enabled: false,
            daily_reset: false,
          }),
        ],
      }),
    );

    // Only the matching runtime mark merges in.
    await waitFor(() =>
      expect(screen.getByTestId("ai-gateway-reenable-key-0")).toBeInTheDocument(),
    );
    expect(
      screen.queryByTestId("ai-gateway-reenable-key-1"),
      "未携带运行时标记的第 2 行不应出现重新启用控件",
    ).not.toBeInTheDocument();

    // Unsaved user drafts survive the refresh untouched.
    expect(screen.getByTestId("ai-gateway-key-name-0")).toHaveTextContent("Primary Renamed");
    expect(
      screen.getByTestId("ai-gateway-key-name-1"),
      "第 2 行未保存的名称不应被运行时快照替换",
    ).toHaveTextContent("Backup");
    expect(
      screen.getByRole("switch", { name: "Enable key 1" }),
      "未保存的启用状态应保留",
    ).toHaveAttribute("aria-checked", "false");
    expect(
      screen.getByTestId("ai-gateway-key-daily-reset-badge-0"),
      "未保存的 daily reset 编辑应保留徽标",
    ).toBeInTheDocument();
    expect(
      screen.getByTestId("ai-gateway-key-daily-reset-badge-1"),
    ).toBeInTheDocument();
  });
});

describe("ProviderDetailDialog 批量启用/禁用映射", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("映射为空时不展示批量按钮", () => {
    const provider = makeProvider({ mappings: [] });

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
      screen.queryByTestId("ai-gateway-enable-all-mappings"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("ai-gateway-disable-all-mappings"),
    ).not.toBeInTheDocument();
  });

  it("全部禁用只改 enabled 并保留 auto_disabled，且不触发自动禁用恢复", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const onReenableModel = vi.fn();
    const onReenableModels = vi.fn();
    const provider = makeProvider({
      mappings: [
        { local_model: "a", upstream_model: "ra", enabled: true },
        {
          local_model: "b",
          upstream_model: "rb",
          enabled: true,
          auto_disabled: true,
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
        onReenableModel={onReenableModel}
        onReenableModels={onReenableModels}
      />,
    );

    await user.click(screen.getByTestId("ai-gateway-disable-all-mappings"));

    expect(
      screen.getByRole("switch", { name: "Enable mapping 1" }),
    ).toHaveAttribute("aria-checked", "false");
    expect(
      screen.getByRole("switch", { name: "Enable mapping 2" }),
    ).toHaveAttribute("aria-checked", "false");

    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledTimes(1);
    const saved = onSave.mock.calls[0][0] as GatewayUpstreamProvider;
    expect(saved.mappings.map((mapping) => mapping.enabled)).toEqual([
      false,
      false,
    ]);
    expect(saved.mappings[1].auto_disabled).toBe(true);
    expect(onReenableModel).not.toHaveBeenCalled();
    expect(onReenableModels).not.toHaveBeenCalled();
  });

  it("全部启用只改 enabled 并保留 auto_disabled，且不触发自动禁用恢复", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const onReenableModel = vi.fn();
    const onReenableModels = vi.fn();
    const provider = makeProvider({
      mappings: [
        { local_model: "a", upstream_model: "ra", enabled: false },
        {
          local_model: "b",
          upstream_model: "rb",
          enabled: false,
          auto_disabled: true,
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
        onReenableModel={onReenableModel}
        onReenableModels={onReenableModels}
      />,
    );

    await user.click(screen.getByTestId("ai-gateway-enable-all-mappings"));

    // 用户禁用行恢复为开启；自动禁用行因运行状态仍显示关闭
    expect(
      screen.getByRole("switch", { name: "Enable mapping 1" }),
    ).toHaveAttribute("aria-checked", "true");
    expect(
      screen.getByRole("switch", { name: "Enable mapping 2" }),
    ).toHaveAttribute("aria-checked", "false");

    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledTimes(1);
    const saved = onSave.mock.calls[0][0] as GatewayUpstreamProvider;
    expect(saved.mappings.map((mapping) => mapping.enabled)).toEqual([true, true]);
    expect(saved.mappings[1].auto_disabled).toBe(true);
    expect(onReenableModel).not.toHaveBeenCalled();
    expect(onReenableModels).not.toHaveBeenCalled();
  });

  it("已全启用时全部启用按钮禁用、已全禁用时全部禁用按钮禁用", () => {
    const allEnabled = makeProvider({
      mappings: [
        { local_model: "a", upstream_model: "ra", enabled: true },
        { local_model: "b", upstream_model: "rb", enabled: true },
      ],
    });

    const { unmount } = renderWithProviders(
      <ProviderDetailDialog
        open
        provider={allEnabled}
        busy={false}
        onSave={vi.fn()}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );
    expect(screen.getByTestId("ai-gateway-enable-all-mappings")).toBeDisabled();
    expect(screen.getByTestId("ai-gateway-disable-all-mappings")).toBeEnabled();
    unmount();

    const allDisabled = makeProvider({
      mappings: [
        { local_model: "a", upstream_model: "ra", enabled: false },
        { local_model: "b", upstream_model: "rb", enabled: false },
      ],
    });
    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={allDisabled}
        busy={false}
        onSave={vi.fn()}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );
    expect(screen.getByTestId("ai-gateway-enable-all-mappings")).toBeEnabled();
    expect(screen.getByTestId("ai-gateway-disable-all-mappings")).toBeDisabled();
  });
});

describe("ProviderDetailDialog 标签与自定义图标", () => {
  it("renders existing tags and custom icon and saves them properly", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const provider = makeProvider({
      tags: ["Official", "Code"],
      icon: "openai",
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

    expect(screen.getByTestId("provider-tag-badge-Official")).toBeInTheDocument();
    expect(screen.getByTestId("provider-tag-badge-Code")).toBeInTheDocument();

    const iconTrigger = screen.getByTestId("provider-edit-icon-trigger");
    expect(iconTrigger).toBeInTheDocument();

    const saveBtn = screen.getByRole("button", { name: /Save|保存/i });
    await user.click(saveBtn);

    expect(onSave).toHaveBeenCalledTimes(1);
    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        tags: ["Official", "Code"],
        icon: "openai",
      }),
      expect.any(Array),
    );
  });

  it("supports adding new tags via Enter, toggling suggested tags, and removing tags", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const provider = makeProvider({
      tags: ["Existing"],
    });

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={onSave}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
        availableTags={["Existing", "Fast"]}
      />,
    );

    const tagInput = screen.getByTestId("ai-gateway-provider-tags-input");
    await user.type(tagInput, "CustomTag{enter}");
    expect(screen.getByTestId("provider-tag-badge-CustomTag")).toBeInTheDocument();

    const suggestedFast = screen.getByTestId("suggested-tag-Fast");
    await user.click(suggestedFast);
    expect(screen.getByTestId("provider-tag-badge-Fast")).toBeInTheDocument();

    const removeExisting = screen.getByTestId("remove-tag-Existing");
    await user.click(removeExisting);
    expect(screen.queryByTestId("provider-tag-badge-Existing")).not.toBeInTheDocument();

    const saveBtn = screen.getByRole("button", { name: /Save|保存/i });
    await user.click(saveBtn);

    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        tags: ["CustomTag", "Fast"],
      }),
      expect.any(Array),
    );
  });

  it("allows saving without tags (optional) and inherits template icon by default", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const provider = makeProvider({
      template_id: "opencode-zen",
      tags: [],
      icon: null,
    });

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={onSave}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
        templates={[
          {
            template: {
              id: "opencode-zen",
              name: "OpenCode Zen",
              description: "",
              base_url: "https://opencode.ai/zen/v1",
              protocol: "chat_completions",
              api_key_prefix: "",
              default_model: "zen-1",
              recommended_models: [],
              builtin: true,
              created_at: 1000,
              updated_at: 1000,
              models: [],
              icon: "opencode",
            },
            has_local_upstream: false,
            active_provider_count: 1,
          } as any,
        ]}
      />,
    );

    const iconTrigger = screen.getByTestId("provider-edit-icon-trigger");
    expect(iconTrigger).toHaveTextContent(/Inherit from template|继承自服务商模板/i);

    const saveBtn = screen.getByRole("button", { name: /Save|保存/i });
    await user.click(saveBtn);

    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        tags: [],
        icon: null,
      }),
      expect.any(Array),
    );
  });

  it("renders inferred actual provider icon in dialog header when icon is not explicitly selected", () => {
    const provider = makeProvider({
      id: "p-cmd",
      name: "CommandCode",
      base_url: "https://api.commandcode.ai/provider/v1",
      icon: null,
      template_id: null,
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

    const dialog = screen.getByTestId("ai-gateway-provider-detail");
    expect(within(dialog).getAllByTestId("provider-icon-commandcode").length).toBeGreaterThanOrEqual(1);
  });

  describe("ProviderDetailDialog 删除服务商操作", () => {
    it("点击删除按钮弹出二次确认浮层，点击取消关闭确认层且不调用 onDelete", async () => {
      const onDelete = vi.fn();
      const onOpenChange = vi.fn();
      const provider = makeProvider({
        id: "p1",
        name: "DeepSeek",
        base_url: "https://api.deepseek.com",
      });

      renderWithProviders(
        <ProviderDetailDialog
          open
          provider={provider}
          busy={false}
          onSave={vi.fn()}
          onDelete={onDelete}
          onOpenChange={onOpenChange}
        />,
      );

      const deleteBtn = screen.getByRole("button", { name: i18n.t("aiGatewayDelete") });
      fireEvent.click(deleteBtn);

      const confirmDialog = screen.getByTestId("ai-gateway-delete-provider-confirm-dialog");
      expect(confirmDialog).toBeInTheDocument();
      expect(within(confirmDialog).getByText(i18n.t("aiGatewayDeleteProviderTitle"))).toBeInTheDocument();
      expect(within(confirmDialog).getByText(i18n.t("aiGatewayDeleteProviderConfirm", { name: "DeepSeek" }))).toBeInTheDocument();

      const cancelBtn = screen.getByTestId("ai-gateway-delete-provider-cancel");
      fireEvent.click(cancelBtn);

      expect(screen.queryByTestId("ai-gateway-delete-provider-confirm-dialog")).not.toBeInTheDocument();
      expect(onDelete).not.toHaveBeenCalled();
      expect(onOpenChange).not.toHaveBeenCalled();
    });

    it("在二次确认浮层中确认删除，调用 onDelete 并关闭弹窗", async () => {
      const onDelete = vi.fn().mockResolvedValue(true);
      const onOpenChange = vi.fn();
      const provider = makeProvider({
        id: "p1",
        name: "DeepSeek",
        base_url: "https://api.deepseek.com",
      });

      renderWithProviders(
        <ProviderDetailDialog
          open
          provider={provider}
          busy={false}
          onSave={vi.fn()}
          onDelete={onDelete}
          onOpenChange={onOpenChange}
        />,
      );

      const deleteBtn = screen.getByRole("button", { name: i18n.t("aiGatewayDelete") });
      fireEvent.click(deleteBtn);

      const confirmBtn = screen.getByTestId("ai-gateway-delete-provider-confirm");
      fireEvent.click(confirmBtn);

      await waitFor(() => expect(onDelete).toHaveBeenCalledWith("p1"));
      await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    });
  });
});

// ---------------------------------------------------------------------------
// Step 3: provider-dialog ordered key-pool editor and validation.
// RED tests for the frozen interface contract. Only this test file is touched.
// ---------------------------------------------------------------------------

describe("ProviderDetailDialog 上游密钥池编辑", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  function renderKeyDialog({
    provider,
    onSave = vi.fn(),
    onReenableKey = vi.fn(),
    onOpenChange = vi.fn(),
    busy = false,
  }: {
    provider: GatewayUpstreamProvider;
    onSave?: ReturnType<typeof vi.fn>;
    onReenableKey?: ReturnType<typeof vi.fn>;
    onOpenChange?: ReturnType<typeof vi.fn>;
    busy?: boolean;
  }) {
    const props = {
      open: true,
      provider,
      busy,
      onSave,
      onReenableKey,
      onDelete: vi.fn(),
      onOpenChange,
    } as unknown as ComponentProps<typeof ProviderDetailDialog>;
    const view = renderWithProviders(<ProviderDetailDialog {...props} />);
    return { ...view, onSave, onReenableKey, onOpenChange };
  }

  function savedKeys(onSave: ReturnType<typeof vi.fn>): RawProviderKey[] {
    const saved = onSave.mock.calls[0][0] as unknown as {
      keys: RawProviderKey[];
    };
    return saved.keys;
  }

  it("新增密钥提交空白名称时拒绝保存并给出可操作错误", async () => {
    const user = userEvent.setup();
    const { onSave } = renderKeyDialog({ provider: makeProvider({ keys: [] }) });

    await user.click(screen.getByTestId("ai-gateway-add-key"));
    await user.click(screen.getByTestId("ai-gateway-key-dialog-confirm"));

    expect(onSave).not.toHaveBeenCalled();
    expect(screen.getByTestId("ai-gateway-key-name-error")).toHaveTextContent(
      i18n.t("aiGatewayProviderKeyNameRequired"),
    );
  });

  it("新增密钥提交空白值但名称非空时以密钥值错误拒绝保存", async () => {
    const user = userEvent.setup();
    const { onSave } = renderKeyDialog({ provider: makeProvider({ keys: [] }) });

    await user.click(screen.getByTestId("ai-gateway-add-key"));
    await user.type(screen.getByTestId("ai-gateway-key-dialog-name"), "Backup");
    await user.click(screen.getByTestId("ai-gateway-key-dialog-confirm"));

    expect(onSave).not.toHaveBeenCalled();
    expect(screen.getByTestId("ai-gateway-key-value-error")).toHaveTextContent(
      i18n.t("aiGatewayProviderKeyValueRequired"),
    );
  });

  it("keyRowsExposeIndependentDailyResetControlsAndSavePreservesIdentityAndOrder", async () => {
    const user = userEvent.setup();
    const { onSave } = renderKeyDialog({
      provider: makeProvider({
        keys: [
          providerKey({
            id: "k1",
            name: "Primary",
            value: "sk-one",
            daily_reset: true,
          }),
          // Older record without the field: must load as daily reset off.
          providerKey({ id: "k2", name: "Backup", value: "sk-two" }),
        ],
      }),
    });

    // 既有 daily_reset=true 的密钥卡片展示日重置徽标，未开启的不展示
    expect(
      screen.getByTestId("ai-gateway-key-daily-reset-badge-0"),
      "既有 daily_reset=true 的密钥卡片应显示日重置徽标",
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("ai-gateway-key-daily-reset-badge-1"),
      "缺少 daily_reset 字段的旧记录不应显示日重置徽标",
    ).not.toBeInTheDocument();

    // The former up/down reorder controls must be gone from every row.
    for (const testId of [
      "ai-gateway-key-up-0",
      "ai-gateway-key-down-0",
      "ai-gateway-key-up-1",
      "ai-gateway-key-down-1",
    ]) {
      expect(
        screen.queryByTestId(testId),
        `${testId} 不应再存在于密钥行中`,
      ).not.toBeInTheDocument();
    }

    // Toggling in modal for row 0 (turn off) and row 1 (turn on)
    await user.click(screen.getByTestId("ai-gateway-key-0"));
    const resetSwitch0 = screen.getByTestId("ai-gateway-key-dialog-daily-reset");
    expect(resetSwitch0).toHaveAttribute("aria-checked", "true");
    await user.click(resetSwitch0);
    expect(resetSwitch0).toHaveAttribute("aria-checked", "false");
    await user.click(screen.getByTestId("ai-gateway-key-dialog-confirm"));

    await user.click(screen.getByTestId("ai-gateway-key-1"));
    const resetSwitch1 = screen.getByTestId("ai-gateway-key-dialog-daily-reset");
    expect(resetSwitch1).toHaveAttribute("aria-checked", "false");
    await user.click(resetSwitch1);
    expect(resetSwitch1).toHaveAttribute("aria-checked", "true");
    await user.click(screen.getByTestId("ai-gateway-key-dialog-confirm"));

    expect(screen.queryByTestId("ai-gateway-key-daily-reset-badge-0")).not.toBeInTheDocument();
    expect(screen.getByTestId("ai-gateway-key-daily-reset-badge-1")).toBeInTheDocument();

    // A newly appended row via modal defaults off and stays last.
    await user.click(screen.getByTestId("ai-gateway-add-key"));
    await user.type(screen.getByTestId("ai-gateway-key-dialog-name"), "Third");
    await user.type(screen.getByTestId("ai-gateway-key-dialog-value"), "sk-three");
    expect(
      screen.getByTestId("ai-gateway-key-dialog-daily-reset"),
      "新增密钥弹窗中应默认关闭 daily reset",
    ).toHaveAttribute("aria-checked", "false");
    await user.click(screen.getByTestId("ai-gateway-key-dialog-confirm"));

    expect(
      screen.queryByTestId("ai-gateway-key-daily-reset-badge-2"),
      "新增密钥卡片默认不应显示日重置徽标",
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledTimes(1);
    const keys = savedKeys(onSave);
    // Stable identity and retained relative order; the appended key is last.
    expect(keys.map((entry) => entry.id)).toEqual([
      "k1",
      "k2",
      expect.stringMatching(/^new-key-/),
    ]);
    // Each row's daily-reset setting is independent in the save payload.
    expect(keys.map((entry) => entry.daily_reset)).toEqual([false, true, false]);
  });

  it("provider_dialog_saved_daily_reset_settings_survive_reopen_independently", async () => {
    const user = userEvent.setup();
    const first = renderKeyDialog({
      provider: makeProvider({
        keys: [
          providerKey({ id: "k1", name: "Primary", value: "sk-one" }),
          providerKey({ id: "k2", name: "Backup", value: "sk-two" }),
        ],
      }),
    });

    // Toggle only the second row via modal, then save successfully.
    await user.click(screen.getByTestId("ai-gateway-key-1"));
    await user.click(screen.getByTestId("ai-gateway-key-dialog-daily-reset"));
    await user.click(screen.getByTestId("ai-gateway-key-dialog-confirm"));
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(first.onSave).toHaveBeenCalledTimes(1);
    expect(
      savedKeys(first.onSave).map((entry) => entry.daily_reset),
      "保存载荷应分别携带每行 daily reset 状态",
    ).toEqual([false, true]);

    const saved = first.onSave.mock.calls[0][0] as GatewayUpstreamProvider;
    first.unmount();

    // Reopening the saved provider restores each row's setting independently.
    renderKeyDialog({ provider: saved });
    expect(
      screen.queryByTestId("ai-gateway-key-daily-reset-badge-0"),
      "重开后第 1 行应无徽标",
    ).not.toBeInTheDocument();
    expect(
      screen.getByTestId("ai-gateway-key-daily-reset-badge-1"),
      "重开后第 2 行应有徽标",
    ).toBeInTheDocument();
  });

  it("provider_dialog_unsaved_daily_reset_edit_is_not_published_on_cancel_or_failed_save", async () => {
    const user = userEvent.setup();
    const baseKeys = [
      providerKey({ id: "k1", name: "Primary", value: "sk-one" }),
    ];

    // Cancel: onSave is never invoked, so the draft setting is not published.
    const cancelled = renderKeyDialog({
      provider: makeProvider({ keys: baseKeys }),
    });
    await user.click(screen.getByTestId("ai-gateway-key-0"));
    await user.click(screen.getByTestId("ai-gateway-key-dialog-daily-reset"));
    await user.click(screen.getByTestId("ai-gateway-key-dialog-confirm"));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(cancelled.onSave).not.toHaveBeenCalled();
    expect(cancelled.onOpenChange).toHaveBeenCalledWith(false);
    cancelled.unmount();

    // A save that resolves false keeps the dialog and the unsaved draft.
    const resolvedFalse = renderKeyDialog({
      provider: makeProvider({ keys: baseKeys }),
      onSave: vi.fn().mockResolvedValue(false),
    });
    await user.click(screen.getByTestId("ai-gateway-key-0"));
    await user.click(screen.getByTestId("ai-gateway-key-dialog-daily-reset"));
    await user.click(screen.getByTestId("ai-gateway-key-dialog-confirm"));
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(resolvedFalse.onSave).toHaveBeenCalledTimes(1));
    expect(resolvedFalse.onOpenChange).not.toHaveBeenCalledWith(false);
    expect(
      screen.getByTestId("ai-gateway-key-daily-reset-badge-0"),
      "失败保存后应保留草稿中的 daily reset 徽标",
    ).toBeInTheDocument();
    resolvedFalse.unmount();

    // A rejected save also keeps the dialog and the unsaved draft.
    const rejected = renderKeyDialog({
      provider: makeProvider({ keys: baseKeys }),
      onSave: vi.fn().mockRejectedValue(new Error("save failed")),
    });
    await user.click(screen.getByTestId("ai-gateway-key-0"));
    await user.click(screen.getByTestId("ai-gateway-key-dialog-daily-reset"));
    await user.click(screen.getByTestId("ai-gateway-key-dialog-confirm"));
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(rejected.onSave).toHaveBeenCalledTimes(1));
    expect(rejected.onOpenChange).not.toHaveBeenCalledWith(false);
    expect(
      screen.getByTestId("ai-gateway-key-daily-reset-badge-0"),
      "被拒绝保存后应保留草稿中的 daily reset 徽标",
    ).toBeInTheDocument();
  });

  it("清空既有密钥值后保存以空白值提交（后端保留已存值）", async () => {
    const user = userEvent.setup();
    const { onSave } = renderKeyDialog({
      provider: makeProvider({
        keys: [
          providerKey({
            id: "k1",
            name: "Primary",
            value: "sk-stored",
            auto_marked: true,
            failure_kind: "quota",
            marked_at: 1_700_000_000,
          }),
        ],
      }),
    });

    await user.click(screen.getByTestId("ai-gateway-key-0"));
    const valueInput = screen.getByTestId("ai-gateway-key-dialog-value");
    await user.clear(valueInput);
    await user.click(screen.getByTestId("ai-gateway-key-dialog-confirm"));
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledTimes(1);
    expect(savedKeys(onSave)[0]).toMatchObject({
      id: "k1",
      name: "Primary",
      value: "",
    });
  });

  it("改写既有密钥值时以相同 id 提交新值（后端据此清除运行时状态）", async () => {
    const user = userEvent.setup();
    const { onSave } = renderKeyDialog({
      provider: makeProvider({
        keys: [
          providerKey({
            id: "k1",
            name: "Primary",
            value: "sk-old",
            auto_marked: true,
            failure_kind: "authentication",
            marked_at: 1_700_000_000,
          }),
        ],
      }),
    });

    await user.click(screen.getByTestId("ai-gateway-key-0"));
    const valueInput = screen.getByTestId("ai-gateway-key-dialog-value");
    await user.clear(valueInput);
    await user.type(valueInput, "sk-new");
    await user.click(screen.getByTestId("ai-gateway-key-dialog-confirm"));
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(savedKeys(onSave)[0]).toMatchObject({ id: "k1", value: "sk-new" });
  });

  it("仅重命名既有密钥时 id 与已存值不变（后端据此保留运行时状态）", async () => {
    const user = userEvent.setup();
    const { onSave } = renderKeyDialog({
      provider: makeProvider({
        keys: [
          providerKey({
            id: "k1",
            name: "Primary",
            value: "sk-stored",
            auto_marked: true,
            failure_kind: "quota",
            marked_at: 1_700_000_000,
          }),
        ],
      }),
    });

    await user.click(screen.getByTestId("ai-gateway-key-0"));
    const nameInput = screen.getByTestId("ai-gateway-key-dialog-name");
    await user.clear(nameInput);
    await user.type(nameInput, "Renamed");
    await user.click(screen.getByTestId("ai-gateway-key-dialog-confirm"));
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(savedKeys(onSave)[0]).toMatchObject({
      id: "k1",
      name: "Renamed",
      value: "sk-stored",
    });
  });

  it("渲染每个密钥的状态徽章与标记时间", () => {
    const markedAt = 1_700_000_000;
    renderKeyDialog({
      provider: makeProvider({
        keys: [
          providerKey({ id: "k-usable", name: "Usable Key", enabled: true }),
          providerKey({ id: "k-disabled", name: "Disabled Key", enabled: false }),
          providerKey({
            id: "k-quota",
            name: "Quota Key",
            auto_marked: true,
            failure_kind: "quota",
            marked_at: markedAt,
          }),
          providerKey({
            id: "k-auth",
            name: "Auth Key",
            auto_marked: true,
            failure_kind: "authentication",
            marked_at: markedAt,
          }),
        ],
      }),
    });

    expect(screen.getByTestId("ai-gateway-key-state-0")).toHaveTextContent(
      i18n.t("aiGatewayProviderKeyStateUsable"),
    );
    expect(screen.getByTestId("ai-gateway-key-state-1")).toHaveTextContent(
      i18n.t("aiGatewayProviderKeyStateDisabled"),
    );
    expect(screen.getByTestId("ai-gateway-key-state-2")).toHaveTextContent(
      i18n.t("aiGatewayProviderKeyStateQuota"),
    );
    expect(screen.getByTestId("ai-gateway-key-state-3")).toHaveTextContent(
      i18n.t("aiGatewayProviderKeyStateAuth"),
    );

    const markedTime = formatGatewayTimestamp(markedAt)!;
    expect(screen.getByTestId("ai-gateway-key-state-2")).toHaveTextContent(
      markedTime,
    );
    expect(screen.getByTestId("ai-gateway-key-state-3")).toHaveTextContent(
      markedTime,
    );
  });

  it("关闭启用开关后保存该密钥 enabled=false 且其他密钥不受影响", async () => {
    const user = userEvent.setup();
    const { onSave } = renderKeyDialog({
      provider: makeProvider({
        keys: [
          providerKey({ id: "k1", name: "Primary", value: "sk-one" }),
          providerKey({ id: "k2", name: "Backup", value: "sk-two" }),
        ],
      }),
    });

    await user.click(screen.getByTestId("ai-gateway-key-toggle-1"));
    await user.click(screen.getByRole("button", { name: "Save" }));

    const keys = savedKeys(onSave);
    expect(keys[0]).toMatchObject({ id: "k1", enabled: true });
    expect(keys[1]).toMatchObject({ id: "k2", enabled: false });
  });

  it("自动标记的密钥提供重新启用按钮并回调对应 providerId/keyId", async () => {
    const user = userEvent.setup();
    const { onReenableKey } = renderKeyDialog({
      provider: makeProvider({
        keys: [
          providerKey({
            id: "k1",
            name: "Quota Key",
            auto_marked: true,
            failure_kind: "quota",
            marked_at: 1_700_000_000,
          }),
        ],
      }),
    });

    await user.click(screen.getByTestId("ai-gateway-reenable-key-0"));

    expect(onReenableKey).toHaveBeenCalledWith("p1", "k1");
  });

  it("AC-012 鉴权标记密钥在状态徽章与标记时间旁展示脱敏原因，重新启用按钮仍回调", async () => {
    const user = userEvent.setup();
    const markedAt = 1_700_000_000;
    const { onReenableKey } = renderKeyDialog({
      provider: makeProvider({
        keys: [
          providerKey({
            id: "k-auth",
            name: "Auth Key",
            auto_marked: true,
            failure_kind: "authentication",
            marked_at: markedAt,
            reason: "invalid api key",
          }),
        ],
      }),
    });

    const stateBadge = screen.getByTestId("ai-gateway-key-state-0");
    expect(stateBadge).toHaveTextContent(
      i18n.t("aiGatewayProviderKeyStateAuth"),
    );
    expect(stateBadge).toHaveTextContent(formatGatewayTimestamp(markedAt)!);

    const reason = screen.getByTestId("ai-gateway-key-reason-0");
    expect(reason).toHaveTextContent(i18n.t("aiGatewayProviderKeyReasonLabel"));
    expect(reason).toHaveTextContent("invalid api key");

    await user.click(screen.getByTestId("ai-gateway-reenable-key-0"));
    expect(onReenableKey).toHaveBeenCalledWith("p1", "k-auth");
  });

  it("密钥长错误消息提供展开/收起超链接按钮，点击后展开显示完整内容且不触发编辑弹窗", async () => {
    const user = userEvent.setup();
    const longReason =
      "rate limit exceeded upstream: tokens per minute (TPM) limit reached, please retry after 60s";
    renderKeyDialog({
      provider: makeProvider({
        id: "p1",
        keys: [
          providerKey({
            id: "k-rate",
            name: "RateLimitedKey",
            value: "sk-rate",
            auto_marked: true,
            failure_kind: "quota",
            marked_at: 1_700_000_000,
            reason: longReason,
          }),
        ],
      }),
    });

    const reasonContainer = screen.getByTestId("ai-gateway-key-reason-0");
    expect(reasonContainer).toHaveTextContent(longReason);

    // 存在展开详情超链接按钮
    const toggleBtn = screen.getByTestId("ai-gateway-key-reason-toggle-0");
    expect(toggleBtn).toBeInTheDocument();
    expect(toggleBtn).toHaveTextContent(i18n.t("aiGatewayProviderKeyExpandDetails"));

    // 点击展开按钮，不弹出编辑弹窗，按钮变为收起
    await user.click(toggleBtn);
    expect(screen.queryByTestId("ai-gateway-key-dialog")).not.toBeInTheDocument();
    expect(toggleBtn).toHaveTextContent(i18n.t("aiGatewayProviderKeyCollapseDetails"));

    // 再次点击收起
    await user.click(toggleBtn);
    expect(toggleBtn).toHaveTextContent(i18n.t("aiGatewayProviderKeyExpandDetails"));
  });

  it("编辑密钥弹窗中完整显示被自动标记密钥的错误诊断信息", async () => {
    const user = userEvent.setup();
    const detailedReason = "Detailed upstream authentication failure: invalid Bearer token format.";
    renderKeyDialog({
      provider: makeProvider({
        keys: [
          providerKey({
            id: "k-err",
            name: "ErrorKey",
            value: "sk-err",
            auto_marked: true,
            failure_kind: "authentication",
            marked_at: 1_700_000_000,
            reason: detailedReason,
          }),
        ],
      }),
    });

    await user.click(screen.getByTestId("ai-gateway-key-0"));
    expect(screen.getByTestId("ai-gateway-key-dialog")).toBeInTheDocument();
    const dialogReason = screen.getByTestId("ai-gateway-key-dialog-reason");
    expect(dialogReason).toBeInTheDocument();
    expect(dialogReason).toHaveTextContent(detailedReason);
  });

  it("删除其中一个密钥后保存，提交载荷省略该密钥并保留另一个", async () => {
    const user = userEvent.setup();
    const { onSave } = renderKeyDialog({
      provider: makeProvider({
        keys: [
          providerKey({ id: "k1", name: "Primary", value: "sk-one" }),
          providerKey({ id: "k2", name: "Backup", value: "sk-two" }),
        ],
      }),
    });

    await user.click(screen.getByTestId("ai-gateway-key-0"));
    await user.click(screen.getByTestId("ai-gateway-key-dialog-delete"));
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledTimes(1);
    const keys = savedKeys(onSave);
    expect(keys.map((entry) => entry.id)).toEqual(["k2"]);
    expect(keys).toHaveLength(1);
    expect(keys[0]).toMatchObject({
      id: "k2",
      name: "Backup",
      value: "sk-two",
    });
  });

  it("默认隐藏密钥并提供显示密钥按钮，点击后显示密钥明文，再次点击隐藏", async () => {
    const user = userEvent.setup();
    renderKeyDialog({
      provider: makeProvider({
        keys: [
          providerKey({ id: "k1", name: "Primary", value: "sk-secret-123" }),
        ],
      }),
    });

    const valueEl = screen.getByTestId("ai-gateway-key-value-0");
    expect(valueEl).toHaveTextContent("sk-s••••-123");

    const toggleBtn = screen.getByTestId("ai-gateway-key-toggle-visibility-0");
    expect(toggleBtn).toBeInTheDocument();
    expect(toggleBtn).toHaveAttribute("aria-label", "Show secret");

    await user.click(toggleBtn);
    expect(valueEl).toHaveTextContent("sk-secret-123");
    expect(toggleBtn).toHaveAttribute("aria-label", "Hide secret");

    await user.click(toggleBtn);
    expect(valueEl).toHaveTextContent("sk-s••••-123");
    expect(toggleBtn).toHaveAttribute("aria-label", "Show secret");
  });

  it("多个密钥项显隐状态相互独立", async () => {
    const user = userEvent.setup();
    renderKeyDialog({
      provider: makeProvider({
        keys: [
          providerKey({ id: "k1", name: "Primary", value: "sk-one" }),
          providerKey({ id: "k2", name: "Backup", value: "sk-two" }),
        ],
      }),
    });

    const value0 = screen.getByTestId("ai-gateway-key-value-0");
    const value1 = screen.getByTestId("ai-gateway-key-value-1");
    expect(value0).toHaveTextContent("••••••••");
    expect(value1).toHaveTextContent("••••••••");

    const toggleBtn0 = screen.getByTestId("ai-gateway-key-toggle-visibility-0");
    const toggleBtn1 = screen.getByTestId("ai-gateway-key-toggle-visibility-1");

    await user.click(toggleBtn0);
    expect(value0).toHaveTextContent("sk-one");
    expect(value1).toHaveTextContent("••••••••");

    await user.click(toggleBtn1);
    expect(value0).toHaveTextContent("sk-one");
    expect(value1).toHaveTextContent("sk-two");
  });

  it("密钥列表以分层卡片形式展示，包含名称、状态、快捷开关与操作按钮", () => {
    renderKeyDialog({
      provider: makeProvider({
        keys: [
          providerKey({ id: "k1", name: "Primary", value: "sk-one", daily_reset: true }),
        ],
      }),
    });

    const container = screen.getByTestId("ai-gateway-provider-keys");
    expect(container.className).toContain("grid");
    expect(container.className).toContain("md:grid-cols-2");

    const keyCard = screen.getByTestId("ai-gateway-key-0");
    expect(keyCard.className).toContain("rounded-xl");
    expect(keyCard.className).toContain("bg-card");
    expect(keyCard.className).toContain("border");

    // 头部：名称、快捷启用开关与日重置徽标
    const nameLabel = screen.getByTestId("ai-gateway-key-name-0");
    expect(nameLabel).toHaveTextContent("Primary");
    expect(screen.getByTestId("ai-gateway-key-toggle-0")).toBeInTheDocument();
    expect(screen.getByTestId("ai-gateway-key-daily-reset-badge-0")).toBeInTheDocument();

    // 凭据盒：脱敏值与复制按钮
    expect(screen.getByTestId("ai-gateway-key-value-0")).toBeInTheDocument();
    expect(screen.getByTestId("ai-gateway-key-copy-0")).toBeInTheDocument();

    // 底栏已移除：卡片上不再呈现日重置开关与编辑、删除按钮
    expect(screen.queryByTestId("ai-gateway-key-daily-reset-0")).not.toBeInTheDocument();
    expect(screen.queryByTestId("ai-gateway-key-edit-0")).not.toBeInTheDocument();
    expect(screen.queryByTestId("ai-gateway-key-remove-0")).not.toBeInTheDocument();
  });

  it("点击密钥卡片主体弹出编辑弹窗，取消弹窗不修改卡片", async () => {
    const user = userEvent.setup();
    renderKeyDialog({
      provider: makeProvider({
        keys: [
          providerKey({ id: "k1", name: "Primary", value: "sk-one" }),
        ],
      }),
    });

    await user.click(screen.getByTestId("ai-gateway-key-0"));
    expect(screen.getByTestId("ai-gateway-key-dialog")).toBeInTheDocument();
    expect(screen.getByTestId("ai-gateway-key-dialog-name")).toHaveValue("Primary");

    await user.click(screen.getByTestId("ai-gateway-key-dialog-cancel"));
    await waitFor(() => {
      expect(screen.queryByTestId("ai-gateway-key-dialog")).not.toBeInTheDocument();
    });
    expect(screen.getByTestId("ai-gateway-key-name-0")).toHaveTextContent("Primary");
  });

  it("点击卡片上的快捷启用开关直接操作且不触发编辑弹窗", async () => {
    const user = userEvent.setup();
    renderKeyDialog({
      provider: makeProvider({
        keys: [
          providerKey({ id: "k1", name: "Primary", value: "sk-one", daily_reset: false, enabled: true }),
        ],
      }),
    });

    const enableSwitch = screen.getByTestId("ai-gateway-key-toggle-0");

    await user.click(enableSwitch);
    expect(enableSwitch).toHaveAttribute("aria-checked", "false");
    expect(screen.queryByTestId("ai-gateway-key-dialog")).not.toBeInTheDocument();
  });

  it("点击密钥卡片上的复制按钮复制密文且不触发编辑弹窗", async () => {
    const user = userEvent.setup();
    const writeTextSpy = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: writeTextSpy },
      configurable: true,
      writable: true,
    });

    renderKeyDialog({
      provider: makeProvider({
        keys: [
          providerKey({ id: "k1", name: "Primary", value: "sk-secret-content" }),
        ],
      }),
    });

    const copyBtn = screen.getByTestId("ai-gateway-key-copy-0");
    await user.click(copyBtn);

    expect(writeTextSpy).toHaveBeenCalledWith("sk-secret-content");
    expect(screen.queryByTestId("ai-gateway-key-dialog")).not.toBeInTheDocument();
  });

  it("中文下编辑密钥弹窗确认与取消按钮显示中文", async () => {
    await i18n.changeLanguage("zh");
    const user = userEvent.setup();
    renderKeyDialog({
      provider: makeProvider({
        keys: [providerKey({ id: "k1", name: "Primary", value: "sk-one" })],
      }),
    });

    await user.click(screen.getByTestId("ai-gateway-key-0"));
    expect(screen.getByTestId("ai-gateway-key-dialog-confirm")).toHaveTextContent("确认");
    expect(screen.getByTestId("ai-gateway-key-dialog-cancel")).toHaveTextContent("取消");
    await i18n.changeLanguage("en");
  });

  it("在编辑密钥弹窗中修改日重置开关并确认后保存生效", async () => {
    const user = userEvent.setup();
    const { onSave } = renderKeyDialog({
      provider: makeProvider({
        keys: [providerKey({ id: "k1", name: "Primary", value: "sk-one", daily_reset: false })],
      }),
    });

    // 初始状态下无日重置徽标
    expect(screen.queryByTestId("ai-gateway-key-daily-reset-badge-0")).not.toBeInTheDocument();

    await user.click(screen.getByTestId("ai-gateway-key-0"));
    const resetSwitch = screen.getByTestId("ai-gateway-key-dialog-daily-reset");
    expect(resetSwitch).toHaveAttribute("aria-checked", "false");
    await user.click(resetSwitch);
    expect(resetSwitch).toHaveAttribute("aria-checked", "true");
    await user.click(screen.getByTestId("ai-gateway-key-dialog-confirm"));

    // 卡片上的日重置徽标出现
    expect(screen.getByTestId("ai-gateway-key-daily-reset-badge-0")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(onSave).toHaveBeenCalledTimes(1);
    expect(savedKeys(onSave)[0].daily_reset).toBe(true);
  });

  it("在编辑密钥弹窗中点击最左侧删除按钮移除该密钥并关闭弹窗", async () => {
    const user = userEvent.setup();
    const { onSave } = renderKeyDialog({
      provider: makeProvider({
        keys: [
          providerKey({ id: "k1", name: "Primary", value: "sk-one" }),
          providerKey({ id: "k2", name: "Secondary", value: "sk-two" }),
        ],
      }),
    });

    await user.click(screen.getByTestId("ai-gateway-key-0"));
    const deleteBtn = screen.getByTestId("ai-gateway-key-dialog-delete");
    expect(deleteBtn).toBeInTheDocument();
    await user.click(deleteBtn);

    await waitFor(() => {
      expect(screen.queryByTestId("ai-gateway-key-dialog")).not.toBeInTheDocument();
    });
    // 卡片中只剩下第 2 个密钥
    expect(screen.queryByTestId("ai-gateway-key-name-0")).toHaveTextContent("Secondary");

    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(onSave).toHaveBeenCalledTimes(1);
    expect(savedKeys(onSave)).toHaveLength(1);
    expect(savedKeys(onSave)[0].id).toBe("k2");
  });

  it("新增密钥模式下弹窗底部不展示删除按钮", async () => {
    const user = userEvent.setup();
    renderKeyDialog({
      provider: makeProvider({ keys: [] }),
    });

    await user.click(screen.getByTestId("ai-gateway-add-key"));
    expect(screen.getByTestId("ai-gateway-key-dialog")).toBeInTheDocument();
    expect(screen.queryByTestId("ai-gateway-key-dialog-delete")).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Step 2: provider-template binding, effective-policy display and broken
// binding handling (REQ-008 / AC-004 / AC-006 / AC-023 / AC-024 / AC-025).
// RED tests for the frozen interface contract. Only this test file is touched.
// ---------------------------------------------------------------------------

function bindingTemplateView(
  overrides: Partial<GatewayProviderTemplateView["template"]> = {},
): GatewayProviderTemplateView {
  return {
    template: {
      id: "tpl-cc",
      name: "CommandCode",
      description: "CommandCode provider template",
      base_url: "https://api.commandcode.ai/provider/v1",
      protocol: "chat_completions",
      source: "https://api.commandcode.ai/provider/v1/models",
      models_url: "https://api.commandcode.ai/provider/v1/models",
      models: [],
      sync_strategy: "generic",
      quota_strategy: "commandcode",
      error_strategy: "generic",
      ...overrides,
    },
    synced_at: null,
    source: "snapshot",
    from_snapshot: true,
  };
}

function renderBindingDialog({
  provider,
  templates,
  onSave = vi.fn().mockResolvedValue(true),
  onOpenChange = vi.fn(),
}: {
  provider: GatewayUpstreamProvider;
  templates?: GatewayProviderTemplateView[];
  onSave?: ReturnType<typeof vi.fn>;
  onOpenChange?: ReturnType<typeof vi.fn>;
}) {
  const view = renderWithProviders(
    <ProviderDetailDialog
      open
      provider={provider}
      templates={templates}
      busy={false}
      onSave={onSave}
      onDelete={vi.fn()}
      onOpenChange={onOpenChange}
    />,
  );
  return { ...view, onSave, onOpenChange };
}

describe("ProviderDetailDialog 模板绑定与有效策略（Step 2）", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("列出已有模板与未绑定选项，并通过 upsert 载荷提交 template_id 且保留实例字段", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn().mockResolvedValue(true);
    const provider = makeProvider({
      id: "p-bind",
      name: "Binder",
      base_url: "https://binder.example/v1",
      default_model: "m-1",
      weight: 3,
      tags: ["prod"],
      template_id: null,
      mappings: [{ local_model: "l", upstream_model: "m-1" }],
    });

    renderBindingDialog({
      provider,
      templates: [
        bindingTemplateView({ id: "tpl-cc", name: "CommandCode" }),
        bindingTemplateView({ id: "tpl-go", name: "OpenCode Go" }),
      ],
      onSave,
    });

    const select = screen.getByTestId(
      "ai-gateway-provider-template-select",
    ) as HTMLSelectElement;
    expect(select.tagName).toBe("SELECT");
    expect(select).toHaveValue("");
    const values = Array.from(select.options).map((option) => option.value);
    expect(values).toContain("");
    expect(values).toEqual(expect.arrayContaining(["tpl-cc", "tpl-go"]));

    await user.selectOptions(select, "tpl-cc");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1));
    const [saved] = onSave.mock.calls[0] as [
      GatewayUpstreamProvider,
      ModelPrice[],
    ];
    expect(saved).toMatchObject({
      id: "p-bind",
      name: "Binder",
      base_url: "https://binder.example/v1",
      template_id: "tpl-cc",
      weight: 3,
      tags: ["prod"],
    });
    expect(saved.mappings?.[0]).toMatchObject({
      local_model: "l",
      upstream_model: "m-1",
    });
  });

  it("通过未绑定选项解绑为 template_id=null", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn().mockResolvedValue(true);
    const provider = makeProvider({
      id: "p-unbind",
      template_id: "tpl-cc",
    });

    renderBindingDialog({
      provider,
      templates: [bindingTemplateView({ id: "tpl-cc" })],
      onSave,
    });

    const select = screen.getByTestId("ai-gateway-provider-template-select");
    expect(select).toHaveValue("tpl-cc");

    await user.selectOptions(select, "");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1));
    const [saved] = onSave.mock.calls[0] as [GatewayUpstreamProvider];
    expect(saved.template_id).toBeNull();
  });

  it("不渲染任何服务商级策略覆盖控件", () => {
    renderBindingDialog({
      provider: makeProvider({ template_id: "tpl-cc" }),
      templates: [bindingTemplateView({ id: "tpl-cc" })],
    });

    for (const testId of [
      "ai-gateway-provider-sync-strategy",
      "ai-gateway-provider-quota-strategy",
      "ai-gateway-provider-error-strategy",
    ]) {
      expect(
        screen.queryByTestId(testId),
        `${testId} 不得作为服务商级覆盖控件存在`,
      ).not.toBeInTheDocument();
    }
  });

  it("展示绑定模板的有效策略，并在模板缺失时显示损坏绑定警告且保持可编辑", () => {
    const { unmount } = renderBindingDialog({
      provider: makeProvider({ template_id: "tpl-cc" }),
      templates: [bindingTemplateView({ id: "tpl-cc" })],
    });

    const effective = screen.getByTestId("ai-gateway-provider-effective-strategy");
    expect(effective).toHaveAttribute("data-sync-strategy", "generic");
    expect(effective).toHaveAttribute("data-quota-strategy", "commandcode");
    expect(effective).toHaveAttribute("data-error-strategy", "generic");
    expect(
      screen.queryByTestId("ai-gateway-provider-broken-binding-warning"),
    ).not.toBeInTheDocument();
    unmount();

    renderBindingDialog({
      provider: makeProvider({ id: "p-broken", template_id: "deleted-tpl" }),
      templates: [bindingTemplateView({ id: "tpl-cc" })],
    });

    expect(
      screen.getByTestId("ai-gateway-provider-broken-binding-warning"),
    ).toBeInTheDocument();
    const effectiveBroken = screen.getByTestId(
      "ai-gateway-provider-effective-strategy",
    );
    expect(effectiveBroken).toHaveAttribute("data-quota-strategy", "none");

    // 损坏绑定仍可编辑：名称输入与模板选择器保持可用以便重新绑定/解绑。
    expect(screen.getByLabelText("Name")).toBeEnabled();
    expect(
      screen.getByTestId("ai-gateway-provider-template-select"),
    ).toBeEnabled();
  });

  it("upsert 失败时保留本地状态且不报告成功关闭", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn().mockResolvedValue(false);
    const onOpenChange = vi.fn();

    renderBindingDialog({
      provider: makeProvider({ id: "p-fail", template_id: null }),
      templates: [bindingTemplateView({ id: "tpl-cc" })],
      onSave,
      onOpenChange,
    });

    const select = screen.getByTestId(
      "ai-gateway-provider-template-select",
    ) as HTMLSelectElement;
    await user.selectOptions(select, "tpl-cc");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1));
    expect(onOpenChange).not.toHaveBeenCalledWith(false);
    expect(
      screen.getByTestId("ai-gateway-provider-template-select"),
    ).toHaveValue("tpl-cc");
  });
});



// ---------------------------------------------------------------------------
// Step 4: the provider default and per-row protocol selectors gain the third
// upstream protocol `anthropic_messages` while rows keep inheritance.
// RED before implementation: only Chat/Responses options exist.
// ---------------------------------------------------------------------------

describe("ProviderDetailDialog anthropic_messages 协议选项", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("服务商默认协议选择器提供 Anthropic Messages (native) 选项", () => {
    renderProviderDialog({ provider: makeProvider() });

    const select = screen.getByLabelText("API protocol");
    const option = within(select).getByRole("option", {
      name: "Anthropic Messages (native)",
    });
    expect(option).toHaveValue("anthropic_messages");
  });

  it("映射行协议选择器提供继承与 Anthropic Messages 选项并保存显式协议", async () => {
    const user = userEvent.setup();
    const provider = makeProvider({
      protocol: "chat_completions",
      mappings: [
        { local_model: "claude-local", upstream_model: "cc-opus", protocol: null },
      ],
    });
    const { onSave } = renderProviderDialog({ provider });

    const rowSelect = screen.getByLabelText("Mapping protocol 1");
    expect(
      within(rowSelect).getByRole("option", { name: "Inherit from provider" }),
    ).toBeInTheDocument();
    expect(
      within(rowSelect).getByRole("option", {
        name: "Anthropic Messages (native)",
      }),
    ).toHaveValue("anthropic_messages");

    await user.selectOptions(rowSelect, "anthropic_messages");
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledTimes(1);
    const saved = onSave.mock.calls[0][0] as GatewayUpstreamProvider;
    expect(saved.mappings[0].protocol).toBe("anthropic_messages");
  });

  it("父默认切换为 Anthropic Messages 只影响继承行", async () => {
    const user = userEvent.setup();
    const provider = makeProvider({
      protocol: "chat_completions",
      mappings: [
        { local_model: "inherit", upstream_model: "cc-opus", protocol: null },
        {
          local_model: "responses-row",
          upstream_model: "gpt-4o",
          protocol: "responses",
        },
      ],
    });
    const { onSave } = renderProviderDialog({ provider });

    await user.selectOptions(
      screen.getByLabelText("API protocol"),
      "anthropic_messages",
    );
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledTimes(1);
    const saved = onSave.mock.calls[0][0] as GatewayUpstreamProvider;
    expect(saved.protocol).toBe("anthropic_messages");
    // The inherited row keeps no explicit protocol (null = follow parent).
    expect(saved.mappings[0].protocol).toBeUndefined();
    expect(saved.mappings[1].protocol).toBe("responses");
  });

  it("中文下提供 Anthropic Messages（原生）标签", async () => {
    await i18n.changeLanguage("zh");
    renderProviderDialog({ provider: makeProvider() });

    expect(
      screen.getByRole("option", { name: "Anthropic Messages（原生）" }),
    ).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Step 4 (20261015-pool-only-reasoning) REQ-007 / AC-012 negative isolation:
// the upstream-provider editor gains no pool reasoning controls or persistent
// pool fields, and ordinary provider effort identifiers keep their exact values
// (no canonicalization, no new rejection).
// ---------------------------------------------------------------------------

describe("ProviderDetailDialog 池推理隔离（Step 4）", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("上游服务商编辑器不新增任何池推理控件", async () => {
    renderProviderDialog({ provider: makeProvider() });

    for (const key of [
      "aiGatewayModelPoolReasoningDefaultLabel",
      "aiGatewayModelPoolReasoningProfileLabel",
      "aiGatewayModelPoolReasoningMappingLabel",
      "aiGatewayModelPoolReasoningSupportsDisableLabel",
    ]) {
      expect(
        screen.queryByLabelText(i18n.t(key)),
        `服务商编辑器不应包含 ${key}`,
      ).not.toBeInTheDocument();
    }
    expect(
      document.querySelector('[data-testid^="ai-gateway-model-pool-"]'),
    ).toBeNull();
  });

  it("普通服务商映射保留任意原生档位标识，不做规范化或拒绝", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const provider = makeProvider({
      mappings: [
        {
          local_model: "local-a",
          upstream_model: "remote-a",
          reasoning_efforts: ["max", "minimal", "AUTO"],
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

    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledTimes(1);
    const saved = onSave.mock.calls[0][0] as GatewayUpstreamProvider;
    expect(saved.mappings[0].reasoning_efforts).toEqual([
      "max",
      "minimal",
      "AUTO",
    ]);
  });
});
