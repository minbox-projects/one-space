import { screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "@/App";
import { ThemeProvider } from "@/components/ThemeProvider";
import i18n from "@/i18n";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";
import type { GatewayStatus } from "@/lib/aiGateway";

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
});
