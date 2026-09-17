import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { ApiFusion } from "@/components/ApiFusion";
import {
  API_FUSION_KEY_MASK,
  formatFusionTimestamp,
  maskSecret,
  type FusionConfig,
  type FusionStatus,
  type FusionTerminalTarget,
  type FusionUpstreamProvider,
} from "@/lib/apiFusion";
import { renderWithProviders } from "@/test/mocks/render";
import { emitMock, invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

type Store = {
  config: FusionConfig;
  status: FusionStatus;
  targets: FusionTerminalTarget[];
};

function makeConfig(overrides: Partial<FusionConfig> = {}): FusionConfig {
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

function makeStatus(overrides: Partial<FusionStatus> = {}): FusionStatus {
  return {
    running: true,
    enabled: true,
    port: 17688,
    local_base_url: "http://127.0.0.1:17688",
    provider_count: 0,
    auto_disabled_count: 0,
    key_count: 0,
    default_key_id: null,
    ...overrides,
  };
}

function openCodeTarget(
  overrides: Partial<FusionTerminalTarget> = {},
): FusionTerminalTarget {
  return {
    provider_id: "t-open",
    tool: "opencode",
    name: "OpenCode",
    base_url: "http://127.0.0.1:17688",
    synced: false,
    pending_sync: true,
    synced_key_id: null,
    synced_at: null,
    ...overrides,
  };
}

function mockStore(store: Store) {
  invokeMock.mockImplementation(async (command: string) => {
    switch (command) {
      case "api_fusion_get_config":
        return store.config;
      case "api_fusion_status":
        return store.status;
      case "api_fusion_terminal_targets":
        return store.targets;
      case "api_fusion_start":
        store.status = { ...store.status, running: true };
        return store.status;
      case "api_fusion_stop":
        store.status = { ...store.status, running: false };
        return store.status;
      case "api_fusion_configure_terminal":
        return store.config.terminal_syncs;
      case "api_fusion_sync_terminal":
        return store.config.terminal_syncs;
      default:
        throw new Error(`Unhandled command: ${command}`);
    }
  });
}

function mockStoreWithUpsert(store: Store) {
  mockStore(store);
  const read = invokeMock.getMockImplementation()!;
  invokeMock.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    if (command === "api_fusion_upsert_provider") {
      const { provider } = args as { provider: FusionUpstreamProvider };
      store.config = {
        ...store.config,
        providers: [
          ...store.config.providers.filter((entry) => entry.id !== provider.id),
          provider,
        ],
      };
      return store.config;
    }
    return read(command, args);
  });
}

function makeProvider(
  overrides: Partial<FusionUpstreamProvider> = {},
): FusionUpstreamProvider {
  return {
    id: "p1",
    name: "Upstream A",
    base_url: "https://api.a.example",
    api_key: API_FUSION_KEY_MASK,
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

describe("ApiFusion", () => {
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
        keys: [{ id: "k1", label: "Main", value: API_FUSION_KEY_MASK, enabled: true, created_at: 1 }],
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

    renderWithProviders(<ApiFusion />);
    await screen.findByText("OpenCode");

    expect(screen.queryByText("Claude Code")).not.toBeInTheDocument();
    expect(screen.queryByText("Antigravity")).not.toBeInTheDocument();

    const panel = within(screen.getByTestId("api-fusion-terminals"));
    // The header-level "Add provider / Sync again" actions must be gone:
    // every button inside the panel belongs to a terminal target row.
    const buttons = panel.getAllByRole("button");
    expect(buttons.length).toBeGreaterThan(0);
    for (const button of buttons) {
      expect(button.closest('[data-testid^="api-fusion-target-"]')).not.toBeNull();
    }
    expect(
      panel.queryByRole("button", { name: /sync again/i }),
    ).not.toBeInTheDocument();
  });

  it("待同步的行内显示添加，点击仅以该工具调用配置", async () => {
    const store: Store = {
      config: makeConfig({
        keys: [{ id: "k1", label: "Main", value: API_FUSION_KEY_MASK, enabled: true, created_at: 1 }],
        default_key_id: "k1",
      }),
      status: makeStatus({ key_count: 1, default_key_id: "k1" }),
      targets: [
        openCodeTarget({ provider_id: "gw-open" }),
        openCodeTarget({ provider_id: "gw-codex", tool: "codex", name: "Codex" }),
      ],
    };
    mockStore(store);

    renderWithProviders(<ApiFusion />);
    await screen.findByText("OpenCode");

    const openRow = within(screen.getByTestId("api-fusion-target-opencode"));
    const addButton = openRow.getByRole("button", { name: /add/i });
    expect(addButton).toBeEnabled();
    fireEvent.click(addButton);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_configure_terminal", {
        targetTools: ["opencode"],
      }),
    );
    const configureCalls = invokeMock.mock.calls.filter(
      ([command]) => command === "api_fusion_configure_terminal",
    );
    expect(configureCalls).toHaveLength(1);
    expect(invokeMock).not.toHaveBeenCalledWith("api_fusion_configure_terminal", {
      targetTools: ["opencode", "codex"],
    });
  });

  it("已添加的行内显示同步，点击仅以该工具调用同步", async () => {
    const store: Store = {
      config: makeConfig({
        keys: [{ id: "k1", label: "Main", value: API_FUSION_KEY_MASK, enabled: true, created_at: 1 }],
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

    renderWithProviders(<ApiFusion />);
    await screen.findByText("OpenCode");

    const openRow = within(screen.getByTestId("api-fusion-target-opencode"));
    const syncButton = openRow.getByRole("button", { name: /sync/i });
    expect(syncButton).toBeEnabled();
    fireEvent.click(syncButton);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_sync_terminal", {
        targetTools: ["opencode"],
      }),
    );
    const syncCalls = invokeMock.mock.calls.filter(
      ([command]) => command === "api_fusion_sync_terminal",
    );
    expect(syncCalls).toHaveLength(1);
    expect(invokeMock).not.toHaveBeenCalledWith("api_fusion_sync_terminal", {
      targetTools: ["opencode", "codex"],
    });
  });

  it("已添加但 Key 或地址漂移（synced 为真且待同步）时行内显示同步并仅走同步通道", async () => {
    const store: Store = {
      config: makeConfig({
        keys: [{ id: "k1", label: "Main", value: API_FUSION_KEY_MASK, enabled: true, created_at: 1 }],
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

    renderWithProviders(<ApiFusion />);
    await screen.findByText("OpenCode");

    const openRow = within(screen.getByTestId("api-fusion-target-opencode"));
    const action = openRow.getByRole("button", { name: /sync/i });
    expect(action).toBeEnabled();
    expect(action).toHaveAccessibleName(/sync/i);
    expect(action).not.toHaveAccessibleName(/sync again/i);
    fireEvent.click(action);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_sync_terminal", {
        targetTools: ["opencode"],
      }),
    );
    const syncCalls = invokeMock.mock.calls.filter(
      ([command]) => command === "api_fusion_sync_terminal",
    );
    expect(syncCalls).toHaveLength(1);
    expect(invokeMock).not.toHaveBeenCalledWith(
      "api_fusion_configure_terminal",
      expect.anything(),
    );
  });

  it("网关服务商被删除但同步台账仍在（synced 为假且 synced_key_id/synced_at 非空）时行内显示同步并仅走同步通道", async () => {
    const store: Store = {
      config: makeConfig({
        keys: [{ id: "k1", label: "Main", value: API_FUSION_KEY_MASK, enabled: true, created_at: 1 }],
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

    renderWithProviders(<ApiFusion />);
    await screen.findByText("OpenCode");

    const openRow = within(screen.getByTestId("api-fusion-target-opencode"));
    const action = openRow.getByRole("button", { name: /sync/i });
    expect(action).toBeEnabled();
    expect(action).toHaveAccessibleName(/sync/i);
    expect(action).not.toHaveAccessibleName(/sync again/i);
    fireEvent.click(action);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_sync_terminal", {
        targetTools: ["opencode"],
      }),
    );
    const syncCalls = invokeMock.mock.calls.filter(
      ([command]) => command === "api_fusion_sync_terminal",
    );
    expect(syncCalls).toHaveLength(1);
    expect(invokeMock).not.toHaveBeenCalledWith(
      "api_fusion_configure_terminal",
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

    renderWithProviders(<ApiFusion />);
    await screen.findByText("OpenCode");

    expect(screen.getByTestId("api-fusion-default-key-required")).toHaveTextContent(
      /Add and enable a local key/,
    );
    const openRow = within(screen.getByTestId("api-fusion-target-opencode"));
    const action = openRow.getByRole("button", { name: /add/i });
    expect(action).toBeDisabled();

    fireEvent.click(action);
    expect(invokeMock).not.toHaveBeenCalledWith(
      "api_fusion_configure_terminal",
      expect.anything(),
    );
    expect(invokeMock).not.toHaveBeenCalledWith(
      "api_fusion_sync_terminal",
      expect.anything(),
    );
  });

  it("从未添加的行内显示添加，点击走配置通道并在配置后清除待同步", async () => {
    const store: Store = {
      config: makeConfig({
        keys: [
          { id: "k1", label: "Main", value: API_FUSION_KEY_MASK, enabled: true, created_at: 1 },
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
        case "api_fusion_get_config":
          return store.config;
        case "api_fusion_status":
          return store.status;
        case "api_fusion_terminal_targets":
          return store.targets;
        case "api_fusion_configure_terminal": {
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

    renderWithProviders(<ApiFusion />);
    await screen.findByText("OpenCode");

    // Pending status comes from the backend target payload, not local re-derivation.
    expect(screen.getByText("Pending sync")).toBeInTheDocument();

    const openRow = within(screen.getByTestId("api-fusion-target-opencode"));
    fireEvent.click(openRow.getByRole("button", { name: /add/i }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_configure_terminal", {
        targetTools: ["opencode"],
      }),
    );
    await waitFor(() =>
      expect(screen.queryByText("Pending sync")).not.toBeInTheDocument(),
    );
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
          base_url: "http://127.0.0.1:17688",
          synced: true,
          pending_sync: false,
          synced_key_id: "k1",
          synced_at: 1,
        },
      ],
    };
    mockStore(store);

    renderWithProviders(<ApiFusion />);
    await screen.findByText("OpenCode");

    const panel = within(screen.getByTestId("api-fusion-terminals"));
    expect(panel.getByText("Synced")).toBeInTheDocument();
    expect(panel.queryByText("Pending sync")).not.toBeInTheDocument();
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
            api_key: API_FUSION_KEY_MASK,
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

    renderWithProviders(<ApiFusion />);

    expect(await screen.findByTestId("api-fusion-runtime-state")).toHaveTextContent(
      "Running",
    );
    expect(screen.getByTestId("api-fusion-local-address")).toHaveTextContent(
      "http://127.0.0.1:17688",
    );
    expect(screen.getByText(/Reason: HTTP 401/)).toBeInTheDocument();
    expect(
      screen.getByText(
        new RegExp(`Disabled at ${formatFusionTimestamp(disabledAt)!}`),
      ),
    ).toBeInTheDocument();
    expect(screen.getByTestId("api-fusion-auto-disabled-count")).toHaveTextContent("1");

    fireEvent.click(screen.getByRole("button", { name: /Copy local API address/ }));
    await waitFor(() =>
      expect(writeText).toHaveBeenCalledWith("http://127.0.0.1:17688"),
    );

    const masked = screen.getByTestId("api-fusion-key-value-k1");
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

    renderWithProviders(<ApiFusion />);
    fireEvent.click(await screen.findByText("Upstream A"));

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

    renderWithProviders(<ApiFusion />);
    fireEvent.click(await screen.findByText("Upstream A"));
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "api_fusion_upsert_provider",
        expect.objectContaining({
          provider: expect.objectContaining({ id: "p1" }),
        }),
      ),
    );

    const call = invokeMock.mock.calls.find(
      ([command]) => command === "api_fusion_upsert_provider",
    );
    const payload = call?.[1] as { provider: FusionUpstreamProvider };
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

    renderWithProviders(<ApiFusion />);
    fireEvent.click(await screen.findByText("Upstream A"));
    fireEvent.change(screen.getByLabelText("Mapping protocol 1"), {
      target: { value: "responses" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "api_fusion_upsert_provider",
        expect.objectContaining({
          provider: expect.objectContaining({ id: "p1" }),
        }),
      ),
    );

    const call = invokeMock.mock.calls.find(
      ([command]) => command === "api_fusion_upsert_provider",
    );
    const payload = call?.[1] as { provider: FusionUpstreamProvider };
    expect(payload.provider.mappings[0].protocol).toBe("responses");
  });

  it("新增本地 Key 只需名称，值留空交由后端随机生成", async () => {
    const store: Store = {
      config: makeConfig({ keys: [], default_key_id: null }),
      status: makeStatus({ key_count: 0, default_key_id: null }),
      targets: [],
    };
    invokeMock.mockImplementation(async (command: string) => {
      switch (command) {
        case "api_fusion_get_config":
        case "api_fusion_upsert_key":
          return store.config;
        case "api_fusion_status":
          return store.status;
        case "api_fusion_terminal_targets":
          return store.targets;
        default:
          throw new Error(`Unhandled command: ${command}`);
      }
    });

    renderWithProviders(<ApiFusion />);
    await screen.findByTestId("api-fusion-keys");

    // 页头不再有内联名称输入框，"新增 Key" 按钮始终可用
    expect(screen.queryByLabelText("Name")).not.toBeInTheDocument();
    const addButton = screen.getByRole("button", { name: /Add key/ });
    expect(addButton).toBeEnabled();

    // 点击按钮弹出对话框
    fireEvent.click(addButton);
    const dialog = await screen.findByTestId("api-fusion-key-dialog");
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
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_upsert_key", {
        key: expect.objectContaining({ label: "CI", value: "" }),
      }),
    );

    // 值留空交由后端随机生成，其余字段按约定初始化
    const call = invokeMock.mock.calls.find(
      ([command]) => command === "api_fusion_upsert_key",
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

    renderWithProviders(<ApiFusion />);
    await screen.findByTestId("api-fusion-keys");

    fireEvent.click(screen.getByRole("button", { name: /Add key/ }));
    const dialog = await screen.findByTestId("api-fusion-key-dialog");
    fireEvent.change(within(dialog).getByLabelText("Name"), {
      target: { value: "CI" },
    });
    expect(within(dialog).getByLabelText("Name")).toHaveValue("CI");

    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() =>
      expect(
        screen.queryByTestId("api-fusion-key-dialog"),
      ).not.toBeInTheDocument(),
    );

    fireEvent.click(screen.getByRole("button", { name: /Add key/ }));
    const reopened = await screen.findByTestId("api-fusion-key-dialog");
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
        case "api_fusion_get_config":
        case "api_fusion_upsert_key":
          return store.config;
        case "api_fusion_status":
          return store.status;
        case "api_fusion_terminal_targets":
          return store.targets;
        default:
          throw new Error(`Unhandled command: ${command}`);
      }
    });

    renderWithProviders(<ApiFusion />);
    await screen.findByTestId("api-fusion-keys");

    fireEvent.click(screen.getByRole("button", { name: /Add key/ }));
    const dialog = await screen.findByTestId("api-fusion-key-dialog");
    const nameInput = within(dialog).getByLabelText("Name");
    fireEvent.change(nameInput, { target: { value: "CI" } });
    fireEvent.keyDown(nameInput, { key: "Enter" });

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_upsert_key", {
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
        case "api_fusion_get_config":
          return store.config;
        case "api_fusion_upsert_key":
          throw new Error("boom");
        case "api_fusion_status":
          return store.status;
        case "api_fusion_terminal_targets":
          return store.targets;
        default:
          throw new Error(`Unhandled command: ${command}`);
      }
    });

    renderWithProviders(<ApiFusion />);
    await screen.findByTestId("api-fusion-keys");

    fireEvent.click(screen.getByRole("button", { name: /Add key/ }));
    const dialog = await screen.findByTestId("api-fusion-key-dialog");
    fireEvent.change(within(dialog).getByLabelText("Name"), {
      target: { value: "CI" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(screen.getByTestId("api-fusion-key-dialog")).toBeInTheDocument(),
    );
    expect(
      within(screen.getByTestId("api-fusion-key-dialog")).getByLabelText("Name"),
    ).toHaveValue("CI");
  });

  it("支持在 Tabs 之间顺畅切换且保持各面板挂载与状态", async () => {
    const store: Store = {
      config: makeConfig({
        providers: [makeProvider()],
        keys: [{ id: "k1", label: "Dev Key", value: API_FUSION_KEY_MASK, enabled: true, created_at: 1 }],
      }),
      status: makeStatus({ provider_count: 1, key_count: 1 }),
      targets: [openCodeTarget()],
    };
    mockStore(store);

    renderWithProviders(<ApiFusion />);
    await screen.findByText("Upstream A");

    const tabsList = screen.getByRole("tablist", { name: /API Gateway tabs/i });
    expect(tabsList).toBeInTheDocument();

    const keysTab = screen.getByRole("tab", { name: /Api Keys/i });
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

  it("点击添加服务商打开弹框并成功保存", async () => {
    const store: Store = {
      config: makeConfig({ providers: [] }),
      status: makeStatus({ provider_count: 0 }),
      targets: [],
    };
    invokeMock.mockImplementation(async (command: string, args: any) => {
      switch (command) {
        case "api_fusion_get_config":
          return store.config;
        case "api_fusion_status":
          return store.status;
        case "api_fusion_terminal_targets":
          return store.targets;
        case "api_fusion_upsert_provider":
          store.config = {
            ...store.config,
            providers: [...store.config.providers, { ...args.provider, id: "p-new" }],
          };
          return store.config;
        default:
          throw new Error(`Unhandled command: ${command}`);
      }
    });

    renderWithProviders(<ApiFusion />);
    await screen.findByTestId("api-fusion-providers");

    const addButtons = screen.getAllByRole("button", { name: /Add provider/i });
    fireEvent.click(addButtons[0]);

    const dialog = await screen.findByTestId("api-fusion-provider-detail");
    expect(dialog).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("Name"), { target: { value: "New Remote" } });
    fireEvent.change(screen.getByLabelText("API base URL"), {
      target: { value: "https://new.example.com" },
    });

    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_upsert_provider", {
        provider: expect.objectContaining({
          name: "New Remote",
          base_url: "https://new.example.com",
        }),
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

    renderWithProviders(<ApiFusion />);
    const runningToggleBtn = await screen.findByTestId("api-fusion-toggle-service");
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

    renderWithProviders(<ApiFusion />);
    const stoppedToggleBtn = await screen.findByTestId("api-fusion-toggle-service");
    expect(stoppedToggleBtn).toHaveAttribute("title", "Start service");
    expect(stoppedToggleBtn).toHaveClass("text-emerald-600");
  });

  it("启停服务成功后广播 api-fusion-status-update 事件", async () => {
    const store: Store = {
      config: makeConfig(),
      status: makeStatus({ running: false }),
      targets: [],
    };
    mockStore(store);

    renderWithProviders(<ApiFusion />);
    const toggleBtn = await screen.findByTestId("api-fusion-toggle-service");
    fireEvent.click(toggleBtn);

    await waitFor(() => {
      expect(emitMock).toHaveBeenCalledWith("api-fusion-status-update");
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

    renderWithProviders(<ApiFusion />);

    // 1. 服务商健康率展示：1/2 在线 (因为 provider2 是 disabled)
    const healthCard = await screen.findByTestId("api-fusion-metric-health");
    expect(within(healthCard).getByText("1/2")).toBeInTheDocument();
    expect(within(healthCard).getByText(/Provider health/i)).toBeInTheDocument();

    // 2. 聚合模型数：provider1 贡献了 gpt-4o 和 claude-3-7-sonnet（去重后 2 个，provider2 被禁用不计入）
    const modelsCard = screen.getByTestId("api-fusion-metric-models");
    expect(within(modelsCard).getByText("2")).toBeInTheDocument();
    expect(within(modelsCard).getByText(/Aggregated models/i)).toBeInTheDocument();

    // 3. 本地有效密钥：1 个有效 (共 2 个)
    const keysCard = screen.getByTestId("api-fusion-metric-keys");
    expect(within(keysCard).getByText("1")).toBeInTheDocument();
    expect(within(keysCard).getByText("/ 2")).toBeInTheDocument();

    // 4. 终端同步：1/2 已同步，且有 1 个待同步提示
    const terminalsCard = screen.getByTestId("api-fusion-metric-terminals");
    expect(within(terminalsCard).getByText("1/2")).toBeInTheDocument();
    expect(within(terminalsCard).getByText(/1.*pending sync/i)).toBeInTheDocument();

    // 5. 点击终端指标卡可切换到 terminals Tab
    fireEvent.click(terminalsCard);

    const terminalsTab = screen.getByRole("tab", { name: /AI terminal integration/i });
    expect(terminalsTab).toHaveAttribute("aria-selected", "true");
  });

  it("点击聚合模型指标卡打开弹框并列出有效模型及其上游服务商映射", async () => {
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

    renderWithProviders(<ApiFusion />);

    const modelsCard = await screen.findByTestId("api-fusion-metric-models");
    const cardModelCount = Number(within(modelsCard).getByText("2").textContent);
    fireEvent.click(modelsCard);

    const dialog = await screen.findByTestId("api-fusion-aggregated-models");

    const modelNodes = within(dialog).getAllByTestId("api-fusion-aggregated-model");
    expect(modelNodes).toHaveLength(2);
    expect(modelNodes).toHaveLength(cardModelCount);
    expect(
      modelNodes.map((node) => node.getAttribute("data-model")).sort(),
    ).toEqual(["claude-3-7-sonnet", "gpt-4o"]);

    expect(
      within(dialog).getAllByTestId("api-fusion-aggregated-model-provider"),
    ).toHaveLength(3);

    expect(within(dialog).getAllByText("Provider 1").length).toBeGreaterThan(0);
    expect(within(dialog).getAllByText("gpt-4o-2024").length).toBeGreaterThan(0);
    expect(within(dialog).getAllByText("claude-3-7").length).toBeGreaterThan(0);
    expect(within(dialog).queryAllByText("deepseek-v3")).toHaveLength(0);
    expect(within(dialog).queryAllByText("Provider 2")).toHaveLength(0);
  });

  it("没有启用服务商时聚合模型弹框展示空态且不渲染任何模型", async () => {
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

    renderWithProviders(<ApiFusion />);

    fireEvent.click(await screen.findByTestId("api-fusion-metric-models"));

    const dialog = await screen.findByTestId("api-fusion-aggregated-models");
    expect(
      within(dialog).getByTestId("api-fusion-aggregated-models-empty"),
    ).toBeInTheDocument();
    expect(
      within(dialog).queryAllByTestId("api-fusion-aggregated-model"),
    ).toHaveLength(0);
  });
});
