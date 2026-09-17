import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { ModelPriceDialog } from "@/components/ApiFusion/ModelPriceDialog";
import type { ModelPrice } from "@/lib/apiFusion";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

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
      case "api_fusion_model_prices_get":
        return store;
      case "api_fusion_model_prices_save":
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
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_model_prices_save", {
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
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_model_prices_save", {
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
      if (command === "api_fusion_model_prices_get") return [];
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
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_model_prices_save", {
        prices: [price()],
      }),
    );
    const commands = invokeMock.mock.calls.map(([command]) => command);
    expect(commands).not.toContain("api_fusion_save_config");
    expect(commands).not.toContain("api_fusion_upsert_provider");
    expect(commands).not.toContain("api_fusion_upsert_key");
    expect(commands).not.toContain("api_fusion_sync_terminal");
    expect(commands).not.toContain("api_fusion_usage_retention_save");
    expect(commands).not.toContain("save_storage_config");
  });

  it("保存失败时展示可操作错误且不标记成功", async () => {
    const user = userEvent.setup();
    const onSaved = vi.fn();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "api_fusion_model_prices_get") return [price()];
      if (command === "api_fusion_model_prices_save") {
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
      if (command === "api_fusion_model_prices_get") return [price()];
      throw new Error(`Unhandled command: ${command}`);
    });

    renderWithProviders(
      <ModelPriceDialog open={false} onOpenChange={() => {}} />,
    );

    expect(
      screen.queryByTestId("api-fusion-model-price-dialog"),
    ).not.toBeInTheDocument();
  });

  it("价格输入按四档单价渲染且单位为美元/百万 tokens", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "api_fusion_model_prices_get") return [price()];
      throw new Error(`Unhandled command: ${command}`);
    });

    renderWithProviders(
      <ModelPriceDialog open onOpenChange={() => {}} />,
    );
    const dialog = await screen.findByTestId("api-fusion-model-price-dialog");
    const scoped = within(dialog);
    expect(scoped.getByLabelText("Input")).toBeInTheDocument();
    expect(scoped.getByLabelText("Cache read")).toBeInTheDocument();
    expect(scoped.getByLabelText("Cache write")).toBeInTheDocument();
    expect(scoped.getByLabelText("Output")).toBeInTheDocument();
    expect(scoped.getAllByText(/USD \/ million tokens/).length).toBeGreaterThan(0);
  });
});
