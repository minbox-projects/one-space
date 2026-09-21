import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import i18n from "@/i18n";
import { UsageLogsPanel } from "@/components/ApiGateway/UsageLogsPanel";
import type {
  UsageLogRecord,
  UsageLogsPage,
} from "@/lib/apiGateway";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

function record(overrides: Partial<UsageLogRecord> = {}): UsageLogRecord {
  return {
    timestamp_ms: Date.UTC(2026, 8, 17, 2, 30),
    local_model: "gpt-4o",
    upstream_model: "gpt-4o",
    provider_id: "p1",
    provider_name: "Provider A",
    result: "success",
    status: 200,
    input_tokens: 10,
    cache_read_tokens: 1,
    cache_write_tokens: 2,
    output_tokens: 20,
    total_tokens: 33,
    amount: 0.5,
    duration_ms: 120,
    ...overrides,
  };
}

function page(overrides: Partial<UsageLogsPage> = {}): UsageLogsPage {
  return {
    page: 1,
    page_size: 50,
    total: 1,
    total_pages: 1,
    group_by: null,
    records: [record()],
    groups: [],
    ...overrides,
  };
}

const STORED_UPSTREAM_MESSAGE =
  "upstream provider error: the requested model is temporarily overloaded on this provider, retry after a short delay";

const GENERIC_502_REASON = "Bad gateway / Upstream unavailable";
const GENERIC_502_TITLE = `HTTP 502: ${GENERIC_502_REASON}`;

/** The reason line (or its wrapper) must be reachable with the keyboard to reveal the tooltip. */
function expectKeyboardFocusable(element: HTMLElement) {
  const focusable = element.closest<HTMLElement>(
    "[tabindex], button, a[href], input, select, textarea",
  );
  expect(
    focusable,
    "错误原因行（或其包装元素）应可通过键盘聚焦以显示提示",
  ).not.toBeNull();
  if (focusable?.hasAttribute("tabindex")) {
    expect(Number(focusable.getAttribute("tabindex"))).toBeGreaterThanOrEqual(0);
  }
}

describe("UsageLogsPanel", () => {
  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
  });

  it("默认今日、不分组并以第 1 页取数，切换范围回到第 1 页", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({ total: 120, total_pages: 3 });
    });

    renderWithProviders(<UsageLogsPanel />);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_request_logs", {
        days: 1,
        groupBy: "none",
        status: null,
        model: null,
        page: 1,
      }),
    );

    const rangeTrigger = screen.getByTestId("api-gateway-logs-range-trigger");
    expect(rangeTrigger).toHaveTextContent("Today");
    await user.click(rangeTrigger);
    await user.click(screen.getByRole("option", { name: "30d" }));
    expect(rangeTrigger).toHaveTextContent("30d");
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_request_logs", {
        days: 30,
        groupBy: "none",
        status: null,
        model: null,
        page: 1,
      }),
    );
  });

  it("刷新按钮在请求期间展示刷新中状态", async () => {
    const user = userEvent.setup();
    let calls = 0;
    let resolveSecond: (value: UsageLogsPage) => void = () => {};
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      calls += 1;
      if (calls === 1) return page();
      return new Promise<UsageLogsPage>((resolve) => {
        resolveSecond = resolve;
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("api-gateway-logs-ungrouped");

    await user.click(screen.getByRole("button", { name: "Refresh" }));
    expect(await screen.findByText("Refreshing...")).toBeInTheDocument();

    await act(async () => {
      resolveSecond(page());
    });
    await waitFor(() =>
      expect(screen.queryByText("Refreshing...")).not.toBeInTheDocument(),
    );
  });

  it("不分组表格按时间倒序展示时间/状态/模型/Tokens/花费", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        total: 3,
        records: [
          record({
            timestamp_ms: Date.UTC(2026, 8, 17, 4, 0),
            local_model: "newest",
            duration_ms: 62000,
          }),
          record({
            timestamp_ms: Date.UTC(2026, 8, 17, 2, 0),
            local_model: "middle",
            duration_ms: 2000,
          }),
          record({
            timestamp_ms: Date.UTC(2026, 8, 16, 18, 0),
            local_model: "oldest",
            duration_ms: 500,
          }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);

    const table = await screen.findByTestId("api-gateway-logs-ungrouped");
    expect(within(table).getByText("Time")).toBeInTheDocument();
    expect(within(table).getByText("Status")).toBeInTheDocument();
    expect(within(table).getByText("Model")).toBeInTheDocument();
    expect(within(table).getByText("Duration")).toBeInTheDocument();
    expect(within(table).getByText("Tokens")).toBeInTheDocument();
    expect(within(table).getByText("Cost ($)")).toBeInTheDocument();

    const ths = table.querySelectorAll("th");
    expect(ths.length).toBe(6);
    ths.forEach((th) => {
      expect(th).toHaveClass("text-left");
      expect(th).toHaveClass("whitespace-nowrap");
    });

    const rows = within(table).getAllByTestId("api-gateway-logs-row");
    expect(rows).toHaveLength(3);
    // UTC+8 display: 2026-09-17T18:00Z is 2026-09-18 02:00.
    expect(within(rows[0]).getByText("2026-09-17 12:00")).toBeInTheDocument();
    expect(within(rows[0]).getByText("Success")).toBeInTheDocument();
    expect(within(rows[0]).getByTestId("api-gateway-logs-status-badge")).toHaveClass("bg-emerald-500/10");
    expect(within(rows[0]).getByText("newest")).toBeInTheDocument();
    expect(
      within(rows[0]).getByTestId("api-gateway-logs-duration-cell"),
    ).toHaveTextContent("1m 2s");
    expect(
      within(rows[0]).getByTestId("api-gateway-logs-duration-cell"),
    ).toHaveClass("text-rose-500");
    expect(
      within(rows[1]).getByTestId("api-gateway-logs-duration-cell"),
    ).toHaveTextContent("2s");
    expect(
      within(rows[1]).getByTestId("api-gateway-logs-duration-cell"),
    ).toHaveClass("text-emerald-500");
    expect(
      within(rows[2]).getByTestId("api-gateway-logs-duration-cell"),
    ).toHaveTextContent("1s");
    expect(
      within(rows[2]).getByTestId("api-gateway-logs-duration-cell"),
    ).toHaveClass("text-emerald-500");
  });

  it("中文环境下不分组表格展示中文列头耗时", async () => {
    await i18n.changeLanguage("zh");
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        total: 1,
        records: [record({ duration_ms: 65000 })],
      });
    });

    renderWithProviders(<UsageLogsPanel />);

    const table = await screen.findByTestId("api-gateway-logs-ungrouped");
    expect(within(table).getByText("耗时")).toBeInTheDocument();
    expect(
      within(table).getByTestId("api-gateway-logs-duration-cell"),
    ).toHaveTextContent("1m 5s");
  });

  it("切换为 Day（UTC+8）分组展示分组列且错误数不含 cancelled", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      if (args.groupBy === "day") {
        return page({
          group_by: "day",
          records: [],
          total: 3,
          groups: [
            {
              group: "2026-09-17",
              request_count: 3,
              // 3 requests: 1 failure + 1 success + 1 cancelled; cancelled is not an error.
              error_count: 1,
              last_request_at_ms: Date.UTC(2026, 8, 17, 5, 0),
            },
          ],
        });
      }
      return page();
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("api-gateway-logs-ungrouped");

    const groupTrigger = screen.getByTestId("api-gateway-logs-group-trigger");
    expect(groupTrigger).toHaveTextContent("No grouping");
    await user.click(groupTrigger);
    await user.click(screen.getByRole("option", { name: "Day (UTC+8)" }));
    expect(groupTrigger).toHaveTextContent("Day (UTC+8)");

    const grouped = await screen.findByTestId("api-gateway-logs-grouped");
    expect(within(grouped).getByText("Group")).toBeInTheDocument();
    expect(within(grouped).getByText("Errors")).toBeInTheDocument();

    const groupThs = grouped.querySelectorAll("th");
    expect(groupThs.length).toBe(4);
    groupThs.forEach((th) => {
      expect(th).toHaveClass("text-left");
      expect(th).toHaveClass("whitespace-nowrap");
    });
    const rows = within(grouped).getAllByTestId("api-gateway-logs-group-row");
    expect(rows).toHaveLength(1);
    expect(within(rows[0]).getByText("2026-09-17")).toBeInTheDocument();
    expect(within(rows[0]).getByText("3")).toBeInTheDocument();
    expect(within(rows[0]).getByText("1")).toBeInTheDocument();
    expect(within(rows[0]).getByText("2026-09-17 13:00")).toBeInTheDocument();
    expect(within(grouped).queryByText("Ungrouped")).not.toBeInTheDocument();
  });

  it("过滤面板为选择式且状态与模型条件可组合，应用后回到第 1 页", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page();
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("api-gateway-logs-ungrouped");

    await user.click(screen.getByRole("button", { name: "Filter" }));
    const panel = await screen.findByTestId("api-gateway-logs-filter-panel");
    expect(within(panel).queryByRole("combobox")).not.toBeInTheDocument();

    await user.click(within(panel).getByRole("button", { name: "Failure" }));
    await user.click(within(panel).getByRole("button", { name: "gpt-4o" }));
    await user.click(within(panel).getByRole("button", { name: "Apply" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_request_logs", {
        days: 1,
        groupBy: "none",
        status: "failure",
        model: "gpt-4o",
        page: 1,
      }),
    );
  });

  it("过滤面板仅展示 success 与 failure 状态选项，不包含已取消", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page();
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("api-gateway-logs-ungrouped");

    await user.click(screen.getByRole("button", { name: "Filter" }));
    const panel = await screen.findByTestId("api-gateway-logs-filter-panel");

    // Only success and failure status buttons must be present.
    expect(within(panel).getByRole("button", { name: "Success" })).toBeInTheDocument();
    expect(within(panel).getByRole("button", { name: "Failure" })).toBeInTheDocument();
    expect(
      within(panel).queryByRole("button", { name: "Cancelled" }),
    ).not.toBeInTheDocument();
  });

  it("筛选无匹配显示空状态且不报错", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      if (args.status === "failure") {
        return page({ total: 0, total_pages: 0, records: [] });
      }
      return page();
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("api-gateway-logs-ungrouped");

    await user.click(screen.getByRole("button", { name: "Filter" }));
    const panel = await screen.findByTestId("api-gateway-logs-filter-panel");
    await user.click(within(panel).getByRole("button", { name: "Failure" }));
    await user.click(within(panel).getByRole("button", { name: "Apply" }));

    expect(await screen.findByText("No matching requests.")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("每页 50 条、默认第 1 页并支持翻页", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        page: args.page as number,
        total: 120,
        total_pages: 3,
      });
    });

    renderWithProviders(<UsageLogsPanel />);

    expect(await screen.findByText("Page 1 / 3")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Previous" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "Next" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_request_logs", {
        days: 1,
        groupBy: "none",
        status: null,
        model: null,
        page: 2,
      }),
    );
    expect(await screen.findByText("Page 2 / 3")).toBeInTheDocument();
  });

  it("切换分组时回到第 1 页", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        page: args.page as number,
        total: 120,
        total_pages: 3,
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByText("Page 1 / 3");
    await user.click(screen.getByRole("button", { name: "Next" }));
    await screen.findByText("Page 2 / 3");

    const groupTrigger = screen.getByTestId("api-gateway-logs-group-trigger");
    await user.click(groupTrigger);
    await user.click(screen.getByRole("option", { name: "Model" }));
    expect(groupTrigger).toHaveTextContent("Model");
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_request_logs", {
        days: 1,
        groupBy: "model",
        status: null,
        model: null,
        page: 1,
      }),
    );
  });

  it("范围缩短导致页码越界时回到第 1 页且不显示空白页", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      if (args.days === 7) {
        return page({ page: 1, total: 5, total_pages: 1, records: [record()] });
      }
      return page({ page: args.page as number, total: 120, total_pages: 3 });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByText("Page 1 / 3");
    await user.click(screen.getByRole("button", { name: "Next" }));
    await screen.findByText("Page 2 / 3");

    const rangeTrigger = screen.getByTestId("api-gateway-logs-range-trigger");
    await user.click(rangeTrigger);
    await user.click(screen.getByRole("option", { name: "7d" }));
    expect(rangeTrigger).toHaveTextContent("7d");

    await waitFor(() =>
      expect(screen.getByText("Page 1 / 1")).toBeInTheDocument(),
    );
    expect(
      screen.queryByText("No matching requests."),
    ).not.toBeInTheDocument();
  });

  it("后端返回页码越界时收敛到有效页且不显示空白页", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      if (args.page === 2) {
        // Inconsistent response: page 2 of a single-page result.
        return page({ page: 2, total: 5, total_pages: 1, records: [record()] });
      }
      return page({ page: 1, total: 120, total_pages: 3 });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByText("Page 1 / 3");

    await user.click(screen.getByRole("button", { name: "Next" }));

    await waitFor(() =>
      expect(screen.getByText("Page 1 / 3")).toBeInTheDocument(),
    );
    const pageOneCalls = invokeMock.mock.calls.filter(
      ([command, args]) =>
        command === "api_gateway_request_logs" &&
        (args as { page?: number }).page === 1,
    );
    expect(pageOneCalls.length).toBeGreaterThanOrEqual(2);
    expect(screen.queryByText("No matching requests.")).not.toBeInTheDocument();
  });

  it("隐藏时不发起日志请求", async () => {
    invokeMock.mockImplementation(async () => page());

    renderWithProviders(<UsageLogsPanel isActive={false} />);

    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(invokeMock).not.toHaveBeenCalledWith(
      "api_gateway_request_logs",
      expect.anything(),
    );
  });

  it("过滤面板的模型选项来自范围内模型清单，而不限于当前页记录", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      const value = page({
        total: 1,
        records: [
          record({ local_model: "visible", upstream_model: "visible" }),
        ],
      });
      // Page 1 only contains `visible`, but the range facet also exposes a
      // model that lives on a later page.
      return { ...value, models: ["hidden-model", "visible"] } as UsageLogsPage & {
        models: string[];
      };
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("api-gateway-logs-ungrouped");

    await user.click(screen.getByRole("button", { name: "Filter" }));
    const panel = await screen.findByTestId("api-gateway-logs-filter-panel");

    // A model present in range but absent from page 1 must be selectable.
    expect(
      within(panel).getByRole("button", { name: "hidden-model" }),
    ).toBeInTheDocument();

    await user.click(
      within(panel).getByRole("button", { name: "hidden-model" }),
    );
    await user.click(within(panel).getByRole("button", { name: "Apply" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_request_logs", {
        days: 1,
        groupBy: "none",
        status: null,
        model: "hidden-model",
        page: 1,
      }),
    );
  });

  it("点击过滤选项即时生效，过滤按钮更新文本且清除按钮可重置", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({ records: [record({ local_model: "gpt-4o" })] });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("api-gateway-logs-ungrouped");

    const filterTrigger = screen.getByTestId("api-gateway-logs-filter-trigger");
    await user.click(filterTrigger);

    const panel = await screen.findByTestId("api-gateway-logs-filter-panel");
    const failureBtn = within(panel).getByRole("button", { name: "Failure" });
    await user.click(failureBtn);

    // Filter button updates its text immediately
    await waitFor(() =>
      expect(filterTrigger).toHaveTextContent("Failure"),
    );
    expect(failureBtn).toHaveClass("bg-primary");

    // Click clear
    const clearBtn = within(panel).getByRole("button", { name: "Clear" });
    await user.click(clearBtn);

    await waitFor(() =>
      expect(filterTrigger).toHaveTextContent("Filter"),
    );
  });

  it("失败记录展示状态码与红色错误图标（悬停显示全部错误信息），成功记录不显示错误图标", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        records: [
          record({
            result: "failure",
            status: 502,
            local_model: "gpt-4o",
          }),
          record({
            result: "success",
            status: 200,
            local_model: "claude-3-5",
          }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("api-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("api-gateway-logs-row");
    expect(rows).toHaveLength(2);

    // 第一行：失败记录
    const failureBadge = within(rows[0]).getByTestId(
      "api-gateway-logs-status-badge",
    );
    expect(failureBadge).toHaveTextContent("Failure");
    expect(failureBadge).toHaveTextContent("502");
    const errorIcon = within(rows[0]).getByTestId(
      "api-gateway-logs-error-icon",
    );
    expect(errorIcon).toBeInTheDocument();
    expect(errorIcon).toHaveClass("text-destructive");

    const tooltip = within(rows[0]).getByTestId(
      "api-gateway-logs-error-tooltip",
    );
    expect(tooltip).toHaveTextContent("Bad gateway / Upstream unavailable");

    // 第二行：成功记录
    const successBadge = within(rows[1]).getByTestId(
      "api-gateway-logs-status-badge",
    );
    expect(successBadge).toHaveTextContent("Success");
    expect(
      within(rows[1]).queryByTestId("api-gateway-logs-error-icon"),
    ).not.toBeInTheDocument();
  });

  it("Tokens 列同时显示输入、输出、缓存 Tokens 数，并提供 info 图标展示完整明细", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        records: [
          record({
            input_tokens: 1250,
            output_tokens: 340,
            cache_read_tokens: 80,
            cache_write_tokens: 20,
            total_tokens: 1690,
          }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("api-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("api-gateway-logs-row");
    expect(rows).toHaveLength(1);

    const tokensCell = within(rows[0]).getByTestId(
      "api-gateway-logs-tokens-cell",
    );
    const breakdown = within(tokensCell).getByTestId(
      "api-gateway-logs-tokens-breakdown",
    );
    expect(
      within(breakdown).getByTestId("api-gateway-logs-tokens-input-icon"),
    ).toBeInTheDocument();
    expect(
      within(breakdown).getByTestId("api-gateway-logs-tokens-input-icon"),
    ).toHaveClass("text-emerald-500");
    expect(breakdown).toHaveTextContent("1,250");
    expect(
      within(breakdown).getByTestId("api-gateway-logs-tokens-output-icon"),
    ).toBeInTheDocument();
    expect(
      within(breakdown).getByTestId("api-gateway-logs-tokens-output-icon"),
    ).toHaveClass("text-sky-500");
    expect(breakdown).toHaveTextContent("340");
    expect(
      within(tokensCell).getByTestId("api-gateway-logs-tokens-cache-icon"),
    ).toBeInTheDocument();
    expect(
      within(tokensCell).getByTestId("api-gateway-logs-tokens-cache-value"),
    ).toHaveTextContent("0.1K"); // 80 + 20 = 100 => 0.1K

    const infoBtn = within(tokensCell).getByTestId(
      "api-gateway-logs-tokens-info-btn",
    );
    expect(infoBtn).toBeInTheDocument();
    expect(infoBtn).not.toHaveAttribute("title");

    const tooltip = within(tokensCell).getByTestId(
      "api-gateway-logs-tokens-tooltip",
    );
    expect(tooltip).toHaveClass("top-full");
    expect(tooltip).toHaveTextContent("Tokens breakdown");
    expect(tooltip).toHaveTextContent("Input:");
    expect(tooltip).toHaveTextContent("1,250");
    expect(tooltip).toHaveTextContent("Output:");
    expect(tooltip).toHaveTextContent("340");
    expect(tooltip).toHaveTextContent("Cache:");
    expect(tooltip).toHaveTextContent("100");
    expect(tooltip).toHaveTextContent("Cache read:");
    expect(tooltip).toHaveTextContent("80");
    expect(tooltip).toHaveTextContent("Cache write:");
    expect(tooltip).toHaveTextContent("20");
    expect(tooltip).toHaveTextContent("Total:");
    expect(tooltip).toHaveTextContent("1,690");
  });

  it("Tokens 提示框在前几行向下弹出以避免被列头遮挡，在底部且上方空间充足时向上弹出", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        records: [
          record({ timestamp_ms: 1000 }),
          record({ timestamp_ms: 2000 }),
          record({ timestamp_ms: 3000 }),
          record({ timestamp_ms: 4000 }),
          record({ timestamp_ms: 5000 }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("api-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("api-gateway-logs-row");
    expect(rows).toHaveLength(5);

    // 第 0~3 行（前 4 行）上方空间不足，必须向下弹出避免被 table 列头遮挡
    for (let i = 0; i < 4; i++) {
      const tooltip = within(rows[i]).getByTestId(
        "api-gateway-logs-tokens-tooltip",
      );
      expect(tooltip).toHaveClass("top-full");
      expect(tooltip).not.toHaveClass("bottom-full");
    }

    // 第 4 行（即第 5 行记录，index=4，同时满足 index >= 4 与 index >= 5 - 3）上方有 4 行空间，向上弹出
    const lastTooltip = within(rows[4]).getByTestId(
      "api-gateway-logs-tokens-tooltip",
    );
    expect(lastTooltip).toHaveClass("bottom-full");
    expect(lastTooltip).not.toHaveClass("top-full");
  });

  it("模型列展示本地模型，并在同一行展示上游服务商名称与上游模型", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        records: [
          record({
            local_model: "claude-3-7-sonnet",
            upstream_model: "claude-3-7-sonnet-20250219",
            provider_name: "Anthropic Direct",
          }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("api-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("api-gateway-logs-row");
    expect(rows).toHaveLength(1);

    expect(within(rows[0]).getByText("claude-3-7-sonnet")).toBeInTheDocument();
    expect(
      within(rows[0]).getByTestId("api-gateway-logs-provider-name"),
    ).toHaveTextContent("Anthropic Direct");
    expect(
      within(rows[0]).getByTestId("api-gateway-logs-upstream-model"),
    ).toHaveTextContent("claude-3-7-sonnet-20250219");
  });

  it("模型列展示推理强度徽章，无推理强度时不展示徽章", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        records: [
          record({
            local_model: "deepseek-r1",
            upstream_model: "deepseek-reasoner",
            provider_name: "DeepSeek Direct",
            reasoning_effort: "high",
          }),
          record({
            local_model: "gpt-4o",
            upstream_model: "gpt-4o",
            provider_name: "OpenAI Direct",
            reasoning_effort: null,
          }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("api-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("api-gateway-logs-row");
    expect(rows).toHaveLength(2);

    // 第一行有推理强度，显示 high 徽章
    const effortBadge = within(rows[0]).getByTestId("api-gateway-logs-reasoning-effort");
    expect(effortBadge).toBeInTheDocument();
    expect(effortBadge).toHaveTextContent("high");
    expect(effortBadge).toHaveAttribute("title", expect.stringContaining("high"));

    // 第二行无推理强度，不显示徽章
    expect(
      within(rows[1]).queryByTestId("api-gateway-logs-reasoning-effort"),
    ).toBeNull();
  });

  it("大额 Tokens 在日志行展示真实原值千分位，缓存展示 xxK 格式，并在 Tooltip 中展示完整数值", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        records: [
          record({
            input_tokens: 150_000,
            output_tokens: 1_200_000,
            cache_read_tokens: 10_000_000,
            cache_write_tokens: 500_000,
            total_tokens: 11_850_000,
          }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("api-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("api-gateway-logs-row");
    const tokensCell = within(rows[0]).getByTestId("api-gateway-logs-tokens-cell");
    const breakdown = within(tokensCell).getByTestId("api-gateway-logs-tokens-breakdown");

    // 行内第一行展示下行与上行真实原值（千分位）
    expect(breakdown).toHaveTextContent("150,000");
    expect(breakdown).toHaveTextContent("1,200,000");

    // 行内第二行展示缓存 xxK 格式
    const cacheValue = within(tokensCell).getByTestId("api-gateway-logs-tokens-cache-value");
    expect(cacheValue).toHaveTextContent("10,500K"); // 10,000,000 + 500,000 = 10,500,000 => 10,500K

    // breakdown 外层 span 的 title 提示完整精确数值
    const inputSpan = breakdown.querySelector('span[title*="Input"]');
    expect(inputSpan).toHaveAttribute("title", "Input: 150,000");

    const outputSpan = breakdown.querySelector('span[title*="Output"]');
    expect(outputSpan).toHaveAttribute("title", "Output: 1,200,000");

    const cacheSpan = tokensCell.querySelector('span[title*="Cache"]');
    expect(cacheSpan).toHaveAttribute("title", "Cache: 10,500,000");

    // Tooltip 明细展示原始精确千分位数值
    const tooltip = within(tokensCell).getByTestId("api-gateway-logs-tokens-tooltip");
    expect(tooltip).toHaveTextContent("150,000");
    expect(tooltip).toHaveTextContent("1,200,000");
    expect(tooltip).toHaveTextContent("10,500,000");
    expect(tooltip).toHaveTextContent("10,000,000"); // Cache read: 10_000_000
    expect(tooltip).toHaveTextContent("500,000"); // Cache write: 500_000
    expect(tooltip).toHaveTextContent("11,850,000"); // Total: 11_850_000
  });

  it("有存储错误消息的失败记录展示红色错误图标，悬停或聚焦时揭示完整消息与 HTTP 状态", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        records: [
          record({
            result: "failure",
            status: 502,
            provider_name: "Provider A",
            error_message: STORED_UPSTREAM_MESSAGE,
            terminal: false,
          }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("api-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("api-gateway-logs-row");
    expect(rows).toHaveLength(1);

    // 展示红色错误图标
    const errorIcon = within(rows[0]).getByTestId(
      "api-gateway-logs-error-icon",
    );
    expect(errorIcon).toBeInTheDocument();
    expect(errorIcon).toHaveClass("text-destructive");

    // 提示默认隐藏，悬停或键盘聚焦时显示，并携带完整消息与 HTTP 状态上下文
    const tooltip = within(rows[0]).getByTestId(
      "api-gateway-logs-error-tooltip",
    );
    expect(tooltip).toHaveAttribute("role", "tooltip");
    expect(tooltip).toHaveTextContent(STORED_UPSTREAM_MESSAGE);
    expect(tooltip).toHaveTextContent(
      /HTTP 502:\s*Bad gateway \/ Upstream unavailable/,
    );
    expect(tooltip).toHaveClass("hidden");
    expect(tooltip).toHaveClass("group-hover:flex");
    expect(tooltip).toHaveClass("group-focus-within:flex");

    const reason = within(rows[0]).getByTestId(
      "api-gateway-logs-status-reason",
    );
    expectKeyboardFocusable(reason);
  });

  it("没有存储错误消息的失败记录展示红色错误图标并在悬停时呈现通用 HTTP 状态错误提示", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        total: 2,
        records: [
          record({
            result: "failure",
            status: 502,
            local_model: "explicit-null-message",
            error_message: null,
          }),
          record({
            result: "failure",
            status: 502,
            local_model: "omitted-message-field",
          }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("api-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("api-gateway-logs-row");
    expect(rows).toHaveLength(2);
    for (const row of rows) {
      const errorIcon = within(row).getByTestId("api-gateway-logs-error-icon");
      expect(errorIcon).toBeInTheDocument();
      expect(errorIcon).toHaveClass("text-destructive");

      const tooltip = within(row).getByTestId("api-gateway-logs-error-tooltip");
      expect(tooltip).toHaveTextContent(GENERIC_502_TITLE);
    }
  });

  it("非终止的失败尝试行显示尝试标签，终止记录不显示", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        total: 2,
        records: [
          record({
            timestamp_ms: Date.UTC(2026, 8, 17, 4, 0),
            result: "failure",
            status: 502,
            provider_name: "Terminal Provider",
            error_message: "terminal provider failure",
            terminal: true,
          }),
          record({
            timestamp_ms: Date.UTC(2026, 8, 17, 3, 0),
            result: "failure",
            status: 502,
            provider_name: "Attempt Provider",
            error_message: "attempt provider failure",
            terminal: false,
          }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("api-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("api-gateway-logs-row");
    expect(rows).toHaveLength(2);
    expect(
      within(rows[0]).getByTestId("api-gateway-logs-provider-name"),
    ).toHaveTextContent("Terminal Provider");
    expect(
      within(rows[0]).queryByTestId("api-gateway-logs-attempt-label"),
    ).not.toBeInTheDocument();
    expect(
      within(rows[1]).getByTestId("api-gateway-logs-provider-name"),
    ).toHaveTextContent("Attempt Provider");
    const attemptLabel = within(rows[1]).getByTestId(
      "api-gateway-logs-attempt-label",
    );
    expect(attemptLabel.textContent?.trim()).not.toBe("");
  });

  it("成功记录不显示失败原因行与错误提示", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      // A cancelled record is defensively omitted from the rendered table.
      return page({
        total: 2,
        records: [
          record({
            timestamp_ms: Date.UTC(2026, 8, 17, 4, 0),
            result: "success",
            status: 200,
            local_model: "gpt-4o",
          }),
          // Cancelled row in payload must be omitted by the panel.
          record({
            timestamp_ms: Date.UTC(2026, 8, 17, 3, 0),
            result: "cancelled",
            status: 0,
            local_model: "gpt-4o",
          }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("api-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("api-gateway-logs-row");
    // Only the success record is rendered; cancelled is omitted.
    expect(rows).toHaveLength(1);
    expect(
      within(rows[0]).getByTestId("api-gateway-logs-status-badge"),
    ).toHaveTextContent("Success");
    for (const row of rows) {
      expect(
        within(row).queryByTestId("api-gateway-logs-status-reason"),
      ).not.toBeInTheDocument();
      expect(
        within(row).queryByTestId("api-gateway-logs-error-tooltip"),
      ).not.toBeInTheDocument();
    }
  });

  it("同一请求的终止成功行与其失败尝试行按后端顺序（最新在前）相邻展示", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      // 后端按 timestamp_ms DESC, id DESC 返回：终止成功行最新在前，其后依次是同一请求的失败尝试行
      return page({
        total: 3,
        records: [
          record({
            timestamp_ms: Date.UTC(2026, 8, 17, 4, 0),
            result: "success",
            status: 200,
            local_model: "gpt-4o",
            provider_name: "Provider C",
          }),
          record({
            timestamp_ms: Date.UTC(2026, 8, 17, 3, 59),
            result: "failure",
            status: 500,
            local_model: "gpt-4o",
            provider_name: "Provider B",
            error_message: "Provider B upstream failure",
            terminal: false,
          }),
          record({
            timestamp_ms: Date.UTC(2026, 8, 17, 3, 58),
            result: "failure",
            status: 429,
            local_model: "gpt-4o",
            provider_name: "Provider A",
            error_message: "Provider A upstream failure",
            terminal: false,
          }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("api-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("api-gateway-logs-row");
    expect(rows).toHaveLength(3);
    const providerOf = (row: HTMLElement) =>
      within(row).getByTestId("api-gateway-logs-provider-name").textContent;

    expect(providerOf(rows[0])).toBe("Provider C");
    expect(
      within(rows[0]).getByTestId("api-gateway-logs-status-badge"),
    ).toHaveTextContent("Success");
    expect(
      within(rows[0]).queryByTestId("api-gateway-logs-attempt-label"),
    ).not.toBeInTheDocument();

    expect(providerOf(rows[1])).toBe("Provider B");
    expect(
      within(rows[1]).getByTestId("api-gateway-logs-status-badge"),
    ).toHaveTextContent("Failure");
    expect(
      within(rows[1]).getByTestId("api-gateway-logs-attempt-label"),
    ).toBeInTheDocument();

    expect(providerOf(rows[2])).toBe("Provider A");
    expect(
      within(rows[2]).getByTestId("api-gateway-logs-status-badge"),
    ).toHaveTextContent("Failure");
    expect(
      within(rows[2]).getByTestId("api-gateway-logs-attempt-label"),
    ).toBeInTheDocument();
  });

  it("terminal 字段缺省视为终止记录，不显示尝试标签", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        records: [
          record({
            result: "failure",
            status: 502,
            error_message: STORED_UPSTREAM_MESSAGE,
          }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("api-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("api-gateway-logs-row");
    expect(rows).toHaveLength(1);
    const tooltip = within(rows[0]).getByTestId(
      "api-gateway-logs-error-tooltip",
    );
    expect(tooltip).toHaveTextContent(STORED_UPSTREAM_MESSAGE);
    expect(
      within(rows[0]).queryByTestId("api-gateway-logs-attempt-label"),
    ).not.toBeInTheDocument();
  });

  it("错误行状态列设置 whitespace-nowrap 且不直接输出错误正文，仅展示红色图标并在浮层中展示全部错误信息", async () => {
    const longErrorMessage =
      "Request failed with error: internal proxy error timeout connecting to backend upstream service over TLS after 30000ms";
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        records: [
          record({
            result: "failure",
            status: 504,
            error_message: longErrorMessage,
          }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("api-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("api-gateway-logs-row");
    expect(rows).toHaveLength(1);

    // 状态单元格应具备 whitespace-nowrap
    const statusCell = rows[0].querySelectorAll("td")[1];
    expect(statusCell).toHaveClass("whitespace-nowrap");

    // 状态单元格内错误文本只存在于浮层内部，触发按钮本身不平铺文字（仅为小图标）
    const trigger = within(statusCell).getByTestId("api-gateway-logs-status-reason");
    expect(trigger).not.toHaveTextContent(longErrorMessage);

    // 红色图标存在
    const errorIcon = within(statusCell).getByTestId("api-gateway-logs-error-icon");
    expect(errorIcon).toBeInTheDocument();
    expect(errorIcon).toHaveClass("text-destructive");

    // 鼠标滑过/浮层中展示全部错误信息（包含长错误文本和 HTTP 状态）
    const tooltip = within(statusCell).getByTestId("api-gateway-logs-error-tooltip");
    expect(tooltip).toHaveClass("hidden");
    expect(tooltip).toHaveTextContent(longErrorMessage);
    expect(tooltip).toHaveTextContent("HTTP 504: Gateway timeout");
  });

  it("耗时列值根据耗时区间展示不同颜色（<3s 绿，3~15s 黄，>15s 红，空值灰）", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        total: 4,
        records: [
          record({ duration_ms: 1200 }),
          record({ duration_ms: 5000 }),
          record({ duration_ms: 20000 }),
          record({ duration_ms: undefined as unknown as number }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);

    const table = await screen.findByTestId("api-gateway-logs-ungrouped");
    const rows = within(table).getAllByTestId("api-gateway-logs-row");
    expect(rows).toHaveLength(4);

    const d1 = within(rows[0]).getByTestId("api-gateway-logs-duration-cell");
    expect(d1).toHaveTextContent("1s");
    expect(d1).toHaveClass("text-emerald-500");

    const d2 = within(rows[1]).getByTestId("api-gateway-logs-duration-cell");
    expect(d2).toHaveTextContent("5s");
    expect(d2).toHaveClass("text-amber-500");

    const d3 = within(rows[2]).getByTestId("api-gateway-logs-duration-cell");
    expect(d3).toHaveTextContent("20s");
    expect(d3).toHaveClass("text-rose-500");

    const d4 = within(rows[3]).getByTestId("api-gateway-logs-duration-cell");
    expect(d4).toHaveTextContent("—");
    expect(d4).toHaveClass("text-muted-foreground");
  });

  it("当记录为 429 额度耗尽或网络错误时在详情中展示对应排查建议", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "api_gateway_request_logs") {
        return page({
          total: 2,
          records: [
            record({
              result: "failure",
              status: 429,
              error_message: "You have exceeded your current quota, please check your plan and billing details.",
            }),
            record({
              result: "failure",
              status: 0,
              error_message: "network error: connection refused / unreachable",
            }),
          ],
        });
      }
      return undefined;
    });

    renderWithProviders(<UsageLogsPanel />);

    const table = await screen.findByTestId("api-gateway-logs-ungrouped");
    const rows = within(table).getAllByTestId("api-gateway-logs-row");
    expect(rows).toHaveLength(2);

    const hints = within(table).getAllByTestId("api-gateway-logs-actionable-hint");
    expect(hints).toHaveLength(2);
    expect(hints[0]).toHaveTextContent("Suggestion: Upstream provider quota or periodic limit exhausted");
    expect(hints[1]).toHaveTextContent("Suggestion: Unable to connect to upstream URL");
  });
});

