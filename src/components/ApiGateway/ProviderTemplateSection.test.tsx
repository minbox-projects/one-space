import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import {
  ProviderTemplateSection,
  type ProviderTemplateSectionProps,
} from "./ProviderTemplateSection";
import {
  formatGatewayTimestamp,
  formatOffPeakDays,
  type GatewayProviderTemplate,
  type GatewayProviderTemplateModel,
  type GatewayProviderTemplateView,
} from "@/lib/apiGateway";
import { renderWithProviders } from "@/test/mocks/render";

function makeModel(
  overrides: Partial<GatewayProviderTemplateModel> = {},
): GatewayProviderTemplateModel {
  return {
    upstream_model: "deepseek-chat",
    display_name: "DeepSeek Chat",
    protocol: "chat_completions",
    input: 1.11,
    cache_read: 2.22,
    cache_write: 3.33,
    output: 4.44,
    off_peaks: [],
    reasoning_efforts: [],
    ...overrides,
  };
}

function makeTemplate(
  overrides: Partial<GatewayProviderTemplate> = {},
): GatewayProviderTemplate {
  return {
    id: "t1",
    name: "OpenCode Zen",
    description: "Curated OpenCode models",
    base_url: "https://opencode.ai/zen/v1",
    protocol: "responses",
    source: "snapshot:models.dev",
    snapshot_version: "2026.09.18",
    models: [makeModel()],
    ...overrides,
  };
}

function makeView(
  overrides: Partial<GatewayProviderTemplateView> = {},
): GatewayProviderTemplateView {
  return {
    template: makeTemplate(),
    synced_at: null,
    source: "snapshot:models.dev",
    from_snapshot: true,
    ...overrides,
  };
}

function renderSection(overrides: Partial<ProviderTemplateSectionProps> = {}) {
  const onSync = overrides.onSync ?? vi.fn();
  const onCreateProvider =
    overrides.onCreateProvider ?? vi.fn().mockResolvedValue(true);

  renderWithProviders(
    <ProviderTemplateSection
      templates={overrides.templates ?? [makeView()]}
      busy={overrides.busy ?? false}
      syncingTemplateIds={overrides.syncingTemplateIds ?? {}}
      onSync={onSync}
      onCreateProvider={onCreateProvider}
    />,
  );

  return { onSync, onCreateProvider };
}

describe("ProviderTemplateSection 服务商模板区域", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("rendersTemplateCardsWithMetadataSnapshotBadgeAndLastSync", () => {
    const syncedAt = 1_700_000_000;
    const snapshotView = makeView({
      template: makeTemplate({
        id: "t1",
        name: "OpenCode Zen",
        description: "Curated OpenCode models",
        source: "snapshot:models.dev",
        models: [makeModel(), makeModel({ upstream_model: "m2" }), makeModel({ upstream_model: "m3" })],
      }),
      from_snapshot: true,
      synced_at: null,
    });
    const syncedView = makeView({
      template: makeTemplate({
        id: "t2",
        name: "CommandCode",
        description: "Official CommandCode models",
        source: "https://api.commandcode.ai/provider/v1/models",
        models: [makeModel({ upstream_model: "deepseek-v3" })],
      }),
      from_snapshot: false,
      synced_at: syncedAt,
      source: "live:commandcode",
    });

    renderSection({ templates: [snapshotView, syncedView] });

    const snapshotCard = screen.getByTestId("api-gateway-template-t1");
    expect(snapshotCard).toHaveTextContent("OpenCode Zen");
    expect(snapshotCard).toHaveTextContent("Curated OpenCode models");
    expect(snapshotCard).toHaveTextContent("3");
    expect(snapshotCard).toHaveTextContent("models.dev");
    // synced_at 为空时展示未同步文案，而不是时间戳
    expect(snapshotCard).toHaveTextContent(i18n.t("apiGatewayTemplateNotSynced"));
    expect(snapshotCard).not.toHaveTextContent(/\d{4}-\d{2}-\d{2} \d{2}:\d{2}/);

    // 快照徽标只出现在 from_snapshot=true 的模板上
    expect(screen.getByTestId("api-gateway-template-snapshot-t1")).toBeInTheDocument();
    expect(screen.queryByTestId("api-gateway-template-snapshot-t2")).not.toBeInTheDocument();

    const syncedCard = screen.getByTestId("api-gateway-template-t2");
    expect(syncedCard).toHaveTextContent("CommandCode");
    expect(syncedCard).toHaveTextContent("Official CommandCode models");
    expect(syncedCard).toHaveTextContent("commandcode.ai");
    expect(syncedCard).toHaveTextContent(formatGatewayTimestamp(syncedAt)!);
    expect(screen.getByTestId("api-gateway-provider-templates")).toBeInTheDocument();
  });

  it("expandingTemplateShowsFourTierPricesOffPeakDaysAndReasoningEfforts", () => {
    const view = makeView({
      template: makeTemplate({
        models: [
          makeModel({
            upstream_model: "deepseek-chat",
            input: 1.11,
            cache_read: 2.22,
            cache_write: 3.33,
            output: 4.44,
            off_peaks: [
              {
                start_time: "00:00",
                end_time: "09:00",
                input: 5.55,
                cache_read: 6.66,
                cache_write: 7.77,
                output: 8.88,
                days: [1, 2, 3, 4, 5],
              },
            ],
            reasoning_efforts: ["reasoning-low", "reasoning-medium", "reasoning-high"],
          }),
        ],
      }),
    });

    renderSection({ templates: [view] });

    const expand = screen.getByTestId("api-gateway-template-expand-t1");
    expect(expand).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(expand);
    expect(expand).toHaveAttribute("aria-expanded", "true");

    const row = screen.getByTestId("api-gateway-template-model-t1-deepseek-chat");
    // 四档标准价
    expect(row).toHaveTextContent("1.11");
    expect(row).toHaveTextContent("2.22");
    expect(row).toHaveTextContent("3.33");
    expect(row).toHaveTextContent("4.44");
    // 峰谷时段：时间 + 星期文案 + 四档优惠价
    expect(row).toHaveTextContent("00:00");
    expect(row).toHaveTextContent("09:00");
    expect(row).toHaveTextContent(formatOffPeakDays([1, 2, 3, 4, 5], i18n.t));
    expect(row).toHaveTextContent("5.55");
    expect(row).toHaveTextContent("6.66");
    expect(row).toHaveTextContent("7.77");
    expect(row).toHaveTextContent("8.88");
    // reasoning_efforts chips
    expect(row).toHaveTextContent("reasoning-low");
    expect(row).toHaveTextContent("reasoning-medium");
    expect(row).toHaveTextContent("reasoning-high");
  });

  it("syncButtonIsolatesBusyStatePerTemplate", () => {
    const templateA = makeView({ template: makeTemplate({ id: "t1", name: "Alpha" }) });
    const templateB = makeView({ template: makeTemplate({ id: "t2", name: "Beta" }) });
    const { onSync } = renderSection({
      templates: [templateA, templateB],
      syncingTemplateIds: { t1: true },
    });

    const buttonA = screen.getByTestId("api-gateway-template-sync-t1");
    const buttonB = screen.getByTestId("api-gateway-template-sync-t2");
    expect(buttonA).toBeDisabled();
    expect(buttonA).toHaveTextContent(i18n.t("apiGatewayTemplateSyncing"));
    expect(buttonB).toBeEnabled();

    fireEvent.click(buttonB);

    expect(onSync).toHaveBeenCalledWith("t2");
    expect(onSync).not.toHaveBeenCalledWith("t1");
  });

  it("emptyTemplatesShowsEmptyState", () => {
    renderSection({ templates: [] });

    const root = screen.getByTestId("api-gateway-provider-templates");
    expect(root).toBeInTheDocument();
    expect((root.textContent ?? "").trim().length).toBeGreaterThan(0);
    expect(screen.queryByTestId("api-gateway-template-t1")).not.toBeInTheDocument();
    expect(screen.queryByTestId("api-gateway-template-expand-t1")).not.toBeInTheDocument();
  });

  it("createDialogRejectsEmptyApiKeyWithoutClosing", async () => {
    const { onCreateProvider } = renderSection();

    fireEvent.click(screen.getByTestId("api-gateway-template-add-t1"));
    const keyInput = await screen.findByTestId("api-gateway-template-api-key");
    fireEvent.change(keyInput, { target: { value: "   " } });

    fireEvent.click(screen.getByTestId("api-gateway-template-create-submit"));

    await screen.findByTestId("api-gateway-template-api-key-error");
    expect(onCreateProvider).not.toHaveBeenCalled();
    expect(screen.getByTestId("api-gateway-template-api-key")).toBeInTheDocument();
  });

  it("createDialogSubmitsPrefilledRequestAndClosesOnSuccess", async () => {
    const onCreateProvider = vi.fn().mockResolvedValue(true);
    renderSection({ onCreateProvider });

    fireEvent.click(screen.getByTestId("api-gateway-template-add-t1"));
    await screen.findByTestId("api-gateway-template-api-key");

    // name / base_url 预填模板值
    const nameInput = screen.getByDisplayValue("OpenCode Zen");
    const baseUrlInput = screen.getByDisplayValue("https://opencode.ai/zen/v1");

    fireEvent.change(nameInput, { target: { value: "My Zen" } });
    fireEvent.change(baseUrlInput, { target: { value: "https://custom.example/v1" } });
    fireEvent.change(screen.getByTestId("api-gateway-template-api-key"), {
      target: { value: "sk-live-key" },
    });

    fireEvent.click(screen.getByTestId("api-gateway-template-create-submit"));

    await waitFor(() =>
      expect(onCreateProvider).toHaveBeenCalledWith({
        templateId: "t1",
        name: "My Zen",
        baseUrl: "https://custom.example/v1",
        // protocol 预填模板值，而不是默认的 chat_completions
        protocol: "responses",
        apiKey: "sk-live-key",
      }),
    );

    await waitFor(() =>
      expect(screen.queryByTestId("api-gateway-template-api-key")).not.toBeInTheDocument(),
    );
  });

  it("createDialogStaysOpenWhenCreatorReturnsFalse", async () => {
    const onCreateProvider = vi.fn().mockResolvedValue(false);
    renderSection({ onCreateProvider });

    fireEvent.click(screen.getByTestId("api-gateway-template-add-t1"));
    await screen.findByTestId("api-gateway-template-api-key");
    fireEvent.change(screen.getByTestId("api-gateway-template-api-key"), {
      target: { value: "sk-live-key" },
    });
    fireEvent.click(screen.getByTestId("api-gateway-template-create-submit"));

    await waitFor(() => expect(onCreateProvider).toHaveBeenCalledTimes(1));
    expect(screen.getByTestId("api-gateway-template-api-key")).toBeInTheDocument();
  });
});
