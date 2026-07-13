import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { AiEnvironments } from "@/components/AiEnvironments";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock } from "@/test/mocks/tauri";

const providerState = {
  active_claude: null,
  active_codex: null,
  active_gemini: null,
  active_opencode: null,
  providers: [] as any[],
};

const opencodeProvider = {
  id: "opencode-provider-1",
  name: "OpenCode Provider",
  tool: "opencode",
  icon: "builtin:deepseek",
  api_key: "old-key",
  base_url: "https://old.example/v1",
  model: "old-model",
  provider_key: "ManualProvider",
  is_enabled: true,
  options: {
    apiKey: "old-key",
    baseURL: "https://old.example/v1",
  },
  models: {
    "old-model": {},
  },
};

const preset = {
  id: "vendor",
  name: "Vendor",
  description: "Vendor preset",
  icon: "builtin:claude",
  endpoints: {
    openai_base_url: "https://openai.vendor.example/v1",
    anthropic_base_url: "https://anthropic.vendor.example",
  },
  template: {
    claude_default_model: " claude-sonnet-4-5 ",
    claude_reasoning_effort: " high ",
    claude_model_mappings: [
      {
        family: "haiku",
        display_name: "Haiku",
        upstream_model: " claude-haiku-4-5 ",
        supports_1m: false,
      },
    ],
  },
  created_at: 1,
  updated_at: 1,
};

function mockAiEnvironmentCommands() {
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "service_providers_list") {
      return { ok: true, data: providerState };
    }
    if (command === "service_provider_presets_list") {
      return { ok: true, data: { presets: [preset] } };
    }
    if (command === "service_providers_list_synced_other_devices") {
      return { ok: true, data: [] };
    }
    if (command === "claude_profile_list") {
      return { ok: true, data: [] };
    }
    if (command === "get_storage_config") {
      return {};
    }
    if (command === "detect_cli_version") {
      return { version: "", is_installed: false };
    }
    if (command === "check_cli_update") {
      return undefined;
    }
    if (command === "cli_env_probe") {
      return { ok: true, data: undefined };
    }
    if (command === "service_providers_auto_import_from_system") {
      return { ok: true, data: { imported: false } };
    }
    if (command === "service_provider_presets_upsert") {
      return { ok: true, data: preset };
    }
    return { ok: true, data: null };
  });
}

describe("AiEnvironments provider preset editor", () => {
  beforeEach(() => {
    providerState.active_claude = null;
    providerState.active_codex = null;
    providerState.active_gemini = null;
    providerState.active_opencode = null;
    providerState.providers = [];
  });

  it("loads Claude-only preset fields and saves only non-empty mappings", async () => {
    const user = userEvent.setup();
    mockAiEnvironmentCommands();

    renderWithProviders(<AiEnvironments isVisible />);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("service_provider_presets_list");
    });
    await user.click(screen.getAllByRole("button", { name: /Add Service Provider|添加服务商/ })[0]);
    await screen.findByText("Vendor preset");
    await user.click(screen.getByRole("button", { name: /Edit preset|编辑预设/ }));

    expect(await screen.findByDisplayValue("claude-sonnet-4-5")).toBeInTheDocument();
    expect(screen.getByDisplayValue("high")).toBeInTheDocument();
    expect(screen.getByDisplayValue("claude-haiku-4-5")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Save|保存/ }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "service_provider_presets_upsert",
        expect.objectContaining({
          preset: expect.objectContaining({
            template: {
              claude_default_model: "claude-sonnet-4-5",
              claude_reasoning_effort: "high",
              claude_model_mappings: [
                {
                  family: "haiku",
                  display_name: "Haiku",
                  upstream_model: "claude-haiku-4-5",
                  supports_1m: false,
                },
              ],
            },
          }),
        }),
      );
    });
  });

  it("preserves manually edited OpenCode JSON when saving", async () => {
    const user = userEvent.setup();
    const manualJson = {
      name: "Manual OpenCode",
      options: {
        apiKey: "manual-key",
        baseURL: "https://manual.example/v1",
      },
      models: {
        "manual-model": {
          limit: { context: 128000 },
        },
      },
      customAdvancedOption: { preserve: true },
    };
    providerState.providers = [{ ...opencodeProvider, ...manualJson }];
    mockAiEnvironmentCommands();

    renderWithProviders(<AiEnvironments isVisible />);

    await user.click(screen.getByRole("button", { name: /OpenCode/ }));
    await user.click((await screen.findByText("Manual OpenCode")).closest("button")!);
    await user.click(screen.getByRole("button", { name: /Save|保存/ }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "service_providers_upsert",
        expect.objectContaining({
          provider: expect.objectContaining({
            api_key: "manual-key",
            base_url: "https://manual.example/v1",
            model: "manual-model",
            options: manualJson.options,
            models: manualJson.models,
            customAdvancedOption: manualJson.customAdvancedOption,
          }),
        }),
      );
    });
  });

  it("keeps the OpenCode service provider icon when saving", async () => {
    const user = userEvent.setup();
    providerState.providers = [opencodeProvider];
    mockAiEnvironmentCommands();

    renderWithProviders(<AiEnvironments isVisible />);

    await user.click(screen.getByRole("button", { name: /OpenCode/ }));
    await user.click((await screen.findByText("OpenCode Provider")).closest("button")!);
    await user.click(screen.getByRole("button", { name: /Save|保存/ }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "service_providers_upsert",
        expect.objectContaining({
          provider: expect.objectContaining({ icon: "builtin:deepseek" }),
        }),
      );
    });
  });
});
