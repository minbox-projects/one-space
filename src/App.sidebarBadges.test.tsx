import { act, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "@/App";
import { ThemeProvider } from "@/components/ThemeProvider";
import i18n from "@/i18n";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";
import type { GatewayStatus } from "@/lib/aiGateway";
import { CLOUD_AGENT_SESSIONS_COUNT_EVENT } from "@/lib/cloudAgent";

vi.mock("@/components/Launcher", () => ({
  Launcher: () => <div data-testid="mock-launcher" />,
}));

const TODAY_TOKENS = 1_234_567;

/** Anchored accessible-name matcher; count badges join the accessible name. */
function namePattern(label: string): RegExp {
  const escaped = label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`^${escaped}(\\s|$)`);
}

function renderApp() {
  renderWithProviders(
    <ThemeProvider>
      <App />
    </ThemeProvider>,
  );
}

describe("App 侧边栏统计徽标", () => {
  beforeEach(() => {
    resetTauriMocks();
    localStorage.clear();

    window.matchMedia = vi.fn().mockReturnValue({
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });

    const gatewayStatus: GatewayStatus = {
      running: true,
      enabled: true,
      port: 17688,
      local_base_url: "http://127.0.0.1:17688/v1",
      provider_count: 3,
      auto_disabled_count: 0,
      key_count: 1,
      default_key_id: "k1",
    };

    invokeMock.mockImplementation(async (command: string) => {
      if (command === "get_storage_config") {
        return {
          ok: true,
          data: { language: "zh", storage_type: "local" },
          meta: { schema_version: 1, revision: 1 },
        };
      }
      if (command === "dashboard_counts") {
        return {
          ok: true,
          data: {
            launcher: 0,
            workspaces: 0,
            sessions: 0,
            ssh: 0,
            snippets: 0,
            bookmarks: 0,
            notes: 0,
            ai_news: 0,
            environments: 0,
            skills: 0,
            subagents: 0,
            mcp_servers: 0,
          },
          meta: { schema_version: 1, revision: 1 },
        };
      }
      if (command === "ai_gateway_status") {
        return gatewayStatus;
      }
      if (command === "protocol_router_status") {
        return { enabled: false, running: false, port: 17860, route_count: 0 };
      }
      if (command === "sessions_usage_day_stats") {
        return {
          date: "2026-09-29",
          total_tokens: TODAY_TOKENS,
          calls: 10,
          sessions: 2,
          input_tokens: 100,
          output_tokens: 50,
          cache_tokens: 0,
        };
      }
      if (command === "cloud_agent_list_sessions") {
        return [
          { session_name: "s1", repo_name: "r1", is_active: true },
          { session_name: "s2", repo_name: "r2", is_active: false },
          { session_name: "s3", repo_name: "r3", is_active: true },
        ];
      }
      return undefined;
    });
  });

  it("AI 网关徽标显示上游服务商数量", async () => {
    renderApp();
    await screen.findByText("OneSpace");

    const gatewayButton = await screen.findByRole("button", {
      name: namePattern(i18n.t("aiGateway", "AI Gateway")),
    });
    expect(within(gatewayButton).getByText("3")).toBeInTheDocument();
  });

  it("AI 用量统计徽标显示今日总 Token 的紧凑值", async () => {
    renderApp();
    await screen.findByText("OneSpace");

    const expectedBadge = new Intl.NumberFormat(undefined, {
      notation: "compact",
      maximumFractionDigits: 1,
    }).format(TODAY_TOKENS);

    const usageButton = await screen.findByRole("button", {
      name: namePattern(i18n.t("aiUsageStatsMenu", "AI Usage Stats")),
    });
    await waitFor(() => {
      expect(within(usageButton).getByText(expectedBadge)).toBeInTheDocument();
    });
  });

  it("AI 云端智能体徽标显示活跃会话总数并支持事件联动实时更新", async () => {
    renderApp();
    await screen.findByText("OneSpace");

    const cloudAgentButton = await screen.findByRole("button", {
      name: namePattern(i18n.t("cloudAgent", "AI Cloud Agent")),
    });

    // 初始异步加载完成后的徽标（2 个 active，1 个 inactive，总计 2）
    await waitFor(() => {
      expect(within(cloudAgentButton).getByText("2")).toBeInTheDocument();
    });

    // 触发云端控制台同步事件，验证联动更新至 5
    act(() => {
      window.dispatchEvent(
        new CustomEvent(CLOUD_AGENT_SESSIONS_COUNT_EVENT, {
          detail: { count: 5 },
        }),
      );
    });

    await waitFor(() => {
      expect(within(cloudAgentButton).getByText("5")).toBeInTheDocument();
    });
  });
});
