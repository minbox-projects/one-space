import { describe, expect, it } from "vitest";
import {
  applyProviderPresetToDraft,
  createProviderCopyDraft,
  type ServiceProviderPresetRecord,
} from "./providerPresets";

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
    claude_default_model: "claude-sonnet-4-5",
    claude_reasoning_effort: "high",
    claude_model_mappings: [
      { family: "haiku", display_name: "Haiku", upstream_model: "claude-haiku-4-5" },
      { family: "sonnet", display_name: "Sonnet", upstream_model: "claude-sonnet-4-5", supports_1m: true },
    ],
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

  it("copies Claude-only template fields when creating Claude providers", () => {
    const next = applyProviderPresetToDraft(draft("claude"), preset, "claude");

    expect(next.claude_default_model).toBe("claude-sonnet-4-5");
    expect(next.claude_reasoning_effort).toBe("high");
    expect(next.claude_model_mappings).toEqual([
      { family: "haiku", display_name: "Haiku", upstream_model: "claude-haiku-4-5", supports_1m: false },
      { family: "sonnet", display_name: "Sonnet", upstream_model: "claude-sonnet-4-5", supports_1m: true },
    ]);
  });

  it("does not copy Claude-only template fields to other tools", () => {
    const codex = applyProviderPresetToDraft(draft("codex"), preset, "codex");
    const gemini = applyProviderPresetToDraft(draft("gemini"), preset, "gemini");
    const opencode = applyProviderPresetToDraft(draft("opencode"), preset, "opencode");

    expect(codex.claude_default_model).toBeUndefined();
    expect(codex.claude_reasoning_effort).toBeUndefined();
    expect(codex.claude_model_mappings).toBeUndefined();
    expect(gemini.claude_default_model).toBeUndefined();
    expect(opencode.claude_model_mappings).toBeUndefined();
  });
});

describe("createProviderCopyDraft", () => {
  it("recursively removes credentials without mutating the source", () => {
    const source = {
      id: "saved-provider",
      name: "Acme",
      tool: "opencode",
      api_key: "top-secret",
      provider_key: "saved_key",
      code: "saved-code",
      options: {
        apiKey: "nested-secret",
        baseURL: "https://api.example.com/v1",
        nested: {
          AccessToken: "access-token",
          requestTimeout: 30000,
        },
      },
      tool_config: {
        headers: [
          { Authorization: "Bearer secret", "X-Region": "eu-west-1" },
          { CLIENTSECRET: "client-secret", retry: 2 },
        ],
        unknown: { passwordHint: "also-sensitive", transport: "fetch" },
      },
      models: {
        "acme-chat": {
          name: "Acme Chat",
          options: { reasoning: true, sessionToken: "model-secret" },
        },
      },
    };
    const original = structuredClone(source);

    const draft = createProviderCopyDraft(source);

    expect(source).toEqual(original);
    expect(draft.tool).toBe("opencode");
    expect(draft.options).toEqual({
      baseURL: "https://api.example.com/v1",
      nested: { requestTimeout: 30000 },
    });
    expect(draft.tool_config).toEqual({
      headers: [{ "X-Region": "eu-west-1" }, { retry: 2 }],
      unknown: { transport: "fetch" },
    });
    expect(draft.models).toEqual({
      "acme-chat": {
        name: "Acme Chat",
        options: { reasoning: true },
      },
    });
    expect(JSON.stringify(draft)).not.toContain("top-secret");
    expect(JSON.stringify(draft)).not.toContain("nested-secret");
    expect(JSON.stringify(draft)).not.toContain("access-token");
    expect(JSON.stringify(draft)).not.toContain("client-secret");
    expect(JSON.stringify(draft)).not.toContain("model-secret");
  });

  it("refreshes identity and removes saved-instance state", () => {
    const source = {
      id: "saved-provider",
      name: "Acme",
      tool: "claude",
      api_key: "secret",
      provider_key: "saved_key",
      code: "saved-code",
      is_enabled: true,
      is_active: true,
      env_managed: true,
      favorite_at: 123,
      history: [{ action: "saved" }],
      fetched_models: ["cached-model"],
      created_at: 10,
      updated_at: 20,
      base_url: "https://api.example.com",
      model: "acme-chat",
      icon: "builtin:acme",
      tool_config: { retries: 3 },
      unknown: { nested: [{ value: 1 }] },
    };

    const draft = createProviderCopyDraft(source);

    expect(draft.id).not.toBe(source.id);
    expect(draft.provider_key).not.toBe(source.provider_key);
    expect(draft.code).not.toBe(source.code);
    expect(new Set([draft.id, draft.provider_key, draft.code]).size).toBe(3);
    expect(draft.name).toBe("Acme 副本");
    draft.name = "Editable name";
    expect(draft.name).toBe("Editable name");
    expect(draft).not.toHaveProperty("api_key");
    expect(draft).not.toHaveProperty("is_enabled");
    expect(draft).not.toHaveProperty("is_active");
    expect(draft).not.toHaveProperty("env_managed");
    expect(draft).not.toHaveProperty("favorite_at");
    expect(draft).not.toHaveProperty("history");
    expect(draft).not.toHaveProperty("fetched_models");
    expect(draft).not.toHaveProperty("created_at");
    expect(draft).not.toHaveProperty("updated_at");
    expect(draft).toMatchObject({
      tool: "claude",
      base_url: "https://api.example.com",
      model: "acme-chat",
      icon: "builtin:acme",
      tool_config: { retries: 3 },
      unknown: { nested: [{ value: 1 }] },
    });
  });
});
