import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { AiEnvironments } from "@/components/AiEnvironments";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock } from "@/test/mocks/tauri";

const providerState = {
  active_claude: null,
  active_codex: null,
  active_gemini: null,
  active_opencode: [] as string[],
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
    "old-model": {
      name: "Old Model",
      limit: { context: 128000, output: 4096 },
      unknownModelField: { preserve: true },
    },
  },
  unknownProviderField: { preserve: true },
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

function mockAiEnvironmentCommands(runtimeOpenCodeConfig?: Record<string, unknown> | Error) {
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
    if (command === "service_provider_read_opencode_config") {
      if (runtimeOpenCodeConfig instanceof Error) {
        throw runtimeOpenCodeConfig;
      }
      if (runtimeOpenCodeConfig) {
        return { ok: true, data: runtimeOpenCodeConfig };
      }
      const provider = providerState.providers.find((item) => item.tool === "opencode");
      const internalFields = new Set([
        "id", "tool", "icon", "api_key", "base_url", "model", "provider_key", "is_enabled",
      ]);
      const config = Object.fromEntries(
        Object.entries(provider || {}).filter(([key]) => !internalFields.has(key)),
      );
      return { ok: true, data: config };
    }
    if (command === "service_provider_presets_upsert") {
      return { ok: true, data: preset };
    }
    return { ok: true, data: null };
  });
}

function commandNames() {
  return invokeMock.mock.calls.map(([command]) => command as string);
}

function openCodeJsonEditor() {
  const editor = Array.from(document.querySelectorAll("textarea")).find((element) => {
    try {
      const value = JSON.parse(element.value);
      return value && typeof value === "object" && "models" in value;
    } catch {
      return element.value.startsWith("{");
    }
  });
  expect(editor).toBeDefined();
  return editor as HTMLTextAreaElement;
}

describe("AiEnvironments provider preset editor", () => {
  beforeEach(() => {
    providerState.active_claude = null;
    providerState.active_codex = null;
    providerState.active_gemini = null;
    providerState.active_opencode = [];
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
          limit: { context: 128000, output: 4096 },
          unknownModelField: { nested: true },
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
            options: manualJson.options,
            models: manualJson.models,
            customAdvancedOption: manualJson.customAdvancedOption,
          }),
        }),
      );
      const upsertCall = invokeMock.mock.calls.find(([command]) => command === "service_providers_upsert");
      const savedProvider = upsertCall?.[1]?.provider;
      for (const legacyField of [
        "model",
        "opencode_default_model",
        "opencode_default_agent",
        "opencode_sessions_dir",
        "small_model",
        "timeout",
        "share_mode",
      ]) {
        expect(savedProvider).not.toHaveProperty(legacyField);
      }
    });
  });

  it("duplicates only the canonical saved provider into a clean unsaved identity", async () => {
    const user = userEvent.setup();
    providerState.providers = [{
      ...opencodeProvider,
      history: [{ action: "upsert" }],
      favorite_at: 123,
      options: {
        ...opencodeProvider.options,
        nested: [{ accessToken: "nested-secret", region: "eu" }],
      },
    }];
    mockAiEnvironmentCommands({
      name: "Runtime config must not be copied",
      models: { runtime: { name: "Runtime" } },
    });

    renderWithProviders(<AiEnvironments isVisible />);
    await user.click(screen.getByRole("button", { name: /OpenCode/ }));
    await screen.findByText("OpenCode Provider");
    invokeMock.mockClear();
    await user.click(screen.getByRole("button", { name: /Duplicate provider|复制服务商|复制创建/ }));

    expect(await screen.findByDisplayValue("OpenCode Provider 副本")).toBeInTheDocument();
    const identifierLabel = screen.getByText(/Service Provider Identifier|服务商标识/);
    const identifier = identifierLabel.closest(".field")?.querySelector("input");
    expect(identifier).toBeInTheDocument();
    expect(identifier).not.toHaveValue("ManualProvider");
    expect(screen.getByDisplayValue("https://old.example/v1")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Save|保存/ })).toBeEnabled();
    const json = openCodeJsonEditor().value;
    expect(json).toContain('"region": "eu"');
    expect(json).toContain('"unknownProviderField"');
    expect(json).not.toContain("old-key");
    expect(json).not.toContain("nested-secret");
    expect(json).not.toContain("history");
    expect(commandNames()).not.toContain("service_provider_read_opencode_config");
    expect(commandNames()).not.toContain("service_providers_upsert");
  });

  it("synchronizes JSON and model fields in both directions while preserving unknown fields", async () => {
    const user = userEvent.setup();
    const runtimeConfig = {
      name: "OpenCode Provider",
      options: { apiKey: "old-key", baseURL: "https://old.example/v1" },
      unknownProviderField: { nested: { preserve: true } },
      models: {
        "json-model": {
          name: "JSON Model",
          limit: { context: 64000, output: 2048, unknownLimit: true },
          unknownModelField: { preserve: true },
        },
      },
    };
    providerState.providers = [opencodeProvider];
    mockAiEnvironmentCommands(runtimeConfig);

    renderWithProviders(<AiEnvironments isVisible />);
    await user.click(screen.getByRole("button", { name: /OpenCode/ }));
    await user.click((await screen.findByText("OpenCode Provider")).closest("button")!);

    const editor = openCodeJsonEditor();
    expect(await screen.findByDisplayValue("json-model")).toBeInTheDocument();
    const changedJson = {
      ...runtimeConfig,
      models: {
        "edited-in-json": {
          name: "Edited in JSON",
          limit: { context: 32000, output: 1024 },
          unknownModelField: { preserve: "json" },
        },
      },
    };
    fireEvent.change(editor, { target: { value: JSON.stringify(changedJson, null, 2) } });
    expect(await screen.findByDisplayValue("edited-in-json")).toBeInTheDocument();

    fireEvent.change(screen.getByRole("textbox", { name: /Model name|模型名称/ }), {
      target: { value: "Edited in form" },
    });
    await waitFor(() => {
      const synced = JSON.parse(openCodeJsonEditor().value);
      expect(synced.models["edited-in-json"].name).toBe("Edited in form");
      expect(synced.models["edited-in-json"].unknownModelField).toEqual({ preserve: "json" });
      expect(synced.unknownProviderField).toEqual({ nested: { preserve: true } });
    });
  });

  it("freezes the last valid model snapshot for invalid JSON and immediately recovers", async () => {
    const user = userEvent.setup();
    providerState.providers = [opencodeProvider];
    mockAiEnvironmentCommands({
      name: "OpenCode Provider",
      options: { apiKey: "old-key", baseURL: "https://old.example/v1" },
      models: opencodeProvider.models,
      unknownProviderField: opencodeProvider.unknownProviderField,
    });

    renderWithProviders(<AiEnvironments isVisible />);
    await user.click(screen.getByRole("button", { name: /OpenCode/ }));
    await user.click((await screen.findByText("OpenCode Provider")).closest("button")!);
    const editor = openCodeJsonEditor();
    const modelId = await screen.findByDisplayValue("old-model");

    fireEvent.change(editor, { target: { value: "{" } });
    expect(modelId).toHaveValue("old-model");
    expect(modelId).toBeDisabled();
    expect(screen.getByRole("button", { name: /Save|保存/ })).toBeDisabled();

    const repaired = {
      name: "Recovered",
      options: { apiKey: "recovered-key", baseURL: "https://recovered.example/v1" },
      models: { recovered: { name: "Recovered", limit: { context: 1, output: 1 } } },
    };
    fireEvent.change(editor, { target: { value: JSON.stringify(repaired) } });
    expect(await screen.findByDisplayValue("recovered")).toBeEnabled();
    expect(screen.getByRole("button", { name: /Save|保存/ })).toBeEnabled();
  });

  it("keeps upsert, provider/history refresh, and active projection in order", async () => {
    const user = userEvent.setup();
    providerState.providers = [{
      ...opencodeProvider,
      history: [{ timestamp: 1, action: "upsert", snapshot: { name: "Before" } }],
      opencode_default_model: "legacy-default",
      opencode_default_agent: "legacy-agent",
      opencode_sessions_dir: "/tmp/legacy",
      small_model: "legacy-small",
      timeout: 30,
      share_mode: "manual",
    }];
    providerState.active_opencode = [opencodeProvider.id];
    mockAiEnvironmentCommands({
      name: "OpenCode Provider",
      options: { apiKey: "old-key", baseURL: "https://old.example/v1" },
      models: opencodeProvider.models,
      unknownProviderField: opencodeProvider.unknownProviderField,
    });

    renderWithProviders(<AiEnvironments isVisible />);
    await user.click(screen.getByRole("button", { name: /OpenCode/ }));
    await user.click((await screen.findByText("OpenCode Provider")).closest("button")!);
    await waitFor(() => expect(commandNames()).toContain("service_provider_read_opencode_config"));
    invokeMock.mockClear();
    await user.click(screen.getByRole("button", { name: /Save|保存/ }));

    await waitFor(() => expect(commandNames()).toContain("projection_apply"));
    const commands = commandNames();
    expect(commands.indexOf("service_providers_upsert")).toBeLessThan(commands.indexOf("service_providers_list"));
    expect(commands.indexOf("service_providers_list")).toBeLessThan(commands.indexOf("projection_apply"));
    expect(invokeMock).toHaveBeenCalledWith("projection_apply", {
      tool: "opencode",
      providerId: opencodeProvider.id,
    });
    const savedProvider = invokeMock.mock.calls.find(
      ([command]) => command === "service_providers_upsert",
    )?.[1]?.provider as Record<string, unknown>;
    expect(savedProvider.history).toEqual(providerState.providers[0].history);
    for (const field of ["model", "opencode_default_model", "opencode_default_agent", "opencode_sessions_dir", "small_model", "timeout", "share_mode"]) {
      expect(savedProvider).not.toHaveProperty(field);
    }
  });

  it("loads the clicked OpenCode provider's latest runtime config into the detail editor", async () => {
    const user = userEvent.setup();
    const latestConfig = {
      name: "Latest OpenCode",
      options: { apiKey: "latest-key", nested: { preserve: true } },
      models: { "latest-model": { limit: { context: 200000 } } },
      unknownTopLevel: { keep: true },
    };
    providerState.providers = [opencodeProvider];
    mockAiEnvironmentCommands(latestConfig);

    renderWithProviders(<AiEnvironments isVisible />);

    await user.click(screen.getByRole("button", { name: /OpenCode/ }));
    await user.click((await screen.findByText("OpenCode Provider")).closest("button")!);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("service_provider_read_opencode_config", {
        providerKey: "ManualProvider",
      });
    });
    await waitFor(() => {
      const editor = Array.from(document.querySelectorAll("textarea")).find(
        (element) => element.value === JSON.stringify(latestConfig, null, 2),
      );
      expect(editor).toBeDefined();
    });
  });

  it("does not read OpenCode runtime config when opening a non-OpenCode provider", async () => {
    const user = userEvent.setup();
    providerState.providers = [{
      id: "codex-provider-1",
      name: "Codex Provider",
      tool: "codex",
      api_key: "codex-key",
      base_url: "https://codex.example/v1",
      model: "codex-model",
      is_enabled: true,
    }];
    mockAiEnvironmentCommands();

    renderWithProviders(<AiEnvironments isVisible />);

    await user.click(screen.getByRole("button", { name: /Codex/ }));
    await user.click((await screen.findByText("Codex Provider")).closest("button")!);

    expect(screen.getByDisplayValue("Codex Provider")).toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith(
      "service_provider_read_opencode_config",
      expect.anything(),
    );
  });

  it("keeps the cached OpenCode detail usable and reports runtime config read failures", async () => {
    const user = userEvent.setup();
    providerState.providers = [opencodeProvider];
    mockAiEnvironmentCommands(new Error("invalid JSON in opencode.json"));

    renderWithProviders(<AiEnvironments isVisible />);

    await user.click(screen.getByRole("button", { name: /OpenCode/ }));
    await user.click((await screen.findByText("OpenCode Provider")).closest("button")!);

    expect(await screen.findByText(/Failed to read OpenCode provider config: invalid JSON/)).toBeInTheDocument();
    await waitFor(() => {
      const cachedEditor = Array.from(document.querySelectorAll("textarea")).find(
        (element) => element.value.includes('"old-model"'),
      );
      expect(cachedEditor).toBeDefined();
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
