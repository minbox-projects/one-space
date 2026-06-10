import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { SettingsView } from "@/components/SettingsView";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";
import { resetMessageMocks } from "@/test/mocks/messages";

const baseStorageConfig = {
  storage_type: "local",
  auth_method: "http",
  main_shortcut: "Alt+Space",
  quick_ai_shortcut: "Alt+Shift+A",
  default_ai_model: "claude",
  claude_provider_launch_dir: "",
  ai_terminal_app: "Terminal",
  ai_model_launch_commands: {
    claude: "claude --session-id {session_id}",
    gemini: "gemini",
    codex: "codex",
    opencode: "opencode",
  },
  ai_model_permission_modes: {
    claude: "default",
    gemini: "default",
    codex: "default",
    opencode: "default",
  },
  ai_sessions_history_days: 30,
  message_retention_days: 30,
  launch_at_login: false,
  auto_update_enabled: false,
  update_check_interval_minutes: 360,
  skills_sync_enabled: true,
  skills_auto_update_enabled: false,
  skills_sync_interval_minutes: 60,
  skills_new_badge_hours: 72,
  skills_sources: [],
  subagents_sync_enabled: true,
  subagents_sync_interval_minutes: 60,
  subagents_new_badge_hours: 72,
  subagents_sources: [],
  ai_news_enabled: false,
  ai_news_sync_interval_minutes: 60,
  ai_news_retention_days: 90,
  ai_news_retention_max_items: 1000,
  ai_news_keywords:
    "artificial intelligence, generative AI, LLM, large language model, OpenAI, Anthropic, Gemini",
  ai_news_rss_sources: [],
  sync_policy: {
    providers: true,
    mcp: true,
    content: true,
    workflow_presets: true,
    skills_sources: true,
    skills_repository: false,
    subagents_sources: true,
    subagents_repository: false,
    ai_news: false,
  },
  proxy: {
    proxy_enabled: false,
    proxy_type: "socks5",
    proxy_host: "",
    proxy_port: 1080,
    proxy_username: "",
    proxy_password: "",
    check_interval: 15,
  },
};

describe("SettingsView", () => {
  beforeEach(() => {
    resetTauriMocks();
    resetMessageMocks();

    let currentConfig = structuredClone(baseStorageConfig);

    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "get_storage_config") {
        return structuredClone(currentConfig);
      }
      if (command === "save_storage_config") {
        currentConfig = {
          ...currentConfig,
          ...args.config,
          proxy: args.config.proxy ?? currentConfig.proxy,
        };
        return null;
      }
      if (command === "protocol_router_get_config") {
        return {
          enabled: false,
          port: 17687,
          token: "",
          retention_days: 30,
          routes: [],
        };
      }
      if (command === "protocol_router_status") {
        return { running: false, enabled: false, port: 17687, route_count: 0 };
      }
      if (command === "skills_sync_status_get") {
        return { ok: true, data: null, meta: { revision: 1, ts: 1 } };
      }
      if (command === "subagents_sync_status_get") {
        return { ok: true, data: null, meta: { revision: 1, ts: 1 } };
      }
      if (command === "plugin:autostart|is_enabled") {
        return false;
      }
      if (command === "save_shared_profile") {
        return null;
      }
      if (command === "update_tray_menu" || command === "update_shortcuts") {
        return null;
      }
      throw new Error(`Unhandled command: ${command}`);
    });
  });

  it("does not keep showing unsaved changes after saving the current settings section", async () => {
    const user = userEvent.setup();
    renderWithProviders(<SettingsView initialTab="general" onBack={() => {}} />);

    expect(
      await screen.findByText(/No unsaved changes in this section|当前菜单暂无未保存更改/),
    ).toBeInTheDocument();

    const input = screen.getByDisplayValue("30");
    await user.clear(input);
    await user.type(input, "45");

    expect(
      await screen.findByText(/Unsaved changes in this section|当前菜单有未保存更改/),
    ).toBeInTheDocument();

    await user.click(
      screen.getByRole("button", { name: /Save Settings|保存设置/ }),
    );

    const confirmButtons = screen.getAllByRole("button", { name: /Save|保存/ });
    await user.click(confirmButtons[confirmButtons.length - 1]);

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: /Save Settings|保存设置/ }),
      ).toBeDisabled();
      expect(
        screen.queryByText(/Unsaved changes in this section|当前菜单有未保存更改/),
      ).not.toBeInTheDocument();
    });
  });

  it("allows manually typing ai news sync interval and saves empty keywords", async () => {
    const user = userEvent.setup();
    renderWithProviders(<SettingsView initialTab="news" onBack={() => {}} />);

    await screen.findByText(/Enable AI News|启用 AI 新闻/);
    await user.click(screen.getByRole("switch"));

    const intervalInput = screen.getByDisplayValue("60");
    await user.clear(intervalInput);
    await user.type(intervalInput, "1");
    expect(screen.getByDisplayValue("1")).toBeInTheDocument();
    await user.type(intervalInput, "0");
    expect(screen.getByDisplayValue("10")).toBeInTheDocument();

    const keywordsInput = screen.getAllByRole("textbox")[1];
    await user.clear(keywordsInput);

    await user.click(
      screen.getByRole("button", { name: /Save Settings|保存设置/ }),
    );

    const confirmButtons = screen.getAllByRole("button", { name: /Save|保存/ });
    await user.click(confirmButtons[confirmButtons.length - 1]);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("save_storage_config", {
        config: expect.objectContaining({
          ai_news_sync_interval_minutes: 10,
          ai_news_keywords: "",
        }),
      });
    });
  });
});
