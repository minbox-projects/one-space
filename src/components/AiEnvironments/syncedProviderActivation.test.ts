import { describe, expect, it } from "vitest";
import { buildSyncedProviderActivationPayload, getMissingRequiredProviderFields } from "@/components/AiEnvironments";

describe("buildSyncedProviderActivationPayload", () => {
  it("uses a UUID internal id and preserves OpenCode provider_key", () => {
    const targetId = "11111111-1111-4111-8111-111111111111";
    const result = buildSyncedProviderActivationPayload(
      "MacBook Pro",
      {
        id: "remote-opencode-id",
        name: "Remote OpenCode",
        tool: "opencode",
        api_key: "  sk-open  ",
        base_url: "https://opencode.example.com/v1",
        model: "open-model",
        provider_key: "remote_provider_key",
        is_enabled: false,
      },
      targetId,
    );

    expect(result).not.toBeNull();
    expect(result?.targetId).toBe(targetId);
    expect(result?.targetTool).toBe("opencode");
    expect(result?.payload.id).toBe(targetId);
    expect(result?.payload.id).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i,
    );
    expect(result?.payload.id).not.toBe("remote-opencode-id");
    expect(result?.payload.id).not.toMatch(/^synced-/);
    expect(result?.payload.provider_key).toBe("remote_provider_key");
    expect(result?.payload.api_key).toBe("sk-open");
    expect(result?.payload.is_enabled).toBe(false);
  });

  it("generates fallback OpenCode provider_key without using it as internal id", () => {
    const targetId = "22222222-2222-4222-8222-222222222222";
    const result = buildSyncedProviderActivationPayload(
      "Remote Device",
      {
        id: "remote-opencode-id",
        name: "Remote OpenCode",
        tool: "opencode",
        api_key: "sk-open",
      },
      targetId,
      () => 12345,
    );

    expect(result?.payload.id).toBe(targetId);
    expect(result?.payload.provider_key).toBe("syncedremotedevice");
  });
});

describe("getMissingRequiredProviderFields", () => {
  it("requires API key, base URL, and code for managed tool providers", () => {
    expect(
      getMissingRequiredProviderFields({
        tool: "claude",
        api_key: "  ",
        base_url: "",
        code: "",
      }),
    ).toEqual(["api_key", "base_url", "code"]);

    expect(
      getMissingRequiredProviderFields({
        tool: "claude",
        api_key: "sk-test",
        base_url: "https://api.example.com",
        code: "work",
      }),
    ).toEqual([]);
  });

  it("requires provider_key instead of code for OpenCode providers", () => {
    expect(
      getMissingRequiredProviderFields({
        tool: "opencode",
        api_key: "sk-test",
        base_url: "https://api.example.com",
        code: "ignored",
        provider_key: "",
      }),
    ).toEqual(["provider_key"]);

    expect(
      getMissingRequiredProviderFields({
        tool: "opencode",
        api_key: "sk-test",
        base_url: "https://api.example.com",
        provider_key: "customprovider",
      }),
    ).toEqual([]);
  });
});
