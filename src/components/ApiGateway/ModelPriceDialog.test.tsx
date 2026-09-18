import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { ModelPriceDialog } from "@/components/ApiGateway/ModelPriceDialog";
import type { GatewayUpstreamProvider, ModelPrice } from "@/lib/apiGateway";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

function mockProvider(
  id: string,
  name: string,
  defaultModel: string | null = null,
  mappings: Array<{ local_model: string; upstream_model: string; display_name?: string }> = [],
): GatewayUpstreamProvider {
  return {
    id,
    name,
    base_url: "https://api.example.com",
    api_key: "test-key",
    default_model: defaultModel,
    protocol: "chat_completions",
    mappings,
    enabled: true,
    auto_disabled: false,
    disabled_reason: null,
    disabled_at: null,
    consecutive_failures: 0,
    last_error_at: null,
  };
}

function price(overrides: Partial<ModelPrice> = {}): ModelPrice {
  return {
    upstream_model: "gpt-4o",
    input: 1,
    cache_read: 0.1,
    cache_write: 0.2,
    output: 2,
    ...overrides,
  };
}

function mockPrices(initial: ModelPrice[]) {
  let store = [...initial];
  invokeMock.mockImplementation(async (command: string, args?: any) => {
    switch (command) {
      case "api_gateway_model_prices_get":
        return store;
      case "api_gateway_model_prices_save":
        store = args.prices as ModelPrice[];
        return store;
      default:
        throw new Error(`Unhandled command: ${command}`);
    }
  });
  return () => store;
}

describe("ModelPriceDialog", () => {
  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
  });

  it("加载已有价格行并可编辑保存", async () => {
    const user = userEvent.setup();
    const readStore = mockPrices([price()]);
    const onSaved = vi.fn();

    renderWithProviders(
      <ModelPriceDialog open onOpenChange={() => {}} onSaved={onSaved} />,
    );

    const modelInput = await screen.findByLabelText("Upstream model");
    expect(modelInput).toHaveValue("gpt-4o");

    const outputInput = screen.getByLabelText("Output");
    await user.clear(outputInput);
    await user.type(outputInput, "3.5");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_model_prices_save", {
        prices: [price({ output: 3.5 })],
      }),
    );
    expect(onSaved).toHaveBeenCalledWith([price({ output: 3.5 })]);
    expect(readStore()[0].output).toBe(3.5);
  });

  it("支持新增与删除价格行并按模型名键控", async () => {
    const user = userEvent.setup();
    mockPrices([price()]);

    renderWithProviders(
      <ModelPriceDialog open onOpenChange={() => {}} />,
    );
    await screen.findByLabelText("Upstream model");

    await user.click(screen.getByRole("button", { name: /Add price/ }));
    const modelInputs = screen.getAllByLabelText("Upstream model");
    expect(modelInputs).toHaveLength(2);
    await user.type(modelInputs[1], "deepseek-chat");

    const deleteButtons = screen.getAllByRole("button", {
      name: /Delete price/,
    });
    await user.click(deleteButtons[0]);

    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_model_prices_save", {
        prices: [
          {
            upstream_model: "deepseek-chat",
            input: 0,
            cache_read: 0,
            cache_write: 0,
            output: 0,
          },
        ],
      }),
    );
  });

  it("空价格表展示空状态", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "api_gateway_model_prices_get") return [];
      throw new Error(`Unhandled command: ${command}`);
    });

    renderWithProviders(
      <ModelPriceDialog open onOpenChange={() => {}} />,
    );

    expect(
      await screen.findByText("No model prices configured yet."),
    ).toBeInTheDocument();
  });

  it("保存价格不触碰服务商、本地 Key、终端同步或存储配置", async () => {
    const user = userEvent.setup();
    mockPrices([price()]);

    renderWithProviders(
      <ModelPriceDialog open onOpenChange={() => {}} />,
    );
    await screen.findByLabelText("Upstream model");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_model_prices_save", {
        prices: [price()],
      }),
    );
    const commands = invokeMock.mock.calls.map(([command]) => command);
    expect(commands).not.toContain("api_gateway_save_config");
    expect(commands).not.toContain("api_gateway_upsert_provider");
    expect(commands).not.toContain("api_gateway_upsert_key");
    expect(commands).not.toContain("api_gateway_sync_terminal");
    expect(commands).not.toContain("api_gateway_usage_retention_save");
    expect(commands).not.toContain("save_storage_config");
  });

  it("保存失败时展示可操作错误且不标记成功", async () => {
    const user = userEvent.setup();
    const onSaved = vi.fn();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "api_gateway_model_prices_get") return [price()];
      if (command === "api_gateway_model_prices_save") {
        throw new Error("boom");
      }
      throw new Error(`Unhandled command: ${command}`);
    });

    renderWithProviders(
      <ModelPriceDialog open onOpenChange={() => {}} onSaved={onSaved} />,
    );
    await screen.findByLabelText("Upstream model");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("boom"),
    );
    expect(onSaved).not.toHaveBeenCalled();
  });

  it("弹窗只在请求的 open 状态下渲染价格入口", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "api_gateway_model_prices_get") return [price()];
      throw new Error(`Unhandled command: ${command}`);
    });

    renderWithProviders(
      <ModelPriceDialog open={false} onOpenChange={() => {}} />,
    );

    expect(
      screen.queryByTestId("api-gateway-model-price-dialog"),
    ).not.toBeInTheDocument();
  });

  it("价格输入按四档单价渲染且单位为美元/百万 tokens", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "api_gateway_model_prices_get") return [price()];
      throw new Error(`Unhandled command: ${command}`);
    });

    renderWithProviders(
      <ModelPriceDialog open onOpenChange={() => {}} />,
    );
    const dialog = await screen.findByTestId("api-gateway-model-price-dialog");
    const scoped = within(dialog);
    expect(scoped.getByLabelText("Input")).toBeInTheDocument();
    expect(scoped.getByLabelText("Cache read")).toBeInTheDocument();
    expect(scoped.getByLabelText("Cache write")).toBeInTheDocument();
    expect(scoped.getByLabelText("Output")).toBeInTheDocument();
    expect(scoped.getAllByText(/USD \/ million tokens/).length).toBeGreaterThan(0);
  });

  it("按服务商分组展示已维护的价格，且模型可下拉选择", async () => {
    const user = userEvent.setup();
    const provOpenAI = mockProvider("prov-openai", "OpenAI", "gpt-4o", [
      { local_model: "gpt-4o-mini", upstream_model: "gpt-4o-mini", display_name: "GPT-4o Mini" },
    ]);
    const provDeepSeek = mockProvider("prov-deepseek", "DeepSeek", "deepseek-chat", [
      { local_model: "deepseek-reasoner", upstream_model: "deepseek-reasoner", display_name: "DeepSeek R1" },
    ]);

    mockPrices([
      price({ provider_id: "prov-openai", upstream_model: "gpt-4o", input: 2.5 }),
      price({ provider_id: "prov-deepseek", upstream_model: "deepseek-chat", input: 0.14 }),
    ]);

    renderWithProviders(
      <ModelPriceDialog
        open
        onOpenChange={() => {}}
        providers={[provOpenAI, provDeepSeek]}
      />,
    );

    const openAiGroup = await screen.findByTestId("api-gateway-price-group-prov-openai");
    const deepSeekGroup = await screen.findByTestId("api-gateway-price-group-prov-deepseek");
    expect(openAiGroup).toBeInTheDocument();
    expect(deepSeekGroup).toBeInTheDocument();

    expect(within(openAiGroup).getByText("OpenAI")).toBeInTheDocument();
    expect(within(deepSeekGroup).getByText("DeepSeek")).toBeInTheDocument();

    const select = within(openAiGroup).getByLabelText("Upstream model") as HTMLSelectElement;
    expect(select.tagName).toBe("SELECT");
    expect(select.value).toBe("gpt-4o");

    await user.selectOptions(select, "gpt-4o-mini");
    expect(select.value).toBe("gpt-4o-mini");
  });

  it("点击特定服务商的添加价格按钮，支持从该服务商未定价模型中选择添加并保存", async () => {
    const user = userEvent.setup();
    const provOpenAI = mockProvider("prov-openai", "OpenAI", "gpt-4o", [
      { local_model: "gpt-4o-mini", upstream_model: "gpt-4o-mini" },
    ]);

    mockPrices([
      price({ provider_id: "prov-openai", upstream_model: "gpt-4o", input: 2.5 }),
    ]);

    renderWithProviders(
      <ModelPriceDialog
        open
        onOpenChange={() => {}}
        providers={[provOpenAI]}
      />,
    );

    const openAiGroup = await screen.findByTestId("api-gateway-price-group-prov-openai");
    const addBtn = within(openAiGroup).getByRole("button", { name: /Add price/ });
    await user.click(addBtn);

    const selects = within(openAiGroup).getAllByLabelText("Upstream model") as HTMLSelectElement[];
    expect(selects).toHaveLength(2);
    expect(selects[1].value).toBe("gpt-4o-mini");

    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_model_prices_save", {
        prices: [
          price({ provider_id: "prov-openai", upstream_model: "gpt-4o", input: 2.5 }),
          {
            provider_id: "prov-openai",
            upstream_model: "gpt-4o-mini",
            input: 0,
            cache_read: 0,
            cache_write: 0,
            output: 0,
          },
        ],
      }),
    );
  });

  it("未关联到现有服务商的旧价格显示在其他分组中", async () => {
    const provOpenAI = mockProvider("prov-openai", "OpenAI", "gpt-4o");
    mockPrices([
      price({ provider_id: null, upstream_model: "some-legacy-model", input: 5 }),
    ]);

    renderWithProviders(
      <ModelPriceDialog
        open
        onOpenChange={() => {}}
        providers={[provOpenAI]}
      />,
    );

    const unassignedGroup = await screen.findByTestId("api-gateway-price-group-unassigned");
    expect(unassignedGroup).toBeInTheDocument();
    expect(within(unassignedGroup).getByText("Other / Unassigned")).toBeInTheDocument();
    expect(within(unassignedGroup).getByDisplayValue("some-legacy-model")).toBeInTheDocument();
  });

  it("可配置峰谷时间段及优惠单价，保存时提交 off_peak 数据", async () => {
    const user = userEvent.setup();
    mockPrices([price({ upstream_model: "gpt-4o", input: 2.0, output: 4.0 })]);

    renderWithProviders(
      <ModelPriceDialog open onOpenChange={() => {}} />,
    );

    await screen.findByLabelText("Upstream model");

    // Click off-peak configuration button to expand
    const offPeakBtn = screen.getByRole("button", { name: /Off-peak discount/i });
    await user.click(offPeakBtn);

    // Check "Enable off-peak pricing"
    const enableCheckbox = screen.getByRole("checkbox", { name: /Enable off-peak pricing/i });
    expect(enableCheckbox).not.toBeChecked();
    await user.click(enableCheckbox);
    expect(enableCheckbox).toBeChecked();

    // Modify start time & end time
    const startInput = screen.getByLabelText("Start");
    const endInput = screen.getByLabelText("End");
    await user.clear(startInput);
    await user.type(startInput, "01:00");
    await user.clear(endInput);
    await user.type(endInput, "07:30");

    // Set off-peak input and output prices
    const offPeakInput = screen.getByLabelText("Input (Off-peak)");
    const offPeakOutput = screen.getByLabelText("Output (Off-peak)");
    await user.clear(offPeakInput);
    await user.type(offPeakInput, "1.0");
    await user.clear(offPeakOutput);
    await user.type(offPeakOutput, "2.0");

    // Save
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_model_prices_save", {
        prices: [
          {
            upstream_model: "gpt-4o",
            input: 2.0,
            cache_read: 0.1,
            cache_write: 0.2,
            output: 4.0,
            off_peaks: [
              {
                start_time: "01:00",
                end_time: "07:30",
                input: 1.0,
                cache_read: 0.1, // fallback to standard cache_read
                cache_write: 0.2, // fallback to standard cache_write
                output: 2.0,
              },
            ],
            off_peak: {
              start_time: "01:00",
              end_time: "07:30",
              input: 1.0,
              cache_read: 0.1,
              cache_write: 0.2,
              output: 2.0,
            },
          },
        ],
      }),
    );
  });

  it("支持为一个模型配置多个谷时时段并保存提交", async () => {
    const user = userEvent.setup();
    mockPrices([price({ upstream_model: "gpt-4o", input: 2.0, output: 4.0 })]);

    renderWithProviders(<ModelPriceDialog open onOpenChange={() => {}} />);

    await screen.findByLabelText("Upstream model");

    // Click off-peak button
    const offPeakBtn = screen.getByRole("button", { name: /Off-peak discount/i });
    await user.click(offPeakBtn);

    // Enable off-peak
    const enableCheckbox = screen.getByRole("checkbox", { name: /Enable off-peak pricing/i });
    await user.click(enableCheckbox);

    // Configure 1st window
    const startInputs = screen.getAllByLabelText("Start");
    const endInputs = screen.getAllByLabelText("End");
    await user.clear(startInputs[0]);
    await user.type(startInputs[0], "00:00");
    await user.clear(endInputs[0]);
    await user.type(endInputs[0], "08:00");

    const inputRates = screen.getAllByLabelText("Input (Off-peak)");
    const outputRates = screen.getAllByLabelText("Output (Off-peak)");
    await user.clear(inputRates[0]);
    await user.type(inputRates[0], "1.0");
    await user.clear(outputRates[0]);
    await user.type(outputRates[0], "2.0");

    // Click Add off-peak window
    const addWindowBtn = screen.getByRole("button", { name: /Add off-peak window/i });
    await user.click(addWindowBtn);

    // Now 2 windows exist
    const updatedStarts = screen.getAllByLabelText("Start");
    const updatedEnds = screen.getAllByLabelText("End");
    expect(updatedStarts).toHaveLength(2);

    // Configure 2nd window
    await user.clear(updatedStarts[1]);
    await user.type(updatedStarts[1], "12:00");
    await user.clear(updatedEnds[1]);
    await user.type(updatedEnds[1], "14:00");

    const updatedInputRates = screen.getAllByLabelText("Input (Off-peak)");
    const updatedOutputRates = screen.getAllByLabelText("Output (Off-peak)");
    await user.clear(updatedInputRates[1]);
    await user.type(updatedInputRates[1], "1.5");
    await user.clear(updatedOutputRates[1]);
    await user.type(updatedOutputRates[1], "3.0");

    // Save
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_model_prices_save", {
        prices: [
          {
            upstream_model: "gpt-4o",
            input: 2.0,
            cache_read: 0.1,
            cache_write: 0.2,
            output: 4.0,
            off_peaks: [
              {
                start_time: "00:00",
                end_time: "08:00",
                input: 1.0,
                cache_read: 0.1,
                cache_write: 0.2,
                output: 2.0,
              },
              {
                start_time: "12:00",
                end_time: "14:00",
                input: 1.5,
                cache_read: 0.1,
                cache_write: 0.2,
                output: 3.0,
              },
            ],
            off_peak: {
              start_time: "00:00",
              end_time: "08:00",
              input: 1.0,
              cache_read: 0.1,
              cache_write: 0.2,
              output: 2.0,
            },
          },
        ],
      }),
    );
  });

  it("已配置多个峰谷的模型正确展示徽章并在删除其中一个后正确保存", async () => {
    const user = userEvent.setup();
    mockPrices([
      price({
        upstream_model: "gpt-4o",
        off_peaks: [
          {
            start_time: "00:00",
            end_time: "08:00",
            input: 0.5,
            cache_read: 0.05,
            cache_write: 0.1,
            output: 1.0,
          },
          {
            start_time: "12:00",
            end_time: "14:00",
            input: 0.8,
            cache_read: 0.08,
            cache_write: 0.15,
            output: 1.5,
          },
        ],
      }),
    ]);

    renderWithProviders(<ModelPriceDialog open onOpenChange={() => {}} />);

    // Off-peak button displays active multi-window badge: 00:00-08:00 (+1)
    const multiBadge = await screen.findByRole("button", { name: /00:00-08:00 \(\+1\)/i });
    expect(multiBadge).toBeInTheDocument();

    // Click to expand
    await user.click(multiBadge);

    // Delete the second window
    const deleteBtns = screen.getAllByRole("button", { name: /Delete this off-peak window/i });
    expect(deleteBtns).toHaveLength(2);
    await user.click(deleteBtns[1]);

    // Save
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_model_prices_save", {
        prices: [
          {
            upstream_model: "gpt-4o",
            input: 1,
            cache_read: 0.1,
            cache_write: 0.2,
            output: 2,
            off_peaks: [
              {
                start_time: "00:00",
                end_time: "08:00",
                input: 0.5,
                cache_read: 0.05,
                cache_write: 0.1,
                output: 1.0,
              },
            ],
            off_peak: {
              start_time: "00:00",
              end_time: "08:00",
              input: 0.5,
              cache_read: 0.05,
              cache_write: 0.1,
              output: 1.0,
            },
          },
        ],
      }),
    );
  });

  it("已配置峰谷的模型正确还原并在关闭勾选后保存时不带 off_peak", async () => {
    const user = userEvent.setup();
    mockPrices([
      price({
        upstream_model: "gpt-4o",
        off_peak: {
          start_time: "00:30",
          end_time: "08:30",
          input: 0.5,
          cache_read: 0.05,
          cache_write: 0.1,
          output: 1.0,
        },
      }),
    ]);

    renderWithProviders(
      <ModelPriceDialog open onOpenChange={() => {}} />,
    );

    // Off-peak button displays active time range badge
    const activeBadge = await screen.findByRole("button", { name: /00:30-08:30/i });
    expect(activeBadge).toBeInTheDocument();

    // Click to expand
    await user.click(activeBadge);

    // Uncheck "Enable off-peak pricing"
    const enableCheckbox = screen.getByRole("checkbox", { name: /Enable off-peak pricing/i });
    expect(enableCheckbox).toBeChecked();
    await user.click(enableCheckbox);
    expect(enableCheckbox).not.toBeChecked();

    // Save
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_model_prices_save", {
        prices: [
          {
            upstream_model: "gpt-4o",
            input: 1,
            cache_read: 0.1,
            cache_write: 0.2,
            output: 2,
          },
        ],
      }),
    );
  });
});
