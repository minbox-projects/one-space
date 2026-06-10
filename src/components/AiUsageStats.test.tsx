import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { AiUsageStats } from "@/components/AiUsageStats";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

type ToolId = "claude" | "codex" | "gemini" | "opencode";

const tools: ToolId[] = ["claude", "codex", "gemini", "opencode"];

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

  return {
    tool,
    source_status: tool === "opencode" ? "error" : "empty",
    summary: emptySummary,
    daily: emptyDaily,
    peak_day: null,
    scanned_sessions: 0,
    scanned_calls: 0,
    errors: tool === "opencode" ? ["broken source"] : [],
  };
}

describe("AiUsageStats", () => {
  beforeEach(() => {
    resetTauriMocks();
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "sessions_usage_tool_stats") {
        return makeToolStats(args.tool, args.days || 7);
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
      screen.getByText(/Loading usage data\.\.\.|正在加载用量数据\.\.\./),
    ).toBeInTheDocument();
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

  it("renders failed tool error without blocking other tools", async () => {
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "sessions_usage_tool_stats" && args.tool === "gemini") {
        throw new Error("gemini unavailable");
      }
      if (command === "sessions_usage_tool_stats") {
        return makeToolStats(args.tool, args.days || 7);
      }
      throw new Error(`Unhandled command: ${command}`);
    });

    renderWithProviders(<AiUsageStats />);

    expect(
      await screen.findByText(/Gemini (failed|加载失败).*gemini unavailable/),
    ).toBeInTheDocument();
    expect(screen.getByText("12M")).toBeInTheDocument();
    expect(screen.getByText("2.2K")).toBeInTheDocument();
  });

  it("renders empty state, trend, daily table, peak day, and scan stats", async () => {
    renderWithProviders(<AiUsageStats />);

    expect(await screen.findByText("12M")).toBeInTheDocument();
    expect(screen.getByText("2.2K")).toBeInTheDocument();
    expect(screen.getByText("25%")).toBeInTheDocument();
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
    ).toBeGreaterThanOrEqual(2);

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
});
