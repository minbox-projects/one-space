import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ServiceProviderDetail } from "@/components/AiEnvironments/ServiceProviderDetail";
import { renderWithProviders } from "@/test/mocks/render";

const baseClaudeProvider = {
  id: "claude-provider-1",
  name: "Claude Provider",
  tool: "claude",
  api_key: "",
  base_url: "https://api.anthropic.com",
  claude_api_format: "anthropic_messages",
  claude_connection_mode: "native_anthropic",
  claude_model_mappings: [
    { family: "haiku", display_name: "Haiku", upstream_model: "claude-haiku-4-5", supports_1m: false },
    { family: "sonnet", display_name: "Sonnet", upstream_model: "claude-sonnet-4-5", supports_1m: true, supported_capabilities: ["image"] },
    { family: "opus", display_name: "Opus", upstream_model: "claude-opus-4-5", supports_1m: false },
  ],
  claude_enable_tool_search: false,
  claude_auto_memory_enabled: false,
  claude_always_thinking_enabled: false,
  claude_away_summary_enabled: false,
  claude_include_git_instructions: false,
  claude_enable_attribution: false,
  model: "claude-sonnet-4-5",
  claude_default_model: "claude-sonnet-4-5",
};

describe("ServiceProviderDetail Claude form", () => {
  it("uses simplified Claude mapping fields and API key default", () => {
    const { container } = renderWithProviders(
      <ServiceProviderDetail
        provider={baseClaudeProvider}
        onChange={vi.fn()}
        onSave={vi.fn()}
        onActivate={vi.fn()}
        onDelete={vi.fn()}
        onBack={vi.fn()}
      />,
    );

    const connectionModeLabel = screen.getByText(/Connection Mode|连接模式/);
    const connectionModeField = connectionModeLabel.closest(".field");
    const connectionModeSelect = connectionModeField?.querySelector("select");
    expect(connectionModeSelect).toBeTruthy();
    const connectionModeOptions = Array.from(connectionModeSelect?.querySelectorAll("option") ?? []).map(
      (option) => option.textContent,
    );
    expect(connectionModeOptions).toContain("Native Anthropic（原生）");
    expect(connectionModeOptions).toContain("Protocol Router（协议路由）");

    const authEnvKeyLabel = screen.getByText(/Auth Env Key|认证环境变量/);
    const authEnvKeyField = authEnvKeyLabel.closest(".field");
    const authSelect = authEnvKeyField?.querySelector("select") as HTMLSelectElement | null;
    expect(authSelect?.value).toBe("ANTHROPIC_API_KEY");

    expect(screen.queryByRole("columnheader", { name: /^Effort$/ })).not.toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: /Capabilities|能力/ })).toBeInTheDocument();

    const table = container.querySelector("table");
    expect(table).toBeTruthy();
    const tableScope = within(table as HTMLTableElement);
    expect(tableScope.getByRole("columnheader", { name: /Family|模型家族/ })).toBeInTheDocument();
    expect(tableScope.getByRole("columnheader", { name: /Display Name|显示名称/ })).toBeInTheDocument();
    expect(tableScope.getByRole("columnheader", { name: /Upstream Model|上游模型/ })).toBeInTheDocument();
    expect(tableScope.getByRole("columnheader", { name: /Capabilities|能力/ })).toBeInTheDocument();
    expect(tableScope.getByRole("columnheader", { name: "1M" })).toBeInTheDocument();

    const defaultModelLabel = screen.getByText(/Default Model|默认模型/);
    const reasoningEffortLabel = screen.getByText(/Reasoning Effort|推理努力程度/);
    const defaultModelField = defaultModelLabel.closest(".field");
    const reasoningEffortField = reasoningEffortLabel.closest(".field");
    expect(defaultModelField?.querySelector("input")).toBeTruthy();
    expect(reasoningEffortField?.querySelector("input")?.getAttribute("placeholder")).toBe(
      "high / xhigh / max / auto / custom",
    );
  });

  it("renders synchronized top-level Claude model in JSON preview", () => {
    renderWithProviders(
      <ServiceProviderDetail
        provider={baseClaudeProvider}
        onChange={vi.fn()}
        onSave={vi.fn()}
        onActivate={vi.fn()}
        onDelete={vi.fn()}
        onBack={vi.fn()}
        jsonMode="claude"
      />,
    );

    expect(screen.getByText(/"model": "claude-sonnet-4-5"/)).toBeInTheDocument();
    expect(screen.getByText(/"ANTHROPIC_MODEL": "claude-sonnet-4-5"/)).toBeInTheDocument();
  });

  it("shows saving state and disables detail actions while saving", () => {
    renderWithProviders(
      <ServiceProviderDetail
        provider={baseClaudeProvider}
        onChange={vi.fn()}
        onSave={vi.fn()}
        onActivate={vi.fn()}
        onDelete={vi.fn()}
        onBack={vi.fn()}
        saving
      />,
    );

    expect(screen.getByRole("button", { name: /Saving...|保存中.../ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /Activate|激活/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /Delete|删除/ })).toBeDisabled();
  });

  it("marks provider identifier, API key, and base URL as required fields", () => {
    renderWithProviders(
      <ServiceProviderDetail
        provider={baseClaudeProvider}
        onChange={vi.fn()}
        onSave={vi.fn()}
        onActivate={vi.fn()}
        onDelete={vi.fn()}
        onBack={vi.fn()}
      />,
    );

    expect(screen.getByText(/Service Provider Identifier|服务商标识/)).toHaveClass("required");
    expect(screen.getByText(/API Key/)).toHaveClass("required");
    expect(screen.getByText(/Base URL|API 端点/)).toHaveClass("required");
  });

  it("shows the history entry for every provider tool", async () => {
    const user = userEvent.setup();

    for (const tool of ["claude", "codex", "gemini", "opencode"]) {
      const { unmount } = renderWithProviders(
        <ServiceProviderDetail
          provider={{ ...baseClaudeProvider, id: `${tool}-provider`, tool, name: `${tool} Provider` }}
          jsonHistory={[
            {
              timestamp: 1_700_000_000_000,
              action: "upsert",
              snapshot: { ...baseClaudeProvider, tool, api_key: "sk-old", model: "old-model" },
            },
          ]}
          onChange={vi.fn()}
          onSave={vi.fn()}
          onActivate={vi.fn()}
          onDelete={vi.fn()}
          onBack={vi.fn()}
        />,
      );

      await user.click(screen.getByRole("button", { name: /History/ }));
      expect(screen.getAllByText("upsert").length).toBeGreaterThan(0);
      unmount();
    }
  });

  it("shows an empty history state", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <ServiceProviderDetail
        provider={baseClaudeProvider}
        jsonHistory={[]}
        onChange={vi.fn()}
        onSave={vi.fn()}
        onActivate={vi.fn()}
        onDelete={vi.fn()}
        onBack={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("button", { name: /History/ }));
    expect(screen.getAllByText(/No history/).length).toBeGreaterThan(0);
  });

  it("shows field-level diffs and masks secret values", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <ServiceProviderDetail
        provider={{ ...baseClaudeProvider, api_key: "sk-new", model: "new-model" }}
        jsonHistory={[
          {
            timestamp: 1_700_000_000_000,
            action: "upsert",
            snapshot: { ...baseClaudeProvider, api_key: "sk-old", model: "old-model" },
          },
        ]}
        onChange={vi.fn()}
        onSave={vi.fn()}
        onActivate={vi.fn()}
        onDelete={vi.fn()}
        onBack={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("button", { name: /History/ }));
    expect(screen.getByText("api_key")).toBeInTheDocument();
    expect(screen.getAllByText("********").length).toBeGreaterThanOrEqual(2);
    expect(screen.getByText("model")).toBeInTheDocument();
    expect(screen.getByTitle("old-model")).toBeInTheDocument();
    expect(screen.getByTitle("new-model")).toBeInTheDocument();
  });

  it("clicking rollback loads the draft through callback without saving", async () => {
    const user = userEvent.setup();
    const onRollback = vi.fn();
    const onSave = vi.fn();
    const entry = {
      timestamp: 1_700_000_000_000,
      action: "upsert",
      snapshot: { ...baseClaudeProvider, api_key: "sk-old", model: "old-model" },
    };
    renderWithProviders(
      <ServiceProviderDetail
        provider={baseClaudeProvider}
        jsonHistory={[entry]}
        onChange={vi.fn()}
        onSave={onSave}
        onActivate={vi.fn()}
        onDelete={vi.fn()}
        onBack={vi.fn()}
        onRollback={onRollback}
      />,
    );

    await user.click(screen.getByRole("button", { name: /History/ }));
    await user.click(screen.getByRole("button", { name: /Rollback/ }));
    expect(onRollback).toHaveBeenCalledWith(entry);
    expect(onSave).not.toHaveBeenCalled();
  });
});
