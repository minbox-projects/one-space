import { fireEvent, screen, waitFor } from "@testing-library/react";
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
    base_url: "https://old.example",
    api_key: API_FUSION_KEY_MASK,
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

  it("一键配置只提交所选 OpenCode/Codex 目标 id，不重建记录也不触碰 claude/antigravity", async () => {
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
        openCodeTarget(),
        openCodeTarget({ provider_id: "t-codex", tool: "codex", name: "Codex" }),
        openCodeTarget({
          provider_id: "t-claude",
          tool: "claude",
          name: "Claude Code",
          base_url: null,
        }),
        openCodeTarget({
          provider_id: "t-antigravity",
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

    fireEvent.click(screen.getByRole("checkbox", { name: "OpenCode" }));
    fireEvent.click(screen.getByRole("button", { name: /Configure selected/ }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_configure_terminal", {
        targetIds: ["t-open"],
      }),
    );
    expect(invokeMock).not.toHaveBeenCalledWith(
      "api_fusion_configure_terminal",
      expect.objectContaining({ provider: expect.anything() }),
    );
  });

  it("默认本地 Key 为空时阻止一键配置与同步并给出可操作提示", async () => {
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
    const configure = screen.getByRole("button", { name: /Configure selected/ });
    const sync = screen.getByRole("button", { name: /Sync selected/ });
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

  it("依据同步台账而非脱敏 api_key 判定待同步，同步后清除", async () => {
    const store: Store = {
      config: makeConfig({
        keys: [
          { id: "k1", label: "Old", value: API_FUSION_KEY_MASK, enabled: true, created_at: 1 },
          { id: "k2", label: "New", value: API_FUSION_KEY_MASK, enabled: true, created_at: 1 },
        ],
        default_key_id: "k2",
        terminal_syncs: [
          {
            provider_id: "t-open",
            tool: "opencode",
            synced_key_id: "k1",
            synced_base_url: "http://127.0.0.1:17688",
            synced_at: 1,
          },
        ],
      }),
      status: makeStatus({ key_count: 2, default_key_id: "k2" }),
      targets: [
        openCodeTarget({
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
          store.config = {
            ...store.config,
            terminal_syncs: [
              {
                provider_id: "t-open",
                tool: "opencode",
                synced_key_id: "k2",
                synced_base_url: "http://127.0.0.1:17688",
                synced_at: 2,
              },
            ],
          };
          store.targets = [
            openCodeTarget({
              synced: true,
              pending_sync: false,
              synced_key_id: "k2",
              synced_at: 2,
            }),
          ];
          return store.config.terminal_syncs;
        }
        default:
          throw new Error(`Unhandled command: ${command}`);
      }
    });

    renderWithProviders(<ApiFusion />);
    await screen.findByText("OpenCode");

    // Pending is shown even though api_key is the redacted placeholder.
    expect(screen.getByText("Pending sync")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("checkbox", { name: "OpenCode" }));
    fireEvent.click(screen.getByRole("button", { name: /Sync selected/ }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("api_fusion_sync_terminal", {
        targetIds: ["t-open"],
      }),
    );
    await waitFor(() =>
      expect(screen.queryByText("Pending sync")).not.toBeInTheDocument(),
    );
    expect(screen.getByText("Synced")).toBeInTheDocument();
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

    expect(screen.getByLabelText("Mapping protocol 1")).toHaveValue("");

    fireEvent.click(screen.getByRole("button", { name: /Add mapping/ }));
    expect(screen.getByLabelText("Mapping protocol 2")).toHaveValue("");
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

  it("预览同时展示解析后的上游模型与目标 endpoint", async () => {
    const store: Store = {
      config: makeConfig({
        providers: [
          makeProvider({
            protocol: "chat_completions",
            default_model: "remote-default",
            mappings: [
              {
                local_model: "local-a",
                upstream_model: "remote-a",
                protocol: "responses",
              },
              { local_model: "local-b", upstream_model: "remote-b" },
            ],
          }),
        ],
      }),
      status: makeStatus({ provider_count: 1 }),
      targets: [openCodeTarget()],
    };
    mockStore(store);

    renderWithProviders(<ApiFusion />);
    fireEvent.click(await screen.findByText("Upstream A"));
    const previewInput = screen.getByLabelText("Preview model");
    const modelPreview = screen.getByTestId("api-fusion-model-preview");
    const endpointPreview = screen.getByTestId("api-fusion-endpoint-preview");

    fireEvent.change(previewInput, { target: { value: "local-a" } });
    expect(modelPreview).toHaveTextContent("remote-a");
    expect(endpointPreview).toHaveTextContent("/responses");

    fireEvent.change(previewInput, { target: { value: "local-b" } });
    expect(modelPreview).toHaveTextContent("remote-b");
    expect(endpointPreview).toHaveTextContent("/chat/completions");

    fireEvent.change(previewInput, { target: { value: "local-unknown" } });
    expect(modelPreview).toHaveTextContent("remote-default");
    expect(endpointPreview).toHaveTextContent("/chat/completions");

    fireEvent.change(previewInput, { target: { value: "" } });
    expect(modelPreview).toHaveTextContent("remote-default");
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
});
