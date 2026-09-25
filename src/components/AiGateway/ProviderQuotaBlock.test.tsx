import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import i18n from "@/i18n";
import type {
  GatewayUpstreamProvider,
  ProviderQuota,
} from "@/lib/aiGateway";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";
import { ProviderQuotaBlock } from "./ProviderQuotaBlock";

const FIXTURE: ProviderQuota = {
  credits: {
    monthlyCredits: 42.5,
    purchasedCredits: 1.25,
    freeCredits: 0,
    belowThreshold: false,
  },
  windowLimits: {
    limited: true,
    fiveHour: {
      used: 0.5,
      cap: 14,
      exceeded: false,
      resetAt: 1758600000000,
    },
    weekly: {
      used: 11,
      cap: 35,
      exceeded: false,
      resetAt: 1758600000000,
    },
  },
};

const QUOTA_I18N_KEYS = [
  "aiGatewayQuotaTitle",
  "aiGatewayQuotaCredits",
  "aiGatewayQuotaWindow5h",
  "aiGatewayQuotaWindowWeekly",
  "aiGatewayQuotaReset",
  "aiGatewayQuotaRefresh",
  "aiGatewayQuotaRefreshAria",
  "aiGatewayQuotaLoading",
  "aiGatewayQuotaError",
  "aiGatewayQuotaErrorNoKey",
  "aiGatewayQuotaErrorTimeout",
  "aiGatewayQuotaErrorHttp",
  "aiGatewayQuotaErrorInvalid",
  "aiGatewayQuotaLowBalance",
  "aiGatewayQuotaExceeded",
] as const;

function makeProvider(
  overrides: Partial<GatewayUpstreamProvider> = {},
): GatewayUpstreamProvider {
  return {
    id: "quota-provider",
    name: "CommandCode",
    base_url: "https://api.commandcode.ai/provider/v1",
    keys: [
      {
        id: "k1",
        name: "Default",
        value: "sk-test",
        enabled: true,
        auto_marked: false,
        failure_kind: null,
        marked_at: null,
        reason: null,
      },
    ],
    default_model: null,
    protocol: "chat_completions",
    mappings: [],
    enabled: true,
    ...overrides,
  } as unknown as GatewayUpstreamProvider;
}

function mockQuotaResult(result: ProviderQuota | Error = FIXTURE) {
  invokeMock.mockImplementation(async (command: string) => {
    if (command === "ai_gateway_provider_quota") {
      if (result instanceof Error) throw result;
      return result;
    }
    return undefined;
  });
}

function renderQuotaBlock(quotaProvider = makeProvider(), baseNow?: number | Date) {
  return renderWithProviders(
    <ProviderQuotaBlock provider={quotaProvider} baseNow={baseNow} />,
  );
}

describe("ProviderQuotaBlock", () => {
  beforeEach(async () => {
    resetTauriMocks();
    mockQuotaResult();
    await i18n.changeLanguage("en");
  });

  it("AC-001 renders the credit balance, both window remainders, and localized reset time", async () => {
    renderQuotaBlock();

    const credits = await screen.findByTestId(
      "ai-gateway-provider-quota-credits-quota-provider",
    );
    const fiveHour = screen.getByTestId(
      "ai-gateway-provider-quota-window-5h-quota-provider",
    );
    const weekly = screen.getByTestId(
      "ai-gateway-provider-quota-window-weekly-quota-provider",
    );
    const resetTime = new Intl.DateTimeFormat(undefined, {
      month: "numeric",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hour12: false,
    }).format(new Date(1758600000000));

    expect(credits).toHaveTextContent("$43.75");
    expect(fiveHour).toHaveTextContent("$13.50 / $14");
    expect(fiveHour).toHaveTextContent(
      i18n.t("aiGatewayQuotaReset", { time: resetTime }),
    );
    expect(weekly).toHaveTextContent("$24.00 / $35");
    expect(weekly).toHaveTextContent(
      i18n.t("aiGatewayQuotaReset", { time: resetTime }),
    );
    expect(invokeMock).toHaveBeenCalledWith("ai_gateway_provider_quota", {
      providerId: "quota-provider",
      forceRefresh: false,
    });
  });

  it.each([
    ["limited:false", { ...FIXTURE, windowLimits: { limited: false } }],
    ["missing windowLimits", { credits: FIXTURE.credits }],
  ] as const)("AC-005 shows credits only when %s", async (_label, quota) => {
    mockQuotaResult(quota as ProviderQuota);
    renderQuotaBlock();

    expect(
      await screen.findByTestId(
        "ai-gateway-provider-quota-credits-quota-provider",
      ),
    ).toHaveTextContent("$43.75");
    expect(
      screen.queryByTestId(
        "ai-gateway-provider-quota-window-5h-quota-provider",
      ),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId(
        "ai-gateway-provider-quota-window-weekly-quota-provider",
      ),
    ).not.toBeInTheDocument();
  });

  it.each([0, -1])("AC-005 omits a window whose cap is %s", async (cap) => {
    mockQuotaResult({
      ...FIXTURE,
      windowLimits: {
        limited: true,
        fiveHour: { used: 0, cap, exceeded: false },
        weekly: FIXTURE.windowLimits?.weekly,
      },
    });
    renderQuotaBlock();

    expect(
      await screen.findByTestId(
        "ai-gateway-provider-quota-credits-quota-provider",
      ),
    ).toHaveTextContent("$43.75");
    expect(
      screen.queryByTestId(
        "ai-gateway-provider-quota-window-5h-quota-provider",
      ),
    ).not.toBeInTheDocument();
    expect(
      screen.getByTestId(
        "ai-gateway-provider-quota-window-weekly-quota-provider",
      ),
    ).toHaveTextContent("$24.00 / $35");
  });

  it.each([
    ["explicitly exceeded", { used: 2, cap: 14, exceeded: true }],
    ["used above cap", { used: 20, cap: 14, exceeded: false }],
  ] as const)("AC-006 marks a 5-hour window as exceeded when %s", async (_label, fiveHour) => {
    mockQuotaResult({
      ...FIXTURE,
      windowLimits: {
        limited: true,
        fiveHour,
      },
    });
    renderQuotaBlock();

    const line = await screen.findByTestId(
      "ai-gateway-provider-quota-window-5h-quota-provider",
    );
    expect(line).toHaveTextContent("$0.00 / $14");
    expect(line).not.toHaveTextContent(i18n.t("aiGatewayQuotaExceeded"));
  });

  it("AC-007 shows the localized low-balance warning", async () => {
    mockQuotaResult({
      ...FIXTURE,
      credits: { ...FIXTURE.credits, belowThreshold: true },
    });
    renderQuotaBlock();

    const credits = await screen.findByTestId(
      "ai-gateway-provider-quota-credits-quota-provider",
    );
    expect(credits).toHaveTextContent(i18n.t("aiGatewayQuotaLowBalance"));
  });

  it.each([null, "not-a-date"] as const)(
    "REQ-003 omits reset text for resetAt=%s while retaining the window amounts",
    async (resetAt) => {
      mockQuotaResult({
        ...FIXTURE,
        windowLimits: {
          limited: true,
          fiveHour: { ...FIXTURE.windowLimits!.fiveHour!, resetAt },
        },
      });
      renderQuotaBlock();

      const line = await screen.findByTestId(
        "ai-gateway-provider-quota-window-5h-quota-provider",
      );
      expect(line).toHaveTextContent("$13.50 / $14");
      expect(line).not.toHaveTextContent("1/1 08:00:00");
      expect(line).not.toHaveTextContent(
        i18n.t("aiGatewayQuotaReset", { time: "" }),
      );
    },
  );

  it.each([0, "0", -1, "1970-01-01T00:00:00Z"] as const)(
    "formats zero resetAt=%s as baseNow+5h for 5h window and baseNow+7d for weekly window",
    async (resetAt) => {
      const fixedBaseNow = new Date("2026-09-24T12:00:00.000Z").getTime();
      mockQuotaResult({
        ...FIXTURE,
        windowLimits: {
          limited: true,
          fiveHour: { ...FIXTURE.windowLimits!.fiveHour!, resetAt },
          weekly: { ...FIXTURE.windowLimits!.weekly!, resetAt },
        },
      });
      renderQuotaBlock(makeProvider(), fixedBaseNow);

      const fiveHour = await screen.findByTestId(
        "ai-gateway-provider-quota-window-5h-quota-provider",
      );
      const weekly = screen.getByTestId(
        "ai-gateway-provider-quota-window-weekly-quota-provider",
      );

      const expectedFiveHourTime = new Intl.DateTimeFormat(undefined, {
        month: "numeric",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
        hour12: false,
      }).format(new Date(fixedBaseNow + 5 * 3600 * 1000));

      const expectedWeeklyTime = new Intl.DateTimeFormat(undefined, {
        month: "numeric",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
        hour12: false,
      }).format(new Date(fixedBaseNow + 7 * 24 * 3600 * 1000));

      expect(fiveHour).toHaveTextContent(
        i18n.t("aiGatewayQuotaReset", { time: expectedFiveHourTime }),
      );
      expect(weekly).toHaveTextContent(
        i18n.t("aiGatewayQuotaReset", { time: expectedWeeklyTime }),
      );
      expect(fiveHour).not.toHaveTextContent("1/1 08:00:00");
      expect(weekly).not.toHaveTextContent("1/1 08:00:00");
    },
  );

  it("formats second-level timestamp correctly without showing 1970", async () => {
    mockQuotaResult({
      ...FIXTURE,
      windowLimits: {
        limited: true,
        fiveHour: { ...FIXTURE.windowLimits!.fiveHour!, resetAt: 1758600000 },
      },
    });
    renderQuotaBlock();

    const line = await screen.findByTestId(
      "ai-gateway-provider-quota-window-5h-quota-provider",
    );
    const expectedTime = new Intl.DateTimeFormat(undefined, {
      month: "numeric",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hour12: false,
    }).format(new Date(1758600000000));
    expect(line).toHaveTextContent(
      i18n.t("aiGatewayQuotaReset", { time: expectedTime }),
    );
  });

  it("AC-008 keeps the quota container and localized error visible after a rejected query", async () => {
    mockQuotaResult(new Error("HTTP 429"));
    renderQuotaBlock();

    expect(
      await screen.findByTestId(
        "ai-gateway-provider-quota-error-quota-provider",
      ),
    ).toHaveTextContent(i18n.t("aiGatewayQuotaErrorHttp", { status: "429" }));
    expect(
      screen.getByTestId("ai-gateway-provider-quota-quota-provider"),
    ).toBeInTheDocument();
  });

  it.each([
    [
      "no API key configured for this provider",
      "aiGatewayQuotaErrorNoKey",
      {},
    ],
    [
      "timed out fetching https://api.commandcode.ai/alpha/billing/credits",
      "aiGatewayQuotaErrorTimeout",
      {},
    ],
    [
      "invalid quota response: expected value at line 1",
      "aiGatewayQuotaErrorInvalid",
      {},
    ],
    [
      "unexpected gateway response detail",
      "aiGatewayQuotaError",
      { reason: "unexpected gateway response detail" },
    ],
  ] as const)("AC-008 localizes quota failure reason %s", async (reason, key, options) => {
    mockQuotaResult(new Error(reason));
    renderQuotaBlock();

    expect(
      await screen.findByTestId(
        "ai-gateway-provider-quota-error-quota-provider",
      ),
    ).toHaveTextContent(i18n.t(key, options));
  });

  it("AC-004 refresh requests the provider quota with forceRefresh=true", async () => {
    const user = userEvent.setup();
    renderQuotaBlock();

    await screen.findByTestId(
      "ai-gateway-provider-quota-credits-quota-provider",
    );
    await user.click(
      screen.getByTestId(
        "ai-gateway-provider-quota-refresh-quota-provider",
      ),
    );

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("ai_gateway_provider_quota", {
        providerId: "quota-provider",
        forceRefresh: true,
      }),
    );
  });

  it("renders Antigravity-style progress bars with dynamic color scaling", async () => {
    // 5h window: used 0.5 / cap 14 -> remaining 13.5 (96%) -> bg-emerald-500
    // weekly window: used 11 / cap 35 -> remaining 24 (69%) -> bg-emerald-500
    renderQuotaBlock();

    const fiveHour = await screen.findByTestId(
      "ai-gateway-provider-quota-window-5h-quota-provider",
    );
    const fiveHourBar = fiveHour.querySelector(".bg-emerald-500");
    expect(fiveHourBar).toBeInTheDocument();
    expect(fiveHourBar).toHaveStyle({ width: "96%" });

    const weekly = screen.getByTestId(
      "ai-gateway-provider-quota-window-weekly-quota-provider",
    );
    const weeklyBar = weekly.querySelector(".bg-emerald-500");
    expect(weeklyBar).toBeInTheDocument();
    expect(weeklyBar).toHaveStyle({ width: "69%" });
  });

  it("renders amber progress bar for medium remaining and destructive for low or exceeded", async () => {
    mockQuotaResult({
      ...FIXTURE,
      windowLimits: {
        limited: true,
        // used 8 / cap 10 -> remaining 2 (20% boundary / <50%) -> amber
        fiveHour: { used: 7, cap: 10, exceeded: false },
        // used 9.5 / cap 10 -> remaining 0.5 (5% < 20%) -> destructive
        weekly: { used: 9.5, cap: 10, exceeded: false },
      },
    });
    renderQuotaBlock();

    const fiveHour = await screen.findByTestId(
      "ai-gateway-provider-quota-window-5h-quota-provider",
    );
    const fiveHourBar = fiveHour.querySelector(".bg-amber-500");
    expect(fiveHourBar).toBeInTheDocument();
    expect(fiveHourBar).toHaveStyle({ width: "30%" });

    const weekly = screen.getByTestId(
      "ai-gateway-provider-quota-window-weekly-quota-provider",
    );
    const weeklyBar = weekly.querySelector(".bg-destructive");
    expect(weeklyBar).toBeInTheDocument();
    expect(weeklyBar).toHaveStyle({ width: "5%" });
  });

  it("AC-011 defines every quota copy key in both language dictionaries", () => {
    for (const language of ["en", "zh"] as const) {
      for (const key of QUOTA_I18N_KEYS) {
        const translation = i18n.getResource(language, "translation", key);
        expect(typeof translation, `${language}:${key}`).toBe("string");
        expect((translation as string).trim(), `${language}:${key}`).not.toBe("");
      }
    }
  });
});
