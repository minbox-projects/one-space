// Step 3 (20261013-free-model-pool): saved model-pool card behavior (REQ-008;
// AC-013 / AC-014 / AC-016 / AC-012). Review-repair expansion of the initial
// card-identity RED.
//
// The dedicated `ModelPoolCard` component does not exist yet, so this file
// drives the existing console entry (`AiGateway`) with pool-bearing config and
// status fixtures. No nonexistent product symbol is imported.
//
// Public surface / contracts this file fixes for the implementation:
//   - card:                data-testid="ai-gateway-model-pool-<poolId>"
//   - copy public id:      data-testid="ai-gateway-model-pool-copy-<poolId>"
//   - candidate expansion: data-testid="ai-gateway-model-pool-candidates-<poolId>"
//   - log shortcut:        data-testid="ai-gateway-model-pool-log-<poolId>"
//   - edit:                data-testid="ai-gateway-model-pool-edit-<poolId>"
//   - more menu:           data-testid="ai-gateway-model-pool-menu-<poolId>"
//   - delete item:         data-testid="ai-gateway-model-pool-delete-<poolId>"
//   - delete confirm:      data-testid="ai-gateway-model-pool-delete-confirm"
//   - enable switch:       data-testid="ai-gateway-model-pool-enable-<poolId>"
//   - commands:            ai_gateway_set_model_pool_enabled,
//                          ai_gateway_delete_model_pool,
//                          ai_gateway_model_pools_status

import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { AiGateway } from "@/components/AiGateway";
import {
  type GatewayConfig,
  type GatewayStatus,
} from "@/lib/aiGateway";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

const POOL_ID = "pool-free";
const COOLING_UNTIL = 1_900_000_000;

function buildConfig(): GatewayConfig {
  return {
    enabled: true,
    port: 17688,
    providers: [
      {
        id: "p-a",
        name: "Provider A",
        base_url: "https://api.a.example",
        default_model: null,
        protocol: "chat_completions",
        mappings: [
          { local_model: "local-a1", upstream_model: "remote-a1" },
          { local_model: "local-a2", upstream_model: "remote-a2", protocol: "responses" },
        ],
        enabled: true,
      },
      {
        id: "p-b",
        name: "Provider B",
        base_url: "https://api.b.example",
        default_model: null,
        protocol: "responses",
        mappings: [{ local_model: "local-b1", upstream_model: "remote-b1" }],
        enabled: true,
      },
    ],
    keys: [
      { id: "k1", label: "Main", value: "********", enabled: true, created_at: 1 },
    ],
    default_key_id: "k1",
    terminal_syncs: [],
    model_pools: [
      {
        id: POOL_ID,
        name: "Free Modle",
        model: "free-model",
        enabled: true,
        candidates: [
          {
            provider_id: "p-a",
            local_model: "local-a1",
            upstream_model: "remote-a1",
            protocol: "chat_completions",
            weight: 1,
          },
          {
            provider_id: "p-a",
            local_model: "local-a2",
            upstream_model: "remote-a2",
            protocol: "responses",
            weight: 1,
          },
        ],
      },
    ],
    model_pool_reserved_models: [],
  } as unknown as GatewayConfig;
}

function buildStatus(): GatewayStatus {
  return {
    running: true,
    enabled: true,
    port: 17688,
    local_base_url: "http://127.0.0.1:17688/v1",
    provider_count: 2,
    auto_disabled_count: 0,
    key_count: 1,
    default_key_id: "k1",
  };
}

/** Every stored candidate of the pool is currently unready. */
function poolStatus() {
  return [
    {
      id: POOL_ID,
      name: "Free Modle",
      model: "free-model",
      enabled: true,
      candidates: [
        {
          provider_id: "p-a",
          provider_name: "Provider A",
          local_model: "local-a1",
          upstream_model: "remote-a1",
          protocol: "chat_completions",
          weight: 1,
          ready: false,
          availability: "cooling",
          reason: "cooling after a model-scoped failure",
          cooling_until: COOLING_UNTIL,
        },
        {
          provider_id: "p-a",
          provider_name: "Provider A",
          local_model: "local-a2",
          upstream_model: "remote-a2",
          protocol: "responses",
          weight: 1,
          ready: false,
          availability: "source_disabled",
          reason: "source provider disabled",
          cooling_until: null,
        },
      ],
    },
  ];
}

const mockState = {
  config: null as GatewayConfig | null,
  statusPayload: null as unknown[] | null,
  setEnabled: [] as Array<Record<string, unknown>>,
  deleted: [] as Array<Record<string, unknown>>,
};

function installMock() {
  invokeMock.mockImplementation(async (command: string, args?: any) => {
    switch (command) {
      case "ai_gateway_get_config":
        return mockState.config ?? buildConfig();
      case "ai_gateway_status":
        return buildStatus();
      case "ai_gateway_terminal_targets":
        return [];
      case "ai_gateway_model_pools_status":
        return mockState.statusPayload ?? poolStatus();
      case "ai_gateway_set_model_pool_enabled":
        mockState.setEnabled.push(args ?? {});
        return mockState.config ?? buildConfig();
      case "ai_gateway_delete_model_pool":
        mockState.deleted.push(args ?? {});
        return mockState.config ?? buildConfig();
      default:
        return undefined;
    }
  });
}

async function renderConsole() {
  renderWithProviders(<AiGateway />);
  await screen.findByText("Provider A");
  return screen.getByTestId(`ai-gateway-model-pool-${POOL_ID}`);
}

describe("ModelPoolCard", () => {
  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
    mockState.config = null;
    mockState.statusPayload = null;
    mockState.setEnabled = [];
    mockState.deleted = [];
    installMock();
  });

  it("renders the enabled pool card with its public identity", async () => {
    const card = await renderConsole();
    expect(card).toBeInTheDocument();
    expect(within(card).getByText("Free Modle")).toBeInTheDocument();
    expect(within(card).getByText("free-model")).toBeInTheDocument();
  });

  it("exposes copy, log, edit and more-menu controls", async () => {
    const card = await renderConsole();
    for (const suffix of ["copy", "log", "edit", "menu"]) {
      expect(
        within(card).getByTestId(`ai-gateway-model-pool-${suffix}-${POOL_ID}`),
      ).toBeInTheDocument();
    }
    expect(
      within(card).getByTestId(`ai-gateway-model-pool-enable-${POOL_ID}`),
    ).toBeInTheDocument();
  });

  it("keeps intent on and toggles via the typed command", async () => {
    const card = await renderConsole();

    // No fabricated aggregate/free/traffic claim is rendered on the pool card.
    for (const forbidden of [/traffic/i, /balance/i, /unlimited/i, /infinite/i]) {
      expect(within(card).queryByText(forbidden)).not.toBeInTheDocument();
    }

    // Toggling is explicit and dispatches the command.
    fireEvent.click(
      within(card).getByTestId(`ai-gateway-model-pool-enable-${POOL_ID}`),
    );
    await waitFor(() =>
      expect(mockState.setEnabled).toEqual([{ poolId: POOL_ID, enabled: false }]),
    );
  });

  it("copies the public model id through the clipboard", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });

    const card = await renderConsole();
    fireEvent.click(
      within(card).getByTestId(`ai-gateway-model-pool-copy-${POOL_ID}`),
    );
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("free-model"));
  });

  it("does not render candidate expansion list on card to keep it compact and clean", async () => {
    const card = await renderConsole();
    expect(
      within(card).queryByTestId(`ai-gateway-model-pool-candidates-${POOL_ID}`),
    ).not.toBeInTheDocument();
    // Candidate counts and source counts are still summarized
    expect(within(card).getByText(/candidate/i)).toBeInTheDocument();
    expect(within(card).getByText(/source/i)).toBeInTheDocument();
  });

  it("clicking the model pool name opens the edit dialog", async () => {
    const card = await renderConsole();
    fireEvent.click(
      within(card).getByTestId(`ai-gateway-model-pool-name-${POOL_ID}`),
    );

    const dialog = await screen.findByTestId("model-pool-dialog");
    expect(dialog).toBeInTheDocument();
    expect(screen.getByTestId("model-pool-name-input")).toHaveValue("Free Modle");
  });

  it("log shortcut opens the un-grouped log view filtered by the pool alias", async () => {
    const card = await renderConsole();

    const requestLogsCalls = () =>
      invokeMock.mock.calls.filter(
        ([command]) => command === "ai_gateway_request_logs",
      );

    // Preserve the required mount-dashboard guard: the initial load keeps its
    // today/day, unfiltered request (the same contract pinned in AiGateway.test).
    expect(requestLogsCalls()).toContainEqual([
      "ai_gateway_request_logs",
      {
        range: "today",
        groupBy: "day",
        status: null,
        model: null,
        provider: null,
        page: 1,
      },
    ]);

    // Only inspect the request(s) produced BY THE CLICK, so the earlier
    // dashboard request can never satisfy the assertion.
    const callsBeforeClick = requestLogsCalls().length;

    fireEvent.click(
      within(card).getByTestId(`ai-gateway-model-pool-log-${POOL_ID}`),
    );

    await waitFor(() => {
      const shortcutCalls = requestLogsCalls().slice(callsBeforeClick);
      expect(shortcutCalls.length).toBeGreaterThan(0);
      // No un-filtered / grouped request may leak from the shortcut: every
      // request produced by the click carries the alias and stays un-grouped.
      for (const [, args] of shortcutCalls) {
        expect(args).toEqual({
          range: "today",
          groupBy: "none",
          status: null,
          model: "free-model",
          provider: null,
          page: 1,
        });
      }
    });

    // The existing un-grouped log subtab is selected by the shortcut.
    expect(screen.getByTestId("ai-gateway-subtab-usage-logs")).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });

  it("edit reopens the saved name, public id and weights", async () => {
    const card = await renderConsole();
    fireEvent.click(
      within(card).getByTestId(`ai-gateway-model-pool-edit-${POOL_ID}`),
    );

    const dialog = await screen.findByTestId("model-pool-dialog");
    expect(dialog).toBeInTheDocument();
    expect(screen.getByTestId("model-pool-name-input")).toHaveValue("Free Modle");
    expect(screen.getByTestId("model-pool-model-input")).toHaveValue("free-model");
    expect(screen.getByRole("spinbutton", { name: /local-a1/i })).toHaveValue(1);
    expect(screen.getByRole("spinbutton", { name: /local-a2/i })).toHaveValue(1);
  });

  it("deletes through the more-menu confirm and touches no ordinary provider", async () => {
    const card = await renderConsole();
    fireEvent.click(
      within(card).getByTestId(`ai-gateway-model-pool-menu-${POOL_ID}`),
    );
    fireEvent.click(
      screen.getByTestId(`ai-gateway-model-pool-delete-${POOL_ID}`),
    );
    fireEvent.click(screen.getByTestId("ai-gateway-model-pool-delete-confirm"));

    await waitFor(() =>
      expect(mockState.deleted).toEqual([{ poolId: POOL_ID }]),
    );
    expect(
      invokeMock.mock.calls.some(([command]) => command === "ai_gateway_delete_provider"),
    ).toBe(false);
  });

  it("runs no per-card quota/health network query", async () => {
    await renderConsole();
    const networkCommands = invokeMock.mock.calls
      .map(([command]) => command)
      .filter((command) =>
        ["ai_gateway_provider_quota", "ai_gateway_provider_go_usage"].includes(command),
      );
    expect(networkCommands).toEqual([]);
  });

  it("copies through execCommand fallback when navigator.clipboard fails", async () => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: vi.fn().mockRejectedValue(new Error("denied")),
      },
    });
    const execCommand = vi.fn().mockReturnValue(true);
    document.execCommand = execCommand;

    const card = await renderConsole();
    fireEvent.click(
      within(card).getByTestId(`ai-gateway-model-pool-copy-${POOL_ID}`),
    );
    await waitFor(() => expect(execCommand).toHaveBeenCalledWith("copy"));
  });
});
