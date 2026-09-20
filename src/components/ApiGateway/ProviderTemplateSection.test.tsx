import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import {
  ProviderTemplateSection,
  type ProviderTemplateSectionProps,
} from "./ProviderTemplateSection";
import {
  formatGatewayTimestamp,
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
    enabled: true,
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
    source: "https://opencode.ai/zen/v1/models",
    models_url: "https://opencode.ai/zen/v1/models",
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
    source: "https://opencode.ai/zen/v1/models",
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
  let writeText: ReturnType<typeof vi.fn>;

  beforeEach(async () => {
    await i18n.changeLanguage("en");
    writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
  });

  it("rendersTemplateCardsWithMetadataAndNoSnapshotBadges", () => {
    const syncedAt = 1_700_000_000;
    const snapshotView = makeView({
      template: makeTemplate({
        id: "t1",
        name: "OpenCode Zen",
        description: "Curated OpenCode models",
        source: "https://opencode.ai/zen/v1/models",
        models: [
          makeModel(),
          makeModel({ upstream_model: "m2" }),
          makeModel({ upstream_model: "m3" }),
        ],
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
      source: "https://api.commandcode.ai/provider/v1/models",
    });

    renderSection({ templates: [snapshotView, syncedView] });

    const snapshotCard = screen.getByTestId("api-gateway-template-t1");
    expect(snapshotCard).toHaveTextContent("OpenCode Zen");
    expect(snapshotCard).toHaveTextContent("Curated OpenCode models");
    expect(snapshotCard).toHaveTextContent("3");
    expect(snapshotCard).toHaveTextContent("opencode.ai");
    // synced_at 为空时展示未同步文案，而不是时间戳
    expect(snapshotCard).toHaveTextContent(i18n.t("apiGatewayTemplateNotSynced"));
    expect(snapshotCard).not.toHaveTextContent(/\d{4}-\d{2}-\d{2} \d{2}:\d{2}/);

    // 离线快照与快照版本徽标必须彻底移除
    expect(screen.queryByTestId("api-gateway-template-snapshot-t1")).not.toBeInTheDocument();
    expect(screen.queryByTestId("api-gateway-template-snapshot-version-t1")).not.toBeInTheDocument();
    expect(screen.queryByText("Offline snapshot")).not.toBeInTheDocument();

    const syncedCard = screen.getByTestId("api-gateway-template-t2");
    expect(syncedCard).toHaveTextContent("CommandCode");
    expect(syncedCard).toHaveTextContent("Official CommandCode models");
    expect(syncedCard).toHaveTextContent("commandcode.ai");
    expect(syncedCard).toHaveTextContent(formatGatewayTimestamp(syncedAt)!);
    expect(screen.getByTestId("api-gateway-provider-templates")).toBeInTheDocument();
  });

  it("syncActionRendersOnlyWhenModelsUrlIsConfigured", () => {
    renderSection({
      templates: [
        makeView({ template: makeTemplate({ id: "t-blank", models_url: "" }) }),
        makeView({ template: makeTemplate({ id: "t-space", models_url: "   " }) }),
        makeView({ template: makeTemplate({ id: "t-null", models_url: null }) }),
        makeView({ template: makeTemplate({ id: "t-defined" }) }),
      ],
    });

    expect(screen.queryByTestId("api-gateway-template-sync-t-blank")).not.toBeInTheDocument();
    expect(screen.queryByTestId("api-gateway-template-sync-t-space")).not.toBeInTheDocument();
    expect(screen.queryByTestId("api-gateway-template-sync-t-null")).not.toBeInTheDocument();

    const syncButton = screen.getByTestId("api-gateway-template-sync-t-defined");
    expect(syncButton).toBeEnabled();
    expect(syncButton).toHaveTextContent("Sync models");
  });

  it("syncActionUsesLocalizedLabel", async () => {
    await i18n.changeLanguage("zh");
    renderSection({ templates: [makeView()] });

    expect(screen.getByTestId("api-gateway-template-sync-t1")).toHaveTextContent(
      "同步模型列表",
    );
  });

  it("expandedTemplateListsModelsWithoutPricesOffPeakOrReasoningEfforts", () => {
    const view = makeView({
      template: makeTemplate({
        models: [
          makeModel({
            upstream_model: "deepseek-chat",
            display_name: "DeepSeek Chat",
            enabled: true,
          }),
          makeModel({
            upstream_model: "gpt-4o",
            display_name: "GPT-4o",
            enabled: false,
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
    expect(row).toHaveTextContent("deepseek-chat");
    expect(row).toHaveTextContent("DeepSeek Chat");
    expect(row).toHaveTextContent("Chat");
    expect(screen.getByTestId("template-copy-model-t1-deepseek-chat")).toBeInTheDocument();

    for (const label of [
      "$/1M tokens",
      "Input",
      "Cache read",
      "Cache write",
      "Output",
      "Off-peak",
      "No off-peak windows",
      "Reasoning efforts",
    ]) {
      expect(
        screen.queryAllByText(label),
        `${label} 不应出现在模板卡片`,
      ).toHaveLength(0);
    }
    expect(screen.queryByTestId("api-gateway-template-snapshot-t1")).not.toBeInTheDocument();
    expect(screen.queryByTestId("api-gateway-template-snapshot-version-t1")).not.toBeInTheDocument();
  });

  it("disabledModelRowIsDeemphasizedAndMarked", () => {
    const view = makeView({
      template: makeTemplate({
        models: [
          makeModel({ upstream_model: "enabled-model", enabled: true }),
          makeModel({ upstream_model: "disabled-model", enabled: false }),
        ],
      }),
    });

    renderSection({ templates: [view] });
    fireEvent.click(screen.getByTestId("api-gateway-template-expand-t1"));

    const disabledRow = screen.getByTestId(
      "api-gateway-template-model-t1-disabled-model",
    );
    expect(
      disabledRow.getAttribute("data-disabled"),
      "禁用模型行应带 data-disabled=true",
    ).toBe("true");
    expect(
      disabledRow.className,
      "禁用模型行应带弱化样式 opacity-60",
    ).toContain("opacity-60");

    const enabledRow = screen.getByTestId(
      "api-gateway-template-model-t1-enabled-model",
    );
    expect(
      enabledRow.getAttribute("data-disabled"),
      "启用模型行不应带 data-disabled",
    ).toBeNull();
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
    // 同步中的模板仍可重开添加弹窗，busy 只作用于当前模板的同步按钮
    expect(screen.getByTestId("api-gateway-template-add-t1")).toBeEnabled();

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

  it("zeroModelTemplateShowsDedicatedNoModelsHint", () => {
    const view = makeView({
      template: makeTemplate({ id: "t-empty", models: [] }),
      from_snapshot: true,
    });

    renderSection({ templates: [view] });

    fireEvent.click(screen.getByTestId("api-gateway-template-expand-t-empty"));

    const hint = screen.getByTestId("api-gateway-template-no-models-t-empty");
    const text = (hint.textContent ?? "").toLowerCase();
    expect(text.length).toBeGreaterThan(0);
    // 空模型必须用专用的“无模型”提示，不能复用峰谷语义文案。
    expect(text).not.toContain("off-peak");
  });

  it("filtersModelsBySearchTermAndClearsFilter", () => {
    const view = makeView({
      template: makeTemplate({
        id: "t1",
        models: [
          makeModel({ upstream_model: "deepseek-chat", display_name: "DeepSeek Chat" }),
          makeModel({ upstream_model: "claude-3-opus", display_name: "Claude 3 Opus" }),
        ],
      }),
    });

    renderSection({ templates: [view] });

    fireEvent.click(screen.getByTestId("api-gateway-template-expand-t1"));

    expect(screen.getByTestId("api-gateway-template-model-t1-deepseek-chat")).toBeInTheDocument();
    expect(screen.getByTestId("api-gateway-template-model-t1-claude-3-opus")).toBeInTheDocument();

    const searchInput = screen.getByTestId("template-model-search-t1");
    fireEvent.change(searchInput, { target: { value: "deepseek" } });

    expect(screen.getByTestId("api-gateway-template-model-t1-deepseek-chat")).toBeInTheDocument();
    expect(screen.queryByTestId("api-gateway-template-model-t1-claude-3-opus")).not.toBeInTheDocument();

    fireEvent.change(searchInput, { target: { value: "not-exist" } });
    expect(screen.queryByTestId("api-gateway-template-model-t1-deepseek-chat")).not.toBeInTheDocument();
    expect(screen.getByTestId("template-no-matching-models-t1")).toBeInTheDocument();

    // 点击清除筛选
    fireEvent.click(screen.getByText(i18n.t("apiGatewayTemplateClearFilter")));
    expect(screen.getByTestId("api-gateway-template-model-t1-deepseek-chat")).toBeInTheDocument();
    expect(screen.getByTestId("api-gateway-template-model-t1-claude-3-opus")).toBeInTheDocument();
  });

  it("filtersModelsByProtocolPills", () => {
    const view = makeView({
      template: makeTemplate({
        id: "t1",
        protocol: "chat_completions",
        models: [
          makeModel({ upstream_model: "chat-model", protocol: "chat_completions" }),
          makeModel({ upstream_model: "resp-model", protocol: "responses" }),
        ],
      }),
    });

    renderSection({ templates: [view] });
    fireEvent.click(screen.getByTestId("api-gateway-template-expand-t1"));

    expect(screen.getByTestId("api-gateway-template-model-t1-chat-model")).toBeInTheDocument();
    expect(screen.getByTestId("api-gateway-template-model-t1-resp-model")).toBeInTheDocument();

    // 切换到 Responses
    fireEvent.click(screen.getByRole("button", { name: /Responses/ }));
    expect(screen.queryByTestId("api-gateway-template-model-t1-chat-model")).not.toBeInTheDocument();
    expect(screen.getByTestId("api-gateway-template-model-t1-resp-model")).toBeInTheDocument();

    // 切换到 Chat
    fireEvent.click(screen.getByRole("button", { name: /^Chat/ }));
    expect(screen.getByTestId("api-gateway-template-model-t1-chat-model")).toBeInTheDocument();
    expect(screen.queryByTestId("api-gateway-template-model-t1-resp-model")).not.toBeInTheDocument();

    // 切换回 All
    fireEvent.click(screen.getByRole("button", { name: /All/ }));
    expect(screen.getByTestId("api-gateway-template-model-t1-chat-model")).toBeInTheDocument();
    expect(screen.getByTestId("api-gateway-template-model-t1-resp-model")).toBeInTheDocument();
  });

  it("copiesModelIdentifierToClipboard", async () => {
    const view = makeView({
      template: makeTemplate({
        id: "t1",
        models: [makeModel({ upstream_model: "deepseek-chat" })],
      }),
    });

    renderSection({ templates: [view] });
    fireEvent.click(screen.getByTestId("api-gateway-template-expand-t1"));

    const copyBtn = screen.getByTestId("template-copy-model-t1-deepseek-chat");
    await waitFor(async () => {
      fireEvent.click(copyBtn);
      expect(writeText).toHaveBeenCalledWith("deepseek-chat");
    });
  });

  it("togglesAllTemplatesExpandedState", () => {
    const view1 = makeView({ template: makeTemplate({ id: "t1", name: "T1" }) });
    const view2 = makeView({ template: makeTemplate({ id: "t2", name: "T2" }) });

    renderSection({ templates: [view1, view2] });

    const toggleAllBtn = screen.getByTestId("template-section-toggle-all-btn");
    const expand1 = screen.getByTestId("api-gateway-template-expand-t1");
    const expand2 = screen.getByTestId("api-gateway-template-expand-t2");

    expect(expand1).toHaveAttribute("aria-expanded", "false");
    expect(expand2).toHaveAttribute("aria-expanded", "false");

    // 全部展开
    fireEvent.click(toggleAllBtn);
    expect(expand1).toHaveAttribute("aria-expanded", "true");
    expect(expand2).toHaveAttribute("aria-expanded", "true");

    // 全部收起
    fireEvent.click(toggleAllBtn);
    expect(expand1).toHaveAttribute("aria-expanded", "false");
    expect(expand2).toHaveAttribute("aria-expanded", "false");
  });

  it("renders corresponding template icon in card header", () => {
    const customView = makeView({
      template: makeTemplate({
        id: "tpl-cmd",
        name: "CommandCode",
        icon: "commandcode",
      }),
    });
    renderSection({ templates: [customView] });

    const card = screen.getByTestId("api-gateway-template-tpl-cmd");
    expect(within(card).getByTestId("provider-icon-commandcode")).toBeInTheDocument();
  });
});
