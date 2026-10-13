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

import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { AiGateway } from "@/components/AiGateway";
import {
  formatGatewayTimestamp,
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

  it("exposes copy, candidate expansion, log, edit and more-menu controls", async () => {
    const card = await renderConsole();
    for (const suffix of ["copy", "candidates", "log", "edit", "menu"]) {
      expect(
        within(card).getByTestId(`ai-gateway-model-pool-${suffix}-${POOL_ID}`),
      ).toBeInTheDocument();
    }
    expect(
      within(card).getByTestId(`ai-gateway-model-pool-enable-${POOL_ID}`),
    ).toBeInTheDocument();
  });

  it("shows every unready candidate's reason, keeps intent on, and toggles via the typed command", async () => {
    const card = await renderConsole();

    // The raw backend reasons and the remaining cooldown deadline stay visible
    // (matched by substring so an added localized availability label may render
    // in the same or an adjacent node without breaking this guard).
    expect(
      within(card).getAllByText(/cooling after a model-scoped failure/).length,
    ).toBeGreaterThan(0);
    expect(
      within(card).getAllByText(/source provider disabled/).length,
    ).toBeGreaterThan(0);
    expect(
      within(card).getByText(formatGatewayTimestamp(COOLING_UNTIL) as string),
    ).toBeInTheDocument();

    // No fabricated aggregate/free/traffic claim is rendered on the pool card.
    for (const forbidden of [/traffic/i, /balance/i, /unlimited/i, /infinite/i]) {
      expect(within(card).queryByText(forbidden)).not.toBeInTheDocument();
    }

    // All-unready must not flip the user's enabled intent; toggling is explicit.
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

  it("expands the complete stored candidate list with source identities", async () => {
    const card = await renderConsole();
    const candidates = within(card).getByTestId(
      `ai-gateway-model-pool-candidates-${POOL_ID}`,
    );
    expect(within(candidates).getByText(/local-a1/i)).toBeInTheDocument();
    expect(within(candidates).getByText(/remote-a1/i)).toBeInTheDocument();
    expect(within(candidates).getByText(/local-a2/i)).toBeInTheDocument();
    expect(within(candidates).getByText(/Provider A/i)).toBeInTheDocument();
    expect(within(candidates).getByText(/chat/i)).toBeInTheDocument();
    expect(within(candidates).getByText(/responses/i)).toBeInTheDocument();
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

  // -------------------------------------------------------------------------
  // Review-repair: AC-016 / AC-018 localized status labels and current-config
  // availability (no optimistic ready when status is stale or missing).
  // -------------------------------------------------------------------------

  it("always shows the localized coded availability label and keeps the raw reason in Chinese", async () => {
    const base = buildConfig() as unknown as {
      providers: Array<Record<string, unknown>>;
    };
    const pool = {
      id: POOL_ID,
      name: "Free Modle",
      model: "free-model",
      enabled: true,
      candidates: [
        { provider_id: "p-a", local_model: "local-a1", upstream_model: "remote-a1", protocol: "chat_completions", weight: 1 },
        { provider_id: "p-a", local_model: "local-a2", upstream_model: "remote-a2", protocol: "responses", weight: 1 },
        { provider_id: "p-b", local_model: "local-b1", upstream_model: "remote-b1", protocol: "responses", weight: 1 },
      ],
    };
    mockState.config = {
      ...(base as unknown as GatewayConfig),
      model_pools: [pool],
    } as unknown as GatewayConfig;
    mockState.statusPayload = [
      {
        id: POOL_ID,
        name: "Free Modle",
        model: "free-model",
        enabled: true,
        candidates: [
          { provider_id: "p-a", provider_name: "Provider A", local_model: "local-a1", upstream_model: "remote-a1", protocol: "chat_completions", weight: 1, ready: false, availability: "cooling", reason: "cooling after a model-scoped failure", cooling_until: COOLING_UNTIL },
          { provider_id: "p-a", provider_name: "Provider A", local_model: "local-a2", upstream_model: "remote-a2", protocol: "responses", weight: 1, ready: false, availability: "source_disabled", reason: "source provider disabled", cooling_until: null },
          { provider_id: "p-b", provider_name: "Provider B", local_model: "local-b1", upstream_model: "remote-b1", protocol: "responses", weight: 1, ready: false, availability: "no_usable_key", reason: "no usable upstream key", cooling_until: null },
        ],
      },
    ];

    await i18n.changeLanguage("zh");
    const card = await renderConsole();

    const shows = (pattern: RegExp) =>
      expect(within(card).getAllByText(pattern).length).toBeGreaterThan(0);

    // The localized coded label is always present, per availability code.
    shows(new RegExp(i18n.t("aiGatewayModelPoolStatusCooling")));
    shows(new RegExp(i18n.t("aiGatewayModelPoolStatusSourceDisabled")));
    shows(new RegExp(i18n.t("aiGatewayModelPoolStatusNoUsableKey")));
    // The raw backend reason is preserved as additional detail.
    shows(/cooling after a model-scoped failure/);
    shows(/source provider disabled/);
    shows(/no usable upstream key/);
    // Cooldown deadline and user intent are unchanged.
    expect(
      within(card).getByText(formatGatewayTimestamp(COOLING_UNTIL) as string),
    ).toBeInTheDocument();
    expect(
      within(card).getByTestId(`ai-gateway-model-pool-enable-${POOL_ID}`),
    ).toBeInTheDocument();
    expect(
      within(card).getAllByText(
        new RegExp(i18n.t("aiGatewayModelPoolIntentEnabled")),
      ).length,
    ).toBeGreaterThan(0);
  });

  it("never shows a disabled candidate as ready with a stale or missing status snapshot", async () => {
    const base = buildConfig() as unknown as {
      providers: Array<Record<string, unknown>>;
    } & GatewayConfig;
    const disabledProviders = base.providers.map((provider) =>
      provider.id === "p-a" ? { ...provider, enabled: false } : provider,
    );
    const readySnapshot = [
      {
        id: POOL_ID,
        name: "Free Modle",
        model: "free-model",
        enabled: true,
        candidates: [
          { provider_id: "p-a", provider_name: "Provider A", local_model: "local-a1", upstream_model: "remote-a1", protocol: "chat_completions", weight: 1, ready: true, availability: "ready", reason: null, cooling_until: null },
          { provider_id: "p-a", provider_name: "Provider A", local_model: "local-a2", upstream_model: "remote-a2", protocol: "responses", weight: 1, ready: true, availability: "ready", reason: null, cooling_until: null },
        ],
      },
    ];

    const disabledLabel = () => new RegExp(i18n.t("aiGatewayModelPoolStatusSourceDisabled"));
    const readyLabel = () => new RegExp(`^${i18n.t("aiGatewayModelPoolStatusReady")}$`);

    // With a stale snapshot that still claims ready, current config wins.
    for (const statusPayload of [readySnapshot, []]) {
      cleanup();
      mockState.config = {
        ...base,
        providers: disabledProviders,
      } as unknown as GatewayConfig;
      mockState.statusPayload = statusPayload;

      const card = await renderConsole();
      expect(
        within(card).queryAllByText(disabledLabel()).length,
      ).toBeGreaterThan(0);
      expect(within(card).queryAllByText(readyLabel()).length).toBe(0);
    }
  });
});
