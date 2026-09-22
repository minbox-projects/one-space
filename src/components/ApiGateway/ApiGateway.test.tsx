import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { ApiGateway } from "@/components/ApiGateway";
import {
  API_GATEWAY_KEY_MASK,
  formatGatewayTimestamp,
  maskSecret,
  type GatewayConfig,
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
} from "@/lib/apiGateway";
import { renderWithProviders } from "@/test/mocks/render";
import { emitMock, invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

type Store = {
  config: GatewayConfig;
  status: GatewayStatus;
  targets: GatewayTerminalTarget[];
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

function mockStore(store: Store) {
  invokeMock.mockImplementation(async (command: string, args?: any) => {
    switch (command) {
      case "api_gateway_get_config":
        return store.config;
      case "api_gateway_status":
        return store.status;
      case "api_gateway_terminal_targets":
        return store.targets;
      case "api_gateway_start":
        store.status = { ...store.status, running: true };
        return store.status;
      case "api_gateway_stop":
        store.status = { ...store.status, running: false };
        return store.status;
      case "api_gateway_configure_terminal":
        return store.config.terminal_syncs;
      case "api_gateway_sync_terminal":
        return store.config.terminal_syncs;
      case "api_gateway_usage_stats":
        return emptyUsageStats({ request_count: 1, total_tokens: 2 });
      case "api_gateway_request_logs":
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
    if (command === "api_gateway_upsert_provider") {
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

function makeProvider(
  overrides: Partial<GatewayUpstreamProvider> = {},
): GatewayUpstreamProvider {
  return {
    id: "p1",
    name: "Upstream A",
    base_url: "https://api.a.example",
    api_key: API_GATEWAY_KEY_MASK,
    default_model: null,
    mappings: [],
    enabled: true,
    auto_disabled: false,
    disabled_reason: null,
    disabled_at: null,
    consecutive_failures: 0,
    last_error_at: null,
    ...overrides,
  };
}

describe("ApiGateway", () => {
  let writeText: ReturnType<typeof vi.fn>;

  beforeEach(async () => {
    resetTauriMocks();
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
        keys: [{ id: "k1", label: "Main", value: API_GATEWAY_KEY_MASK, enabled: true, created_at: 1 }],
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

    renderWithProviders(<ApiGateway />);
    await screen.findByText("OpenCode");

    expect(screen.queryByText("Claude Code")).not.toBeInTheDocument();
    expect(screen.queryByText("Antigravity")).not.toBeInTheDocument();

    const panel = within(screen.getByTestId("api-gateway-terminals"));
    // The header-level "Add provider / Sync again" actions must be gone:
    // every button inside the panel belongs to a terminal target row.
    const buttons = panel.getAllByRole("button");
    expect(buttons.length).toBeGreaterThan(0);
    for (const button of buttons) {
      expect(button.closest('[data-testid^="api-gateway-target-"]')).not.toBeNull();
    }
    expect(
      panel.queryByRole("button", { name: /sync again/i }),
    ).not.toBeInTheDocument();
  });

  it("待同步的行内显示添加，点击仅以该工具调用配置", async () => {
    const store: Store = {
      config: makeConfig({
        keys: [{ id: "k1", label: "Main", value: API_GATEWAY_KEY_MASK, enabled: true, created_at: 1 }],
        default_key_id: "k1",
      }),
      status: makeStatus({ key_count: 1, default_key_id: "k1" }),
      targets: [
        openCodeTarget({ provider_id: "gw-open" }),
        openCodeTarget({ provider_id: "gw-codex", tool: "codex", name: "Codex" }),
      ],
    };
    mockStore(store);

    renderWithProviders(<ApiGateway />);
    await screen.findByText("OpenCode");

    const openRow = within(screen.getByTestId("api-gateway-target-opencode"));
    const addButton = openRow.getByRole("button", { name: /add/i });
    expect(addButton).toBeEnabled();
    fireEvent.click(addButton);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_configure_terminal", {
        targetTools: ["opencode"],
      }),
    );
    const configureCalls = invokeMock.mock.calls.filter(
      ([command]) => command === "api_gateway_configure_terminal",
    );
    expect(configureCalls).toHaveLength(1);
    expect(invokeMock).not.toHaveBeenCalledWith("api_gateway_configure_terminal", {
      targetTools: ["opencode", "codex"],
    });
  });

  it("已添加的行内显示同步，点击仅以该工具调用同步", async () => {
    const store: Store = {
      config: makeConfig({
        keys: [{ id: "k1", label: "Main", value: API_GATEWAY_KEY_MASK, enabled: true, created_at: 1 }],
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

    renderWithProviders(<ApiGateway />);
    await screen.findByText("OpenCode");

    const openRow = within(screen.getByTestId("api-gateway-target-opencode"));
    const syncButton = openRow.getByRole("button", { name: /sync/i });
    expect(syncButton).toBeEnabled();
    fireEvent.click(syncButton);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_sync_terminal", {
        targetTools: ["opencode"],
      }),
    );
    const syncCalls = invokeMock.mock.calls.filter(
      ([command]) => command === "api_gateway_sync_terminal",
    );
    expect(syncCalls).toHaveLength(1);
    expect(invokeMock).not.toHaveBeenCalledWith("api_gateway_sync_terminal", {
      targetTools: ["opencode", "codex"],
    });
  });

  it("已添加但 Key 或地址漂移（synced 为真且待同步）时行内显示同步并仅走同步通道", async () => {
    const store: Store = {
      config: makeConfig({
        keys: [{ id: "k1", label: "Main", value: API_GATEWAY_KEY_MASK, enabled: true, created_at: 1 }],
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

    renderWithProviders(<ApiGateway />);
    await screen.findByText("OpenCode");

    const openRow = within(screen.getByTestId("api-gateway-target-opencode"));
    const action = openRow.getByRole("button", { name: /sync/i });
    expect(action).toBeEnabled();
    expect(action).toHaveAccessibleName(/sync/i);
    expect(action).not.toHaveAccessibleName(/sync again/i);
    fireEvent.click(action);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_sync_terminal", {
        targetTools: ["opencode"],
      }),
    );
    const syncCalls = invokeMock.mock.calls.filter(
      ([command]) => command === "api_gateway_sync_terminal",
    );
    expect(syncCalls).toHaveLength(1);
    expect(invokeMock).not.toHaveBeenCalledWith(
      "api_gateway_configure_terminal",
      expect.anything(),
    );
  });

  it("网关服务商被删除但同步台账仍在（synced 为假且 synced_key_id/synced_at 非空）时行内显示同步并仅走同步通道", async () => {
    const store: Store = {
      config: makeConfig({
        keys: [{ id: "k1", label: "Main", value: API_GATEWAY_KEY_MASK, enabled: true, created_at: 1 }],
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

    renderWithProviders(<ApiGateway />);
    await screen.findByText("OpenCode");

    const openRow = within(screen.getByTestId("api-gateway-target-opencode"));
    const action = openRow.getByRole("button", { name: /sync/i });
    expect(action).toBeEnabled();
    expect(action).toHaveAccessibleName(/sync/i);
    expect(action).not.toHaveAccessibleName(/sync again/i);
    fireEvent.click(action);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_sync_terminal", {
        targetTools: ["opencode"],
      }),
    );
    const syncCalls = invokeMock.mock.calls.filter(
      ([command]) => command === "api_gateway_sync_terminal",
    );
    expect(syncCalls).toHaveLength(1);
    expect(invokeMock).not.toHaveBeenCalledWith(
      "api_gateway_configure_terminal",
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

    renderWithProviders(<ApiGateway />);
    await screen.findByText("OpenCode");

    expect(screen.getByTestId("api-gateway-default-key-required")).toHaveTextContent(
      /Add and enable a local key/,
    );
    const openRow = within(screen.getByTestId("api-gateway-target-opencode"));
    const action = openRow.getByRole("button", { name: /add/i });
    expect(action).toBeDisabled();

    fireEvent.click(action);
    expect(invokeMock).not.toHaveBeenCalledWith(
      "api_gateway_configure_terminal",
      expect.anything(),
    );
    expect(invokeMock).not.toHaveBeenCalledWith(
      "api_gateway_sync_terminal",
      expect.anything(),
    );
  });

  it("从未添加的行内显示添加，点击走配置通道并在配置后清除待同步", async () => {
    const store: Store = {
      config: makeConfig({
        keys: [
          { id: "k1", label: "Main", value: API_GATEWAY_KEY_MASK, enabled: true, created_at: 1 },
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
        case "api_gateway_get_config":
          return store.config;
        case "api_gateway_status":
          return store.status;
        case "api_gateway_terminal_targets":
          return store.targets;
        case "api_gateway_configure_terminal": {
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

    renderWithProviders(<ApiGateway />);
    await screen.findByText("OpenCode");

    // Pending status comes from the backend target payload, not local re-derivation.
    expect(screen.getByText("Pending sync")).toBeInTheDocument();

    const openRow = within(screen.getByTestId("api-gateway-target-opencode"));
    fireEvent.click(openRow.getByRole("button", { name: /add/i }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_configure_terminal", {
        targetTools: ["opencode"],
      }),
    );
    await waitFor(() =>
      expect(screen.queryByText("Pending sync")).not.toBeInTheDocument(),
    );
    expect(
      screen.getByTestId("api-gateway-target-synced-opencode"),
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

    renderWithProviders(<ApiGateway />);
    await screen.findByText("OpenCode");

    const panel = within(screen.getByTestId("api-gateway-terminals"));
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

    renderWithProviders(<ApiGateway />);
    await screen.findByText("OpenCode");

    const openCodeSyncedEl = screen.getByTestId("api-gateway-target-synced-opencode");
    expect(openCodeSyncedEl).toHaveTextContent(formatGatewayTimestamp(syncedAt)!);
    expect(openCodeSyncedEl).toHaveTextContent("Last sync");

    const codexSyncedEl = screen.getByTestId("api-gateway-target-synced-codex");
    expect(codexSyncedEl).toHaveTextContent("Not synced yet");
    expect(codexSyncedEl).toHaveTextContent("Last sync");
  });

  it("展示运行状态、端口、自动禁用原因与时间，并支持复制地址与掩码 Key", async () => {
    const disabledAt = 1_700_000_000;
    const rawKey = "sk-live-secret-1234";
    const store: Store = {
      config: makeConfig({
        providers: [
          {
            id: "p1",
            name: "Upstream Broken",
            base_url: "https://api.broken.example",
            api_key: API_GATEWAY_KEY_MASK,
            default_model: null,
            mappings: [],
            enabled: true,
            auto_disabled: true,
            disabled_reason: "HTTP 401",
            disabled_at: disabledAt,
            consecutive_failures: 3,
            last_error_at: disabledAt,
          },
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

    renderWithProviders(<ApiGateway />);

    expect(await screen.findByTestId("api-gateway-runtime-state")).toHaveTextContent(
      "Running",
    );
    expect(screen.getByTestId("api-gateway-local-address")).toHaveTextContent(
      "http://127.0.0.1:17688/v1",
    );
    expect(screen.getByText(/Reason: HTTP 401/)).toBeInTheDocument();
    expect(
      screen.getByText(
        new RegExp(`Disabled at ${formatGatewayTimestamp(disabledAt)!}`),
      ),
    ).toBeInTheDocument();
    expect(screen.getByTestId("api-gateway-auto-disabled-count")).toHaveTextContent("1");

    fireEvent.click(screen.getByRole("button", { name: /Copy local API address/ }));
    await waitFor(() =>
      expect(writeText).toHaveBeenCalledWith("http://127.0.0.1:17688/v1"),
    );

    const masked = screen.getByTestId("api-gateway-key-value-k1");
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

    renderWithProviders(<ApiGateway />);
    fireEvent.click(
      await within(
        await screen.findByTestId("api-gateway-providers"),
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

    renderWithProviders(<ApiGateway />);
    fireEvent.click(
      await within(
        await screen.findByTestId("api-gateway-providers"),
      ).findByText("Upstream A"),
    );
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "api_gateway_upsert_provider",
        expect.objectContaining({
          provider: expect.objectContaining({ id: "p1" }),
        }),
      ),
    );

    const call = invokeMock.mock.calls.find(
      ([command]) => command === "api_gateway_upsert_provider",
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

    renderWithProviders(<ApiGateway />);
    fireEvent.click(
      await within(
        await screen.findByTestId("api-gateway-providers"),
      ).findByText("Upstream A"),
    );
    fireEvent.change(screen.getByLabelText("Mapping protocol 1"), {
      target: { value: "responses" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "api_gateway_upsert_provider",
        expect.objectContaining({
          provider: expect.objectContaining({ id: "p1" }),
        }),
      ),
    );

    const call = invokeMock.mock.calls.find(
      ([command]) => command === "api_gateway_upsert_provider",
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

    renderWithProviders(<ApiGateway />);
    fireEvent.click(
      await within(
        await screen.findByTestId("api-gateway-providers"),
      ).findByText("Upstream A"),
    );

    fireEvent.click(screen.getByTestId("api-gateway-mapping-expand-0"));
    await screen.findByTestId("api-gateway-price-0-input");
    fireEvent.change(screen.getByTestId("api-gateway-price-0-input"), {
      target: { value: "2" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "api_gateway_upsert_provider",
        expect.objectContaining({
          provider: expect.objectContaining({ id: "p1" }),
          prices: expect.any(Array),
        }),
      ),
    );

    const upsertCalls = invokeMock.mock.calls.filter(
      ([command]) => command === "api_gateway_upsert_provider",
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

  it("新增本地 Key 只需名称，值留空交由后端随机生成", async () => {
    const store: Store = {
      config: makeConfig({ keys: [], default_key_id: null }),
      status: makeStatus({ key_count: 0, default_key_id: null }),
      targets: [],
    };
    invokeMock.mockImplementation(async (command: string) => {
      switch (command) {
        case "api_gateway_get_config":
        case "api_gateway_upsert_key":
          return store.config;
        case "api_gateway_status":
          return store.status;
        case "api_gateway_terminal_targets":
          return store.targets;
        default:
          throw new Error(`Unhandled command: ${command}`);
      }
    });

    renderWithProviders(<ApiGateway />);
    await screen.findByTestId("api-gateway-keys");

    // 页头不再有内联名称输入框，"新增 Key" 按钮始终可用
    expect(screen.queryByLabelText("Name")).not.toBeInTheDocument();
    const addButton = screen.getByRole("button", { name: /Add key/ });
    expect(addButton).toBeEnabled();

    // 点击按钮弹出对话框
    fireEvent.click(addButton);
    const dialog = await screen.findByTestId("api-gateway-key-dialog");
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
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_upsert_key", {
        key: expect.objectContaining({ label: "CI", value: "" }),
      }),
    );

    // 值留空交由后端随机生成，其余字段按约定初始化
    const call = invokeMock.mock.calls.find(
      ([command]) => command === "api_gateway_upsert_key",
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

    renderWithProviders(<ApiGateway />);
    await screen.findByTestId("api-gateway-keys");

    fireEvent.click(screen.getByRole("button", { name: /Add key/ }));
    const dialog = await screen.findByTestId("api-gateway-key-dialog");
    fireEvent.change(within(dialog).getByLabelText("Name"), {
      target: { value: "CI" },
    });
    expect(within(dialog).getByLabelText("Name")).toHaveValue("CI");

    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() =>
      expect(
        screen.queryByTestId("api-gateway-key-dialog"),
      ).not.toBeInTheDocument(),
    );

    fireEvent.click(screen.getByRole("button", { name: /Add key/ }));
    const reopened = await screen.findByTestId("api-gateway-key-dialog");
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
        case "api_gateway_get_config":
        case "api_gateway_upsert_key":
          return store.config;
        case "api_gateway_status":
          return store.status;
        case "api_gateway_terminal_targets":
          return store.targets;
        default:
          throw new Error(`Unhandled command: ${command}`);
      }
    });

    renderWithProviders(<ApiGateway />);
    await screen.findByTestId("api-gateway-keys");

    fireEvent.click(screen.getByRole("button", { name: /Add key/ }));
    const dialog = await screen.findByTestId("api-gateway-key-dialog");
    const nameInput = within(dialog).getByLabelText("Name");
    fireEvent.change(nameInput, { target: { value: "CI" } });
    fireEvent.keyDown(nameInput, { key: "Enter" });

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_upsert_key", {
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
        case "api_gateway_get_config":
          return store.config;
        case "api_gateway_upsert_key":
          throw new Error("boom");
        case "api_gateway_status":
          return store.status;
        case "api_gateway_terminal_targets":
          return store.targets;
        default:
          throw new Error(`Unhandled command: ${command}`);
      }
    });

    renderWithProviders(<ApiGateway />);
    await screen.findByTestId("api-gateway-keys");

    fireEvent.click(screen.getByRole("button", { name: /Add key/ }));
    const dialog = await screen.findByTestId("api-gateway-key-dialog");
    fireEvent.change(within(dialog).getByLabelText("Name"), {
      target: { value: "CI" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(screen.getByTestId("api-gateway-key-dialog")).toBeInTheDocument(),
    );
    expect(
      within(screen.getByTestId("api-gateway-key-dialog")).getByLabelText("Name"),
    ).toHaveValue("CI");
  });

  it("支持在 Tabs 之间顺畅切换且保持各面板挂载与状态", async () => {
    const store: Store = {
      config: makeConfig({
        providers: [makeProvider()],
        keys: [{ id: "k1", label: "Dev Key", value: API_GATEWAY_KEY_MASK, enabled: true, created_at: 1 }],
      }),
      status: makeStatus({ provider_count: 1, key_count: 1 }),
      targets: [openCodeTarget()],
    };
    mockStore(store);

    renderWithProviders(<ApiGateway />);
    await within(
      await screen.findByTestId("api-gateway-providers"),
    ).findByText("Upstream A");

    const tabsList = screen.getByRole("tablist", { name: /API Gateway tabs/i });
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
        keys: [{ id: "k1", label: "Dev Key", value: API_GATEWAY_KEY_MASK, enabled: true, created_at: 1 }],
      }),
      status: makeStatus({ provider_count: 1, key_count: 1 }),
      targets: [openCodeTarget()],
    };
    mockStore(store);

    renderWithProviders(<ApiGateway />);
    await within(
      await screen.findByTestId("api-gateway-providers"),
    ).findByText("Upstream A");

    const tabsList = screen.getByRole("tablist", { name: /API Gateway tabs/i });
    const usageTab = within(tabsList).getByRole("tab", { name: /Usage & Logs/i });
    const providersTab = within(tabsList).getByRole("tab", {
      name: /Upstream providers/,
    });

    fireEvent.click(usageTab);
    expect(usageTab).toHaveAttribute("aria-selected", "true");

    // 默认展示用量统计二级子页
    const statsSubTab = screen.getByTestId("api-gateway-subtab-usage-stats");
    const logsSubTab = screen.getByTestId("api-gateway-subtab-usage-logs");
    expect(statsSubTab).toHaveAttribute("aria-selected", "true");
    expect(logsSubTab).toHaveAttribute("aria-selected", "false");

    const usagePanel = await screen.findByTestId("api-gateway-usage-stats");
    await within(usagePanel).findByTestId("api-gateway-usage-card-requests");
    fireEvent.click(within(usagePanel).getByTestId("api-gateway-usage-range-trigger"));
    fireEvent.click(screen.getByRole("option", { name: "7d" }));
    await within(usagePanel).findByTestId("api-gateway-usage-card-requests");

    // The legacy model-price entry must not exist anywhere in the usage panel.
    expect(
      within(usagePanel).queryByRole("button", { name: "Model prices" }),
    ).not.toBeInTheDocument();

    // 切换到请求日志二级子页
    fireEvent.click(logsSubTab);
    expect(logsSubTab).toHaveAttribute("aria-selected", "true");
    expect(statsSubTab).toHaveAttribute("aria-selected", "false");

    await screen.findByTestId("api-gateway-logs-ungrouped");
    const logsPanel = screen.getByTestId("api-gateway-usage-logs");
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
      within(usagePanel).getByTestId("api-gateway-usage-range-trigger"),
    ).toHaveTextContent("7d");

    fireEvent.click(logsSubTab);
    await screen.findByText("Page 2 / 3");

    // Grouping selection also survives a tab round-trip.
    fireEvent.click(
      within(logsPanel).getByTestId("api-gateway-logs-group-trigger"),
    );
    fireEvent.click(screen.getByRole("option", { name: "Day (UTC+8)" }));
    await screen.findByTestId("api-gateway-logs-grouped");
    fireEvent.click(providersTab);
    fireEvent.click(usageTab);
    fireEvent.click(logsSubTab);
    expect(
      within(logsPanel).getByTestId("api-gateway-logs-group-trigger"),
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
        case "api_gateway_get_config":
          return store.config;
        case "api_gateway_status":
          return store.status;
        case "api_gateway_terminal_targets":
          return store.targets;
        case "api_gateway_upsert_provider":
          store.config = {
            ...store.config,
            providers: [...store.config.providers, { ...args.provider, id: "p-new" }],
          };
          return store.config;
        default:
          throw new Error(`Unhandled command: ${command}`);
      }
    });

    renderWithProviders(<ApiGateway />);
    await screen.findByTestId("api-gateway-providers");

    const addButtons = screen.getAllByRole("button", { name: /Add provider/i });
    fireEvent.click(addButtons[0]);

    const blankBtn = await screen.findByTestId("template-picker-blank-btn");
    fireEvent.click(blankBtn);

    const dialog = await screen.findByTestId("api-gateway-provider-detail");
    expect(dialog).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("Name"), { target: { value: "New Remote" } });
    fireEvent.change(screen.getByLabelText("API base URL"), {
      target: { value: "https://new.example.com" },
    });

    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_upsert_provider", {
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

    renderWithProviders(<ApiGateway />);
    const runningToggleBtn = await screen.findByTestId("api-gateway-toggle-service");
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

    renderWithProviders(<ApiGateway />);
    const stoppedToggleBtn = await screen.findByTestId("api-gateway-toggle-service");
    expect(stoppedToggleBtn).toHaveAttribute("title", "Start service");
    expect(stoppedToggleBtn).toHaveClass("text-emerald-600");
  });

  it("启停服务成功后广播 api-gateway-status-update 事件", async () => {
    const store: Store = {
      config: makeConfig(),
      status: makeStatus({ running: false }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<ApiGateway />);
    const toggleBtn = await screen.findByTestId("api-gateway-toggle-service");
    fireEvent.click(toggleBtn);

    await waitFor(() => {
      expect(emitMock).toHaveBeenCalledWith("api-gateway-status-update");
    });
  });

  it("顶部状态卡片展示健康率、聚合模型数、有效密钥及终端同步指标，且点击指标卡可切换对应 Tab", async () => {
    const provider1 = makeProvider({
      id: "p1",
      name: "Provider 1",
      enabled: true,
      auto_disabled: false,
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
      auto_disabled: false,
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

    renderWithProviders(<ApiGateway />);

    // 1. 服务商健康率展示：1/2 在线 (因为 provider2 是 disabled)
    const healthCard = await screen.findByTestId("api-gateway-metric-health");
    expect(within(healthCard).getByText("1/2")).toBeInTheDocument();
    expect(within(healthCard).getByText(/Provider health/i)).toBeInTheDocument();

    // 2. 聚合模型数：provider1 贡献了 gpt-4o 和 claude-3-7-sonnet（去重后 2 个，provider2 被禁用不计入）
    const modelsCard = screen.getByTestId("api-gateway-metric-models");
    expect(within(modelsCard).getByText("2")).toBeInTheDocument();
    expect(within(modelsCard).getByText(/Aggregated models/i)).toBeInTheDocument();

    // 3. 今日请求量指标卡（高频业务）：显示 1 次请求及失败告警 (mock 返回 error_count: 1)
    const requestsCard = await screen.findByTestId("api-gateway-metric-requests");
    expect(within(requestsCard).getByText("1")).toBeInTheDocument();
    expect(within(requestsCard).getByText(/Today's requests|今日请求/i)).toBeInTheDocument();
    expect(
      within(requestsCard).getByTestId("api-gateway-today-failed-requests"),
    ).toHaveTextContent(/1.*failed|1.*失败/i);

    // 4. 今日消耗指标卡（高频消耗）：显示 2 Tokens
    const tokensCard = await screen.findByTestId("api-gateway-metric-tokens");
    expect(within(tokensCard).getByText("2")).toBeInTheDocument();
    expect(within(tokensCard).getByText(/Today's tokens|今日消耗/i)).toBeInTheDocument();

    // 5. 顶部操作栏提供默认 Key 快捷复制入口
    expect(screen.getByTestId("api-gateway-default-key-preview")).toBeInTheDocument();
    expect(screen.getByTestId("api-gateway-copy-default-key")).toBeInTheDocument();

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
    expect(screen.getByTestId("api-gateway-model-list")).toBeInTheDocument();
  });

  it("点击聚合模型指标卡切换到模型列表Tab并列出有效模型及其上游服务商映射", async () => {
    const provider1 = makeProvider({
      id: "p1",
      name: "Provider 1",
      protocol: "chat_completions",
      enabled: true,
      auto_disabled: false,
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
      auto_disabled: false,
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

    renderWithProviders(<ApiGateway />);

    const modelsCard = await screen.findByTestId("api-gateway-metric-models");
    const cardModelCount = Number(within(modelsCard).getByText("2").textContent);
    fireEvent.click(modelsCard);

    const modelsTab = screen.getByRole("tab", { name: /Model list|模型列表/i });
    expect(modelsTab).toHaveAttribute("aria-selected", "true");

    const panel = await screen.findByTestId("api-gateway-model-list");

    const modelNodes = within(panel).getAllByTestId("api-gateway-model-list-row");
    expect(modelNodes).toHaveLength(2);
    expect(modelNodes).toHaveLength(cardModelCount);

    const modelIds = within(panel)
      .getAllByTestId("api-gateway-model-list-id")
      .map((node) => node.textContent?.trim());
    expect(modelIds.sort()).toEqual(["claude-3-7-sonnet", "gpt-4o"]);

    expect(
      within(panel).getAllByTestId("api-gateway-model-list-upstream"),
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
      auto_disabled: false,
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

    renderWithProviders(<ApiGateway />);

    fireEvent.click(await screen.findByTestId("api-gateway-metric-models"));

    const panel = await screen.findByTestId("api-gateway-model-list");
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

    renderWithProviders(<ApiGateway />);

    const keysTab = await screen.findByRole("tab", { name: /API Keys|API 密钥/i });
    fireEvent.click(keysTab);
    expect(keysTab).toHaveAttribute("aria-selected", "true");

    fireEvent.keyDown(screen.getByTestId("api-gateway-metric-models"), {
      key: "Enter",
    });

    const modelsTab = screen.getByRole("tab", { name: /Model list|模型列表/i });
    expect(modelsTab).toHaveAttribute("aria-selected", "true");
    expect(await screen.findByTestId("api-gateway-model-list")).toBeInTheDocument();
  });

  it("在聚合模型指标卡上按空格切换到模型列表Tab", async () => {
    const store: Store = {
      config: makeConfig({ providers: [makeProvider()] }),
      status: makeStatus({ provider_count: 1 }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<ApiGateway />);

    const keysTab = await screen.findByRole("tab", { name: /API Keys|API 密钥/i });
    fireEvent.click(keysTab);
    expect(keysTab).toHaveAttribute("aria-selected", "true");

    fireEvent.keyDown(screen.getByTestId("api-gateway-metric-models"), {
      key: " ",
    });

    const modelsTab = screen.getByRole("tab", { name: /Model list|模型列表/i });
    expect(modelsTab).toHaveAttribute("aria-selected", "true");
    expect(await screen.findByTestId("api-gateway-model-list")).toBeInTheDocument();
  });

  it("没有启用服务商时点击聚合模型指标卡切换到模型列表展示空态且不渲染任何模型", async () => {
    const disabledProvider = makeProvider({
      id: "p1",
      name: "Provider 1",
      enabled: false,
      auto_disabled: false,
      default_model: "gpt-4o",
      mappings: [{ local_model: "gpt-4o", upstream_model: "gpt-4o-2024" }],
    });

    const store: Store = {
      config: makeConfig({ providers: [disabledProvider] }),
      status: makeStatus({ provider_count: 1 }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<ApiGateway />);

    fireEvent.click(await screen.findByTestId("api-gateway-metric-models"));

    const panel = await screen.findByTestId("api-gateway-model-list");
    expect(
      within(panel).getByTestId("api-gateway-model-list-empty"),
    ).toBeInTheDocument();
    expect(
      within(panel).queryAllByTestId("api-gateway-model-list-row"),
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

    renderWithProviders(<ApiGateway />);
    fireEvent.click(
      await within(
        await screen.findByTestId("api-gateway-providers"),
      ).findByText("Upstream A"),
    );

    fireEvent.click(screen.getByRole("switch", { name: "Enable mapping 2" }));
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "api_gateway_upsert_provider",
        expect.objectContaining({
          provider: expect.objectContaining({ id: "p1" }),
        }),
      ),
    );

    const call = invokeMock.mock.calls.find(
      ([command]) => command === "api_gateway_upsert_provider",
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
      auto_disabled: false,
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

    renderWithProviders(<ApiGateway />);

    fireEvent.click(await screen.findByTestId("api-gateway-metric-models"));

    const panel = await screen.findByTestId("api-gateway-model-list");
    const modelRows = within(panel).getAllByTestId(
      "api-gateway-model-list-row",
    );
    const modelNames = modelRows.map(
      (node) =>
        within(node).getByTestId("api-gateway-model-list-id").textContent?.trim(),
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
      auto_disabled: false,
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
      auto_disabled: false,
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

    renderWithProviders(<ApiGateway />);

    const modelsCard = await screen.findByTestId("api-gateway-metric-models");
    const metricCount = Number(within(modelsCard).getByText("3").textContent);
    expect(metricCount).toBe(3);

    const tabsList = screen.getByRole("tablist", {
      name: /API Gateway tabs/i,
    });
    const tabNodes = within(tabsList).getAllByRole("tab");
    const providersIndex = tabNodes.findIndex((node) =>
      (node.textContent ?? "").includes("Upstream providers"),
    );
    const modelsIndex = tabNodes.findIndex((node) =>
      (node.textContent ?? "").includes("Model list"),
    );
    expect(providersIndex).toBeGreaterThanOrEqual(0);
    expect(modelsIndex).toBe(providersIndex + 1);

    const modelsTab = tabNodes[modelsIndex];
    const modelsTabBadge = within(modelsTab).getByText("3");
    expect(modelsTabBadge.textContent).toBe(String(metricCount));

    fireEvent.click(modelsTab);
    expect(modelsTab).toHaveAttribute("aria-selected", "true");

    const panel = screen.getByRole("tabpanel", { name: "Model list" });
    const rows = within(panel).getAllByTestId("api-gateway-model-list-row");
    expect(rows).toHaveLength(metricCount);

    const search = within(panel).getByTestId("api-gateway-model-list-search");
    fireEvent.change(search, { target: { value: "gpt" } });
    const filteredRows = within(panel).getAllByTestId(
      "api-gateway-model-list-row",
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
      within(panelAfterRoundTrip).getByTestId("api-gateway-model-list-search"),
    ).toHaveValue("gpt");
    const rowsAfterRoundTrip = within(panelAfterRoundTrip).getAllByTestId(
      "api-gateway-model-list-row",
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
      if (command === "api_gateway_provider_templates") {
        return templates;
      }
      if (command === "api_gateway_sync_provider_template") {
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
      if (command === "api_gateway_create_provider_from_template") {
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

    renderWithProviders(<ApiGateway />);

    const manageBtn1 = (await screen.findAllByRole("button", { name: /Provider templates|服务商模板/i }))[0];
    fireEvent.click(manageBtn1);
    const region = await screen.findByTestId("api-gateway-provider-templates");
    expect(region).toBeInTheDocument();
    expect(screen.getByTestId("api-gateway-template-t1")).toBeInTheDocument();
    expect(screen.getByTestId("api-gateway-template-t2")).toBeInTheDocument();
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

    renderWithProviders(<ApiGateway />);

    const manageBtn2 = (await screen.findAllByRole("button", { name: /Provider templates|服务商模板/i }))[0];
    fireEvent.click(manageBtn2);
    const syncButton = await screen.findByTestId("api-gateway-template-sync-t1");
    fireEvent.click(syncButton);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_sync_provider_template", {
        templateId: "t1",
      }),
    );

    // 同步成功后刷新出本次同步时间，且按钮恢复可用
    expect(await screen.findByText(formatGatewayTimestamp(syncedAt)!)).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByTestId("api-gateway-template-sync-t1")).toBeEnabled(),
    );
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
        case "api_gateway_get_config":
          getConfigCalls += 1;
          return store.config;
        case "api_gateway_status":
          return store.status;
        case "api_gateway_terminal_targets":
          return store.targets;
        case "api_gateway_provider_templates":
          return templates;
        case "api_gateway_sync_provider_template": {
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

    renderWithProviders(<ApiGateway />);

    const manageBtn3 = (await screen.findAllByRole("button", { name: /Provider templates|服务商模板/i }))[0];
    fireEvent.click(manageBtn3);
    const syncButton = await screen.findByTestId("api-gateway-template-sync-t1");
    const callsBeforeSync = getConfigCalls;
    fireEvent.click(syncButton);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_gateway_sync_provider_template", {
        templateId: "t1",
      }),
    );

    // 同步后必须重新拉取配置，派生服务商的传播结果才能进入详情。
    await waitFor(() => expect(getConfigCalls).toBeGreaterThan(callsBeforeSync));

    fireEvent.click(
      within(screen.getByTestId("api-gateway-providers")).getByText(
        "Template Bound",
      ),
    );
    await waitFor(() =>
      expect(
        within(screen.getByTestId("api-gateway-provider-detail")).getByLabelText(
          "Upstream model 2",
        ),
      ).toHaveValue("new-model"),
    );
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

    renderWithProviders(<ApiGateway />);

    const manageBtn4 = (await screen.findAllByRole("button", { name: /Provider templates|服务商模板/i }))[0];
    fireEvent.click(manageBtn4);
    fireEvent.click(await screen.findByTestId("api-gateway-template-add-t1"));
    await screen.findByTestId("api-gateway-template-api-key");
    fireEvent.change(screen.getByTestId("api-gateway-template-api-key"), {
      target: { value: "sk-live-key" },
    });
    fireEvent.click(screen.getByTestId("api-gateway-template-create-submit"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "api_gateway_create_provider_from_template",
        {
          templateId: "t1",
          name: "OpenCode Zen",
          baseUrl: "https://opencode.ai/zen/v1",
          protocol: "responses",
          apiKey: "sk-live-key",
        },
      ),
    );

    const detail = await screen.findByTestId("api-gateway-provider-detail");
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

    renderWithProviders(<ApiGateway />);

    // 1. 点击服务商模板管理按钮
    const manageTemplatesBtn = (await screen.findAllByRole("button", { name: /Provider templates|服务商模板/i }))[0];
    fireEvent.click(manageTemplatesBtn);

    // 可以在弹出的服务商模板管理中查看到模板卡片和管理按钮
    expect(await screen.findByText("Test Presets Vendor")).toBeInTheDocument();
    expect(screen.getByTestId("template-section-reset-btn")).toBeInTheDocument();
    expect(screen.getByTestId("template-section-new-btn")).toBeInTheDocument();
    // 关闭模板管理弹窗
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() =>
      expect(screen.queryByTestId("api-gateway-templates-dialog")).not.toBeInTheDocument(),
    );

    // 2. 点击添加服务商
    const addBtn = screen.getAllByRole("button", { name: /Add provider/i })[0];
    fireEvent.click(addBtn);

    // 弹出选择服务商模板弹窗
    const picker = await screen.findByTestId("api-gateway-template-picker-dialog");
    expect(picker).toBeInTheDocument();

    // 各模板卡片中已移除编辑图标按钮
    expect(screen.queryByTestId("template-picker-edit-tpl-test")).not.toBeInTheDocument();

    // 在弹窗中点击新建模板按钮打开模板编辑/新建弹窗
    const newTemplateBtn = screen.getByTestId("template-picker-new-btn");
    fireEvent.click(newTemplateBtn);

    // 弹出编辑模板弹窗
    const editDialog = await screen.findByTestId("api-gateway-template-edit-dialog");
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

    renderWithProviders(<ApiGateway />);

    // index.tsx 必须把已加载的 templates 透传给服务商列表：绑定卡片显示模板头像。
    const boundCard = await screen.findByTestId("api-gateway-provider-p-bound");
    const boundIcon = within(boundCard).getByTestId(
      "api-gateway-provider-template-icon-p-bound",
    );
    expect(boundIcon).toHaveAttribute(
      "title",
      "Created from template OpenCode Zen",
    );
    expect(
      within(boundIcon).getByTestId("provider-icon-opencode"),
    ).toBeInTheDocument();

    // 手动服务商卡片不显示模板头像。
    const manualCard = screen.getByTestId("api-gateway-provider-p-manual");
    expect(
      within(manualCard).queryByTestId(
        "api-gateway-provider-template-icon-p-manual",
      ),
    ).not.toBeInTheDocument();

    // 退休提示同样来自已加载的模板视图。
    const hint = within(boundCard).getByTestId(
      "api-gateway-provider-retired-mappings-p-bound",
    );
    expect(hint).toHaveTextContent("1 mapping(s) removed from template");
    expect(hint).toHaveAttribute(
      "title",
      "Removed from the template and disabled: gone-model",
    );
  });
});

describe("ApiGateway 模板服务商模型维护", () => {
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
        case "api_gateway_get_config":
          return store.config;
        case "api_gateway_status":
          return store.status;
        case "api_gateway_terminal_targets":
          return store.targets;
        case "api_gateway_provider_templates":
          return [templateView(["remote-a", "retired-model"])];
        case "api_gateway_delete_provider_model":
        case "api_gateway_restore_provider_model":
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

    renderWithProviders(<ApiGateway />);
    fireEvent.click(
      await within(
        await screen.findByTestId("api-gateway-providers"),
      ).findByText("Upstream A"),
    );

    fireEvent.click(screen.getByRole("button", { name: "Remove mapping 1" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "api_gateway_delete_provider_model",
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

    renderWithProviders(<ApiGateway />);
    fireEvent.click(
      await within(
        await screen.findByTestId("api-gateway-providers"),
      ).findByText("Upstream A"),
    );

    fireEvent.click(
      await screen.findByTestId("api-gateway-restore-model-retired-model"),
    );

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "api_gateway_restore_provider_model",
        { providerId: "p1", upstreamModel: "retired-model" },
      ),
    );
  });
});

// ---------------------------------------------------------------------------
// Step 3: frontend row-level state, recovery, copy and counting
// ---------------------------------------------------------------------------

describe("ApiGateway 逐行自动禁用前端计数与重新启用入口", () => {
  beforeEach(async () => {
    resetTauriMocks();
    await i18n.changeLanguage("en");
  });

  it("运行时状态卡按 enabled 统计服务商数量，不扣除 auto_disabled", async () => {
    const provider1 = makeProvider({
      id: "p1",
      name: "Healthy Provider",
      enabled: true,
      mappings: [{ local_model: "gpt-4o", upstream_model: "remote-a" }],
    });
    const provider2 = makeProvider({
      id: "p2",
      name: "Auto-disabled Provider",
      enabled: true,
      auto_disabled: true,
      disabled_reason: "HTTP 401",
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

    renderWithProviders(<ApiGateway />);

    // 健康率应为 2/3：provider1(enabled) + provider2(auto_disabled but enabled=true) / total 3
    // provider3(enabled=false) 被排除但不影响分母
    const healthCard = await screen.findByTestId("api-gateway-metric-health");
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

    renderWithProviders(<ApiGateway />);

    // footer 区域应展示来自 status 的数字
    expect(await screen.findByTestId("api-gateway-auto-disabled-count")).toHaveTextContent(
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

    renderWithProviders(<ApiGateway />);

    fireEvent.click(await screen.findByTestId("api-gateway-metric-models"));

    const panel = await screen.findByTestId("api-gateway-model-list");
    const models = within(panel).getAllByTestId("api-gateway-model-list-row");
    const modelNames = models.map(
      (m) => within(m).getByTestId("api-gateway-model-list-id").textContent?.trim(),
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
        if (command === "api_gateway_reenable_provider_model") {
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

    renderWithProviders(<ApiGateway />);

    // 打开服务商详情弹窗
    await screen.findByTestId("api-gateway-providers");
    const providerCard = screen.getByTestId("api-gateway-provider-p1");
    fireEvent.click(within(providerCard).getByText("Broken Provider"));

    const dialog = await screen.findByTestId("api-gateway-provider-detail");
    expect(dialog).toBeInTheDocument();

    // 逐行重新启用
    const rowBtn = screen.getByTestId("api-gateway-reenable-mapping-gpt-4o");
    fireEvent.click(rowBtn);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "api_gateway_reenable_provider_model",
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
        screen.queryByTestId("api-gateway-reenable-mapping-gpt-4o"),
      ).not.toBeInTheDocument(),
    );

    // 不应出现旧的 provider-level 重新启用命令
    const providerLevelCalls = invokeMock.mock.calls.filter(
      ([cmd]) => cmd === "api_gateway_reenable_provider",
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
        if (command === "api_gateway_reenable_provider_models") {
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

    renderWithProviders(<ApiGateway />);

    // 打开服务商详情弹窗
    await screen.findByTestId("api-gateway-providers");
    const providerCard = screen.getByTestId("api-gateway-provider-p1");
    fireEvent.click(within(providerCard).getByText("Broken Provider"));

    const dialog = await screen.findByTestId("api-gateway-provider-detail");
    expect(dialog).toBeInTheDocument();

    // 批量重新启用
    const allBtn = screen.getByTestId("api-gateway-reenable-models-p1");
    fireEvent.click(allBtn);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "api_gateway_reenable_provider_models",
        { providerId: "p1" },
      ),
    );

    // 成功后按钮应从仍然开放的弹窗中消失（所有行的 auto_disabled 被清除）
    await waitFor(() =>
      expect(
        screen.queryByTestId("api-gateway-reenable-models-p1"),
      ).not.toBeInTheDocument(),
    );

    // 不应出现旧的 provider-level 重新启用命令
    const providerLevelCalls = invokeMock.mock.calls.filter(
      ([cmd]) => cmd === "api_gateway_reenable_provider",
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
            auto_disabled: true,
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

    renderWithProviders(<ApiGateway />);

    const providersSection = await screen.findByTestId("api-gateway-providers");

    // 不应出现整个服务商级别的 Re-enable 按钮
    expect(
      within(providersSection).queryByRole("button", {
        name: /Re-enable provider|Re-enable/i,
      }),
    ).not.toBeInTheDocument();

    // 底栏徽章仍反映 enabled
    const badge = within(providersSection).getByTestId(
      "api-gateway-status-badge-p1",
    );
    expect(badge).toHaveTextContent("Enabled");
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
        keys: [key],
        default_key_id: "k1",
      }),
      status: makeStatus({ running: true }),
      targets: [],
    };
    mockStore(store);

    const spy = vi.spyOn(navigator.clipboard, "writeText");

    renderWithProviders(<ApiGateway />);

    const copyBtn = await screen.findByTestId("api-gateway-copy-default-key");
    fireEvent.click(copyBtn);

    expect(spy).toHaveBeenCalledWith("sk-gateway-secret-12345");
  });

  it("顶部未配置默认 Key 时显示未配置文案且点击可直达密钥Tab", async () => {
    const store: Store = {
      config: makeConfig({
        keys: [],
        default_key_id: null,
      }),
      status: makeStatus({ running: true }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<ApiGateway />);

    const noKeyHint = await screen.findByText(/No key configured|未配置密钥/i);
    expect(noKeyHint).toBeInTheDocument();

    fireEvent.click(noKeyHint);
    const keysTab = screen.getByRole("tab", { name: /API Keys|API 密钥/i });
    expect(keysTab).toHaveAttribute("aria-selected", "true");
  });

  it("顶部操作栏展示手动刷新按钮，点击后重新获取今日统计与日志", async () => {
    const store: Store = {
      config: makeConfig(),
      status: makeStatus({ running: true }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<ApiGateway />);

    const refreshBtn = await screen.findByTestId("api-gateway-refresh-btn");
    expect(refreshBtn).toBeInTheDocument();
    expect(refreshBtn).toHaveAttribute("title", "Refresh data");

    const callsBefore = invokeMock.mock.calls.filter(
      (call) => call[0] === "api_gateway_usage_stats",
    ).length;

    await act(async () => {
      fireEvent.click(refreshBtn);
    });

    await waitFor(() => {
      const callsAfter = invokeMock.mock.calls.filter(
        (call) => call[0] === "api_gateway_usage_stats",
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

    renderWithProviders(<ApiGateway />);
    await screen.findByTestId("api-gateway-runtime");

    const callsBefore = invokeMock.mock.calls.filter(
      (call) => call[0] === "api_gateway_usage_stats",
    ).length;

    await act(async () => {
      window.dispatchEvent(new Event("focus"));
    });

    await waitFor(() => {
      const callsAfter = invokeMock.mock.calls.filter(
        (call) => call[0] === "api_gateway_usage_stats",
      ).length;
      expect(callsAfter).toBeGreaterThan(callsBefore);
    });
  });
});
