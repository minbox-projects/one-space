import { screen, within } from "@testing-library/react";
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
});
