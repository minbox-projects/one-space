import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import i18n from "@/i18n";
import { UsageLogsPanel } from "@/components/AiGateway/UsageLogsPanel";
import type {
  UsageLogRecord,
  UsageLogsPage,
} from "@/lib/aiGateway";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

type CostBreakdown = {
  input_price: number;
  output_price: number;
  cache_read_price: number;
  cache_write_price: number;
  input_cost: number;
  output_cost: number;
  cache_read_cost: number;
  cache_write_cost: number;
};

/** Independent fixed snapshot fixture: 1/4/0.5/2 USD per million tokens. */
function costBreakdown(overrides: Partial<CostBreakdown> = {}): CostBreakdown {
  return {
    input_price: 1,
    output_price: 4,
    cache_read_price: 0.5,
    cache_write_price: 2,
    input_cost: 1,
    output_cost: 4,
    cache_read_cost: 1,
    cache_write_cost: 0,
    ...overrides,
  };
}

function record(
  overrides: Partial<UsageLogRecord> & {
    cost_breakdown?: CostBreakdown | null;
    /** Step 1 nullable per-attempt measurements (absent mirrors a legacy row). */
    first_token_ms?: number | null;
    avg_tps?: number | null;
  } = {},
): UsageLogRecord & { first_token_ms?: number | null; avg_tps?: number | null } {
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

/** The ungrouped row's cost cell is the last public table cell. */
function rowCostCell(row: HTMLElement): HTMLElement {
  const cells = row.querySelectorAll("td");
  return cells[cells.length - 1] as HTMLElement;
}

// ---------------------------------------------------------------------------
// Step 2 (REQ-001/REQ-002/REQ-003/REQ-004): the latency cell keeps its existing
// whole-cell marker and exposes stable per-metric label/value/unit markers.
// These marker names are the proposed contract for Frontend.
// ---------------------------------------------------------------------------
const LATENCY_CELL = "ai-gateway-logs-duration-cell";
const LATENCY_TOTAL_LABEL = "ai-gateway-logs-latency-total-label";
const LATENCY_TOTAL_VALUE = "ai-gateway-logs-latency-total-value";
const LATENCY_TOTAL_UNIT = "ai-gateway-logs-latency-total-unit";
const LATENCY_FIRST_TOKEN_LABEL = "ai-gateway-logs-latency-first-token-label";
const LATENCY_FIRST_TOKEN_VALUE = "ai-gateway-logs-latency-first-token-value";
const LATENCY_FIRST_TOKEN_UNIT = "ai-gateway-logs-latency-first-token-unit";
const LATENCY_AVG_TPS_LABEL = "ai-gateway-logs-latency-avg-tps-label";
const LATENCY_AVG_TPS_VALUE = "ai-gateway-logs-latency-avg-tps-value";
const LATENCY_AVG_TPS_UNIT = "ai-gateway-logs-latency-avg-tps-unit";

const LATENCY_GREEN = /(emerald|green)/;
const LATENCY_ORANGE = /(amber|orange)/;
const LATENCY_RED = /(rose|red)/;
const LATENCY_MUTED = /text-muted-foreground/;

const LATENCY_FAMILY_TONES = {
  green: LATENCY_GREEN,
  orange: LATENCY_ORANGE,
  red: LATENCY_RED,
} as const;

/**
 * REQ-004 / Step 2: an available rendered metric value must carry a readable
 * light-theme shade (shade number >= 500) and a matching `dark:` variant in the
 * same color family. Exact shade numbers are intentionally not pinned.
 */
function expectReadableLatencyVariant(
  value: HTMLElement,
  family: keyof typeof LATENCY_FAMILY_TONES,
) {
  const tone = LATENCY_FAMILY_TONES[family];
  const classes = value.className.split(/\s+/).filter(Boolean);
  const lightShades = classes.filter(
    (cls) => /^text-[a-z]+-\d{2,3}$/.test(cls) && tone.test(cls),
  );
  expect(
    lightShades.some((cls) => Number(cls.split("-").pop()) >= 500),
    `可用指标值应使用 >= 500 的亮色主题色阶，实际类名：${value.className}`,
  ).toBe(true);
  const darkShades = classes.filter(
    (cls) => /^dark:text-[a-z]+-\d{2,3}$/.test(cls) && tone.test(cls),
  );
  expect(
    darkShades.length,
    `可用指标值应提供同色系 dark: 变体，实际类名：${value.className}`,
  ).toBeGreaterThan(0);
}

function latencyCell(row: HTMLElement): HTMLElement {
  return within(row).getByTestId(LATENCY_CELL);
}

function latencyValue(cell: HTMLElement, marker: string): HTMLElement {
  return within(cell).getByTestId(marker);
}

function expectMutedMetric(cell: HTMLElement, valueMarker: string, unitMarker: string) {
  const value = latencyValue(cell, valueMarker);
  expect(value.textContent, "不可用指标应显示单个破折号").toBe("—");
  expect(value.className).toMatch(LATENCY_MUTED);
  expect(value.className).not.toMatch(LATENCY_GREEN);
  expect(value.className).not.toMatch(LATENCY_ORANGE);
  expect(value.className).not.toMatch(LATENCY_RED);
  // A single muted dash carries no unit suffix.
  expect(within(cell).queryByTestId(unitMarker)?.textContent ?? "").toBe("");
}

function expectPerformanceValue(
  cell: HTMLElement,
  valueMarker: string,
  text: string,
  tone: RegExp,
) {
  const value = latencyValue(cell, valueMarker);
  expect(value.textContent).toBe(text);
  expect(value.className).toMatch(tone);
  expect(value.className).not.toMatch(LATENCY_MUTED);
  expect(
    value.className,
    "数值应使用对齐的表格数字（tabular-nums 或等宽字体）",
  ).toMatch(/tabular-nums|font-mono/);
}

function expectAlignedRight(value: HTMLElement) {
  const line = value.parentElement as HTMLElement | null;
  const combined = `${value.className} ${line?.className ?? ""}`;
  expect(
    /text-right|justify-end|justify-between|ml-auto/.test(combined),
    "数值应在行内右对齐",
  ).toBe(true);
}

/** The keyboard-accessible cost-detail control rendered next to the cost. */
function costDetailControl(row: HTMLElement): HTMLElement {
  return within(rowCostCell(row)).getByRole("button");
}

/**
 * The detail content the cost control reveals. Mirrors the public Tokens
 * hover/focus boundary: a `role="tooltip"` element inside the same cell.
 */
function costDetailContent(row: HTMLElement): HTMLElement {
  return within(rowCostCell(row)).getByRole("tooltip");
}

/** The control must be tied to detail content within its own cost cell. */
function expectCostDetailAssociated(control: HTMLElement, detail: HTMLElement) {
  const cell = control.closest("td");
  expect(
    cell,
    "cost 明细控件应位于该行的花费单元格内",
  ).not.toBeNull();
  expect(
    cell === detail || cell?.contains(detail),
    "cost 明细内容应与花费控件在同一单元格内关联",
  ).toBe(true);
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
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({ total: 120, total_pages: 3 });
    });

    renderWithProviders(<UsageLogsPanel />);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_request_logs", {
        range: "today",
        groupBy: "none",
        status: null,
        model: null,
        provider: null,
        page: 1,
      }),
    );

    const rangeTrigger = screen.getByTestId("ai-gateway-logs-range-trigger");
    expect(rangeTrigger).toHaveTextContent("Today");
    await user.click(rangeTrigger);
    await user.click(screen.getByRole("option", { name: "30d" }));
    expect(rangeTrigger).toHaveTextContent("30d");
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_request_logs", {
        range: "30d",
        groupBy: "none",
        status: null,
        model: null,
        provider: null,
        page: 1,
      }),
    );
  });

  it("刷新按钮在请求期间展示刷新中状态", async () => {
    const user = userEvent.setup();
    let calls = 0;
    let resolveSecond: (value: UsageLogsPage) => void = () => {};
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      calls += 1;
      if (calls === 1) return page();
      return new Promise<UsageLogsPage>((resolve) => {
        resolveSecond = resolve;
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("ai-gateway-logs-ungrouped");

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
      if (command !== "ai_gateway_request_logs") {
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

    const table = await screen.findByTestId("ai-gateway-logs-ungrouped");
    expect(within(table).getByText("Time")).toBeInTheDocument();
    expect(within(table).getByText("Status")).toBeInTheDocument();
    expect(within(table).getByText("Model")).toBeInTheDocument();
    expect(within(table).getByText("Latency")).toBeInTheDocument();
    expect(within(table).getByText("Tokens")).toBeInTheDocument();
    expect(within(table).getByText("Cost ($)")).toBeInTheDocument();

    const ths = table.querySelectorAll("th");
    expect(ths.length).toBe(6);
    ths.forEach((th) => {
      expect(th).toHaveClass("text-left");
      expect(th).toHaveClass("whitespace-nowrap");
    });

    const rows = within(table).getAllByTestId("ai-gateway-logs-row");
    expect(rows).toHaveLength(3);
    // UTC+8 display: 2026-09-17T18:00Z is 2026-09-18 02:00.
    expect(within(rows[0]).getByText("2026-09-17 12:00:00")).toBeInTheDocument();
    expect(within(rows[0]).getByText("Success")).toBeInTheDocument();
    expect(within(rows[0]).getByTestId("ai-gateway-logs-status-badge")).toHaveClass("bg-emerald-500/10");
    expect(within(rows[0]).getByText("newest")).toBeInTheDocument();
    // Total duration keeps its raw-ms color: 62s is red, 2s / 0.5s are green.
    expectPerformanceValue(
      latencyCell(rows[0]),
      LATENCY_TOTAL_VALUE,
      "62.000",
      LATENCY_RED,
    );
    expectPerformanceValue(
      latencyCell(rows[1]),
      LATENCY_TOTAL_VALUE,
      "2.000",
      LATENCY_GREEN,
    );
    expectPerformanceValue(
      latencyCell(rows[2]),
      LATENCY_TOTAL_VALUE,
      "0.500",
      LATENCY_GREEN,
    );
  });

  it("中文环境下不分组表格展示中文列头 延迟", async () => {
    await i18n.changeLanguage("zh");
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        total: 1,
        records: [record({ duration_ms: 65000 })],
      });
    });

    renderWithProviders(<UsageLogsPanel />);

    const table = await screen.findByTestId("ai-gateway-logs-ungrouped");
    expect(within(table).getByText("延迟")).toBeInTheDocument();
    expectPerformanceValue(
      latencyCell(within(table).getAllByTestId("ai-gateway-logs-row")[0]),
      LATENCY_TOTAL_VALUE,
      "65.000",
      LATENCY_RED,
    );
  });

  it("切换为 Day（UTC+8）分组展示分组列与错误数", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      if (args.groupBy === "day") {
        return page({
          group_by: "day",
          records: [],
          total: 2,
          groups: [
            {
              group: "2026-09-17",
              request_count: 2,
              // 2 requests: 1 failure + 1 success.
              error_count: 1,
              last_request_at_ms: Date.UTC(2026, 8, 17, 5, 0),
            },
          ],
        });
      }
      return page();
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("ai-gateway-logs-ungrouped");

    const groupTrigger = screen.getByTestId("ai-gateway-logs-group-trigger");
    expect(groupTrigger).toHaveTextContent("No grouping");
    await user.click(groupTrigger);
    await user.click(screen.getByRole("option", { name: "Day (UTC+8)" }));
    expect(groupTrigger).toHaveTextContent("Day (UTC+8)");

    const grouped = await screen.findByTestId("ai-gateway-logs-grouped");
    expect(within(grouped).getByText("Group")).toBeInTheDocument();
    expect(within(grouped).getByText("Errors")).toBeInTheDocument();

    const groupThs = grouped.querySelectorAll("th");
    expect(groupThs.length).toBe(4);
    groupThs.forEach((th) => {
      expect(th).toHaveClass("text-left");
      expect(th).toHaveClass("whitespace-nowrap");
    });
    const rows = within(grouped).getAllByTestId("ai-gateway-logs-group-row");
    expect(rows).toHaveLength(1);
    expect(within(rows[0]).getByText("2026-09-17")).toBeInTheDocument();
    expect(within(rows[0]).getByText("2")).toBeInTheDocument();
    expect(within(rows[0]).getByText("1")).toBeInTheDocument();
    expect(within(rows[0]).getByText("2026-09-17 13:00:00")).toBeInTheDocument();
    expect(within(grouped).queryByText("Ungrouped")).not.toBeInTheDocument();
  });

  it("过滤面板为选择式且状态与模型条件可组合，应用后回到第 1 页", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page();
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("ai-gateway-logs-ungrouped");

    await user.click(screen.getByRole("button", { name: "Filter" }));
    const panel = await screen.findByTestId("ai-gateway-logs-filter-panel");
    expect(within(panel).queryByRole("combobox")).not.toBeInTheDocument();

    await user.click(within(panel).getByRole("button", { name: "Failure" }));
    await user.click(within(panel).getByRole("button", { name: "gpt-4o" }));
    await user.click(within(panel).getByRole("button", { name: "Apply" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_request_logs", {
        range: "today",
        groupBy: "none",
        status: "failure",
        model: "gpt-4o",
        provider: null,
        page: 1,
      }),
    );
  });

  it("过滤面板仅展示 success 与 failure 状态选项，不包含已取消", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page();
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("ai-gateway-logs-ungrouped");

    await user.click(screen.getByRole("button", { name: "Filter" }));
    const panel = await screen.findByTestId("ai-gateway-logs-filter-panel");

    // The status filter offers exactly Any status, Success and Failure.
    const statusGroup = within(panel).getByText("Status")
      .parentElement as HTMLElement;
    expect(
      within(statusGroup)
        .getAllByRole("button")
        .map((button) => button.textContent?.trim()),
    ).toEqual(["Any status", "Success", "Failure"]);
    expect(
      within(panel).queryByRole("button", { name: "Cancelled" }),
    ).not.toBeInTheDocument();
  });

  it("筛选无匹配显示空状态且不报错", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      if (args.status === "failure") {
        return page({ total: 0, total_pages: 0, records: [] });
      }
      return page();
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("ai-gateway-logs-ungrouped");

    await user.click(screen.getByRole("button", { name: "Filter" }));
    const panel = await screen.findByTestId("ai-gateway-logs-filter-panel");
    await user.click(within(panel).getByRole("button", { name: "Failure" }));
    await user.click(within(panel).getByRole("button", { name: "Apply" }));

    expect(await screen.findByText("No matching requests.")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("每页 50 条、默认第 1 页并支持翻页", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command !== "ai_gateway_request_logs") {
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
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_request_logs", {
        range: "today",
        groupBy: "none",
        status: null,
        model: null,
        provider: null,
        page: 2,
      }),
    );
    expect(await screen.findByText("Page 2 / 3")).toBeInTheDocument();
  });

  it("切换分组时回到第 1 页", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command !== "ai_gateway_request_logs") {
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

    const groupTrigger = screen.getByTestId("ai-gateway-logs-group-trigger");
    await user.click(groupTrigger);
    await user.click(screen.getByRole("option", { name: "Model" }));
    expect(groupTrigger).toHaveTextContent("Model");
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_request_logs", {
        range: "today",
        groupBy: "model",
        status: null,
        model: null,
        provider: null,
        page: 1,
      }),
    );
  });

  it("范围缩短导致页码越界时回到第 1 页且不显示空白页", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      if (args.range === "yesterday") {
        return page({ page: 1, total: 5, total_pages: 1, records: [record()] });
      }
      return page({ page: args.page as number, total: 120, total_pages: 3 });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByText("Page 1 / 3");
    await user.click(screen.getByRole("button", { name: "Next" }));
    await screen.findByText("Page 2 / 3");

    const rangeTrigger = screen.getByTestId("ai-gateway-logs-range-trigger");
    await user.click(rangeTrigger);
    await user.click(screen.getByRole("option", { name: "Yesterday" }));
    expect(rangeTrigger).toHaveTextContent("Yesterday");

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_request_logs", {
        range: "yesterday",
        groupBy: "none",
        status: null,
        model: null,
        provider: null,
        page: 1,
      }),
    );
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
      if (command !== "ai_gateway_request_logs") {
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
        command === "ai_gateway_request_logs" &&
        (args as { page?: number }).page === 1 &&
        (args as { provider?: string | null }).provider === null,
    );
    expect(pageOneCalls.length).toBeGreaterThanOrEqual(2);
    expect(screen.queryByText("No matching requests.")).not.toBeInTheDocument();
  });

  it("隐藏时不发起日志请求", async () => {
    invokeMock.mockImplementation(async () => page());

    renderWithProviders(<UsageLogsPanel isActive={false} />);

    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(invokeMock).not.toHaveBeenCalledWith(
      "ai_gateway_request_logs",
      expect.anything(),
    );
  });

  it("过滤面板的模型选项来自范围内模型清单，而不限于当前页记录", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
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
    await screen.findByTestId("ai-gateway-logs-ungrouped");

    await user.click(screen.getByRole("button", { name: "Filter" }));
    const panel = await screen.findByTestId("ai-gateway-logs-filter-panel");

    // A model present in range but absent from page 1 must be selectable.
    expect(
      within(panel).getByRole("button", { name: "hidden-model" }),
    ).toBeInTheDocument();

    await user.click(
      within(panel).getByRole("button", { name: "hidden-model" }),
    );
    await user.click(within(panel).getByRole("button", { name: "Apply" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_request_logs", {
        range: "today",
        groupBy: "none",
        status: null,
        model: "hidden-model",
        provider: null,
        page: 1,
      }),
    );
  });

  it("点击过滤选项即时生效，过滤按钮更新文本且清除按钮可重置", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({ records: [record({ local_model: "gpt-4o" })] });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("ai-gateway-logs-ungrouped");

    const filterTrigger = screen.getByTestId("ai-gateway-logs-filter-trigger");
    await user.click(filterTrigger);

    const panel = await screen.findByTestId("ai-gateway-logs-filter-panel");
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
      if (command !== "ai_gateway_request_logs") {
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
    await screen.findByTestId("ai-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("ai-gateway-logs-row");
    expect(rows).toHaveLength(2);

    // 第一行：失败记录
    const failureBadge = within(rows[0]).getByTestId(
      "ai-gateway-logs-status-badge",
    );
    expect(failureBadge).toHaveTextContent("Failure");
    expect(failureBadge).toHaveTextContent("502");
    const errorIcon = within(rows[0]).getByTestId(
      "ai-gateway-logs-error-icon",
    );
    expect(errorIcon).toBeInTheDocument();
    expect(errorIcon).toHaveClass("text-destructive");

    const tooltip = within(rows[0]).getByTestId(
      "ai-gateway-logs-error-tooltip",
    );
    expect(tooltip).toHaveTextContent("Bad gateway / Upstream unavailable");

    // 第二行：成功记录
    const successBadge = within(rows[1]).getByTestId(
      "ai-gateway-logs-status-badge",
    );
    expect(successBadge).toHaveTextContent("Success");
    expect(
      within(rows[1]).queryByTestId("ai-gateway-logs-error-icon"),
    ).not.toBeInTheDocument();
  });

  it("Tokens 列同时显示输入、输出、缓存 Tokens 数，并提供 info 图标展示完整明细", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
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
    await screen.findByTestId("ai-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("ai-gateway-logs-row");
    expect(rows).toHaveLength(1);

    const tokensCell = within(rows[0]).getByTestId(
      "ai-gateway-logs-tokens-cell",
    );
    const breakdown = within(tokensCell).getByTestId(
      "ai-gateway-logs-tokens-breakdown",
    );
    expect(
      within(breakdown).getByTestId("ai-gateway-logs-tokens-input-icon"),
    ).toBeInTheDocument();
    expect(
      within(breakdown).getByTestId("ai-gateway-logs-tokens-input-icon"),
    ).toHaveClass("text-emerald-500");
    expect(breakdown).toHaveTextContent("1,250");
    expect(
      within(breakdown).getByTestId("ai-gateway-logs-tokens-output-icon"),
    ).toBeInTheDocument();
    expect(
      within(breakdown).getByTestId("ai-gateway-logs-tokens-output-icon"),
    ).toHaveClass("text-sky-500");
    expect(breakdown).toHaveTextContent("340");
    expect(
      within(tokensCell).getByTestId("ai-gateway-logs-tokens-cache-icon"),
    ).toBeInTheDocument();
    expect(
      within(tokensCell).getByTestId("ai-gateway-logs-tokens-cache-value"),
    ).toHaveTextContent("0.1K"); // 80 + 20 = 100 => 0.1K

    const infoBtn = within(tokensCell).getByTestId(
      "ai-gateway-logs-tokens-info-btn",
    );
    expect(infoBtn).toBeInTheDocument();
    expect(infoBtn).not.toHaveAttribute("title");

    const tooltip = within(tokensCell).getByTestId(
      "ai-gateway-logs-tokens-tooltip",
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
      if (command !== "ai_gateway_request_logs") {
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
    await screen.findByTestId("ai-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("ai-gateway-logs-row");
    expect(rows).toHaveLength(5);

    // 第 0~3 行（前 4 行）上方空间不足，必须向下弹出避免被 table 列头遮挡
    for (let i = 0; i < 4; i++) {
      const tooltip = within(rows[i]).getByTestId(
        "ai-gateway-logs-tokens-tooltip",
      );
      expect(tooltip).toHaveClass("top-full");
      expect(tooltip).not.toHaveClass("bottom-full");
    }

    // 第 4 行（即第 5 行记录，index=4，同时满足 index >= 4 与 index >= 5 - 3）上方有 4 行空间，向上弹出
    const lastTooltip = within(rows[4]).getByTestId(
      "ai-gateway-logs-tokens-tooltip",
    );
    expect(lastTooltip).toHaveClass("bottom-full");
    expect(lastTooltip).not.toHaveClass("top-full");
  });

  it("模型列展示本地模型，并在同一行展示上游服务商名称与上游模型", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
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
    await screen.findByTestId("ai-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("ai-gateway-logs-row");
    expect(rows).toHaveLength(1);

    expect(within(rows[0]).getByText("claude-3-7-sonnet")).toBeInTheDocument();
    expect(
      within(rows[0]).getByTestId("ai-gateway-logs-provider-name"),
    ).toHaveTextContent("Anthropic Direct");
    expect(
      within(rows[0]).getByTestId("ai-gateway-logs-upstream-model"),
    ).toHaveTextContent("claude-3-7-sonnet-20250219");
  });

  it("模型列展示推理强度徽章，无推理强度时不展示徽章", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
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
    await screen.findByTestId("ai-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("ai-gateway-logs-row");
    expect(rows).toHaveLength(2);

    // 第一行有推理强度，显示 high 徽章
    const effortBadge = within(rows[0]).getByTestId("ai-gateway-logs-reasoning-effort");
    expect(effortBadge).toBeInTheDocument();
    expect(effortBadge).toHaveTextContent("high");
    expect(effortBadge).toHaveAttribute("title", expect.stringContaining("high"));

    // 第二行无推理强度，不显示徽章
    expect(
      within(rows[1]).queryByTestId("ai-gateway-logs-reasoning-effort"),
    ).toBeNull();
  });

  it("大额 Tokens 在日志行展示真实原值千分位，缓存展示 xxK 格式，并在 Tooltip 中展示完整数值", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
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
    await screen.findByTestId("ai-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("ai-gateway-logs-row");
    const tokensCell = within(rows[0]).getByTestId("ai-gateway-logs-tokens-cell");
    const breakdown = within(tokensCell).getByTestId("ai-gateway-logs-tokens-breakdown");

    // 行内第一行展示下行与上行真实原值（千分位）
    expect(breakdown).toHaveTextContent("150,000");
    expect(breakdown).toHaveTextContent("1,200,000");

    // 行内第二行展示缓存 xxK 格式
    const cacheValue = within(tokensCell).getByTestId("ai-gateway-logs-tokens-cache-value");
    expect(cacheValue).toHaveTextContent("10,500K"); // 10,000,000 + 500,000 = 10,500,000 => 10,500K

    // breakdown 外层 span 的 title 提示完整精确数值
    const inputSpan = breakdown.querySelector('span[title*="Input"]');
    expect(inputSpan).toHaveAttribute("title", "Input: 150,000");

    const outputSpan = breakdown.querySelector('span[title*="Output"]');
    expect(outputSpan).toHaveAttribute("title", "Output: 1,200,000");

    const cacheSpan = tokensCell.querySelector('span[title*="Cache"]');
    expect(cacheSpan).toHaveAttribute("title", "Cache: 10,500,000");

    // Tooltip 明细展示原始精确千分位数值
    const tooltip = within(tokensCell).getByTestId("ai-gateway-logs-tokens-tooltip");
    expect(tooltip).toHaveTextContent("150,000");
    expect(tooltip).toHaveTextContent("1,200,000");
    expect(tooltip).toHaveTextContent("10,500,000");
    expect(tooltip).toHaveTextContent("10,000,000"); // Cache read: 10_000_000
    expect(tooltip).toHaveTextContent("500,000"); // Cache write: 500_000
    expect(tooltip).toHaveTextContent("11,850,000"); // Total: 11_850_000
  });

  it("有存储错误消息的失败记录展示红色错误图标，悬停或聚焦时揭示完整消息与 HTTP 状态", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
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
    await screen.findByTestId("ai-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("ai-gateway-logs-row");
    expect(rows).toHaveLength(1);

    // 展示红色错误图标
    const errorIcon = within(rows[0]).getByTestId(
      "ai-gateway-logs-error-icon",
    );
    expect(errorIcon).toBeInTheDocument();
    expect(errorIcon).toHaveClass("text-destructive");

    // 提示默认隐藏，悬停或键盘聚焦时显示，并携带完整消息与 HTTP 状态上下文
    const tooltip = within(rows[0]).getByTestId(
      "ai-gateway-logs-error-tooltip",
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
      "ai-gateway-logs-status-reason",
    );
    expectKeyboardFocusable(reason);
  });

  it("没有存储错误消息的失败记录展示红色错误图标并在悬停时呈现通用 HTTP 状态错误提示", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
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
    await screen.findByTestId("ai-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("ai-gateway-logs-row");
    expect(rows).toHaveLength(2);
    for (const row of rows) {
      const errorIcon = within(row).getByTestId("ai-gateway-logs-error-icon");
      expect(errorIcon).toBeInTheDocument();
      expect(errorIcon).toHaveClass("text-destructive");

      const tooltip = within(row).getByTestId("ai-gateway-logs-error-tooltip");
      expect(tooltip).toHaveTextContent(GENERIC_502_TITLE);
    }
  });

  it("非终止的失败尝试行显示尝试标签，终止记录不显示", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
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
    await screen.findByTestId("ai-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("ai-gateway-logs-row");
    expect(rows).toHaveLength(2);
    expect(
      within(rows[0]).getByTestId("ai-gateway-logs-provider-name"),
    ).toHaveTextContent("Terminal Provider");
    expect(
      within(rows[0]).queryByTestId("ai-gateway-logs-attempt-label"),
    ).not.toBeInTheDocument();
    expect(
      within(rows[1]).getByTestId("ai-gateway-logs-provider-name"),
    ).toHaveTextContent("Attempt Provider");
    const attemptLabel = within(rows[1]).getByTestId(
      "ai-gateway-logs-attempt-label",
    );
    expect(attemptLabel.textContent?.trim()).not.toBe("");
  });

  it("成功记录不显示失败原因行与错误提示", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        total: 1,
        records: [
          record({
            timestamp_ms: Date.UTC(2026, 8, 17, 4, 0),
            result: "success",
            status: 200,
            local_model: "gpt-4o",
          }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("ai-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("ai-gateway-logs-row");
    expect(rows).toHaveLength(1);
    expect(
      within(rows[0]).getByTestId("ai-gateway-logs-status-badge"),
    ).toHaveTextContent("Success");
    for (const row of rows) {
      expect(
        within(row).queryByTestId("ai-gateway-logs-status-reason"),
      ).not.toBeInTheDocument();
      expect(
        within(row).queryByTestId("ai-gateway-logs-error-tooltip"),
      ).not.toBeInTheDocument();
    }
  });

  it("状态徽章按 payload 仅展示 success 与 failure", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        total: 2,
        records: [
          record({
            timestamp_ms: Date.UTC(2026, 8, 17, 4, 0),
            result: "success",
            status: 200,
          }),
          record({
            timestamp_ms: Date.UTC(2026, 8, 17, 3, 0),
            result: "failure",
            status: 500,
          }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("ai-gateway-logs-ungrouped");

    const labels = screen
      .getAllByTestId("ai-gateway-logs-status-badge")
      .map(
        (badge) => within(badge).getByText(/^(Success|Failure)$/).textContent,
      );
    expect(labels).toEqual(["Success", "Failure"]);
  });

  it("同一请求的终止成功行与其失败尝试行按后端顺序（最新在前）相邻展示", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
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
    await screen.findByTestId("ai-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("ai-gateway-logs-row");
    expect(rows).toHaveLength(3);
    const providerOf = (row: HTMLElement) =>
      within(row).getByTestId("ai-gateway-logs-provider-name").textContent;

    expect(providerOf(rows[0])).toBe("Provider C");
    expect(
      within(rows[0]).getByTestId("ai-gateway-logs-status-badge"),
    ).toHaveTextContent("Success");
    expect(
      within(rows[0]).queryByTestId("ai-gateway-logs-attempt-label"),
    ).not.toBeInTheDocument();

    expect(providerOf(rows[1])).toBe("Provider B");
    expect(
      within(rows[1]).getByTestId("ai-gateway-logs-status-badge"),
    ).toHaveTextContent("Failure");
    expect(
      within(rows[1]).getByTestId("ai-gateway-logs-attempt-label"),
    ).toBeInTheDocument();

    expect(providerOf(rows[2])).toBe("Provider A");
    expect(
      within(rows[2]).getByTestId("ai-gateway-logs-status-badge"),
    ).toHaveTextContent("Failure");
    expect(
      within(rows[2]).getByTestId("ai-gateway-logs-attempt-label"),
    ).toBeInTheDocument();
  });

  it("terminal 字段缺省视为终止记录，不显示尝试标签", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
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
    await screen.findByTestId("ai-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("ai-gateway-logs-row");
    expect(rows).toHaveLength(1);
    const tooltip = within(rows[0]).getByTestId(
      "ai-gateway-logs-error-tooltip",
    );
    expect(tooltip).toHaveTextContent(STORED_UPSTREAM_MESSAGE);
    expect(
      within(rows[0]).queryByTestId("ai-gateway-logs-attempt-label"),
    ).not.toBeInTheDocument();
  });

  it("错误行状态列设置 whitespace-nowrap 且不直接输出错误正文，仅展示红色图标并在浮层中展示全部错误信息", async () => {
    const longErrorMessage =
      "Request failed with error: internal proxy error timeout connecting to backend upstream service over TLS after 30000ms";
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
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
    await screen.findByTestId("ai-gateway-logs-ungrouped");

    const rows = screen.getAllByTestId("ai-gateway-logs-row");
    expect(rows).toHaveLength(1);

    // 状态单元格应具备 whitespace-nowrap
    const statusCell = rows[0].querySelectorAll("td")[1];
    expect(statusCell).toHaveClass("whitespace-nowrap");

    // 状态单元格内错误文本只存在于浮层内部，触发按钮本身不平铺文字（仅为小图标）
    const trigger = within(statusCell).getByTestId("ai-gateway-logs-status-reason");
    expect(trigger).not.toHaveTextContent(longErrorMessage);

    // 红色图标存在
    const errorIcon = within(statusCell).getByTestId("ai-gateway-logs-error-icon");
    expect(errorIcon).toBeInTheDocument();
    expect(errorIcon).toHaveClass("text-destructive");

    // 鼠标滑过/浮层中展示全部错误信息（包含长错误文本和 HTTP 状态）
    const tooltip = within(statusCell).getByTestId("ai-gateway-logs-error-tooltip");
    expect(tooltip).toHaveClass("hidden");
    expect(tooltip).toHaveTextContent(longErrorMessage);
    expect(tooltip).toHaveTextContent("HTTP 504: Gateway timeout");
  });

  it("REQ-004 总耗时数值按原始毫秒分档（<3s 绿，3~15s 橙，>15s 红，非法灰）", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
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

    const table = await screen.findByTestId("ai-gateway-logs-ungrouped");
    const rows = within(table).getAllByTestId("ai-gateway-logs-row");
    expect(rows).toHaveLength(4);

    // Independent stored values -> three-decimal seconds with raw-value colors.
    expectPerformanceValue(latencyCell(rows[0]), LATENCY_TOTAL_VALUE, "1.200", LATENCY_GREEN);
    expectPerformanceValue(latencyCell(rows[1]), LATENCY_TOTAL_VALUE, "5.000", LATENCY_ORANGE);
    expectPerformanceValue(latencyCell(rows[2]), LATENCY_TOTAL_VALUE, "20.000", LATENCY_RED);
    expectMutedMetric(latencyCell(rows[3]), LATENCY_TOTAL_VALUE, LATENCY_TOTAL_UNIT);
  });

  it("当记录为 429 额度耗尽或网络错误时在详情中展示对应排查建议", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "ai_gateway_request_logs") {
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

    const table = await screen.findByTestId("ai-gateway-logs-ungrouped");
    const rows = within(table).getAllByTestId("ai-gateway-logs-row");
    expect(rows).toHaveLength(2);

    const hints = within(table).getAllByTestId("ai-gateway-logs-actionable-hint");
    expect(hints).toHaveLength(2);
    expect(hints[0]).toHaveTextContent("Suggestion: Upstream provider quota or periodic limit exhausted");
    expect(hints[1]).toHaveTextContent("Suggestion: Unable to connect to upstream URL");
  });

  it("不分组表格底部分页栏展示当前查询条件下的日志总条数（英文及千分位格式化）", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({ total: 1280, total_pages: 26 });
    });

    renderWithProviders(<UsageLogsPanel />);

    const totalCount = await screen.findByTestId("ai-gateway-logs-total-count");
    expect(totalCount).toHaveTextContent("Total 1,280 requests");
    expect(screen.getByText("Page 1 / 26")).toBeInTheDocument();
  });

  it("中文环境下底部分页栏展示当前查询条件下的日志总条数", async () => {
    await i18n.changeLanguage("zh");
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({ total: 256, total_pages: 6 });
    });

    renderWithProviders(<UsageLogsPanel />);

    const totalCount = await screen.findByTestId("ai-gateway-logs-total-count");
    expect(totalCount).toHaveTextContent("共 256 条");
    expect(screen.getByText("第 1 / 6 页")).toBeInTheDocument();
  });

  it("筛选条件变更后，底部分页栏日志总条数同步更新", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string, args: Record<string, unknown> = {}) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      if (args.status === "failure") {
        return page({ total: 15, total_pages: 1 });
      }
      return page({ total: 100, total_pages: 2 });
    });

    renderWithProviders(<UsageLogsPanel />);

    const totalCount = await screen.findByTestId("ai-gateway-logs-total-count");
    expect(totalCount).toHaveTextContent("Total 100 requests");

    // 打开筛选面板并选择 failure 状态
    await user.click(screen.getByTestId("ai-gateway-logs-filter-trigger"));
    await user.click(screen.getByRole("button", { name: /Failure/i }));

    await waitFor(() => {
      expect(screen.getByTestId("ai-gateway-logs-total-count")).toHaveTextContent("Total 15 requests");
    });
  });

  it("切换为 Provider 分组按服务商聚合，空名分组展示破折号且四类分组选项保持可选", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      if (args?.groupBy === "provider") {
        return page({
          group_by: "provider",
          records: [],
          total: 6,
          groups: [
            {
              group: "Alpha",
              request_count: 3,
              error_count: 1,
              last_request_at_ms: Date.UTC(2026, 8, 17, 5, 0),
            },
            {
              group: "Beta",
              request_count: 2,
              error_count: 0,
              last_request_at_ms: Date.UTC(2026, 8, 17, 4, 0),
            },
            {
              group: "",
              request_count: 1,
              error_count: 0,
              last_request_at_ms: Date.UTC(2026, 8, 17, 3, 0),
            },
          ],
        });
      }
      return page();
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("ai-gateway-logs-ungrouped");

    const groupTrigger = screen.getByTestId("ai-gateway-logs-group-trigger");
    await user.click(groupTrigger);
    expect(
      screen.getAllByRole("option").map((option) => option.textContent?.trim()),
    ).toEqual(["No grouping", "Model", "Provider", "Day (UTC+8)"]);
    await user.click(screen.getByRole("option", { name: "Provider" }));
    expect(groupTrigger).toHaveTextContent("Provider");

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_request_logs", {
        range: "today",
        groupBy: "provider",
        status: null,
        model: null,
        provider: null,
        page: 1,
      }),
    );

    const grouped = await screen.findByTestId("ai-gateway-logs-grouped");
    const rows = within(grouped).getAllByTestId("ai-gateway-logs-group-row");
    expect(rows).toHaveLength(3);
    const groupLabel = (row: HTMLElement) => row.querySelectorAll("td")[0];
    expect(groupLabel(rows[0])).toHaveTextContent("Alpha");
    expect(groupLabel(rows[1])).toHaveTextContent("Beta");
    expect(groupLabel(rows[2])).toHaveTextContent("—");

    // The other three grouping options remain offered after switching.
    await user.click(groupTrigger);
    expect(
      screen.getAllByRole("option").map((option) => option.textContent?.trim()),
    ).toEqual(["No grouping", "Model", "Provider", "Day (UTC+8)"]);
  });

  it("选择服务商并应用后请求 provider 且回到第 1 页，切换分组保持筛选，清除后重置筛选", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      const provider: string | null = args?.provider ?? null;
      const groupBy: string | null = args?.groupBy ?? null;
      const requestPage: number = args?.page ?? 1;
      if (groupBy === "provider") {
        return page({
          group_by: "provider",
          records: [],
          total: provider === "Beta" ? 1 : 2,
          providers: [],
          groups:
            provider === "Beta"
              ? [
                  {
                    group: "Beta",
                    request_count: 1,
                    error_count: 0,
                    last_request_at_ms: Date.UTC(2026, 8, 17, 4, 0),
                  },
                ]
              : [
                  {
                    group: "Alpha",
                    request_count: 1,
                    error_count: 0,
                    last_request_at_ms: Date.UTC(2026, 8, 17, 3, 0),
                  },
                  {
                    group: "Beta",
                    request_count: 1,
                    error_count: 0,
                    last_request_at_ms: Date.UTC(2026, 8, 17, 4, 0),
                  },
                ],
        });
      }
      if (provider === "Beta") {
        return page({
          page: requestPage,
          total: 1,
          total_pages: 2,
          providers: ["Alpha", "Beta"],
          records: [record({ provider_name: "Beta", local_model: "beta-only" })],
        });
      }
      return page({
        page: requestPage,
        total: 60,
        total_pages: 2,
        providers: ["Alpha", "Beta"],
        records: [
          record({
            provider_name: "Alpha",
            local_model: requestPage === 2 ? "alpha-page-two" : "alpha-only",
          }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("ai-gateway-logs-ungrouped");
    expect(screen.getByText("alpha-only")).toBeInTheDocument();

    // Page to page 2 first, so every later reset must actually return to page 1.
    await user.click(screen.getByRole("button", { name: "Next" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_request_logs", {
        range: "today",
        groupBy: "none",
        status: null,
        model: null,
        provider: null,
        page: 2,
      }),
    );
    expect(await screen.findByText("alpha-page-two")).toBeInTheDocument();

    const filterTrigger = screen.getByTestId("ai-gateway-logs-filter-trigger");
    await user.click(filterTrigger);
    const panel = await screen.findByTestId("ai-gateway-logs-filter-panel");
    // The provider chips are driven by the response `providers` list.
    expect(
      within(panel).getByRole("button", { name: "Alpha" }),
    ).toBeInTheDocument();
    await user.click(within(panel).getByRole("button", { name: "Beta" }));
    await user.click(within(panel).getByRole("button", { name: "Apply" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_request_logs", {
        range: "today",
        groupBy: "none",
        status: null,
        model: null,
        provider: "Beta",
        page: 1,
      }),
    );
    await waitFor(() => expect(filterTrigger).toHaveTextContent("Beta"));
    expect(await screen.findByText("beta-only")).toBeInTheDocument();
    expect(screen.queryByText("alpha-only")).not.toBeInTheDocument();

    // The provider filter survives switching the grouping mode, which also
    // resets paging to the first page.
    const groupTrigger = screen.getByTestId("ai-gateway-logs-group-trigger");
    await user.click(groupTrigger);
    await user.click(screen.getByRole("option", { name: "Provider" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_request_logs", {
        range: "today",
        groupBy: "provider",
        status: null,
        model: null,
        provider: "Beta",
        page: 1,
      }),
    );
    const grouped = await screen.findByTestId("ai-gateway-logs-grouped");
    expect(
      within(grouped).getByTestId("ai-gateway-logs-group-row"),
    ).toHaveTextContent("Beta");

    // Return to the ungrouped list so the pager is available, page to page 2
    // again, then clear: clearing must reset to page 1 and drop every filter.
    await user.click(groupTrigger);
    await user.click(screen.getByRole("option", { name: "No grouping" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_request_logs", {
        range: "today",
        groupBy: "none",
        status: null,
        model: null,
        provider: "Beta",
        page: 1,
      }),
    );
    await user.click(screen.getByRole("button", { name: "Next" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_request_logs", {
        range: "today",
        groupBy: "none",
        status: null,
        model: null,
        provider: "Beta",
        page: 2,
      }),
    );

    await user.click(filterTrigger);
    const reopenedPanel = await screen.findByTestId(
      "ai-gateway-logs-filter-panel",
    );
    await user.click(
      within(reopenedPanel).getByRole("button", { name: "Clear" }),
    );
    // The last request must be the cleared one: an identical initial page-1
    // call exists, so only a last-call assertion proves Clear reset paging.
    await waitFor(() =>
      expect(invokeMock).toHaveBeenLastCalledWith("ai_gateway_request_logs", {
        range: "today",
        groupBy: "none",
        status: null,
        model: null,
        provider: null,
        page: 1,
      }),
    );
  });

  it("响应缺少服务商清单时仅展示 Any provider 且不报错，新增分组与筛选文案具备中英文", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page();
    });

    const english = renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("ai-gateway-logs-ungrouped");

    await user.click(screen.getByTestId("ai-gateway-logs-group-trigger"));
    expect(
      screen.getByRole("option", { name: "Provider" }),
    ).toBeInTheDocument();
    await user.click(screen.getByTestId("ai-gateway-logs-filter-trigger"));
    const panel = await screen.findByTestId("ai-gateway-logs-filter-panel");
    const providerSection = within(panel).getByText("Provider")
      .parentElement as HTMLElement;
    expect(
      within(providerSection)
        .getAllByRole("button")
        .map((button) => button.textContent?.trim()),
    ).toEqual(["Any provider"]);
    // The list is never derived from visible records.
    expect(
      within(panel).queryByRole("button", { name: "Provider A" }),
    ).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();

    english.unmount();
    await i18n.changeLanguage("zh");

    renderWithProviders(<UsageLogsPanel />);
    await screen.findByTestId("ai-gateway-logs-ungrouped");

    await user.click(screen.getByTestId("ai-gateway-logs-group-trigger"));
    expect(
      screen.getByRole("option", { name: "服务商" }),
    ).toBeInTheDocument();
    await user.click(screen.getByTestId("ai-gateway-logs-filter-trigger"));
    const zhPanel = await screen.findByTestId("ai-gateway-logs-filter-panel");
    const zhProviderSection = within(zhPanel).getByText("服务商")
      .parentElement as HTMLElement;
    expect(
      within(zhProviderSection)
        .getAllByRole("button")
        .map((button) => button.textContent?.trim()),
    ).toEqual(["全部服务商"]);
  });

  // -------------------------------------------------------------------------
  // AC-001 / AC-002 / AC-006: stored cost breakdown is inspectable per row.
  // -------------------------------------------------------------------------

  const pricedRecord = (overrides: Partial<UsageLogRecord> = {}) =>
    record({
      input_tokens: 1_000_000,
      output_tokens: 1_000_000,
      cache_read_tokens: 2_000_000,
      cache_write_tokens: 0,
      total_tokens: 4_000_000,
      amount: 6,
      cost_breakdown: costBreakdown(),
      ...overrides,
    });

  it("AC-001 英文价目行的花费单元格提供可聚焦明细控件并展示七项费用、单价与总额", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({ total: 1, records: [pricedRecord()] });
    });

    renderWithProviders(<UsageLogsPanel />);
    const table = await screen.findByTestId("ai-gateway-logs-ungrouped");
    const row = within(table).getAllByTestId("ai-gateway-logs-row")[0];

    const control = costDetailControl(row);
    expect(control).toHaveAccessibleName();
    control.focus();
    expect(control).toHaveFocus();

    const detail = costDetailContent(row);
    expectCostDetailAssociated(control, detail);

    expect(detail).toHaveTextContent("Input");
    expect(detail).toHaveTextContent("$1.0000");
    expect(detail).toHaveTextContent("Output");
    expect(detail).toHaveTextContent("$4.0000");
    expect(detail).toHaveTextContent("Cache read");
    expect(detail).toHaveTextContent("$1.0000");
    expect(detail).toHaveTextContent("Total");
    expect(detail).toHaveTextContent("$6.0000");

    // Unit rates are USD per million tokens and keep their numeric value.
    expect(detail).toHaveTextContent("USD / million tokens");
    expect(detail).toHaveTextContent(/\$1(?![\d.])/);
    expect(detail).toHaveTextContent(/\$4(?![\d.])/);
    expect(detail).toHaveTextContent(/\$0\.5(?![\d])/);

    // cache_write_tokens = 0 hides the optional cache-write detail.
    expect(detail).not.toHaveTextContent("Cache write");
  });

  it("AC-001 中文价目行展示本地化标签并保留相同费用与单价", async () => {
    await i18n.changeLanguage("zh");
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({ total: 1, records: [pricedRecord()] });
    });

    renderWithProviders(<UsageLogsPanel />);
    const table = await screen.findByTestId("ai-gateway-logs-ungrouped");
    const row = within(table).getAllByTestId("ai-gateway-logs-row")[0];

    const control = costDetailControl(row);
    expect(control).toHaveAccessibleName();
    const detail = costDetailContent(row);
    expectCostDetailAssociated(control, detail);

    expect(detail).toHaveTextContent("输入");
    expect(detail).toHaveTextContent("$1.0000");
    expect(detail).toHaveTextContent("输出");
    expect(detail).toHaveTextContent("$4.0000");
    expect(detail).toHaveTextContent("缓存");
    expect(detail).toHaveTextContent("$1.0000");
    expect(detail).toHaveTextContent("总");
    expect(detail).toHaveTextContent("$6.0000");
    expect(detail).toHaveTextContent("美元 / 百万 tokens");
  });

  it("AC-002 存在缓存写入 token 时额外展示 cache-write 费用与单价，总额为 $7.0000", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        total: 1,
        records: [
          pricedRecord({
            cache_write_tokens: 500_000,
            total_tokens: 4_500_000,
            amount: 7,
            cost_breakdown: costBreakdown({ cache_write_cost: 1 }),
          }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    const table = await screen.findByTestId("ai-gateway-logs-ungrouped");
    const row = within(table).getAllByTestId("ai-gateway-logs-row")[0];

    const detail = costDetailContent(row);
    expect(detail).toHaveTextContent("Cache write");
    expect(detail).toHaveTextContent("$1.0000");
    expect(detail).toHaveTextContent(/\$2(?![\d.])/);
    expect(detail).toHaveTextContent("$7.0000");
  });

  it("AC-002 无缓存写入 token 时隐藏写入项，但仍保留零用量的 cache-read 费用与单价", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        total: 1,
        records: [
          pricedRecord({
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            amount: 5,
            cost_breakdown: costBreakdown({
              input_cost: 1,
              output_cost: 4,
              cache_read_cost: 0,
              cache_write_cost: 0,
            }),
          }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    const table = await screen.findByTestId("ai-gateway-logs-ungrouped");
    const row = within(table).getAllByTestId("ai-gateway-logs-row")[0];

    const detail = costDetailContent(row);
    expect(detail).toHaveTextContent("Cache read");
    expect(detail).toHaveTextContent("$0.0000");
    expect(detail).toHaveTextContent(/\$0\.5(?![\d])/);
    expect(detail).not.toHaveTextContent("Cache write");
  });

  it("AC-006 零用量价目行展示真实单价与全部 $0.0000 费用", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        total: 1,
        records: [
          pricedRecord({
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            total_tokens: 0,
            amount: 0,
            cost_breakdown: costBreakdown({
              input_cost: 0,
              output_cost: 0,
              cache_read_cost: 0,
              cache_write_cost: 0,
            }),
          }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    const table = await screen.findByTestId("ai-gateway-logs-ungrouped");
    const row = within(table).getAllByTestId("ai-gateway-logs-row")[0];

    const detail = costDetailContent(row);
    // Zero token counts still show the real effective rates.
    expect(detail).toHaveTextContent(/\$1(?![\d.])/);
    expect(detail).toHaveTextContent(/\$4(?![\d.])/);
    expect(detail).toHaveTextContent(/\$0\.5(?![\d])/);
    // Every fee rounds to $0.0000 and the total is an explicit zero.
    expect(detail).toHaveTextContent("$0.0000");
    expect(detail).not.toHaveTextContent("—");
  });

  it("AC-006 旧行缺失快照与未计价行显示 — 和本地化不可用说明，且不按当前价格回退", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        total: 3,
        records: [
          record({ local_model: "unpriced", amount: null, cost_breakdown: null }),
          record({
            local_model: "legacy-null-snapshot",
            amount: 0.5,
            cost_breakdown: null,
          }),
          // Snapshot field omitted entirely, mirroring an old payload.
          record({ local_model: "legacy-missing-field", amount: 0.5 }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    const table = await screen.findByTestId("ai-gateway-logs-ungrouped");
    const rows = within(table).getAllByTestId("ai-gateway-logs-row");
    expect(rows).toHaveLength(3);

    for (const row of rows) {
      const control = costDetailControl(row);
      expect(control).toHaveAccessibleName();
      const detail = costDetailContent(row);
      // Unknown component fees/rates render as dashes. REQ-001/REQ-003 keep the
      // recorded total, so the known legacy amount may remain a currency value.
      expect(detail).toHaveTextContent("—");
      // No fabricated priced components from the current configuration.
      expect(detail).not.toHaveTextContent("$1.0000");
      expect(detail).not.toHaveTextContent("$4.0000");
      // The exact localized unavailable explanation must be present, so the
      // clause is genuinely evidenced rather than satisfied by row labels.
      expect(
        within(detail).getByText(
          "Recorded pricing details are unavailable; unknown fees and rates are shown as —.",
        ),
      ).toBeInTheDocument();
      // Each unknown component fee and rate renders a dash (Input, Output and
      // Cache read are always shown, i.e. at least three components).
      expect(
        (detail.textContent?.match(/—/g) ?? []).length,
        "每个未知组件费用与单价都应显示 —",
      ).toBeGreaterThanOrEqual(6);
    }

    const rowFor = (model: string) =>
      rows.find((row) => within(row).queryByText(model) !== null) as HTMLElement;
    // The existing amount stays authoritative for priced legacy rows; the
    // unpriced row keeps its existing dash.
    expect(rowCostCell(rowFor("legacy-null-snapshot"))).toHaveTextContent("0.5000");
    expect(rowCostCell(rowFor("legacy-missing-field"))).toHaveTextContent("0.5000");
    expect(rowCostCell(rowFor("unpriced"))).toHaveTextContent("—");
  });

  it("AC-006 中文环境下缺失快照行给出中文不可用说明且不展示虚构金额", async () => {
    await i18n.changeLanguage("zh");
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        total: 1,
        records: [record({ amount: 0.5, cost_breakdown: null })],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    const table = await screen.findByTestId("ai-gateway-logs-ungrouped");
    const row = within(table).getAllByTestId("ai-gateway-logs-row")[0];

    const detail = costDetailContent(row);
    // Unknown component fees/rates render as dashes; the recorded total may
    // stay as `$0.5000` or any localized currency formatting.
    expect(detail).toHaveTextContent("—");
    expect(detail).not.toHaveTextContent("$1.0000");
    expect(detail).not.toHaveTextContent("$4.0000");
    // The exact delivered Chinese unavailable explanation must be present.
    expect(
      within(detail).getByText(
        "记录的价格明细不可用；未知费用与单价显示为 —。",
      ),
    ).toBeInTheDocument();
    expect(
      (detail.textContent?.match(/—/g) ?? []).length,
      "每个未知组件费用与单价都应显示 —",
    ).toBeGreaterThanOrEqual(6);
  });

  it("AC-001/AC-002 明细控件在普通行与表格末行均可通过键盘聚焦并与明细关联", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        total: 5,
        records: Array.from({ length: 5 }, (_, index) =>
          record({
            timestamp_ms: Date.UTC(2026, 8, 17, index, 0),
            local_model: `priced-${index}`,
            upstream_model: `priced-${index}`,
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cache_read_tokens: 2_000_000,
            cache_write_tokens: 0,
            total_tokens: 4_000_000,
            amount: 6,
            cost_breakdown: costBreakdown(),
          }),
        ),
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    const table = await screen.findByTestId("ai-gateway-logs-ungrouped");
    const rows = within(table).getAllByTestId("ai-gateway-logs-row");
    expect(rows).toHaveLength(5);

    // A normal row and the end-of-table row both expose a keyboard-focusable,
    // associated control. Positioning is left to the bounded real-render smoke.
    for (const row of [rows[0], rows[rows.length - 1]]) {
      const control = costDetailControl(row);
      expect(control).toHaveAccessibleName();
      control.focus();
      expect(control).toHaveFocus();
      expectCostDetailAssociated(control, costDetailContent(row));
    }
  });

  // Review-repair (2026-10-13), SPEC-1 regression: a model-pool card log
  // shortcut focuses one alias once via (focusModel, focusToken>=1) and the
  // parent never clears it while both usage panes stay mounted. Changing the
  // model filter afterwards must still issue the new query on page 1 instead of
  // being blocked forever by the pending-focus guard.
  it("SPEC-1 外部 alias 聚焦后，切换模型筛选仍发出新的日志请求", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      const selected: string | null = args?.model ?? null;
      return {
        ...page({ records: [record({ local_model: selected ?? "free-model" })] }),
        models: ["free-model", "other-model"],
      } as UsageLogsPage & { models: string[] };
    });

    // Simulate the parent pool-card log shortcut: the pool alias is focused
    // once and the parent never clears focusModel/focusToken.
    renderWithProviders(
      <UsageLogsPanel focusModel="free-model" focusToken={1} />,
    );

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_request_logs", {
        range: "today",
        groupBy: "none",
        status: null,
        model: "free-model",
        provider: null,
        page: 1,
      }),
    );

    // The existing manual Refresh control must keep working after the focus.
    const logsCalls = () =>
      invokeMock.mock.calls.filter(
        (call) => call[0] === "ai_gateway_request_logs",
      ).length;
    const beforeRefresh = logsCalls();
    await user.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(logsCalls()).toBeGreaterThan(beforeRefresh));

    // The user then picks a different model and applies the filter; the next
    // query must carry the new model value on page 1.
    await user.click(screen.getByTestId("ai-gateway-logs-filter-trigger"));
    const panel = await screen.findByTestId("ai-gateway-logs-filter-panel");
    await user.click(
      within(panel).getByRole("button", { name: "other-model" }),
    );
    await user.click(within(panel).getByRole("button", { name: "Apply" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_request_logs", {
        range: "today",
        groupBy: "none",
        status: null,
        model: "other-model",
        provider: null,
        page: 1,
      }),
    );
  });

  // -------------------------------------------------------------------------
  // Step 2 (REQ-001/REQ-002/REQ-003/REQ-004): the rendered latency cell.
  // -------------------------------------------------------------------------

  it("REQ-001/REQ-002 延迟单元格展示三条对齐指标、独立颜色与可见单位（英文）", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      // Approved example: start 0s, first output 1s, end 5s, 200 output tokens.
      return page({
        total: 1,
        records: [
          record({
            duration_ms: 5000,
            first_token_ms: 1000,
            avg_tps: 50,
            output_tokens: 200,
          }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    const table = await screen.findByTestId("ai-gateway-logs-ungrouped");

    // REQ-001: the column is labeled Latency; the other five columns survive.
    expect(within(table).getByText("Latency")).toBeInTheDocument();
    for (const header of ["Time", "Status", "Model", "Tokens", "Cost ($)"]) {
      expect(within(table).getByText(header)).toBeInTheDocument();
    }
    expect(table.querySelectorAll("th").length).toBe(6);

    const rows = within(table).getAllByTestId("ai-gateway-logs-row");
    expect(rows).toHaveLength(1);
    const cell = latencyCell(rows[0]);

    // Exactly three labeled lines.
    expect(within(cell).getAllByTestId(/-label$/)).toHaveLength(3);
    expect(latencyValue(cell, LATENCY_TOTAL_LABEL).textContent?.trim()).toBe("Total");
    expect(latencyValue(cell, LATENCY_FIRST_TOKEN_LABEL).textContent?.trim()).toBe("First token");
    expect(latencyValue(cell, LATENCY_AVG_TPS_LABEL).textContent?.trim()).toBe("Average TPS");

    // Stored measurements render at three-decimal seconds / two-decimal TPS.
    // 5s and 1s fall in the orange band; 50 t/s is orange too.
    expectPerformanceValue(cell, LATENCY_TOTAL_VALUE, "5.000", LATENCY_ORANGE);
    expectPerformanceValue(cell, LATENCY_FIRST_TOKEN_VALUE, "1.000", LATENCY_ORANGE);
    expectPerformanceValue(cell, LATENCY_AVG_TPS_VALUE, "50.00", LATENCY_ORANGE);

    // Units are separate, visible and muted; they never carry a performance color.
    for (const [marker, unit] of [
      [LATENCY_TOTAL_UNIT, "s"],
      [LATENCY_FIRST_TOKEN_UNIT, "s"],
      [LATENCY_AVG_TPS_UNIT, "t/s"],
    ] as const) {
      const unitEl = latencyValue(cell, marker);
      expect(unitEl.textContent).toBe(unit);
      expect(unitEl.className).toMatch(LATENCY_MUTED);
      expect(unitEl.className).not.toMatch(LATENCY_GREEN);
      expect(unitEl.className).not.toMatch(LATENCY_ORANGE);
      expect(unitEl.className).not.toMatch(LATENCY_RED);
    }

    // Labels are muted and never carry a performance color.
    for (const marker of [
      LATENCY_TOTAL_LABEL,
      LATENCY_FIRST_TOKEN_LABEL,
      LATENCY_AVG_TPS_LABEL,
    ]) {
      const label = latencyValue(cell, marker);
      expect(label.className).toMatch(LATENCY_MUTED);
      expect(label.className).not.toMatch(LATENCY_GREEN);
      expect(label.className).not.toMatch(LATENCY_ORANGE);
      expect(label.className).not.toMatch(LATENCY_RED);
    }

    // Values are right-aligned and use tabular numerals.
    for (const marker of [
      LATENCY_TOTAL_VALUE,
      LATENCY_FIRST_TOKEN_VALUE,
      LATENCY_AVG_TPS_VALUE,
    ]) {
      expectAlignedRight(latencyValue(cell, marker));
    }

    // Exactly three metric values inside the whole cell marker.
    expect(within(cell).getAllByTestId(/-value$/)).toHaveLength(3);
  });

  it("REQ-001 延迟单元格与三条指标标签提供中文文案", async () => {
    await i18n.changeLanguage("zh");
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        total: 1,
        records: [
          record({ duration_ms: 5000, first_token_ms: 1000, avg_tps: 50 }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    const table = await screen.findByTestId("ai-gateway-logs-ungrouped");

    expect(within(table).getByText("延迟")).toBeInTheDocument();
    const row = within(table).getAllByTestId("ai-gateway-logs-row")[0];
    const cell = latencyCell(row);
    expect(latencyValue(cell, LATENCY_TOTAL_LABEL).textContent?.trim()).toBe("总耗时");
    expect(latencyValue(cell, LATENCY_FIRST_TOKEN_LABEL).textContent?.trim()).toBe("首字");
    expect(latencyValue(cell, LATENCY_AVG_TPS_LABEL).textContent?.trim()).toBe("平均 TPS");
    expectPerformanceValue(cell, LATENCY_TOTAL_VALUE, "5.000", LATENCY_ORANGE);
    expectPerformanceValue(cell, LATENCY_FIRST_TOKEN_VALUE, "1.000", LATENCY_ORANGE);
    expectPerformanceValue(cell, LATENCY_AVG_TPS_VALUE, "50.00", LATENCY_ORANGE);
  });

  it("REQ-003 缺失/非法指标显示单个静音破折号，显式零仍显示数值", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        total: 6,
        records: [
          record({
            timestamp_ms: 6000,
            local_model: "null-metrics",
            duration_ms: 5000,
            first_token_ms: null,
            avg_tps: null,
          }),
          record({
            timestamp_ms: 5000,
            local_model: "missing-metrics",
            duration_ms: 5000,
          }),
          record({
            timestamp_ms: 4000,
            local_model: "negative-metrics",
            duration_ms: 5000,
            first_token_ms: -1,
            avg_tps: -5,
          }),
          record({
            timestamp_ms: 3000,
            local_model: "nonfinite-metrics",
            duration_ms: 5000,
            first_token_ms: Number.NaN,
            avg_tps: Number.POSITIVE_INFINITY,
          }),
          record({
            timestamp_ms: 2000,
            local_model: "zero-metrics",
            duration_ms: 5000,
            first_token_ms: 0,
            avg_tps: 0,
          }),
          record({
            timestamp_ms: 1000,
            local_model: "invalid-total",
            duration_ms: Number.NaN,
          }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    const table = await screen.findByTestId("ai-gateway-logs-ungrouped");
    const rows = within(table).getAllByTestId("ai-gateway-logs-row");
    expect(rows).toHaveLength(6);

    // Null, missing, negative and non-finite first-token/TPS render one muted dash.
    for (const row of rows.slice(0, 4)) {
      const cell = latencyCell(row);
      expectMutedMetric(cell, LATENCY_FIRST_TOKEN_VALUE, LATENCY_FIRST_TOKEN_UNIT);
      expectMutedMetric(cell, LATENCY_AVG_TPS_VALUE, LATENCY_AVG_TPS_UNIT);
      // The total duration stays an available measurement.
      expect(latencyValue(cell, LATENCY_TOTAL_VALUE).textContent).not.toBe("—");
    }

    // Explicit zero is a measured value, distinct from unknown: not a dash, not muted.
    const zeroCell = latencyCell(rows[4]);
    const zeroFirst = latencyValue(zeroCell, LATENCY_FIRST_TOKEN_VALUE);
    const zeroTps = latencyValue(zeroCell, LATENCY_AVG_TPS_VALUE);
    expect(zeroFirst.textContent).toBe("0.000");
    expect(zeroTps.textContent).toBe("0.00");
    expect(zeroFirst.className).not.toMatch(LATENCY_MUTED);
    expect(zeroTps.className).not.toMatch(LATENCY_MUTED);

    // An invalid total also renders one muted dash.
    expectMutedMetric(latencyCell(rows[5]), LATENCY_TOTAL_VALUE, LATENCY_TOTAL_UNIT);
  });

  it("REQ-004 三个指标各自按原始值取色，含显示相同但原始跨阈值的值", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        total: 5,
        records: [
          record({
            timestamp_ms: 5000,
            local_model: "below",
            duration_ms: 2999.6,
            first_token_ms: 999.6,
            avg_tps: 59.999,
          }),
          record({
            timestamp_ms: 4000,
            local_model: "at-low",
            duration_ms: 3000,
            first_token_ms: 1000,
            avg_tps: 60,
          }),
          record({
            timestamp_ms: 3000,
            local_model: "at-high",
            duration_ms: 15000,
            first_token_ms: 3000,
            avg_tps: 20,
          }),
          record({
            timestamp_ms: 2000,
            local_model: "above",
            duration_ms: 15000.4,
            first_token_ms: 3000.4,
            avg_tps: 19.999,
          }),
          record({
            timestamp_ms: 1000,
            local_model: "high",
            duration_ms: 15001,
            first_token_ms: 3001,
            avg_tps: 600,
          }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    const table = await screen.findByTestId("ai-gateway-logs-ungrouped");
    const rows = within(table).getAllByTestId("ai-gateway-logs-row");
    expect(rows).toHaveLength(5);

    // Row 0: each value rounds up across a threshold but keeps its raw segment.
    const c0 = latencyCell(rows[0]);
    expectPerformanceValue(c0, LATENCY_TOTAL_VALUE, "3.000", LATENCY_GREEN);
    expectPerformanceValue(c0, LATENCY_FIRST_TOKEN_VALUE, "1.000", LATENCY_GREEN);
    expectPerformanceValue(c0, LATENCY_AVG_TPS_VALUE, "60.00", LATENCY_ORANGE);

    // Row 1: exact inclusive lower boundaries -> orange / orange / green.
    const c1 = latencyCell(rows[1]);
    expectPerformanceValue(c1, LATENCY_TOTAL_VALUE, "3.000", LATENCY_ORANGE);
    expectPerformanceValue(c1, LATENCY_FIRST_TOKEN_VALUE, "1.000", LATENCY_ORANGE);
    expectPerformanceValue(c1, LATENCY_AVG_TPS_VALUE, "60.00", LATENCY_GREEN);

    // Row 2: inclusive upper orange boundaries (15s, 3s) and 60 is not; 20 t/s is orange.
    const c2 = latencyCell(rows[2]);
    expectPerformanceValue(c2, LATENCY_TOTAL_VALUE, "15.000", LATENCY_ORANGE);
    expectPerformanceValue(c2, LATENCY_FIRST_TOKEN_VALUE, "3.000", LATENCY_ORANGE);
    expectPerformanceValue(c2, LATENCY_AVG_TPS_VALUE, "20.00", LATENCY_ORANGE);

    // Row 3: rounded-lookalike values just above -> red / red / red.
    const c3 = latencyCell(rows[3]);
    expectPerformanceValue(c3, LATENCY_TOTAL_VALUE, "15.000", LATENCY_RED);
    expectPerformanceValue(c3, LATENCY_FIRST_TOKEN_VALUE, "3.000", LATENCY_RED);
    expectPerformanceValue(c3, LATENCY_AVG_TPS_VALUE, "20.00", LATENCY_RED);

    // Row 4: whole values above the boundary -> red / red / green.
    const c4 = latencyCell(rows[4]);
    expectPerformanceValue(c4, LATENCY_TOTAL_VALUE, "15.001", LATENCY_RED);
    expectPerformanceValue(c4, LATENCY_FIRST_TOKEN_VALUE, "3.001", LATENCY_RED);
    expectPerformanceValue(c4, LATENCY_AVG_TPS_VALUE, "600.00", LATENCY_GREEN);
  });

  it("REQ-004 可用指标值提供同色系亮/暗主题可读色阶，不可用值保持静音", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command !== "ai_gateway_request_logs") {
        throw new Error(`Unhandled command: ${command}`);
      }
      return page({
        total: 4,
        records: [
          record({
            timestamp_ms: 4000,
            local_model: "green",
            duration_ms: 1200,
            first_token_ms: 500,
            avg_tps: 100,
          }),
          record({
            timestamp_ms: 3000,
            local_model: "orange",
            duration_ms: 5000,
            first_token_ms: 1000,
            avg_tps: 50,
          }),
          record({
            timestamp_ms: 2000,
            local_model: "red",
            duration_ms: 20000,
            first_token_ms: 4000,
            avg_tps: 10,
          }),
          record({
            timestamp_ms: 1000,
            local_model: "unavailable",
            duration_ms: Number.NaN,
            first_token_ms: null,
            avg_tps: null,
          }),
        ],
      });
    });

    renderWithProviders(<UsageLogsPanel />);
    const table = await screen.findByTestId("ai-gateway-logs-ungrouped");
    const rows = within(table).getAllByTestId("ai-gateway-logs-row");
    expect(rows).toHaveLength(4);

    const expectRow = (
      row: HTMLElement,
      family: keyof typeof LATENCY_FAMILY_TONES,
    ) => {
      const cell = latencyCell(row);
      expectReadableLatencyVariant(
        latencyValue(cell, LATENCY_TOTAL_VALUE),
        family,
      );
      expectReadableLatencyVariant(
        latencyValue(cell, LATENCY_FIRST_TOKEN_VALUE),
        family,
      );
      expectReadableLatencyVariant(
        latencyValue(cell, LATENCY_AVG_TPS_VALUE),
        family,
      );
    };

    expectRow(rows[0], "green");
    expectRow(rows[1], "orange");
    expectRow(rows[2], "red");

    const unavailable = latencyCell(rows[3]);
    expectMutedMetric(unavailable, LATENCY_TOTAL_VALUE, LATENCY_TOTAL_UNIT);
    expectMutedMetric(
      unavailable,
      LATENCY_FIRST_TOKEN_VALUE,
      LATENCY_FIRST_TOKEN_UNIT,
    );
    expectMutedMetric(unavailable, LATENCY_AVG_TPS_VALUE, LATENCY_AVG_TPS_UNIT);
  });
});



