// Step 3 (20261013-free-model-pool): operator model-pool dialog behavior
// (REQ-008; AC-013 / AC-014 / AC-015). Review-repair expansion of the initial
// basic-field RED.
//
// The dedicated `ModelPoolDialog` component does not exist yet, so this file
// drives the existing console entry (`AiGateway`) through the pinned
// Add-provider picker channel. No nonexistent product symbol is imported.
//
// Public surface / contracts this file fixes for the implementation:
//   - picker channel:      data-testid="template-picker-model-pool-btn"
//   - dialog container:    data-testid="model-pool-dialog"
//   - name field:          data-testid="model-pool-name-input"   (default "Free Modle")
//   - public id field:     data-testid="model-pool-model-input"  (default "free-model")
//   - source search:       data-testid="model-pool-source-search"
//   - source group:        data-testid="model-pool-source-group-<providerId>"
//   - candidate checkbox:  role="checkbox", accessible name contains the local model
//   - weight input:        role="spinbutton", accessible name contains the local model
//   - selected list:       data-testid="model-pool-selected-list"
//   - save / cancel:       data-testid="model-pool-save" / "model-pool-cancel"
//   - validation error:    data-testid="model-pool-error" (role="alert")
//   - upsert command:      invoke("ai_gateway_upsert_model_pool", { pool })

import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { AiGateway } from "@/components/AiGateway";
import {
  AI_GATEWAY_CONFIG_UPDATED_EVENT,
  type GatewayConfig,
  type GatewayStatus,
} from "@/lib/aiGateway";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, listenMock, resetTauriMocks } from "@/test/mocks/tauri";

type ProviderFixture = {
  id: string;
  name: string;
  protocol: "chat_completions" | "responses";
  mappings: Array<{
    local_model: string;
    upstream_model: string;
    protocol?: "chat_completions" | "responses";
  }>;
  enabled?: boolean;
};

const PROVIDERS: ProviderFixture[] = [
  {
    id: "p-a",
    name: "Provider A",
    protocol: "chat_completions",
    enabled: true,
    mappings: [
      { local_model: "local-a1", upstream_model: "remote-a1" },
      { local_model: "local-a2", upstream_model: "remote-a2", protocol: "responses" },
    ],
  },
  {
    id: "p-b",
    name: "Provider B",
    protocol: "responses",
    enabled: true,
    mappings: [{ local_model: "local-b1", upstream_model: "remote-b1" }],
  },
];

const mockState: {
  config: GatewayConfig;
  statusPayload: unknown[];
  upsertError: string | null;
  deleted: Array<Record<string, unknown>>;
  templates: unknown[];
} = {
  config: buildConfig([]),
  statusPayload: [],
  upsertError: null,
  deleted: [],
  templates: [],
};

function makeProvider(
  overrides: Partial<ProviderFixture> & { template_id?: string; enabled?: boolean } = {},
): ProviderFixture & { template_id?: string; enabled?: boolean } {
  return {
    id: "p-test",
    name: "Provider Test",
    protocol: "chat_completions",
    mappings: [],
    enabled: true,
    ...overrides,
  };
}

function buildConfig(
  pools: unknown[],
  providers: unknown[] = PROVIDERS,
): GatewayConfig {
  return {
    enabled: true,
    port: 17688,
    providers: providers as unknown as GatewayConfig["providers"],
    keys: [
      { id: "k1", label: "Main", value: "********", enabled: true, created_at: 1 },
    ],
    default_key_id: "k1",
    terminal_syncs: [],
    model_pools: pools,
  } as unknown as GatewayConfig;
}

function buildStatus(): GatewayStatus {
  return {
    running: true,
    enabled: true,
    port: 17688,
    local_base_url: "http://127.0.0.1:17688/v1",
    provider_count: PROVIDERS.length,
    auto_disabled_count: 0,
    key_count: 1,
    default_key_id: "k1",
  };
}

function installMock() {
  invokeMock.mockImplementation(async (command: string, args?: any) => {
    switch (command) {
      case "ai_gateway_get_config":
        return mockState.config;
      case "ai_gateway_status":
        return buildStatus();
      case "ai_gateway_provider_templates":
        return mockState.templates;
      case "ai_gateway_terminal_targets":
        return [];
      case "ai_gateway_model_pools_status":
        return mockState.statusPayload;
      case "ai_gateway_upsert_model_pool":
        if (mockState.upsertError) throw new Error(mockState.upsertError);
        mockState.config = buildConfig([args?.pool]);
        return mockState.config;
      case "ai_gateway_delete_model_pool":
        mockState.deleted.push(args ?? {});
        return mockState.config;
      default:
        return undefined;
    }
  });
}

/** Open the Model pools tab in the active locale so its panel mounts. */
function openModelPoolsTab() {
  fireEvent.click(
    screen.getByRole("tab", {
      name: new RegExp(`^${i18n.t("aiGatewayModelPoolsTab")}`),
    }),
  );
}

/** Open the Model pools tab, then click the Create-model-pool button. */
async function openModelPoolDialog(firstProviderName = "Provider A") {
  renderWithProviders(<AiGateway />);
  await screen.findByText(firstProviderName);
  openModelPoolsTab();
  fireEvent.click(screen.getByTestId("ai-gateway-add-model-pool"));
  return screen.getByTestId("model-pool-dialog");
}

function upsertCalls(): Array<Record<string, unknown>> {
  return invokeMock.mock.calls
    .filter(([command]) => command === "ai_gateway_upsert_model_pool")
    .map(([, args]) => (args as { pool: Record<string, unknown> }).pool);
}

function candidatesOf(pool: Record<string, unknown>): Array<Record<string, unknown>> {
  const candidates = (pool.candidates ?? []) as Array<Record<string, unknown>>;
  return [...candidates].sort((a, b) =>
    String(a.local_model).localeCompare(String(b.local_model)),
  );
}

describe("ModelPoolDialog", () => {
  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
    mockState.config = buildConfig([]);
    mockState.statusPayload = [];
    mockState.upsertError = null;
    mockState.deleted = [];
    installMock();
  });

  it("opens from the picker with the Free Modle / free-model defaults", async () => {
    const dialog = await openModelPoolDialog();

    expect(dialog).toBeInTheDocument();
    expect(screen.getByTestId("model-pool-name-input")).toHaveValue("Free Modle");
    expect(screen.getByTestId("model-pool-model-input")).toHaveValue("free-model");
  });

  it("selects same-provider siblings independently with integer weights and saves exact tuples", async () => {
    await openModelPoolDialog();

    // Two models from ONE provider are independent candidates.
    fireEvent.click(screen.getByRole("checkbox", { name: /local-a1/i }));
    fireEvent.click(screen.getByRole("checkbox", { name: /local-a2/i }));

    fireEvent.change(screen.getByRole("spinbutton", { name: /local-a1/i }), {
      target: { value: "3" },
    });
    fireEvent.change(screen.getByRole("spinbutton", { name: /local-a2/i }), {
      target: { value: "1" },
    });

    // A new pool starts at the concrete `medium` default, which requires at
    // least one valid candidate mapping. Select `auto` so this pre-existing
    // test still covers the original save/payload behavior; the new default and
    // its save guard keep their dedicated coverage in the Step 4 tests below.
    fireEvent.change(screen.getByTestId("model-pool-reasoning-default"), {
      target: { value: "auto" },
    });

    fireEvent.click(screen.getByTestId("model-pool-save"));

    await waitFor(() => expect(upsertCalls()).toHaveLength(1));
    const pool = upsertCalls()[0];

    expect(pool.name).toBe("Free Modle");
    expect(pool.model).toBe("free-model");
    expect(candidatesOf(pool)).toEqual([
      {
        provider_id: "p-a",
        local_model: "local-a1",
        upstream_model: "remote-a1",
        protocol: "chat_completions",
        weight: 3,
      },
      {
        provider_id: "p-a",
        local_model: "local-a2",
        upstream_model: "remote-a2",
        protocol: "responses",
        weight: 1,
      },
    ]);

    // No source key, endpoint or secret is cloned into the pool payload.
    const serialized = JSON.stringify(pool);
    expect(serialized).not.toContain("api_key");
    expect(serialized).not.toContain("base_url");
    expect(serialized).not.toContain("sk-");
  });

  it("blocks saving an empty selection or an invalid weight with a visible error and no write", async () => {
    await openModelPoolDialog();

    // Empty selection: no candidate may be published.
    fireEvent.click(screen.getByTestId("model-pool-save"));
    expect(await screen.findByTestId("model-pool-error")).toBeInTheDocument();
    expect(upsertCalls()).toHaveLength(0);

    // A valid selection with weight 0 / 101 / fractional is rejected.
    for (const invalid of ["0", "101", "1.5"]) {
      fireEvent.click(screen.getByRole("checkbox", { name: /local-a1/i }));
      fireEvent.change(screen.getByRole("spinbutton", { name: /local-a1/i }), {
        target: { value: invalid },
      });
      fireEvent.click(screen.getByTestId("model-pool-save"));
      await waitFor(() =>
        expect(screen.getByTestId("model-pool-error")).toBeInTheDocument(),
      );
      expect(upsertCalls()).toHaveLength(0);
      // Deselect before the next iteration so each case starts from a fresh
      // single-candidate selection.
      fireEvent.click(screen.getByRole("checkbox", { name: /local-a1/i }));
    }
  });

  it("keeps the name, selection and weights across search, group collapse and a config reload", async () => {
    await openModelPoolDialog();

    fireEvent.change(screen.getByTestId("model-pool-name-input"), {
      target: { value: "My Pool" },
    });
    fireEvent.change(screen.getByTestId("model-pool-model-input"), {
      target: { value: "my-pool" },
    });
    fireEvent.click(screen.getByRole("checkbox", { name: /local-a1/i }));
    fireEvent.change(screen.getByRole("spinbutton", { name: /local-a1/i }), {
      target: { value: "7" },
    });

    // Capture the public config-update event bridge (no private hook mock).
    const configHandlers: Array<(event: unknown) => void> = [];
    (
      listenMock as unknown as {
        mockImplementation: (
          fn: (
            eventName: string,
            handler: (event: unknown) => void,
          ) => Promise<() => void>,
        ) => void;
      }
    ).mockImplementation(
      async (eventName: string, handler: (event: unknown) => void) => {
        if (eventName === AI_GATEWAY_CONFIG_UPDATED_EVENT) {
          configHandlers.push(handler);
        }
        return vi.fn();
      },
    );

    // Search change and group collapse must not clear the draft.
    fireEvent.change(screen.getByTestId("model-pool-source-search"), {
      target: { value: "local-a1" },
    });
    fireEvent.click(screen.getByTestId("model-pool-source-group-p-a"));
    expect(screen.getByTestId("model-pool-name-input")).toHaveValue("My Pool");
    expect(screen.getByTestId("model-pool-model-input")).toHaveValue("my-pool");
    expect(screen.getByRole("spinbutton", { name: /local-a1/i })).toHaveValue(7);

    // A runtime/config update arrives: the draft is preserved and availability
    // refreshes from the latest config.
    mockState.config = buildConfig([]);
    await act(async () => {
      for (const handler of configHandlers) handler({ payload: undefined });
    });
    expect(screen.getByTestId("model-pool-name-input")).toHaveValue("My Pool");
    expect(screen.getByRole("spinbutton", { name: /local-a1/i })).toHaveValue(7);
  });

  it("preserves the draft and surfaces an actionable error when a new source disappears on save", async () => {
    await openModelPoolDialog();

    fireEvent.change(screen.getByTestId("model-pool-name-input"), {
      target: { value: "My Pool" },
    });
    fireEvent.click(screen.getByRole("checkbox", { name: /local-a1/i }));
    fireEvent.change(screen.getByRole("spinbutton", { name: /local-a1/i }), {
      target: { value: "9" },
    });

    // Select `auto` so the new concrete-default save guard does not preempt the
    // mocked backend failure this test exercises; the draft and error
    // assertions below are unchanged.
    fireEvent.change(screen.getByTestId("model-pool-reasoning-default"), {
      target: { value: "auto" },
    });

    mockState.upsertError =
      "model pool candidate references an unknown provider/mapping/protocol: local-a1 -> remote-a1 (chat/completions)";
    fireEvent.click(screen.getByTestId("model-pool-save"));

    expect(await screen.findByTestId("model-pool-error")).toHaveTextContent(
      /unknown provider/i,
    );
    // Draft survives the failed save.
    expect(screen.getByTestId("model-pool-name-input")).toHaveValue("My Pool");
    expect(screen.getByRole("spinbutton", { name: /local-a1/i })).toHaveValue(9);
    // Nothing was persisted.
    expect(mockState.upsertError).not.toBeNull();
  });

  it("keeps a stored missing-provider candidate explicit when editing an existing pool", async () => {
    mockState.config = buildConfig([
      {
        id: "pool-existing",
        name: "Existing Pool",
        model: "existing-model",
        enabled: true,
        candidates: [
          {
            provider_id: "p-gone",
            local_model: "local-gone",
            upstream_model: "remote-gone",
            protocol: "chat_completions",
            weight: 5,
          },
        ],
      },
    ]);

    renderWithProviders(<AiGateway />);
    await screen.findByText("Provider A");
    openModelPoolsTab();
    fireEvent.click(screen.getByTestId("ai-gateway-model-pool-edit-pool-existing"));

    const selected = screen.getByTestId("model-pool-selected-list");
    // The stored reference stays explicit and visible even though its provider
    // no longer resolves, and its stored weight is preserved.
    expect(within(selected).getByText(/local-gone/i)).toBeInTheDocument();
    expect(screen.getByRole("spinbutton", { name: /local-gone/i })).toHaveValue(5);
    // A new reference that was not selected is not published as selected.
    expect(within(selected).queryByText(/local-b1/i)).not.toBeInTheDocument();
  });

  it("cancel publishes no write", async () => {
    await openModelPoolDialog();

    fireEvent.click(screen.getByTestId("model-pool-cancel"));

    await waitFor(() => {
      expect(
        invokeMock.mock.calls.some(
          ([command]) => command === "ai_gateway_upsert_model_pool",
        ),
      ).toBe(false);
    });
  });

  it("renders bilingual labels through the existing i18n helper", async () => {
    await openModelPoolDialog();
    expect(screen.getByTestId("model-pool-name-input")).toBeInTheDocument();
    // The channel label is a new bilingual key: the English string must not
    // survive a switch to Chinese.
    const channelEn = screen.queryByTestId("template-picker-model-pool-btn");
    // The dialog is open; re-open the picker label through i18n directly so the
    // assertion is independent of the currently open dialog.
    const enLabel = i18n.t("aiGatewayModelPoolCreate");
    await i18n.changeLanguage("zh");
    const zhLabel = i18n.t("aiGatewayModelPoolCreate");
    expect(zhLabel).not.toBe(enLabel);
    expect(zhLabel.length).toBeGreaterThan(0);
    void channelEn;
  });

  // -------------------------------------------------------------------------
  // Review-repair: AC-016 / AC-018 availability for a NEW draft derives from
  // current config (no saved pool snapshot), localized codes, no cross-pool
  // cooldown leakage, and a removed source keeps its draft with a fresh reason.
  // -------------------------------------------------------------------------

  const configuredKey = (id: string) => ({
    id,
    name: "Main",
    value: "********",
    enabled: true,
    auto_marked: false,
    failure_kind: null,
    marked_at: null,
    reason: null,
  });

  it("shows localized current-config availability for a new draft's disabled and unusable sources", async () => {
    const providers = [
      {
        id: "p-a",
        name: "Provider A",
        base_url: "https://api.a.example",
        default_model: null,
        protocol: "chat_completions",
        enabled: true,
        keys: [configuredKey("k-a")],
        mappings: [{ local_model: "local-a1", upstream_model: "remote-a1" }],
      },
      {
        id: "p-b",
        name: "Provider B",
        base_url: "https://api.b.example",
        default_model: null,
        protocol: "responses",
        enabled: false,
        keys: [configuredKey("k-b")],
        mappings: [{ local_model: "local-b1", upstream_model: "remote-b1" }],
      },
      {
        id: "p-c",
        name: "Provider C",
        base_url: "https://api.c.example",
        default_model: null,
        protocol: "chat_completions",
        enabled: true,
        keys: [],
        mappings: [{ local_model: "local-c1", upstream_model: "remote-c1" }],
      },
    ];
    const savedPool = {
      id: "pool-saved",
      name: "Saved Pool",
      model: "saved-model",
      enabled: true,
      candidates: [
        {
          provider_id: "p-b",
          local_model: "local-b1",
          upstream_model: "remote-b1",
          protocol: "chat_completions",
          weight: 1,
        },
      ],
    };

    mockState.config = {
      ...(buildConfig([savedPool]) as unknown as Record<string, unknown>),
      providers,
      model_pools: [savedPool],
    } as unknown as GatewayConfig;
    // Another saved pool currently cools p-a/local-a1; the NEW draft must not
    // inherit that unrelated cooldown.
    mockState.statusPayload = [
      {
        id: "pool-other",
        name: "Other",
        model: "other-model",
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
            cooling_until: 1_900_000_000,
          },
        ],
      },
    ];

    renderWithProviders(<AiGateway />);
    await screen.findByText("Provider A");
    openModelPoolsTab();
    await waitFor(() =>
      expect(screen.getByTestId("ai-gateway-model-pool-pool-saved")).toBeInTheDocument(),
    );

    fireEvent.click(screen.getByTestId("ai-gateway-model-pool-edit-pool-saved"));
    expect(await screen.findByTestId("model-pool-dialog")).toBeInTheDocument();

    // 来源模型中不展示已禁用的 Provider B
    expect(screen.queryByTestId("model-pool-source-group-p-b")).not.toBeInTheDocument();

    // 默认仅展开第一个已启用的服务商 Provider A，展开 Provider C
    fireEvent.click(screen.getByTestId("model-pool-source-group-p-c"));

    const rowFor = (model: string) =>
      screen
        .getByRole("checkbox", { name: new RegExp(model) })
        .closest("label") as HTMLElement;

    const disabledLabel = new RegExp(
      i18n.t("aiGatewayModelPoolStatusSourceDisabled"),
    );
    const noKeyLabel = new RegExp(i18n.t("aiGatewayModelPoolStatusNoUsableKey"));
    const coolingLabel = new RegExp(i18n.t("aiGatewayModelPoolStatusCooling"));

    // Current-config state is visible immediately, with no saved pool snapshot.
    expect(
      within(rowFor("local-c1")).queryAllByText(noKeyLabel).length,
    ).toBeGreaterThan(0);
    // The healthy source is healthy and does not inherit the other pool's cooldown.
    expect(
      within(rowFor("local-a1")).queryAllByText(coolingLabel).length,
    ).toBe(0);
    expect(
      within(rowFor("local-a1")).queryAllByText(disabledLabel).length,
    ).toBe(0);

    // 已选列表中保留了先前来自已禁用服务商的候选，并展示“来源已禁用”状态
    const selected = screen.getByTestId("model-pool-selected-list");
    expect(within(selected).getAllByText(/local-b1/i).length).toBeGreaterThan(0);
    expect(
      within(selected).queryAllByText(disabledLabel).length,
    ).toBeGreaterThan(0);
  });

  it("keeps a newly selected draft whose source is removed and shows the missing reason", async () => {
    const configHandlers: Array<(event: unknown) => void> = [];
    (
      listenMock as unknown as {
        mockImplementation: (
          fn: (
            eventName: string,
            handler: (event: unknown) => void,
          ) => Promise<() => void>,
        ) => void;
      }
    ).mockImplementation(
      async (eventName: string, handler: (event: unknown) => void) => {
        if (eventName === AI_GATEWAY_CONFIG_UPDATED_EVENT) {
          configHandlers.push(handler);
        }
        return vi.fn();
      },
    );

    await openModelPoolDialog();

    fireEvent.change(screen.getByTestId("model-pool-name-input"), {
      target: { value: "My Pool" },
    });
    fireEvent.change(screen.getByTestId("model-pool-model-input"), {
      target: { value: "my-pool" },
    });
    fireEvent.click(screen.getByRole("checkbox", { name: /local-a1/i }));
    fireEvent.change(screen.getByRole("spinbutton", { name: /local-a1/i }), {
      target: { value: "7" },
    });

    // The current config drops p-a entirely.
    mockState.config = {
      ...(buildConfig([]) as unknown as Record<string, unknown>),
      providers: [
        {
          id: "p-b",
          name: "Provider B",
          base_url: "https://api.b.example",
          default_model: null,
          protocol: "responses",
          enabled: true,
          keys: [configuredKey("k-b")],
          mappings: [{ local_model: "local-b1", upstream_model: "remote-b1" }],
        },
      ],
    } as unknown as GatewayConfig;

    await act(async () => {
      for (const handler of configHandlers) handler({ payload: undefined });
    });

    // The draft survives the config update.
    expect(screen.getByTestId("model-pool-name-input")).toHaveValue("My Pool");
    expect(screen.getByTestId("model-pool-model-input")).toHaveValue("my-pool");
    expect(screen.getByRole("spinbutton", { name: /local-a1/i })).toHaveValue(7);

    // The removed source stays explicit and its latest missing state is visible.
    const selected = screen.getByTestId("model-pool-selected-list");
    expect(within(selected).getAllByText(/local-a1/i).length).toBeGreaterThan(0);
    expect(
      within(selected).queryAllByText(
        new RegExp(i18n.t("aiGatewayModelPoolStatusMissingProvider")),
      ).length,
    ).toBeGreaterThan(0);
  });

  it("renders a template tag badge for providers created from a template in both source groups and selected candidates", async () => {
    const templates = [
      {
        template: {
          id: "tpl-deepseek",
          name: "DeepSeek Official",
          protocol: "chat_completions",
          models: ["deepseek-chat"],
        },
      },
    ];
    mockState.templates = templates;

    const pool = {
      id: "pool-1",
      name: "Tpl Pool",
      model: "tpl-pool",
      enabled: true,
      candidates: [
        {
          provider_id: "p-a",
          local_model: "local-a1",
          upstream_model: "remote-a1",
          protocol: "chat_completions",
          weight: 1,
        },
      ],
    };

    mockState.config = {
      ...(buildConfig([pool]) as unknown as Record<string, unknown>),
      providers: [
        {
          id: "p-a",
          name: "Provider A",
          template_id: "tpl-deepseek",
          protocol: "chat_completions",
          mappings: [{ local_model: "local-a1", upstream_model: "remote-a1" }],
          enabled: true,
        },
      ],
      model_pools: [pool],
    } as unknown as GatewayConfig;

    renderWithProviders(<AiGateway />);
    await screen.findByText("Provider A");
    openModelPoolsTab();
    await waitFor(() =>
      expect(screen.getByTestId("ai-gateway-model-pool-pool-1")).toBeInTheDocument(),
    );

    fireEvent.click(screen.getByTestId("ai-gateway-model-pool-edit-pool-1"));
    expect(await screen.findByTestId("model-pool-dialog")).toBeInTheDocument();

    // Source group header badge
    const groupBadge = screen.getByTestId("model-pool-source-template-badge-p-a");
    expect(groupBadge).toHaveTextContent("DeepSeek Official");

    // Selected candidate badge
    const selectedBadge = screen.getByTestId(
      "model-pool-selected-template-badge-p-a",
    );
    expect(selectedBadge).toHaveTextContent("DeepSeek Official");
  });

  it("默认仅展开第一个服务商，后续服务商默认折叠", async () => {
    installMock();
    mockState.config = buildConfig([], [
      makeProvider({ id: "p-1", name: "Provider 1" }),
      makeProvider({ id: "p-2", name: "Provider 2" }),
      makeProvider({ id: "p-3", name: "Provider 3" }),
    ]);

    await openModelPoolDialog("Provider 1");

    const group1 = screen.getByTestId("model-pool-source-group-p-1");
    const group2 = screen.getByTestId("model-pool-source-group-p-2");
    const group3 = screen.getByTestId("model-pool-source-group-p-3");

    expect(group1).toHaveAttribute("aria-expanded", "true");
    expect(group2).toHaveAttribute("aria-expanded", "false");
    expect(group3).toHaveAttribute("aria-expanded", "false");
  });

  it("支持根据筛选条件全选与反选", async () => {
    installMock();
    mockState.config = buildConfig([], [
      makeProvider({
        id: "p-1",
        name: "Provider 1",
        mappings: [
          { local_model: "gpt-4o", upstream_model: "gpt-4o" },
          { local_model: "claude-3-5-sonnet", upstream_model: "claude-3-5" },
        ],
      }),
      makeProvider({
        id: "p-2",
        name: "Provider 2",
        mappings: [
          { local_model: "gpt-4o-mini", upstream_model: "gpt-4o-mini" },
          { local_model: "deepseek-chat", upstream_model: "deepseek-v3" },
        ],
      }),
    ]);

    await openModelPoolDialog("Provider 1");

    // 搜索 gpt
    const searchInput = screen.getByTestId("model-pool-source-search");
    fireEvent.change(searchInput, { target: { value: "gpt" } });

    // 当前筛选出的候选模型为 gpt-4o 与 gpt-4o-mini (2 个)
    const selectAllBtn = screen.getByTestId("model-pool-select-all-filtered");
    expect(selectAllBtn).toHaveTextContent("Select all (2)");

    // 点击全选
    fireEvent.click(selectAllBtn);

    const selectedList = screen.getByTestId("model-pool-selected-list");
    expect(within(selectedList).getAllByText("gpt-4o").length).toBeGreaterThanOrEqual(1);
    expect(within(selectedList).getAllByText("gpt-4o-mini").length).toBeGreaterThanOrEqual(1);
    expect(within(selectedList).queryByText("claude-3-5-sonnet")).not.toBeInTheDocument();

    // 此时按钮文案变为 Deselect all (2)
    expect(selectAllBtn).toHaveTextContent("Deselect all (2)");

    // 再次点击反选
    fireEvent.click(selectAllBtn);
    expect(within(selectedList).queryByText("gpt-4o")).not.toBeInTheDocument();
    expect(within(selectedList).queryByText("gpt-4o-mini")).not.toBeInTheDocument();
    expect(selectAllBtn).toHaveTextContent("Select all (2)");
  });

  it("来源模型中仅显示已启用的服务商，已禁用的服务商不予显示", async () => {
    installMock();
    mockState.config = buildConfig([], [
      makeProvider({ id: "p-enabled-1", name: "Enabled Provider 1", enabled: true }),
      makeProvider({ id: "p-disabled", name: "Disabled Provider", enabled: false }),
      makeProvider({ id: "p-enabled-2", name: "Enabled Provider 2", enabled: true }),
    ]);

    await openModelPoolDialog("Enabled Provider 1");

    expect(screen.getByTestId("model-pool-source-group-p-enabled-1")).toBeInTheDocument();
    expect(screen.getByTestId("model-pool-source-group-p-enabled-2")).toBeInTheDocument();
    expect(screen.queryByTestId("model-pool-source-group-p-disabled")).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Step 4 (20261015-pool-only-reasoning) REQ-004 / REQ-001 / REQ-002:
// pool-only reasoning policy editor. REQ-005/REQ-006 capability + coverage
// surfacing, REQ-007 pool-subtree isolation; AC-008 / AC-009 (UI) / AC-011
// (presentation) / AC-012 / AC-014.
//
// Public contract this block fixes for the frontend:
//   - pool default control: a labelled native <select> whose accessible name is
//       t("aiGatewayModelPoolReasoningDefaultLabel"); option values are the six
//       canonical tokens (auto|low|medium|high|xhigh|none).
//   - mapping matrix: per-effort native <select> with
//       data-testid="model-pool-reasoning-mapping-<effort>"; option value "" is
//       t("aiGatewayModelPoolReasoningMappingUnmapped"), otherwise
//       "enum:<value>" | "budget:<tokens>" | "disable".
//   - persisted pool payload gains pool.default_reasoning_effort and
//       candidate.reasoning {profile,values,budget,supports_disable,mappings}.
//
// Direct follow-up (model-pool-bindings-only): the candidate profile /
// declared-values / min-max budget / supports-disable controls and the
// per-effort numeric budget input were REMOVED. Stored native metadata is a
// read-only input for round-trip valid configuration and never an editable
// parameter.
// ---------------------------------------------------------------------------

function reasoningDefaultSelect(): HTMLSelectElement {
  return screen.getByLabelText(
    i18n.t("aiGatewayModelPoolReasoningDefaultLabel"),
  ) as HTMLSelectElement;
}

function mappingSelect(effort: string): HTMLSelectElement {
  return screen.getByTestId(
    `model-pool-reasoning-mapping-${effort}`,
  ) as HTMLSelectElement;
}

function reasoningOf(
  pool: Record<string, unknown>,
  index = 0,
): Record<string, unknown> {
  const candidates = (pool.candidates ?? []) as Array<Record<string, unknown>>;
  return (candidates[index]?.reasoning ?? {}) as Record<string, unknown>;
}

describe("ModelPoolDialog 模型池推理策略（Step 4）", () => {
  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
    mockState.config = buildConfig([]);
    mockState.statusPayload = [];
    mockState.upsertError = null;
    mockState.deleted = [];
    installMock();
  });

  it("新池默认选中 medium，可显式改为 auto 并按池局部默认值保存", async () => {
    const user = userEvent.setup();
    await openModelPoolDialog();

    expect(reasoningDefaultSelect()).toHaveValue("medium");

    await user.selectOptions(reasoningDefaultSelect(), "auto");
    fireEvent.click(screen.getByRole("checkbox", { name: /local-a1/i }));
    fireEvent.click(screen.getByTestId("model-pool-save"));

    await waitFor(() => expect(upsertCalls()).toHaveLength(1));
    expect(upsertCalls()[0].default_reasoning_effort).toBe("auto");
    // A pool save never touches the referenced providers.
    expect(
      invokeMock.mock.calls.some(
        ([command]) => command === "ai_gateway_upsert_provider",
      ),
    ).toBe(false);
  });

  it("编辑遗留池时默认读取 auto，且不推断任何映射", async () => {
    mockState.config = buildConfig([
      {
        id: "pool-legacy",
        name: "Legacy Pool",
        model: "legacy-model",
        enabled: true,
        candidates: [
          {
            provider_id: "p-a",
            local_model: "local-a1",
            upstream_model: "remote-a1",
            protocol: "chat_completions",
            weight: 1,
          },
        ],
      },
    ]);

    renderWithProviders(<AiGateway />);
    await screen.findByText("Provider A");
    openModelPoolsTab();
    fireEvent.click(screen.getByTestId("ai-gateway-model-pool-edit-pool-legacy"));

    expect(await screen.findByTestId("model-pool-dialog")).toBeInTheDocument();
    expect(reasoningDefaultSelect()).toHaveValue("auto");
    // A legacy candidate has an unknown profile: no mapping may be invented.
    expect(mappingSelect("high")).toHaveValue("");
  });

  it("存储的声明只作为只读输入，编辑器只暴露绑定选择", async () => {
    mockState.config = buildConfig([
      {
        id: "pool-declared",
        name: "Declared Pool",
        model: "declared-model",
        enabled: true,
        default_reasoning_effort: "high",
        candidates: [
          {
            provider_id: "p-a",
            local_model: "local-a1",
            upstream_model: "remote-a1",
            protocol: "chat_completions",
            weight: 1,
            reasoning: {
              profile: "openai_effort",
              values: ["low", "medium", "high"],
              supports_disable: false,
              mappings: [
                { effort: "high", action: { kind: "enum", value: "high" } },
              ],
            },
          },
        ],
      },
    ]);

    renderWithProviders(<AiGateway />);
    await screen.findByText("Provider A");
    openModelPoolsTab();
    fireEvent.click(screen.getByTestId("ai-gateway-model-pool-edit-pool-declared"));

    expect(await screen.findByTestId("model-pool-dialog")).toBeInTheDocument();
    const editor = screen.getByTestId("model-pool-reasoning-editor-p-a-local-a1");
    // 高级声明已被移除：profile / 声明取值 / 预算区间 / 禁用复选框都不再存在。
    expect(
      screen.queryByLabelText(i18n.t("aiGatewayModelPoolReasoningProfileLabel")),
    ).toBeNull();
    expect(
      screen.queryByLabelText(i18n.t("aiGatewayModelPoolReasoningValuesLabel")),
    ).toBeNull();
    expect(
      screen.queryByLabelText(
        i18n.t("aiGatewayModelPoolReasoningSupportsDisableLabel"),
      ),
    ).toBeNull();
    expect(editor.querySelector("details")).toBeNull();
    expect(editor.querySelector("input")).toBeNull();

    // 存储的声明仅作为只读输入：绑定选择回显已存的 enum 并仍提供其原生取值。
    const high = mappingSelect("high");
    expect(high).toHaveValue("enum:high");
    expect(within(high).getByRole("option", { name: "high" })).toBeInTheDocument();
    // The five canonical preferences each expose an explicit unmapped option.
    for (const effort of ["low", "medium", "high", "xhigh", "none"]) {
      const select = mappingSelect(effort);
      expect(
        within(select).getByRole("option", {
          name: i18n.t("aiGatewayModelPoolReasoningMappingUnmapped"),
        }),
      ).toBeInTheDocument();
    }
    expect(mappingSelect("low")).toHaveValue("");
  });

  it("编辑遗留/未知 profile 的候选时不推断映射也不暴露声明控件", async () => {
    mockState.config = buildConfig([
      {
        id: "pool-unknown",
        name: "Unknown Pool",
        model: "unknown-model",
        enabled: true,
        default_reasoning_effort: "auto",
        candidates: [
          {
            provider_id: "p-a",
            local_model: "local-a1",
            upstream_model: "remote-a1",
            protocol: "chat_completions",
            weight: 1,
            reasoning: {
              profile: "legacy_unknown_profile",
              values: [],
              supports_disable: false,
              mappings: [],
            },
          },
        ],
      },
    ]);

    renderWithProviders(<AiGateway />);
    await screen.findByText("Provider A");
    openModelPoolsTab();
    fireEvent.click(screen.getByTestId("ai-gateway-model-pool-edit-pool-unknown"));

    expect(await screen.findByTestId("model-pool-dialog")).toBeInTheDocument();
    const editor = screen.getByTestId("model-pool-reasoning-editor-p-a-local-a1");
    // 未知 profile 不再作为表单暴露，DOM 中不存在任何声明控件。
    expect(
      screen.queryByLabelText(i18n.t("aiGatewayModelPoolReasoningProfileLabel")),
    ).toBeNull();
    expect(
      screen.queryByLabelText(i18n.t("aiGatewayModelPoolReasoningValuesLabel")),
    ).toBeNull();
    expect(editor.querySelector("input")).toBeNull();
    // No declared value is fabricated from the unknown profile, no mapping inferred.
    expect(mappingSelect("high")).toHaveValue("");
  });

  it("多档位共享同一原生动作可往返保存；switch 配置说明其没有强度梯度", async () => {
    const sharedAction = { kind: "enum", value: "enabled" };
    mockState.config = buildConfig([
      {
        id: "pool-switch",
        name: "Switch Pool",
        model: "switch-model",
        enabled: true,
        default_reasoning_effort: "medium",
        candidates: [
          {
            provider_id: "p-a",
            local_model: "local-a1",
            upstream_model: "remote-a1",
            protocol: "chat_completions",
            weight: 1,
            reasoning: {
              profile: "chat_thinking_switch",
              values: [],
              supports_disable: true,
              mappings: [
                { effort: "low", action: { ...sharedAction } },
                { effort: "medium", action: { ...sharedAction } },
              ],
            },
          },
        ],
      },
    ]);

    renderWithProviders(<AiGateway />);
    await screen.findByText("Provider A");
    openModelPoolsTab();
    fireEvent.click(screen.getByTestId("ai-gateway-model-pool-edit-pool-switch"));

    expect(await screen.findByTestId("model-pool-dialog")).toBeInTheDocument();
    expect(
      screen.getByText(i18n.t("aiGatewayModelPoolReasoningSwitchNoGradient")),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByTestId("model-pool-save"));

    await waitFor(() => expect(upsertCalls()).toHaveLength(1));
    const mappings = reasoningOf(upsertCalls()[0]).mappings as Array<
      Record<string, unknown>
    >;
    const low = mappings.find((row) => row.effort === "low");
    const medium = mappings.find((row) => row.effort === "medium");
    expect(low?.action).toEqual(sharedAction);
    expect(medium?.action).toEqual(sharedAction);
  });

  it("同一次保存删除默认档最后一条映射被原子拒绝并保留草稿", async () => {
    const user = userEvent.setup();
    mockState.config = buildConfig([
      {
        id: "pool-guard",
        name: "Guard Pool",
        model: "guard-model",
        enabled: true,
        default_reasoning_effort: "medium",
        candidates: [
          {
            provider_id: "p-a",
            local_model: "local-a1",
            upstream_model: "remote-a1",
            protocol: "chat_completions",
            weight: 1,
            reasoning: {
              profile: "openai_effort",
              values: ["medium"],
              supports_disable: false,
              mappings: [
                { effort: "medium", action: { kind: "enum", value: "medium" } },
              ],
            },
          },
        ],
      },
    ]);

    renderWithProviders(<AiGateway />);
    await screen.findByText("Provider A");
    openModelPoolsTab();
    fireEvent.click(screen.getByTestId("ai-gateway-model-pool-edit-pool-guard"));

    expect(await screen.findByTestId("model-pool-dialog")).toBeInTheDocument();
    await user.selectOptions(mappingSelect("medium"), "");
    fireEvent.click(screen.getByTestId("model-pool-save"));

    expect(await screen.findByTestId("model-pool-error")).toBeInTheDocument();
    expect(upsertCalls()).toHaveLength(0);
    // The rejected draft is preserved: default stays medium and the mapping row
    // keeps the operator's explicit unmapped selection.
    expect(reasoningDefaultSelect()).toHaveValue("medium");
    expect(mappingSelect("medium")).toHaveValue("");
  });

  it("保存失败与运行时刷新都保留推理草稿", async () => {
    const user = userEvent.setup();
    const configHandlers: Array<(event: unknown) => void> = [];
    (
      listenMock as unknown as {
        mockImplementation: (
          fn: (
            eventName: string,
            handler: (event: unknown) => void,
          ) => Promise<() => void>,
        ) => void;
      }
    ).mockImplementation(
      async (eventName: string, handler: (event: unknown) => void) => {
        if (eventName === AI_GATEWAY_CONFIG_UPDATED_EVENT) {
          configHandlers.push(handler);
        }
        return vi.fn();
      },
    );

    await openModelPoolDialog();
    await user.selectOptions(reasoningDefaultSelect(), "high");

    // A runtime config refresh must not overwrite the unsaved reasoning draft.
    mockState.config = buildConfig([]);
    await act(async () => {
      for (const handler of configHandlers) handler({ payload: undefined });
    });
    expect(reasoningDefaultSelect()).toHaveValue("high");

    // A rejected save keeps the same reasoning draft visible.
    mockState.upsertError = "invalid model pool reasoning policy";
    fireEvent.click(screen.getByTestId("model-pool-save"));
    expect(await screen.findByTestId("model-pool-error")).toHaveTextContent(
      /invalid model pool reasoning policy/i,
    );
    expect(reasoningDefaultSelect()).toHaveValue("high");
    expect(upsertCalls()).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// Step 4 repair (20261015-pool-only-reasoning) REQ-001 / REQ-002 / REQ-004 /
// AC-002 / AC-004 / AC-008:
//   (A) each manual-budget mapping keeps its own native `budget_tokens`, and the
//       existing distinct stored budgets are the only rebind targets exposed as
//       read-only options; no numeric budget/range input exists;
//   (D) the dialog shows a live static coverage summary separated from runtime
//       readiness, a new/changed concrete default needs a currently resolvable
//       mapping, while an unchanged default + declaration stays retainable;
//   (E) binding/action option labels are localized in both locales while their
//       option values stay the stable product tokens.
//
// Public interfaces pinned for Frontend:
//   - candidate editor: data-testid="model-pool-reasoning-editor-<providerId>-<localModel>"
//       holding the binding selects and NO advanced declaration surface.
//   - live static coverage value: data-testid="model-pool-reasoning-static-coverage"
//       (label text keeps aiGatewayModelPoolReasoningCoverageLabel).
//   - live runtime readiness value: data-testid="model-pool-reasoning-runtime-readiness"
//       (label text keeps aiGatewayModelPoolReasoningRuntimeReadinessLabel).
//   - a stored manual-budget action is a read-only option `budget:<tokens>`
//       labelled `${t("aiGatewayModelPoolReasoningActionBudget")} · <tokens>`.
// ---------------------------------------------------------------------------

type PoolCandidatePayload = {
  effort: string;
  action: Record<string, unknown>;
};

describe("ModelPoolDialog 模型池推理策略回归（Step 4 repair）", () => {
  const ANTHROPIC_PROVIDERS = [
    {
      id: "p-a",
      name: "Provider A",
      protocol: "anthropic_messages",
      enabled: true,
      mappings: [
        {
          local_model: "local-a1",
          upstream_model: "remote-a1",
          protocol: "anthropic_messages",
        },
      ],
    },
    {
      id: "p-b",
      name: "Provider B",
      protocol: "chat_completions",
      enabled: true,
      mappings: [{ local_model: "local-b1", upstream_model: "remote-b1" }],
    },
  ] as unknown as ProviderFixture[];

  const manualBudgetPool: Record<string, unknown> = {
    id: "pool-manual-budget",
    name: "Manual Budget Pool",
    model: "manual-budget-model",
    enabled: true,
    default_reasoning_effort: "high",
    candidates: [
      {
        provider_id: "p-a",
        local_model: "local-a1",
        upstream_model: "remote-a1",
        protocol: "anthropic_messages",
        weight: 1,
        reasoning: {
          profile: "anthropic_manual_budget",
          values: [],
          budget: { min: 1024, max: 8192 },
          supports_disable: false,
          mappings: [
            { effort: "low", action: { kind: "budget", budget_tokens: 1536 } },
            { effort: "high", action: { kind: "budget", budget_tokens: 3072 } },
          ],
        },
      },
    ],
  };

  const liveCoveragePool: Record<string, unknown> = {
    id: "pool-live-coverage",
    name: "Live Coverage Pool",
    model: "live-coverage-model",
    enabled: true,
    default_reasoning_effort: "high",
    candidates: [
      {
        provider_id: "p-a",
        local_model: "local-a1",
        upstream_model: "remote-a1",
        protocol: "chat_completions",
        weight: 1,
        reasoning: {
          profile: "openai_effort",
          values: ["high"],
          supports_disable: false,
          mappings: [
            { effort: "high", action: { kind: "enum", value: "high" } },
          ],
        },
      },
      {
        provider_id: "p-b",
        local_model: "local-b1",
        upstream_model: "remote-b1",
        protocol: "responses",
        weight: 1,
      },
    ],
  };

  const danglingDeclarationPool: Record<string, unknown> = {
    id: "pool-dangling",
    name: "Dangling Pool",
    model: "dangling-model",
    enabled: true,
    default_reasoning_effort: "auto",
    candidates: [
      {
        provider_id: "p-gone",
        local_model: "gone",
        upstream_model: "gone-remote",
        protocol: "chat_completions",
        weight: 1,
        reasoning: {
          profile: "openai_effort",
          values: ["high"],
          supports_disable: false,
          mappings: [
            { effort: "high", action: { kind: "enum", value: "high" } },
          ],
        },
      },
    ],
  };

  async function openForEdit(
    pool: Record<string, unknown>,
    providers: unknown[] = PROVIDERS,
  ) {
    mockState.config = buildConfig([pool], providers);
    renderWithProviders(<AiGateway />);
    await screen.findByText("Provider A");
    openModelPoolsTab();
    fireEvent.click(screen.getByTestId(`ai-gateway-model-pool-edit-${pool.id}`));
    await screen.findByTestId("model-pool-dialog");
  }

  function editorFor(providerId: string, localModel: string): HTMLElement {
    return screen.getByTestId(
      `model-pool-reasoning-editor-${providerId}-${localModel}`,
    );
  }

  function expandEditor(editor: HTMLElement): void {
    fireEvent.click(within(editor).getAllByRole("button")[0]);
  }

  function optionByValue(
    select: HTMLSelectElement,
    value: string,
  ): HTMLOptionElement | undefined {
    return Array.from(select.options).find((option) => option.value === value);
  }

  function mappingAction(
    pool: Record<string, unknown>,
    effort: string,
  ): Record<string, unknown> | undefined {
    const mappings = (reasoningOf(pool).mappings ?? []) as PoolCandidatePayload[];
    return mappings.find((row) => row.effort === effort)?.action;
  }

  function coverageCopy(mapped: number, total: number): string {
    return i18n.t("aiGatewayModelPoolReasoningCoverageValue", { mapped, total });
  }

  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
    mockState.config = buildConfig([]);
    mockState.statusPayload = [];
    mockState.upsertError = null;
    mockState.deleted = [];
    installMock();
  });

  it.each(["en", "zh"] as const)(
    "%s 下已存手动预算只读展示：含 token 数的本地化选项且无数字输入",
    async (language) => {
      await i18n.changeLanguage(language);
      await openForEdit(manualBudgetPool, ANTHROPIC_PROVIDERS);

      const editor = editorFor("p-a", "local-a1");
      expandEditor(editor);
      const prefix = i18n.t("aiGatewayModelPoolReasoningActionBudget");

      for (const [effort, tokens] of [
        ["low", 1536],
        ["high", 3072],
      ] as const) {
        const select = within(editor).getByTestId(
          `model-pool-reasoning-mapping-${effort}`,
        ) as HTMLSelectElement;
        expect(select).toHaveValue(`budget:${tokens}`);
        expect(optionByValue(select, `budget:${tokens}`)?.textContent).toBe(
          `${prefix} · ${tokens}`,
        );
      }

      // No per-effort numeric budget input, no advanced declaration controls.
      expect(within(editor).queryByRole("spinbutton")).toBeNull();
      expect(editor.querySelector("input")).toBeNull();
      expect(editor.querySelector("details")).toBeNull();
      expect(
        screen.queryByLabelText(
          i18n.t("aiGatewayModelPoolReasoningSupportsDisableLabel"),
        ),
      ).toBeNull();
    },
  );

  it("把 high 重绑定到已存预算 budget:1536 并保存为类型化动作", async () => {
    const user = userEvent.setup();
    await openForEdit(manualBudgetPool, ANTHROPIC_PROVIDERS);

    const editor = editorFor("p-a", "local-a1");
    expandEditor(editor);
    const high = within(editor).getByTestId(
      "model-pool-reasoning-mapping-high",
    ) as HTMLSelectElement;
    await user.selectOptions(high, "budget:1536");
    fireEvent.click(screen.getByTestId("model-pool-save"));

    await waitFor(() => expect(upsertCalls()).toHaveLength(1));
    const saved = upsertCalls()[0];
    expect(saved.default_reasoning_effort).toBe("high");
    // Exactly the chosen existing budget is emitted for high; low is untouched.
    expect(mappingAction(saved, "low")).toEqual({
      kind: "budget",
      budget_tokens: 1536,
    });
    expect(mappingAction(saved, "high")).toEqual({
      kind: "budget",
      budget_tokens: 1536,
    });
    // The manual profile and its declared range survive unchanged.
    expect(reasoningOf(saved).profile).toBe("anthropic_manual_budget");
    expect(reasoningOf(saved).budget).toEqual({ min: 1024, max: 8192 });
    expect(reasoningOf(saved).values).toEqual([]);
  });

  it("取消再重新勾选同一来源仍保留已存 manual-budget 元数据与预算档位", async () => {
    await openForEdit(manualBudgetPool, ANTHROPIC_PROVIDERS);

    // Uncheck the saved candidate in the source list, then check the very same
    // source again: the immutable stored base must survive the round-trip
    // instead of being replaced by the bare source candidate.
    const sourceCheckbox = screen.getByRole("checkbox", {
      name: "Provider A local-a1 remote-a1",
    });
    fireEvent.click(sourceCheckbox);
    expect(
      screen.queryByTestId("model-pool-reasoning-editor-p-a-local-a1"),
    ).toBeNull();
    fireEvent.click(sourceCheckbox);

    const editor = editorFor("p-a", "local-a1");
    expandEditor(editor);

    // No advanced declaration or numeric budget surface returns.
    expect(within(editor).queryByRole("spinbutton")).toBeNull();
    expect(editor.querySelector("input")).toBeNull();
    expect(editor.querySelector("details")).toBeNull();
    expect(
      screen.queryByLabelText(
        i18n.t("aiGatewayModelPoolReasoningSupportsDisableLabel"),
      ),
    ).toBeNull();

    for (const [effort, tokens] of [
      ["low", 1536],
      ["high", 3072],
    ] as const) {
      const select = within(editor).getByTestId(
        `model-pool-reasoning-mapping-${effort}`,
      ) as HTMLSelectElement;
      expect(select).toHaveValue(`budget:${tokens}`);
    }

    fireEvent.click(screen.getByTestId("model-pool-save"));
    await waitFor(() => expect(upsertCalls()).toHaveLength(1));

    const saved = upsertCalls()[0];
    expect(saved.default_reasoning_effort).toBe("high");
    expect(reasoningOf(saved).profile).toBe("anthropic_manual_budget");
    expect(reasoningOf(saved).budget).toEqual({ min: 1024, max: 8192 });
    expect(reasoningOf(saved).values).toEqual([]);
    expect(mappingAction(saved, "low")).toEqual({
      kind: "budget",
      budget_tokens: 1536,
    });
    expect(mappingAction(saved, "high")).toEqual({
      kind: "budget",
      budget_tokens: 3072,
    });
  });

  it("草稿态静态覆盖随绑定实时更新", async () => {
    const user = userEvent.setup();
    // Both exact source mappings declare native `high`, so binding the
    // untouched candidate's high preference is a plain binding with no manual
    // profile/value editing.
    const providers = [
      {
        id: "p-a",
        name: "Provider A",
        protocol: "chat_completions",
        enabled: true,
        mappings: [
          {
            local_model: "local-a1",
            upstream_model: "remote-a1",
            reasoning_efforts: ["high"],
          },
        ],
      },
      {
        id: "p-b",
        name: "Provider B",
        protocol: "responses",
        enabled: true,
        mappings: [
          {
            local_model: "local-b1",
            upstream_model: "remote-b1",
            reasoning_efforts: ["high"],
          },
        ],
      },
    ];
    await openForEdit(liveCoveragePool, providers);

    expect(
      screen.getByTestId("model-pool-reasoning-static-coverage"),
    ).toHaveTextContent(coverageCopy(1, 2));

    const editor = editorFor("p-b", "local-b1");
    expandEditor(editor);
    const high = within(editor).getByTestId(
      "model-pool-reasoning-mapping-high",
    ) as HTMLSelectElement;
    await user.selectOptions(
      high,
      within(high).getByRole("option", { name: "high" }),
    );

    expect(
      screen.getByTestId("model-pool-reasoning-static-coverage"),
    ).toHaveTextContent(coverageCopy(2, 2));
  });

  it("静态覆盖不读取密钥状态，运行时就绪单独展示", async () => {
    const degradedProviders = [
      {
        id: "p-a",
        name: "Provider A",
        protocol: "chat_completions",
        enabled: true,
        keys: [
          {
            id: "k1",
            name: "Default",
            value: "********",
            enabled: false,
            auto_marked: true,
            failure_kind: "quota",
            marked_at: 1,
            reason: "quota",
          },
        ],
        mappings: [{ local_model: "local-a1", upstream_model: "remote-a1" }],
      },
      PROVIDERS[1],
    ];

    await openForEdit(liveCoveragePool, degradedProviders);

    expect(
      screen.getByTestId("model-pool-reasoning-static-coverage"),
    ).toHaveTextContent(coverageCopy(1, 2));
    expect(
      screen.getByTestId("model-pool-reasoning-runtime-readiness"),
    ).toHaveTextContent(
      i18n.t("aiGatewayModelPoolReasoningRuntimeReadyCount", {
        ready: 1,
        total: 2,
      }),
    );
  });

  it("把默认改为只有失效来源的档位时在保存前拒绝", async () => {
    const user = userEvent.setup();
    await openForEdit(danglingDeclarationPool);

    await user.selectOptions(reasoningDefaultSelect(), "high");
    fireEvent.click(screen.getByTestId("model-pool-save"));

    expect(await screen.findByTestId("model-pool-error")).toBeInTheDocument();
    expect(upsertCalls()).toHaveLength(0);
    expect(reasoningDefaultSelect()).toHaveValue("high");
  });

  it("默认与声明都未变时，失效来源可在无关编辑中保留", async () => {
    const user = userEvent.setup();
    const pool = { ...danglingDeclarationPool, default_reasoning_effort: "high" };
    await openForEdit(pool);

    const nameInput = screen.getByTestId("model-pool-name-input");
    await user.clear(nameInput);
    await user.type(nameInput, "Renamed Dangling Pool");
    fireEvent.click(screen.getByTestId("model-pool-save"));

    await waitFor(() => expect(upsertCalls()).toHaveLength(1));
    const saved = upsertCalls()[0];
    expect(saved.default_reasoning_effort).toBe("high");
    const candidates = saved.candidates as Array<Record<string, unknown>>;
    expect(candidates[0].reasoning).toEqual(
      (danglingDeclarationPool.candidates as Array<Record<string, unknown>>)[0]
        .reasoning,
    );
  });

  it("manual-budget 绑定只提供已存的预算动作，不把来源原生档位当作 enum 也不能输入新预算", async () => {
    // The exact source mapping declares native ['low','high'], which a
    // manual-budget profile can never target as an enum: the binding offers only
    // the existing distinct stored budgets and never enum:low/high.
    const providers = [
      {
        id: "p-a",
        name: "Provider A",
        protocol: "anthropic_messages",
        enabled: true,
        mappings: [
          {
            local_model: "local-a1",
            upstream_model: "remote-a1",
            protocol: "anthropic_messages",
            reasoning_efforts: ["low", "high"],
          },
        ],
      },
    ];
    await openForEdit(manualBudgetPool, providers);

    const editor = editorFor("p-a", "local-a1");
    expandEditor(editor);

    const high = within(editor).getByTestId(
      "model-pool-reasoning-mapping-high",
    ) as HTMLSelectElement;
    const optionValues = Array.from(high.options).map((option) => option.value);
    expect(optionValues.some((value) => value.startsWith("enum:"))).toBe(false);
    expect(optionValues).toEqual(
      expect.arrayContaining(["budget:1536", "budget:3072"]),
    );
    // Only existing stored budget actions are offered: no numeric/typing surface.
    expect(within(editor).queryByRole("spinbutton")).toBeNull();
    expect(editor.querySelector("input")).toBeNull();
  });

  it("chat-thinking-switch 绑定只提供内建 enabled，不把来源原生档位当作 enum", async () => {
    const user = userEvent.setup();
    // The source declares native ['medium','high'], but a switch profile's only
    // valid positive native value is `enabled`; the incompatible unioned
    // metadata must be filtered and the intrinsic `enabled` offered instead.
    const providers = [
      {
        id: "p-a",
        name: "Provider A",
        protocol: "chat_completions",
        enabled: true,
        mappings: [
          {
            local_model: "local-a1",
            upstream_model: "remote-a1",
            reasoning_efforts: ["medium", "high"],
          },
        ],
      },
    ];
    const switchPool: Record<string, unknown> = {
      id: "pool-switch-bind",
      name: "Switch Bind Pool",
      model: "switch-bind-model",
      enabled: true,
      default_reasoning_effort: "high",
      candidates: [
        {
          provider_id: "p-a",
          local_model: "local-a1",
          upstream_model: "remote-a1",
          protocol: "chat_completions",
          weight: 1,
          reasoning: {
            profile: "chat_thinking_switch",
            values: [],
            supports_disable: false,
            mappings: [],
          },
        },
      ],
    };
    await openForEdit(switchPool, providers);

    const editor = screen.getByTestId("model-pool-reasoning-editor-p-a-local-a1");
    fireEvent.click(within(editor).getAllByRole("button")[0]);

    const high = within(editor).getByTestId(
      "model-pool-reasoning-mapping-high",
    ) as HTMLSelectElement;
    const optionValues = Array.from(high.options).map((option) => option.value);
    expect(optionValues).toContain("enum:enabled");
    expect(optionValues).not.toContain("enum:medium");
    expect(optionValues).not.toContain("enum:high");

    await user.selectOptions(high, "enum:enabled");
    fireEvent.click(screen.getByTestId("model-pool-save"));

    await waitFor(() => expect(upsertCalls()).toHaveLength(1));
    const saved = upsertCalls()[0];
    expect(saved.default_reasoning_effort).toBe("high");
    expect(reasoningOf(saved).profile).toBe("chat_thinking_switch");
    expect(mappingAction(saved, "high")).toEqual({
      kind: "enum",
      value: "enabled",
    });
  });

  it("支持在已选列表中切换候选模型推理配置的折叠与展开状态（默认收起）", async () => {
    await openModelPoolDialog();

    // 勾选一个候选模型加入已选列表
    fireEvent.click(screen.getByRole("checkbox", { name: /local-a1/i }));

    const selectedList = screen.getByTestId("model-pool-selected-list");
    expect(within(selectedList).getByText(/local-a1/i)).toBeInTheDocument();

    const editor = screen.getByTestId("model-pool-reasoning-editor-p-a-local-a1");
    const body = within(editor).getByTestId("model-pool-reasoning-body");

    // 默认是收起状态，带有 hidden 样式类
    expect(body).toHaveClass("hidden");

    // 点击展开按钮：按外层切换条的可访问名称定位，避免新增的 <summary>
    // 披露控件使旧的结构化单按钮选择失效（导航适配，不弱化折叠断言）。
    const toggleBtn = within(editor).getByRole("button", {
      name: new RegExp(`^${i18n.t("aiGatewayModelPoolReasoningBindingsLabel")}`),
    });
    fireEvent.click(toggleBtn);

    // 展开后移除 hidden 样式类
    expect(body).not.toHaveClass("hidden");

    // 再次点击折叠按钮
    fireEvent.click(toggleBtn);
    expect(body).toHaveClass("hidden");
  });
});

// ---------------------------------------------------------------------------
// Direct change (model-pool-reasoning-bindings): the main candidate editor
// binds each model-supported native effort to the five canonical pool
// preferences and derives a pool-local native declaration from the chosen
// bindings only. The former advanced profile/values/budget surface was removed
// entirely by the model-pool-bindings-only follow-up.
//
// Public contract pinned for Frontend:
//   - per-candidate panel:  data-testid="model-pool-reasoning-editor-<providerId>-<localModel>"
//   - binding selects:      data-testid="model-pool-reasoning-mapping-<effort>"
//       inside the panel and the ONLY controls it contains
//   - no advanced surface:  data-testid="model-pool-reasoning-advanced" must not
//       exist, and the panel contains no <details>/<summary>/<input>
//   - binding option labels are plain native values; the stable option values
//       may keep the `enum:<value>` spelling.
//   - choosing a binding builds candidate.reasoning {profile,values,mappings};
//       an untouched candidate stays without a `reasoning` key.
// ---------------------------------------------------------------------------

describe("ModelPoolDialog 模型池推理绑定（binding）", () => {
  const PROTOCOL_CASES = [
    { protocol: "chat_completions", expectedProfile: "openai_effort" },
    { protocol: "responses", expectedProfile: "openai_effort" },
    { protocol: "anthropic_messages", expectedProfile: "anthropic_effort_inherit" },
  ] as const;

  function bindingProviders(protocol: string): unknown[] {
    return [
      {
        id: "p-a",
        name: "Provider A",
        protocol,
        enabled: true,
        mappings: [
          {
            local_model: "local-a1",
            upstream_model: "remote-a1",
            protocol,
            reasoning_efforts: ["minimal", "max", "none"],
          },
        ],
      },
      {
        id: "p-b",
        name: "Provider B",
        protocol: "responses",
        enabled: true,
        mappings: [
          {
            local_model: "local-b1",
            upstream_model: "remote-b1",
            reasoning_efforts: ["sibling_only"],
          },
        ],
      },
    ];
  }

  function editorFor(providerId: string, localModel: string): HTMLElement {
    return screen.getByTestId(
      `model-pool-reasoning-editor-${providerId}-${localModel}`,
    );
  }

  function expandPanel(editor: HTMLElement): void {
    fireEvent.click(within(editor).getAllByRole("button")[0]);
  }

  function bindingSelect(editor: HTMLElement, effort: string): HTMLSelectElement {
    return within(editor).getByTestId(
      `model-pool-reasoning-mapping-${effort}`,
    ) as HTMLSelectElement;
  }

  function optionTexts(select: HTMLSelectElement): string[] {
    return Array.from(select.options).map((option) => option.textContent ?? "");
  }

  function offersDisable(select: HTMLSelectElement): boolean {
    return Array.from(select.options).some(
      (option) => option.value === "disable",
    );
  }

  function candidateReasoning(
    pool: Record<string, unknown>,
    localModel: string,
  ): Record<string, unknown> | undefined {
    const candidate = candidatesOf(pool).find(
      (item) => item.local_model === localModel,
    );
    return candidate?.reasoning as Record<string, unknown> | undefined;
  }

  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
    mockState.config = buildConfig([]);
    mockState.statusPayload = [];
    mockState.upsertError = null;
    mockState.deleted = [];
    installMock();
  });

  it.each(PROTOCOL_CASES)(
    "把 canonical medium 绑定到原生 max 并用候选协议默认适配器保存（%s）",
    async ({ protocol, expectedProfile }) => {
      const user = userEvent.setup();
      mockState.config = buildConfig([], bindingProviders(protocol));
      await openModelPoolDialog();

      fireEvent.click(screen.getByRole("checkbox", { name: /local-a1/i }));
      // Provider B is the second enabled provider and starts collapsed.
      fireEvent.click(screen.getByTestId("model-pool-source-group-p-b"));
      fireEvent.click(screen.getByRole("checkbox", { name: /local-b1/i }));

      const editor = editorFor("p-a", "local-a1");
      expandPanel(editor);

      for (const effort of ["low", "medium", "high", "xhigh", "max", "none"]) {
        expect(bindingSelect(editor, effort)).toBeInTheDocument();
      }

      const medium = bindingSelect(editor, "medium");
      // Native options are the exact source-declared values as plain labels,
      // never the `enum:` implementation text.
      expect(optionTexts(medium)).toEqual(
        expect.arrayContaining(["minimal", "max"]),
      );
      for (const text of optionTexts(medium)) {
        expect(text).not.toContain("enum:");
      }
      // No cross-sibling borrowing: Provider B's native option never appears.
      expect(
        within(medium).queryByRole("option", { name: "sibling_only" }),
      ).toBeNull();
      // A positive preference never offers the explicit disable action.
      expect(offersDisable(medium)).toBe(false);

      // The former advanced declaration surface is physically absent: no
      // <details>/<summary>, no advanced testid and no <input> anywhere in the
      // candidate editor. The binding selects are the only controls.
      expect(
        within(editor).queryByTestId("model-pool-reasoning-advanced"),
      ).toBeNull();
      expect(editor.querySelector("details")).toBeNull();
      expect(editor.querySelector("summary")).toBeNull();
      expect(editor.querySelector("input")).toBeNull();

      // `none` offers the explicit disable only where the source declares it
      // supported (Provider A declares `none`; Provider B does not).
      expect(offersDisable(bindingSelect(editor, "none"))).toBe(true);
      const siblingEditor = editorFor("p-b", "local-b1");
      expandPanel(siblingEditor);
      expect(offersDisable(bindingSelect(siblingEditor, "none"))).toBe(false);

      // Bind canonical medium -> native max without entering profile/values/range.
      await user.selectOptions(
        medium,
        within(medium).getByRole("option", { name: "max" }),
      );

      fireEvent.click(screen.getByTestId("model-pool-save"));
      await waitFor(() => expect(upsertCalls()).toHaveLength(1));

      const pool = upsertCalls()[0];
      const bound = candidateReasoning(pool, "local-a1");
      // The default adapter follows the candidate protocol.
      expect(bound?.profile).toBe(expectedProfile);
      // The declaration is derived: exactly the bound native value, and exactly
      // one mapping with no implicit mapping for any other canonical effort.
      expect(bound?.values).toEqual(["max"]);
      expect(bound?.mappings).toEqual([
        { effort: "medium", action: { kind: "enum", value: "max" } },
        { effort: "max", action: { kind: "enum", value: "max" } },
        { effort: "none", action: { kind: "disable" } },
      ]);
      // The untouched candidate stays unknown: no `reasoning` key is emitted.
      expect(candidateReasoning(pool, "local-b1")).toBeUndefined();
      // The source provider list is never rewritten by a pool save.
      expect(
        invokeMock.mock.calls.some(
          ([command]) => command === "ai_gateway_upsert_provider",
        ),
      ).toBe(false);
    },
  );

  it("绑定改到另一个原生档位时，未在元数据中的已存值仍保留", async () => {
    const user = userEvent.setup();
    const bindProviders = [
      {
        id: "p-a",
        name: "Provider A",
        protocol: "chat_completions",
        enabled: true,
        mappings: [
          {
            local_model: "local-a1",
            upstream_model: "remote-a1",
            reasoning_efforts: ["minimal", "max"],
          },
        ],
      },
    ];
    const pool: Record<string, unknown> = {
      id: "pool-bind-edit",
      name: "Bind Edit Pool",
      model: "bind-edit",
      enabled: true,
      default_reasoning_effort: "medium",
      candidates: [
        {
          provider_id: "p-a",
          local_model: "local-a1",
          upstream_model: "remote-a1",
          protocol: "chat_completions",
          weight: 1,
          reasoning: {
            profile: "openai_effort",
            values: ["legacy_custom"],
            supports_disable: false,
            mappings: [
              {
                effort: "medium",
                action: { kind: "enum", value: "legacy_custom" },
              },
            ],
          },
        },
      ],
    };
    mockState.config = buildConfig([pool], bindProviders);
    renderWithProviders(<AiGateway />);
    await screen.findByText("Provider A");
    openModelPoolsTab();
    fireEvent.click(
      screen.getByTestId("ai-gateway-model-pool-edit-pool-bind-edit"),
    );
    await screen.findByTestId("model-pool-dialog");

    const editor = editorFor("p-a", "local-a1");
    expandPanel(editor);
    const medium = bindingSelect(editor, "medium");
    // A stored declaration value absent from the source metadata stays offered.
    expect(
      within(medium).getByRole("option", { name: "legacy_custom" }),
    ).toBeInTheDocument();
    expect(
      within(medium).getByRole("option", { name: "minimal" }),
    ).toBeInTheDocument();

    await user.selectOptions(
      medium,
      within(medium).getByRole("option", { name: "minimal" }),
    );
    fireEvent.click(screen.getByTestId("model-pool-save"));

    await waitFor(() => expect(upsertCalls()).toHaveLength(1));
    const saved = candidateReasoning(upsertCalls()[0], "local-a1");
    expect(saved?.profile).toBe("openai_effort");
    expect(saved?.mappings).toEqual([
      { effort: "medium", action: { kind: "enum", value: "minimal" } },
    ]);
    const values = (saved?.values ?? []) as string[];
    expect(values).toHaveLength(2);
    expect(values).toEqual(expect.arrayContaining(["minimal", "legacy_custom"]));
  });

  it("把 canonical none 绑定到源声明的 none 时保存为显式 disable", async () => {
    const user = userEvent.setup();
    mockState.config = buildConfig([], bindingProviders("chat_completions"));
    await openModelPoolDialog();

    // Isolate the `none` binding: an `auto` default needs no positive mapping.
    await user.selectOptions(reasoningDefaultSelect(), "auto");
    fireEvent.click(screen.getByRole("checkbox", { name: /local-a1/i }));
    const editor = editorFor("p-a", "local-a1");
    expandPanel(editor);

    const none = bindingSelect(editor, "none");
    // The exact source mapping declares `none`, so the explicit disable is
    // offered without any advanced declaration.
    expect(offersDisable(none)).toBe(true);
    await user.selectOptions(none, "disable");

    fireEvent.click(screen.getByTestId("model-pool-save"));
    await waitFor(() => expect(upsertCalls()).toHaveLength(1));

    const pool = upsertCalls()[0];
    expect(pool.default_reasoning_effort).toBe("auto");
    const bound = candidateReasoning(pool, "local-a1");
    expect(bound?.profile).toBe("openai_effort");
    expect(bound?.supports_disable).toBe(true);
    expect(bound?.mappings).toEqual([
      { effort: "max", action: { kind: "enum", value: "max" } },
      { effort: "none", action: { kind: "disable" } },
    ]);
  });
});

// ---------------------------------------------------------------------------
// Direct follow-up (model-pool-bindings-only): after expanding a fresh candidate
// there is NO advanced declaration/details/summary/profileSelect/textbox/numeric
// budget/disable checkbox anywhere in the candidate editor; the five canonical
// binding selects are the only controls and save exact enum/none metadata.
// ---------------------------------------------------------------------------

describe("ModelPoolDialog 模型池推理绑定回归（无高级声明界面）", () => {
  function providerWithNative(protocol: string): unknown[] {
    return [
      {
        id: "p-a",
        name: "Provider A",
        protocol,
        enabled: true,
        mappings: [
          {
            local_model: "local-a1",
            upstream_model: "remote-a1",
            protocol,
            reasoning_efforts: ["minimal", "max", "none"],
          },
        ],
      },
    ];
  }

  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
    mockState.config = buildConfig([]);
    mockState.statusPayload = [];
    mockState.upsertError = null;
    mockState.deleted = [];
    installMock();
  });

  it.each(["en", "zh"] as const)(
    "%s 下展开全新候选后不存在任何高级声明控件，五个绑定选择保存精确枚举/禁用元数据",
    async (language) => {
      const user = userEvent.setup();
      await i18n.changeLanguage(language);
      mockState.config = buildConfig([], providerWithNative("chat_completions"));
      // The console entry and its localized tab label, since the shared helper
      // only clicks the English tab name.
      renderWithProviders(<AiGateway />);
      await screen.findByText("Provider A");
      fireEvent.click(
        screen.getByRole("tab", {
          name: new RegExp(`^${i18n.t("aiGatewayModelPoolsTab")}`),
        }),
      );
      fireEvent.click(screen.getByTestId("ai-gateway-add-model-pool"));
      await screen.findByTestId("model-pool-dialog");

      fireEvent.click(screen.getByRole("checkbox", { name: /local-a1/i }));
      const editor = screen.getByTestId(
        "model-pool-reasoning-editor-p-a-local-a1",
      );
      fireEvent.click(within(editor).getAllByRole("button")[0]);

      // Physically absent: no details/summary and no <input> at all, so no
      // profile select, declared-values textbox, numeric budget/range input or
      // disable checkbox can be present (not merely CSS-hidden).
      expect(editor.querySelector("details")).toBeNull();
      expect(editor.querySelector("summary")).toBeNull();
      expect(editor.querySelector("input")).toBeNull();
      expect(
        within(editor).queryByTestId("model-pool-reasoning-advanced"),
      ).toBeNull();
      expect(
        screen.queryByLabelText(
          i18n.t("aiGatewayModelPoolReasoningProfileLabel"),
        ),
      ).toBeNull();
      expect(
        screen.queryByLabelText(
          i18n.t("aiGatewayModelPoolReasoningValuesLabel"),
        ),
      ).toBeNull();
      expect(
        screen.queryByLabelText(
          i18n.t("aiGatewayModelPoolReasoningBudgetMinLabel"),
        ),
      ).toBeNull();
      expect(
        screen.queryByLabelText(
          i18n.t("aiGatewayModelPoolReasoningSupportsDisableLabel"),
        ),
      ).toBeNull();

      // All five canonical preference selects exist and keep an explicit
      // unmapped option in the active locale.
      for (const effort of ["low", "medium", "high", "xhigh", "max", "none"]) {
        const select = within(editor).getByTestId(
          `model-pool-reasoning-mapping-${effort}`,
        ) as HTMLSelectElement;
        expect(select).toBeInTheDocument();
        expect(
          within(select).getByRole("option", {
            name: i18n.t("aiGatewayModelPoolReasoningMappingUnmapped"),
          }),
        ).toBeInTheDocument();
      }

      // Bind medium -> source-declared native `max`; none -> explicit disable.
      const medium = within(editor).getByTestId(
        "model-pool-reasoning-mapping-medium",
      ) as HTMLSelectElement;
      await user.selectOptions(
        medium,
        within(medium).getByRole("option", { name: "max" }),
      );
      const none = within(editor).getByTestId(
        "model-pool-reasoning-mapping-none",
      ) as HTMLSelectElement;
      expect(
        within(none).getByRole("option", {
          name: i18n.t("aiGatewayModelPoolReasoningActionDisable"),
        }),
      ).toBeInTheDocument();
      await user.selectOptions(none, "disable");

      fireEvent.click(screen.getByTestId("model-pool-save"));
      await waitFor(() => expect(upsertCalls()).toHaveLength(1));

      const pool = upsertCalls()[0];
      expect(pool.default_reasoning_effort).toBe("medium");
      const candidate = candidatesOf(pool).find(
        (item) => item.local_model === "local-a1",
      );
      const reasoning = candidate?.reasoning as Record<string, unknown>;
      expect(reasoning.profile).toBe("openai_effort");
      expect(reasoning.values).toEqual(["max"]);
      expect(reasoning.supports_disable).toBe(true);
      expect(reasoning.mappings).toEqual([
        { effort: "medium", action: { kind: "enum", value: "max" } },
        { effort: "max", action: { kind: "enum", value: "max" } },
        { effort: "none", action: { kind: "disable" } },
      ]);
    },
  );
});

describe("ModelPoolDialog 添加到候选模型时的自动推理强度绑定与无匹配提示", () => {
  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
    mockState.config = buildConfig([]);
    mockState.statusPayload = [];
    mockState.upsertError = null;
    mockState.deleted = [];
    installMock();
  });

  function autoBindProviders(): unknown[] {
    return [
      {
        id: "p-auto",
        name: "Provider Auto",
        protocol: "chat_completions",
        enabled: true,
        mappings: [
          {
            local_model: "m-matched",
            upstream_model: "remote-matched",
            reasoning_efforts: ["low", "max", "none"],
          },
          {
            local_model: "m-unmatched",
            upstream_model: "remote-unmatched",
            reasoning_efforts: ["minimal", "ultra"],
          },
          {
            local_model: "m-empty",
            upstream_model: "remote-empty",
            reasoning_efforts: [],
          },
        ],
      },
    ];
  }

  it("勾选具备匹配原生强度的模型时自动绑定对应统一档位（含 max）", async () => {
    const user = userEvent.setup();
    mockState.config = buildConfig([], autoBindProviders());
    await openModelPoolDialog("Provider Auto");

    // 勾选 m-matched 加入候选
    fireEvent.click(screen.getByRole("checkbox", { name: /m-matched/i }));

    const editor = screen.getByTestId(
      "model-pool-reasoning-editor-p-auto-m-matched",
    );
    // 展开面板检查自动绑定状态
    fireEvent.click(within(editor).getAllByRole("button")[0]);

    const lowSelect = within(editor).getByTestId(
      "model-pool-reasoning-mapping-low",
    ) as HTMLSelectElement;
    const maxSelect = within(editor).getByTestId(
      "model-pool-reasoning-mapping-max",
    ) as HTMLSelectElement;
    const noneSelect = within(editor).getByTestId(
      "model-pool-reasoning-mapping-none",
    ) as HTMLSelectElement;
    const mediumSelect = within(editor).getByTestId(
      "model-pool-reasoning-mapping-medium",
    ) as HTMLSelectElement;

    expect(lowSelect.value).toBe("enum:low");
    expect(maxSelect.value).toBe("enum:max");
    expect(noneSelect.value).toBe("disable");
    expect(mediumSelect.value).toBe("");

    // 将默认偏好切换为已绑定的 max，然后保存
    await user.selectOptions(screen.getByTestId("model-pool-reasoning-default"), "max");

    fireEvent.click(screen.getByTestId("model-pool-save"));
    await waitFor(() => expect(upsertCalls()).toHaveLength(1));

    const pool = upsertCalls()[0];
    const candidate = candidatesOf(pool).find(
      (item) => item.local_model === "m-matched",
    );
    const reasoning = candidate?.reasoning as Record<string, unknown>;
    expect(reasoning.values).toEqual(["low", "max"]);
    expect(reasoning.supports_disable).toBe(true);
    expect(reasoning.mappings).toEqual([
      { effort: "low", action: { kind: "enum", value: "low" } },
      { effort: "max", action: { kind: "enum", value: "max" } },
      { effort: "none", action: { kind: "disable" } },
    ]);
  });

  it("勾选未匹配到推理强度的模型时跳过绑定并弹出 Toast 警告提示", async () => {
    mockState.config = buildConfig([], autoBindProviders());
    await openModelPoolDialog("Provider Auto");

    // 勾选未匹配模型 m-unmatched 加入候选
    fireEvent.click(screen.getByRole("checkbox", { name: /m-unmatched/i }));

    // 验证 Toast 警告出现
    await waitFor(() => {
      expect(
        screen.getByText(/No matching reasoning levels found for m-unmatched/i),
      ).toBeInTheDocument();
    });

    const editor = screen.getByTestId(
      "model-pool-reasoning-editor-p-auto-m-unmatched",
    );
    fireEvent.click(within(editor).getAllByRole("button")[0]);

    for (const effort of ["low", "medium", "high", "xhigh", "max", "none"]) {
      const select = within(editor).getByTestId(
        `model-pool-reasoning-mapping-${effort}`,
      ) as HTMLSelectElement;
      expect(select.value).toBe("");
    }
  });

  it("全选筛选结果批量添加时，若有未匹配模型则弹出聚合 Toast 警告", async () => {
    mockState.config = buildConfig([], autoBindProviders());
    await openModelPoolDialog("Provider Auto");

    // 点击全选筛选结果
    const selectAllBtn = screen.getByText(/Select all \(3\)/i);
    fireEvent.click(selectAllBtn);

    // 验证聚合 Toast 警告出现
    await waitFor(() => {
      expect(
        screen.getByText(/2 models have no matching reasoning levels/i),
      ).toBeInTheDocument();
    });
  });

  it("默认推理偏好支持选择 max 并保存", async () => {
    const user = userEvent.setup();
    mockState.config = buildConfig([], autoBindProviders());
    await openModelPoolDialog("Provider Auto");

    // 勾选 m-matched（其自动绑定了 max）
    fireEvent.click(screen.getByRole("checkbox", { name: /m-matched/i }));

    const defaultSelect = screen.getByTestId("model-pool-reasoning-default");
    expect(
      within(defaultSelect).getByRole("option", { name: "max" }),
    ).toBeInTheDocument();

    await user.selectOptions(defaultSelect, "max");
    expect(defaultSelect).toHaveValue("max");

    fireEvent.click(screen.getByTestId("model-pool-save"));
    await waitFor(() => expect(upsertCalls()).toHaveLength(1));

    const pool = upsertCalls()[0];
    expect(pool.default_reasoning_effort).toBe("max");
  });
});


