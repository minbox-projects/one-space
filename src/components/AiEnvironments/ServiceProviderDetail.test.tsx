import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
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

const openCodeProvider = {
  id: "opencode-provider-1",
  name: "OpenCode Provider",
  tool: "opencode",
  api_key: "key",
  base_url: "https://api.example.com/v1",
  provider_key: "ExampleProvider",
  model: "legacy-primary",
  opencode_default_model: "legacy-default",
  opencode_default_agent: "legacy-agent",
  opencode_sessions_dir: "/tmp/legacy",
  small_model: "legacy-small",
  timeout: 30000,
  share_mode: "manual",
};

const openCodeModelForm = {
  models: [{
    id: "model-a",
    name: "Model A",
    cost: { enabled: false, input: "", output: "", cacheRead: "", cacheWrite: "" },
    limit: { enabled: false, context: "", output: "" },
    options: [],
    variants: [],
  }],
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

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

  it("allows ASCII digits in OpenCode provider identifiers while filtering other characters", () => {
    const onChange = vi.fn();
    renderWithProviders(
      <ServiceProviderDetail
        provider={{ ...baseClaudeProvider, tool: "opencode", provider_key: "" }}
        onChange={onChange}
        onSave={vi.fn()}
        onActivate={vi.fn()}
        onDelete={vi.fn()}
        onBack={vi.fn()}
      />,
    );

    const identifierField = screen.getByText(/Service Provider Identifier|服务商标识/).closest(".field");
    fireEvent.change(identifierField?.querySelector("input") as HTMLInputElement, {
      target: { value: "Provider-42_!" },
    });

    expect(onChange).toHaveBeenCalledWith({ provider_key: "Provider42" });
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

  it("shows field-level diffs with complete fixed test values", async () => {
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
    expect(screen.getByTitle("sk-old")).toBeInTheDocument();
    expect(screen.getByTitle("sk-new")).toBeInTheDocument();
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

describe("ServiceProviderDetail OpenCode model form", () => {
  const openCodeDetail = (overrides: Record<string, unknown> = {}) => (
    <ServiceProviderDetail
      provider={openCodeProvider}
      onChange={vi.fn()}
      onSave={vi.fn()}
      onActivate={vi.fn()}
      onDelete={vi.fn()}
      onBack={vi.fn()}
      jsonMode="opencode"
      jsonValue={JSON.stringify({ models: { "model-a": { name: "Model A" } } }, null, 2)}
      openCodeModelForm={openCodeModelForm}
      onOpenCodeModelFormChange={vi.fn()}
      {...overrides}
    />
  );
  const renderOpenCode = (overrides: Record<string, unknown> = {}) => renderWithProviders(openCodeDetail(overrides));

  it("removes the legacy Primary Model and six OpenCode-specific fields", () => {
    renderOpenCode();

    expect(screen.queryByText(/Primary Model|主模型/)).not.toBeInTheDocument();
    for (const label of [
      /Default Model|默认模型/,
      /Default Agent|默认代理/,
      /Sessions Directory|会话目录/,
      /Small Model|小模型/,
      /Request Timeout|请求超时/,
      /Share Mode|共享模式/,
    ]) {
      expect(screen.queryByText(label)).not.toBeInTheDocument();
    }
    expect(screen.queryByText(/Tool Specific Config|工具特定配置/)).not.toBeInTheDocument();
  });

  it("copies the complete visible API key by keyboard and exposes the success label", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    renderOpenCode({ provider: { ...openCodeProvider, api_key: "complete-runtime-key" } });

    expect(screen.getByDisplayValue("complete-runtime-key")).toHaveAttribute("type", "text");
    const copyButton = screen.getByRole("button", { name: /Copy API Key|复制 API Key/ });
    copyButton.focus();
    await user.keyboard("{Enter}");

    expect(writeText).toHaveBeenCalledWith("complete-runtime-key");
    expect(await screen.findByRole("button", { name: /API Key copied|API Key 已复制/ })).toBeInTheDocument();
  });

  it("does not report clipboard failures as success and allows retry", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn()
      .mockRejectedValueOnce(new Error("permission denied"))
      .mockResolvedValueOnce(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    renderOpenCode({ provider: { ...openCodeProvider, api_key: "retry-key" } });

    await user.click(screen.getByRole("button", { name: /Copy API Key|复制 API Key/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent(/Failed to copy API Key|复制 API Key 失败/);
    expect(screen.queryByRole("button", { name: /API Key copied|API Key 已复制/ })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Copy API Key|复制 API Key/ }));
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(2));
    expect(await screen.findByRole("button", { name: /API Key copied|API Key 已复制/ })).toBeInTheDocument();
  });

  it("ignores a completed copy after the API key changes", async () => {
    const firstCopy = deferred<void>();
    const writeText = vi.fn().mockReturnValue(firstCopy.promise);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    const view = renderOpenCode({ provider: { ...openCodeProvider, api_key: "old-copy-key" } });

    fireEvent.click(screen.getByRole("button", { name: /Copy API Key|复制 API Key/ }));
    view.rerender(openCodeDetail({ provider: { ...openCodeProvider, api_key: "new-copy-key" } }));
    await act(async () => {
      firstCopy.resolve();
      await firstCopy.promise;
    });

    expect(writeText).toHaveBeenCalledWith("old-copy-key");
    expect(screen.getByRole("button", { name: /Copy API Key|复制 API Key/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /API Key copied|API Key 已复制/ })).not.toBeInTheDocument();
  });

  it("keeps the latest copy result when two requests finish out of order", async () => {
    const firstCopy = deferred<void>();
    const secondCopy = deferred<void>();
    const writeText = vi.fn()
      .mockReturnValueOnce(firstCopy.promise)
      .mockReturnValueOnce(secondCopy.promise);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    renderOpenCode({ provider: { ...openCodeProvider, api_key: "race-key" } });

    const copyButton = screen.getByRole("button", { name: /Copy API Key|复制 API Key/ });
    fireEvent.click(copyButton);
    fireEvent.click(copyButton);
    await act(async () => {
      secondCopy.resolve();
      await secondCopy.promise;
    });
    expect(screen.getByRole("button", { name: /API Key copied|API Key 已复制/ })).toBeInTheDocument();

    await act(async () => {
      firstCopy.reject(new Error("late failure"));
      await firstCopy.promise.catch(() => undefined);
    });
    expect(screen.getByRole("button", { name: /API Key copied|API Key 已复制/ })).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("emits dynamic model, cost, limit, option, and variant edits", async () => {
    const user = userEvent.setup();
    const onFormChange = vi.fn();
    renderOpenCode({ onOpenCodeModelFormChange: onFormChange });

    fireEvent.change(screen.getByRole("textbox", { name: /Model ID|模型 ID/ }), {
      target: { value: "model-b" },
    });
    expect(onFormChange).toHaveBeenLastCalledWith(expect.objectContaining({
      models: [expect.objectContaining({ id: "model-b" })],
    }));

    await user.click(screen.getByRole("button", { name: /Add option|添加选项/ }));
    expect(onFormChange).toHaveBeenCalledWith(expect.objectContaining({
      models: [expect.objectContaining({ options: [expect.objectContaining({ valueType: "json", custom: true })] })],
    }));
    await user.click(screen.getByRole("button", { name: /Add variant|添加变体/ }));
    expect(onFormChange).toHaveBeenCalledWith(expect.objectContaining({
      models: [expect.objectContaining({ variants: [expect.objectContaining({ name: "", options: [] })] })],
    }));

    await user.click(screen.getByRole("checkbox", { name: /Cost per 1M tokens|每 100 万/ }));
    await user.click(screen.getByRole("checkbox", { name: /Limits|限制/ }));
    expect(onFormChange).toHaveBeenCalledWith(expect.objectContaining({
      models: [expect.objectContaining({ limit: expect.objectContaining({ enabled: true }) })],
    }));
  });

  it("shows validation boundaries and disables Save for invalid model state", () => {
    renderOpenCode({
      openCodeModelErrors: [
        { code: "duplicate", path: "models.0.id", message: "Model ID must be unique", modelIndex: 0 },
        { code: "invalid_number", path: "models.0.cost.input", message: "Cost must be non-negative", modelIndex: 0 },
        { code: "invalid_number", path: "models.0.limit.output", message: "Limit must be positive", modelIndex: 0 },
      ],
    });

    expect(screen.getByText("Model ID must be unique")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Save|保存/ })).toBeDisabled();
  });

  it("freezes all model writes and Save while JSON is invalid", () => {
    renderOpenCode({ jsonError: "Invalid OpenCode JSON", openCodeModelFrozen: true });

    expect(screen.getAllByText("Invalid OpenCode JSON")).toHaveLength(2);
    expect(screen.getByRole("textbox", { name: /Model ID|模型 ID/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /Add model|添加模型/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /Save|保存/ })).toBeDisabled();
  });
});
