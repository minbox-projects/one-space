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
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

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

  it("按工具提交所选目标名，且只渲染受支持工具", async () => {
    const store: Store = {
      config: makeConfig({
        providers: [
          {
            id: "p1",
            name: "Upstream A",
            base_url: "https://api.a.example",
            api_key: API_FUSION_KEY_MASK,
            default_model: "gpt-4o",
            mappings: [],
            enabled: true,
            auto_disabled: false,
            disabled_reason: null,
            disabled_at: null,
            consecutive_failures: 0,
            last_error_at: null,
          },
        ],
        keys: [{ id: "k1", label: "Main", value: API_FUSION_KEY_MASK, enabled: true, created_at: 1 }],
        default_key_id: "k1",
      }),
      status: makeStatus({ provider_count: 1, key_count: 1, default_key_id: "k1" }),
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
    fireEvent.click(panel.getByRole("checkbox", { name: "OpenCode" }));
    fireEvent.click(panel.getByRole("button", { name: /Add provider/ }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_configure_terminal", {
        targetTools: ["opencode"],
      }),
    );
  });

  it("默认本地 Key 为空时禁用添加与再次同步", async () => {
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
    const panel = within(screen.getByTestId("api-fusion-terminals"));
    const configure = panel.getByRole("button", { name: /Add provider/ });
    const sync = panel.getByRole("button", { name: /Sync again/ });
    expect(configure).toBeDisabled();
    expect(sync).toBeDisabled();

    fireEvent.click(configure);
    fireEvent.click(sync);
    expect(invokeMock).not.toHaveBeenCalledWith(
      "api_fusion_configure_terminal",
      expect.anything(),
    );
    expect(invokeMock).not.toHaveBeenCalledWith(
      "api_fusion_sync_terminal",
      expect.anything(),
    );
  });

  it("依据台账判定待同步，再次同步后清除", async () => {
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
          synced: true,
          pending_sync: true,
          synced_key_id: "k1",
          synced_at: 1,
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
        case "api_fusion_sync_terminal": {
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

    const panel = within(screen.getByTestId("api-fusion-terminals"));
    fireEvent.click(panel.getByRole("checkbox", { name: "OpenCode" }));
    fireEvent.click(panel.getByRole("button", { name: /Sync again/ }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_sync_terminal", {
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
    expect(screen.getByText("Port 17688")).toBeInTheDocument();
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

    const addButton = screen.getByRole("button", { name: /Add key/ });
    expect(addButton).toBeDisabled();
    expect(screen.queryByLabelText("Key")).not.toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("Label"), { target: { value: "CI" } });
    expect(addButton).toBeEnabled();
    fireEvent.click(addButton);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_upsert_key", {
        key: expect.objectContaining({ label: "CI", value: "" }),
      }),
    );
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

    const keysTab = screen.getByRole("tab", { name: /Local keys/i });
    const terminalsTab = screen.getByRole("tab", { name: /Terminal sync/i });
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
});
