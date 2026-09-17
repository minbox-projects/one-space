import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import i18n from "@/i18n";
import { UsageStatsPanel } from "@/components/ApiFusion/UsageStatsPanel";
import type { UsageStats } from "@/lib/apiFusion";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

function metrics(overrides: Partial<UsageStats> = {}): UsageStats {
  return {
    request_count: 0,
    input_tokens: 0,
    cache_read_tokens: 0,
    cache_write_tokens: 0,
    output_tokens: 0,
    total_tokens: 0,
    amount: 0,
    unpriced_count: 0,
    granularity: "hour",
    buckets: [],
    models: [],
    ...overrides,
  };
}

function formatCount(value: number) {
  return new Intl.NumberFormat().format(value);
}

describe("UsageStatsPanel", () => {
  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
  });

  it("默认今日并在切范围时按 days 重新取数，卡片展示 Tokens/请求数/花费", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command !== "api_fusion_usage_stats") {
        throw new Error(`Unhandled command: ${command}`);
      }
      const days = args.days as number | null;
      return metrics({
        request_count: days === 1 ? 4 : 10,
        total_tokens: days === 1 ? 1000 : 2500,
        amount: 0.1234,
      });
    });

    renderWithProviders(<UsageStatsPanel />);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_usage_stats", {
        days: 1,
      }),
    );
    const tokensCard = await screen.findByTestId("api-fusion-usage-card-tokens");
    expect(tokensCard).toHaveTextContent(formatCount(1000));
    expect(screen.getByTestId("api-fusion-usage-card-requests")).toHaveTextContent(
      formatCount(4),
    );
    expect(screen.getByTestId("api-fusion-usage-card-cost")).toHaveTextContent(
      "0.1234",
    );

    await user.click(screen.getByRole("button", { name: "7d" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_usage_stats", {
        days: 7,
      }),
    );
    expect(
      await screen.findByTestId("api-fusion-usage-card-requests"),
    ).toHaveTextContent(formatCount(10));
  });

  it("刷新按钮在请求期间展示刷新中状态并重新取数", async () => {
    const user = userEvent.setup();
    let deferredResolve: (value: UsageStats) => void = () => {};
    let calls = 0;
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_fusion_usage_stats") {
        throw new Error(`Unhandled command: ${command}`);
      }
      calls += 1;
      if (calls === 1) {
        return metrics({ request_count: 1 });
      }
      return new Promise<UsageStats>((resolve) => {
        deferredResolve = resolve;
      });
    });

    renderWithProviders(<UsageStatsPanel />);
    await screen.findByTestId("api-fusion-usage-card-requests");

    await user.click(screen.getByRole("button", { name: "Refresh" }));
    expect(await screen.findByText("Refreshing...")).toBeInTheDocument();

    await act(async () => {
      deferredResolve(metrics({ request_count: 2 }));
    });
    await waitFor(() =>
      expect(
        screen.getByTestId("api-fusion-usage-card-requests"),
      ).toHaveTextContent(formatCount(2)),
    );
    expect(screen.queryByText("Refreshing...")).not.toBeInTheDocument();
  });

  it("未定价模型行显示破折号且不计入合计，同时提示未定价请求数", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_fusion_usage_stats") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return metrics({
        request_count: 3,
        unpriced_count: 3,
        amount: 0,
        models: [
          {
            local_model: "local-x",
            request_count: 3,
            input_tokens: 10,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            output_tokens: 20,
            total_tokens: 30,
            amount: 0,
            unpriced_count: 3,
            providers: [
              {
                provider_id: "p1",
                provider_name: "Provider A",
                request_count: 3,
                input_tokens: 10,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                output_tokens: 20,
                total_tokens: 30,
                amount: 0,
                unpriced_count: 3,
              },
            ],
          },
        ],
      });
    });

    renderWithProviders(<UsageStatsPanel />);

    const modelTable = await screen.findByTestId("api-fusion-usage-models");
    expect(within(modelTable).getAllByText("—").length).toBeGreaterThanOrEqual(2);
    expect(screen.getByTestId("api-fusion-usage-card-cost")).toHaveTextContent(
      "0.0000",
    );
    expect(
      screen.getByText(/3 requests have no configured price/),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("api-fusion-usage-unpriced-hint"),
    ).toHaveTextContent("3");
  });

  it("模型行只展示范围内实际调用过的服务商明细", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_fusion_usage_stats") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return metrics({
        request_count: 2,
        total_tokens: 60,
        models: [
          {
            local_model: "shared-local",
            request_count: 2,
            input_tokens: 20,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            output_tokens: 40,
            total_tokens: 60,
            amount: 0.5,
            unpriced_count: 0,
            providers: [
              {
                provider_id: "called",
                provider_name: "Called Provider",
                request_count: 2,
                input_tokens: 20,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                output_tokens: 40,
                total_tokens: 60,
                amount: 0.5,
                unpriced_count: 0,
              },
            ],
          },
        ],
      });
    });

    renderWithProviders(<UsageStatsPanel />);

    const modelTable = await screen.findByTestId("api-fusion-usage-models");
    expect(within(modelTable).getByText("Called Provider")).toBeInTheDocument();
    expect(within(modelTable).queryByText("Never Called Provider")).not.toBeInTheDocument();
  });

  it("单日按 UTC+8 小时展示有数据的小时且不超过 24 行", async () => {
    const buckets = Array.from({ length: 26 }, (_, index) => ({
      label: `${String(index).padStart(2, "0")}:00`,
      request_count: index % 13 === 0 ? 0 : 1,
      input_tokens: 1,
      cache_read_tokens: 0,
      cache_write_tokens: 0,
      output_tokens: 1,
      total_tokens: 2,
      amount: 0.01,
      unpriced_count: 0,
    }));
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_fusion_usage_stats") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return metrics({ request_count: 24, granularity: "hour", buckets });
    });

    renderWithProviders(<UsageStatsPanel />);

    const bucketTable = await screen.findByTestId("api-fusion-usage-buckets");
    const rows = within(bucketTable).getAllByTestId("api-fusion-usage-bucket-row");
    expect(rows).toHaveLength(24);
    expect(within(bucketTable).queryByText("00:00")).not.toBeInTheDocument();
  });

  it("多日或全部范围按自然日展示", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_fusion_usage_stats") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return metrics({
        request_count: 2,
        granularity: "day",
        buckets: [
          {
            label: "2026-09-16",
            request_count: 1,
            input_tokens: 1,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            output_tokens: 1,
            total_tokens: 2,
            amount: 0.01,
            unpriced_count: 0,
          },
          {
            label: "2026-09-17",
            request_count: 1,
            input_tokens: 1,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            output_tokens: 1,
            total_tokens: 2,
            amount: 0.01,
            unpriced_count: 0,
          },
        ],
      });
    });

    renderWithProviders(<UsageStatsPanel />);

    const bucketTable = await screen.findByTestId("api-fusion-usage-buckets");
    expect(within(bucketTable).getByText("2026-09-16")).toBeInTheDocument();
    expect(within(bucketTable).getByText("2026-09-17")).toBeInTheDocument();
  });

  it("页内模型价格入口打开独立弹窗并向后端读取价格", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "api_fusion_usage_stats") return metrics({ request_count: 1 });
      if (command === "api_fusion_model_prices_get") return [];
      throw new Error(`Unhandled command: ${command}`);
    });

    renderWithProviders(<UsageStatsPanel />);
    await screen.findByTestId("api-fusion-usage-card-requests");

    await user.click(screen.getByRole("button", { name: "Model prices" }));

    expect(
      await screen.findByTestId("api-fusion-model-price-dialog"),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_model_prices_get"),
    );
  });

  it("范围内无记录时显示空状态且不报错", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_fusion_usage_stats") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return metrics();
    });

    renderWithProviders(<UsageStatsPanel />);

    expect(
      await screen.findByText("No usage records in this range."),
    ).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("隐藏时（isActive=false）不发起用量请求", async () => {
    invokeMock.mockImplementation(async () => metrics());

    renderWithProviders(<UsageStatsPanel isActive={false} />);

    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(invokeMock).not.toHaveBeenCalledWith(
      "api_fusion_usage_stats",
      expect.anything(),
    );
  });

  it("每个模型的提供商明细紧跟其模型行之后渲染", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_fusion_usage_stats") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return metrics({
        request_count: 4,
        total_tokens: 60,
        models: [
          {
            local_model: "alpha-model",
            request_count: 2,
            input_tokens: 10,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            output_tokens: 20,
            total_tokens: 30,
            amount: 0.1,
            unpriced_count: 0,
            providers: [
              {
                provider_id: "p-alpha",
                provider_name: "Alpha Provider",
                request_count: 2,
                input_tokens: 10,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                output_tokens: 20,
                total_tokens: 30,
                amount: 0.1,
                unpriced_count: 0,
              },
            ],
          },
          {
            local_model: "beta-model",
            request_count: 2,
            input_tokens: 10,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            output_tokens: 20,
            total_tokens: 30,
            amount: 0.2,
            unpriced_count: 0,
            providers: [
              {
                provider_id: "p-beta",
                provider_name: "Beta Provider",
                request_count: 2,
                input_tokens: 10,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                output_tokens: 20,
                total_tokens: 30,
                amount: 0.2,
                unpriced_count: 0,
              },
            ],
          },
        ],
      });
    });

    renderWithProviders(<UsageStatsPanel />);

    const modelTable = await screen.findByTestId("api-fusion-usage-models");
    const rows = within(modelTable).getAllByTestId(
      /api-fusion-usage-(model|provider)-row/,
    );
    const labels = rows.map(
      (row) => within(row).getAllByRole("cell")[0].textContent,
    );
    expect(labels).toEqual([
      "alpha-model",
      "Alpha Provider",
      "beta-model",
      "Beta Provider",
    ]);
  });
});
