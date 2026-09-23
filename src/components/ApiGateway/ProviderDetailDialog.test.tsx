import { act, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
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

    const expand = screen.getByTestId("api-gateway-mapping-expand-0");
    expect(
      expand,
      "默认收起时展开箭头应为 aria-expanded=false",
    ).toHaveAttribute("aria-expanded", "false");
    expect(
      screen.queryByTestId("api-gateway-mapping-price-0"),
      "默认收起时不渲染计价配置",
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("api-gateway-mapping-efforts-0"),
      "默认收起时不渲染推理档位面板",
    ).not.toBeInTheDocument();

    await user.click(expand);
    expect(
      expand,
      "点击展开后箭头应为 aria-expanded=true",
    ).toHaveAttribute("aria-expanded", "true");
    expect(
      screen.getByTestId("api-gateway-mapping-price-0"),
      "展开后应渲染计价配置",
    ).toBeInTheDocument();
    expect(
      screen.getByTestId("api-gateway-mapping-efforts-0"),
      "展开后应渲染推理档位面板",
    ).toBeInTheDocument();
    expect(
      valueOf("api-gateway-price-0-input"),
      "展开后应回显已存价格",
    ).toBe("1.5");

    await user.click(expand);
    expect(
      expand,
      "再次点击后箭头应回到 aria-expanded=false",
    ).toHaveAttribute("aria-expanded", "false");
    expect(
      screen.queryByTestId("api-gateway-mapping-price-0"),
      "再次点击应收起计价配置",
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("api-gateway-mapping-efforts-0"),
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
      screen.queryByTestId("api-gateway-mapping-price-0"),
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
      screen.getByTestId("api-gateway-mapping-expand-0"),
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

    await user.click(screen.getByTestId("api-gateway-mapping-expand-1"));
    expect(
      valueOf("api-gateway-price-1-input"),
      "显式存储 0 应回显为字符串 0 而不是空白",
    ).toBe("0");

    await user.click(screen.getByTestId("api-gateway-mapping-expand-0"));
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
    await user.click(screen.getByTestId("api-gateway-mapping-expand-0"));
    await user.click(screen.getByTestId("api-gateway-mapping-expand-1"));
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

    await user.click(screen.getByTestId("api-gateway-mapping-expand-0"));
    await user.click(screen.getByTestId("api-gateway-mapping-expand-1"));
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

    await user.click(screen.getByTestId("api-gateway-mapping-expand-0"));
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
    await user.click(screen.getByTestId("api-gateway-mapping-expand-0"));
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
    await user.click(screen.getByTestId("api-gateway-mapping-expand-0"));
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
    await user.click(screen.getByTestId("api-gateway-mapping-expand-0"));
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
    await user.click(screen.getByTestId("api-gateway-mapping-expand-0"));
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
    await user.click(screen.getByTestId("api-gateway-mapping-expand-0"));
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

  it("provider_dialog_blank_upstream_model_has_no_price_editor", async () => {
    const user = userEvent.setup();
    const provider = makeProvider({
      mappings: [
        { local_model: "local-a", upstream_model: "   " },
        { local_model: "local-b", upstream_model: "remote-a" },
      ],
    });
    renderProviderDialog({ provider });

    await user.click(screen.getByTestId("api-gateway-mapping-expand-0"));
    await user.click(screen.getByTestId("api-gateway-mapping-expand-1"));

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
    const banner = screen.getByTestId("api-gateway-bound-template-banner");
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
    const countBadge = screen.getByTestId("api-gateway-mappings-count-badge");
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

    const banner = screen.getByTestId("api-gateway-bound-template-banner");
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

    const countBadge = screen.getByTestId("api-gateway-mappings-count-badge");
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
      screen.queryByTestId("api-gateway-bound-template-banner"),
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
      within(udRow!).queryByTestId("api-gateway-reenable-mapping-ud"),
    ).not.toBeInTheDocument();

    // Auto-disabled 行：有 data-auto-disabled，且有 per-row 重新启用按钮
    const adSwitch = screen.getByRole("switch", { name: "Enable mapping 2" });
    const adRow = adSwitch.closest("li") as HTMLElement;
    expect(adRow!.getAttribute("data-auto-disabled")).toBe("true");
    // 行为层：auto-disabled 行必须包含 per-row re-enable 控件
    expect(
      within(adRow!).getByTestId("api-gateway-reenable-mapping-ad"),
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
      "api-gateway-reenable-mapping-gpt-4o",
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

    // 用户禁用行不应有 api-gateway-reenable-mapping-* 按钮
    // （只保留普通的 Enable switch）
    const udSwitch = screen.getByRole("switch", {
      name: "Enable mapping 1",
    });
    expect(udSwitch).toBeEnabled();
    // 不应存在重新启用按钮 testid
    expect(
      screen.queryByTestId("api-gateway-reenable-mapping-ud-only"),
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
      screen.queryByTestId("api-gateway-reenable-mapping-healthy-row"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("api-gateway-provider-auto-disabled-models-p1"),
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
      "api-gateway-reenable-models-p1",
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
      screen.queryByTestId("api-gateway-reenable-models-p1"),
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

    const weightInput = screen.getByTestId("api-gateway-provider-weight-input");
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

    const weightInput = screen.getByTestId("api-gateway-provider-weight-input");
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

    const weightInput = screen.getByTestId("api-gateway-provider-weight-input");
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
      within(row2).getByTestId("api-gateway-reenable-mapping-claude-3"),
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
      within(row1).queryByTestId("api-gateway-reenable-mapping-gpt-4o-custom"),
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
      screen.queryByTestId("api-gateway-reenable-models-p1"),
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
    const reenableAll = screen.getByTestId("api-gateway-reenable-models-p1");
    expect(reenableAll).toBeInTheDocument();
    expect(reenableAll).toHaveAccessibleName(/Re-enable all|重新启用所有/i);

    const row1 = screen
      .getByRole("switch", { name: "Enable mapping 1" })
      .closest("li") as HTMLElement;
    expect(row1.getAttribute("data-auto-disabled")).toBe("true");
    expect(
      within(row1).getByTestId("api-gateway-reenable-mapping-gpt-4o"),
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
});


