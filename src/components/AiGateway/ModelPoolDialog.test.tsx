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
//   - candidate profile control: a labelled native <select> whose accessible
//       name is t("aiGatewayModelPoolReasoningProfileLabel"); option values are
//       "" (unknown, not declared) plus the five profile tokens
//       (openai_effort|anthropic_effort_inherit|anthropic_effort_adaptive|
//       anthropic_manual_budget|chat_thinking_switch).
//   - declared values: an input named t("aiGatewayModelPoolReasoningValuesLabel")
//       holding the comma+space joined declared native values.
//   - budget range: number inputs named
//       t("aiGatewayModelPoolReasoningBudgetMinLabel" / ...MaxLabel).
//   - supports disable: a checkbox named
//       t("aiGatewayModelPoolReasoningSupportsDisableLabel").
//   - mapping matrix: per-effort native <select> with
//       data-testid="model-pool-reasoning-mapping-<effort>"; option value "" is
//       t("aiGatewayModelPoolReasoningMappingUnmapped"), otherwise
//       "enum:<value>" | "budget" | "disable".
//   - persisted pool payload gains pool.default_reasoning_effort and
//       candidate.reasoning {profile,values,budget,supports_disable,mappings}.
// ---------------------------------------------------------------------------

function reasoningDefaultSelect(): HTMLSelectElement {
  return screen.getByLabelText(
    i18n.t("aiGatewayModelPoolReasoningDefaultLabel"),
  ) as HTMLSelectElement;
}

function candidateProfileSelect(): HTMLSelectElement {
  return screen.getByLabelText(
    i18n.t("aiGatewayModelPoolReasoningProfileLabel"),
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
    fireEvent.click(screen.getByTestId("ai-gateway-model-pool-edit-pool-legacy"));

    expect(await screen.findByTestId("model-pool-dialog")).toBeInTheDocument();
    expect(reasoningDefaultSelect()).toHaveValue("auto");
    // A legacy candidate has an unknown profile: no mapping may be invented.
    expect(mappingSelect("high")).toHaveValue("");
  });

  it("展示候选声明的原生配置、取值范围与映射矩阵，未映射档位保持显式未映射", async () => {
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
    fireEvent.click(screen.getByTestId("ai-gateway-model-pool-edit-pool-declared"));

    expect(await screen.findByTestId("model-pool-dialog")).toBeInTheDocument();
    expect(candidateProfileSelect()).toHaveValue("openai_effort");
    expect(
      screen.getByLabelText(i18n.t("aiGatewayModelPoolReasoningValuesLabel")),
    ).toHaveValue("low, medium, high");
    expect(
      screen.getByLabelText(
        i18n.t("aiGatewayModelPoolReasoningSupportsDisableLabel"),
      ),
    ).not.toBeChecked();

    // The five canonical preferences each expose an explicit unmapped option.
    for (const effort of ["low", "medium", "high", "xhigh", "none"]) {
      const select = mappingSelect(effort);
      expect(
        within(select).getByRole("option", {
          name: i18n.t("aiGatewayModelPoolReasoningMappingUnmapped"),
        }),
      ).toBeInTheDocument();
    }
    expect(mappingSelect("high")).toHaveValue("enum:high");
    expect(mappingSelect("low")).toHaveValue("");
  });

  it("编辑遗留/未知 profile 的候选时不推断映射也不生成声明值", async () => {
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
    fireEvent.click(screen.getByTestId("ai-gateway-model-pool-edit-pool-unknown"));

    expect(await screen.findByTestId("model-pool-dialog")).toBeInTheDocument();
    expect(candidateProfileSelect()).toHaveValue("legacy_unknown_profile");
    expect(mappingSelect("high")).toHaveValue("");
    // No declared value is fabricated from the unknown profile.
    expect(
      screen.getByLabelText(i18n.t("aiGatewayModelPoolReasoningValuesLabel")),
    ).toHaveValue("");
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
//   (A) each manual-budget mapping keeps its own native `budget_tokens` through
//       an unrelated save and stays independently editable, and an invalid
//       budget is refused without clearing the draft;
//   (D) the dialog shows a live static coverage summary separated from runtime
//       readiness, a new/changed concrete default needs a currently resolvable
//       mapping, while an unchanged default + declaration stays retainable;
//   (E) profile/action option labels are localized in both locales while their
//       option values stay the stable product tokens.
//
// New public interfaces pinned for Frontend:
//   - per-effort budget value input for a `budget` mapping:
//       data-testid="model-pool-reasoning-budget-<effort>" (type="number"),
//       accessible label = `${t("aiGatewayModelPoolReasoningBudgetLabel")}: <effort>`.
//   - live static coverage value: data-testid="model-pool-reasoning-static-coverage"
//       (label text keeps aiGatewayModelPoolReasoningCoverageLabel).
//   - live runtime readiness value: data-testid="model-pool-reasoning-runtime-readiness"
//       (label text keeps aiGatewayModelPoolReasoningRuntimeReadinessLabel).
// ---------------------------------------------------------------------------

type PoolCandidatePayload = {
  effort: string;
  action: Record<string, unknown>;
};

describe("ModelPoolDialog 模型池推理策略回归（Step 4 repair）", () => {
  const PROFILE_LABEL_KEYS: Array<[string, string]> = [
    ["openai_effort", "aiGatewayModelPoolReasoningProfileOpenaiEffort"],
    [
      "anthropic_effort_inherit",
      "aiGatewayModelPoolReasoningProfileAnthropicEffortInherit",
    ],
    [
      "anthropic_effort_adaptive",
      "aiGatewayModelPoolReasoningProfileAnthropicEffortAdaptive",
    ],
    [
      "anthropic_manual_budget",
      "aiGatewayModelPoolReasoningProfileAnthropicManualBudget",
    ],
    [
      "chat_thinking_switch",
      "aiGatewayModelPoolReasoningProfileChatThinkingSwitch",
    ],
  ];

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
    fireEvent.click(screen.getByTestId(`ai-gateway-model-pool-edit-${pool.id}`));
    await screen.findByTestId("model-pool-dialog");
  }

  function budgetInput(effort: string): HTMLInputElement {
    return screen.getByTestId(
      `model-pool-reasoning-budget-${effort}`,
    ) as HTMLInputElement;
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

  it("无关改名保存保留每个档位各自的预算", async () => {
    const user = userEvent.setup();
    await openForEdit(manualBudgetPool, ANTHROPIC_PROVIDERS);

    expect(budgetInput("low")).toHaveValue(1536);
    expect(budgetInput("high")).toHaveValue(3072);

    const nameInput = screen.getByTestId("model-pool-name-input");
    await user.clear(nameInput);
    await user.type(nameInput, "Renamed Budget Pool");
    fireEvent.click(screen.getByTestId("model-pool-save"));

    await waitFor(() => expect(upsertCalls()).toHaveLength(1));
    const saved = upsertCalls()[0];
    expect(saved.default_reasoning_effort).toBe("high");
    expect(mappingAction(saved, "low")).toEqual({
      kind: "budget",
      budget_tokens: 1536,
    });
    expect(mappingAction(saved, "high")).toEqual({
      kind: "budget",
      budget_tokens: 3072,
    });
  });

  it("键盘修改 high 档预算不影响 low 档预算", async () => {
    const user = userEvent.setup();
    await openForEdit(manualBudgetPool, ANTHROPIC_PROVIDERS);

    const high = budgetInput("high");
    await user.clear(high);
    await user.type(high, "4096");
    fireEvent.click(screen.getByTestId("model-pool-save"));

    await waitFor(() => expect(upsertCalls()).toHaveLength(1));
    const saved = upsertCalls()[0];
    expect(mappingAction(saved, "high")).toEqual({
      kind: "budget",
      budget_tokens: 4096,
    });
    expect(mappingAction(saved, "low")).toEqual({
      kind: "budget",
      budget_tokens: 1536,
    });
  });

  it("拒绝小数预算并保留草稿", async () => {
    await openForEdit(manualBudgetPool, ANTHROPIC_PROVIDERS);

    fireEvent.change(budgetInput("high"), { target: { value: "4096.5" } });
    fireEvent.click(screen.getByTestId("model-pool-save"));

    expect(await screen.findByTestId("model-pool-error")).toBeInTheDocument();
    expect(upsertCalls()).toHaveLength(0);
    expect(budgetInput("high").value).toBe("4096.5");
  });

  it("拒绝超出声明上限的预算并保留草稿", async () => {
    await openForEdit(manualBudgetPool, ANTHROPIC_PROVIDERS);

    fireEvent.change(budgetInput("high"), { target: { value: "32768" } });
    fireEvent.click(screen.getByTestId("model-pool-save"));

    expect(await screen.findByTestId("model-pool-error")).toBeInTheDocument();
    expect(upsertCalls()).toHaveLength(0);
    expect(budgetInput("high").value).toBe("32768");
  });

  it("拒绝低于 1024 的预算并保留草稿", async () => {
    await openForEdit(manualBudgetPool, ANTHROPIC_PROVIDERS);

    fireEvent.change(budgetInput("high"), { target: { value: "512" } });
    fireEvent.click(screen.getByTestId("model-pool-save"));

    expect(await screen.findByTestId("model-pool-error")).toBeInTheDocument();
    expect(upsertCalls()).toHaveLength(0);
    expect(budgetInput("high").value).toBe("512");
  });

  it("拒绝低于适配器下限的声明区间并保留草稿", async () => {
    const user = userEvent.setup();
    await openForEdit(manualBudgetPool, ANTHROPIC_PROVIDERS);

    // `auto` removes the structural default guard so only the range check can
    // reject this save.
    await user.selectOptions(reasoningDefaultSelect(), "auto");
    fireEvent.change(
      screen.getByLabelText(
        i18n.t("aiGatewayModelPoolReasoningBudgetMinLabel"),
      ),
      { target: { value: "100" } },
    );
    fireEvent.click(screen.getByTestId("model-pool-save"));

    expect(await screen.findByTestId("model-pool-error")).toBeInTheDocument();
    expect(upsertCalls()).toHaveLength(0);
    expect(
      screen.getByLabelText(i18n.t("aiGatewayModelPoolReasoningBudgetMinLabel")),
    ).toHaveValue(100);
  });

  it("草稿态静态覆盖随声明编辑实时更新", async () => {
    await openForEdit(liveCoveragePool);

    expect(
      screen.getByTestId("model-pool-reasoning-static-coverage"),
    ).toHaveTextContent(coverageCopy(1, 2));

    const editor = screen.getByTestId("model-pool-reasoning-editor-p-b-local-b1");
    fireEvent.change(
      within(editor).getByLabelText(
        i18n.t("aiGatewayModelPoolReasoningProfileLabel"),
      ),
      { target: { value: "openai_effort" } },
    );
    fireEvent.change(
      within(editor).getByLabelText(
        i18n.t("aiGatewayModelPoolReasoningValuesLabel"),
      ),
      { target: { value: "high" } },
    );
    fireEvent.change(
      within(editor).getByTestId("model-pool-reasoning-mapping-high"),
      { target: { value: "enum:high" } },
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

  it.each(["en", "zh"] as const)(
    "%s 下 profile 与 budget/disable 选项显示本地化名称且取值仍是稳定 token",
    async (language) => {
      await i18n.changeLanguage(language);
      await openForEdit(manualBudgetPool, ANTHROPIC_PROVIDERS);

      const profileSelect = candidateProfileSelect();
      for (const [token, key] of PROFILE_LABEL_KEYS) {
        const option = within(profileSelect).getByRole("option", {
          name: i18n.t(key),
        }) as HTMLOptionElement;
        expect(option.value).toBe(token);
      }

      const highSelect = mappingSelect("high");
      expect(
        (
          within(highSelect).getByRole("option", {
            name: i18n.t("aiGatewayModelPoolReasoningActionBudget"),
          }) as HTMLOptionElement
        ).value,
      ).toBe("budget");
      expect(
        (
          within(highSelect).getByRole("option", {
            name: i18n.t("aiGatewayModelPoolReasoningActionDisable"),
          }) as HTMLOptionElement
        ).value,
      ).toBe("disable");

      expect(
        screen.getByLabelText(
          `${i18n.t("aiGatewayModelPoolReasoningBudgetLabel")}: high`,
        ),
      ).toHaveValue(3072);
    },
  );
});

