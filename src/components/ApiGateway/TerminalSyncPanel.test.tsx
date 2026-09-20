import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { TerminalSyncPanel } from "./TerminalSyncPanel";
import {
  formatGatewayTimestamp,
  type GatewayConfig,
  type GatewayTerminalTarget,
} from "@/lib/apiGateway";
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

    const openCodeSynced = screen.getByTestId("api-gateway-target-synced-opencode");
    expect(openCodeSynced).toHaveTextContent(formatGatewayTimestamp(syncedAt)!);
    expect(openCodeSynced).toHaveTextContent("Last sync");

    const codexSynced = screen.getByTestId("api-gateway-target-synced-codex");
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

    const openCodeSynced = screen.getByTestId("api-gateway-target-synced-opencode");
    expect(openCodeSynced).toHaveTextContent(formatGatewayTimestamp(syncedAt)!);
    expect(openCodeSynced).toHaveTextContent("最近同步");

    const codexSynced = screen.getByTestId("api-gateway-target-synced-codex");
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
    await user.click(screen.getByTestId("api-gateway-sync-opencode"));
    expect(handleSync).toHaveBeenCalledWith("opencode");

    // Codex 从未添加过，按钮为 Add provider
    await user.click(screen.getByTestId("api-gateway-sync-codex"));
    expect(handleConfigure).toHaveBeenCalledWith("codex");
  });
});
