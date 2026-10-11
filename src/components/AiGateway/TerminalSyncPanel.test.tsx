import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { TerminalSyncPanel } from "./TerminalSyncPanel";
import {
  formatGatewayTimestamp,
  type GatewayConfig,
  type GatewayTerminalTarget,
  type GatewayUpstreamProvider,
} from "@/lib/aiGateway";
import { renderWithProviders } from "@/test/mocks/render";

function makeConfig(overrides: Partial<GatewayConfig> = {}): GatewayConfig {
  return {
    enabled: true,
    port: 17688,
    providers: [],
    keys: [
      {
        id: "key-1",
        label: "Default Key",
        value: "sk-gateway-1234",
        enabled: true,
        created_at: 1_700_000_000,
      },
    ],
    default_key_id: "key-1",
    terminal_syncs: [],
    ...overrides,
  };
}

describe("TerminalSyncPanel 终端同步面板与最后同步时间展示", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("正确展示已同步终端的最近同步时间和未同步终端的尚未同步文案", () => {
    const syncedAt = 1_700_000_000;
    const targets: GatewayTerminalTarget[] = [
      {
        tool: "opencode",
        name: "OpenCode",
        provider_id: "gw-open",
        base_url: "http://127.0.0.1:17688/v1",
        synced: true,
        pending_sync: false,
        synced_key_id: "key-1",
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
    ];

    renderWithProviders(
      <TerminalSyncPanel
        targets={targets}
        config={makeConfig()}
        syncingTools={{}}
        onConfigureTool={vi.fn()}
        onSyncTool={vi.fn()}
      />,
    );

    const openCodeSynced = screen.getByTestId("ai-gateway-target-synced-opencode");
    expect(openCodeSynced).toHaveTextContent(formatGatewayTimestamp(syncedAt)!);
    expect(openCodeSynced).toHaveTextContent("Last sync");

    const codexSynced = screen.getByTestId("ai-gateway-target-synced-codex");
    expect(codexSynced).toHaveTextContent("Not synced yet");
    expect(codexSynced).toHaveTextContent("Last sync");
  });

  it("切换为中文后正确展示最近同步与尚未同步", async () => {
    await i18n.changeLanguage("zh");
    const syncedAt = 1_700_000_000;
    const targets: GatewayTerminalTarget[] = [
      {
        tool: "opencode",
        name: "OpenCode",
        provider_id: "gw-open",
        base_url: "http://127.0.0.1:17688/v1",
        synced: true,
        pending_sync: false,
        synced_key_id: "key-1",
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
    ];

    renderWithProviders(
      <TerminalSyncPanel
        targets={targets}
        config={makeConfig()}
        syncingTools={{}}
        onConfigureTool={vi.fn()}
        onSyncTool={vi.fn()}
      />,
    );

    const openCodeSynced = screen.getByTestId("ai-gateway-target-synced-opencode");
    expect(openCodeSynced).toHaveTextContent(formatGatewayTimestamp(syncedAt)!);
    expect(openCodeSynced).toHaveTextContent("最近同步");

    const codexSynced = screen.getByTestId("ai-gateway-target-synced-codex");
    expect(codexSynced).toHaveTextContent("尚未同步");
    expect(codexSynced).toHaveTextContent("最近同步");
  });

  it("点击操作按钮能够正确触发对应工具的配置或同步回调", async () => {
    const user = userEvent.setup();
    const handleConfigure = vi.fn();
    const handleSync = vi.fn();

    const targets: GatewayTerminalTarget[] = [
      {
        tool: "opencode",
        name: "OpenCode",
        provider_id: "gw-open",
        base_url: "http://127.0.0.1:17688/v1",
        synced: true,
        pending_sync: false,
        synced_key_id: "key-1",
        synced_at: 1_700_000_000,
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
    ];

    renderWithProviders(
      <TerminalSyncPanel
        targets={targets}
        config={makeConfig()}
        syncingTools={{}}
        onConfigureTool={handleConfigure}
        onSyncTool={handleSync}
      />,
    );

    // OpenCode 已添加过，按钮为 Sync
    await user.click(screen.getByTestId("ai-gateway-sync-opencode"));
    expect(handleSync).toHaveBeenCalledWith("opencode");

    // Codex 从未添加过，按钮为 Add provider
    await user.click(screen.getByTestId("ai-gateway-sync-codex"));
    expect(handleConfigure).toHaveBeenCalledWith("codex");
  });

  it("当 gatewayRunning 为 false 时展示服务停止警告，并支持点击一键启动", async () => {
    const user = userEvent.setup();
    const handleStart = vi.fn();

    renderWithProviders(
      <TerminalSyncPanel
        targets={[]}
        config={makeConfig()}
        gatewayRunning={false}
        onStartGateway={handleStart}
        syncingTools={{}}
        onConfigureTool={vi.fn()}
        onSyncTool={vi.fn()}
      />,
    );

    const warning = screen.getByTestId("ai-gateway-terminal-stopped-warning");
    expect(warning).toHaveTextContent("Local AI Gateway is stopped");
    expect(warning).toHaveTextContent("Cannot connect to API: Unable to connect");

    const startBtn = screen.getByTestId("ai-gateway-start-from-terminals");
    await user.click(startBtn);
    expect(handleStart).toHaveBeenCalledTimes(1);
  });

  it("支持展开终端调用常见报错排查指引并展示 FAQ 详情", async () => {
    const user = userEvent.setup();

    renderWithProviders(
      <TerminalSyncPanel
        targets={[]}
        config={makeConfig()}
        gatewayRunning={true}
        syncingTools={{}}
        onConfigureTool={vi.fn()}
        onSyncTool={vi.fn()}
      />,
    );

    // 默认收起
    expect(screen.queryByTestId("ai-gateway-terminal-faq-content")).not.toBeInTheDocument();

    // 点击展开
    const toggle = screen.getByTestId("ai-gateway-terminal-faq-toggle");
    await user.click(toggle);

    const content = screen.getByTestId("ai-gateway-terminal-faq-content");
    expect(content).toBeInTheDocument();
    expect(content).toHaveTextContent("Cannot connect to API: Unable to connect. Is the computer able to access the url?");
    expect(content).toHaveTextContent("all providers unavailable: 429");
  });
});

// ---------------------------------------------------------------------------
// Step 4: the Claude terminal target row and its four-choice dialog.
// RED before implementation: claude is excluded from the supported targets and
// no dialog or claude_models argument exists.
// ---------------------------------------------------------------------------

function anthropicProvider(
  overrides: Partial<GatewayUpstreamProvider> = {},
): GatewayUpstreamProvider {
  return {
    id: "p-claude",
    name: "CommandCode",
    base_url: "https://api.commandcode.ai/provider/v1",
    default_model: null,
    protocol: "anthropic_messages",
    mappings: [
      { local_model: "cc-haiku", upstream_model: "claude-haiku-5-5" },
      { local_model: "cc-sonnet", upstream_model: "claude-sonnet-5-5" },
      { local_model: "cc-opus", upstream_model: "claude-opus-5-5" },
    ],
    enabled: true,
    ...overrides,
  } as GatewayUpstreamProvider;
}

function claudeTarget(
  overrides: Partial<GatewayTerminalTarget> = {},
): GatewayTerminalTarget {
  return {
    tool: "claude",
    name: "Claude Code",
    provider_id: null,
    base_url: null,
    synced: false,
    pending_sync: true,
    synced_key_id: null,
    synced_at: null,
    ...overrides,
  };
}

function claudeConfig(overrides: Partial<GatewayConfig> = {}): GatewayConfig {
  return makeConfig({
    providers: [anthropicProvider()],
    ...overrides,
  });
}

async function chooseFour(user: ReturnType<typeof userEvent.setup>): Promise<void> {
  await user.selectOptions(screen.getByTestId("claude-model-select-default"), "cc-opus");
  await user.selectOptions(screen.getByTestId("claude-model-select-haiku"), "cc-haiku");
  await user.selectOptions(screen.getByTestId("claude-model-select-sonnet"), "cc-sonnet");
  await user.selectOptions(screen.getByTestId("claude-model-select-opus"), "cc-opus");
}

describe("TerminalSyncPanel Claude 目标与四模型选择对话框", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("将 Claude 纳入受支持目标行", () => {
    renderWithProviders(
      <TerminalSyncPanel
        targets={[claudeTarget()]}
        config={claudeConfig()}
        syncingTools={{}}
        onConfigureTool={vi.fn()}
        onSyncTool={vi.fn()}
      />,
    );

    expect(screen.getByTestId("ai-gateway-target-claude")).toBeInTheDocument();
  });

  it("首次同步 Claude 打开四选择对话框并在提交后携带 claude_models 回调", async () => {
    const user = userEvent.setup();
    const onConfigureTool = vi.fn();

    renderWithProviders(
      <TerminalSyncPanel
        targets={[claudeTarget()]}
        config={claudeConfig()}
        syncingTools={{}}
        onConfigureTool={onConfigureTool}
        onSyncTool={vi.fn()}
      />,
    );

    await user.click(screen.getByTestId("ai-gateway-sync-claude"));
    expect(screen.getByTestId("claude-model-selection-dialog")).toBeInTheDocument();

    await chooseFour(user);
    await user.click(screen.getByTestId("claude-model-selection-submit"));

    expect(onConfigureTool).toHaveBeenCalledWith("claude", {
      default_model: "cc-opus",
      haiku: "cc-haiku",
      sonnet: "cc-sonnet",
      opus: "cc-opus",
    });
    expect(onConfigureTool).toHaveBeenCalledTimes(1);
  });

  it("取消 Claude 四选择对话框不调用任何配置或同步回调", async () => {
    const user = userEvent.setup();
    const onConfigureTool = vi.fn();
    const onSyncTool = vi.fn();

    renderWithProviders(
      <TerminalSyncPanel
        targets={[claudeTarget()]}
        config={claudeConfig()}
        syncingTools={{}}
        onConfigureTool={onConfigureTool}
        onSyncTool={onSyncTool}
      />,
    );

    await user.click(screen.getByTestId("ai-gateway-sync-claude"));
    await user.click(screen.getByTestId("claude-model-selection-cancel"));

    expect(onConfigureTool).not.toHaveBeenCalled();
    expect(onSyncTool).not.toHaveBeenCalled();
  });

  it("有效的已存 claude_models 直接复用且不再弹窗", async () => {
    const user = userEvent.setup();
    const onSyncTool = vi.fn();
    const saved = {
      ...claudeTarget({
        synced: true,
        pending_sync: false,
        synced_key_id: "key-1",
        synced_at: 1_700_000_000,
      }),
      claude_models: {
        default_model: "cc-opus",
        haiku: "cc-haiku",
        sonnet: "cc-sonnet",
        opus: "cc-opus",
      },
    } as unknown as GatewayTerminalTarget;

    renderWithProviders(
      <TerminalSyncPanel
        targets={[saved]}
        config={claudeConfig()}
        syncingTools={{}}
        onConfigureTool={vi.fn()}
        onSyncTool={onSyncTool}
      />,
    );

    await user.click(screen.getByTestId("ai-gateway-sync-claude"));

    expect(
      screen.queryByTestId("claude-model-selection-dialog"),
    ).not.toBeInTheDocument();
    expect(onSyncTool).toHaveBeenCalledWith("claude", {
      default_model: "cc-opus",
      haiku: "cc-haiku",
      sonnet: "cc-sonnet",
      opus: "cc-opus",
    });
  });

  it("对话框候选列表只提供原生 Messages 可服务的本地模型 id", async () => {
    const user = userEvent.setup();

    renderWithProviders(
      <TerminalSyncPanel
        targets={[claudeTarget()]}
        config={claudeConfig()}
        syncingTools={{}}
        onConfigureTool={vi.fn()}
        onSyncTool={vi.fn()}
      />,
    );

    await user.click(screen.getByTestId("ai-gateway-sync-claude"));
    const dialog = screen.getByTestId("claude-model-selection-dialog");

    // The three CommandCode native-Messages local ids are selectable.
    for (const model of ["cc-haiku", "cc-sonnet", "cc-opus"]) {
      expect(
        within(dialog).getByRole("option", { name: model }),
      ).toBeInTheDocument();
    }
  });

  // R1-frontend: a saved Claude selection that references a local model no
  // native-Messages candidate can serve must open the four-choice dialog instead
  // of silently resending an unservable selection.
  it("已存 claude_models 引用不可服务模型时打开对话框且不调用同步回调", async () => {
    const user = userEvent.setup();
    const onSyncTool = vi.fn();
    const onConfigureTool = vi.fn();
    const staleSaved = {
      ...claudeTarget({
        synced: true,
        pending_sync: true,
        provider_id: "gateway-claude",
        base_url: "http://127.0.0.1:17688",
        synced_key_id: "key-1",
        synced_at: 1_700_000_000,
      }),
      claude_models: {
        default_model: "cc-opus",
        haiku: "removed-haiku",
        sonnet: "removed-sonnet",
        opus: "removed-opus",
      },
    } as unknown as GatewayTerminalTarget;

    renderWithProviders(
      <TerminalSyncPanel
        targets={[staleSaved]}
        config={claudeConfig()}
        syncingTools={{}}
        onConfigureTool={onConfigureTool}
        onSyncTool={onSyncTool}
      />,
    );

    await user.click(screen.getByTestId("ai-gateway-sync-claude"));

    expect(onSyncTool).not.toHaveBeenCalled();
    expect(onConfigureTool).not.toHaveBeenCalled();
    expect(
      screen.getByTestId("claude-model-selection-dialog"),
    ).toBeInTheDocument();
  });
});


