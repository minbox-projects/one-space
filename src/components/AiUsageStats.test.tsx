import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AiUsageStats } from "@/components/AiUsageStats";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

type ToolId = "claude" | "codex" | "antigravity" | "opencode";

interface InvokeArgs {
  tool?: ToolId;
  days?: 7 | 15 | 30;
  date?: string;
}

interface AntigravityQuotaBucket {
  id: string;
  name: string;
  window: string;
  remaining_fraction: number;
  reset_time: string;
  description: string | null;
}

interface AntigravityQuotaGroup {
  name: string;
  description: string | null;
  buckets: AntigravityQuotaBucket[];
}

// Local type mirroring the unexported product interface shape so the helper's
// return type is known.  Product file intentionally does not export it, per
// scope rules.
interface AiUsageToolStats {
  tool: ToolId;
  source_status: string;
  summary: {
    total_tokens: number;
    calls: number;
    sessions: number;
    cache_hit_rate: number;
    input_tokens: number;
    output_tokens: number;
    cache_tokens: number;
  };
  daily: AiUsageDailyItem[];
  peak_day: { date: string; total_tokens: number; calls: number } | null;
  scanned_sessions: number;
  scanned_calls: number;
  errors: string[];
}

interface AiUsageDailyItem {
  date: string;
  total_tokens: number;
  calls: number;
  sessions: number;
  cache_hit_rate: number;
  input_tokens: number;
  output_tokens: number;
  cache_tokens: number;
}

interface AiUsageDayBreakdown {
  tool: ToolId;
  total_tokens: number;
  calls: number;
  input_tokens: number;
  output_tokens: number;
  cache_tokens: number;
  cache_hit_rate: number;
  models: Array<{
    model: string;
    total_tokens: number;
    calls: number;
    sessions: number;
    input_tokens: number;
    output_tokens: number;
    cache_tokens: number;
    cache_hit_rate: number;
  }>;
}

interface AiUsageDayStats {
  date: string;
  total_tokens: number;
  calls: number;
  sessions: number;
  input_tokens: number;
  output_tokens: number;
  cache_tokens: number;
  breakdown: AiUsageDayBreakdown[];
}

const tools: ToolId[] = ["claude", "codex", "antigravity", "opencode"];

function makeDayStats(date: string): AiUsageDayStats {
  const breakdown: AiUsageDayBreakdown[] = [
    {
      tool: "claude",
      total_tokens: 12000000,
      calls: 6,
      input_tokens: 8000000,
      output_tokens: 3000000,
      cache_tokens: 1000000,
      cache_hit_rate: 42,
      models: [
        { model: "claude-opus-4-6", total_tokens: 9000000, calls: 4, sessions: 2, input_tokens: 6000000, output_tokens: 2200000, cache_tokens: 800000, cache_hit_rate: 40 },
        { model: "claude-sonnet-4-5", total_tokens: 3000000, calls: 2, sessions: 1, input_tokens: 2000000, output_tokens: 800000, cache_tokens: 200000, cache_hit_rate: 45 },
      ],
    },
    {
      tool: "codex",
      total_tokens: 2222,
      calls: 1,
      input_tokens: 1000,
      output_tokens: 900,
      cache_tokens: 100,
      cache_hit_rate: 10,
      models: [
        { model: "gpt-5-codex", total_tokens: 2222, calls: 1, sessions: 1, input_tokens: 1000, output_tokens: 900, cache_tokens: 100, cache_hit_rate: 10 },
      ],
    },
    { tool: "antigravity", total_tokens: 0, calls: 0, input_tokens: 0, output_tokens: 0, cache_tokens: 0, cache_hit_rate: 0, models: [] },
    {
      tool: "opencode",
      total_tokens: 500,
      calls: 2,
      input_tokens: 300,
      output_tokens: 150,
      cache_tokens: 50,
      cache_hit_rate: 25,
      models: [
        { model: "deepseek-v4", total_tokens: 500, calls: 2, sessions: 1, input_tokens: 300, output_tokens: 150, cache_tokens: 50, cache_hit_rate: 25 },
      ],
    },
  ];
  return {
    date,
    total_tokens: breakdown.reduce((s, b) => s + b.total_tokens, 0),
    calls: breakdown.reduce((s, b) => s + b.calls, 0),
    sessions: 5,
    input_tokens: breakdown.reduce((s, b) => s + b.input_tokens, 0),
    output_tokens: breakdown.reduce((s, b) => s + b.output_tokens, 0),
    cache_tokens: breakdown.reduce((s, b) => s + b.cache_tokens, 0),
    breakdown,
  };
}

function makeEmptyDayStats(date: string): AiUsageDayStats {
  return {
    date,
    total_tokens: 0,
    calls: 0,
    sessions: 0,
    input_tokens: 0,
    output_tokens: 0,
    cache_tokens: 0,
    breakdown: tools.map((tool) => ({
      tool,
      total_tokens: 0,
      calls: 0,
      input_tokens: 0,
      output_tokens: 0,
      cache_tokens: 0,
      cache_hit_rate: 0,
      models: [],
    })),
  };
}

function makeToolStats(tool: ToolId, days: 7 | 15 | 30) {
  const dates = Array.from({ length: days }, (_, index) => {
    const day = String(index + 1).padStart(2, "0");
    return `2026-06-${day}`;
  });
  const emptySummary = {
    total_tokens: 0,
    calls: 0,
    sessions: 0,
    cache_hit_rate: 0,
    input_tokens: 0,
    output_tokens: 0,
    cache_tokens: 0,
  };
  const emptyDaily = dates.map((date) => ({
    date,
    ...emptySummary,
  }));

  if (tool === "claude") {
    return {
      tool,
      source_status: "available",
      summary: {
        total_tokens: 12000000,
        calls: 6,
        sessions: 2,
        cache_hit_rate: 25,
        input_tokens: 8000000,
        output_tokens: 3000000,
        cache_tokens: 1000000,
      },
      daily: dates.map((date, index) => ({
        date,
        total_tokens: index === dates.length - 1 ? 12000000 : 3000,
        calls: index === dates.length - 1 ? 4 : 2,
        sessions: 1,
        cache_hit_rate: 25,
        input_tokens: index === dates.length - 1 ? 8000000 : 1000,
        output_tokens: index === dates.length - 1 ? 3000000 : 500,
        cache_tokens: index === dates.length - 1 ? 1000000 : 250,
      })),
      peak_day: {
        date: dates[dates.length - 1],
        total_tokens: 12000000,
        calls: 4,
      },
      scanned_sessions: 4,
      scanned_calls: 6,
      errors: [],
    };
  }

  if (tool === "codex") {
    return {
      tool,
      source_status: "available",
      summary: {
        total_tokens: 2222,
        calls: 1,
        sessions: 1,
        cache_hit_rate: 10,
        input_tokens: 1000,
        output_tokens: 900,
        cache_tokens: 100,
      },
      daily: emptyDaily.map((day, index) =>
        index === 0
          ? {
              ...day,
              total_tokens: 2222,
              calls: 1,
              sessions: 1,
              input_tokens: 1000,
              output_tokens: 900,
              cache_tokens: 100,
              cache_hit_rate: 10,
            }
          : day,
      ),
      peak_day: { date: dates[0], total_tokens: 2222, calls: 1 },
      scanned_sessions: 1,
      scanned_calls: 1,
      errors: [],
    };
  }

  if (tool === "opencode") {
    return {
      tool,
      source_status: "error",
      summary: emptySummary,
      daily: emptyDaily,
      peak_day: null,
      scanned_sessions: 0,
      scanned_calls: 0,
      errors: ["broken source"],
    };
  }

  // Default antigravity — "unavailable" with no scanned sessions.
  return {
    tool,
    source_status: "unavailable",
    summary: emptySummary,
    daily: emptyDaily,
    peak_day: null,
    scanned_sessions: 0,
    scanned_calls: 0,
    errors: [],
  };
}

function makeToolStatsAntigravityEmpty(): AiUsageToolStats {
  const dates = Array.from({ length: 7 }, (_, index) => {
    const day = String(index + 1).padStart(2, "0");
    return `2026-06-${day}`;
  });
  const emptySummary = {
    total_tokens: 0,
    calls: 0,
    sessions: 0,
    cache_hit_rate: 0,
    input_tokens: 0,
    output_tokens: 0,
    cache_tokens: 0,
  };
  const emptyDaily = dates.map((date) => ({
    date,
    ...emptySummary,
  }));
  return {
    tool: "antigravity",
    source_status: "empty",
    summary: emptySummary,
    daily: emptyDaily,
    peak_day: null,
    scanned_sessions: 2,
    scanned_calls: 3,
    errors: [],
  };
}

function makeAntigravityQuotaResponse(): { groups: AntigravityQuotaGroup[] } {
  return {
    groups: [
      {
        name: "Free tier",
        description: "Default free usage bucket",
        buckets: [
          {
            id: "free-daily",
            name: "Daily requests",
            window: "daily",
            remaining_fraction: 0.245,
            reset_time: "2026-07-08T00:00:00Z",
            description: "Reset every day at midnight UTC",
          },
        ],
      },
      {
        name: "Pro tier",
        description: null,
        buckets: [
          {
            id: "pro-monthly",
            name: "Monthly tokens",
            window: "monthly",
            remaining_fraction: 0.875,
            reset_time: "2026-08-01T00:00:00Z",
            description: "End-of-month reset",
          },
        ],
      },
    ],
  };
}

describe("AiUsageStats", () => {
  beforeEach(() => {
    resetTauriMocks();
    invokeMock.mockImplementation(async (command: string, args?: InvokeArgs) => {
      if (command === "sessions_usage_tool_stats") {
        return makeToolStats(args?.tool || "claude", args?.days || 7);
      }
      if (command === "sessions_usage_day_stats") {
        return makeDayStats(args?.date || "");
      }
      if (command === "sessions_usage_clear_cache") {
        return null;
      }
      throw new Error(`Unhandled command: ${command}`);
    });
  });

  it("renders shell and four tool loading states before data resolves", () => {
    invokeMock.mockImplementation(() => new Promise(() => {}));

    renderWithProviders(<AiUsageStats />);

    expect(
      screen.getByRole("heading", { name: /AI Usage Stats|AI 用量统计/ }),
    ).toBeInTheDocument();
    for (const tool of tools) {
      expect(screen.getByTestId(`ai-usage-tool-${tool}`)).toBeInTheDocument();
    }
    expect(
      screen.getAllByText(/Loading usage data\.\.\.|正在加载用量数据\.\.\./).length,
    ).toBe(2);
    expect(
      screen.getAllByText(/Loading\.\.\.|加载中\.\.\./).length,
    ).toBeGreaterThanOrEqual(8);
  });

  it("requests each tool with default 7 day window", async () => {
    renderWithProviders(<AiUsageStats />);

    await waitFor(() => {
      for (const tool of tools) {
        expect(invokeMock).toHaveBeenCalledWith("sessions_usage_tool_stats", {
          tool,
          days: 7,
        });
      }
    });
  });

  it("switches 15d and 30d by requesting all tools again", async () => {
    const user = userEvent.setup();
    renderWithProviders(<AiUsageStats />);

    await user.click(screen.getByRole("button", { name: /15d|15天/ }));
    await user.click(screen.getByRole("button", { name: /30d|30天/ }));

    await waitFor(() => {
      for (const tool of tools) {
        expect(invokeMock).toHaveBeenCalledWith("sessions_usage_tool_stats", {
          tool,
          days: 15,
        });
        expect(invokeMock).toHaveBeenCalledWith("sessions_usage_tool_stats", {
          tool,
          days: 30,
        });
      }
    });
  });

  it("clears cached scans and refreshes both window and selected day", async () => {
    const user = userEvent.setup();
    renderWithProviders(<AiUsageStats />);
    const dateInput = screen.getByLabelText(/Select Date|选择日期/);

    await waitFor(() => expect(screen.getByText("2.2K")).toBeInTheDocument());
    invokeMock.mockClear();
    await user.click(screen.getByRole("button", { name: /Refresh|刷新/ }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("sessions_usage_clear_cache");
      expect(invokeMock).toHaveBeenCalledWith("sessions_usage_day_stats", {
        date: (dateInput as HTMLInputElement).value,
      });
      for (const tool of tools) {
        expect(invokeMock).toHaveBeenCalledWith("sessions_usage_tool_stats", {
          tool,
          days: 7,
        });
      }
    });
  });

  it("ignores a slower initial response after the user selects another date", async () => {
    let resolveInitialDay!: () => void;
    const initialDayGate = new Promise<void>((resolve) => {
      resolveInitialDay = resolve;
    });
    let initialDate = "";
    invokeMock.mockImplementation(async (command: string, args?: InvokeArgs) => {
      if (command === "sessions_usage_tool_stats") {
        return makeToolStats(args?.tool || "claude", args?.days || 7);
      }
      if (command === "sessions_usage_day_stats") {
        const date = args?.date || "";
        if (date !== "2026-06-07") {
          initialDate = date;
          await initialDayGate;
          return makeEmptyDayStats(date);
        }
        return makeDayStats(date);
      }
      throw new Error(`Unhandled command: ${command}`);
    });
    renderWithProviders(<AiUsageStats />);

    fireEvent.change(screen.getByLabelText(/Select Date|选择日期/), {
      target: { value: "2026-06-07" },
    });
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("sessions_usage_day_stats", {
        date: "2026-06-07",
      });
    });

    const section = screen.getByTestId("ai-usage-day-stats");
    expect(await within(section).findByText("12M")).toBeInTheDocument();
    await act(async () => {
      resolveInitialDay();
      await Promise.resolve();
    });

    const dayCalls = invokeMock.mock.calls.filter(
      ([command]) => command === "sessions_usage_day_stats",
    );
    expect(dayCalls).toEqual([
      ["sessions_usage_day_stats", { date: initialDate }],
      ["sessions_usage_day_stats", { date: "2026-06-07" }],
    ]);
    expect(within(section).getByText("12M")).toBeInTheDocument();
  });

  it("ignores a slower refresh response after selecting another date", async () => {
    const user = userEvent.setup();
    renderWithProviders(<AiUsageStats />);
    await screen.findByText("2.2K");

    let resolveRefreshDay!: () => void;
    const refreshDayGate = new Promise<void>((resolve) => {
      resolveRefreshDay = resolve;
    });
    const refreshDate = (screen.getByLabelText(
      /Select Date|选择日期/,
    ) as HTMLInputElement).value;
    invokeMock.mockClear();
    invokeMock.mockImplementation(async (command: string, args?: InvokeArgs) => {
      if (command === "sessions_usage_clear_cache") return null;
      if (command === "sessions_usage_tool_stats") {
        return makeToolStats(args?.tool || "claude", args?.days || 7);
      }
      if (command === "sessions_usage_day_stats") {
        const date = args?.date || "";
        if (date === refreshDate) {
          await refreshDayGate;
          return makeEmptyDayStats(date);
        }
        return makeDayStats(date);
      }
      throw new Error(`Unhandled command: ${command}`);
    });

    await user.click(screen.getByRole("button", { name: /Refresh|刷新/ }));
    fireEvent.change(screen.getByLabelText(/Select Date|选择日期/), {
      target: { value: "2026-06-07" },
    });
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("sessions_usage_day_stats", {
        date: "2026-06-07",
      });
    });

    const section = screen.getByTestId("ai-usage-day-stats");
    expect(await within(section).findByText("12M")).toBeInTheDocument();
    await act(async () => {
      resolveRefreshDay();
      await Promise.resolve();
    });
    const dayCalls = invokeMock.mock.calls.filter(
      ([command]) => command === "sessions_usage_day_stats",
    );
    expect(dayCalls).toEqual([
      ["sessions_usage_day_stats", { date: refreshDate }],
      ["sessions_usage_day_stats", { date: "2026-06-07" }],
    ]);
    expect(within(section).getByText("12M")).toBeInTheDocument();
  });

  it("renders failed tool error without blocking other tools", async () => {
    invokeMock.mockImplementation(async (command: string, args?: InvokeArgs) => {
      if (command === "sessions_usage_tool_stats" && args?.tool === "antigravity") {
        throw new Error("antigravity unavailable");
      }
      if (command === "sessions_usage_tool_stats") {
        return makeToolStats(args?.tool || "claude", args?.days || 7);
      }
      throw new Error(`Unhandled command: ${command}`);
    });

    renderWithProviders(<AiUsageStats />);

    expect(
      await screen.findByText(/Antigravity (failed|加载失败).*antigravity unavailable/),
    ).toBeInTheDocument();
    expect(screen.getByText("12M")).toBeInTheDocument();
    expect(screen.getByText("2.2K")).toBeInTheDocument();
  });

  it("renders Antigravity usage as one unavailable state without numeric token values", async () => {
    // source_status:"unavailable" + scanned_sessions==0 → still show unavailable panel
    renderWithProviders(<AiUsageStats />);

    const panel = await screen.findByTestId("ai-usage-tool-antigravity");
    const unavailable = await within(panel).findByTestId(
      "ai-usage-unavailable-antigravity",
    );

    expect(unavailable).toHaveTextContent(/Unavailable|暂不可用/);
    expect(
      within(panel).getAllByText(/Unavailable|暂不可用/).length,
    ).toBeGreaterThanOrEqual(1);
    // Unavailable sources must not render the numeric token summary grid.
    expect(
      within(panel).queryByText(/Total Tokens|Token 总量/),
    ).not.toBeInTheDocument();
  });

  it("renders antigravity empty with scan counts and local-inaccessible message, no unavailable badge", async () => {
    // Override antigravity to be "empty" with scanned sessions but zero calls.
    let resolveAntigravity!: () => void;
    const antigravityGate = new Promise<void>((resolve) => {
      resolveAntigravity = resolve;
    });
    invokeMock.mockImplementation(async (command: string, args?: InvokeArgs) => {
      if (command === "sessions_usage_tool_stats" && args?.tool === "antigravity") {
        await antigravityGate;
        return makeToolStatsAntigravityEmpty();
      }
      if (command === "sessions_usage_tool_stats") {
        return makeToolStats(args?.tool || "claude", args?.days || 7);
      }
      if (command === "sessions_usage_day_stats") {
        return makeDayStats(args?.date || "");
      }
      if (command === "sessions_usage_clear_cache") {
        return null;
      }
      throw new Error(`Unhandled command: ${command}`);
    });

    renderWithProviders(<AiUsageStats />);

    // Wait for claude day-stats number so we know the component mounted.
    const daySection = await screen.findByTestId("ai-usage-day-stats");
    expect(await within(daySection).findByText("12M")).toBeInTheDocument();

    await act(async () => {
      resolveAntigravity();
    });

    const panel = await screen.findByTestId("ai-usage-tool-antigravity");

    // Should NOT render the unavailable badge element.
    expect(
      within(panel).queryByTestId("ai-usage-unavailable-antigravity"),
    ).not.toBeInTheDocument();

    // Should display scan session/call count.
    expect(
      within(panel).getByText(/2 sessions.*3 calls|2 个会话.*3 次调用/),
    ).toBeInTheDocument();

    // Should display the local-inaccessible message (English or Chinese).
    expect(
      within(panel).getByText(/Token usage locally unavailable|Token 用量暂不可用/),
    ).toBeInTheDocument();

    // Must not render a numeric Total Tokens cell.
    expect(
      within(panel).queryByText(/Total Tokens|Token 总量/),
    ).not.toBeInTheDocument();
  });

  it("renders antigravity with tokens same-shape numeric grid when calls > 0", async () => {
    let resolveAntigravity!: () => void;
    const antigravityGate = new Promise<void>((resolve) => {
      resolveAntigravity = resolve;
    });
    invokeMock.mockImplementation(async (command: string, args?: InvokeArgs) => {
      if (command === "sessions_usage_tool_stats" && args?.tool === "antigravity") {
        await antigravityGate;
        return {
          ...makeToolStatsAntigravityEmpty(),
          source_status: "available",
          summary: {
            total_tokens: 500000,
            calls: 2,
            sessions: 1,
            cache_hit_rate: 30,
            input_tokens: 350000,
            output_tokens: 100000,
            cache_tokens: 80000,
          },
          daily: makeToolStatsAntigravityEmpty().daily.map((d: AiUsageDailyItem, i: number) =>
            i === 0 ? { ...d, total_tokens: 500000, calls: 2, sessions: 1, cache_hit_rate: 30, input_tokens: 350000, output_tokens: 100000, cache_tokens: 80000 } : d,
          ),
          peak_day: { date: "2026-06-01", total_tokens: 500000, calls: 2 },
          scanned_sessions: 1,
          scanned_calls: 2,
        };
      }
      if (command === "sessions_usage_tool_stats") {
        return makeToolStats(args?.tool || "claude", args?.days || 7);
      }
      if (command === "sessions_usage_day_stats") {
        return makeDayStats(args?.date || "");
      }
      if (command === "sessions_usage_clear_cache") {
        return null;
      }
      throw new Error(`Unhandled command: ${command}`);
    });

    renderWithProviders(<AiUsageStats />);

    // Wait for day stats section to confirm component mounted.
    const daySection = await screen.findByTestId("ai-usage-day-stats");
    expect(await within(daySection).findByText("12M")).toBeInTheDocument();

    await act(async () => {
      resolveAntigravity();
    });

    const panel = await screen.findByTestId("ai-usage-tool-antigravity");

    // When calls > 0, should render the numeric grid (like claude).
    // Peak day cell proves the data path rendered (not empty/unavailable).
    expect(within(panel).getByText(/Peak Day|最高消耗日/)).toBeInTheDocument();
    expect(
      within(panel).queryByTestId("ai-usage-unavailable-antigravity"),
    ).not.toBeInTheDocument();
  });

  it("renders quota card with group names, rounded percentages, reset times and invokes command with no args", async () => {
    invokeMock.mockImplementation(async (command: string, args?: InvokeArgs) => {
      if (command === "sessions_usage_tool_stats") {
        return makeToolStats(args?.tool || "claude", args?.days || 7);
      }
      if (command === "sessions_usage_day_stats") {
        return makeDayStats(args?.date || "");
      }
      if (command === "sessions_usage_clear_cache") {
        return null;
      }
      if (command === "sessions_antigravity_quota") {
        if (args !== undefined && args !== null) {
          throw new Error("sessions_antigravity_quota should be called with no arguments");
        }
        return makeAntigravityQuotaResponse();
      }
      throw new Error(`Unhandled command: ${command}`);
    });

    renderWithProviders(<AiUsageStats />);

    const quotaPanel = await screen.findByTestId("ai-usage-quota-card");
    expect(quotaPanel).toBeInTheDocument();

    // Group names should appear.
    expect(
      within(quotaPanel).getByText(/Free tier/),
    ).toBeInTheDocument();
    expect(
      within(quotaPanel).getByText(/Pro tier/),
    ).toBeInTheDocument();

    // Buckets: remaining_fraction rounded to nearest integer percentage.
    // 0.245 → 25%, 0.875 → 88%
    expect(
      within(quotaPanel).getByText(/\b25\b%/),
    ).toBeInTheDocument();
    expect(
      within(quotaPanel).getByText(/\b88\b%/),
    ).toBeInTheDocument();

    // Reset times should be in plain text (original ISO-like strings).
    expect(
      within(quotaPanel).getByText(/2026-07-08T00:00:00Z/),
    ).toBeInTheDocument();
    expect(
      within(quotaPanel).getByText(/2026-08-01T00:00:00Z/),
    ).toBeInTheDocument();

    // The quota command must have been invoked with no arguments.
    expect(invokeMock).toHaveBeenCalledWith("sessions_antigravity_quota");
    const quotaCalls = invokeMock.mock.calls.filter(
      ([cmd]) => cmd === "sessions_antigravity_quota",
    );
    expect(quotaCalls.length).toBeGreaterThanOrEqual(1);
    expect(quotaCalls[0][1]).toBeUndefined();
  });

  it("shows quota error without blocking tool cards rendering", async () => {
    invokeMock.mockImplementation(async (command: string, args?: InvokeArgs) => {
      if (command === "sessions_usage_tool_stats") {
        return makeToolStats(args?.tool || "claude", args?.days || 7);
      }
      if (command === "sessions_usage_day_stats") {
        return makeDayStats(args?.date || "");
      }
      if (command === "sessions_usage_clear_cache") {
        return null;
      }
      if (command === "sessions_antigravity_quota") {
        throw new Error("quota service unavailable");
      }
      throw new Error(`Unhandled command: ${command}`);
    });

    renderWithProviders(<AiUsageStats />);

    // Tool cards must render normally — claude day-stats number confirms mount.
    const daySection = await screen.findByTestId("ai-usage-day-stats");
    expect(await within(daySection).findByText("12M")).toBeInTheDocument();
    expect(screen.getByText("2.2K")).toBeInTheDocument();

    // Quota panel shows an error message.
    const quotaPanel = await screen.findByTestId("ai-usage-quota-card");
    expect(quotaPanel).toBeInTheDocument();
    expect(
      within(quotaPanel).getByText(/quota service unavailable/),
    ).toBeInTheDocument();
  });

  it("renders empty state, trend, daily table, peak day, and scan stats", async () => {
    renderWithProviders(<AiUsageStats />);

    expect(await screen.findByText("2.2K")).toBeInTheDocument();
    expect(screen.getByText("2.2K")).toBeInTheDocument();
    expect(screen.getAllByText("25%").length).toBeGreaterThanOrEqual(1);
    expect(
      screen.getByText(/4 (sessions|个会话).*6 (calls|次调用)/),
    ).toBeInTheDocument();
    expect(screen.getAllByText(/Peak:|峰值：/).length).toBeGreaterThanOrEqual(
      1,
    );
    expect(
      screen.getAllByText(
        /No token usage records found in this window\.|当前时间窗口内未找到 Token 用量记录。/,
      ).length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      screen.getAllByText(/Unavailable|暂不可用/).length,
    ).toBeGreaterThanOrEqual(1);

    const claudePanel = screen.getByTestId("ai-usage-tool-claude");
    expect(
      within(claudePanel).getByText(/Daily Trend|每日趋势/),
    ).toBeInTheDocument();
    expect(
      within(claudePanel).getByRole("columnheader", { name: /Date|日期/ }),
    ).toBeInTheDocument();
    expect(
      within(claudePanel).getByRole("columnheader", { name: /Input|输入/ }),
    ).toBeInTheDocument();

    const rows = within(claudePanel).getAllByRole("row");
    expect(within(rows[1]).getByText(/Jun 7|6月7日/)).toBeInTheDocument();
    expect(within(rows[7]).getByText(/Jun 1|6月1日/)).toBeInTheDocument();
    expect(
      claudePanel.querySelector(
        '[title*="2026-06-07"][title*="1.2千万"][title*="8百万"][title*="3百万"][title*="1百万"]',
      ),
    ).toBeInTheDocument();
  });

  it("renders day stats section with date input and auto-loads today", async () => {
    renderWithProviders(<AiUsageStats />);
    const section = screen.getByTestId("ai-usage-day-stats");
    expect(section).toBeInTheDocument();
    expect(
      within(section).getByText(/Daily Stats|每日统计/),
    ).toBeInTheDocument();
    expect(
      within(section).getByLabelText(/Select Date|选择日期/),
    ).toBeInTheDocument();
    expect(
      await within(section).findByText("12M"),
    ).toBeInTheDocument();
  });

  it("can select today again after viewing an earlier date", async () => {
    const user = userEvent.setup();
    renderWithProviders(<AiUsageStats />);

    const dateInput = screen.getByLabelText(/Select Date|选择日期/);
    fireEvent.change(dateInput, { target: { value: "2026-06-07" } });
    const today = dateInput.getAttribute("max");
    expect(today).toBeTruthy();

    await user.click(screen.getByRole("button", { name: /Today|今天/ }));

    expect(dateInput).toHaveValue(today);
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("sessions_usage_day_stats", {
        date: today,
      });
    });
  });

  it("refreshes today's date after midnight", () => {
    vi.useFakeTimers();
    try {
      invokeMock.mockImplementation(() => new Promise(() => {}));
      vi.setSystemTime(new Date(2026, 6, 15, 23, 59));
      renderWithProviders(<AiUsageStats />);
      const dateInput = screen.getByLabelText(/Select Date|选择日期/);
      expect(dateInput).toHaveAttribute("max", "2026-07-15");

      vi.setSystemTime(new Date(2026, 6, 16, 0, 1));
      fireEvent.click(screen.getByRole("button", { name: /Today|今天/ }));

      expect(dateInput).toHaveAttribute("max", "2026-07-16");
      expect(dateInput).toHaveValue("2026-07-16");
    } finally {
      vi.useRealTimers();
    }
  });

  it("queries day stats on date selection and renders summary + breakdown", async () => {
    renderWithProviders(<AiUsageStats />);

    const dateInput = screen.getByLabelText(/Select Date|选择日期/);
    fireEvent.change(dateInput, { target: { value: "2026-06-07" } });

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("sessions_usage_day_stats", {
        date: "2026-06-07",
      });
    });

    const section = screen.getByTestId("ai-usage-day-stats");
    expect(await within(section).findByText("12M")).toBeInTheDocument();
    expect(await within(section).findByText("9")).toBeInTheDocument();
    expect(
      await within(section).findByText(/Per-Tool Breakdown|各工具明细/),
    ).toBeInTheDocument();
    expect(
      within(section).getByRole("columnheader", {
        name: /Cache Hit|缓存命中/,
      }),
    ).toBeInTheDocument();
    expect(await within(section).findByText("42%")).toBeInTheDocument();
  });

  it("renders day stats breakdown rows for tools with calls", async () => {
    renderWithProviders(<AiUsageStats />);

    const dateInput = screen.getByLabelText(/Select Date|选择日期/);
    fireEvent.change(dateInput, { target: { value: "2026-06-07" } });

    const section = screen.getByTestId("ai-usage-day-stats");
    const toolCells = await within(section).findAllByText(/Claude Code|Codex|Antigravity|OpenCode/i);
    expect(toolCells.length).toBeGreaterThanOrEqual(4);
    within(section).getByText("12,000,000");
    expect(within(section).getAllByText("2,222").length).toBeGreaterThanOrEqual(1);
    expect(within(section).getAllByText("500").length).toBeGreaterThanOrEqual(1);
    const antigravityRow = within(section).getByRole("row", { name: /Antigravity/i });
    expect(within(antigravityRow).getAllByText("-")).toHaveLength(6);
    expect(within(section).getByText("claude-opus-4-6")).toBeInTheDocument();
    expect(within(section).getByText("claude-sonnet-4-5")).toBeInTheDocument();
    expect(within(section).getByText("gpt-5-codex")).toBeInTheDocument();
    expect(within(section).getByText("deepseek-v4")).toBeInTheDocument();
    expect(within(section).queryByText(/gemini-3/i)).not.toBeInTheDocument();
    expect(within(section).getByText("75%")).toBeInTheDocument();
  });
});
