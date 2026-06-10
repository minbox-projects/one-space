import { describe, expect, it } from "vitest";
import { applyProviderPresetToDraft, type ServiceProviderPresetRecord } from "./providerPresets";

const preset: ServiceProviderPresetRecord = {
  id: "vendor",
  name: "Vendor",
  icon: "builtin:deepseek",
  description: "Vendor preset",
  endpoints: {
    openai_base_url: "https://openai.vendor.example/v1",
    anthropic_base_url: "https://anthropic.vendor.example",
  },
  template: {
    model: "vendor-model",
    api_key: "should-not-copy",
    code: "should-not-copy",
    provider_key: "should-not-copy",
    is_enabled: false,
    fetched_models: ["cached"],
  },
  created_at: 1,
  updated_at: 1,
};

const draft = (tool: string, extra: Record<string, any> = {}) => ({
  id: `${tool}-id`,
  name: "Draft",
  tool,
  api_key: "sk-existing",
  base_url: "",
  model: "",
  code: tool === "opencode" ? undefined : `${tool}-code`,
  provider_key: tool === "opencode" ? "opencode_key" : undefined,
  is_enabled: true,
  ...extra,
});

describe("applyProviderPresetToDraft", () => {
  it("uses Anthropic URL for native Claude drafts", () => {
    const next = applyProviderPresetToDraft(
      draft("claude", {
        claude_api_format: "anthropic_messages",
        claude_connection_mode: "native_anthropic",
      }),
      preset,
      "claude",
    );

    expect(next.base_url).toBe("https://anthropic.vendor.example");
    expect(next.api_key).toBe("sk-existing");
    expect(next.code).toBe("claude-code");
  });

  it("uses OpenAI URL for Claude protocol router and OpenAI formats", () => {
    expect(
      applyProviderPresetToDraft(
        draft("claude", {
          claude_connection_mode: "protocol_router",
          claude_api_format: "anthropic_messages",
        }),
        preset,
        "claude",
      ).base_url,
    ).toBe("https://openai.vendor.example/v1");

    expect(
      applyProviderPresetToDraft(
        draft("claude", {
          claude_connection_mode: "native_anthropic",
          claude_api_format: "open_ai_responses",
        }),
        preset,
        "claude",
      ).base_url,
    ).toBe("https://openai.vendor.example/v1");
  });

  it("uses OpenAI URL for Codex and OpenCode while preserving identifiers", () => {
    const codex = applyProviderPresetToDraft(draft("codex"), preset, "codex");
    expect(codex.base_url).toBe("https://openai.vendor.example/v1");
    expect(codex.code).toBe("codex-code");
    expect(codex.api_key).toBe("sk-existing");

    const opencode = applyProviderPresetToDraft(draft("opencode"), preset, "opencode");
    expect(opencode.base_url).toBe("https://openai.vendor.example/v1");
    expect(opencode.provider_key).toBe("opencode_key");
    expect(opencode.options?.baseURL).toBe("https://openai.vendor.example/v1");
  });

  it("does not apply preset URLs to Gemini drafts", () => {
    const next = applyProviderPresetToDraft(
      draft("gemini", { base_url: "existing" }),
      { ...preset, endpoints: { openai_base_url: "https://openai.vendor.example/v1" } },
      "gemini",
    );
    expect(next.base_url).toBe("existing");
  });

  it("does not copy sensitive or instance template fields", () => {
    const next = applyProviderPresetToDraft(draft("codex"), preset, "codex");
    expect(next.model).toBe("vendor-model");
    expect(next.api_key).toBe("sk-existing");
    expect(next.code).toBe("codex-code");
    expect(next.is_enabled).toBe(true);
    expect(next.fetched_models).toBeUndefined();
  });
});
