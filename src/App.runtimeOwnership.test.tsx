import { waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "@/App";
import { ThemeProvider } from "@/components/ThemeProvider";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, listenMock, resetTauriMocks } from "@/test/mocks/tauri";

// ---------------------------------------------------------------------------
// Frozen Step 2 contract (20261009-core-workflows-cleanup-and-optimization,
// REQ-002 / AC-002). The process (Rust) owns template scheduling, so the App
// renderer is only a read/subscription adapter:
//
//   * mounting App performs exactly one
//     `ai_gateway_template_auto_refresh_status` read and registers exactly one
//     `ai-gateway-template-auto-refresh-updated` subscription,
//   * the renderer never starts a schedule itself: at startup it must not list
//     templates (`ai_gateway_provider_templates`) nor sync one
//     (`ai_gateway_sync_provider_template`).
//
// Heavy child views are mocked so the assertion observes only App-level
// ownership, exactly like the existing App.* test suites.
// ---------------------------------------------------------------------------

vi.mock("@/components/Launcher", () => ({
  Launcher: () => <div data-testid="mock-launcher" />,
}));
vi.mock("@/components/MoreToolsHub", () => ({
  MoreToolsHub: () => <div data-testid="mock-more-tools" />,
}));
vi.mock("@/components/AiSessions", () => ({
  AiSessions: () => <div data-testid="mock-ai-sessions" />,
}));
vi.mock("@/components/Workspaces", () => ({
  Workspaces: () => <div data-testid="mock-workspaces" />,
}));
vi.mock("@/components/AiGateway", () => ({
  AiGateway: () => <div data-testid="mock-ai-gateway-page">AI Gateway Page</div>,
}));

const TEMPLATE_STATUS_COMMAND = "ai_gateway_template_auto_refresh_status";
const TEMPLATE_UPDATED_EVENT = "ai-gateway-template-auto-refresh-updated";
const SYNC_PROVIDER_TEMPLATE_COMMAND = "ai_gateway_sync_provider_template";
const PROVIDER_TEMPLATES_COMMAND = "ai_gateway_provider_templates";

function commandNames(): string[] {
  return invokeMock.mock.calls.map(([command]) => String(command));
}

function subscriptions(eventName: string) {
  return listenMock.mock.calls.filter(([event]) => event === eventName);
}

describe("App runtime ownership of provider-template refresh (REQ-002/AC-002)", () => {
  beforeEach(() => {
    resetTauriMocks();
    window.matchMedia = vi.fn().mockReturnValue({
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });

    // Mirror the established App.* suite mock: answer the commands the header
    // effects expect and let every other command resolve to undefined.
    invokeMock.mockImplementation(async (command: string) => {
      if (command === TEMPLATE_STATUS_COMMAND) {
        return { failures: [] };
      }
      if (command === "ai_gateway_status") {
        return {
          running: false,
          enabled: true,
          port: 17688,
          local_base_url: "http://127.0.0.1:17688/v1",
          provider_count: 0,
          auto_disabled_count: 0,
          key_count: 0,
          default_key_id: null,
        };
      }
      if (command === "protocol_router_status") {
        return { enabled: false, running: false, port: 17860, route_count: 0 };
      }
      if (command === "ssh_tunnels_snapshot") {
        return { runtime: [], tunnels: [] };
      }
      if (command === "file_sharing_status") {
        return { running: false, files: [] };
      }
      if (command === "get_storage_config") {
        return {
          ok: true,
          data: { language: "zh", storage_type: "local" },
          meta: { schema_version: 1, revision: 1 },
        };
      }
      if (command === "dashboard_counts" || command === "get_dashboard_counts") {
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
      return undefined;
    });
  });

  it("subscribes once to the backend snapshot and never starts a schedule at startup", async () => {
    renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(TEMPLATE_STATUS_COMMAND),
    );
    await waitFor(() =>
      expect(subscriptions(TEMPLATE_UPDATED_EVENT)).toHaveLength(1),
    );

    // Exactly one read and exactly one subscription: the adapter mirrors the
    // backend-owned failure snapshot without owning any schedule.
    expect(
      invokeMock.mock.calls.filter(([command]) => command === TEMPLATE_STATUS_COMMAND),
    ).toHaveLength(1);

    // The renderer must not list or sync templates on startup; the process
    // scheduler does that.
    expect(commandNames()).not.toContain(PROVIDER_TEMPLATES_COMMAND);
    expect(commandNames()).not.toContain(SYNC_PROVIDER_TEMPLATE_COMMAND);
  });
});
