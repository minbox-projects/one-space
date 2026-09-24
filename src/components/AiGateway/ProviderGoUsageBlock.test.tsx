import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import i18n from "@/i18n";
import type { GatewayUpstreamProvider, ProviderGoUsage } from "@/lib/aiGateway";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";
import { ProviderGoUsageBlock } from "./ProviderGoUsageBlock";

const FIXTURE: ProviderGoUsage = {
  usage: {
    rolling: { status: "ok", percent: 25, resetsAt: "2026-09-24T12:00:00Z" },
    weekly: { status: "rate-limited", percent: 65, resetsAt: "2026-09-25T12:00:00Z" },
    monthly: { status: "ok", percent: 120, resetsAt: "2026-10-01T12:00:00Z" },
  },
};

const GO_USAGE_I18N_KEYS = [
  "aiGatewayGoUsageTitle",
  "aiGatewayGoUsageRolling",
  "aiGatewayGoUsageWeekly",
  "aiGatewayGoUsageMonthly",
  "aiGatewayGoUsageReset",
  "aiGatewayGoUsageRefresh",
  "aiGatewayGoUsageRefreshAria",
  "aiGatewayGoUsageLoading",
  "aiGatewayGoUsageError",
  "aiGatewayGoUsageErrorNoKey",
  "aiGatewayGoUsageErrorTimeout",
  "aiGatewayGoUsageErrorHttp",
  "aiGatewayGoUsageErrorInvalid",
  "aiGatewayGoUsageErrorNoSubscription",
  "aiGatewayGoUsageRateLimited",
] as const;

function makeProvider(overrides: Partial<GatewayUpstreamProvider> = {}): GatewayUpstreamProvider {
  return {
    id: "go-provider",
    name: "OpenCode Go",
    base_url: "https://opencode.ai/zen/go",
    api_key: "sk-test",
    default_model: null,
    protocol: "chat_completions",
    mappings: [],
    enabled: true,
    ...overrides,
  } as GatewayUpstreamProvider;
}

function mockUsageResult(result: ProviderGoUsage | Error = FIXTURE) {
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "ai_gateway_provider_go_usage") {
      if (result instanceof Error) throw result;
      return result;
    }
    return undefined;
  });
}

function renderUsageBlock(provider = makeProvider()) {
  return renderWithProviders(<ProviderGoUsageBlock provider={provider} />);
}

describe("ProviderGoUsageBlock", () => {
  beforeEach(async () => {
    resetTauriMocks();
    mockUsageResult();
    await i18n.changeLanguage("en");
  });

  it("renders all usage windows with percentage bars and the rate-limited status", async () => {
    renderUsageBlock();

    const rolling = await screen.findByTestId("ai-gateway-provider-go-usage-rolling-go-provider");
    const weekly = screen.getByTestId("ai-gateway-provider-go-usage-weekly-go-provider");
    const monthly = screen.getByTestId("ai-gateway-provider-go-usage-monthly-go-provider");

    expect(rolling).toHaveTextContent("25%");
    expect(rolling.querySelector("[style*='width']")).toHaveStyle({ width: "25%" });
    expect(weekly).toHaveTextContent("65%");
    expect(weekly).toHaveTextContent(i18n.t("aiGatewayGoUsageRateLimited"));
    expect(monthly).toHaveTextContent("120%");
    expect(monthly.querySelector("[style*='width']")).toHaveStyle({ width: "100%" });
    expect(screen.getByTestId("ai-gateway-provider-go-usage-summary-go-provider")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_provider_go_usage", {
      providerId: "go-provider",
      forceRefresh: false,
    });
  });

  it("requests a forced refresh from the accessible refresh button", async () => {
    const user = userEvent.setup();
    renderUsageBlock();

    await screen.findByTestId("ai-gateway-provider-go-usage-summary-go-provider");
    await user.click(screen.getByTestId("ai-gateway-provider-go-usage-refresh-go-provider"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_provider_go_usage", {
        providerId: "go-provider",
        forceRefresh: true,
      }),
    );
  });

  it("announces loading while the usage request is pending and hides refresh", () => {
    invokeMock.mockImplementation(() => new Promise(() => undefined));
    renderUsageBlock();

    expect(screen.getByTestId("ai-gateway-provider-go-usage-loading-go-provider"))
      .toHaveAttribute("aria-live", "polite");
    expect(screen.queryByTestId("ai-gateway-provider-go-usage-refresh-go-provider"))
      .not.toBeInTheDocument();
  });

  it.each([
    ["no API key configured", "aiGatewayGoUsageErrorNoKey", {}],
    ["request timed out", "aiGatewayGoUsageErrorTimeout", {}],
    ["HTTP 403 EntitlementError: subscription required", "aiGatewayGoUsageErrorNoSubscription", {}],
    ["HTTP 429", "aiGatewayGoUsageErrorHttp", { status: "429" }],
    ["missing usage data", "aiGatewayGoUsageErrorInvalid", {}],
    ["gateway detail", "aiGatewayGoUsageError", { reason: "gateway detail" }],
  ] as const)("localizes usage failure %s", async (reason, key, options) => {
    mockUsageResult(new Error(reason));
    renderUsageBlock();

    expect(await screen.findByTestId("ai-gateway-provider-go-usage-error-go-provider"))
      .toHaveTextContent(i18n.t(key, options));
  });

  it("maps a response missing a required usage window to the invalid response message", async () => {
    mockUsageResult({ usage: { rolling: FIXTURE.usage.rolling } } as unknown as ProviderGoUsage);
    renderUsageBlock();

    expect(await screen.findByTestId("ai-gateway-provider-go-usage-error-go-provider"))
      .toHaveTextContent(i18n.t("aiGatewayGoUsageErrorInvalid"));
  });

  it("defines all Go usage copy keys in both language dictionaries", () => {
    for (const language of ["en", "zh"] as const) {
      for (const key of GO_USAGE_I18N_KEYS) {
        const translation = i18n.getResource(language, "translation", key);
        expect(typeof translation, `${language}:${key}`).toBe("string");
        expect((translation as string).trim(), `${language}:${key}`).not.toBe("");
      }
    }
  });
});
