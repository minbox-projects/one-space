import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import i18n from "@/i18n";
import { UsageLogsPanel } from "@/components/ApiFusion/UsageLogsPanel";
import type {
  UsageLogRecord,
  UsageLogsPage,
} from "@/lib/apiFusion";
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

describe("UsageLogsPanel", () => {
  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
  });

  it("默认今日、不分组并以第 1 页取数，切换范围回到第 1 页", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "api_fusion_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({ total: 120, total_pages: 3 });
    });

    renderWithProviders(<UsageLogsPanel />);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_request_logs", {
        days: 1,
        groupBy: "none",
        status: null,
        model: null,
        page: 1,
      }),
    );

    await user.click(screen.getByRole("button", { name: "30d" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_request_logs", {
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
      if (command !== "api_fusion_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      calls += 1;
      if (calls === 1) return page();
      return new Promise<UsageLogsPage>((resolve) => {
        resolveSecond = resolve;
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("api-fusion-logs-ungrouped");

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
      if (command !== "api_fusion_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        total: 3,
        records: [
          record({ timestamp_ms: Date.UTC(2026, 8, 17, 4, 0), local_model: "newest" }),
          record({ timestamp_ms: Date.UTC(2026, 8, 17, 2, 0), local_model: "middle" }),
          record({ timestamp_ms: Date.UTC(2026, 8, 16, 18, 0), local_model: "oldest" }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);

    const table = await screen.findByTestId("api-fusion-logs-ungrouped");
    expect(within(table).getByText("Time")).toBeInTheDocument();
    expect(within(table).getByText("Status")).toBeInTheDocument();
    expect(within(table).getByText("Tokens")).toBeInTheDocument();
    expect(within(table).getByText("Cost ($)")).toBeInTheDocument();

    const rows = within(table).getAllByTestId("api-fusion-logs-row");
    expect(rows).toHaveLength(3);
    // UTC+8 display: 2026-09-17T18:00Z is 2026-09-18 02:00.
    expect(within(rows[0]).getByText("2026-09-17 12:00")).toBeInTheDocument();
    expect(within(rows[0]).getByText("Success")).toBeInTheDocument();
    expect(within(rows[0]).getByText("newest")).toBeInTheDocument();
  });

  it("切换为 Day（UTC+8）分组展示分组列且错误数不含 cancelled", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command !== "api_fusion_request_logs") {
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
    await screen.findByTestId("api-fusion-logs-ungrouped");

    await user.click(screen.getByRole("button", { name: "Day (UTC+8)" }));

    const grouped = await screen.findByTestId("api-fusion-logs-grouped");
    expect(within(grouped).getByText("Group")).toBeInTheDocument();
    expect(within(grouped).getByText("Errors")).toBeInTheDocument();
    const rows = within(grouped).getAllByTestId("api-fusion-logs-group-row");
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
      if (command !== "api_fusion_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page();
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("api-fusion-logs-ungrouped");

    await user.click(screen.getByRole("button", { name: "Filter" }));
    const panel = await screen.findByTestId("api-fusion-logs-filter-panel");
    expect(within(panel).queryByRole("combobox")).not.toBeInTheDocument();

    await user.click(within(panel).getByRole("button", { name: "Failure" }));
    await user.click(within(panel).getByRole("button", { name: "gpt-4o" }));
    await user.click(within(panel).getByRole("button", { name: "Apply" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_request_logs", {
        days: 1,
        groupBy: "none",
        status: "failure",
        model: "gpt-4o",
        page: 1,
      }),
    );
  });

  it("筛选无匹配显示空状态且不报错", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command !== "api_fusion_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      if (args.status === "failure") {
        return page({ total: 0, total_pages: 0, records: [] });
      }
      return page();
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("api-fusion-logs-ungrouped");

    await user.click(screen.getByRole("button", { name: "Filter" }));
    const panel = await screen.findByTestId("api-fusion-logs-filter-panel");
    await user.click(within(panel).getByRole("button", { name: "Failure" }));
    await user.click(within(panel).getByRole("button", { name: "Apply" }));

    expect(await screen.findByText("No matching requests.")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("每页 50 条、默认第 1 页并支持翻页", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command !== "api_fusion_request_logs") {
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
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_request_logs", {
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
      if (command !== "api_fusion_request_logs") {
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

    await user.click(screen.getByRole("button", { name: "Model" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_request_logs", {
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
      if (command !== "api_fusion_request_logs") {
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

    await user.click(screen.getByRole("button", { name: "7d" }));

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
      if (command !== "api_fusion_request_logs") {
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
        command === "api_fusion_request_logs" &&
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
      "api_fusion_request_logs",
      expect.anything(),
    );
  });
});
