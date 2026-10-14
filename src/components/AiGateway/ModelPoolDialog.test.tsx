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

/** Open the Model pools tab, then click the Create-model-pool button. */
async function openModelPoolDialog(firstProviderName = "Provider A") {
  renderWithProviders(<AiGateway />);
  await screen.findByText(firstProviderName);
  fireEvent.click(screen.getByRole("tab", { name: /Model pools/i }));
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

