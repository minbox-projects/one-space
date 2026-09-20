import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import i18n from "@/i18n";
import { UsageStatsPanel } from "@/components/ApiGateway/UsageStatsPanel";
import type { UsageStats } from "@/lib/apiGateway";
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
      if (command !== "api_gateway_usage_stats") {
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
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_usage_stats", {
        days: 1,
      }),
    );
    const tokensCard = await screen.findByTestId("api-gateway-usage-card-tokens");
    expect(tokensCard).toHaveTextContent(formatCount(1000));
    expect(screen.getByTestId("api-gateway-usage-card-requests")).toHaveTextContent(
      formatCount(4),
    );
    expect(screen.getByTestId("api-gateway-usage-card-cost")).toHaveTextContent(
      "0.1234",
    );

    const rangeTrigger = screen.getByTestId("api-gateway-usage-range-trigger");
    expect(rangeTrigger).toHaveTextContent("Today");
    await user.click(rangeTrigger);
    await user.click(screen.getByRole("option", { name: "7d" }));
    expect(rangeTrigger).toHaveTextContent("7d");
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_usage_stats", {
        days: 7,
      }),
    );
    expect(
      await screen.findByTestId("api-gateway-usage-card-requests"),
    ).toHaveTextContent(formatCount(10));
  });

  it("刷新按钮在请求期间展示刷新中状态并重新取数", async () => {
    const user = userEvent.setup();
    let deferredResolve: (value: UsageStats) => void = () => {};
    let calls = 0;
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_usage_stats") {
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
    await screen.findByTestId("api-gateway-usage-card-requests");

    await user.click(screen.getByRole("button", { name: "Refresh" }));
    expect(await screen.findByText("Refreshing...")).toBeInTheDocument();

    await act(async () => {
      deferredResolve(metrics({ request_count: 2 }));
    });
    await waitFor(() =>
      expect(
        screen.getByTestId("api-gateway-usage-card-requests"),
      ).toHaveTextContent(formatCount(2)),
    );
    expect(screen.queryByText("Refreshing...")).not.toBeInTheDocument();
  });

  it("未定价模型行显示破折号且不计入合计，同时提示未定价请求数", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_usage_stats") {
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

    const modelTable = await screen.findByTestId("api-gateway-usage-models");
    expect(within(modelTable).getAllByText("—").length).toBeGreaterThanOrEqual(2);
    expect(screen.getByTestId("api-gateway-usage-card-cost")).toHaveTextContent(
      "0.0000",
    );
    expect(
      screen.getByText(/3 requests have no configured price/),
    ).toBeInTheDocument();
    const hint = screen.getByTestId("api-gateway-usage-unpriced-hint");
    expect(hint).toHaveTextContent("3");
    // Fallback extracts from models when unpriced_items is absent
    expect(hint).toHaveTextContent("Provider A");
    expect(hint).toHaveTextContent("local-x");
  });

  it("未配置价格提示中展示服务商名称、模型 ID 以及上游模型和请求数", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_usage_stats") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return metrics({
        request_count: 5,
        unpriced_count: 5,
        unpriced_items: [
          {
            provider_id: "p-openai",
            provider_name: "OpenAI",
            local_model: "gpt-4o",
            upstream_model: "gpt-4o-2024-08-06",
            count: 3,
          },
          {
            provider_id: "p-deepseek",
            provider_name: "DeepSeek",
            local_model: "deepseek-chat",
            upstream_model: "deepseek-chat",
            count: 2,
          },
        ],
        models: [
          {
            local_model: "gpt-4o",
            request_count: 3,
            input_tokens: 100,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            output_tokens: 100,
            total_tokens: 200,
            amount: 0,
            unpriced_count: 3,
            providers: [
              {
                provider_id: "p-openai",
                provider_name: "OpenAI",
                request_count: 3,
                input_tokens: 100,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                output_tokens: 100,
                total_tokens: 200,
                amount: 0,
                unpriced_count: 3,
              },
            ],
          },
          {
            local_model: "deepseek-chat",
            request_count: 2,
            input_tokens: 50,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            output_tokens: 50,
            total_tokens: 100,
            amount: 0,
            unpriced_count: 2,
            providers: [
              {
                provider_id: "p-deepseek",
                provider_name: "DeepSeek",
                request_count: 2,
                input_tokens: 50,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                output_tokens: 50,
                total_tokens: 100,
                amount: 0,
                unpriced_count: 2,
              },
            ],
          },
        ],
      });
    });

    renderWithProviders(<UsageStatsPanel />);

    const hint = await screen.findByTestId("api-gateway-usage-unpriced-hint");
    expect(hint).toHaveTextContent("5 requests have no configured price");
    expect(hint).toHaveTextContent("Unconfigured models:");

    const items = within(hint).getAllByTestId("api-gateway-usage-unpriced-item");
    expect(items).toHaveLength(2);

    // Item 1: OpenAI / gpt-4o (gpt-4o-2024-08-06) (3)
    expect(items[0]).toHaveTextContent("OpenAI");
    expect(items[0]).toHaveTextContent("gpt-4o");
    expect(items[0]).toHaveTextContent("gpt-4o-2024-08-06");
    expect(items[0]).toHaveTextContent("(3)");

    // Item 2: DeepSeek / deepseek-chat (2) (upstream matches local, no duplicate upstream displayed)
    expect(items[1]).toHaveTextContent("DeepSeek");
    expect(items[1]).toHaveTextContent("deepseek-chat");
    expect(items[1]).toHaveTextContent("(2)");
  });

  it("中文环境下展示未配置价格的模型提示与服务商", async () => {
    await i18n.changeLanguage("zh");
    invokeMock.mockImplementation(async () => {
      return metrics({
        request_count: 2,
        unpriced_count: 2,
        unpriced_items: [
          {
            provider_id: "p-deepseek",
            provider_name: "DeepSeek",
            local_model: "deepseek-chat",
            upstream_model: "deepseek-chat",
            count: 2,
          },
        ],
      });
    });

    renderWithProviders(<UsageStatsPanel />);

    const hint = await screen.findByTestId("api-gateway-usage-unpriced-hint");
    expect(hint).toHaveTextContent("2 条请求未配置价格，未计入合计。");
    expect(hint).toHaveTextContent("未配置价格模型：");
    expect(hint).toHaveTextContent("DeepSeek");
    expect(hint).toHaveTextContent("deepseek-chat");
    expect(hint).toHaveTextContent("(2)");
  });

  it("模型行只展示范围内实际调用过的服务商明细", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_usage_stats") {
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

    const modelTable = await screen.findByTestId("api-gateway-usage-models");
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
      if (command !== "api_gateway_usage_stats") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return metrics({ request_count: 24, granularity: "hour", buckets });
    });

    renderWithProviders(<UsageStatsPanel />);

    const bucketTable = await screen.findByTestId("api-gateway-usage-buckets");
    const rows = within(bucketTable).getAllByTestId("api-gateway-usage-bucket-row");
    expect(rows).toHaveLength(24);
    expect(within(bucketTable).queryByText("00:00")).not.toBeInTheDocument();
  });

  it("多日或全部范围按自然日展示", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_usage_stats") {
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

    const bucketTable = await screen.findByTestId("api-gateway-usage-buckets");
    expect(within(bucketTable).getByText("2026-09-16")).toBeInTheDocument();
    expect(within(bucketTable).getByText("2026-09-17")).toBeInTheDocument();
  });

  it("usage_stats_panel_has_no_model_prices_entry", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_usage_stats") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return metrics({ request_count: 1 });
    });

    renderWithProviders(<UsageStatsPanel />);
    await screen.findByTestId("api-gateway-usage-card-requests");

    expect(
      screen.queryByRole("button", { name: "Model prices" }),
    ).toBeNull();
    expect(
      screen.queryByTestId("api-gateway-model-price-dialog"),
    ).toBeNull();
    expect(invokeMock).not.toHaveBeenCalledWith("api_gateway_model_prices_get");
  });

  it("范围内无记录时显示空状态且不报错", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_usage_stats") {
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
      "api_gateway_usage_stats",
      expect.anything(),
    );
  });

  it("每个模型的提供商明细紧跟其模型行之后渲染", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_usage_stats") {
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

    const modelTable = await screen.findByTestId("api-gateway-usage-models");
    const rows = within(modelTable).getAllByTestId(
      /api-gateway-usage-(model|provider)-row/,
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

  it("Tokens 卡片、时间分布与用量分析行正确应用单位转换格式并展示完整数值 title", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_usage_stats") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return metrics({
        request_count: 50,
        total_tokens: 15_200_000,
        amount: 1.25,
        buckets: [
          {
            label: "10:00",
            request_count: 10,
            input_tokens: 1_000_000,
            cache_read_tokens: 500_000,
            cache_write_tokens: 0,
            output_tokens: 1_000_000,
            total_tokens: 2_500_000,
            amount: 0.25,
            unpriced_count: 0,
          },
        ],
        models: [
          {
            local_model: "gpt-4o",
            request_count: 50,
            input_tokens: 120_000,
            cache_read_tokens: 35_000,
            cache_write_tokens: 100_000_000,
            output_tokens: 800,
            total_tokens: 15_200_000,
            amount: 1.25,
            unpriced_count: 0,
            providers: [],
          },
        ],
      });
    });

    renderWithProviders(<UsageStatsPanel />);

    // 顶部卡片
    const tokensCard = await screen.findByTestId("api-gateway-usage-card-tokens");
    expect(tokensCard).toHaveTextContent("1.5千万");
    const tokensVal = tokensCard.querySelector(".text-lg");
    expect(tokensVal).toHaveAttribute("title", "15,200,000");

    // 时间分布表格
    const bucketTable = screen.getByTestId("api-gateway-usage-buckets");
    const bucketRow = within(bucketTable).getByTestId("api-gateway-usage-bucket-row");
    const bucketCells = within(bucketRow).getAllByRole("cell");
    expect(bucketCells[2]).toHaveTextContent("2.5百万");
    expect(bucketCells[2]).toHaveAttribute("title", "2,500,000");

    // 模型分析表格
    const modelTable = screen.getByTestId("api-gateway-usage-models");
    const modelRow = within(modelTable).getByTestId("api-gateway-usage-model-row");
    const modelCells = within(modelRow).getAllByRole("cell");
    // [0]=Label, [1]=Requests, [2]=Input, [3]=CacheRead, [4]=CacheWrite, [5]=Output, [6]=Cost
    expect(modelCells[1]).toHaveTextContent("50"); // 请求数不带 token转换
    expect(modelCells[2]).toHaveTextContent("12万");
    expect(modelCells[2]).toHaveAttribute("title", "120,000");
    expect(modelCells[3]).toHaveTextContent("3.5万");
    expect(modelCells[3]).toHaveAttribute("title", "35,000");
    expect(modelCells[4]).toHaveTextContent("1亿");
    expect(modelCells[4]).toHaveAttribute("title", "100,000,000");
    expect(modelCells[5]).toHaveTextContent("800");
    expect(modelCells[5]).toHaveAttribute("title", "800");
  });

  it("展示细分指标卡片（缓存命中率、Input、Output、Cache Read、Cache Write）", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_usage_stats") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return metrics({
        request_count: 10,
        total_tokens: 1000,
        input_tokens: 300,
        cache_read_tokens: 100,
        cache_write_tokens: 50,
        output_tokens: 550,
        amount: 0.05,
      });
    });

    renderWithProviders(<UsageStatsPanel />);

    // 平均缓存命中率: 100 / (300 + 100) = 25%
    const cacheHitCard = await screen.findByTestId("api-gateway-usage-card-cache-hit");
    expect(cacheHitCard).toHaveTextContent("25%");

    const inputCard = screen.getByTestId("api-gateway-usage-card-input");
    expect(inputCard).toHaveTextContent("300");

    const cacheReadCard = screen.getByTestId("api-gateway-usage-card-cache-read");
    expect(cacheReadCard).toHaveTextContent("100");

    const cacheWriteCard = screen.getByTestId("api-gateway-usage-card-cache-write");
    expect(cacheWriteCard).toHaveTextContent("50");

    const outputCard = screen.getByTestId("api-gateway-usage-card-output");
    expect(outputCard).toHaveTextContent("550");
  });

  it("渲染时间趋势柱状图并在切换维度时更新柱条", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_usage_stats") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return metrics({
        request_count: 5,
        total_tokens: 500,
        amount: 0.15,
        granularity: "day",
        buckets: [
          {
            label: "2026-09-19",
            request_count: 2,
            input_tokens: 50,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            output_tokens: 50,
            total_tokens: 100,
            amount: 0.05,
            unpriced_count: 0,
          },
          {
            label: "2026-09-20",
            request_count: 3,
            input_tokens: 150,
            cache_read_tokens: 50,
            cache_write_tokens: 0,
            output_tokens: 200,
            total_tokens: 400,
            amount: 0.1,
            unpriced_count: 0,
          },
        ],
      });
    });

    renderWithProviders(<UsageStatsPanel />);

    const trendCard = await screen.findByTestId("api-gateway-usage-trend-card");
    expect(trendCard).toBeInTheDocument();

    // 峰值徽标应该指示最高的一天 2026-09-20
    const peakBadge = screen.getByTestId("api-gateway-usage-peak-badge");
    expect(peakBadge).toHaveTextContent("2026-09-20");

    const bars = screen.getAllByTestId("api-gateway-usage-trend-bar");
    expect(bars).toHaveLength(2);

    // 切换到 Requests 维度
    const requestsBtn = screen.getByTestId("api-gateway-trend-view-requests");
    await user.click(requestsBtn);

    // 切换到 Cost 维度
    const costBtn = screen.getByTestId("api-gateway-trend-view-cost");
    await user.click(costBtn);
    expect(bars[1]).toHaveClass("bg-emerald-600/70");

    // 切回 Tokens 维度
    const tokensBtn = screen.getByTestId("api-gateway-trend-view-tokens");
    await user.click(tokensBtn);
    expect(bars[1]).toHaveClass("bg-primary/70");
  });

  it("支持展开和收起时间分布详细明细表格", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_usage_stats") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return metrics({
        request_count: 2,
        granularity: "day",
        buckets: [
          {
            label: "2026-09-20",
            request_count: 2,
            input_tokens: 10,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            output_tokens: 20,
            total_tokens: 30,
            amount: 0.01,
            unpriced_count: 0,
          },
        ],
      });
    });

    renderWithProviders(<UsageStatsPanel />);

    // 初始状态下表格可见
    expect(await screen.findByTestId("api-gateway-usage-buckets")).toBeInTheDocument();

    const toggleBtn = screen.getByTestId("api-gateway-toggle-bucket-table");
    expect(toggleBtn).toHaveTextContent("Hide Details");

    // 点击收起
    await user.click(toggleBtn);
    expect(screen.queryByTestId("api-gateway-usage-buckets")).not.toBeInTheDocument();
    expect(toggleBtn).toHaveTextContent("Show Details");

    // 点击再次展开
    await user.click(toggleBtn);
    expect(screen.getByTestId("api-gateway-usage-buckets")).toBeInTheDocument();
  });

  it("模型与服务商分析行中展示占比进度条", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_usage_stats") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return metrics({
        request_count: 10,
        total_tokens: 1000,
        models: [
          {
            local_model: "gpt-4o",
            request_count: 8,
            input_tokens: 200,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            output_tokens: 600,
            total_tokens: 800,
            amount: 0.4,
            unpriced_count: 0,
            providers: [
              {
                provider_id: "prov-1",
                provider_name: "Provider 1",
                request_count: 8,
                input_tokens: 200,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                output_tokens: 600,
                total_tokens: 800,
                amount: 0.4,
                unpriced_count: 0,
              },
            ],
          },
          {
            local_model: "claude-3-5",
            request_count: 2,
            input_tokens: 50,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            output_tokens: 150,
            total_tokens: 200,
            amount: 0.1,
            unpriced_count: 0,
            providers: [],
          },
        ],
      });
    });

    renderWithProviders(<UsageStatsPanel />);

    const shareBars = await screen.findAllByTestId("api-gateway-usage-share-bar");
    // gpt-4o (80%), Provider 1 (100% of gpt-4o), claude-3-5 (20%)
    expect(shareBars).toHaveLength(3);
    expect(shareBars[0]).toHaveAttribute("aria-label", "80%");
    expect(shareBars[1]).toHaveAttribute("aria-label", "100%");
    expect(shareBars[2]).toHaveAttribute("aria-label", "20%");
  });

  it("用量分析列表中展示缓存命中率列并正确计算百分比", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_usage_stats") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return metrics({
        request_count: 5,
        total_tokens: 500,
        models: [
          {
            local_model: "claude-3-7-sonnet",
            request_count: 5,
            input_tokens: 300,
            cache_read_tokens: 100,
            cache_write_tokens: 50,
            output_tokens: 50,
            total_tokens: 500,
            amount: 0.25,
            unpriced_count: 0,
            providers: [
              {
                provider_id: "prov-anthropic",
                provider_name: "Anthropic Direct",
                request_count: 5,
                input_tokens: 300,
                cache_read_tokens: 100,
                cache_write_tokens: 50,
                output_tokens: 50,
                total_tokens: 500,
                amount: 0.25,
                unpriced_count: 0,
              },
            ],
          },
        ],
      });
    });

    renderWithProviders(<UsageStatsPanel />);

    const modelTable = await screen.findByTestId("api-gateway-usage-models");
    // 表头包含 Cache hit
    expect(within(modelTable).getByRole("columnheader", { name: "Cache hit" })).toBeInTheDocument();

    // 模型行缓存命中率: 100 / (300 + 100) = 25%
    const modelCacheHitCell = screen.getByTestId("api-gateway-usage-model-cache-hit");
    expect(modelCacheHitCell).toHaveTextContent("25%");

    // 提供商行缓存命中率
    const providerCacheHitCell = screen.getByTestId("api-gateway-usage-provider-cache-hit");
    expect(providerCacheHitCell).toHaveTextContent("25%");
  });

  it("脏数据中空服务商行不渲染，含数据的真实服务商行仍保留", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_usage_stats") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return metrics({
        request_count: 3,
        total_tokens: 32,
        models: [
          {
            // 旧库脏数据：真实服务商行与 provider_id=''/provider_name='' 的空白行并存。
            local_model: "local-good",
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
                provider_id: "prov-a",
                provider_name: "Provider A",
                request_count: 2,
                input_tokens: 10,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                output_tokens: 20,
                total_tokens: 30,
                amount: 0.1,
                unpriced_count: 0,
              },
              {
                provider_id: "",
                provider_name: "",
                request_count: 0,
                input_tokens: 0,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                amount: 0,
                unpriced_count: 0,
              },
            ],
          },
          {
            // 仅有空白服务商行的模型：模型行本身有数据，不能被过滤掉。
            local_model: "local-dirty-only",
            request_count: 1,
            input_tokens: 1,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            output_tokens: 1,
            total_tokens: 2,
            amount: 0,
            unpriced_count: 0,
            providers: [
              {
                provider_id: "",
                provider_name: "",
                request_count: 1,
                input_tokens: 1,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                output_tokens: 1,
                total_tokens: 2,
                amount: 0,
                unpriced_count: 0,
              },
            ],
          },
        ],
      });
    });

    renderWithProviders(<UsageStatsPanel />);

    const modelTable = await screen.findByTestId("api-gateway-usage-models");
    // 含数据的模型行全部保留，空白服务商行不得把它一起隐藏。
    expect(
      within(modelTable).getAllByTestId("api-gateway-usage-model-row"),
    ).toHaveLength(2);
    expect(within(modelTable).getByText("local-dirty-only")).toBeInTheDocument();

    // 只保留 prov-a 这一条真实服务商行，空 provider 行不渲染。
    const providerRows = within(modelTable).queryAllByTestId(
      "api-gateway-usage-provider-row",
    );
    expect(providerRows).toHaveLength(1);
    expect(providerRows[0]).toHaveTextContent("Provider A");
  });
});


