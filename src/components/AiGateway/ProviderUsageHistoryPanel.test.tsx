// Step 2 (20261014-ai-gateway-provider-usage-history): the Provider history
// vertical inside Usage & Logs, driven through the public AI Gateway console.
//
// The dedicated `ProviderUsageHistoryPanel` product module does not exist yet,
// so this file drives the existing console entry (`AiGateway`) exactly like the
// `ModelPoolCard.test.tsx` precedent: no nonexistent product symbol is imported.
//
// Public contract this file fixes for the implementation:
//   - third usage subtab:   data-testid="ai-gateway-subtab-usage-history"
//   - panel root:           data-testid="ai-gateway-provider-usage-history"
//   - provider selector:    SelectDropdown testId="ai-gateway-provider-history-provider"
//                           (trigger "-trigger", options role="option")
//   - coverage summary:     data-testid="ai-gateway-provider-history-coverage"
//   - historical note:      data-testid="ai-gateway-provider-history-historical-note"
//   - cumulative metrics:   data-testid="ai-gateway-provider-history-cumulative"
//   - trend chart:          data-testid="ai-gateway-provider-history-chart"
//   - daily table:          data-testid="ai-gateway-provider-history-days"
//   - day row:              data-testid="ai-gateway-provider-history-day-<YYYY-MM-DD>"
//                           with data-coverage retained_historical|continuous|unavailable
//                           and data-ongoing="true|false"
//   - day row action:       data-testid="ai-gateway-provider-history-open-day-<YYYY-MM-DD>"
//   - empty / error:        data-testid="ai-gateway-provider-history-empty" / "-error"
//   - retry / refresh:      data-testid="ai-gateway-provider-history-retry" / "-refresh"
//   - trend metric:         SelectDropdown testId="ai-gateway-provider-history-metric"
//                           (trigger "-trigger", options role="option": Tokens / Requests / Cost)
//                           The operator selects the trend metric; each day's trend entry
//                           exposes its day plus the selected metric's value as an
//                           accessible label, an unavailable day is labeled unavailable
//                           (never a numeric 0), and a covered no-request day is a real 0.
//
// Panel prop contract reported to Frontend (compatible with the existing
// isActive convention):
//   ProviderUsageHistoryPanel({ isActive?: boolean;
//     onOpenDay?: (request: { providerId: string; day: string }) => void })

import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import i18n from "@/i18n";
import { AiGateway } from "@/components/AiGateway";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

type HistoryMetrics = {
  request_count: number;
  input_tokens: number;
  cache_read_tokens: number;
  cache_write_tokens: number;
  output_tokens: number;
  total_tokens: number;
  amount: number;
  unpriced_count: number;
  cache_hit_tokens: number;
  cache_eligible_tokens: number;
  cache_hit_rate_percent: number | null;
  cache_rate_eligible_count: number;
  successful_request_count: number;
};

type DayCoverage = "retained_historical" | "continuous" | "unavailable";

type DailyEntry = {
  day: string;
  coverage: DayCoverage;
  ongoing: boolean;
  metrics: HistoryMetrics | null;
};

type Identity = {
  provider_id: string;
  provider_name: string;
  deleted: boolean;
  has_usage: boolean;
};

type History = {
  provider_id: string;
  earliest_available_day: string | null;
  continuity_start_day: string | null;
  end_day: string | null;
  cumulative: HistoryMetrics | null;
  days: DailyEntry[];
};

function metrics(overrides: Partial<HistoryMetrics> = {}): HistoryMetrics {
  return {
    request_count: 0,
    input_tokens: 0,
    cache_read_tokens: 0,
    cache_write_tokens: 0,
    output_tokens: 0,
    total_tokens: 0,
    amount: 0,
    unpriced_count: 0,
    cache_hit_tokens: 0,
    cache_eligible_tokens: 0,
    cache_hit_rate_percent: null,
    cache_rate_eligible_count: 0,
    successful_request_count: 0,
    ...overrides,
  };
}

function identity(
  overrides: Partial<Identity> & Pick<Identity, "provider_id" | "provider_name">,
): Identity {
  return { deleted: false, has_usage: true, ...overrides };
}

function history(
  overrides: Partial<History> & Pick<History, "provider_id">,
): History {
  return {
    earliest_available_day: null,
    continuity_start_day: null,
    end_day: null,
    cumulative: null,
    days: [],
    ...overrides,
  };
}

const state = {
  identities: [] as Identity[],
  histories: {} as Record<string, History>,
  deferred: {} as Record<string, Promise<History>>,
  providersError: null as string | null,
  historyError: null as string | null,
};

function makeConfig() {
  return {
    enabled: true,
    port: 17688,
    providers: state.identities
      .filter((entry) => !entry.deleted)
      .map((entry) => ({
        id: entry.provider_id,
        name: entry.provider_name,
        base_url: "https://api.example",
        default_model: null,
        protocol: "chat_completions",
        mappings: [],
        enabled: true,
      })),
    keys: [],
    default_key_id: null,
    terminal_syncs: [],
  };
}

function makeStatus() {
  return {
    running: true,
    enabled: true,
    port: 17688,
    local_base_url: "http://127.0.0.1:17688/v1",
    provider_count: state.identities.length,
    auto_disabled_count: 0,
    key_count: 0,
    default_key_id: null,
  };
}

function emptyStats() {
  return { ...metrics(), granularity: "hour", buckets: [], models: [] };
}

function logsPage(args?: Record<string, unknown>) {
  return {
    page: (args?.page as number | undefined) ?? 1,
    page_size: 50,
    total: 0,
    total_pages: 1,
    group_by: (args?.groupBy as string | null | undefined) ?? null,
    records: [],
    groups: [],
    models: [],
    providers: [],
  };
}

function installMock() {
  invokeMock.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    switch (command) {
      case "ai_gateway_get_config":
        return makeConfig();
      case "ai_gateway_status":
        return makeStatus();
      case "ai_gateway_terminal_targets":
        return [];
      case "ai_gateway_model_pools_status":
        return [];
      case "ai_gateway_provider_templates":
        return [];
      case "ai_gateway_template_auto_refresh_status":
        return { failures: [] };
      case "ai_gateway_usage_stats":
        return emptyStats();
      case "ai_gateway_request_logs":
        return logsPage(args);
      case "ai_gateway_usage_providers":
        if (state.providersError) throw new Error(state.providersError);
        return state.identities;
      case "ai_gateway_provider_usage_history": {
        if (state.historyError) throw new Error(state.historyError);
        const providerId = args?.providerId as string;
        return state.deferred[providerId] ?? state.histories[providerId];
      }
      default:
        return undefined;
    }
  });
}

function historyQueryCount() {
  return invokeMock.mock.calls.filter(
    ([command]) => command === "ai_gateway_provider_usage_history",
  ).length;
}

function providersQueryCount() {
  return invokeMock.mock.calls.filter(
    ([command]) => command === "ai_gateway_usage_providers",
  ).length;
}

async function openProviderHistory() {
  renderWithProviders(<AiGateway />);
  await screen.findByTestId("ai-gateway-providers");
  const tabsList = screen.getByRole("tablist", {
    name: i18n.t("aiGatewayWorkspaceTabs", "AI Gateway tabs"),
  });
  fireEvent.click(
    within(tabsList).getByRole("tab", {
      name: i18n.t("aiGatewayUsageAndLogsTab", "Usage & Logs"),
    }),
  );
  fireEvent.click(screen.getByTestId("ai-gateway-subtab-usage-history"));
  return within(await screen.findByTestId("ai-gateway-provider-usage-history"));
}

async function chooseProvider(
  panel: ReturnType<typeof within>,
  name: RegExp | string,
) {
  const user = userEvent.setup();
  await user.click(
    panel.getByTestId("ai-gateway-provider-history-provider-trigger"),
  );
  await user.click(panel.getByRole("option", { name }));
}

async function clearProvider(panel: ReturnType<typeof within>) {
  const user = userEvent.setup();
  await user.click(
    panel.getByTestId("ai-gateway-provider-history-provider-trigger"),
  );
  await user.click(
    panel.getByRole("option", {
      name: i18n.t("aiGatewayProviderHistorySelectProvider", "Select a provider"),
    }),
  );
}

async function chooseMetric(
  panel: ReturnType<typeof within>,
  name: RegExp,
) {
  const user = userEvent.setup();
  await user.click(
    panel.getByTestId("ai-gateway-provider-history-metric-trigger"),
  );
  await user.click(panel.getByRole("option", { name }));
}

/**
 * Concatenate the chart's visible text and every accessible label it exposes.
 * Used so the trend assertions stay at the accessible-value boundary instead of
 * pinning a particular bar element or CSS class.
 */
function chartAccessibleText(chart: HTMLElement): string {
  const parts = [chart.textContent ?? ""];
  chart.querySelectorAll("[aria-label]").forEach((el) => {
    parts.push(el.getAttribute("aria-label") ?? "");
  });
  chart.querySelectorAll("[title]").forEach((el) => {
    parts.push(el.getAttribute("title") ?? "");
  });
  return parts.join(" ");
}

/**
 * A complete UTC+8 calendar from 2024-01-01 through 2025-01-05 inclusive:
 * 366 leap-year days (2024) plus 5 days (2025) = 371 observed days. The count is
 * written as a literal so the test does not re-derive it with the same loop.
 */
function buildLongHistory(): History {
  const days: DailyEntry[] = [];
  const cursor = new Date(Date.UTC(2024, 0, 1));
  const end = Date.UTC(2025, 0, 5);
  while (cursor.getTime() <= end) {
    const day = cursor.toISOString().slice(0, 10);
    days.push({
      day,
      coverage: "continuous",
      ongoing: day === "2025-01-05",
      metrics: metrics({ request_count: 1 }),
    });
    cursor.setUTCDate(cursor.getUTCDate() + 1);
  }
  return history({
    provider_id: "p-long",
    earliest_available_day: "2024-01-01",
    continuity_start_day: "2024-01-01",
    end_day: "2025-01-05",
    cumulative: metrics({ request_count: days.length }),
    days,
  });
}

/**
 * A four-day UTC+8 history crossing a year boundary with distinct, independent
 * values per metric so request, token and cost trends cannot be confused:
 * 2024-12-30 retained (18 req / 444 tok / 0.25), 2024-12-31 unavailable,
 * 2025-01-01 covered zero, 2025-01-02 ongoing (23 req / 333 tok / 0.5).
 */
function crossYearHistory(): History {
  return history({
    provider_id: "p-a",
    earliest_available_day: "2024-12-30",
    continuity_start_day: "2025-01-01",
    end_day: "2025-01-02",
    cumulative: metrics({ request_count: 41 }),
    days: [
      {
        day: "2024-12-30",
        coverage: "retained_historical",
        ongoing: false,
        metrics: metrics({
          request_count: 18,
          successful_request_count: 18,
          total_tokens: 444,
          amount: 0.25,
        }),
      },
      {
        day: "2024-12-31",
        coverage: "unavailable",
        ongoing: false,
        metrics: null,
      },
      {
        day: "2025-01-01",
        coverage: "continuous",
        ongoing: false,
        metrics: metrics({
          request_count: 0,
          successful_request_count: 0,
          total_tokens: 0,
          amount: 0,
        }),
      },
      {
        day: "2025-01-02",
        coverage: "continuous",
        ongoing: true,
        metrics: metrics({
          request_count: 23,
          successful_request_count: 23,
          total_tokens: 333,
          amount: 0.5,
        }),
      },
    ],
  });
}

describe("ProviderUsageHistoryPanel", () => {
  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
    state.identities = [];
    state.histories = {};
    state.deferred = {};
    state.providersError = null;
    state.historyError = null;
    installMock();
  });

  it("选择服务商展示覆盖与累计指标、打开精确某日日志并可返回同一选择", async () => {
    state.identities = [
      identity({ provider_id: "p-a", provider_name: "Acme" }),
      identity({ provider_id: "p-b", provider_name: "Beta" }),
    ];
    state.histories = {
      "p-a": history({
        provider_id: "p-a",
        earliest_available_day: "2026-09-15",
        continuity_start_day: "2026-09-17",
        end_day: "2026-09-17",
        cumulative: metrics({
          request_count: 12,
          successful_request_count: 9,
          amount: 1.5,
          unpriced_count: 2,
          cache_hit_tokens: 1,
          cache_eligible_tokens: 4,
          cache_hit_rate_percent: 25,
          cache_rate_eligible_count: 2,
        }),
        days: [
          {
            day: "2026-09-15",
            coverage: "retained_historical",
            ongoing: false,
            metrics: metrics({
              request_count: 2,
              successful_request_count: 1,
              cache_hit_tokens: 1,
              cache_eligible_tokens: 1,
              cache_hit_rate_percent: 100,
              cache_rate_eligible_count: 1,
            }),
          },
          {
            day: "2026-09-16",
            coverage: "unavailable",
            ongoing: false,
            metrics: null,
          },
          {
            day: "2026-09-17",
            coverage: "continuous",
            ongoing: true,
            metrics: metrics({
              request_count: 10,
              successful_request_count: 8,
              cache_hit_tokens: 0,
              cache_eligible_tokens: 3,
              cache_hit_rate_percent: 0,
              cache_rate_eligible_count: 1,
            }),
          },
        ],
      }),
      "p-b": history({
        provider_id: "p-b",
        earliest_available_day: "2026-09-16",
        continuity_start_day: "2026-09-16",
        end_day: "2026-09-17",
        cumulative: metrics({ request_count: 42 }),
        days: [],
      }),
    };

    const panel = await openProviderHistory();
    await chooseProvider(panel, /Acme/);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "ai_gateway_provider_usage_history",
        { providerId: "p-a" },
      ),
    );

    // Coverage disclosure: earliest available day through the current end day.
    const coverage = panel.getByTestId("ai-gateway-provider-history-coverage");
    expect(coverage).toHaveTextContent("2026-09-15");
    expect(coverage).toHaveTextContent("2026-09-17");

    // Cumulative additive metrics; the backend cache rate is rendered as
    // delivered (25%), never re-averaged from the 100% / 0% days (which would
    // be 50%), and failure is derived from request minus success. Rate coverage
    // is eligible *requests* over successful requests (2 / 9), never the token
    // denominator (which would wrongly read 4 / 9).
    const cumulative = panel.getByTestId("ai-gateway-provider-history-cumulative");
    expect(cumulative).toHaveTextContent("12");
    expect(cumulative).toHaveTextContent("9");
    expect(cumulative).toHaveTextContent("3");
    expect(cumulative).toHaveTextContent("1.5");
    expect(cumulative).toHaveTextContent("2");
    expect(cumulative).toHaveTextContent("25");
    expect(cumulative).toHaveTextContent(/2\s*\/\s*9/);
    expect(cumulative).not.toHaveTextContent("50");
    expect(cumulative).not.toHaveTextContent("100");

    // Open the exact day in the existing request-log view.
    const user = userEvent.setup();
    await user.click(
      panel.getByTestId("ai-gateway-provider-history-open-day-2026-09-17"),
    );
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_request_logs", {
        range: "today",
        groupBy: "none",
        status: null,
        model: null,
        provider: null,
        page: 1,
        providerId: "p-a",
        day: "2026-09-17",
      }),
    );

    // Returning restores the same provider selection, and activation must
    // obtain the CURRENT history rather than the snapshot captured before the
    // log view opened.
    const queriesBeforeReturn = historyQueryCount();
    state.histories["p-a"] = history({
      provider_id: "p-a",
      earliest_available_day: "2026-09-15",
      continuity_start_day: "2026-09-17",
      end_day: "2026-09-18",
      cumulative: metrics({ request_count: 13, successful_request_count: 10 }),
      days: [],
    });

    await user.click(screen.getByTestId("ai-gateway-logs-return-history"));
    expect(
      screen.getByTestId("ai-gateway-subtab-usage-history"),
    ).toHaveAttribute("aria-selected", "true");
    expect(
      panel.getByTestId("ai-gateway-provider-history-provider-trigger"),
    ).toHaveTextContent("Acme");
    await waitFor(() => expect(historyQueryCount()).toBeGreaterThan(queriesBeforeReturn));
    await waitFor(() =>
      expect(
        panel.getByTestId("ai-gateway-provider-history-coverage"),
      ).toHaveTextContent("2026-09-18"),
    );
  });

  it("每日明细暴露合格/成功请求覆盖率，而不是 token 分母", async () => {
    state.identities = [identity({ provider_id: "p-a", provider_name: "Acme" })];
    state.histories = {
      "p-a": history({
        provider_id: "p-a",
        earliest_available_day: "2026-09-17",
        continuity_start_day: "2026-09-17",
        end_day: "2026-09-17",
        cumulative: metrics({
          request_count: 10,
          successful_request_count: 8,
          cache_rate_eligible_count: 1,
        }),
        days: [
          {
            day: "2026-09-17",
            coverage: "continuous",
            ongoing: true,
            metrics: metrics({
              request_count: 10,
              successful_request_count: 8,
              cache_hit_tokens: 0,
              cache_eligible_tokens: 3,
              cache_hit_rate_percent: 0,
              cache_rate_eligible_count: 1,
            }),
          },
        ],
      }),
    };

    const panel = await openProviderHistory();
    await chooseProvider(panel, /Acme/);
    await waitFor(() =>
      expect(
        panel.getByTestId("ai-gateway-provider-history-days"),
      ).toBeInTheDocument(),
    );

    // 1 eligible request of 8 successful; the token denominator is 3, so a
    // token-based row would wrongly read 3 / 8.
    const row = panel.getByTestId("ai-gateway-provider-history-day-2026-09-17");
    expect(row).toHaveTextContent(/1\s*\/\s*8/);
    expect(row).not.toHaveTextContent(/3\s*\/\s*8/);
  });

  it("未激活时 Provider history 不取数，激活后才查询", async () => {
    state.identities = [identity({ provider_id: "p-a", provider_name: "Acme" })];
    state.histories = {
      "p-a": history({
        provider_id: "p-a",
        earliest_available_day: "2026-09-17",
        continuity_start_day: "2026-09-17",
        end_day: "2026-09-17",
        cumulative: metrics({ request_count: 1 }),
        days: [],
      }),
    };

    renderWithProviders(<AiGateway />);
    await screen.findByTestId("ai-gateway-providers");
    expect(providersQueryCount()).toBe(0);
    expect(historyQueryCount()).toBe(0);

    const tabsList = screen.getByRole("tablist", { name: /AI Gateway tabs/i });
    fireEvent.click(within(tabsList).getByRole("tab", { name: /Usage & Logs/i }));
    await screen.findByTestId("ai-gateway-usage-stats");
    expect(providersQueryCount()).toBe(0);

    fireEvent.click(screen.getByTestId("ai-gateway-subtab-usage-history"));
    const panel = within(
      await screen.findByTestId("ai-gateway-provider-usage-history"),
    );
    await waitFor(() => expect(providersQueryCount()).toBeGreaterThan(0));

    await chooseProvider(panel, /Acme/);
    await waitFor(() => expect(historyQueryCount()).toBeGreaterThan(0));
  });

  it("同名服务商可区分、已删除项仍可读可选、未使用项为空且不编造首次使用", async () => {
    state.identities = [
      identity({ provider_id: "p-1", provider_name: "Acme" }),
      identity({ provider_id: "p-2", provider_name: "Acme" }),
      identity({ provider_id: "gone", provider_name: "Deleted Co", deleted: true }),
      identity({
        provider_id: "unused",
        provider_name: "Unused Co",
        has_usage: false,
      }),
    ];
    state.histories = {
      gone: history({
        provider_id: "gone",
        earliest_available_day: "2026-01-02",
        continuity_start_day: "2026-01-02",
        end_day: "2026-09-17",
        cumulative: metrics({ request_count: 4 }),
        days: [],
      }),
      unused: history({ provider_id: "unused" }),
    };

    const panel = await openProviderHistory();
    const user = userEvent.setup();
    await user.click(
      panel.getByTestId("ai-gateway-provider-history-provider-trigger"),
    );
    const options = panel.getAllByRole("option");
    const labels = options.map((option) => option.textContent?.trim() ?? "");
    expect(labels.filter((label) => label.includes("Acme")).length).toBe(2);
    // Every option carries a distinct, non-empty label: same-named entries are
    // disambiguated and the deleted identity stays readable.
    expect(new Set(labels).size).toBe(labels.length);
    expect(labels.every((label) => label !== "")).toBe(true);

    // Unused configured provider: empty state, no invented first-use date.
    await user.click(panel.getByRole("option", { name: /Unused Co/ }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "ai_gateway_provider_usage_history",
        { providerId: "unused" },
      ),
    );
    expect(
      panel.getByTestId("ai-gateway-provider-history-empty"),
    ).toBeInTheDocument();
    expect(
      panel.queryByTestId("ai-gateway-provider-history-coverage"),
    ).not.toBeInTheDocument();

    // Deleted identity with historical usage remains selectable and readable.
    await user.click(
      panel.getByTestId("ai-gateway-provider-history-provider-trigger"),
    );
    await user.click(panel.getByRole("option", { name: /Deleted Co/ }));
    await waitFor(() =>
      expect(
        panel.getByTestId("ai-gateway-provider-history-coverage"),
      ).toHaveTextContent("2026-01-02"),
    );
  });

  it("跨年完整日期与覆盖分类：连续日内零值为 0、缺失历史日不可用、今日进行中", async () => {
    state.identities = [identity({ provider_id: "p-a", provider_name: "Acme" })];
    state.histories = { "p-a": crossYearHistory() };

    const panel = await openProviderHistory();
    await chooseProvider(panel, /Acme/);
    await waitFor(() =>
      expect(
        panel.getByTestId("ai-gateway-provider-history-days"),
      ).toBeInTheDocument(),
    );

    const coverage = panel.getByTestId("ai-gateway-provider-history-coverage");
    expect(coverage).toHaveTextContent("2024-12-30");
    expect(coverage).toHaveTextContent("2025-01-01");
    expect(coverage).toHaveTextContent("2025-01-02");
    expect(
      panel.getByTestId("ai-gateway-provider-history-historical-note"),
    ).toBeInTheDocument();

    // Chart and table retain full multi-year dates without dropping older days.
    expect(
      panel.getByTestId("ai-gateway-provider-history-chart"),
    ).toBeInTheDocument();
    for (const day of [
      "2024-12-30",
      "2024-12-31",
      "2025-01-01",
      "2025-01-02",
    ]) {
      expect(
        panel.getByTestId(`ai-gateway-provider-history-day-${day}`),
      ).toBeInTheDocument();
    }

    // Missing historical day is unavailable (placeholder), never a numeric zero.
    const unavailable = panel.getByTestId(
      "ai-gateway-provider-history-day-2024-12-31",
    );
    expect(unavailable).toHaveAttribute("data-coverage", "unavailable");
    expect(unavailable.textContent).toContain("—");

    // Covered no-request day is a genuine zero.
    const zeroDay = panel.getByTestId(
      "ai-gateway-provider-history-day-2025-01-01",
    );
    expect(zeroDay).toHaveAttribute("data-coverage", "continuous");
    expect(zeroDay.textContent).not.toContain("—");

    // Today's bucket is marked ongoing.
    expect(
      panel.getByTestId("ai-gateway-provider-history-day-2025-01-02"),
    ).toHaveAttribute("data-ongoing", "true");

    // The trend must distinguish an unavailable day from a numeric zero.
    const chart = panel.getByTestId("ai-gateway-provider-history-chart");
    const chartText = () => chartAccessibleText(chart);
    expect(chartText()).toContain(
      i18n.t("aiGatewayProviderHistoryCoverageUnavailable", "Unavailable"),
    );
    expect(chartText()).not.toContain("2024-12-31: 0");
    // A covered no-request day is a genuine zero in the trend too.
    expect(chartText()).toContain("2025-01-01: 0");
  });

  it("趋势图支持 Token/请求/花费三种指标", async () => {
    state.identities = [identity({ provider_id: "p-a", provider_name: "Acme" })];
    state.histories = { "p-a": crossYearHistory() };

    const panel = await openProviderHistory();
    await chooseProvider(panel, /Acme/);
    await waitFor(() =>
      expect(
        panel.getByTestId("ai-gateway-provider-history-days"),
      ).toBeInTheDocument(),
    );

    // All three additive metrics are inspectable as trends: the operator picks
    // the metric and the trend exposes that metric's distinct daily values
    // (tokens 444 / 333, requests 18 / 23, cost 0.25 / 0.5).
    const chart = panel.getByTestId("ai-gateway-provider-history-chart");
    const chartText = () => chartAccessibleText(chart);
    expect(chartText()).toContain("444");
    expect(chartText()).toContain("333");
    await chooseMetric(panel, /Requests?/i);
    expect(chartText()).toContain("18");
    expect(chartText()).toContain("23");
    await chooseMetric(panel, /Cost/i);
    expect(chartText()).toContain("0.25");
    expect(chartText()).toContain("0.5");
  });

  it("长历史保留每一天的完整日期（图表与表格均不截断）", async () => {
    const longHistory = buildLongHistory();
    expect(longHistory.days.length).toBe(371);
    state.identities = [
      identity({ provider_id: "p-long", provider_name: "Longtail" }),
    ];
    state.histories = { "p-long": longHistory };

    const panel = await openProviderHistory();
    await chooseProvider(panel, /Longtail/);
    await waitFor(() =>
      expect(
        panel.getByTestId("ai-gateway-provider-history-days"),
      ).toBeInTheDocument(),
    );

    // Every observed day is present exactly once; the count is a literal so it
    // is not re-derived with the fixture generator's own loop.
    const dayRows = panel.getAllByTestId(
      /^ai-gateway-provider-history-day-\d{4}-\d{2}-\d{2}$/,
    );
    expect(dayRows).toHaveLength(371);

    const chart = panel.getByTestId("ai-gateway-provider-history-chart");
    for (const day of ["2024-01-01", "2024-07-01", "2025-01-05"]) {
      expect(
        panel.getByTestId(`ai-gateway-provider-history-day-${day}`),
      ).toBeInTheDocument();
      expect(within(chart).getByText(day)).toBeInTheDocument();
    }
  });

  it("延迟的旧服务商响应最后返回也不得替换或错标当前选择", async () => {
    let resolveAlpha: (value: History) => void = () => {};
    state.deferred = {
      "p-a": new Promise<History>((resolve) => {
        resolveAlpha = resolve;
      }),
    };
    state.identities = [
      identity({ provider_id: "p-a", provider_name: "Alpha" }),
      identity({ provider_id: "p-b", provider_name: "Beta" }),
    ];
    state.histories = {
      "p-b": history({
        provider_id: "p-b",
        earliest_available_day: "2026-09-16",
        continuity_start_day: "2026-09-16",
        end_day: "2026-09-17",
        cumulative: metrics({ request_count: 42 }),
        days: [],
      }),
    };

    const panel = await openProviderHistory();
    await chooseProvider(panel, /Alpha/);
    await waitFor(() => expect(historyQueryCount()).toBeGreaterThan(0));

    await chooseProvider(panel, /Beta/);
    await waitFor(() =>
      expect(
        panel.getByTestId("ai-gateway-provider-history-cumulative"),
      ).toHaveTextContent("42"),
    );

    // The older Alpha response resolves last.
    await act(async () => {
      resolveAlpha(
        history({
          provider_id: "p-a",
          earliest_available_day: "2026-09-01",
          continuity_start_day: "2026-09-01",
          end_day: "2026-09-17",
          cumulative: metrics({ request_count: 999 }),
          days: [],
        }),
      );
    });

    const cumulative = panel.getByTestId(
      "ai-gateway-provider-history-cumulative",
    );
    expect(cumulative).toHaveTextContent("42");
    expect(cumulative).not.toHaveTextContent("999");
  });

  it("查询失败与空历史可区分，重试与手动刷新拉取当前数据", async () => {
    state.identities = [
      identity({ provider_id: "p-a", provider_name: "Alpha" }),
    ];
    state.historyError = "network down";

    const panel = await openProviderHistory();
    await chooseProvider(panel, /Alpha/);
    await waitFor(() =>
      expect(
        panel.getByTestId("ai-gateway-provider-history-error"),
      ).toBeInTheDocument(),
    );
    expect(
      panel.queryByTestId("ai-gateway-provider-history-empty"),
    ).not.toBeInTheDocument();

    // Retry after the failure clears fetches current data.
    state.historyError = null;
    state.histories = {
      "p-a": history({
        provider_id: "p-a",
        earliest_available_day: "2026-09-17",
        continuity_start_day: "2026-09-17",
        end_day: "2026-09-17",
        cumulative: metrics({ request_count: 7 }),
        days: [],
      }),
    };
    const user = userEvent.setup();
    await user.click(panel.getByTestId("ai-gateway-provider-history-retry"));
    await waitFor(() =>
      expect(
        panel.getByTestId("ai-gateway-provider-history-coverage"),
      ).toHaveTextContent("2026-09-17"),
    );

    // Manual refresh re-reads the current provider.
    const before = historyQueryCount();
    await user.click(panel.getByTestId("ai-gateway-provider-history-refresh"));
    await waitFor(() => expect(historyQueryCount()).toBeGreaterThan(before));
  });

  it("中英双语下均提供 Provider history 控件与日期明细入口", async () => {
    state.identities = [identity({ provider_id: "p-a", provider_name: "Acme" })];
    state.histories = {
      "p-a": history({
        provider_id: "p-a",
        earliest_available_day: "2026-09-17",
        continuity_start_day: "2026-09-17",
        end_day: "2026-09-17",
        cumulative: metrics({ request_count: 1 }),
        days: [
          {
            day: "2026-09-17",
            coverage: "continuous",
            ongoing: true,
            metrics: metrics({ request_count: 1 }),
          },
        ],
      }),
    };
    await i18n.changeLanguage("zh");

    const panel = await openProviderHistory();
    expect(
      panel.getByTestId("ai-gateway-provider-history-provider-trigger"),
    ).toBeInTheDocument();
    await chooseProvider(panel, /Acme/);
    await waitFor(() =>
      expect(
        panel.getByTestId("ai-gateway-provider-history-open-day-2026-09-17"),
      ).toBeInTheDocument(),
    );
    expect(
      panel.getByTestId("ai-gateway-provider-history-days"),
    ).toBeInTheDocument();
  });

  it("清除选择后迟到的服务商响应不得提交，且不触发隐藏查询", async () => {
    let resolveAlpha: (value: History) => void = () => {};
    state.deferred = {
      "p-a": new Promise<History>((resolve) => {
        resolveAlpha = resolve;
      }),
    };
    state.identities = [
      identity({ provider_id: "p-a", provider_name: "Alpha" }),
      identity({ provider_id: "p-b", provider_name: "Beta" }),
    ];
    state.histories = {
      "p-b": history({
        provider_id: "p-b",
        earliest_available_day: "2026-09-16",
        continuity_start_day: "2026-09-16",
        end_day: "2026-09-17",
        cumulative: metrics({ request_count: 42 }),
        days: [],
      }),
    };

    const panel = await openProviderHistory();
    await chooseProvider(panel, /Alpha/);
    await waitFor(() => expect(historyQueryCount()).toBeGreaterThan(0));
    const queriesAfterSelect = historyQueryCount();

    // Clearing the selection shows the prompt and issues no hidden query.
    await clearProvider(panel);
    expect(
      panel.queryByTestId("ai-gateway-provider-history-cumulative"),
    ).not.toBeInTheDocument();
    expect(historyQueryCount()).toBe(queriesAfterSelect);

    // The cleared provider's response resolves late; it cannot be committed or
    // shown as if it belonged to the empty selection.
    await act(async () => {
      resolveAlpha(
        history({
          provider_id: "p-a",
          earliest_available_day: "2026-09-01",
          continuity_start_day: "2026-09-01",
          end_day: "2026-09-17",
          cumulative: metrics({ request_count: 999 }),
          days: [],
        }),
      );
    });
    expect(
      panel.queryByTestId("ai-gateway-provider-history-cumulative"),
    ).not.toBeInTheDocument();
    expect(
      panel.queryByTestId("ai-gateway-provider-history-coverage"),
    ).not.toBeInTheDocument();

    // Selecting another provider shows only that provider's current data.
    await chooseProvider(panel, /Beta/);
    await waitFor(() =>
      expect(
        panel.getByTestId("ai-gateway-provider-history-cumulative"),
      ).toHaveTextContent("42"),
    );
    expect(
      panel.getByTestId("ai-gateway-provider-history-cumulative"),
    ).not.toHaveTextContent("999");
  });

  it("未激活视图不取数；迟到响应不得出现，重新激活获得当前数据", async () => {
    let resolveAlpha: (value: History) => void = () => {};
    state.deferred = {
      "p-a": new Promise<History>((resolve) => {
        resolveAlpha = resolve;
      }),
    };
    state.identities = [
      identity({ provider_id: "p-a", provider_name: "Alpha" }),
    ];

    const panel = await openProviderHistory();
    await chooseProvider(panel, /Alpha/);
    await waitFor(() => expect(historyQueryCount()).toBeGreaterThan(0));

    // Inactivate the mounted view: no query may fire while hidden.
    fireEvent.click(screen.getByTestId("ai-gateway-subtab-usage-stats"));
    const whileInactive = historyQueryCount();
    await act(async () => {
      resolveAlpha(
        history({
          provider_id: "p-a",
          earliest_available_day: "2026-09-01",
          continuity_start_day: "2026-09-01",
          end_day: "2026-09-17",
          cumulative: metrics({ request_count: 111 }),
          days: [],
        }),
      );
    });
    expect(historyQueryCount()).toBe(whileInactive);

    // The backend's current value differs; reactivation obtains it fresh and
    // never surfaces the late inactive response.
    delete state.deferred["p-a"];
    state.histories["p-a"] = history({
      provider_id: "p-a",
      earliest_available_day: "2026-09-15",
      continuity_start_day: "2026-09-17",
      end_day: "2026-09-18",
      cumulative: metrics({ request_count: 6 }),
      days: [],
    });
    fireEvent.click(screen.getByTestId("ai-gateway-subtab-usage-history"));
    await waitFor(() =>
      expect(
        panel.getByTestId("ai-gateway-provider-history-cumulative"),
      ).toHaveTextContent("6"),
    );
    expect(
      panel.getByTestId("ai-gateway-provider-history-cumulative"),
    ).not.toHaveTextContent("111");
  });

  it("手动刷新同时获得当前服务商身份列表", async () => {
    state.identities = [
      identity({ provider_id: "p-a", provider_name: "Alpha" }),
    ];
    state.histories = {
      "p-a": history({
        provider_id: "p-a",
        earliest_available_day: "2026-09-17",
        continuity_start_day: "2026-09-17",
        end_day: "2026-09-17",
        cumulative: metrics({ request_count: 5 }),
        days: [],
      }),
    };

    const panel = await openProviderHistory();
    await chooseProvider(panel, /Alpha/);
    await waitFor(() =>
      expect(
        panel.getByTestId("ai-gateway-provider-history-cumulative"),
      ).toHaveTextContent("5"),
    );

    const providersBefore = providersQueryCount();
    const historyBefore = historyQueryCount();
    state.identities = [
      identity({ provider_id: "p-a", provider_name: "Alpha" }),
      identity({ provider_id: "p-b", provider_name: "Beta" }),
    ];

    const user = userEvent.setup();
    await user.click(panel.getByTestId("ai-gateway-provider-history-refresh"));

    await waitFor(() => expect(historyQueryCount()).toBeGreaterThan(historyBefore));
    // Manual refresh obtains current data, including the provider identities.
    await waitFor(() => expect(providersQueryCount()).toBeGreaterThan(providersBefore));

    await user.click(
      panel.getByTestId("ai-gateway-provider-history-provider-trigger"),
    );
    expect(panel.getByRole("option", { name: /Beta/ })).toBeInTheDocument();
  });

  it("服务商列表失败可重试且不显示误导空态", async () => {
    state.providersError = "boom";
    const panel = await openProviderHistory();

    expect(
      await panel.findByTestId("ai-gateway-provider-history-providers-error"),
    ).toBeInTheDocument();
    expect(
      panel.getByTestId("ai-gateway-provider-history-providers-retry"),
    ).toBeInTheDocument();
    expect(
      panel.queryByTestId("ai-gateway-provider-history-empty"),
    ).not.toBeInTheDocument();

    // Retrying reloads the current identities and clears the error.
    state.providersError = null;
    state.identities = [
      identity({ provider_id: "p-a", provider_name: "Alpha" }),
    ];
    const before = providersQueryCount();
    const user = userEvent.setup();
    await user.click(
      panel.getByTestId("ai-gateway-provider-history-providers-retry"),
    );
    await waitFor(() => expect(providersQueryCount()).toBeGreaterThan(before));
    await waitFor(() =>
      expect(
        panel.queryByTestId("ai-gateway-provider-history-providers-error"),
      ).not.toBeInTheDocument(),
    );

    await chooseProvider(panel, /Alpha/);
    await waitFor(() => expect(historyQueryCount()).toBeGreaterThan(0));
  });

  it("清除精确服务商/日期聚焦后恢复普通快捷范围六键请求", async () => {
    state.identities = [identity({ provider_id: "p-a", provider_name: "Acme" })];
    state.histories = {
      "p-a": history({
        provider_id: "p-a",
        earliest_available_day: "2026-09-17",
        continuity_start_day: "2026-09-17",
        end_day: "2026-09-17",
        cumulative: metrics({ request_count: 1, successful_request_count: 1 }),
        days: [
          {
            day: "2026-09-17",
            coverage: "continuous",
            ongoing: true,
            metrics: metrics({ request_count: 1, successful_request_count: 1 }),
          },
        ],
      }),
    };

    const panel = await openProviderHistory();
    const user = userEvent.setup();
    await chooseProvider(panel, /Acme/);

    // Opening an exact day stays inside the request-log view with a dated context.
    await user.click(
      panel.getByTestId("ai-gateway-provider-history-open-day-2026-09-17"),
    );
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_request_logs", {
        range: "today",
        groupBy: "none",
        status: null,
        model: null,
        provider: null,
        page: 1,
        providerId: "p-a",
        day: "2026-09-17",
      }),
    );

    const context = screen.getByTestId("ai-gateway-logs-day-context");
    expect(context).toHaveTextContent("p-a");
    expect(context).toHaveTextContent("2026-09-17");

    // A quick range chosen while focused is retained but cannot take effect,
    // because the explicit day takes precedence.
    await user.click(screen.getByTestId("ai-gateway-logs-range-trigger"));
    await user.click(screen.getByRole("option", { name: "30d" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_request_logs", {
        range: "30d",
        groupBy: "none",
        status: null,
        model: null,
        provider: null,
        page: 1,
        providerId: "p-a",
        day: "2026-09-17",
      }),
    );

    // The explicit clear-focus control exits the dated context while staying in
    // the request-log view, and the next request is the exact ordinary
    // six-key payload with the now-active quick range.
    const clearControl = screen.getByTestId("ai-gateway-logs-clear-focus");
    expect(clearControl).toHaveAttribute(
      "aria-label",
      "Clear provider/day focus",
    );
    await user.click(clearControl);
    expect(
      screen.queryByTestId("ai-gateway-logs-day-context"),
    ).not.toBeInTheDocument();
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

    // Ordinary quick-range browsing then keeps working normally.
    await user.click(screen.getByTestId("ai-gateway-logs-range-trigger"));
    await user.click(screen.getByRole("option", { name: "7d" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_request_logs", {
        range: "7d",
        groupBy: "none",
        status: null,
        model: null,
        provider: null,
        page: 1,
      }),
    );
  });
});
