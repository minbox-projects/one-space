import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "@/test/mocks/render";
import i18n from "@/i18n";
import { UpstreamProviderList } from "./UpstreamProviderList";
import type { GatewayModelMapping, GatewayUpstreamProvider } from "@/lib/aiGateway";

afterEach(() => {
  cleanup();
});

function makeProvider(overrides: Partial<GatewayUpstreamProvider> = {}): GatewayUpstreamProvider {
  return {
    id: "p1",
    name: "Provider 1",
    base_url: "https://api.example.com",
    keys: [],
    default_model: "gpt-4o",
    protocol: "chat_completions",
    mappings: [
      {
        local_model: "m1",
        upstream_model: "u1",
        enabled: true,
        auto_disabled: true,
        disabled_reason: "probe failed",
        disabled_at: 1700000000000,
      },
    ],
    enabled: true,
    ...overrides,
  } as GatewayUpstreamProvider;
}

describe("Auto-disabled models dialog entry", () => {
  it("shows zero count and opens the empty state without providers", async () => {
    await i18n.changeLanguage("en");
    renderWithProviders(
      <UpstreamProviderList
        providers={[]}
        selectedProviderId={null}
        busy={false}
        onSelect={vi.fn()}
        onToggleEnabled={vi.fn()}
        onAdd={vi.fn()}
        onManageTemplates={vi.fn()}
      />,
    );

    const button = screen.getByTestId("ai-gateway-auto-disabled-models");
    expect(button).toHaveTextContent("0");
    await userEvent.click(button);
    expect(await screen.findByTestId("ai-gateway-auto-disabled-models-dialog")).toBeInTheDocument();
    expect(screen.getByText(/no/i)).toBeInTheDocument();
  });

  it("counts distinct local model ids and opens grouped sources", async () => {
    await i18n.changeLanguage("en");
    const provider = makeProvider({
      mappings: [
        {
          local_model: "m1",
          upstream_model: "u1",
          enabled: true,
          auto_disabled: true,
          disabled_reason: "probe failed",
          disabled_at: 1700000000000,
        },
        {
          local_model: "m2",
          upstream_model: "u2",
          enabled: false,
          auto_disabled: true,
          disabled_reason: null,
          disabled_at: null,
        },
      ],
    });
    renderWithProviders(
      <UpstreamProviderList
        providers={[provider]}
        selectedProviderId={null}
        busy={false}
        onSelect={vi.fn()}
        onToggleEnabled={vi.fn()}
        onAdd={vi.fn()}
        onManageTemplates={vi.fn()}
      />,
    );

    const button = screen.getByTestId("ai-gateway-auto-disabled-models");
    expect(button).toHaveTextContent("2");
    await userEvent.click(button);
    const dialog = await screen.findByTestId("ai-gateway-auto-disabled-models-dialog");
    expect(within(dialog).getByText("m1")).toBeInTheDocument();
    expect(within(dialog).getByText("m2")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: /re-?enable/i })).toBeInTheDocument();
  });

  it("keeps provider and model identity distinct for equal display names", async () => {
    await i18n.changeLanguage("en");
    const p1 = makeProvider({
      id: "p-a",
      name: "Same Name",
      mappings: [
        {
          local_model: "Model",
          upstream_model: "up",
          enabled: true,
          auto_disabled: true,
          disabled_reason: "reason",
          disabled_at: 100,
        },
      ],
    });
    const p2 = makeProvider({
      id: "p-b",
      name: "Same Name",
      mappings: [
        {
          local_model: "model",
          upstream_model: "up",
          enabled: true,
          auto_disabled: true,
          disabled_reason: "reason",
          disabled_at: 200,
        },
      ],
    });
    renderWithProviders(
      <UpstreamProviderList
        providers={[p1, p2]}
        selectedProviderId={null}
        busy={false}
        onSelect={vi.fn()}
        onToggleEnabled={vi.fn()}
        onAdd={vi.fn()}
        onManageTemplates={vi.fn()}
      />,
    );
    await userEvent.click(screen.getByTestId("ai-gateway-auto-disabled-models"));
    const dialog = await screen.findByTestId("ai-gateway-auto-disabled-models-dialog");
    expect(within(dialog).getAllByText(/Same Name/).length).toBeGreaterThanOrEqual(2);
  });

  it("searches models and sources with trimmed case-insensitive matching", async () => {
    await i18n.changeLanguage("en");
    renderWithProviders(
      <UpstreamProviderList
        providers={[makeProvider()]}
        selectedProviderId={null}
        busy={false}
        onSelect={vi.fn()}
        onToggleEnabled={vi.fn()}
        onAdd={vi.fn()}
        onManageTemplates={vi.fn()}
      />,
    );
    await userEvent.click(screen.getByTestId("ai-gateway-auto-disabled-models"));
    const dialog = await screen.findByTestId("ai-gateway-auto-disabled-models-dialog");
    const input = within(dialog).getByRole("searchbox");
    await userEvent.type(input, "  M1  ");
    expect(within(dialog).getByText("m1")).toBeInTheDocument();
    await userEvent.clear(input);
    await userEvent.type(input, "nope");
    expect(within(dialog).getByText(/no match/i)).toBeInTheDocument();
  });

  it("guards repeated recovery submissions and preserves source on rejection", async () => {
    await i18n.changeLanguage("en");
    const onReenableModel = vi.fn(() => new Promise<boolean>(() => {}));
    renderWithProviders(
      <UpstreamProviderList
        providers={[makeProvider()]}
        selectedProviderId={null}
        busy={false}
        onSelect={vi.fn()}
        onToggleEnabled={vi.fn()}
        onAdd={vi.fn()}
        onManageTemplates={vi.fn()}
        onReenableModel={onReenableModel}
      />,
    );
    await userEvent.click(screen.getByTestId("ai-gateway-auto-disabled-models"));
    const dialog = await screen.findByTestId("ai-gateway-auto-disabled-models-dialog");
    const action = within(dialog).getByRole("button", { name: /re-?enable/i });
    await userEvent.click(action);
    await userEvent.click(action);
    expect(onReenableModel).toHaveBeenCalledTimes(1);
    expect(within(dialog).getByText("m1")).toBeInTheDocument();
  });
});

function autoMapping(
  overrides: {
    local_model?: string;
    upstream_model?: string;
    enabled?: boolean;
    auto_disabled?: boolean | undefined;
    disabled_reason?: string | null;
    disabled_at?: number | null;
    protocol?: "chat_completions" | "responses" | "anthropic_messages";
  } = {},
): GatewayModelMapping {
  return {
    local_model: "m1",
    upstream_model: "u1",
    enabled: true,
    auto_disabled: true,
    disabled_reason: "probe failed",
    disabled_at: 1_700_000_000,
    ...overrides,
  };
}

function renderListWith(
  providers: GatewayUpstreamProvider[],
  onReenableModel?: (
    providerId: string,
    localModel: string,
    upstreamModel: string,
  ) => boolean | void | Promise<boolean>,
) {
  return renderWithProviders(
    <UpstreamProviderList
      providers={providers}
      selectedProviderId={null}
      busy={false}
      onSelect={vi.fn()}
      onToggleEnabled={vi.fn()}
      onAdd={vi.fn()}
      onManageTemplates={vi.fn()}
      onReenableModel={onReenableModel}
    />,
  );
}

async function openDialog() {
  await userEvent.click(screen.getByTestId("ai-gateway-auto-disabled-models"));
  return screen.findByTestId("ai-gateway-auto-disabled-models-dialog");
}

function sourceRows(dialog: HTMLElement, pattern: RegExp): HTMLElement[] {
  return within(dialog).getAllByLabelText(pattern) as HTMLElement[];
}

describe("Auto-disabled dialog identities and metadata", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("merges duplicate recovery identities into one action with the protocol union, newest non-null metadata, time tie and manual constituent", async () => {
    const provider = makeProvider({
      id: "p-dup",
      name: "Dup Provider",
      mappings: [
        autoMapping({
          local_model: "gpt-4o",
          upstream_model: "u1",
          protocol: "responses",
          disabled_reason: "older reason",
          disabled_at: 1_000,
        }),
        autoMapping({
          local_model: "gpt-4o",
          upstream_model: "u1",
          enabled: false,
          protocol: "anthropic_messages",
          disabled_reason: "newest reason",
          disabled_at: 3_000,
        }),
        autoMapping({
          local_model: "gpt-4o",
          upstream_model: "u1",
          protocol: "chat_completions",
          disabled_reason: "tie earlier source",
          disabled_at: 3_000,
        }),
      ],
    });
    renderListWith([provider], vi.fn());

    const dialog = await openDialog();

    // Duplicate rows share exactly one recovery action.
    expect(
      within(dialog).getAllByRole("button", { name: /clear automatic disable|re-?enable/i }),
    ).toHaveLength(1);

    // The union of all effective protocols is displayed in the stable order.
    expect(within(dialog).getByText("Chat")).toBeInTheDocument();
    expect(within(dialog).getByText("Responses")).toBeInTheDocument();
    expect(within(dialog).getByText("Anthropic Messages")).toBeInTheDocument();

    // Metadata follows the newest non-null disable time; equal times keep the
    // earlier source row.
    expect(within(dialog).getByText(/newest reason/)).toBeInTheDocument();
    expect(
      within(dialog).queryByText(/older reason|tie earlier source/),
    ).not.toBeInTheDocument();

    // The manually disabled constituent keeps its inline indicator.
    expect(
      within(dialog).getByText(i18n.t("aiGatewayAutoDisabledMappingManual")),
    ).toBeInTheDocument();
  });

  it("preserves case-distinct local ids, disambiguates equal provider names by id and orders sources deterministically", async () => {
    const pA = makeProvider({
      id: "aaa",
      name: "Twin",
      mappings: [
        autoMapping({
          local_model: " M ",
          upstream_model: " b-up ",
          disabled_reason: null,
          disabled_at: null,
        }),
        autoMapping({
          local_model: "m",
          upstream_model: "a-up",
          disabled_reason: "plain lower",
          disabled_at: 2_000,
        }),
      ],
    });
    const pB = makeProvider({
      id: "bbb",
      name: "Twin",
      mappings: [
        autoMapping({
          local_model: " M ",
          upstream_model: "z-up",
          disabled_reason: "z source",
          disabled_at: 5_000,
        }),
      ],
    });
    renderListWith([pA, pB], vi.fn());

    const dialog = await openDialog();

    // Trimmed case-sensitive identities stay distinct: uppercase trimmed " M "
    // and lowercase "m" never merge.
    expect(within(dialog).getByText("M")).toBeInTheDocument();
    expect(within(dialog).getByText("m")).toBeInTheDocument();

    // Equal provider display names are distinguished by their IDs.
    expect(within(dialog).getAllByTitle("aaa")).toHaveLength(2);
    expect(within(dialog).getAllByTitle("bbb")).toHaveLength(1);

    // Inside one provider group the upstream targets render in deterministic
    // ascending order.
    const upperSources = sourceRows(dialog, /^Auto-disabled source M to /);
    expect(upperSources).toHaveLength(2);
    const upperOrder = upperSources.map((row) => row.textContent ?? "");
    expect(
      upperOrder.findIndex((text) => text.includes("b-up")),
    ).toBeLessThan(
      upperOrder.findIndex((text) => text.includes("z-up")),
    );
  });

  it("renders missing reason and time as unavailable without fabricating values and discloses a long reason through the keyboard", async () => {
    const longReason = "relay timeout; backend ".repeat(10);
    const provider = makeProvider({
      id: "p-detail",
      name: "Detail Provider",
      mappings: [
        autoMapping({
          local_model: "m-missing",
          upstream_model: "u-missing",
          disabled_reason: null,
          disabled_at: null,
        }),
        autoMapping({
          local_model: "m-plain",
          upstream_model: "u-plain",
          disabled_reason: "<b>HTTP 500 from proxy</b>",
          disabled_at: null,
        }),
        autoMapping({
          local_model: "m-long",
          upstream_model: "u-long",
          disabled_reason: longReason,
          disabled_at: null,
        }),
      ],
    });
    renderListWith([provider], vi.fn());

    const dialog = await openDialog();

    // Missing reason is marked unavailable and missing time degrades to an em
    // dash; neither value is invented.
    expect(
      within(dialog).getByText(i18n.t("aiGatewayAutoDisabledReasonUnavailable")),
    ).toBeInTheDocument();
    const missingTime = within(dialog).getAllByText(
      i18n.t("aiGatewayAutoDisabledDisabledAt", { time: "—" }),
      { exact: false },
    );
    expect(missingTime).toHaveLength(3);

    // Reasons are plain text: markup stays visible as characters.
    expect(within(dialog).getByText(/<b>HTTP 500/)).toBeInTheDocument();

    // A long reason collapses by default and the disclosure works purely with
    // the keyboard.
    const actions = within(dialog).getAllByLabelText(
      i18n.t("aiGatewayAutoDisabledShowReasonAria", {
        upstream: "u-long",
        local: "m-long",
        provider: "Detail Provider",
      }),
    );
    expect(actions).toHaveLength(1);
    expect(actions[0]).toHaveAttribute("aria-expanded", "false");
    actions[0].focus();
    await userEvent.keyboard("{Enter}");
    expect(actions[0]).toHaveAttribute("aria-expanded", "true");
    expect(within(dialog).getByText(/relay timeout/i)).toBeInTheDocument();
    expect(
      within(dialog).getByText(i18n.t("aiGatewayAutoDisabledHideReason")),
    ).toBeInTheDocument();
  });

  it("model search keeps the whole group while a source search keeps only the matching sources and no-match is distinct", async () => {
    const provider = makeProvider({
      id: "p-search",
      name: "Search Provider",
      mappings: [
        autoMapping({ local_model: "m1", upstream_model: "u-keep" }),
        autoMapping({ local_model: "m1", upstream_model: "u-extra" }),
      ],
    });
    renderListWith([provider], vi.fn());

    const dialog = await openDialog();
    const input = within(dialog).getByRole("searchbox");

    // A source-only match keeps just the matching source inside the group.
    await userEvent.type(input, "u-keep");
    let rows = sourceRows(dialog, /Auto-disabled source m1 to /);
    expect(rows).toHaveLength(1);
    expect(rows[0].textContent).toContain("u-keep");
    expect(within(dialog).getByText(/Showing 1 of 1 models · 1 of 2 sources/)).toBeInTheDocument();

    // A model match exposes the whole group with every source.
    await userEvent.clear(input);
    await userEvent.type(input, "m1");
    rows = sourceRows(dialog, /Auto-disabled source m1 to /);
    expect(rows).toHaveLength(2);

    // A no-match state is distinct from the stored-empty state.
    await userEvent.clear(input);
    await userEvent.type(input, "zzz");
    expect(
      within(dialog).getByText(i18n.t("aiGatewayAutoDisabledNoMatch")),
    ).toBeInTheDocument();
  });
});

describe("Auto-disabled dialog zh descriptors and keyboard flows", () => {
  it("renders Chinese labels, pending aria state, no-match and focus return on Escape", async () => {
    await i18n.changeLanguage("zh");
    const recover = vi.fn(() => new Promise<boolean>(() => {}));
    const provider = makeProvider({
      id: "p-zh",
      name: "中文服务商",
      enabled: false,
      mappings: [
        autoMapping({
          local_model: "模型甲",
          upstream_model: "上游甲",
          enabled: false,
          disabled_reason: "上游持续失败",
          disabled_at: 1_000,
        }),
      ],
    });
    renderListWith([provider], recover);

    // Keyboard opens the dialog: focus the trigger and press Enter.
    const trigger = screen.getByTestId("ai-gateway-auto-disabled-models");
    trigger.focus();
    await userEvent.keyboard("{Enter}");
    const dialog = await screen.findByTestId(
      "ai-gateway-auto-disabled-models-dialog",
    );

    // Chinese manual-disable indicators and pending state copy come from i18n.
    expect(
      within(dialog).getByText(i18n.t("aiGatewayAutoDisabledMappingManual")),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText(i18n.t("aiGatewayAutoDisabledProviderManual")),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByText(/原因/),
    ).toBeInTheDocument();

    const pendingAction = within(dialog).getByRole("button", {
      name: i18n.t("aiGatewayAutoDisabledClearAria", {
        upstream: "上游甲",
        local: "模型甲",
        provider: "中文服务商",
      }),
    });
    fireEvent.click(pendingAction);
    expect(recover).toHaveBeenCalledTimes(1);
    expect(
      within(dialog).getByText(i18n.t("aiGatewayAutoDisabledReenabling")),
    ).toBeInTheDocument();

    // Escape closes the dialog and focus returns to the trigger.
    await userEvent.keyboard("{Escape}");
    await waitFor(() =>
      expect(
        screen.queryByTestId("ai-gateway-auto-disabled-models-dialog"),
      ).not.toBeInTheDocument(),
    );
    expect(document.activeElement).toBe(trigger);
  });

  it("renders the Chinese no-match state", async () => {
    const provider = makeProvider({
      id: "p-zh-empty",
      name: "中文空态",
      mappings: [
        autoMapping({
          local_model: "m-zh",
          upstream_model: "u-zh",
          disabled_reason: null,
          disabled_at: null,
        }),
      ],
    });
    renderListWith([provider], vi.fn());

    const dialog = await openDialog();
    await userEvent.type(within(dialog).getByRole("searchbox"), "不存在的关键字");
    expect(
      within(dialog).getByText(i18n.t("aiGatewayAutoDisabledNoMatch")),
    ).toBeInTheDocument();
  });

  it("renders the Chinese empty state for an empty provider list", async () => {
    renderListWith([], vi.fn());
    const dialog = await openDialog();
    expect(
      within(dialog).getByText(i18n.t("aiGatewayAutoDisabledEmpty")),
    ).toBeInTheDocument();
  });
});
