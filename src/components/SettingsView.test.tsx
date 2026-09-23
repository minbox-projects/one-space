import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
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
    antigravity: "agy",
    codex: "codex",
    opencode: "opencode",
  },
  ai_model_permission_modes: {
    claude: "default",
    antigravity: "default",
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
  let currentTemplateAutoRefreshMinutes = 60;
  let templateAutoRefreshSaveError: Error | null = null;
  let failTemplateAutoRefreshGetAfterSave = false;
  let templateAutoRefreshGetError: Error | null = null;

  afterEach(() => {
    vi.restoreAllMocks();
  });

  beforeEach(async () => {
    resetTauriMocks();
    resetMessageMocks();
    currentTemplateAutoRefreshMinutes = 60;
    templateAutoRefreshSaveError = null;
    failTemplateAutoRefreshGetAfterSave = false;
    templateAutoRefreshGetError = null;
    await i18n.changeLanguage("en");

    let currentConfig = structuredClone(baseStorageConfig);
    let currentRetention = 90;

    invokeMock.mockImplementation(async (command: string, args?: any) => {
      if (command === "get_storage_config") {
        return structuredClone(currentConfig);
      }
      if (command === "ai_gateway_usage_retention_get") {
        return currentRetention;
      }
      if (command === "ai_gateway_usage_retention_save") {
        const days = args.days as number;
        if (!Number.isInteger(days) || days < 1 || days > 365) {
          throw new Error("Retention days must be between 1 and 365");
        }
        currentRetention = days;
        return currentRetention;
      }
      if (command === "ai_gateway_template_auto_refresh_get") {
        if (templateAutoRefreshGetError) {
          throw templateAutoRefreshGetError;
        }
        return currentTemplateAutoRefreshMinutes;
      }
      if (command === "ai_gateway_template_auto_refresh_save") {
        if (templateAutoRefreshSaveError) {
          throw templateAutoRefreshSaveError;
        }
        const minutes = args.minutes as number;
        if (
          minutes !== 0 &&
          (!Number.isInteger(minutes) || minutes < 10 || minutes > 1440)
        ) {
          throw new Error(
            "template auto refresh minutes must be 0 (disabled) or between 10 and 1440",
          );
        }
        currentTemplateAutoRefreshMinutes = minutes;
        if (failTemplateAutoRefreshGetAfterSave) {
          templateAutoRefreshGetError = new Error(
            "transient template auto refresh read failure",
          );
        }
        return currentTemplateAutoRefreshMinutes;
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
      if (command === "get_master_password") {
        return "existing-master-password";
      }
      if (command === "save_shared_profile") {
        return null;
      }
      if (command === "update_shortcuts") {
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

  it("does not show AI usage tab or load usage stats in AI terminal settings", async () => {
    renderWithProviders(<SettingsView initialTab="ai" onBack={() => {}} />);

    await waitFor(() => {
      expect(screen.getByText(/Default Model|默认模型/)).toBeInTheDocument();
    });
    expect(
      screen.queryByRole("button", { name: /Usage/ }),
    ).not.toBeInTheDocument();
    expect(
      invokeMock.mock.calls.some(([command]) =>
        String(command).startsWith("sessions_usage"),
      ),
    ).toBe(false);
  });

  it("AI 网关分区独立保存保留天数且不改写其他配置", async () => {
    const user = userEvent.setup();
    renderWithProviders(<SettingsView initialTab="ai-gateway" onBack={() => {}} />);

    const input = await screen.findByLabelText("Log retention days");
    expect(input).toHaveValue(90);

    await user.clear(input);
    await user.type(input, "30");
    await user.click(
      screen.getByRole("button", { name: /Save Settings|保存设置/ }),
    );

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_usage_retention_save", {
        days: 30,
      }),
    );
    expect(screen.getByLabelText("Log retention days")).toHaveValue(30);
    const commands = invokeMock.mock.calls.map(([command]) => command);
    expect(commands).not.toContain("save_storage_config");
    expect(commands).not.toContain("protocol_router_save_config");
  });

  it("保留天数 0 或 400 被拒绝并显示可操作错误，1 与 365 保存成功", async () => {
    const user = userEvent.setup();
    renderWithProviders(<SettingsView initialTab="ai-gateway" onBack={() => {}} />);

    const input = await screen.findByLabelText("Log retention days");

    await user.clear(input);
    await user.type(input, "0");
    await user.click(
      screen.getByRole("button", { name: /Save Settings|保存设置/ }),
    );
    expect(
      await screen.findByText(
        /Retention days must be a whole number between 1 and 365/,
      ),
    ).toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith(
      "ai_gateway_usage_retention_save",
      expect.anything(),
    );

    await user.clear(input);
    await user.type(input, "400");
    await user.click(
      screen.getByRole("button", { name: /Save Settings|保存设置/ }),
    );
    expect(
      await screen.findByText(
        /Retention days must be a whole number between 1 and 365/,
      ),
    ).toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith(
      "ai_gateway_usage_retention_save",
      expect.anything(),
    );

    await user.clear(input);
    await user.type(input, "1");
    await user.click(
      screen.getByRole("button", { name: /Save Settings|保存设置/ }),
    );
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_usage_retention_save", {
        days: 1,
      }),
    );

    await user.clear(input);
    await user.type(input, "365");
    await user.click(
      screen.getByRole("button", { name: /Save Settings|保存设置/ }),
    );
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_usage_retention_save", {
        days: 365,
      }),
    );
  });

  it("重置恢复最近一次已保存的保留天数且不改写其他分区", async () => {
    const user = userEvent.setup();
    renderWithProviders(<SettingsView initialTab="ai-gateway" onBack={() => {}} />);

    const input = await screen.findByLabelText("Log retention days");
    await user.clear(input);
    await user.type(input, "30");
    await user.click(
      screen.getByRole("button", { name: /Save Settings|保存设置/ }),
    );
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_usage_retention_save", {
        days: 30,
      }),
    );

    await user.clear(input);
    await user.type(input, "45");
    const resetButtons = screen.getAllByRole("button", {
      name: /Reset|重置/,
    });
    await user.click(resetButtons[0]);
    const confirmButtons = screen.getAllByRole("button", {
      name: /Reset|重置/,
    });
    await user.click(confirmButtons[confirmButtons.length - 1]);

    await waitFor(() =>
      expect(screen.getByLabelText("Log retention days")).toHaveValue(30),
    );
    const commands = invokeMock.mock.calls.map(([command]) => command);
    expect(commands).not.toContain("save_storage_config");
  });

  it("AI 网关分区不包含模型价格维护入口", async () => {
    renderWithProviders(<SettingsView initialTab="ai-gateway" onBack={() => {}} />);

    await screen.findByLabelText("Log retention days");
    expect(
      screen.queryByRole("button", { name: /Model prices|模型价格/ }),
    ).not.toBeInTheDocument();
    expect(screen.queryByText(/USD \/ million tokens/)).not.toBeInTheDocument();
  });

  it("keeps the random MD5 password generation contract", async () => {
    const user = userEvent.setup();
    const randomUuidSpy = vi
      .spyOn(crypto, "randomUUID")
      .mockReturnValue("12345678-1234-4234-8234-123456789abc");
    const dateNowSpy = vi.spyOn(Date, "now").mockReturnValue(1_725_000_000_000);
    const mathRandomSpy = vi
      .spyOn(Math, "random")
      .mockReturnValueOnce(0.125)
      .mockReturnValueOnce(0.875);

    renderWithProviders(<SettingsView initialTab="security" onBack={() => {}} />);
    await screen.findByDisplayValue("existing-master-password");
    await user.click(
      screen.getByRole("button", { name: /Change Master Password|修改主密码/ }),
    );
    const generateButton = screen.getByRole("button", {
      name: /Generate MD5 Password|生成 MD5 密码/,
    });
    const generateClickEvent = new MouseEvent("click", { bubbles: true });
    randomUuidSpy.mockClear();
    dateNowSpy.mockClear();
    mathRandomSpy.mockClear();
    act(() => {
      generateButton.dispatchEvent(generateClickEvent);
    });

    const generatedInputs = screen.getAllByRole("textbox");
    expect(generatedInputs).toHaveLength(2);
    expect(generatedInputs[0]).toHaveValue(generatedInputs[1].getAttribute("value"));
    expect(generatedInputs[0]).toHaveValue(
      "4c8f6640-adcf-528b-ae09-358a57a50b53",
    );
    expect((generatedInputs[0] as HTMLInputElement).value).toMatch(
      /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/,
    );
    expect(randomUuidSpy).toHaveBeenCalledTimes(1);
    expect(dateNowSpy).toHaveBeenCalledTimes(1);
    expect(mathRandomSpy).toHaveBeenCalledTimes(2);
  });

  it("shows the loaded template auto refresh minutes, defaulting a missing field to 60", async () => {
    currentTemplateAutoRefreshMinutes = 60;
    const firstView = renderWithProviders(
      <SettingsView initialTab="ai-gateway" onBack={() => {}} />,
    );
    const defaultInput = await screen.findByLabelText(
      "Template auto refresh minutes",
    );
    await waitFor(() => expect(defaultInput).toHaveValue(60));
    firstView.unmount();

    for (const value of [0, 10, 1440]) {
      currentTemplateAutoRefreshMinutes = value;
      const view = renderWithProviders(
        <SettingsView initialTab="ai-gateway" onBack={() => {}} />,
      );
      const input = await screen.findByLabelText(
        "Template auto refresh minutes",
      );
      await waitFor(() => expect(input).toHaveValue(value));
      view.unmount();
    }
  });

  it("saves 0/10/60/1440, re-reads the persisted value and notifies subscribers", async () => {
    const { subscribeTemplateAutoRefreshIntervalChanged } = (await import(
      "@/lib/aiGateway"
    )) as unknown as {
      subscribeTemplateAutoRefreshIntervalChanged: (
        listener: () => void,
      ) => () => void;
    };
    const listener = vi.fn();
    const unsubscribe = subscribeTemplateAutoRefreshIntervalChanged(listener);

    currentTemplateAutoRefreshMinutes = 30;
    const user = userEvent.setup();
    renderWithProviders(<SettingsView initialTab="ai-gateway" onBack={() => {}} />);

    const input = await screen.findByLabelText(
      "Template auto refresh minutes",
    );
    await waitFor(() => expect(input).toHaveValue(30));
    const saveButton = screen.getByRole("button", {
      name: /Save Settings|保存设置/,
    });

    for (const minutes of [0, 10, 60, 1440]) {
      await user.clear(input);
      await user.type(input, String(minutes));
      invokeMock.mockClear();
      const listenerCallsBeforeSave = listener.mock.calls.length;

      await user.click(saveButton);

      await waitFor(() =>
        expect(invokeMock).toHaveBeenCalledWith(
          "ai_gateway_template_auto_refresh_save",
          { minutes },
        ),
      );
      await waitFor(() =>
        expect(invokeMock).toHaveBeenCalledWith(
          "ai_gateway_template_auto_refresh_get",
        ),
      );
      await waitFor(() =>
        expect(listener.mock.calls.length).toBeGreaterThan(
          listenerCallsBeforeSave,
        ),
      );
    }

    unsubscribe();
  });

  it("shows success and keeps the saved value when the post-save re-read fails", async () => {
    failTemplateAutoRefreshGetAfterSave = true;
    currentTemplateAutoRefreshMinutes = 30;
    const user = userEvent.setup();
    renderWithProviders(<SettingsView initialTab="ai-gateway" onBack={() => {}} />);

    const input = await screen.findByLabelText(
      "Template auto refresh minutes",
    );
    await waitFor(() => expect(input).toHaveValue(30));

    await user.clear(input);
    await user.type(input, "60");
    await user.click(
      screen.getByRole("button", { name: /Save Settings|保存设置/ }),
    );

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "ai_gateway_template_auto_refresh_save",
        { minutes: 60 },
      ),
    );

    expect(
      await screen.findByText(/Current section saved\.|当前菜单已保存。/),
    ).toBeInTheDocument();
    expect(input).toHaveValue(60);
    expect(
      screen.queryByText(
        /Template auto refresh minutes must be 0 \(disabled\) or between 10 and 1440/,
      ),
    ).not.toBeInTheDocument();
  });

  it("blocks 5 and an empty value, never saving and showing the localized range message", async () => {
    currentTemplateAutoRefreshMinutes = 30;
    const user = userEvent.setup();
    renderWithProviders(<SettingsView initialTab="ai-gateway" onBack={() => {}} />);

    const input = await screen.findByLabelText(
      "Template auto refresh minutes",
    );
    await waitFor(() => expect(input).toHaveValue(30));
    const saveButton = screen.getByRole("button", {
      name: /Save Settings|保存设置/,
    });

    await user.clear(input);
    await user.type(input, "5");
    await user.click(saveButton);

    expect(
      await screen.findByText(
        /Template auto refresh minutes must be 0 \(disabled\) or between 10 and 1440/,
      ),
    ).toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith(
      "ai_gateway_template_auto_refresh_save",
      expect.anything(),
    );

    await user.clear(input);
    await user.click(saveButton);

    expect(
      await screen.findByText(
        /Template auto refresh minutes must be 0 \(disabled\) or between 10 and 1440/,
      ),
    ).toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith(
      "ai_gateway_template_auto_refresh_save",
      expect.anything(),
    );
  });

  it.each(["10.5", "-1"])(
    "blocks the non-whole/negative interval %s, never saving and showing the localized range message",
    async (value) => {
      currentTemplateAutoRefreshMinutes = 30;
      const user = userEvent.setup();
      renderWithProviders(
        <SettingsView initialTab="ai-gateway" onBack={() => {}} />,
      );

      const input = await screen.findByLabelText(
        "Template auto refresh minutes",
      );
      await waitFor(() => expect(input).toHaveValue(30));
      const saveButton = screen.getByRole("button", {
        name: /Save Settings|保存设置/,
      });

      await user.clear(input);
      await user.type(input, value);
      expect(input).toHaveValue(Number(value));
      await user.click(saveButton);

      expect(
        await screen.findByText(
          /Template auto refresh minutes must be 0 \(disabled\) or between 10 and 1440/,
        ),
      ).toBeInTheDocument();
      expect(invokeMock).not.toHaveBeenCalledWith(
        "ai_gateway_template_auto_refresh_save",
        expect.anything(),
      );
    },
  );

  it("maps a backend rejection to the localized message instead of the raw error", async () => {
    currentTemplateAutoRefreshMinutes = 30;
    templateAutoRefreshSaveError = new Error(
      "template_auto_refresh_invalid: backend rejected 15",
    );
    const user = userEvent.setup();
    renderWithProviders(<SettingsView initialTab="ai-gateway" onBack={() => {}} />);

    const input = await screen.findByLabelText(
      "Template auto refresh minutes",
    );
    await waitFor(() => expect(input).toHaveValue(30));
    await user.clear(input);
    await user.type(input, "15");
    await user.click(
      screen.getByRole("button", { name: /Save Settings|保存设置/ }),
    );

    expect(
      await screen.findByText(
        /Template auto refresh minutes must be 0 \(disabled\) or between 10 and 1440/,
      ),
    ).toBeInTheDocument();
    expect(
      screen.queryByText(/template_auto_refresh_invalid: backend rejected 15/),
    ).not.toBeInTheDocument();
  });

  it("ships bilingual template auto refresh label and validation text", async () => {
    await i18n.changeLanguage("en");
    const enLabel = i18n.t("aiGatewayTemplateAutoRefreshLabel");
    const enInvalid = i18n.t("aiGatewayTemplateAutoRefreshInvalid");
    expect(enLabel).toBe("Template auto refresh minutes");
    expect(enInvalid).toBe(
      "Template auto refresh minutes must be 0 (disabled) or between 10 and 1440",
    );

    await i18n.changeLanguage("zh");
    const zhLabel = i18n.t("aiGatewayTemplateAutoRefreshLabel");
    const zhInvalid = i18n.t("aiGatewayTemplateAutoRefreshInvalid");
    expect(zhLabel).not.toBe("aiGatewayTemplateAutoRefreshLabel");
    expect(zhInvalid).not.toBe("aiGatewayTemplateAutoRefreshInvalid");
    expect(zhLabel.trim()).not.toBe("");
    expect(zhInvalid.trim()).not.toBe("");
    expect(zhLabel).not.toBe(enLabel);
    expect(zhInvalid).not.toBe(enInvalid);
  });

  it("renders the interval input and invalid message in Chinese without key fallback", async () => {
    await i18n.changeLanguage("zh");
    try {
      const zhLabel = i18n.t("aiGatewayTemplateAutoRefreshLabel");
      const zhInvalid = i18n.t("aiGatewayTemplateAutoRefreshInvalid");

      currentTemplateAutoRefreshMinutes = 30;
      const user = userEvent.setup();
      renderWithProviders(
        <SettingsView initialTab="ai-gateway" onBack={() => {}} />,
      );

      const input = await screen.findByLabelText(zhLabel);
      await waitFor(() => expect(input).toHaveValue(30));

      await user.clear(input);
      await user.type(input, "5");
      await user.click(
        screen.getByRole("button", { name: /Save Settings|保存设置/ }),
      );

      expect(await screen.findByText(zhInvalid)).toBeInTheDocument();
      // 中文渲染不得回退到键名，也不得泄露原始错误串。
      expect(
        screen.queryByText("aiGatewayTemplateAutoRefreshLabel"),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByText("aiGatewayTemplateAutoRefreshInvalid"),
      ).not.toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });
});
