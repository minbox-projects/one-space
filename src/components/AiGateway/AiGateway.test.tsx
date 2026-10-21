import { useState } from "react";
import { act, fireEvent, renderHook, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { AiGateway } from "@/components/AiGateway";
import {
  AI_GATEWAY_CONFIG_UPDATED_EVENT,
  AI_GATEWAY_KEY_AUTH_FAILED_EVENT,
  AI_GATEWAY_KEY_AUTH_TOAST_WINDOW_MS,
  formatGatewayTimestamp,
  maskSecret,
  type GatewayConfig,
  type GatewayKeyAuthFailedEvent,
  type GatewayModelPool,
  type GatewayProviderTemplate,
  type GatewayProviderTemplateModel,
  type GatewayProviderTemplateView,
  type GatewayStatus,
  type GatewayTerminalTarget,
  type GatewayUpstreamProvider,
  type ModelPrice,
  type UsageLogRecord,
  type UsageLogsPage,
  type UsageStats,
} from "@/lib/aiGateway";
import { renderWithProviders } from "@/test/mocks/render";
import { setNativeWindowVisible } from "@/lib/runtimeStatus";
import { emitMock, invokeMock, listenMock, resetTauriMocks } from "@/test/mocks/tauri";
import {
  recordMessageMock,
  resetMessageMocks,
  safeRecordMessageMock,
} from "@/test/mocks/messages";
import { useTemplateAutoRefresh } from "./useTemplateAutoRefresh";

type Store = {
  config: GatewayConfig;
  status: GatewayStatus;
  targets: GatewayTerminalTarget[];
  /** Nonsecret pool status payload; absent means an empty collection. */
  poolStatus?: unknown[];
};

function makeConfig(overrides: Partial<GatewayConfig> = {}): GatewayConfig {
  return {
    enabled: true,
    port: 17688,
    providers: [],
    keys: [],
    default_key_id: null,
    terminal_syncs: [],
    ...overrides,
  };
}

function makeStatus(overrides: Partial<GatewayStatus> = {}): GatewayStatus {
  return {
    running: true,
    enabled: true,
    port: 17688,
    local_base_url: "http://127.0.0.1:17688/v1",
    provider_count: 0,
    auto_disabled_count: 0,
    key_count: 0,
    default_key_id: null,
    ...overrides,
  };
}

async function visitTerminalTab() {
  const tab = await screen.findByRole("tab", {
    name: /AI terminal integration/i,
  });
  fireEvent.click(tab);
}

function openCodeTarget(
  overrides: Partial<GatewayTerminalTarget> = {},
): GatewayTerminalTarget {
  return {
    provider_id: "t-open",
    tool: "opencode",
    name: "OpenCode",
    base_url: "http://127.0.0.1:17688/v1",
    synced: false,
    pending_sync: true,
    synced_key_id: null,
    synced_at: null,
    ...overrides,
  };
}

function emptyUsageStats(overrides: Partial<UsageStats> = {}): UsageStats {
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
    granularity: "hour",
    buckets: [],
    models: [],
    ...overrides,
  };
}

function usageLogRecord(
  overrides: Partial<UsageLogRecord> = {},
): UsageLogRecord {
  return {
    timestamp_ms: 1_700_000_000_000,
    local_model: "local-a",
    upstream_model: "remote-a",
    provider_id: "p1",
    provider_name: "Provider",
    result: "success",
    status: 200,
    input_tokens: 1,
    cache_read_tokens: 0,
    cache_write_tokens: 0,
    output_tokens: 1,
    total_tokens: 2,
    amount: 0.1,
    duration_ms: 10,
    ...overrides,
  };
}

function usageLogsPage(overrides: Partial<UsageLogsPage> = {}): UsageLogsPage {
  return {
    page: 1,
    page_size: 50,
    total: 120,
    total_pages: 3,
    group_by: null,
    records: [usageLogRecord()],
    groups: [],
    ...overrides,
  };
}

function deferredPromise<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function mockStore(store: Store) {
  invokeMock.mockImplementation(async (command: string, args?: any) => {
    switch (command) {
      case "ai_gateway_get_config":
        return store.config;
      case "ai_gateway_status":
        return store.status;
      case "ai_gateway_terminal_targets":
        return store.targets;
      case "ai_gateway_model_pools_status":
        return store.poolStatus ?? [];
      case "ai_gateway_start":
        store.status = { ...store.status, running: true };
        return store.status;
      case "ai_gateway_stop":
        store.status = { ...store.status, running: false };
        return store.status;
      case "ai_gateway_delete_provider":
        store.config = {
          ...store.config,
          providers: store.config.providers.filter((p) => p.id !== args?.providerId),
        };
        return store.config;
      case "ai_gateway_configure_terminal":
        return store.config.terminal_syncs;
      case "ai_gateway_sync_terminal":
        return store.config.terminal_syncs;
      case "ai_gateway_usage_stats":
        return emptyUsageStats({ request_count: 1, total_tokens: 2 });
      case "ai_gateway_request_logs":
        if (args?.groupBy === "day") {
          return usageLogsPage({
            group_by: "day",
            records: [],
            groups: [
              {
                group: "2026-09-17",
                request_count: 3,
                error_count: 1,
                last_request_at_ms: 1_700_000_000_000,
              },
            ],
          });
        }
        return usageLogsPage({ page: (args?.page as number) ?? 1 });
      default:
        throw new Error(`Unhandled command: ${command}`);
    }
  });
}

function mockStoreWithUpsert(store: Store) {
  mockStore(store);
  const read = invokeMock.getMockImplementation()!;
  invokeMock.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    if (command === "ai_gateway_upsert_provider") {
      const { provider, prices } = args as {
        provider: GatewayUpstreamProvider;
        prices?: ModelPrice[] | null;
      };
      const otherRows = (store.config.model_prices ?? []).filter(
        (row) => row.provider_id !== provider.id,
      );
      store.config = {
        ...store.config,
        providers: [
          ...store.config.providers.filter((entry) => entry.id !== provider.id),
          provider,
        ],
        ...(prices == null
          ? {}
          : {
              model_prices: [
                ...otherRows,
                ...prices.map((row) => ({ ...row, provider_id: provider.id })),
              ],
            }),
      };
      return store.config;
    }
    return read(command, args);
  });
}

type RawProviderKey = Record<string, unknown>;

function providerKey(overrides: RawProviderKey = {}): RawProviderKey {
  return {
    id: "k1",
    name: "Default",
    value: "sk-default",
    enabled: true,
    auto_marked: false,
    failure_kind: null,
    marked_at: null,
    reason: null,
    ...overrides,
  };
}

function makeProvider(
  overrides: Partial<GatewayUpstreamProvider> & { keys?: RawProviderKey[] } = {},
): GatewayUpstreamProvider {
  const { keys, ...rest } = overrides;
  return {
    id: "p1",
    name: "Upstream A",
    base_url: "https://api.a.example",
    keys: keys ?? [providerKey()],
    default_model: null,
    mappings: [],
    enabled: true,
    ...rest,
  } as unknown as GatewayUpstreamProvider;
}

describe("AiGateway", () => {
  let writeText: ReturnType<typeof vi.fn>;

  beforeEach(async () => {
    resetTauriMocks();
    resetMessageMocks();
    await i18n.changeLanguage("en");
    writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
  });

  it("只渲染受支持工具，且顶部不再渲染全局操作按钮", async () => {
    const store: Store = {
      config: makeConfig({
        keys: [{ id: "k1", label: "Main", value: "********", enabled: true, created_at: 1 }],
        default_key_id: "k1",
      }),
      status: makeStatus({ key_count: 1, default_key_id: "k1" }),
      targets: [
        openCodeTarget({ provider_id: "gw-open" }),
        openCodeTarget({ provider_id: "gw-codex", tool: "codex", name: "Codex" }),
        openCodeTarget({
          provider_id: "gw-claude",
          tool: "claude",
          name: "Claude Code",
          base_url: null,
        }),
        openCodeTarget({
          provider_id: "gw-antigravity",
          tool: "antigravity",
          name: "Antigravity",
          base_url: null,
        }),
      ],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);
    await visitTerminalTab();
    await screen.findByText("OpenCode");

    // Step 4 adds Claude as a supported terminal target while Antigravity
    // remains unsupported.
    expect(screen.getByText("Claude Code")).toBeInTheDocument();
    expect(screen.queryByText("Antigravity")).not.toBeInTheDocument();

    const panel = within(screen.getByTestId("ai-gateway-terminals"));
    // The header-level "Add provider / Sync again" actions must be gone:
    // every button inside the panel belongs to a terminal target row.
    const buttons = panel.getAllByRole("button");
    expect(buttons.length).toBeGreaterThan(0);
    for (const button of buttons) {
      expect(button.closest('[data-testid^="ai-gateway-target-"]')).not.toBeNull();
    }
    expect(
      panel.queryByRole("button", { name: /sync again/i }),
    ).not.toBeInTheDocument();
  });

  it("首次同步 Claude 打开四选择对话框并以 claude_models 载荷调用配置命令", async () => {
    const store: Store = {
      config: makeConfig({
        keys: [{ id: "k1", label: "Main", value: "********", enabled: true, created_at: 1 }],
        default_key_id: "k1",
        providers: [
          makeProvider({
            id: "p-claude",
            name: "CommandCode",
            protocol: "anthropic_messages",
            mappings: [
              { local_model: "cc-haiku", upstream_model: "claude-haiku-5-5" },
              { local_model: "cc-sonnet", upstream_model: "claude-sonnet-5-5" },
              { local_model: "cc-opus", upstream_model: "claude-opus-5-5" },
            ],
          }),
        ],
      }),
      status: makeStatus({ key_count: 1, default_key_id: "k1", provider_count: 1 }),
      targets: [
        openCodeTarget({
          provider_id: "gw-claude",
          tool: "claude",
          name: "Claude Code",
          base_url: null,
        }),
      ],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);
    await visitTerminalTab();
    await screen.findByText("Claude Code");

    const claudeRow = within(screen.getByTestId("ai-gateway-target-claude"));
    fireEvent.click(claudeRow.getByRole("button", { name: /add/i }));

    const dialog = await screen.findByTestId("claude-model-selection-dialog");
    fireEvent.change(within(dialog).getByTestId("claude-model-select-default"), {
      target: { value: "cc-opus" },
    });
    fireEvent.change(within(dialog).getByTestId("claude-model-select-haiku"), {
      target: { value: "cc-haiku" },
    });
    fireEvent.change(within(dialog).getByTestId("claude-model-select-sonnet"), {
      target: { value: "cc-sonnet" },
    });
    fireEvent.change(within(dialog).getByTestId("claude-model-select-opus"), {
      target: { value: "cc-opus" },
    });
    fireEvent.click(within(dialog).getByTestId("claude-model-selection-submit"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_configure_terminal", {
        targetTools: ["claude"],
        claudeModels: {
          default_model: "cc-opus",
          haiku: "cc-haiku",
          sonnet: "cc-sonnet",
          opus: "cc-opus",
        },
      }),
    );
  });

  it("待同步的行内显示添加，点击仅以该工具调用配置", async () => {
    const store: Store = {
      config: makeConfig({
        keys: [{ id: "k1", label: "Main", value: "********", enabled: true, created_at: 1 }],
        default_key_id: "k1",
      }),
      status: makeStatus({ key_count: 1, default_key_id: "k1" }),
      targets: [
        openCodeTarget({ provider_id: "gw-open" }),
        openCodeTarget({ provider_id: "gw-codex", tool: "codex", name: "Codex" }),
      ],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);
    await visitTerminalTab();
    await screen.findByText("OpenCode");

    const openRow = within(screen.getByTestId("ai-gateway-target-opencode"));
    const addButton = openRow.getByRole("button", { name: /add/i });
    expect(addButton).toBeEnabled();
    fireEvent.click(addButton);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_configure_terminal", {
        targetTools: ["opencode"],
      }),
    );
    const configureCalls = invokeMock.mock.calls.filter(
      ([command]) => command === "ai_gateway_configure_terminal",
    );
    expect(configureCalls).toHaveLength(1);
    expect(invokeMock).not.toHaveBeenCalledWith("ai_gateway_configure_terminal", {
      targetTools: ["opencode", "codex"],
    });
  });

  it("已添加的行内显示同步，点击仅以该工具调用同步", async () => {
    const store: Store = {
      config: makeConfig({
        keys: [{ id: "k1", label: "Main", value: "********", enabled: true, created_at: 1 }],
        default_key_id: "k1",
      }),
      status: makeStatus({ key_count: 1, default_key_id: "k1" }),
      targets: [
        openCodeTarget({
          provider_id: "gw-open",
          synced: true,
          pending_sync: false,
          synced_key_id: "k1",
          synced_at: 1,
        }),
        openCodeTarget({
          provider_id: "gw-codex",
          tool: "codex",
          name: "Codex",
          synced: true,
          pending_sync: false,
          synced_key_id: "k1",
          synced_at: 1,
        }),
      ],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);
    await visitTerminalTab();
    await screen.findByText("OpenCode");

    const openRow = within(screen.getByTestId("ai-gateway-target-opencode"));
    const syncButton = openRow.getByRole("button", { name: /sync/i });
    expect(syncButton).toBeEnabled();
    fireEvent.click(syncButton);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_sync_terminal", {
        targetTools: ["opencode"],
      }),
    );
    const syncCalls = invokeMock.mock.calls.filter(
      ([command]) => command === "ai_gateway_sync_terminal",
    );
    expect(syncCalls).toHaveLength(1);
    expect(invokeMock).not.toHaveBeenCalledWith("ai_gateway_sync_terminal", {
      targetTools: ["opencode", "codex"],
    });
  });

  it("已添加但 Key 或地址漂移（synced 为真且待同步）时行内显示同步并仅走同步通道", async () => {
    const store: Store = {
      config: makeConfig({
        keys: [{ id: "k1", label: "Main", value: "********", enabled: true, created_at: 1 }],
        default_key_id: "k1",
      }),
      status: makeStatus({ key_count: 1, default_key_id: "k1" }),
      targets: [
        openCodeTarget({
          provider_id: "gw-open",
          // Gateway provider still exists, but the synced key/address drifted:
          // the backend reports synced=true together with pending_sync=true.
          synced: true,
          pending_sync: true,
          synced_key_id: "k0",
          synced_at: 1,
        }),
      ],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);
    await visitTerminalTab();
    await screen.findByText("OpenCode");

    const openRow = within(screen.getByTestId("ai-gateway-target-opencode"));
    const action = openRow.getByRole("button", { name: /sync/i });
    expect(action).toBeEnabled();
    expect(action).toHaveAccessibleName(/sync/i);
    expect(action).not.toHaveAccessibleName(/sync again/i);
    fireEvent.click(action);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_sync_terminal", {
        targetTools: ["opencode"],
      }),
    );
    const syncCalls = invokeMock.mock.calls.filter(
      ([command]) => command === "ai_gateway_sync_terminal",
    );
    expect(syncCalls).toHaveLength(1);
    expect(invokeMock).not.toHaveBeenCalledWith(
      "ai_gateway_configure_terminal",
      expect.anything(),
    );
  });

  it("网关服务商被删除但同步台账仍在（synced 为假且 synced_key_id/synced_at 非空）时行内显示同步并仅走同步通道", async () => {
    const store: Store = {
      config: makeConfig({
        keys: [{ id: "k1", label: "Main", value: "********", enabled: true, created_at: 1 }],
        default_key_id: "k1",
      }),
      status: makeStatus({ key_count: 1, default_key_id: "k1" }),
      targets: [
        openCodeTarget({
          provider_id: null,
          base_url: null,
          // The managed gateway provider was removed by hand, so synced=false,
          // but the local sync ledger still records a previous sync.
          synced: false,
          pending_sync: true,
          synced_key_id: "k1",
          synced_at: 1,
        }),
      ],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);
    await visitTerminalTab();
    await screen.findByText("OpenCode");

    const openRow = within(screen.getByTestId("ai-gateway-target-opencode"));
    const action = openRow.getByRole("button", { name: /sync/i });
    expect(action).toBeEnabled();
    expect(action).toHaveAccessibleName(/sync/i);
    expect(action).not.toHaveAccessibleName(/sync again/i);
    fireEvent.click(action);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_sync_terminal", {
        targetTools: ["opencode"],
      }),
    );
    const syncCalls = invokeMock.mock.calls.filter(
      ([command]) => command === "ai_gateway_sync_terminal",
    );
    expect(syncCalls).toHaveLength(1);
    expect(invokeMock).not.toHaveBeenCalledWith(
      "ai_gateway_configure_terminal",
      expect.anything(),
    );
  });

  it("默认本地 Key 为空时禁用行内操作按钮且不触发调用", async () => {
    const store: Store = {
      config: makeConfig({ keys: [], default_key_id: null }),
      status: makeStatus({ key_count: 0, default_key_id: null }),
      targets: [openCodeTarget()],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);
    await visitTerminalTab();
    await screen.findByText("OpenCode");

    expect(screen.getByTestId("ai-gateway-default-key-required")).toHaveTextContent(
      /Add and enable a local key/,
    );
    const openRow = within(screen.getByTestId("ai-gateway-target-opencode"));
    const action = openRow.getByRole("button", { name: /add/i });
    expect(action).toBeDisabled();

    fireEvent.click(action);
    expect(invokeMock).not.toHaveBeenCalledWith(
      "ai_gateway_configure_terminal",
      expect.anything(),
    );
    expect(invokeMock).not.toHaveBeenCalledWith(
      "ai_gateway_sync_terminal",
      expect.anything(),
    );
  });

  it("从未添加的行内显示添加，点击走配置通道并在配置后清除待同步", async () => {
    const store: Store = {
      config: makeConfig({
        keys: [
          { id: "k1", label: "Main", value: "********", enabled: true, created_at: 1 },
        ],
        default_key_id: "k1",
      }),
      status: makeStatus({ key_count: 1, default_key_id: "k1" }),
      targets: [
        openCodeTarget({
          provider_id: "gw-open",
          synced: false,
          pending_sync: true,
        }),
      ],
    };

    invokeMock.mockImplementation(async (command: string) => {
      switch (command) {
        case "ai_gateway_get_config":
          return store.config;
        case "ai_gateway_status":
          return store.status;
        case "ai_gateway_terminal_targets":
          return store.targets;
        case "ai_gateway_configure_terminal": {
          store.targets = [
            openCodeTarget({
              provider_id: "gw-open",
              synced: true,
              pending_sync: false,
              synced_key_id: "k1",
              synced_at: 2,
            }),
          ];
          return [];
        }
        default:
          throw new Error(`Unhandled command: ${command}`);
      }
    });

    renderWithProviders(<AiGateway />);
    await visitTerminalTab();
    await screen.findByText("OpenCode");

    // Pending status comes from the backend target payload, not local re-derivation.
    expect(screen.getByText("Pending sync")).toBeInTheDocument();

    const openRow = within(screen.getByTestId("ai-gateway-target-opencode"));
    fireEvent.click(openRow.getByRole("button", { name: /add/i }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_configure_terminal", {
        targetTools: ["opencode"],
      }),
    );
    await waitFor(() =>
      expect(screen.queryByText("Pending sync")).not.toBeInTheDocument(),
    );
    expect(
      screen.getByTestId("ai-gateway-target-synced-opencode"),
    ).toHaveTextContent(formatGatewayTimestamp(2)!);
    expect(screen.getByText("Synced")).toBeInTheDocument();
  });

  it("待同步状态直接采用后端 pending_sync", async () => {
    const store: Store = {
      config: makeConfig({ keys: [], default_key_id: null, terminal_syncs: [] }),
      status: makeStatus({ key_count: 0, default_key_id: null }),
      targets: [
        {
          tool: "opencode",
          name: "OpenCode",
          provider_id: "gw-open",
          base_url: "http://127.0.0.1:17688/v1",
          synced: true,
          pending_sync: false,
          synced_key_id: "k1",
          synced_at: 1,
        },
      ],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);
    await visitTerminalTab();
    await screen.findByText("OpenCode");

    const panel = within(screen.getByTestId("ai-gateway-terminals"));
    expect(panel.getByText("Synced")).toBeInTheDocument();
    expect(panel.queryByText("Pending sync")).not.toBeInTheDocument();
  });

  it("AI终端集成展示未同步文案及已同步的最后一次同步时间", async () => {
    const syncedAt = 1_700_000_000;
    const store: Store = {
      config: makeConfig({
        keys: [{ id: "k1", label: "Default Key", value: "sk-test", enabled: true, created_at: 1 }],
        default_key_id: "k1",
        terminal_syncs: [
          {
            provider_id: "gw-open",
            tool: "opencode",
            synced_key_id: "k1",
            synced_base_url: "http://127.0.0.1:17688/v1",
            synced_at: syncedAt,
          },
        ],
      }),
      status: makeStatus({ key_count: 1, default_key_id: "k1" }),
      targets: [
        {
          tool: "opencode",
          name: "OpenCode",
          provider_id: "gw-open",
          base_url: "http://127.0.0.1:17688/v1",
          synced: true,
          pending_sync: false,
          synced_key_id: "k1",
          synced_at: syncedAt,
        },
        {
          tool: "codex",
          name: "Codex",
          provider_id: null,
          base_url: null,
          synced: false,
          pending_sync: true,
          synced_key_id: null,
          synced_at: null,
        },
      ],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);
    await visitTerminalTab();
    await screen.findByText("OpenCode");

    const openCodeSyncedEl = screen.getByTestId("ai-gateway-target-synced-opencode");
    expect(openCodeSyncedEl).toHaveTextContent(formatGatewayTimestamp(syncedAt)!);
    expect(openCodeSyncedEl).toHaveTextContent("Last sync");

    const codexSyncedEl = screen.getByTestId("ai-gateway-target-synced-codex");
    expect(codexSyncedEl).toHaveTextContent("Not synced yet");
    expect(codexSyncedEl).toHaveTextContent("Last sync");
  });

  it("展示运行状态、端口与掩码 Key，并支持复制地址与 Key", async () => {
    const rawKey = "sk-live-secret-1234";
    const store: Store = {
      config: makeConfig({
        providers: [
          makeProvider({
            id: "p1",
            name: "Upstream Broken",
            base_url: "https://api.broken.example",
          }),
        ],
        keys: [{ id: "k1", label: "Main", value: rawKey, enabled: true, created_at: 1 }],
        default_key_id: "k1",
      }),
      status: makeStatus({
        provider_count: 1,
        auto_disabled_count: 1,
        key_count: 1,
        default_key_id: "k1",
      }),
      targets: [openCodeTarget()],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);

    expect(await screen.findByTestId("ai-gateway-runtime-state")).toHaveTextContent(
      "Running",
    );
    expect(screen.getByTestId("ai-gateway-local-address")).toHaveTextContent(
      "http://127.0.0.1:17688/v1",
    );
    expect(screen.getByTestId("ai-gateway-auto-disabled-count")).toHaveTextContent("1");

    fireEvent.click(screen.getByRole("button", { name: /Copy local API address/ }));
    await waitFor(() =>
      expect(writeText).toHaveBeenCalledWith("http://127.0.0.1:17688/v1"),
    );

    const masked = screen.getByTestId("ai-gateway-key-value-k1");
    expect(masked).toHaveTextContent(maskSecret(rawKey));
    expect(masked).not.toHaveTextContent(rawKey);

    fireEvent.click(screen.getByRole("button", { name: /Copy key Main/ }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(rawKey));
  });

  it("映射协议默认跟随服务商，新增映射行同样默认跟随", async () => {
    const store: Store = {
      config: makeConfig({
        providers: [
          makeProvider({
            mappings: [{ local_model: "local-a", upstream_model: "remote-a" }],
          }),
        ],
      }),
      status: makeStatus({ provider_count: 1 }),
      targets: [openCodeTarget()],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);
    fireEvent.click(
      await within(
        await screen.findByTestId("ai-gateway-providers"),
      ).findByText("Upstream A"),
    );

    const firstMappingProtocol = screen.getByLabelText("Mapping protocol 1");
    expect(firstMappingProtocol).toHaveValue("");
    expect(firstMappingProtocol).toHaveDisplayValue("Inherit from provider");
    expect(
      screen.getByRole("option", { name: "Inherit from provider" }),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /Add mapping/ }));
    const secondMappingProtocol = screen.getByLabelText("Mapping protocol 2");
    expect(secondMappingProtocol).toHaveValue("");
    expect(secondMappingProtocol).toHaveDisplayValue("Inherit from provider");
  });

  it("保存继承行时映射协议缺省或为 null 且不为空字符串", async () => {
    const store: Store = {
      config: makeConfig({
        providers: [
          makeProvider({
            mappings: [{ local_model: "local-a", upstream_model: "remote-a" }],
          }),
        ],
      }),
      status: makeStatus({ provider_count: 1 }),
      targets: [openCodeTarget()],
    };
    mockStoreWithUpsert(store);

    renderWithProviders(<AiGateway />);
    fireEvent.click(
      await within(
        await screen.findByTestId("ai-gateway-providers"),
      ).findByText("Upstream A"),
    );
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "ai_gateway_upsert_provider",
        expect.objectContaining({
          provider: expect.objectContaining({ id: "p1" }),
        }),
      ),
    );

    const call = invokeMock.mock.calls.find(
      ([command]) => command === "ai_gateway_upsert_provider",
    );
    const payload = call?.[1] as { provider: GatewayUpstreamProvider };
    expect(payload.provider.mappings[0].protocol ?? null).toBeNull();
    expect(payload.provider.mappings[0].protocol).not.toBe("");
  });

  it("保存显式协议时提交该协议", async () => {
    const store: Store = {
      config: makeConfig({
        providers: [
          makeProvider({
            mappings: [{ local_model: "local-a", upstream_model: "remote-a" }],
          }),
        ],
      }),
      status: makeStatus({ provider_count: 1 }),
      targets: [openCodeTarget()],
    };
    mockStoreWithUpsert(store);

    renderWithProviders(<AiGateway />);
    fireEvent.click(
      await within(
        await screen.findByTestId("ai-gateway-providers"),
      ).findByText("Upstream A"),
    );
    fireEvent.change(screen.getByLabelText("Mapping protocol 1"), {
      target: { value: "responses" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "ai_gateway_upsert_provider",
        expect.objectContaining({
          provider: expect.objectContaining({ id: "p1" }),
        }),
      ),
    );

    const call = invokeMock.mock.calls.find(
      ([command]) => command === "ai_gateway_upsert_provider",
    );
    const payload = call?.[1] as { provider: GatewayUpstreamProvider };
    expect(payload.provider.mappings[0].protocol).toBe("responses");
  });

  it("provider_save_sends_prices_in_one_upsert_call", async () => {
    const store: Store = {
      config: makeConfig({
        providers: [
          makeProvider({
            mappings: [{ local_model: "local-a", upstream_model: "remote-a" }],
          }),
        ],
      }),
      status: makeStatus({ provider_count: 1 }),
      targets: [openCodeTarget()],
    };
    mockStoreWithUpsert(store);

    renderWithProviders(<AiGateway />);
    fireEvent.click(
      await within(
        await screen.findByTestId("ai-gateway-providers"),
      ).findByText("Upstream A"),
    );

    fireEvent.click(screen.getByTestId("ai-gateway-mapping-expand-0"));
    await screen.findByTestId("ai-gateway-price-0-input");
    fireEvent.change(screen.getByTestId("ai-gateway-price-0-input"), {
      target: { value: "2" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "ai_gateway_upsert_provider",
        expect.objectContaining({
          provider: expect.objectContaining({ id: "p1" }),
          prices: expect.any(Array),
        }),
      ),
    );

    const upsertCalls = invokeMock.mock.calls.filter(
      ([command]) => command === "ai_gateway_upsert_provider",
    );
    expect(upsertCalls).toHaveLength(1);
    const payload = upsertCalls[0][1] as {
      provider: GatewayUpstreamProvider;
      prices: ModelPrice[];
    };
    expect(payload.provider.id).toBe("p1");
    expect(payload.prices).toHaveLength(1);
    expect(payload.prices[0]).toMatchObject({
      upstream_model: "remote-a",
      input: 2,
      cache_read: 0,
      cache_write: 0,
      output: 0,
    });
  });

  it("编辑服务商时推荐标签包含其他服务商已有标签并可复用", async () => {
    const store: Store = {
      config: makeConfig({
        providers: [
          makeProvider({ id: "p1", name: "Upstream A", tags: ["alpha"] }),
          makeProvider({ id: "p2", name: "Upstream B", tags: ["beta"] }),
        ],
      }),
      status: makeStatus({ provider_count: 2 }),
      targets: [],
    };
    mockStoreWithUpsert(store);

    renderWithProviders(<AiGateway />);
    fireEvent.click(
      await within(
        await screen.findByTestId("ai-gateway-providers"),
      ).findByText("Upstream A"),
    );

    const otherProviderTagSuggestion = await screen.findByTestId(
      "suggested-tag-beta",
    );
    expect(otherProviderTagSuggestion).toBeInTheDocument();
    expect(screen.getByTestId("suggested-tag-alpha")).toBeInTheDocument();

    fireEvent.click(otherProviderTagSuggestion);
    expect(screen.getByTestId("provider-tag-badge-beta")).toBeInTheDocument();
  });

  it("新增本地 Key 只需名称，值留空交由后端随机生成", async () => {
    const store: Store = {
      config: makeConfig({ keys: [], default_key_id: null }),
      status: makeStatus({ key_count: 0, default_key_id: null }),
      targets: [],
    };
    invokeMock.mockImplementation(async (command: string) => {
      switch (command) {
        case "ai_gateway_get_config":
        case "ai_gateway_upsert_key":
          return store.config;
        case "ai_gateway_status":
          return store.status;
        case "ai_gateway_terminal_targets":
          return store.targets;
        default:
          throw new Error(`Unhandled command: ${command}`);
      }
    });

    renderWithProviders(<AiGateway />);
    await screen.findByTestId("ai-gateway-keys");

    // 页头不再有内联名称输入框，"新增 Key" 按钮始终可用
    expect(screen.queryByLabelText("Name")).not.toBeInTheDocument();
    const addButton = screen.getByRole("button", { name: /Add key/ });
    expect(addButton).toBeEnabled();

    // 点击按钮弹出对话框
    fireEvent.click(addButton);
    const dialog = await screen.findByTestId("ai-gateway-key-dialog");
    expect(dialog).toBeInTheDocument();

    // 对话框中只有名称输入，没有 Key 值输入
    expect(within(dialog).queryByLabelText("Key")).not.toBeInTheDocument();

    const saveButton = within(dialog).getByRole("button", { name: "Save" });
    expect(saveButton).toBeDisabled();

    fireEvent.change(within(dialog).getByLabelText("Name"), {
      target: { value: "CI" },
    });
    expect(saveButton).toBeEnabled();
    fireEvent.click(saveButton);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_upsert_key", {
        key: expect.objectContaining({ label: "CI", value: "" }),
      }),
    );

    // 值留空交由后端随机生成，其余字段按约定初始化
    const call = invokeMock.mock.calls.find(
      ([command]) => command === "ai_gateway_upsert_key",
    );
    expect(call?.[1]).toEqual({
      key: {
        id: "",
        label: "CI",
        value: "",
        enabled: true,
        created_at: 0,
      },
    });
  });

  it("取消新增 Key 弹框后重新打开会清空名称输入", async () => {
    const store: Store = {
      config: makeConfig({ keys: [], default_key_id: null }),
      status: makeStatus({ key_count: 0, default_key_id: null }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);
    await screen.findByTestId("ai-gateway-keys");

    fireEvent.click(screen.getByRole("button", { name: /Add key/ }));
    const dialog = await screen.findByTestId("ai-gateway-key-dialog");
    fireEvent.change(within(dialog).getByLabelText("Name"), {
      target: { value: "CI" },
    });
    expect(within(dialog).getByLabelText("Name")).toHaveValue("CI");

    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() =>
      expect(
        screen.queryByTestId("ai-gateway-key-dialog"),
      ).not.toBeInTheDocument(),
    );

    fireEvent.click(screen.getByRole("button", { name: /Add key/ }));
    const reopened = await screen.findByTestId("ai-gateway-key-dialog");
    expect(within(reopened).getByLabelText("Name")).toHaveValue("");
  });

  it("在名称输入中按 Enter 会提交新增 Key 且载荷初始化字段正确", async () => {
    const store: Store = {
      config: makeConfig({ keys: [], default_key_id: null }),
      status: makeStatus({ key_count: 0, default_key_id: null }),
      targets: [],
    };
    invokeMock.mockImplementation(async (command: string) => {
      switch (command) {
        case "ai_gateway_get_config":
        case "ai_gateway_upsert_key":
          return store.config;
        case "ai_gateway_status":
          return store.status;
        case "ai_gateway_terminal_targets":
          return store.targets;
        default:
          throw new Error(`Unhandled command: ${command}`);
      }
    });

    renderWithProviders(<AiGateway />);
    await screen.findByTestId("ai-gateway-keys");

    fireEvent.click(screen.getByRole("button", { name: /Add key/ }));
    const dialog = await screen.findByTestId("ai-gateway-key-dialog");
    const nameInput = within(dialog).getByLabelText("Name");
    fireEvent.change(nameInput, { target: { value: "CI" } });
    fireEvent.keyDown(nameInput, { key: "Enter" });

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_upsert_key", {
        key: {
          id: "",
          label: "CI",
          value: "",
          enabled: true,
          created_at: 0,
        },
      }),
    );
  });

  it("保存新 Key 失败时弹框保持打开并保留名称输入", async () => {
    const store: Store = {
      config: makeConfig({ keys: [], default_key_id: null }),
      status: makeStatus({ key_count: 0, default_key_id: null }),
      targets: [],
    };
    invokeMock.mockImplementation(async (command: string) => {
      switch (command) {
        case "ai_gateway_get_config":
          return store.config;
        case "ai_gateway_upsert_key":
          throw new Error("boom");
        case "ai_gateway_status":
          return store.status;
        case "ai_gateway_terminal_targets":
          return store.targets;
        default:
          throw new Error(`Unhandled command: ${command}`);
      }
    });

    renderWithProviders(<AiGateway />);
    await screen.findByTestId("ai-gateway-keys");

    fireEvent.click(screen.getByRole("button", { name: /Add key/ }));
    const dialog = await screen.findByTestId("ai-gateway-key-dialog");
    fireEvent.change(within(dialog).getByLabelText("Name"), {
      target: { value: "CI" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(screen.getByTestId("ai-gateway-key-dialog")).toBeInTheDocument(),
    );
    expect(
      within(screen.getByTestId("ai-gateway-key-dialog")).getByLabelText("Name"),
    ).toHaveValue("CI");
  });

  it("支持在 Tabs 之间顺畅切换且保持各面板挂载与状态", async () => {
    const store: Store = {
      config: makeConfig({
        providers: [makeProvider()],
        keys: [{ id: "k1", label: "Dev Key", value: "********", enabled: true, created_at: 1 }],
      }),
      status: makeStatus({ provider_count: 1, key_count: 1 }),
      targets: [openCodeTarget()],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);
    await within(
      await screen.findByTestId("ai-gateway-providers"),
    ).findByText("Upstream A");

    const tabsList = screen.getByRole("tablist", { name: /AI Gateway tabs/i });
    expect(tabsList).toBeInTheDocument();

    const keysTab = screen.getByRole("tab", { name: /API Keys|API 密钥/i });
    const terminalsTab = screen.getByRole("tab", { name: /AI terminal integration/i });
    const providersTab = screen.getByRole("tab", { name: /Upstream providers/i });

    expect(providersTab).toHaveAttribute("aria-selected", "true");
    expect(keysTab).toHaveAttribute("aria-selected", "false");

    fireEvent.click(keysTab);
    expect(keysTab).toHaveAttribute("aria-selected", "true");
    expect(providersTab).toHaveAttribute("aria-selected", "false");

    fireEvent.click(terminalsTab);
    expect(terminalsTab).toHaveAttribute("aria-selected", "true");
  });

  it("用量与日志页签可达且用量统计与请求日志二级切换后保留各自状态，且不再提供模型价格入口", async () => {
    const store: Store = {
      config: makeConfig({
        providers: [makeProvider()],
        keys: [{ id: "k1", label: "Dev Key", value: "********", enabled: true, created_at: 1 }],
      }),
      status: makeStatus({ provider_count: 1, key_count: 1 }),
      targets: [openCodeTarget()],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);
    await within(
      await screen.findByTestId("ai-gateway-providers"),
    ).findByText("Upstream A");

    const tabsList = screen.getByRole("tablist", { name: /AI Gateway tabs/i });
    const usageTab = within(tabsList).getByRole("tab", { name: /Usage & Logs/i });
    const providersTab = within(tabsList).getByRole("tab", {
      name: /Upstream providers/,
    });

    fireEvent.click(usageTab);
    expect(usageTab).toHaveAttribute("aria-selected", "true");

    // 默认展示用量统计二级子页
    const statsSubTab = screen.getByTestId("ai-gateway-subtab-usage-stats");
    const logsSubTab = screen.getByTestId("ai-gateway-subtab-usage-logs");
    expect(statsSubTab).toHaveAttribute("aria-selected", "true");
    expect(logsSubTab).toHaveAttribute("aria-selected", "false");

    const usagePanel = await screen.findByTestId("ai-gateway-usage-stats");
    await within(usagePanel).findByTestId("ai-gateway-usage-card-requests");
    fireEvent.click(within(usagePanel).getByTestId("ai-gateway-usage-range-trigger"));
    fireEvent.click(screen.getByRole("option", { name: "7d" }));
    await within(usagePanel).findByTestId("ai-gateway-usage-card-requests");

    // The legacy model-price entry must not exist anywhere in the usage panel.
    expect(
      within(usagePanel).queryByRole("button", { name: "Model prices" }),
    ).not.toBeInTheDocument();

    // 切换到请求日志二级子页
    fireEvent.click(logsSubTab);
    expect(logsSubTab).toHaveAttribute("aria-selected", "true");
    expect(statsSubTab).toHaveAttribute("aria-selected", "false");

    await screen.findByTestId("ai-gateway-logs-ungrouped");
    const logsPanel = screen.getByTestId("ai-gateway-usage-logs");
    expect(
      within(logsPanel).queryByRole("button", { name: "Model prices" }),
    ).not.toBeInTheDocument();
    fireEvent.click(within(logsPanel).getByRole("button", { name: "Next" }));
    await screen.findByText("Page 2 / 3");

    // Switch away and back: usage range and logs page must survive.
    fireEvent.click(providersTab);
    fireEvent.click(usageTab);
    fireEvent.click(statsSubTab);
    expect(
      within(usagePanel).getByTestId("ai-gateway-usage-range-trigger"),
    ).toHaveTextContent("7d");

    fireEvent.click(logsSubTab);
    await screen.findByText("Page 2 / 3");

    // Grouping selection also survives a tab round-trip.
    fireEvent.click(
      within(logsPanel).getByTestId("ai-gateway-logs-group-trigger"),
    );
    fireEvent.click(screen.getByRole("option", { name: "Day (UTC+8)" }));
    await screen.findByTestId("ai-gateway-logs-grouped");
    fireEvent.click(providersTab);
    fireEvent.click(usageTab);
    fireEvent.click(logsSubTab);
    expect(
      within(logsPanel).getByTestId("ai-gateway-logs-group-trigger"),
    ).toHaveTextContent("Day (UTC+8)");
    // Flush the re-activation reloads before the test unmounts.
    await act(async () => {});
  });

  it("点击添加服务商打开弹框并成功保存", async () => {
    const store: Store = {
      config: makeConfig({ providers: [] }),
      status: makeStatus({ provider_count: 0 }),
      targets: [],
    };
    invokeMock.mockImplementation(async (command: string, args: any) => {
      switch (command) {
        case "ai_gateway_get_config":
          return store.config;
        case "ai_gateway_status":
          return store.status;
        case "ai_gateway_terminal_targets":
          return store.targets;
        case "ai_gateway_upsert_provider":
          store.config = {
            ...store.config,
            providers: [...store.config.providers, { ...args.provider, id: "p-new" }],
          };
          return store.config;
        default:
          throw new Error(`Unhandled command: ${command}`);
      }
    });

    renderWithProviders(<AiGateway />);
    await screen.findByTestId("ai-gateway-providers");

    const addButtons = screen.getAllByRole("button", { name: /Add provider/i });
    fireEvent.click(addButtons[0]);

    const blankBtn = await screen.findByTestId("template-picker-blank-btn");
    fireEvent.click(blankBtn);

    const dialog = await screen.findByTestId("ai-gateway-provider-detail");
    expect(dialog).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("Name"), { target: { value: "New Remote" } });
    fireEvent.change(screen.getByLabelText("API base URL"), {
      target: { value: "https://new.example.com" },
    });

    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_upsert_provider", {
        provider: expect.objectContaining({
          name: "New Remote",
          base_url: "https://new.example.com",
        }),
        prices: [],
      }),
    );
  });

  it("启停按钮在服务运行时显示停止红色方块按钮", async () => {
    const store: Store = {
      config: makeConfig(),
      status: makeStatus({ running: true }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);
    const runningToggleBtn = await screen.findByTestId("ai-gateway-toggle-service");
    expect(runningToggleBtn).toHaveAttribute("title", "Stop service");
    expect(runningToggleBtn).toHaveClass("text-destructive");
  });

  it("启停按钮在服务停止时显示启动图标按钮", async () => {
    const store: Store = {
      config: makeConfig(),
      status: makeStatus({ running: false }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);
    const stoppedToggleBtn = await screen.findByTestId("ai-gateway-toggle-service");
    expect(stoppedToggleBtn).toHaveAttribute("title", "Start service");
    expect(stoppedToggleBtn).toHaveClass("text-emerald-600");
  });

  it("启停服务成功后广播 ai-gateway-status-update 事件", async () => {
    const store: Store = {
      config: makeConfig(),
      status: makeStatus({ running: false }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);
    const toggleBtn = await screen.findByTestId("ai-gateway-toggle-service");
    fireEvent.click(toggleBtn);

    await waitFor(() => {
      expect(emitMock).toHaveBeenCalledWith("ai-gateway-status-update");
    });
  });

  it("顶部状态卡片展示健康率、聚合模型数、有效密钥及终端同步指标，且点击指标卡可切换对应 Tab", async () => {
    const provider1 = makeProvider({
      id: "p1",
      name: "Provider 1",
      enabled: true,
      mappings: [
        { local_model: "gpt-4o", upstream_model: "gpt-4o-2024" },
        { local_model: "claude-3-7-sonnet", upstream_model: "claude-3-7" },
      ],
      default_model: "gpt-4o",
    });
    const provider2 = makeProvider({
      id: "p2",
      name: "Provider 2",
      enabled: false,
      mappings: [
        { local_model: "deepseek-v3", upstream_model: "deepseek-chat" },
      ],
    });

    const store: Store = {
      config: makeConfig({
        providers: [provider1, provider2],
        keys: [
          { id: "k1", label: "Key 1", value: "sk-1", enabled: true, created_at: 1 },
          { id: "k2", label: "Key 2", value: "sk-2", enabled: false, created_at: 2 },
        ],
      }),
      status: makeStatus({
        running: true,
        provider_count: 2,
        key_count: 2,
      }),
      targets: [
        openCodeTarget({ tool: "opencode", name: "OpenCode", synced: true, pending_sync: false }),
        openCodeTarget({ tool: "codex", name: "Codex", synced: false, pending_sync: true }),
      ],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);

    // 1. 服务商健康率展示：1/2 在线 (因为 provider2 是 disabled)
    const healthCard = await screen.findByTestId("ai-gateway-metric-health");
    expect(within(healthCard).getByText("1/2")).toBeInTheDocument();
    expect(within(healthCard).getByText(/Provider health/i)).toBeInTheDocument();

    // 2. 聚合模型数：provider1 贡献了 gpt-4o 和 claude-3-7-sonnet（去重后 2 个，provider2 被禁用不计入）
    const modelsCard = screen.getByTestId("ai-gateway-metric-models");
    expect(within(modelsCard).getByText("2")).toBeInTheDocument();
    expect(within(modelsCard).getByText(/Aggregated models/i)).toBeInTheDocument();

    // 3. 今日请求量指标卡（高频业务）：显示 1 次请求及失败告警 (mock 返回 error_count: 1)
    const requestsCard = await screen.findByTestId("ai-gateway-metric-requests");
    expect(within(requestsCard).getByText("1")).toBeInTheDocument();
    expect(within(requestsCard).getByText(/Today's requests|今日请求/i)).toBeInTheDocument();
    expect(
      within(requestsCard).getByTestId("ai-gateway-today-failed-requests"),
    ).toHaveTextContent(/1.*failed|1.*失败/i);

    // 4. 今日消耗指标卡（高频消耗）：显示 2 Tokens
    const tokensCard = await screen.findByTestId("ai-gateway-metric-tokens");
    expect(within(tokensCard).getByText("2")).toBeInTheDocument();
    expect(within(tokensCard).getByText(/Today's tokens|今日消耗/i)).toBeInTheDocument();

    // 5. 协议端点栏提供 Anthropic Messages 协议地址与复制入口
    expect(screen.getByTestId("ai-gateway-anthropic-address")).toHaveTextContent(
      "http://127.0.0.1:17688/v1/messages",
    );
    expect(
      screen.getByTestId("ai-gateway-copy-anthropic-address"),
    ).toBeInTheDocument();

    // 6. 终端同步状态转移到终端集成 Tab 徽标：展示 1/2 已同步
    const terminalsTab = screen.getByRole("tab", { name: /AI terminal integration/i });
    expect(within(terminalsTab).getByText("1/2")).toBeInTheDocument();

    // 7. 点击今日请求量指标卡可切换到 usage Tab
    fireEvent.click(requestsCard);
    const usageTab = screen.getByRole("tab", { name: /Usage & Logs|用量与日志/i });
    expect(usageTab).toHaveAttribute("aria-selected", "true");

    // 8. 点击聚合模型指标卡可切换到 models Tab
    fireEvent.click(modelsCard);
    const modelsTab = screen.getByRole("tab", { name: /Model list|模型列表/i });
    expect(modelsTab).toHaveAttribute("aria-selected", "true");
    expect(screen.getByTestId("ai-gateway-model-list")).toBeInTheDocument();
  });

  it("今日失败数取自已加载的统计总量，手动刷新取得新数据", async () => {
    const store: Store = {
      config: makeConfig({ providers: [makeProvider({ id: "p1", name: "Upstream A" })] }),
      status: makeStatus({ running: true, provider_count: 1 }),
      targets: [],
    };
    mockStore(store);
    const base = invokeMock.getMockImplementation()!;
    let stats = emptyUsageStats({
      request_count: 5,
      successful_request_count: 3,
    });
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "ai_gateway_usage_stats") return stats;
      return base(command, args);
    });

    renderWithProviders(<AiGateway />);
    const requestsCard = await screen.findByTestId("ai-gateway-metric-requests");
    expect(within(requestsCard).getByText("5")).toBeInTheDocument();
    expect(
      within(requestsCard).getByTestId("ai-gateway-today-failed-requests"),
    ).toHaveTextContent("2 failed");

    const statsCalls = () =>
      invokeMock.mock.calls.filter(
        ([command]) => command === "ai_gateway_usage_stats",
      ).length;
    expect(statsCalls()).toBe(1);

    stats = emptyUsageStats({
      request_count: 8,
      successful_request_count: 5,
    });
    fireEvent.click(screen.getByTestId("ai-gateway-refresh-btn"));
    await waitFor(() =>
      expect(within(requestsCard).getByText("8")).toBeInTheDocument(),
    );
    await waitFor(() =>
      expect(
        within(requestsCard).getByTestId("ai-gateway-today-failed-requests"),
      ).toHaveTextContent("3 failed"),
    );
    expect(statsCalls()).toBe(2);
  });

  it("头部今日用量刷新不额外发起分组日志查询", async () => {
    const store: Store = {
      config: makeConfig({ providers: [makeProvider({ id: "p1", name: "Upstream A" })] }),
      status: makeStatus({ running: true, provider_count: 1 }),
      targets: [],
    };
    mockStore(store);
    const base = invokeMock.getMockImplementation()!;
    let stats = emptyUsageStats({
      request_count: 5,
      successful_request_count: 3,
    });
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "ai_gateway_usage_stats") return stats;
      if (command === "ai_gateway_request_logs") {
        return usageLogsPage({
          group_by: "day",
          records: [],
          groups: [
            {
              group: "2026-09-17",
              request_count: 9,
              error_count: 9,
              last_request_at_ms: 1_700_000_000_000,
            },
          ],
        });
      }
      return base(command, args);
    });

    const groupedLogCalls = () =>
      invokeMock.mock.calls.filter(
        ([command, args]) =>
          command === "ai_gateway_request_logs" &&
          (args as Record<string, unknown> | undefined)?.groupBy === "day",
      ).length;

    renderWithProviders(<AiGateway />);
    await screen.findByTestId("ai-gateway-metric-requests");
    expect(groupedLogCalls()).toBe(0);

    stats = emptyUsageStats({
      request_count: 8,
      successful_request_count: 5,
    });
    fireEvent.click(screen.getByTestId("ai-gateway-refresh-btn"));
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.filter(
          ([command]) => command === "ai_gateway_usage_stats",
        ).length,
      ).toBe(2),
    );
    expect(groupedLogCalls()).toBe(0);
  });

  it("首次访问前不构造用量统计、请求日志与历史面板，访问后才挂载", async () => {
    const store: Store = {
      config: makeConfig({ providers: [makeProvider({ id: "p1", name: "Upstream A" })] }),
      status: makeStatus({ running: true, provider_count: 1 }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);
    await screen.findByTestId("ai-gateway-providers");

    expect(screen.queryByTestId("ai-gateway-usage-stats")).not.toBeInTheDocument();
    expect(screen.queryByTestId("ai-gateway-usage-logs")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("ai-gateway-provider-usage-history"),
    ).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: /Usage & Logs/i }));
    expect(await screen.findByTestId("ai-gateway-usage-stats")).toBeInTheDocument();
    expect(screen.queryByTestId("ai-gateway-usage-logs")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("ai-gateway-provider-usage-history"),
    ).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId("ai-gateway-subtab-usage-logs"));
    expect(await screen.findByTestId("ai-gateway-usage-logs")).toBeInTheDocument();

    fireEvent.click(screen.getByTestId("ai-gateway-subtab-usage-history"));
    expect(
      await screen.findByTestId("ai-gateway-provider-usage-history"),
    ).toBeInTheDocument();
  });

  it("未访问的模型列表、模型池与终端页签不构造面板，首次访问后保持挂载", async () => {
    const store: Store = {
      config: makeConfig({
        providers: [
          makeProvider({
            id: "p1",
            name: "Upstream A",
            mappings: [
              {
                local_model: "gpt-4o",
                upstream_model: "gpt-4o-2024",
                enabled: true,
              },
            ],
          }),
        ],
        model_pools: [modelPoolFixture()],
      }),
      status: makeStatus({ running: true, provider_count: 1 }),
      targets: [openCodeTarget()],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);
    await screen.findByTestId("ai-gateway-providers");

    // 默认服务商页签正常构造。
    expect(screen.getByTestId("ai-gateway-providers")).toBeInTheDocument();
    // 未访问的重面板不在 DOM 中构造。
    expect(screen.queryByTestId("ai-gateway-model-list")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("ai-gateway-model-pools-panel"),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId("ai-gateway-terminals")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: /Model pools/i }));
    expect(
      await screen.findByTestId("ai-gateway-model-pools-panel"),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("ai-gateway-model-list")).not.toBeInTheDocument();
    expect(screen.queryByTestId("ai-gateway-terminals")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: /Model list/i }));
    expect(await screen.findByTestId("ai-gateway-model-list")).toBeInTheDocument();
    expect(screen.queryByTestId("ai-gateway-terminals")).not.toBeInTheDocument();

    fireEvent.click(
      screen.getByRole("tab", { name: /AI terminal integration/i }),
    );
    expect(await screen.findByTestId("ai-gateway-terminals")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: /Upstream providers/i }));
    // 已访问面板保持挂载，状态得以保留。
    expect(
      screen.getByTestId("ai-gateway-model-pools-panel"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("ai-gateway-model-list")).toBeInTheDocument();
    expect(screen.getByTestId("ai-gateway-terminals")).toBeInTheDocument();
  });

  it("已访问面板在头部刷新与配置事件后保留筛选状态", async () => {
    const store: Store = {
      config: makeConfig({
        providers: [
          makeProvider({
            id: "p1",
            name: "Upstream A",
            mappings: [
              {
                local_model: "gpt-4o",
                upstream_model: "gpt-4o-2024",
                enabled: true,
              },
            ],
          }),
        ],
      }),
      status: makeStatus({ running: true, provider_count: 1 }),
      targets: [],
    };
    mockStore(store);

    const configUpdateHandlers: Array<(event: unknown) => void> = [];
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
          configUpdateHandlers.push(handler);
        }
        return vi.fn();
      },
    );

    renderWithProviders(<AiGateway />);
    await screen.findByTestId("ai-gateway-providers");

    fireEvent.click(screen.getByRole("tab", { name: /Model list/i }));
    const search = await screen.findByTestId("ai-gateway-model-list-search");
    fireEvent.change(search, { target: { value: "gpt" } });
    expect(screen.getAllByTestId("ai-gateway-model-list-row")).toHaveLength(1);

    // 面板转到后台，头部刷新与配置事件都不应重建它或清空筛选。
    fireEvent.click(screen.getByRole("tab", { name: /Upstream providers/i }));
    fireEvent.click(screen.getByTestId("ai-gateway-refresh-btn"));
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.filter(
          ([command]) => command === "ai_gateway_usage_stats",
        ).length,
      ).toBeGreaterThan(0),
    );

    store.config = makeConfig({
      providers: [
        makeProvider({
          id: "p1",
          name: "Upstream A",
          mappings: [
            {
              local_model: "gpt-4o",
              upstream_model: "gpt-4o-2024",
              enabled: true,
            },
          ],
        }),
        makeProvider({
          id: "p2",
          name: "Upstream B",
          mappings: [
            {
              local_model: "claude-3-7-sonnet",
              upstream_model: "claude-3-7",
              enabled: true,
            },
          ],
        }),
      ],
    });
    await act(async () => {
      for (const handler of configUpdateHandlers) {
        handler({ payload: undefined });
      }
      await Promise.resolve();
    });
    await waitFor(() =>
      expect(screen.getByTestId("ai-gateway-providers")).toHaveTextContent(
        "Upstream B",
      ),
    );

    fireEvent.click(screen.getByRole("tab", { name: /Model list/i }));
    const list = await screen.findByTestId("ai-gateway-model-list");
    expect(within(list).getByTestId("ai-gateway-model-list-search")).toHaveValue(
      "gpt",
    );
    const rows = within(list).getAllByTestId("ai-gateway-model-list-row");
    expect(rows).toHaveLength(1);
    expect(rows[0]).toHaveAttribute("data-model", "gpt-4o");
  });

  it("点击聚合模型指标卡切换到模型列表Tab并列出有效模型及其上游服务商映射", async () => {
    const provider1 = makeProvider({
      id: "p1",
      name: "Provider 1",
      protocol: "chat_completions",
      enabled: true,
      default_model: "gpt-4o",
      mappings: [
        { local_model: "gpt-4o", upstream_model: "gpt-4o-2024" },
        { local_model: "claude-3-7-sonnet", upstream_model: "claude-3-7" },
      ],
    });
    const provider2 = makeProvider({
      id: "p2",
      name: "Provider 2",
      enabled: false,
      mappings: [
        { local_model: "deepseek-v3", upstream_model: "deepseek-chat" },
      ],
    });

    const store: Store = {
      config: makeConfig({ providers: [provider1, provider2] }),
      status: makeStatus({ provider_count: 2 }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);

    const modelsCard = await screen.findByTestId("ai-gateway-metric-models");
    const cardModelCount = Number(within(modelsCard).getByText("2").textContent);
    fireEvent.click(modelsCard);

    const modelsTab = screen.getByRole("tab", { name: /Model list|模型列表/i });
    expect(modelsTab).toHaveAttribute("aria-selected", "true");

    const panel = await screen.findByTestId("ai-gateway-model-list");

    const modelNodes = within(panel).getAllByTestId("ai-gateway-model-list-row");
    expect(modelNodes).toHaveLength(2);
    expect(modelNodes).toHaveLength(cardModelCount);

    const modelIds = within(panel)
      .getAllByTestId("ai-gateway-model-list-id")
      .map((node) => node.textContent?.trim());
    expect(modelIds.sort()).toEqual(["claude-3-7-sonnet", "gpt-4o"]);

    expect(
      within(panel).getAllByTestId("ai-gateway-model-list-upstream"),
    ).toHaveLength(2);

    expect(within(panel).getAllByText("Provider 1").length).toBeGreaterThan(0);
    expect(within(panel).getAllByText("gpt-4o-2024").length).toBeGreaterThan(0);
    expect(within(panel).getAllByText("claude-3-7").length).toBeGreaterThan(0);
    expect(within(panel).queryAllByText("deepseek-v3")).toHaveLength(0);
    expect(within(panel).queryAllByText("Provider 2")).toHaveLength(0);
  });

  it("点击聚合模型指标卡切换到模型列表Tab并以路径形式展示 endpoint 而非协议枚举", async () => {
    const provider1 = makeProvider({
      id: "p1",
      name: "Provider 1",
      protocol: "chat_completions",
      enabled: true,
      default_model: "gpt-4o",
      mappings: [
        { local_model: "gpt-4o", upstream_model: "gpt-4o-2024" },
        { local_model: "claude-3-7-sonnet", upstream_model: "claude-3-7" },
      ],
    });

    const store: Store = {
      config: makeConfig({ providers: [provider1] }),
      status: makeStatus({ provider_count: 1 }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);

    fireEvent.click(await screen.findByTestId("ai-gateway-metric-models"));

    const panel = await screen.findByTestId("ai-gateway-model-list");
    // 2 个模型在第二列协议徽章与第三列上游服务商徽标各渲染 1 次，共 4 处
    expect(within(panel).getAllByText("/chat/completions")).toHaveLength(4);
    expect(within(panel).queryAllByText("chat_completions")).toHaveLength(0);
  });

  it("在聚合模型指标卡上按 Enter 切换到模型列表Tab", async () => {
    const store: Store = {
      config: makeConfig({ providers: [makeProvider()] }),
      status: makeStatus({ provider_count: 1 }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);

    const keysTab = await screen.findByRole("tab", { name: /API Keys|API 密钥/i });
    fireEvent.click(keysTab);
    expect(keysTab).toHaveAttribute("aria-selected", "true");

    fireEvent.keyDown(screen.getByTestId("ai-gateway-metric-models"), {
      key: "Enter",
    });

    const modelsTab = screen.getByRole("tab", { name: /Model list|模型列表/i });
    expect(modelsTab).toHaveAttribute("aria-selected", "true");
    expect(await screen.findByTestId("ai-gateway-model-list")).toBeInTheDocument();
  });

  it("在聚合模型指标卡上按空格切换到模型列表Tab", async () => {
    const store: Store = {
      config: makeConfig({ providers: [makeProvider()] }),
      status: makeStatus({ provider_count: 1 }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);

    const keysTab = await screen.findByRole("tab", { name: /API Keys|API 密钥/i });
    fireEvent.click(keysTab);
    expect(keysTab).toHaveAttribute("aria-selected", "true");

    fireEvent.keyDown(screen.getByTestId("ai-gateway-metric-models"), {
      key: " ",
    });

    const modelsTab = screen.getByRole("tab", { name: /Model list|模型列表/i });
    expect(modelsTab).toHaveAttribute("aria-selected", "true");
    expect(await screen.findByTestId("ai-gateway-model-list")).toBeInTheDocument();
  });

  it("没有启用服务商时点击聚合模型指标卡切换到模型列表展示空态且不渲染任何模型", async () => {
    const disabledProvider = makeProvider({
      id: "p1",
      name: "Provider 1",
      enabled: false,
      default_model: "gpt-4o",
      mappings: [{ local_model: "gpt-4o", upstream_model: "gpt-4o-2024" }],
    });

    const store: Store = {
      config: makeConfig({ providers: [disabledProvider] }),
      status: makeStatus({ provider_count: 1 }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);

    fireEvent.click(await screen.findByTestId("ai-gateway-metric-models"));

    const panel = await screen.findByTestId("ai-gateway-model-list");
    expect(
      within(panel).getByTestId("ai-gateway-model-list-empty"),
    ).toBeInTheDocument();
    expect(
      within(panel).queryAllByTestId("ai-gateway-model-list-row"),
    ).toHaveLength(0);
  });

  it("保存服务商时将映射行启用状态写入载荷", async () => {
    const store: Store = {
      config: makeConfig({
        providers: [
          makeProvider({
            mappings: [
              { local_model: "local-a", upstream_model: "remote-a" },
              { local_model: "local-b", upstream_model: "remote-b" },
            ],
          }),
        ],
      }),
      status: makeStatus({ provider_count: 1 }),
      targets: [openCodeTarget()],
    };
    mockStoreWithUpsert(store);

    renderWithProviders(<AiGateway />);
    fireEvent.click(
      await within(
        await screen.findByTestId("ai-gateway-providers"),
      ).findByText("Upstream A"),
    );

    fireEvent.click(screen.getByRole("switch", { name: "Enable mapping 2" }));
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "ai_gateway_upsert_provider",
        expect.objectContaining({
          provider: expect.objectContaining({ id: "p1" }),
        }),
      ),
    );

    const call = invokeMock.mock.calls.find(
      ([command]) => command === "ai_gateway_upsert_provider",
    );
    const payload = call?.[1] as { provider: GatewayUpstreamProvider };
    expect(
      payload.provider.mappings[0].enabled,
      "未切换的第 1 条映射应写入启用",
    ).toBe(true);
    expect(
      payload.provider.mappings[1].enabled,
      "被关闭的第 2 条映射应写入禁用",
    ).toBe(false);
  });

  it("点击聚合模型指标卡切换到模型列表排除禁用映射", async () => {
    const provider1 = makeProvider({
      id: "p1",
      name: "Provider 1",
      enabled: true,
      default_model: "d",
      mappings: [
        { local_model: "a", upstream_model: "ra", enabled: true },
        { local_model: "b", upstream_model: "rb", enabled: false },
      ],
    });

    const store: Store = {
      config: makeConfig({ providers: [provider1] }),
      status: makeStatus({ provider_count: 1 }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);

    fireEvent.click(await screen.findByTestId("ai-gateway-metric-models"));

    const panel = await screen.findByTestId("ai-gateway-model-list");
    const modelRows = within(panel).getAllByTestId(
      "ai-gateway-model-list-row",
    );
    const modelNames = modelRows.map(
      (node) =>
        within(node).getByTestId("ai-gateway-model-list-id").textContent?.trim(),
    );
    expect(modelNames, "模型列表应包含启用映射 a").toContain("a");
    expect(modelNames, "模型列表应排除未映射的默认模型 d").not.toContain("d");
    expect(modelNames, "模型列表应排除禁用映射 b").not.toContain("b");
    expect(within(panel).queryByText("rb")).not.toBeInTheDocument();
  });

  it("模型列表页签紧随上游服务商、徽标数与指标卡一致，且搜索状态在页签往返后保留", async () => {
    const provider1 = makeProvider({
      id: "p1",
      name: "Provider 1",
      enabled: true,
      default_model: "gpt-4o",
      mappings: [
        { local_model: "gpt-4o", upstream_model: "gpt-4o-2024" },
        { local_model: "claude-3-7-sonnet", upstream_model: "claude-3-7" },
      ],
    });
    const provider2 = makeProvider({
      id: "p2",
      name: "Provider 2",
      enabled: true,
      mappings: [
        { local_model: "deepseek-v3", upstream_model: "deepseek-chat" },
      ],
    });

    const store: Store = {
      config: makeConfig({ providers: [provider1, provider2] }),
      status: makeStatus({ provider_count: 2 }),
      targets: [openCodeTarget()],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);

    const modelsCard = await screen.findByTestId("ai-gateway-metric-models");
    const metricCount = Number(within(modelsCard).getByText("3").textContent);
    expect(metricCount).toBe(3);

    const tabsList = screen.getByRole("tablist", {
      name: /AI Gateway tabs/i,
    });
    const tabNodes = within(tabsList).getAllByRole("tab");
    const providersIndex = tabNodes.findIndex((node) =>
      (node.textContent ?? "").includes("Upstream providers"),
    );
    const modelPoolsIndex = tabNodes.findIndex((node) =>
      (node.textContent ?? "").includes("Model pools"),
    );
    const modelsIndex = tabNodes.findIndex((node) =>
      (node.textContent ?? "").includes("Model list"),
    );
    expect(providersIndex).toBeGreaterThanOrEqual(0);
    expect(modelPoolsIndex).toBe(providersIndex + 1);
    expect(modelsIndex).toBe(modelPoolsIndex + 1);

    const modelsTab = tabNodes[modelsIndex];
    const modelsTabBadge = within(modelsTab).getByText("3");
    expect(modelsTabBadge.textContent).toBe(String(metricCount));

    fireEvent.click(modelsTab);
    expect(modelsTab).toHaveAttribute("aria-selected", "true");

    const panel = screen.getByRole("tabpanel", { name: "Model list" });
    const rows = within(panel).getAllByTestId("ai-gateway-model-list-row");
    expect(rows).toHaveLength(metricCount);

    const search = within(panel).getByTestId("ai-gateway-model-list-search");
    fireEvent.change(search, { target: { value: "gpt" } });
    const filteredRows = within(panel).getAllByTestId(
      "ai-gateway-model-list-row",
    );
    expect(filteredRows).toHaveLength(1);
    expect(filteredRows[0]).toHaveAttribute("data-model", "gpt-4o");

    const providersTab = within(tabsList).getByRole("tab", {
      name: /Upstream providers/,
    });
    fireEvent.click(providersTab);
    fireEvent.click(modelsTab);

    const panelAfterRoundTrip = screen.getByRole("tabpanel", {
      name: "Model list",
    });
    expect(
      within(panelAfterRoundTrip).getByTestId("ai-gateway-model-list-search"),
    ).toHaveValue("gpt");
    const rowsAfterRoundTrip = within(panelAfterRoundTrip).getAllByTestId(
      "ai-gateway-model-list-row",
    );
    expect(rowsAfterRoundTrip).toHaveLength(1);
    expect(rowsAfterRoundTrip[0]).toHaveAttribute("data-model", "gpt-4o");
  });

  function makeTemplateModel(
    overrides: Partial<GatewayProviderTemplateModel> = {},
  ): GatewayProviderTemplateModel {
    return {
      upstream_model: "deepseek-chat",
      display_name: "DeepSeek Chat",
      protocol: "chat_completions",
      enabled: true,
      ...overrides,
    };
  }

  function makeTemplate(
    overrides: Partial<GatewayProviderTemplate> = {},
  ): GatewayProviderTemplate {
    return {
      id: "t1",
      name: "OpenCode Zen",
      description: "Curated OpenCode models",
      base_url: "https://opencode.ai/zen/v1",
      protocol: "responses",
      source: "https://opencode.ai/zen/v1/models",
      models_url: "https://opencode.ai/zen/v1/models",
      models: [makeTemplateModel()],
      ...overrides,
    };
  }

  function makeTemplateView(
    overrides: Partial<GatewayProviderTemplateView> = {},
  ): GatewayProviderTemplateView {
    return {
      template: makeTemplate(),
      synced_at: null,
      source: "https://opencode.ai/zen/v1/models",
      from_snapshot: true,
      ...overrides,
    };
  }

  function mockStoreWithTemplates(
    store: Store,
    templates: GatewayProviderTemplateView[],
    options: { syncedAt?: number } = {},
  ) {
    mockStore(store);
    const read = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "ai_gateway_provider_templates") {
        return templates;
      }
      if (command === "ai_gateway_sync_provider_template") {
        const index = templates.findIndex(
          (entry) => entry.template.id === args?.templateId,
        );
        const current = index >= 0 ? templates[index] : templates[0];
        const updated: GatewayProviderTemplateView = {
          ...current,
          synced_at: options.syncedAt ?? 1_800_000_000,
          from_snapshot: false,
          source: "live:refreshed",
        };
        if (index >= 0) templates[index] = updated;
        return updated;
      }
      if (command === "ai_gateway_create_provider_from_template") {
        const provider = makeProvider({
          id: "p-new",
          name: args?.name,
          base_url: args?.baseUrl,
          protocol: args?.protocol,
          template_id: args?.templateId,
          default_model: null,
          mappings: [],
        });
        store.config = {
          ...store.config,
          providers: [...store.config.providers, provider],
        };
        return store.config;
      }
      return read(command, args);
    });
  }

  it("loadsProviderTemplatesAndRendersTemplateArea", async () => {
    const store: Store = {
      config: makeConfig(),
      status: makeStatus(),
      targets: [],
    };
    const templates = [
      makeTemplateView({
        template: makeTemplate({ id: "t1", name: "OpenCode Zen" }),
      }),
      makeTemplateView({
        template: makeTemplate({
          id: "t2",
          name: "CommandCode",
          description: "Official CommandCode models",
        }),
        synced_at: 1_700_000_000,
        from_snapshot: false,
        source: "snapshot:commandcode",
      }),
    ];
    mockStoreWithTemplates(store, templates);

    renderWithProviders(<AiGateway />);

    const manageBtn1 = (await screen.findAllByRole("button", { name: /Provider templates|服务商模板/i }))[0];
    fireEvent.click(manageBtn1);
    const region = await screen.findByTestId("ai-gateway-provider-templates");
    expect(region).toBeInTheDocument();
    expect(screen.getByTestId("ai-gateway-template-t1")).toBeInTheDocument();
    expect(screen.getByTestId("ai-gateway-template-t2")).toBeInTheDocument();
  });

  it("syncTemplateCallsCommandAndShowsSuccess", async () => {
    const syncedAt = 1_800_000_000;
    const store: Store = {
      config: makeConfig(),
      status: makeStatus(),
      targets: [],
    };
    const templates = [
      makeTemplateView({ template: makeTemplate({ id: "t1" }), synced_at: null }),
    ];
    mockStoreWithTemplates(store, templates, { syncedAt });

    renderWithProviders(<AiGateway />);

    const manageBtn2 = (await screen.findAllByRole("button", { name: /Provider templates|服务商模板/i }))[0];
    fireEvent.click(manageBtn2);
    const syncButton = await screen.findByTestId("ai-gateway-template-sync-t1");
    fireEvent.click(syncButton);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_sync_provider_template", {
        templateId: "t1",
      }),
    );

    // 同步成功后刷新出本次同步时间，且按钮恢复可用
    expect(await screen.findByText(formatGatewayTimestamp(syncedAt)!)).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByTestId("ai-gateway-template-sync-t1")).toBeEnabled(),
    );

    // AC-009：手动「同步模型列表」成功后绝不创建任何消息中心消息。
    // 手动处理器会为 UI 刷新读取 ai_gateway_get_config，但那不是通知。
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some(
          ([command]) => command === "ai_gateway_get_config",
        ),
      ).toBe(true),
    );
    expect(safeRecordMessageMock).not.toHaveBeenCalled();
    expect(recordMessageMock).not.toHaveBeenCalled();
  });

  it("syncTemplateRefreshesProviderConfiguration", async () => {
    const providerBefore = makeProvider({
      id: "p1",
      name: "Template Bound",
      template_id: "t1",
      mappings: [{ local_model: "remote-a", upstream_model: "remote-a" }],
    });
    const providerAfter = makeProvider({
      ...providerBefore,
      mappings: [
        { local_model: "remote-a", upstream_model: "remote-a" },
        { local_model: "new-model", upstream_model: "new-model" },
      ],
    });
    const store: Store = {
      config: makeConfig({ providers: [providerBefore] }),
      status: makeStatus({ provider_count: 1 }),
      targets: [],
    };
    const templates = [
      makeTemplateView({ template: makeTemplate({ id: "t1" }), synced_at: null }),
    ];

    let getConfigCalls = 0;
    invokeMock.mockImplementation(async (command: string, _args?: any) => {
      switch (command) {
        case "ai_gateway_get_config":
          getConfigCalls += 1;
          return store.config;
        case "ai_gateway_status":
          return store.status;
        case "ai_gateway_terminal_targets":
          return store.targets;
        case "ai_gateway_provider_templates":
          return templates;
        case "ai_gateway_sync_provider_template": {
          // 后端同步会把官方新模型增量传播到绑定服务商。
          store.config = { ...store.config, providers: [providerAfter] };
          return {
            ...templates[0],
            synced_at: 1_800_000_000,
            from_snapshot: false,
          };
        }
        default:
          throw new Error(`Unhandled command: ${command}`);
      }
    });

    renderWithProviders(<AiGateway />);

    const manageBtn3 = (await screen.findAllByRole("button", { name: /Provider templates|服务商模板/i }))[0];
    fireEvent.click(manageBtn3);
    const syncButton = await screen.findByTestId("ai-gateway-template-sync-t1");
    const callsBeforeSync = getConfigCalls;
    fireEvent.click(syncButton);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_sync_provider_template", {
        templateId: "t1",
      }),
    );

    // 同步后必须重新拉取配置，派生服务商的传播结果才能进入详情。
    await waitFor(() => expect(getConfigCalls).toBeGreaterThan(callsBeforeSync));

    fireEvent.click(
      within(screen.getByTestId("ai-gateway-providers")).getByText(
        "Template Bound",
      ),
    );
    await waitFor(() =>
      expect(
        within(screen.getByTestId("ai-gateway-provider-detail")).getByLabelText(
          "Upstream model 2",
        ),
      ).toHaveValue("new-model"),
    );
  });

  it("manual sync keeps the spinner and leaves the backend-owned failure snapshot until the update event clears it", async () => {
    const store: Store = {
      config: makeConfig(),
      status: makeStatus(),
      targets: [],
    };
    const templates = [
      makeTemplateView({ template: makeTemplate({ id: "t1" }), synced_at: null }),
    ];
    const sync = deferredPromise<GatewayProviderTemplateView>();

    invokeMock.mockImplementation(async (command: string, _args?: any) => {
      switch (command) {
        case "ai_gateway_get_config":
          return store.config;
        case "ai_gateway_status":
          return store.status;
        case "ai_gateway_terminal_targets":
          return store.targets;
        case "ai_gateway_provider_templates":
          return templates;
        case "ai_gateway_template_auto_refresh_status":
          return { failures: [{ template_id: "t1", reason: "old reason" }] };
        case "ai_gateway_sync_provider_template":
          return sync.promise;
        default:
          throw new Error(`Unhandled command: ${command}`);
      }
    });

    // The App-level adapter is the only writer of the frontend failure map.
    const adapter = renderHook(() => useTemplateAutoRefresh());
    renderWithProviders(<AiGateway />);

    const manageButton = (
      await screen.findAllByRole("button", {
        name: /Provider templates|服务商模板/i,
      })
    )[0];
    fireEvent.click(manageButton);

    // The inline failure comes from the backend snapshot, not a local setter.
    const failure = await screen.findByTestId(
      "ai-gateway-template-auto-refresh-failure-t1",
    );
    expect(failure).toHaveTextContent("old reason");

    fireEvent.click(await screen.findByTestId("ai-gateway-template-sync-t1"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "ai_gateway_sync_provider_template",
        { templateId: "t1" },
      ),
    );

    // 手动同步挂起期间，该模板的同步按钮保持禁用。
    expect(screen.getByTestId("ai-gateway-template-sync-t1")).toBeDisabled();

    await act(async () => {
      sync.resolve({
        ...templates[0],
        synced_at: 1_800_000_000,
        from_snapshot: false,
      });
      await sync.promise;
    });

    // 同步结束后按钮恢复，但手动路径不得改动后端拥有的失败快照。
    await waitFor(() =>
      expect(screen.getByTestId("ai-gateway-template-sync-t1")).toBeEnabled(),
    );
    expect(
      screen.getByTestId("ai-gateway-template-auto-refresh-failure-t1"),
    ).toHaveTextContent("old reason");

    // 只有后端快照事件才会清除失败原因。
    const handler = listenMock.mock.calls.find(
      ([event]) => event === "ai-gateway-template-auto-refresh-updated",
    )?.[1];
    expect(handler).toBeTypeOf("function");
    act(() => {
      handler?.({ payload: { failures: [] } });
    });
    await waitFor(() =>
      expect(
        screen.queryByTestId("ai-gateway-template-auto-refresh-failure-t1"),
      ).not.toBeInTheDocument(),
    );

    adapter.unmount();
  });

  it("manual sync rejection keeps the backend-owned failure snapshot and clears the spinner", async () => {
    const store: Store = {
      config: makeConfig(),
      status: makeStatus(),
      targets: [],
    };
    const templates = [
      makeTemplateView({ template: makeTemplate({ id: "t1" }), synced_at: null }),
    ];
    const sync = deferredPromise<GatewayProviderTemplateView>();

    invokeMock.mockImplementation(async (command: string, _args?: any) => {
      switch (command) {
        case "ai_gateway_get_config":
          return store.config;
        case "ai_gateway_status":
          return store.status;
        case "ai_gateway_terminal_targets":
          return store.targets;
        case "ai_gateway_provider_templates":
          return templates;
        case "ai_gateway_template_auto_refresh_status":
          return { failures: [{ template_id: "t1", reason: "old reason" }] };
        case "ai_gateway_sync_provider_template":
          return sync.promise;
        default:
          throw new Error(`Unhandled command: ${command}`);
      }
    });

    const adapter = renderHook(() => useTemplateAutoRefresh());
    renderWithProviders(<AiGateway />);

    const manageButton = (
      await screen.findAllByRole("button", {
        name: /Provider templates|服务商模板/i,
      })
    )[0];
    fireEvent.click(manageButton);

    await screen.findByTestId("ai-gateway-template-auto-refresh-failure-t1");

    fireEvent.click(await screen.findByTestId("ai-gateway-template-sync-t1"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "ai_gateway_sync_provider_template",
        { templateId: "t1" },
      ),
    );
    expect(screen.getByTestId("ai-gateway-template-sync-t1")).toBeDisabled();

    await act(async () => {
      sync.reject(new Error("manual sync failed"));
      await sync.promise.catch(() => undefined);
    });

    await waitFor(() =>
      expect(screen.getByTestId("ai-gateway-template-sync-t1")).toBeEnabled(),
    );
    // 手动同步失败不得清除已记录的后端自动刷新失败原因。
    expect(
      screen.getByTestId("ai-gateway-template-auto-refresh-failure-t1"),
    ).toHaveTextContent("old reason");

    // 通过后端快照事件清理共享 store，避免影响后续用例。
    const handler = listenMock.mock.calls.find(
      ([event]) => event === "ai-gateway-template-auto-refresh-updated",
    )?.[1];
    act(() => {
      handler?.({ payload: { failures: [] } });
    });
    adapter.unmount();
  });

  it("createFromTemplateOpensNewProviderDetail", async () => {
    const store: Store = {
      config: makeConfig(),
      status: makeStatus(),
      targets: [],
    };
    const template = makeTemplate({
      id: "t1",
      name: "OpenCode Zen",
      base_url: "https://opencode.ai/zen/v1",
      protocol: "responses",
    });
    mockStoreWithTemplates(store, [makeTemplateView({ template })]);

    renderWithProviders(<AiGateway />);

    const manageBtn4 = (await screen.findAllByRole("button", { name: /Provider templates|服务商模板/i }))[0];
    fireEvent.click(manageBtn4);
    fireEvent.click(await screen.findByTestId("ai-gateway-template-add-t1"));
    await screen.findByTestId("ai-gateway-template-api-key");
    fireEvent.change(screen.getByTestId("ai-gateway-template-api-key"), {
      target: { value: "sk-live-key" },
    });
    fireEvent.click(screen.getByTestId("ai-gateway-template-create-submit"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "ai_gateway_create_provider_from_template",
        {
          templateId: "t1",
          name: "OpenCode Zen",
          baseUrl: "https://opencode.ai/zen/v1",
          protocol: "responses",
          apiKey: "sk-live-key",
        },
      ),
    );

    const detail = await screen.findByTestId("ai-gateway-provider-detail");
    expect(within(detail).getByLabelText("Name")).toHaveValue("OpenCode Zen");
  });

  it("点击服务商模板管理按钮弹出模板管理弹窗，以及点击添加服务商弹出预设选择器并可就地打开编辑弹窗", async () => {
    const store: Store = {
      config: makeConfig(),
      status: makeStatus(),
      targets: [],
    };
    const templates = [
      makeTemplateView({
        template: makeTemplate({
          id: "tpl-test",
          name: "Test Presets Vendor",
        }),
      }),
    ];
    mockStoreWithTemplates(store, templates);

    renderWithProviders(<AiGateway />);

    // 1. 点击服务商模板管理按钮
    const manageTemplatesBtn = (await screen.findAllByRole("button", { name: /Provider templates|服务商模板/i }))[0];
    fireEvent.click(manageTemplatesBtn);

    // 可以在弹出的服务商模板管理中查看到模板卡片和管理按钮
    expect(await screen.findByText("Test Presets Vendor")).toBeInTheDocument();
    expect(screen.queryByTestId("template-section-reset-btn")).not.toBeInTheDocument();
    expect(screen.getByTestId("template-section-new-btn")).toBeInTheDocument();
    // 关闭模板管理弹窗
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() =>
      expect(screen.queryByTestId("ai-gateway-templates-dialog")).not.toBeInTheDocument(),
    );

    // 2. 点击添加服务商
    const addBtn = screen.getAllByRole("button", { name: /Add provider/i })[0];
    fireEvent.click(addBtn);

    // 弹出选择服务商模板弹窗
    const picker = await screen.findByTestId("ai-gateway-template-picker-dialog");
    expect(picker).toBeInTheDocument();

    // 各模板卡片中已移除编辑图标按钮
    expect(screen.queryByTestId("template-picker-edit-tpl-test")).not.toBeInTheDocument();

    // 在弹窗中点击新建模板按钮打开模板编辑/新建弹窗
    const newTemplateBtn = screen.getByTestId("template-picker-new-btn");
    fireEvent.click(newTemplateBtn);

    // 弹出编辑模板弹窗
    const editDialog = await screen.findByTestId("ai-gateway-template-edit-dialog");
    expect(editDialog).toBeInTheDocument();
  });

  it("gatewayProviderCardsUseLoadedTemplatesForIconAndRetiredHint", async () => {
    const boundProvider = makeProvider({
      id: "p-bound",
      name: "Zone Bound",
      template_id: "t1",
      mappings: [
        { local_model: "l-gone", upstream_model: "gone-model", enabled: false },
        { local_model: "l-keep", upstream_model: "remote-a", enabled: true },
      ],
    });
    const manualProvider = makeProvider({
      id: "p-manual",
      name: "Manual Provider",
    });
    const store: Store = {
      config: makeConfig({ providers: [boundProvider, manualProvider] }),
      status: makeStatus({ provider_count: 2 }),
      targets: [],
    };
    const templates = [
      makeTemplateView({
        template: makeTemplate({
          id: "t1",
          name: "OpenCode Zen",
          icon: "opencode",
          models: [makeTemplateModel({ upstream_model: "remote-a" })],
        }),
      }),
    ];
    mockStoreWithTemplates(store, templates);

    renderWithProviders(<AiGateway />);

    // index.tsx 必须把已加载的 templates 透传给服务商列表：绑定卡片显示模板头像。
    const boundCard = await screen.findByTestId("ai-gateway-provider-p-bound");
    const boundIcon = within(boundCard).getByTestId(
      "ai-gateway-provider-template-icon-p-bound",
    );
    expect(boundIcon).toHaveAttribute(
      "title",
      "Created from template OpenCode Zen",
    );
    expect(
      within(boundIcon).getByTestId("provider-icon-opencode"),
    ).toBeInTheDocument();

    // 手动服务商卡片不显示模板头像。
    const manualCard = screen.getByTestId("ai-gateway-provider-p-manual");
    expect(
      within(manualCard).queryByTestId(
        "ai-gateway-provider-template-icon-p-manual",
      ),
    ).not.toBeInTheDocument();

    // 退休提示同样来自已加载的模板视图。
    const hint = within(boundCard).getByTestId(
      "ai-gateway-provider-retired-mappings-p-bound",
    );
    expect(hint).toHaveTextContent("1 mapping(s) removed from template");
    expect(hint).toHaveAttribute(
      "title",
      "Removed from the template and disabled: gone-model",
    );
  });

  it("首屏今日统计使用 range=today 且头部不发起分组日志查询", async () => {
    const store: Store = {
      config: makeConfig(),
      status: makeStatus({ running: true }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_usage_stats", {
        range: "today",
      }),
    );
    // Today's failure count comes from that statistics snapshot; the header
    // must not issue its own day-grouped request-log query.
    expect(
      invokeMock.mock.calls.filter(
        ([command, args]) =>
          command === "ai_gateway_request_logs" &&
          (args as Record<string, unknown> | undefined)?.groupBy === "day",
      ),
    ).toHaveLength(0);
  });
});

describe("AiGateway 模板服务商模型维护", () => {
  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
  });

  function templateView(
    models: string[],
  ): GatewayProviderTemplateView {
    return {
      template: {
        id: "t1",
        name: "OpenCode Zen",
        description: "Curated OpenCode models",
        base_url: "https://opencode.ai/zen/v1",
        protocol: "chat_completions",
        source: "https://opencode.ai/zen/v1/models",
        models_url: "https://opencode.ai/zen/v1/models",
        models: models.map((upstream_model) => ({
          upstream_model,
          display_name: upstream_model,
          protocol: "chat_completions" as const,
          enabled: true,
        })),
      },
      synced_at: null,
      source: "https://opencode.ai/zen/v1/models",
      from_snapshot: true,
    };
  }

  function mockStoreForTemplateProvider(provider: GatewayUpstreamProvider) {
    const store: Store = {
      config: makeConfig({ providers: [provider] }),
      status: makeStatus({ provider_count: 1 }),
      targets: [],
    };
    invokeMock.mockImplementation(async (command: string, _args?: any) => {
      switch (command) {
        case "ai_gateway_get_config":
          return store.config;
        case "ai_gateway_status":
          return store.status;
        case "ai_gateway_terminal_targets":
          return store.targets;
        case "ai_gateway_provider_templates":
          return [templateView(["remote-a", "retired-model"])];
        case "ai_gateway_delete_provider_model":
        case "ai_gateway_restore_provider_model":
          return store.config;
        default:
          throw new Error(`Unhandled command: ${command}`);
      }
    });
    return store;
  }

  it("deleteMappingOnTemplateProviderCallsCommand", async () => {
    mockStoreForTemplateProvider(
      makeProvider({
        id: "p1",
        name: "Upstream A",
        template_id: "t1",
        mappings: [{ local_model: "local-a", upstream_model: "remote-a" }],
      }),
    );

    renderWithProviders(<AiGateway />);
    fireEvent.click(
      await within(
        await screen.findByTestId("ai-gateway-providers"),
      ).findByText("Upstream A"),
    );

    fireEvent.click(screen.getByRole("button", { name: "Remove mapping 1" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "ai_gateway_delete_provider_model",
        { providerId: "p1", upstreamModel: "remote-a" },
      ),
    );
  });

  it("restoreIgnoredModelCallsCommand", async () => {
    mockStoreForTemplateProvider(
      makeProvider({
        id: "p1",
        name: "Upstream A",
        template_id: "t1",
        ignored_models: ["retired-model"],
        mappings: [],
      }),
    );

    renderWithProviders(<AiGateway />);
    fireEvent.click(
      await within(
        await screen.findByTestId("ai-gateway-providers"),
      ).findByText("Upstream A"),
    );

    fireEvent.click(
      await screen.findByTestId("ai-gateway-restore-model-retired-model"),
    );

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "ai_gateway_restore_provider_model",
        { providerId: "p1", upstreamModel: "retired-model" },
      ),
    );
  });
});

// ---------------------------------------------------------------------------
// Step 3: frontend row-level state, recovery, copy and counting
// ---------------------------------------------------------------------------

describe("AiGateway 逐行自动禁用前端计数与重新启用入口", () => {
  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
  });

  it("运行时状态卡按 enabled 统计服务商数量", async () => {
    const provider1 = makeProvider({
      id: "p1",
      name: "Healthy Provider",
      enabled: true,
      mappings: [{ local_model: "gpt-4o", upstream_model: "remote-a" }],
    });
    const provider2 = makeProvider({
      id: "p2",
      name: "Enabled Provider",
      enabled: true,
      mappings: [{ local_model: "claude-3", upstream_model: "remote-b" }],
    });
    const provider3 = makeProvider({
      id: "p3",
      name: "User-disabled Provider",
      enabled: false,
      mappings: [],
    });

    const store: Store = {
      config: makeConfig({ providers: [provider1, provider2, provider3] }),
      status: makeStatus({ provider_count: 3 }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);

    // 健康率应为 2/3：provider1(enabled) + provider2(enabled) / total 3
    // provider3(enabled=false) 被排除但不影响分母
    const healthCard = await screen.findByTestId("ai-gateway-metric-health");
    expect(within(healthCard).getByText(/2\/3/)).toBeInTheDocument();
  });

  it("auto_disabled_count 从 status 读取而非从 providers 过滤推导", async () => {
    const store: Store = {
      config: makeConfig({ providers: [makeProvider({ id: "p1" })] }),
      status: makeStatus({
        provider_count: 1,
        auto_disabled_count: 5, // 后端统计的自动禁用映射行数
      }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);

    // footer 区域应展示来自 status 的数字
    expect(await screen.findByTestId("ai-gateway-auto-disabled-count")).toHaveTextContent(
      "5",
    );
  });

  it("点击聚合模型指标卡切换到模型列表时 auto-disabled 专属模型被排除", async () => {
    const store: Store = {
      config: makeConfig({
        providers: [
          makeProvider({
            id: "p1",
            mappings: [
              { local_model: "keep", upstream_model: "ra" },
              {
                local_model: "auto-exclude",
                upstream_model: "rb",
                auto_disabled: true,
              },
            ],
          }),
        ],
      }),
      status: makeStatus({ provider_count: 1 }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);

    fireEvent.click(await screen.findByTestId("ai-gateway-metric-models"));

    const panel = await screen.findByTestId("ai-gateway-model-list");
    const models = within(panel).getAllByTestId("ai-gateway-model-list-row");
    const modelNames = models.map(
      (m) => within(m).getByTestId("ai-gateway-model-list-id").textContent?.trim(),
    );
    expect(modelNames).toContain("keep");
    expect(modelNames).not.toContain("auto-exclude");
  });

  it("UI→逐行重新启用命令并刷新开放弹窗属性", async () => {
    const p1 = makeProvider({
      id: "p1",
      name: "Broken Provider",
      mappings: [
        {
          local_model: "gpt-4o",
          upstream_model: "gpt-4o-2024",
          enabled: true,
          auto_disabled: true,
          consecutive_failures: 3,
        },
        {
          local_model: "claude-3",
          upstream_model: "claude-3-2024",
          enabled: true,
        },
      ],
    });

    const store: Store = {
      config: makeConfig({ providers: [p1] }),
      status: makeStatus({ provider_count: 1, auto_disabled_count: 1 }),
      targets: [openCodeTarget()],
    };
    mockStore(store);

    // Wrap original implementation to additionally handle the new per-row command
    const originalImpl = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(
      async (command: string, args?: Record<string, unknown>) => {
        if (command === "ai_gateway_reenable_provider_model") {
          const { providerId, localModel } = args as {
            providerId: string;
            localModel: string;
          };
          // Update the stored config — simulate backend clearing runtime state
          const provider = store.config.providers.find(
            (p) => p.id === providerId,
          );
          if (provider) {
            const mapping = provider.mappings.find(
              (m) => m.local_model === localModel,
            );
            if (mapping) {
              (mapping as any).auto_disabled = false;
              (mapping as any).consecutive_failures = 0;
            }
          }
          return store.config;
        }
        return originalImpl(command, args);
      },
    );

    renderWithProviders(<AiGateway />);

    // 打开服务商详情弹窗
    await screen.findByTestId("ai-gateway-providers");
    const providerCard = screen.getByTestId("ai-gateway-provider-p1");
    fireEvent.click(within(providerCard).getByText("Broken Provider"));

    const dialog = await screen.findByTestId("ai-gateway-provider-detail");
    expect(dialog).toBeInTheDocument();

    // 逐行重新启用
    const rowBtn = screen.getByTestId("ai-gateway-reenable-mapping-gpt-4o");
    fireEvent.click(rowBtn);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "ai_gateway_reenable_provider_model",
        {
          providerId: "p1",
          localModel: "gpt-4o",
          upstreamModel: "gpt-4o-2024",
        },
      ),
    );

    // 成功后按钮应从仍然开放的弹窗中消失（该行的 auto_disabled 被清除，不再显示 re-enable）
    await waitFor(() =>
      expect(
        screen.queryByTestId("ai-gateway-reenable-mapping-gpt-4o"),
      ).not.toBeInTheDocument(),
    );

    // 不应出现旧的 provider-level 重新启用命令
    const providerLevelCalls = invokeMock.mock.calls.filter(
      ([cmd]) => cmd === "ai_gateway_reenable_provider",
    );
    expect(providerLevelCalls).toHaveLength(0);
  });

  it("UI→批量重新启用命令并刷新开放弹窗属性", async () => {
    const p1 = makeProvider({
      id: "p1",
      name: "Broken Provider",
      mappings: [
        {
          local_model: "gpt-4o",
          upstream_model: "gpt-4o-2024",
          enabled: true,
          auto_disabled: true,
          consecutive_failures: 3,
        },
        {
          local_model: "claude-3",
          upstream_model: "claude-3-2024",
          enabled: true,
        },
      ],
    });

    const store: Store = {
      config: makeConfig({ providers: [p1] }),
      status: makeStatus({ provider_count: 1, auto_disabled_count: 1 }),
      targets: [openCodeTarget()],
    };
    mockStore(store);

    // Wrap original implementation to additionally handle the new batch command
    const originalImpl = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(
      async (command: string, args?: Record<string, unknown>) => {
        if (command === "ai_gateway_reenable_provider_models") {
          const { providerId } = args as { providerId: string };
          const provider = store.config.providers.find(
            (p) => p.id === providerId,
          );
          if (provider) {
            for (const m of provider.mappings) {
              (m as any).auto_disabled = false;
              (m as any).consecutive_failures = 0;
            }
          }
          return store.config;
        }
        return originalImpl(command, args);
      },
    );

    renderWithProviders(<AiGateway />);

    // 打开服务商详情弹窗
    await screen.findByTestId("ai-gateway-providers");
    const providerCard = screen.getByTestId("ai-gateway-provider-p1");
    fireEvent.click(within(providerCard).getByText("Broken Provider"));

    const dialog = await screen.findByTestId("ai-gateway-provider-detail");
    expect(dialog).toBeInTheDocument();

    // 批量重新启用
    const allBtn = screen.getByTestId("ai-gateway-reenable-models-p1");
    fireEvent.click(allBtn);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "ai_gateway_reenable_provider_models",
        { providerId: "p1" },
      ),
    );

    // 成功后按钮应从仍然开放的弹窗中消失（所有行的 auto_disabled 被清除）
    await waitFor(() =>
      expect(
        screen.queryByTestId("ai-gateway-reenable-models-p1"),
      ).not.toBeInTheDocument(),
    );

    // 不应出现旧的 provider-level 重新启用命令
    const providerLevelCalls = invokeMock.mock.calls.filter(
      ([cmd]) => cmd === "ai_gateway_reenable_provider",
    );
    expect(providerLevelCalls).toHaveLength(0);
  });

  it("服务商模板区域不再展示整个服务商的重新启用按钮", async () => {
    const store: Store = {
      config: makeConfig({
        providers: [
          makeProvider({
            id: "p1",
            name: "Broken Provider",
            mappings: [
              {
                local_model: "m1",
                upstream_model: "r1",
                auto_disabled: true,
              },
            ],
          }),
        ],
      }),
      status: makeStatus({ provider_count: 1, auto_disabled_count: 1 }),
      targets: [openCodeTarget()],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);

    const providersSection = await screen.findByTestId("ai-gateway-providers");

    // 不应出现整个服务商级别的 Re-enable 按钮
    expect(
      within(providersSection).queryByRole("button", {
        name: /Re-enable provider|Re-enable/i,
      }),
    ).not.toBeInTheDocument();

    // 服务商开关仍反映 enabled
    expect(
      within(providersSection).getByRole("switch", {
        name: /Broken Provider/i,
      }),
    ).toBeChecked();
  });

  it("顶部操作栏默认 API Key 点击复制调用 clipboard 并展示成功", async () => {
    const key = {
      id: "k1",
      label: "My Main Key",
      value: "sk-gateway-secret-12345",
      enabled: true,
      created_at: 1,
    };
    const store: Store = {
      config: makeConfig({
        port: 17688,
        keys: [key],
        default_key_id: "k1",
      }),
      status: makeStatus({ running: true }),
      targets: [],
    };
    mockStore(store);

    const spy = vi.spyOn(navigator.clipboard, "writeText");

    renderWithProviders(<AiGateway />);

    // Anthropic Messages 协议地址快捷复制
    const anthropicAddress = await screen.findByTestId(
      "ai-gateway-anthropic-address",
    );
    expect(anthropicAddress).toHaveTextContent(
      "http://127.0.0.1:17688/v1/messages",
    );

    const copyAnthropicBtn = await screen.findByTestId(
      "ai-gateway-copy-anthropic-address",
    );
    fireEvent.click(copyAnthropicBtn);
    expect(spy).toHaveBeenCalledWith("http://127.0.0.1:17688/v1/messages");
  });

  it("顶部操作栏同时展示 OpenAI 与 Anthropic Messages 两个协议地址与徽标", async () => {
    const store: Store = {
      config: makeConfig({ port: 17688 }),
      status: makeStatus({ running: false }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);

    expect(
      await screen.findByTestId("ai-gateway-local-address"),
    ).toHaveTextContent("http://127.0.0.1:17688/v1");
    expect(screen.getByText("OpenAI")).toBeInTheDocument();

    expect(
      await screen.findByTestId("ai-gateway-anthropic-address"),
    ).toHaveTextContent("http://127.0.0.1:17688/v1/messages");
    expect(screen.getByText("Anthropic Messages")).toBeInTheDocument();
  });

  it("顶部操作栏展示手动刷新按钮，点击后重新获取今日统计与日志", async () => {
    const store: Store = {
      config: makeConfig(),
      status: makeStatus({ running: true }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);

    const refreshBtn = await screen.findByTestId("ai-gateway-refresh-btn");
    expect(refreshBtn).toBeInTheDocument();
    expect(refreshBtn).toHaveAttribute("title", "Refresh data");

    const callsBefore = invokeMock.mock.calls.filter(
      (call) => call[0] === "ai_gateway_usage_stats",
    ).length;

    await act(async () => {
      fireEvent.click(refreshBtn);
    });

    await waitFor(() => {
      const callsAfter = invokeMock.mock.calls.filter(
        (call) => call[0] === "ai_gateway_usage_stats",
      ).length;
      expect(callsAfter).toBeGreaterThan(callsBefore);
    });
  });

  it("窗口获得焦点时触发今日数据轻量刷新", async () => {
    const store: Store = {
      config: makeConfig(),
      status: makeStatus({ running: true }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);
    await screen.findByTestId("ai-gateway-runtime");

    const callsBefore = invokeMock.mock.calls.filter(
      (call) => call[0] === "ai_gateway_usage_stats",
    ).length;

    await act(async () => {
      window.dispatchEvent(new Event("focus"));
    });

    await waitFor(() => {
      const callsAfter = invokeMock.mock.calls.filter(
        (call) => call[0] === "ai_gateway_usage_stats",
      ).length;
      expect(callsAfter).toBeGreaterThan(callsBefore);
    });
  });
});

// ---------------------------------------------------------------------------
// Step 4: live page refresh on the config-update broadcast (AC-009, REQ-006)
// ---------------------------------------------------------------------------

describe("AiGateway 配置更新事件实时刷新", () => {
  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
  });

  afterEach(() => {
    vi.useRealTimers();
    setNativeWindowVisible(true);
  });

  it("ai_gateway_config_updated_event_triggers_a_configuration_reload_and_shows_the_new_state", async () => {
    const store: Store = {
      config: makeConfig({
        providers: [
          makeProvider({
            id: "p1",
            name: "Upstream A",
            mappings: [
              {
                local_model: "gpt-4o",
                upstream_model: "gpt-4o-2024",
                enabled: true,
              },
            ],
          }),
        ],
      }),
      status: makeStatus({ provider_count: 1, auto_disabled_count: 0 }),
      targets: [],
    };
    mockStore(store);

    // 捕获页面注册的配置更新事件回调（默认挂到配置更新事件名上）。
    const configUpdateHandlers: Array<(event: unknown) => void> = [];
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
          configUpdateHandlers.push(handler);
        }
        return vi.fn();
      },
    );

    renderWithProviders(<AiGateway />);
    await within(
      await screen.findByTestId("ai-gateway-providers"),
    ).findByText("Upstream A");

    // 初始为健康行：服务商卡片没有自动禁用提示。
    expect(
      screen.queryByTestId("ai-gateway-provider-auto-disabled-models-p1"),
    ).not.toBeInTheDocument();

    // 冻结接口契约：导出的事件名必须与后端广播一致，且页面在 mount 时订阅它。
    expect(AI_GATEWAY_CONFIG_UPDATED_EVENT).toBe(
      "ai-gateway-config-update",
    );
    expect(listenMock).toHaveBeenCalledWith(
      AI_GATEWAY_CONFIG_UPDATED_EVENT,
      expect.any(Function),
    );
    expect(configUpdateHandlers.length).toBeGreaterThan(0);

    const getConfigCallsBefore = invokeMock.mock.calls.filter(
      ([command]) => command === "ai_gateway_get_config",
    ).length;

    // 后端结算把该映射行自动禁用；mock store 反映新状态。
    store.config = {
      ...store.config,
      providers: [
        makeProvider({
          id: "p1",
          name: "Upstream A",
          mappings: [
            {
              local_model: "gpt-4o",
              upstream_model: "gpt-4o-2024",
              enabled: true,
              auto_disabled: true,
              disabled_reason: "HTTP 500",
              disabled_at: 1_700_000_000,
              consecutive_failures: 3,
              last_error_at: 1_700_000_100,
            },
          ],
        }),
      ],
    };
    store.status = makeStatus({ provider_count: 1, auto_disabled_count: 1 });

    // 广播到达：无任何用户操作。
    await act(async () => {
      for (const handler of configUpdateHandlers) {
        handler({ payload: undefined });
      }
    });

    await waitFor(() => {
      const getConfigCallsAfter = invokeMock.mock.calls.filter(
        ([command]) => command === "ai_gateway_get_config",
      ).length;
      expect(getConfigCallsAfter).toBeGreaterThan(getConfigCallsBefore);
    });

    // 服务商卡片与运行时状态卡直接展示新状态。
    expect(
      await screen.findByTestId("ai-gateway-provider-auto-disabled-models-p1"),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(
        screen.getByTestId("ai-gateway-auto-disabled-count"),
      ).toHaveTextContent("1"),
    );
  });

  it("配置更新事件只刷新受影响数据并保留可见时的用量轮询", async () => {
    vi.useFakeTimers();
    const store: Store = {
      config: makeConfig({
        providers: [makeProvider({ id: "p1", name: "Upstream A" })],
      }),
      status: makeStatus({ running: true }),
      targets: [],
    };
    mockStore(store);

    const configUpdateHandlers: Array<(event: unknown) => void> = [];
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
          configUpdateHandlers.push(handler);
        }
        return vi.fn();
      },
    );

    const flushAsyncWork = async () => {
      await act(async () => {
        for (let index = 0; index < 12; index += 1) {
          await Promise.resolve();
        }
      });
    };

    renderWithProviders(<AiGateway />);
    await flushAsyncWork();
    await flushAsyncWork();

    expect(
      within(screen.getByTestId("ai-gateway-providers")).getByText("Upstream A"),
    ).toBeInTheDocument();
    expect(configUpdateHandlers.length).toBeGreaterThan(0);

    const invokeCount = (command: string) =>
      invokeMock.mock.calls.filter((call) => call[0] === command).length;

    const configBefore = invokeCount("ai_gateway_get_config");
    const usageBefore = invokeCount("ai_gateway_usage_stats");
    const logsBefore = invokeCount("ai_gateway_request_logs");

    // 配置更新广播：只重新读取受影响的配置，不重复拉取用量统计与请求日志。
    await act(async () => {
      for (const handler of configUpdateHandlers) {
        handler({ payload: undefined });
      }
      await Promise.resolve();
    });
    await flushAsyncWork();

    expect(invokeCount("ai_gateway_get_config")).toBeGreaterThan(configBefore);
    expect(invokeCount("ai_gateway_usage_stats")).toBe(usageBefore);
    expect(invokeCount("ai_gateway_request_logs")).toBe(logsBefore);

    // 可见且运行时，用量新鲜度轮询必须保留：推进一个间隔触发一次刷新。
    const usageBeforePoll = invokeCount("ai_gateway_usage_stats");
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5_000);
    });
    expect(invokeCount("ai_gateway_usage_stats")).toBeGreaterThan(usageBeforePoll);
  });

  it("配置事件突发只触发当前刷新加至多一次追并，且不重读用量或日志", async () => {
    vi.useFakeTimers();
    const store: Store = {
      config: makeConfig({
        providers: [makeProvider({ id: "p1", name: "Upstream A" })],
      }),
      status: makeStatus({ running: false }),
      targets: [],
    };
    mockStore(store);

    const configUpdateHandlers: Array<(event: unknown) => void> = [];
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
          configUpdateHandlers.push(handler);
        }
        return vi.fn();
      },
    );

    const flushAsyncWork = async () => {
      await act(async () => {
        for (let index = 0; index < 16; index += 1) {
          await Promise.resolve();
        }
      });
    };

    renderWithProviders(<AiGateway />);
    await flushAsyncWork();
    await flushAsyncWork();
    expect(configUpdateHandlers.length).toBeGreaterThan(0);

    const invokeCount = (command: string) =>
      invokeMock.mock.calls.filter((call) => call[0] === command).length;
    const configBefore = invokeCount("ai_gateway_get_config");
    const usageBefore = invokeCount("ai_gateway_usage_stats");
    const logsBefore = invokeCount("ai_gateway_request_logs");

    await act(async () => {
      for (let index = 0; index < 5; index += 1) {
        for (const handler of configUpdateHandlers) {
          handler({ payload: undefined });
        }
      }
      await Promise.resolve();
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_000);
    });
    await flushAsyncWork();

    const configReads = invokeCount("ai_gateway_get_config") - configBefore;
    expect(configReads).toBeGreaterThanOrEqual(1);
    expect(configReads).toBeLessThanOrEqual(2);
    expect(invokeCount("ai_gateway_usage_stats")).toBe(usageBefore);
    expect(invokeCount("ai_gateway_request_logs")).toBe(logsBefore);
  });

  it("隐藏时消费配置与鉴权通知但不发起页面读取", async () => {
    const store: Store = {
      config: makeConfig({
        providers: [makeProvider({ id: "p1", name: "Upstream A" })],
      }),
      status: makeStatus({ running: true }),
      targets: [],
    };
    mockStore(store);

    const configUpdateHandlers: Array<(event: unknown) => void> = [];
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
          configUpdateHandlers.push(handler);
        }
        return vi.fn();
      },
    );

    function Host() {
      const [visible, setVisible] = useState(true);
      return (
        <>
          <button type="button" onClick={() => setVisible(false)}>
            hide
          </button>
          <AiGateway isVisible={visible} />
        </>
      );
    }

    const flushAsyncWork = async () => {
      await act(async () => {
        for (let index = 0; index < 16; index += 1) {
          await Promise.resolve();
        }
      });
    };

    renderWithProviders(<Host />);
    await screen.findByText("Upstream A");
    await flushAsyncWork();
    expect(configUpdateHandlers.length).toBeGreaterThan(0);

    const pageReadCommands = [
      "ai_gateway_get_config",
      "ai_gateway_status",
      "ai_gateway_terminal_targets",
      "ai_gateway_provider_templates",
      "ai_gateway_model_pools_status",
      "ai_gateway_usage_stats",
      "ai_gateway_request_logs",
    ];
    const invokeCount = (command: string) =>
      invokeMock.mock.calls.filter((call) => call[0] === command).length;
    const pageReadCounts = () =>
      Object.fromEntries(
        pageReadCommands.map((command) => [command, invokeCount(command)]),
      );

    await act(async () => {
      setNativeWindowVisible(false);
      fireEvent.click(screen.getByRole("button", { name: "hide" }));
    });
    await flushAsyncWork();
    const hiddenBaseline = pageReadCounts();

    await act(async () => {
      for (let index = 0; index < 5; index += 1) {
        for (const handler of configUpdateHandlers) {
          handler({ payload: undefined });
        }
      }
      for (let index = 0; index < 16; index += 1) {
        await Promise.resolve();
      }
    });
    await flushAsyncWork();

    expect(pageReadCounts()).toEqual(hiddenBaseline);

    const authCall = listenMock.mock.calls.find(
      ([eventName]) => eventName === AI_GATEWAY_KEY_AUTH_FAILED_EVENT,
    );
    expect(
      authCall,
      "AiGateway 隐藏时仍须订阅鉴权失败通知",
    ).toBeDefined();

    vi.useFakeTimers();
    act(() => {
      (
        authCall![1] as (event: {
          payload: GatewayKeyAuthFailedEvent;
        }) => void
      )({
        payload: {
          provider_id: "p1",
          provider_name: "Upstream A",
          key_id: "k1",
          key_name: "sk-secret",
          reason: "invalid api key",
          marked_at: 1_700_000_000,
        },
      });
      vi.advanceTimersByTime(AI_GATEWAY_KEY_AUTH_TOAST_WINDOW_MS);
    });

    expect(
      screen.getAllByText(i18n.t("aiGatewayKeyAuthFailedToastTitle")),
    ).toHaveLength(1);
    expect(pageReadCounts()).toEqual(hiddenBaseline);
    vi.useRealTimers();
  });

  it("恢复可见时只执行一次连贯追赶", async () => {
    const store: Store = {
      config: makeConfig({
        providers: [makeProvider({ id: "p1", name: "Upstream A" })],
      }),
      status: makeStatus({ running: true }),
      targets: [],
    };
    mockStore(store);

    function Host() {
      const [visible, setVisible] = useState(true);
      return (
        <>
          <button type="button" onClick={() => setVisible(false)}>
            hide
          </button>
          <button type="button" onClick={() => setVisible(true)}>
            show
          </button>
          <AiGateway isVisible={visible} />
        </>
      );
    }

    const flushAsyncWork = async () => {
      await act(async () => {
        for (let index = 0; index < 16; index += 1) {
          await Promise.resolve();
        }
      });
    };

    renderWithProviders(<Host />);
    await screen.findByText("Upstream A");
    await flushAsyncWork();

    const pageReadCommands = [
      "ai_gateway_get_config",
      "ai_gateway_status",
      "ai_gateway_terminal_targets",
      "ai_gateway_provider_templates",
      "ai_gateway_model_pools_status",
      "ai_gateway_usage_stats",
      "ai_gateway_request_logs",
    ];
    const invokeCount = (command: string) =>
      invokeMock.mock.calls.filter((call) => call[0] === command).length;
    const pageReadCounts = () =>
      Object.fromEntries(
        pageReadCommands.map((command) => [command, invokeCount(command)]),
      );

    await act(async () => {
      setNativeWindowVisible(false);
      fireEvent.click(screen.getByRole("button", { name: "hide" }));
    });
    await flushAsyncWork();
    const hiddenBaseline = pageReadCounts();

    await act(async () => {
      setNativeWindowVisible(true);
      fireEvent.click(screen.getByRole("button", { name: "show" }));
    });
    await waitFor(() => {
      for (const command of pageReadCommands) {
        expect(invokeCount(command)).toBeGreaterThanOrEqual(
          (hiddenBaseline as Record<string, number>)[command] + 1,
        );
      }
    });
    await flushAsyncWork();

    const resumed = pageReadCounts();
    for (const command of pageReadCommands) {
      expect(
        (resumed as Record<string, number>)[command] -
          (hiddenBaseline as Record<string, number>)[command],
        `${command} must run exactly once for the visible catch-up`,
      ).toBe(1);
    }
  });

  it("可见性变化取代旧配置请求后，迟到响应不能覆盖当前数据", async () => {
    const store: Store = {
      config: makeConfig({
        providers: [makeProvider({ id: "p1", name: "Current A" })],
      }),
      status: makeStatus({ running: true }),
      targets: [],
    };
    mockStore(store);

    const configUpdateHandlers: Array<(event: unknown) => void> = [];
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
          configUpdateHandlers.push(handler);
        }
        return vi.fn();
      },
    );

    function Host() {
      const [visible, setVisible] = useState(true);
      return (
        <>
          <button type="button" onClick={() => setVisible(false)}>
            hide
          </button>
          <button type="button" onClick={() => setVisible(true)}>
            show
          </button>
          <AiGateway isVisible={visible} />
        </>
      );
    }

    const flushAsyncWork = async () => {
      await act(async () => {
        for (let index = 0; index < 16; index += 1) {
          await Promise.resolve();
        }
      });
    };

    renderWithProviders(<Host />);
    await screen.findByText("Current A");
    await flushAsyncWork();
    expect(configUpdateHandlers.length).toBeGreaterThan(0);

    const staleReply = deferredPromise<GatewayConfig>();
    const originalInvoke = invokeMock.getMockImplementation()!;
    let staleRequestStarted = false;
    invokeMock.mockImplementation(
      async (command: string, args?: Record<string, unknown>) => {
        if (command === "ai_gateway_get_config" && !staleRequestStarted) {
          staleRequestStarted = true;
          return staleReply.promise;
        }
        return originalInvoke(command, args);
      },
    );

    await act(async () => {
      for (const handler of configUpdateHandlers) {
        handler({ payload: undefined });
      }
      await Promise.resolve();
    });
    expect(staleRequestStarted).toBe(true);

    await act(async () => {
      setNativeWindowVisible(false);
      fireEvent.click(screen.getByRole("button", { name: "hide" }));
    });
    await flushAsyncWork();

    store.config = makeConfig({
      providers: [makeProvider({ id: "p1", name: "Fresh A" })],
    });

    await act(async () => {
      setNativeWindowVisible(true);
      fireEvent.click(screen.getByRole("button", { name: "show" }));
    });
    await waitFor(() =>
      expect(
        within(screen.getByTestId("ai-gateway-providers")).getByText("Fresh A"),
      ).toBeInTheDocument(),
    );

    await act(async () => {
      staleReply.resolve(
        makeConfig({
          providers: [makeProvider({ id: "p1", name: "Stale A" })],
        }),
      );
      for (let index = 0; index < 16; index += 1) {
        await Promise.resolve();
      }
    });
    await flushAsyncWork();

    const currentProvider = screen.getByTestId("ai-gateway-provider-p1");
    expect(currentProvider).toHaveTextContent("Fresh A");
    expect(currentProvider).not.toHaveTextContent("Stale A");
  });

  it("pauses today-usage polling while hidden and catches up exactly once on show", async () => {
    vi.useFakeTimers();
    const store: Store = {
      config: makeConfig(),
      status: makeStatus({ running: true }),
      targets: [],
    };
    mockStore(store);

    const flushAsyncWork = async () => {
      await act(async () => {
        for (let index = 0; index < 12; index += 1) {
          await Promise.resolve();
        }
      });
    };

    renderWithProviders(<AiGateway />);
    await flushAsyncWork();
    await flushAsyncWork();

    const invokeCount = (command: string) =>
      invokeMock.mock.calls.filter((call) => call[0] === command).length;
    const usageBefore = invokeCount("ai_gateway_usage_stats");

    // Hidden native window: no poll across two intervals.
    await act(async () => {
      setNativeWindowVisible(false);
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5_000);
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5_000);
    });
    expect(invokeCount("ai_gateway_usage_stats")).toBe(usageBefore);

    // Showing the window catches up exactly once.
    await act(async () => {
      setNativeWindowVisible(true);
    });
    expect(invokeCount("ai_gateway_usage_stats")).toBe(usageBefore + 1);

    // The interval then resumes normally.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5_000);
    });
    expect(invokeCount("ai_gateway_usage_stats")).toBe(usageBefore + 2);
  });

  it("ai_gateway_page_releases_the_config_update_listener_on_unmount", async () => {
    const store: Store = {
      config: makeConfig(),
      status: makeStatus(),
      targets: [],
    };
    mockStore(store);

    const unlisten = vi.fn();
    (
      listenMock as unknown as {
        mockImplementation: (
          fn: (eventName: string) => Promise<() => void>,
        ) => void;
      }
    ).mockImplementation(async (eventName: string) => {
      if (eventName === AI_GATEWAY_CONFIG_UPDATED_EVENT) {
        return unlisten;
      }
      return vi.fn();
    });

    const { unmount } = renderWithProviders(<AiGateway />);
    await screen.findByTestId("ai-gateway-providers");

    expect(listenMock).toHaveBeenCalledWith(
      AI_GATEWAY_CONFIG_UPDATED_EVENT,
      expect.any(Function),
    );

    unmount();

    await waitFor(() => expect(unlisten).toHaveBeenCalledTimes(1));
  });

  describe("删除服务商二次确认", () => {
    it("点击删除后弹出二次确认对话框，点击取消不执行删除", async () => {
      const store: Store = {
        config: makeConfig({
          providers: [
            makeProvider({
              id: "p1",
              name: "DeepSeek Provider",
              base_url: "https://api.deepseek.com",
            }),
          ],
        }),
        status: makeStatus({ provider_count: 1 }),
        targets: [openCodeTarget()],
      };
      mockStore(store);

      renderWithProviders(<AiGateway />);

      fireEvent.click(
        await within(
          await screen.findByTestId("ai-gateway-providers"),
        ).findByText("DeepSeek Provider"),
      );

      const deleteBtn = await screen.findByRole("button", {
        name: i18n.t("aiGatewayDelete"),
      });
      fireEvent.click(deleteBtn);

      const confirmDialog = await screen.findByTestId("ai-gateway-delete-provider-confirm-dialog");
      expect(confirmDialog).toBeInTheDocument();
      expect(
        within(confirmDialog).getByText(
          i18n.t("aiGatewayDeleteProviderConfirm", { name: "DeepSeek Provider" }),
        ),
      ).toBeInTheDocument();

      const cancelBtn = screen.getByTestId("ai-gateway-delete-provider-cancel");
      fireEvent.click(cancelBtn);

      expect(invokeMock).not.toHaveBeenCalledWith("ai_gateway_delete_provider", {
        providerId: "p1",
      });

      expect(screen.getByTestId("ai-gateway-provider-detail")).toBeInTheDocument();
    });

    it("点击删除并在二次确认对话框中确认后，调用后端删除接口并关闭弹窗", async () => {
      const store: Store = {
        config: makeConfig({
          providers: [
            makeProvider({
              id: "p1",
              name: "DeepSeek Provider",
              base_url: "https://api.deepseek.com",
            }),
          ],
        }),
        status: makeStatus({ provider_count: 1 }),
        targets: [openCodeTarget()],
      };
      mockStore(store);

      renderWithProviders(<AiGateway />);

      fireEvent.click(
        await within(
          await screen.findByTestId("ai-gateway-providers"),
        ).findByText("DeepSeek Provider"),
      );

      const deleteBtn = await screen.findByRole("button", {
        name: i18n.t("aiGatewayDelete"),
      });
      fireEvent.click(deleteBtn);

      const confirmDialog = await screen.findByTestId("ai-gateway-delete-provider-confirm-dialog");
      expect(confirmDialog).toBeInTheDocument();
      expect(
        within(confirmDialog).getByText(
          i18n.t("aiGatewayDeleteProviderConfirm", { name: "DeepSeek Provider" }),
        ),
      ).toBeInTheDocument();

      const confirmDeleteBtn = screen.getByTestId("ai-gateway-delete-provider-confirm");
      fireEvent.click(confirmDeleteBtn);

      await waitFor(() =>
        expect(invokeMock).toHaveBeenCalledWith("ai_gateway_delete_provider", {
          providerId: "p1",
        }),
      );

      await waitFor(() =>
        expect(screen.queryByTestId("ai-gateway-provider-detail")).not.toBeInTheDocument(),
      );
    });
  });
});

// ---------------------------------------------------------------------------
// Step 3: UI wiring for manual upstream-key re-enable.
// RED test for the frozen interface contract. Only this test file is touched.
// ---------------------------------------------------------------------------

describe("AiGateway 上游密钥池 UI 接线", () => {
  beforeEach(async () => {
    resetTauriMocks();
    resetMessageMocks();
    await i18n.changeLanguage("en");
  });

  it("UI→重新启用上游密钥命令并刷新开放弹窗", async () => {
    const p1 = makeProvider({
      id: "p1",
      name: "Keyed Provider",
      keys: [
        providerKey({
          id: "k1",
          name: "Primary",
          value: "sk-one",
          auto_marked: true,
          failure_kind: "quota",
          marked_at: 1_700_000_000,
        }),
      ],
    });

    const store: Store = {
      config: makeConfig({ providers: [p1] }),
      status: makeStatus({ provider_count: 1 }),
      targets: [openCodeTarget()],
    };
    mockStore(store);

    const originalImpl = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(
      async (command: string, args?: Record<string, unknown>) => {
        if (command === "ai_gateway_reenable_provider_key") {
          const { providerId, keyId } = args as {
            providerId: string;
            keyId: string;
          };
          const provider = store.config.providers.find(
            (entry) => entry.id === providerId,
          ) as unknown as { keys?: RawProviderKey[] } | undefined;
          const key = provider?.keys?.find((entry) => entry.id === keyId);
          if (key) {
            key.auto_marked = false;
            key.failure_kind = null;
            key.marked_at = null;
            key.reason = null;
          }
          return store.config;
        }
        return originalImpl(command, args);
      },
    );

    renderWithProviders(<AiGateway />);

    await screen.findByTestId("ai-gateway-providers");
    const providerCard = screen.getByTestId("ai-gateway-provider-p1");
    fireEvent.click(within(providerCard).getByText("Keyed Provider"));

    await screen.findByTestId("ai-gateway-provider-detail");
    fireEvent.click(screen.getByTestId("ai-gateway-reenable-key-0"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "ai_gateway_reenable_provider_key",
        { providerId: "p1", keyId: "k1" },
      ),
    );

    await waitFor(() =>
      expect(
        screen.queryByTestId("ai-gateway-reenable-key-0"),
      ).not.toBeInTheDocument(),
    );
  });
});

// ---------------------------------------------------------------------------
// AI 网关操作提示：成功/失败标题必须与实际操作相符并带上操作对象，
// 避免历史上「启停都显示 Saved」「删除不分对象」「失败只有 Action failed」。
// ---------------------------------------------------------------------------

describe("AiGateway 操作提示与实际操作相符", () => {
  beforeEach(async () => {
    resetTauriMocks();
    resetMessageMocks();
    await i18n.changeLanguage("en");
  });

  it("启停服务的操作提示区分启动与停止，启动附带本地地址", async () => {
    const store: Store = {
      config: makeConfig(),
      status: makeStatus({ running: false }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);
    const toggleBtn = await screen.findByTestId("ai-gateway-toggle-service");
    fireEvent.click(toggleBtn);

    expect(
      await screen.findByText(i18n.t("aiGatewayServiceStarted")),
    ).toBeInTheDocument();
    // 启动成功的 toast 描述携带本地地址：此时页面上至少有状态卡与 toast 两处。
    await waitFor(() =>
      expect(
        screen.getAllByText("http://127.0.0.1:17688/v1").length,
      ).toBeGreaterThanOrEqual(2),
    );

    fireEvent.click(await screen.findByTestId("ai-gateway-toggle-service"));
    expect(
      await screen.findByText(i18n.t("aiGatewayServiceStopped")),
    ).toBeInTheDocument();
  });

  it("启停服务失败时标题区分启动与停止", async () => {
    const store: Store = {
      config: makeConfig(),
      status: makeStatus({ running: false }),
      targets: [],
    };
    mockStore(store);
    const read = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "ai_gateway_start") throw new Error("port busy");
      return read(command, args);
    });

    renderWithProviders(<AiGateway />);
    fireEvent.click(await screen.findByTestId("ai-gateway-toggle-service"));

    expect(
      await screen.findByText(i18n.t("aiGatewayServiceStartFailed")),
    ).toBeInTheDocument();
    // errorToMessage 透出后端原因：失败描述中包含原始错误文本。
    await waitFor(() =>
      expect(document.body.textContent).toContain("port busy"),
    );
  });

  it("删除服务商的操作提示带服务商名，不再是笼统的 Deleted", async () => {
    const store: Store = {
      config: makeConfig({
        providers: [
          makeProvider({
            id: "p1",
            name: "DeepSeek Provider",
            base_url: "https://api.deepseek.com",
          }),
        ],
      }),
      status: makeStatus({ provider_count: 1 }),
      targets: [openCodeTarget()],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);

    fireEvent.click(
      await within(
        await screen.findByTestId("ai-gateway-providers"),
      ).findByText("DeepSeek Provider"),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: i18n.t("aiGatewayDelete") }),
    );
    fireEvent.click(
      await screen.findByTestId("ai-gateway-delete-provider-confirm"),
    );

    expect(
      await screen.findByText(
        i18n.t("aiGatewayProviderDeleted", { name: "DeepSeek Provider" }),
      ),
    ).toBeInTheDocument();
  });

  it("终端同步的操作提示带工具名", async () => {
    const store: Store = {
      config: makeConfig({
        keys: [{ id: "k1", label: "Main", value: "********", enabled: true, created_at: 1 }],
        default_key_id: "k1",
      }),
      status: makeStatus({ key_count: 1, default_key_id: "k1" }),
      targets: [
        openCodeTarget({
          provider_id: "gw-open",
          synced: true,
          pending_sync: false,
          synced_key_id: "k1",
          synced_at: 1,
        }),
      ],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);
    await visitTerminalTab();
    await screen.findByText("OpenCode");

    const openRow = within(screen.getByTestId("ai-gateway-target-opencode"));
    fireEvent.click(openRow.getByRole("button", { name: /sync/i }));

    expect(
      await screen.findByText(
        i18n.t("aiGatewaySyncSuccess", { tool: "OpenCode" }),
      ),
    ).toBeInTheDocument();
  });

  it("删除本地密钥前弹出二次确认，取消时不调用后端", async () => {
    const store: Store = {
      config: makeConfig({
        keys: [{ id: "k1", label: "Main", value: "********", enabled: true, created_at: 1 }],
        default_key_id: "k1",
      }),
      status: makeStatus({ key_count: 1, default_key_id: "k1" }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);
    fireEvent.click(
      await screen.findByRole("tab", { name: /API Keys|API 密钥/i }),
    );

    fireEvent.click(
      await screen.findByRole("button", { name: "Delete key Main" }),
    );

    // 确认框标题与文案均带密钥名。
    expect(
      await screen.findByText(i18n.t("aiGatewayDeleteKeyTitle")),
    ).toBeInTheDocument();
    expect(
      await screen.findByText(
        i18n.t("aiGatewayDeleteKeyConfirm", { label: "Main" }),
      ),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    await waitFor(() =>
      expect(
        screen.queryByText(i18n.t("aiGatewayDeleteKeyTitle")),
      ).not.toBeInTheDocument(),
    );
    expect(invokeMock).not.toHaveBeenCalledWith("ai_gateway_delete_key", {
      keyId: "k1",
    });
    expect(screen.getByTestId("ai-gateway-key-k1")).toBeInTheDocument();
  });

  it("在二次确认框中确认后删除本地密钥并提示密钥名", async () => {
    const store: Store = {
      config: makeConfig({
        keys: [{ id: "k1", label: "Main", value: "********", enabled: true, created_at: 1 }],
        default_key_id: "k1",
      }),
      status: makeStatus({ key_count: 1, default_key_id: "k1" }),
      targets: [],
    };
    mockStore(store);
    const read = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "ai_gateway_delete_key") {
        store.config = {
          ...store.config,
          keys: store.config.keys.filter((key) => key.id !== args?.keyId),
        };
        return store.config;
      }
      return read(command, args);
    });

    renderWithProviders(<AiGateway />);
    fireEvent.click(
      await screen.findByRole("tab", { name: /API Keys|API 密钥/i }),
    );

    fireEvent.click(
      await screen.findByRole("button", { name: "Delete key Main" }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "Delete" }),
    );

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_delete_key", {
        keyId: "k1",
      }),
    );
    expect(
      await screen.findByText(
        i18n.t("aiGatewayLocalKeyDeleted", { label: "Main" }),
      ),
    ).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Step 3: per-provider authentication-failed toast aggregation (AC-013).
// RED tests for the frozen interface contract. Only this test file is touched.
// ---------------------------------------------------------------------------

describe("AiGateway 鉴权失败 toast 按服务商聚合 (AC-013)", () => {
  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  function authFailedEvent(
    overrides: Partial<GatewayKeyAuthFailedEvent> = {},
  ): GatewayKeyAuthFailedEvent {
    return {
      provider_id: "p-alpha",
      provider_name: "Provider Alpha",
      key_id: "k-alpha-1",
      key_name: "sk-alpha-secret-one",
      reason: "invalid api key",
      marked_at: 1_700_000_000,
      ...overrides,
    };
  }

  function deliver(
    handler: (event: { payload: GatewayKeyAuthFailedEvent }) => void,
    payload: GatewayKeyAuthFailedEvent,
  ) {
    act(() => {
      handler({ payload });
    });
  }

  it("同一服务商窗口内聚合为一条 toast，过期后再推送，不同服务商各自成条且不含密钥值", async () => {
    const title = i18n.t("aiGatewayKeyAuthFailedToastTitle");
    const alphaCount2 = i18n.t("aiGatewayKeyAuthFailedToastDescription", {
      provider: "Provider Alpha",
      count: 2,
    });
    const alphaCount1 = i18n.t("aiGatewayKeyAuthFailedToastDescription", {
      provider: "Provider Alpha",
      count: 1,
    });
    const betaCount1 = i18n.t("aiGatewayKeyAuthFailedToastDescription", {
      provider: "Provider Beta",
      count: 1,
    });

    const store: Store = {
      config: makeConfig(),
      status: makeStatus({ running: false }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<AiGateway />);
    await screen.findByTestId("ai-gateway-providers");

    const authCall = listenMock.mock.calls.find(
      ([eventName]) => eventName === AI_GATEWAY_KEY_AUTH_FAILED_EVENT,
    );
    expect(
      authCall,
      "AiGateway 必须订阅 ai-gateway-key-auth-failed 事件",
    ).toBeDefined();
    const handler = authCall![1] as (event: {
      payload: GatewayKeyAuthFailedEvent;
    }) => void;

    vi.useFakeTimers();

    // 同一服务商的两把密钥在窗口内到达：窗口未到期前不推送 toast。
    deliver(
      handler,
      authFailedEvent({ key_id: "k-a1", key_name: "sk-alpha-secret-one" }),
    );
    deliver(
      handler,
      authFailedEvent({ key_id: "k-a2", key_name: "sk-alpha-secret-two" }),
    );
    expect(screen.queryAllByText(title)).toHaveLength(0);

    act(() => {
      vi.advanceTimersByTime(AI_GATEWAY_KEY_AUTH_TOAST_WINDOW_MS);
    });

    // 窗口到期后恰好一条 toast，命名服务商并携带计数 2。
    expect(screen.getAllByText(title)).toHaveLength(1);
    expect(screen.getByText(alphaCount2)).toBeInTheDocument();
    expect(document.body.textContent).not.toContain("sk-alpha-secret-one");
    expect(document.body.textContent).not.toContain("sk-alpha-secret-two");

    // 另一服务商在自己的窗口内到达：未到期前不追加，到期后追加该服务商一条。
    deliver(
      handler,
      authFailedEvent({
        provider_id: "p-beta",
        provider_name: "Provider Beta",
        key_id: "k-b1",
        key_name: "sk-beta-secret",
      }),
    );
    expect(screen.getAllByText(title)).toHaveLength(1);

    act(() => {
      vi.advanceTimersByTime(AI_GATEWAY_KEY_AUTH_TOAST_WINDOW_MS);
    });
    expect(screen.getAllByText(title)).toHaveLength(2);
    expect(screen.getByText(betaCount1)).toBeInTheDocument();

    // 第一个服务商的窗口已过期：再次到达开新窗口并推送新 toast。
    deliver(
      handler,
      authFailedEvent({ key_id: "k-a3", key_name: "sk-alpha-secret-three" }),
    );
    expect(screen.getAllByText(title)).toHaveLength(2);

    act(() => {
      vi.advanceTimersByTime(AI_GATEWAY_KEY_AUTH_TOAST_WINDOW_MS);
    });
    expect(screen.getAllByText(title)).toHaveLength(3);
    expect(screen.getByText(alphaCount1)).toBeInTheDocument();
    expect(document.body.textContent).not.toContain("sk-alpha-secret-three");

    // 先推送的两条 toast 仍在（默认展示 5 秒，尚未到期）。
    expect(screen.getByText(alphaCount2)).toBeInTheDocument();
    expect(screen.getByText(betaCount1)).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Step 2: provider-template binding flows through the existing provider upsert
// wrapper and failed persistence keeps the dialog open (REQ-008 / AC-004 /
// AC-025). RED tests for the frozen interface contract.
// ---------------------------------------------------------------------------

describe("AiGateway 服务商模板绑定接线（Step 2）", () => {
  beforeEach(async () => {
    resetTauriMocks();
    resetMessageMocks();
    await i18n.changeLanguage("en");
  });

  function bindingTemplateView(
    overrides: Partial<GatewayProviderTemplate> = {},
  ): GatewayProviderTemplateView {
    return {
      template: {
        id: "tpl-cc",
        name: "CommandCode",
        description: "CommandCode provider template",
        base_url: "https://api.commandcode.ai/provider/v1",
        protocol: "chat_completions",
        source: "https://api.commandcode.ai/provider/v1/models",
        models_url: "https://api.commandcode.ai/provider/v1/models",
        models: [],
        sync_strategy: "generic",
        quota_strategy: "commandcode",
        error_strategy: "generic",
        ...overrides,
      },
      synced_at: null,
      source: "snapshot",
      from_snapshot: true,
    };
  }

  function mockBindingStore(options: { failUpsert?: boolean } = {}) {
    const store: Store = {
      config: makeConfig({
        providers: [
          makeProvider({
            id: "p-bind",
            name: "Bindable Provider",
            base_url: "https://bindable.example/v1",
            template_id: null,
          }),
        ],
      }),
      status: makeStatus({ provider_count: 1 }),
      targets: [],
    };
    const templates = [bindingTemplateView({ id: "tpl-cc" })];

    invokeMock.mockImplementation(async (command: string, args?: any) => {
      switch (command) {
        case "ai_gateway_get_config":
          return store.config;
        case "ai_gateway_status":
          return store.status;
        case "ai_gateway_terminal_targets":
          return store.targets;
        case "ai_gateway_provider_templates":
          return templates;
        case "ai_gateway_upsert_provider": {
          if (options.failUpsert) {
            throw new Error("upsert rejected");
          }
          store.config = {
            ...store.config,
            providers: [
              ...store.config.providers.filter(
                (entry) => entry.id !== args.provider.id,
              ),
              args.provider,
            ],
          };
          return store.config;
        }
        default:
          throw new Error(`Unhandled command: ${command}`);
      }
    });
  }

  it("通过既有 upsert 包装器提交所选模板 template_id 并保留实例字段", async () => {
    mockBindingStore();
    renderWithProviders(<AiGateway />);
    await screen.findByTestId("ai-gateway-providers");

    fireEvent.click(
      within(screen.getByTestId("ai-gateway-provider-p-bind")).getByText(
        "Bindable Provider",
      ),
    );
    await screen.findByTestId("ai-gateway-provider-detail");

    fireEvent.change(
      screen.getByTestId("ai-gateway-provider-template-select"),
      { target: { value: "tpl-cc" } },
    );
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "ai_gateway_upsert_provider",
        expect.objectContaining({
          provider: expect.objectContaining({
            id: "p-bind",
            name: "Bindable Provider",
            base_url: "https://bindable.example/v1",
            template_id: "tpl-cc",
          }),
          prices: expect.any(Array),
        }),
      ),
    );
  });

  it("upsert 失败时绑定弹窗保持打开且不报告成功", async () => {
    mockBindingStore({ failUpsert: true });
    renderWithProviders(<AiGateway />);
    await screen.findByTestId("ai-gateway-providers");

    fireEvent.click(
      within(screen.getByTestId("ai-gateway-provider-p-bind")).getByText(
        "Bindable Provider",
      ),
    );
    await screen.findByTestId("ai-gateway-provider-detail");

    fireEvent.change(
      screen.getByTestId("ai-gateway-provider-template-select"),
      { target: { value: "tpl-cc" } },
    );
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "ai_gateway_upsert_provider",
        expect.anything(),
      ),
    );
    expect(screen.getByTestId("ai-gateway-provider-detail")).toBeInTheDocument();
  });
});

describe("AI 网关头部与副标题渲染", () => {
  it("在英文环境下渲染更新后的副标题", async () => {
    await i18n.changeLanguage("en");
    renderWithProviders(<AiGateway />);
    expect(
      await screen.findByText(
        "Manage upstream providers and key pools with OpenAI / Anthropic dual-protocol relay, intelligent routing, and failover, syncing seamlessly to OpenCode, Codex, and Claude Code.",
      ),
    ).toBeInTheDocument();
  });

  it("在中文环境下渲染更新后的副标题", async () => {
    await i18n.changeLanguage("zh");
    renderWithProviders(<AiGateway />);
    expect(
      await screen.findByText(
        "统一管理多上游服务商与密钥池，提供 OpenAI / Anthropic 双协议转发、智能路由与故障转移，并一键同步到 OpenCode / Codex / Claude Code 等终端。",
      ),
    ).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Step 3 (20261013-free-model-pool): the AI Gateway console renders and
// inspects a saved model pool alongside real upstream providers (REQ-008;
// AC-011 / AC-013 / AC-014 / AC-016).
//
// `model_pools` / `model_pool_reserved_models` are typed on `GatewayConfig`, so
// the fixtures attach them through the real types. Every assertion is
// behavior-level: the card is visible, the nonsecret status command is used,
// and the enabled alias appears exactly once in the model list.
// ---------------------------------------------------------------------------

function withModelPools(
  config: GatewayConfig,
  pools: GatewayModelPool[],
): GatewayConfig {
  return { ...config, model_pools: pools };
}

function modelPoolFixture(
  overrides: Partial<GatewayModelPool> = {},
): GatewayModelPool {
  return {
    id: "pool-free",
    name: "Free Modle",
    model: "free-model",
    enabled: true,
    candidates: [
      {
        provider_id: "p1",
        local_model: "local-a",
        upstream_model: "remote-a",
        protocol: "chat_completions",
        weight: 1,
      },
    ],
    ...overrides,
  };
}

function poolStore(
  pools: GatewayModelPool[],
  poolStatus: unknown[] = [],
): Store {
  return {
    config: withModelPools(
      makeConfig({
        keys: [
          { id: "k1", label: "Main", value: "********", enabled: true, created_at: 1 },
        ],
        default_key_id: "k1",
        providers: [
          makeProvider({
            id: "p1",
            name: "Upstream A",
            mappings: [{ local_model: "local-a", upstream_model: "remote-a" }],
          }),
          makeProvider({
            id: "p2",
            name: "Upstream B",
            mappings: [{ local_model: "local-b", upstream_model: "remote-b" }],
          }),
        ],
      }),
      pools,
    ),
    status: makeStatus({ key_count: 1, default_key_id: "k1", provider_count: 2 }),
    targets: [],
    poolStatus,
  };
}

describe("AiGateway model pool integration", () => {
  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
  });

  it("renders an enabled model pool card alongside real providers", async () => {
    mockStore(poolStore([modelPoolFixture()]));

    renderWithProviders(<AiGateway />);
    await screen.findByText("Upstream A");

    fireEvent.click(
      screen.getByRole("tab", {
        name: new RegExp(`^${i18n.t("aiGatewayModelPoolsTab")}`),
      }),
    );

    expect(screen.getByText("Free Modle")).toBeInTheDocument();
    expect(screen.getByText("free-model")).toBeInTheDocument();
    expect(screen.getByText("Upstream A")).toBeInTheDocument();
  });

  it("merges nonsecret pool status through the pool status command", async () => {
    mockStore(
      poolStore(
        [modelPoolFixture()],
        [
          {
            id: "pool-free",
            name: "Free Modle",
            model: "free-model",
            enabled: true,
            candidates: [
              {
                provider_id: "p1",
                provider_name: "Upstream A",
                local_model: "local-a",
                upstream_model: "remote-a",
                protocol: "chat_completions",
                weight: 1,
                ready: false,
                availability: "missing_provider",
                reason: "missing provider 'p1'",
                cooling_until: null,
              },
            ],
          },
        ],
      ),
    );

    renderWithProviders(<AiGateway />);
    await screen.findByText("Upstream A");

    await waitFor(() => {
      expect(
        invokeMock.mock.calls.some(
          ([command]) => command === "ai_gateway_model_pools_status",
        ),
      ).toBe(true);
    });
  });

  it("lists the enabled pool once with its display name, both ingress protocols and real sources", async () => {
    mockStore(poolStore([modelPoolFixture()]));

    renderWithProviders(<AiGateway />);
    await screen.findByText("Upstream A");

    fireEvent.click(screen.getByRole("tab", { name: /Model list/i }));
    const modelList = await screen.findByTestId("ai-gateway-model-list");

    expect(modelList.querySelectorAll('[data-model="free-model"]')).toHaveLength(1);
    const row = modelList.querySelector('[data-model="free-model"]') as HTMLElement;
    expect(row).not.toBeNull();
    expect(
      within(row).getByTestId("ai-gateway-model-list-name"),
    ).toHaveTextContent("Free Modle");
    expect(within(row).getByText("Chat")).toBeInTheDocument();
    expect(within(row).getByText("Responses")).toBeInTheDocument();
    expect(
      within(row).getByTestId("ai-gateway-model-list-upstream-provider"),
    ).toHaveTextContent("Upstream A");
    expect(
      within(row).getByTestId("ai-gateway-model-list-upstream-model"),
    ).toHaveTextContent("remote-a");
    // No reasoning variants are guessed for the pool.
    expect(
      within(row).queryByTestId("ai-gateway-model-list-reasoning-efforts"),
    ).not.toBeInTheDocument();

    // Counts stay consistent: two ordinary mapping rows plus the enabled pool.
    expect(within(modelList).getAllByTestId("ai-gateway-model-list-row")).toHaveLength(3);
  });

  it("keeps an enabled all-unready pool advertised in the model list", async () => {
    mockStore(
      poolStore(
        [modelPoolFixture()],
        [
          {
            id: "pool-free",
            name: "Free Modle",
            model: "free-model",
            enabled: true,
            candidates: [
              {
                provider_id: "p1",
                provider_name: "Upstream A",
                local_model: "local-a",
                upstream_model: "remote-a",
                protocol: "chat_completions",
                weight: 1,
                ready: false,
                availability: "cooling",
                reason: "cooling after a model-scoped failure",
                cooling_until: 1_900_000_000,
              },
            ],
          },
        ],
      ),
    );

    renderWithProviders(<AiGateway />);
    await screen.findByText("Upstream A");

    fireEvent.click(screen.getByRole("tab", { name: /Model list/i }));
    const modelList = await screen.findByTestId("ai-gateway-model-list");
    expect(modelList.querySelectorAll('[data-model="free-model"]')).toHaveLength(1);
  });

  it("excludes a disabled pool from the model list", async () => {
    mockStore(poolStore([modelPoolFixture({ enabled: false })]));

    renderWithProviders(<AiGateway />);
    await screen.findByText("Upstream A");

    fireEvent.click(screen.getByRole("tab", { name: /Model list/i }));
    const modelList = await screen.findByTestId("ai-gateway-model-list");
    expect(modelList.querySelectorAll('[data-model="free-model"]')).toHaveLength(0);
    expect(within(modelList).getAllByTestId("ai-gateway-model-list-row")).toHaveLength(2);
  });

  it("does not poll pool status while the page is hidden", async () => {
    mockStore(poolStore([modelPoolFixture()]));

    renderWithProviders(<AiGateway isVisible={false} />);
    await act(async () => {
      await Promise.resolve();
    });

    const commands = invokeMock.mock.calls.map(([command]) => command);
    expect(commands).not.toContain("ai_gateway_get_config");
    expect(commands).not.toContain("ai_gateway_model_pools_status");
  });

  it("refreshes pool status exactly once when the page becomes visible", async () => {
    mockStore(poolStore([modelPoolFixture()]));

    function Host() {
      const [visible, setVisible] = useState(false);
      return (
        <>
          <button type="button" onClick={() => setVisible(true)}>
            show
          </button>
          <AiGateway isVisible={visible} />
        </>
      );
    }

    const poolStatusCalls = () =>
      invokeMock.mock.calls.filter(
        ([command]) => command === "ai_gateway_model_pools_status",
      ).length;

    renderWithProviders(<Host />);
    expect(poolStatusCalls()).toBe(0);

    fireEvent.click(screen.getByRole("button", { name: "show" }));
    await screen.findByTestId("ai-gateway-providers");
    await waitFor(() => expect(poolStatusCalls()).toBe(1));
  });

  it("switches to the dedicated model pools tab and renders model pool panel", async () => {
    mockStore(poolStore([modelPoolFixture()]));

    renderWithProviders(<AiGateway />);
    await screen.findByText("Upstream A");

    const poolTab = screen.getByRole("tab", { name: /Model pools/i });
    expect(poolTab).toBeInTheDocument();
    fireEvent.click(poolTab);

    expect(poolTab).toHaveAttribute("aria-selected", "true");
    const panel = screen.getByTestId("ai-gateway-model-pools-panel");
    expect(panel).toBeInTheDocument();
    expect(within(panel).getByText("Free Modle")).toBeInTheDocument();
  });

  it("Usage and logs expose a provider-history subtab and keep the existing subtabs", async () => {
    const store: Store = {
      config: makeConfig({ providers: [makeProvider()] }),
      status: makeStatus({ provider_count: 1 }),
      targets: [],
    };
    invokeMock.mockImplementation(async (command: string) => {
      switch (command) {
        case "ai_gateway_get_config":
          return store.config;
        case "ai_gateway_status":
          return store.status;
        case "ai_gateway_terminal_targets":
          return store.targets;
        case "ai_gateway_model_pools_status":
          return [];
        case "ai_gateway_provider_templates":
          return [];
        case "ai_gateway_template_auto_refresh_status":
          return { failures: [] };
        case "ai_gateway_usage_stats":
          return emptyUsageStats();
        case "ai_gateway_request_logs":
          return usageLogsPage();
        case "ai_gateway_usage_providers":
          return [];
        case "ai_gateway_provider_usage_history":
          return {
            provider_id: "p1",
            earliest_available_day: null,
            continuity_start_day: null,
            end_day: null,
            cumulative: null,
            days: [],
          };
        default:
          return undefined;
      }
    });

    renderWithProviders(<AiGateway />);
    await screen.findByTestId("ai-gateway-providers");

    const tabsList = screen.getByRole("tablist", { name: /AI Gateway tabs/i });
    fireEvent.click(within(tabsList).getByRole("tab", { name: /Usage & Logs/i }));

    const statsSubTab = screen.getByTestId("ai-gateway-subtab-usage-stats");
    const logsSubTab = screen.getByTestId("ai-gateway-subtab-usage-logs");
    const historySubTab = screen.getByTestId("ai-gateway-subtab-usage-history");
    expect(statsSubTab).toHaveAttribute("aria-selected", "true");
    expect(historySubTab).toHaveAttribute("aria-selected", "false");

    fireEvent.click(historySubTab);
    expect(historySubTab).toHaveAttribute("aria-selected", "true");
    expect(statsSubTab).toHaveAttribute("aria-selected", "false");
    expect(logsSubTab).toHaveAttribute("aria-selected", "false");

    expect(
      await screen.findByTestId("ai-gateway-provider-usage-history"),
    ).toBeInTheDocument();

    // Switching back keeps the existing request-log subtab reachable.
    fireEvent.click(logsSubTab);
    expect(logsSubTab).toHaveAttribute("aria-selected", "true");
    await screen.findByTestId("ai-gateway-logs-ungrouped");
  });

  it("从每日历史跳转到请求日志列表后，切换到其他 tab 会自动清空全部选择的查询条件", async () => {
    const store: Store = {
      config: makeConfig({
        providers: [makeProvider({ id: "p1", name: "Upstream A" })],
        keys: [],
      }),
      status: makeStatus({ provider_count: 1 }),
      targets: [],
    };
    mockStore(store);

    invokeMock.mockImplementation(async (command: string) => {
      switch (command) {
        case "ai_gateway_get_config":
          return store.config;
        case "ai_gateway_status":
          return store.status;
        case "ai_gateway_terminal_targets":
          return store.targets;
        case "ai_gateway_model_pools_status":
          return [];
        case "ai_gateway_template_auto_refresh_status":
          return { failures: [] };
        case "ai_gateway_provider_templates":
          return [];
        case "ai_gateway_usage_stats":
          return emptyUsageStats();
        case "ai_gateway_request_logs":
          return usageLogsPage();
        case "ai_gateway_usage_providers":
          return [{ provider_id: "p1", provider_name: "Upstream A" }];
        case "ai_gateway_provider_usage_history":
          return {
            provider_id: "p1",
            earliest_available_day: "2026-09-17",
            continuity_start_day: "2026-09-17",
            end_day: "2026-09-17",
            cumulative: null,
            days: [
              {
                day: "2026-09-17",
                metrics: {
                  request_count: 1,
                  error_count: 0,
                  input_tokens: 10,
                  output_tokens: 20,
                  cache_read_tokens: 0,
                  cache_write_tokens: 0,
                  total_tokens: 30,
                  amount: 0.001,
                  unpriced_token_requests: 0,
                  cache_coverage: 1.0,
                },
              },
            ],
          };
        default:
          return undefined;
      }
    });

    renderWithProviders(<AiGateway />);
    await screen.findByTestId("ai-gateway-providers");

    const tabsList = screen.getByRole("tablist", { name: /AI Gateway tabs/i });
    fireEvent.click(within(tabsList).getByRole("tab", { name: /Usage & Logs/i }));

    const statsSubTab = screen.getByTestId("ai-gateway-subtab-usage-stats");
    const logsSubTab = screen.getByTestId("ai-gateway-subtab-usage-logs");
    const historySubTab = screen.getByTestId("ai-gateway-subtab-usage-history");

    fireEvent.click(historySubTab);
    const historyPanel = await screen.findByTestId("ai-gateway-provider-usage-history");

    fireEvent.click(within(historyPanel).getByTestId("ai-gateway-provider-history-provider-trigger"));
    fireEvent.click(within(historyPanel).getByRole("option", { name: "Upstream A" }));

    const viewButton = await within(historyPanel).findByTestId(
      "ai-gateway-provider-history-open-day-2026-09-17",
    );
    fireEvent.click(viewButton);

    expect(logsSubTab).toHaveAttribute("aria-selected", "true");
    const logsPanel = await screen.findByTestId("ai-gateway-usage-logs");

    await waitFor(() => {
      expect(within(logsPanel).getByTestId("ai-gateway-logs-range-trigger")).toHaveTextContent("2026-09-17");
    });

    // 切换到其他 tab
    fireEvent.click(statsSubTab);
    expect(statsSubTab).toHaveAttribute("aria-selected", "true");

    // 再次切回 logsSubTab
    fireEvent.click(logsSubTab);
    expect(logsSubTab).toHaveAttribute("aria-selected", "true");

    // 验证全部查询条件已被清空，恢复为默认的 Today
    await waitFor(() => {
      expect(within(logsPanel).getByTestId("ai-gateway-logs-range-trigger")).toHaveTextContent(/Today|今日/i);
    });
  });
});

// ---------------------------------------------------------------------------
// Step 4 (20261015-pool-only-reasoning) vertical UI acceptance slice:
// AC-008 (pool-only editing + persisted defaults), AC-009 UI (configured pool
// capability), AC-012 (no provider mutation / ordinary isolation) and AC-014
// (keyboard-driven bilingual accessible names) in one console interaction.
// ---------------------------------------------------------------------------

describe("AiGateway 模型池推理纵向切片（Step 4）", () => {
  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
  });

  it("新池默认 medium、可改 auto 保存、重开保留且不触碰服务商，键盘与双语可用", async () => {
    const user = userEvent.setup();
    const provider = makeProvider({
      id: "p-a",
      name: "Provider A",
      protocol: "chat_completions",
      mappings: [{ local_model: "local-a1", upstream_model: "remote-a1" }],
    });

    const store: { config: GatewayConfig; poolStatus: unknown[] } = {
      config: makeConfig({ providers: [provider], model_pools: [] }),
      poolStatus: [],
    };

    invokeMock.mockImplementation(async (command: string, args?: any) => {
      switch (command) {
        case "ai_gateway_get_config":
          return store.config;
        case "ai_gateway_status":
          return makeStatus({ provider_count: 1 });
        case "ai_gateway_provider_templates":
          return [];
        case "ai_gateway_terminal_targets":
          return [];
        case "ai_gateway_model_pools_status":
          return store.poolStatus;
        case "ai_gateway_upsert_model_pool": {
          const pool = args?.pool as Record<string, unknown>;
          store.config = makeConfig({
            providers: [provider],
            model_pools: [pool as unknown as GatewayModelPool],
          });
          store.poolStatus = [
            {
              id: pool.id,
              name: pool.name,
              model: pool.model,
              enabled: true,
              default_reasoning_effort: "auto",
              reasoning_efforts: [],
              reasoning_coverage: [],
              candidates: [],
            },
          ];
          return store.config;
        }
        default:
          return undefined;
      }
    });

    renderWithProviders(<AiGateway />);
    await screen.findByText("Provider A");
    fireEvent.click(screen.getByRole("tab", { name: /Model pools/i }));
    await user.click(screen.getByTestId("ai-gateway-add-model-pool"));

    const dialog = await screen.findByTestId("model-pool-dialog");
    const defaultSelect = screen.getByLabelText(
      i18n.t("aiGatewayModelPoolReasoningDefaultLabel"),
    );
    expect(defaultSelect).toHaveValue("medium");

    await user.selectOptions(defaultSelect, "auto");
    await user.click(
      within(dialog).getByRole("checkbox", { name: /local-a1/i }),
    );
    await user.click(screen.getByTestId("model-pool-save"));

    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some(
          ([command]) => command === "ai_gateway_upsert_model_pool",
        ),
      ).toBe(true),
    );

    const upsertCall = invokeMock.mock.calls.find(
      ([command]) => command === "ai_gateway_upsert_model_pool",
    );
    const payload = (upsertCall?.[1] as { pool: Record<string, unknown> }).pool;
    expect(payload.default_reasoning_effort).toBe("auto");
    // AC-012: the pool save must not mutate the referenced provider.
    expect(
      invokeMock.mock.calls.some(
        ([command]) => command === "ai_gateway_upsert_provider",
      ),
    ).toBe(false);

    // Reopen the saved pool: the pool-local default round-trips.
    const card = await screen.findByTestId(
      `ai-gateway-model-pool-${String(payload.id)}`,
    );
    await user.click(
      within(card).getByTestId(
        `ai-gateway-model-pool-name-${String(payload.id)}`,
      ),
    );
    await screen.findByTestId("model-pool-dialog");
    expect(
      screen.getByLabelText(i18n.t("aiGatewayModelPoolReasoningDefaultLabel")),
    ).toHaveValue("auto");

    // AC-014: the accessible name is localized in Chinese.
    await act(async () => {
      await i18n.changeLanguage("zh");
    });
    expect(
      screen.getByLabelText(i18n.t("aiGatewayModelPoolReasoningDefaultLabel")),
    ).toHaveValue("auto");
    await act(async () => {
      await i18n.changeLanguage("en");
    });
  });
});

