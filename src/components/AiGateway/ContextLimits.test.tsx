import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import {
  type GatewayConfig,
  type GatewayModelPool,
  type GatewayProviderTemplate,
  type GatewayProviderTemplateModel,
  type GatewayProviderTemplateView,
  type GatewayTerminalTarget,
  type GatewayUpstreamProvider,
  modelPoolCandidateKey,
} from "@/lib/aiGateway";
import { ProviderTemplateEditDialog } from "./ProviderTemplateEditDialog";
import { ProviderDetailDialog } from "./ProviderDetailDialog";
import { ModelListPanel } from "./ModelListPanel";
import { TerminalSyncPanel } from "./TerminalSyncPanel";
import { ClaudeModelSelectionDialog } from "./ClaudeModelSelectionDialog";
import { ModelPoolDialog } from "./ModelPoolDialog";
import { ModelPoolCard } from "./ModelPoolCard";
import { renderWithProviders } from "@/test/mocks/render";

/**
 * Operator-facing contract for model context windows across the template,
 * provider, model-list and terminal surfaces. Fixtures carry the additive
 * `context_window` field the backend already stores; these tests pin the UI
 * behavior before the frontend DTOs and controls exist.
 */

type ContextModel = GatewayProviderTemplateModel & { context_window?: number | null };
type ContextMapping = GatewayUpstreamProvider["mappings"][number] & {
  context_window?: number | null;
};
type ContextTarget = GatewayTerminalTarget & {
  context?: {
    mode: "per_model" | "selected_model" | "shared_compaction";
    scope: "generated_models" | "activation" | "global_and_isolated_settings";
    coverage: { total: number; known: number; unknown: number };
    selected_model?: string | null;
    raw_context?: number | null;
    applied_context?: number | null;
    reasons: string[];
  } | null;
};

function templateWith(
  models: ContextModel[],
  overrides: Partial<GatewayProviderTemplate> = {},
): GatewayProviderTemplate {
  return {
    id: "tpl-context",
    name: "Context Template",
    description: "",
    base_url: "https://context.example/v1",
    protocol: "chat_completions",
    source: "https://context.example/v1/models",
    models_url: "https://context.example/v1/models",
    models,
    ...overrides,
  } as GatewayProviderTemplate;
}

function providerWith(
  mappings: ContextMapping[],
  overrides: Partial<GatewayUpstreamProvider> = {},
): GatewayUpstreamProvider {
  return {
    id: "prov-context",
    name: "Context Provider",
    base_url: "https://context.example/v1",
    keys: [],
    default_model: null,
    protocol: "chat_completions",
    mappings,
    enabled: true,
    ...overrides,
  } as unknown as GatewayUpstreamProvider;
}

function templateView(template: GatewayProviderTemplate): GatewayProviderTemplateView {
  return {
    template,
    synced_at: null,
    source: template.source,
    from_snapshot: false,
  };
}

function contextInput(index: number): HTMLElement {
  return screen.getByLabelText(
    new RegExp(`(context (window|tokens)|上下文窗口).*${index + 1}`, "i"),
  );
}

function renderTemplateDialog(
  template: GatewayProviderTemplate,
  onSave = vi.fn().mockResolvedValue(true),
) {
  const onOpenChange = vi.fn();
  render(
    <ProviderTemplateEditDialog
      open
      onOpenChange={onOpenChange}
      template={template}
      providers={[]}
      busy={false}
      onSave={onSave}
      onDelete={vi.fn().mockResolvedValue(true)}
    />,
  );
  return { onSave, onOpenChange };
}

function renderProviderDialog(
  provider: GatewayUpstreamProvider,
  options: { templates?: GatewayProviderTemplateView[] } = {},
) {
  const onSave = vi.fn().mockResolvedValue(true);
  const onOpenChange = vi.fn();
  render(
    <ProviderDetailDialog
      open
      onOpenChange={onOpenChange}
      provider={provider}
      templates={options.templates}
      busy={false}
      onSave={onSave}
      onDelete={vi.fn().mockResolvedValue(true)}
    />,
  );
  return { onSave, onOpenChange };
}

function makeConfig(providers: GatewayUpstreamProvider[]): GatewayConfig {
  return {
    enabled: true,
    port: 17688,
    providers,
    keys: [
      {
        id: "key-1",
        label: "Default Key",
        value: "sk-gateway-1234",
        enabled: true,
        created_at: 1_700_000_000,
      },
    ],
    default_key_id: "key-1",
    terminal_syncs: [],
  } as GatewayConfig;
}

function target(
  tool: "opencode" | "codex" | "claude",
  context: ContextTarget["context"],
  overrides: Partial<GatewayTerminalTarget> = {},
): GatewayTerminalTarget {
  return {
    tool,
    name: tool === "opencode" ? "OpenCode" : tool === "codex" ? "Codex" : "Claude Code",
    provider_id: `gw-${tool}`,
    base_url: "http://127.0.0.1:17688/v1",
    synced: false,
    pending_sync: true,
    synced_key_id: null,
    synced_at: null,
    context,
    ...overrides,
  } as ContextTarget;
}

function renderTerminalPanel(
  targets: GatewayTerminalTarget[],
  overrides: Partial<Parameters<typeof TerminalSyncPanel>[0]> = {},
) {
  const onConfigureTool = vi.fn();
  const onSyncTool = vi.fn();
  renderWithProviders(
    <TerminalSyncPanel
      targets={targets}
      config={makeConfig([])}
      syncingTools={{}}
      onConfigureTool={onConfigureTool}
      onSyncTool={onSyncTool}
      {...overrides}
    />,
  );
  return { onConfigureTool, onSyncTool };
}

describe("context window limits", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  describe("template editor", () => {
    it("keeps context input hidden in the template model row until expanded", () => {
      renderTemplateDialog(
        templateWith([
          { upstream_model: "model-a", local_model: "model-a", enabled: true, context_window: 128000 },
        ]),
      );
      expect(screen.queryByLabelText(/(context (window|tokens)|上下文窗口).*1/i)).not.toBeInTheDocument();
    });

    it("labels each template model's token input and shows its stored value in expanded details", () => {
      renderTemplateDialog(
        templateWith([
          { upstream_model: "model-a", local_model: "model-a", enabled: true, context_window: 128000 },
          { upstream_model: "model-b", local_model: "model-b", enabled: true },
        ]),
      );

      fireEvent.click(screen.getByTestId("template-edit-model-expand-0"));
      expect(contextInput(0)).toHaveValue("128000");

      fireEvent.click(screen.getByTestId("template-edit-model-expand-1"));
      expect(contextInput(1)).toHaveValue("");
    });

    it("accepts the inclusive token bounds and submits numbers", async () => {
      const user = userEvent.setup();
      const { onSave } = renderTemplateDialog(
        templateWith([
          { upstream_model: "model-min", local_model: "model-min", enabled: true, context_window: 1 },
          { upstream_model: "model-max", local_model: "model-max", enabled: true, context_window: 4294967295 },
        ]),
      );

      fireEvent.click(screen.getByTestId("template-edit-model-expand-0"));
      fireEvent.click(screen.getByTestId("template-edit-model-expand-1"));

      expect(contextInput(0)).toHaveValue("1");
      expect(contextInput(1)).toHaveValue("4294967295");

      await user.clear(contextInput(0));
      await user.type(contextInput(0), "256000");
      fireEvent.click(screen.getByTestId("template-edit-save-btn"));

      expect(onSave).toHaveBeenCalledTimes(1);
      const payload = onSave.mock.calls[0][0] as GatewayProviderTemplate;
      const models = payload.models as ContextModel[];
      expect(models[0].context_window).toBe(256000);
      expect(models[1].context_window).toBe(4294967295);
    });

    it("treats a blank input as unspecified instead of zero", async () => {
      const user = userEvent.setup();
      const { onSave } = renderTemplateDialog(
        templateWith([
          { upstream_model: "model-a", local_model: "model-a", enabled: true, context_window: 128000 },
        ]),
      );

      fireEvent.click(screen.getByTestId("template-edit-model-expand-0"));
      await user.clear(contextInput(0));
      fireEvent.click(screen.getByTestId("template-edit-save-btn"));

      expect(onSave).toHaveBeenCalledTimes(1);
      const savedModel = (onSave.mock.calls[0][0] as GatewayProviderTemplate)
        .models[0] as ContextModel;
      expect(savedModel.context_window ?? null).toBeNull();
    });

    it("keeps invalid drafts visible and never invokes the save", async () => {
      const user = userEvent.setup();
      const { onSave } = renderTemplateDialog(
        templateWith([
          { upstream_model: "model-a", local_model: "model-a", enabled: true, context_window: 1000 },
        ]),
      );

      fireEvent.click(screen.getByTestId("template-edit-model-expand-0"));

      for (const invalid of ["0", "-5", "1.5", "4294967296", "abc"]) {
        await user.clear(contextInput(0));
        await user.type(contextInput(0), invalid);
        fireEvent.click(screen.getByTestId("template-edit-save-btn"));
        expect(onSave).not.toHaveBeenCalled();
        expect(contextInput(0)).toHaveValue(invalid);
        expect(
          screen.getAllByText(/context window|context tokens/i).length,
        ).toBeGreaterThan(0);
      }
    });

    it("writes nothing when the operator cancels", async () => {
      const user = userEvent.setup();
      const { onSave, onOpenChange } = renderTemplateDialog(
        templateWith([
          { upstream_model: "model-a", local_model: "model-a", enabled: true, context_window: 1000 },
        ]),
      );

      fireEvent.click(screen.getByTestId("template-edit-model-expand-0"));
      await user.clear(contextInput(0));
      await user.type(contextInput(0), "2048");
      fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

      expect(onSave).not.toHaveBeenCalled();
      expect(onOpenChange).toHaveBeenCalledWith(false);
    });

    it("explains valid-source overwrite, missing-source preservation and null clearing", () => {
      renderTemplateDialog(templateWith([]));

      expect(
        screen.getByText(/valid (model[- ]list )?(source|value).*(replac|overwrit)/i),
      ).toBeInTheDocument();
      expect(
        screen.getByText(/missing (source|field).*(preserv|keep)/i),
      ).toBeInTheDocument();
      expect(screen.getByText(/explicit null.*clear/i)).toBeInTheDocument();
    });
  });

  describe("provider mapping editor", () => {
    it("keeps context input hidden in the mapping row until expanded", () => {
      renderProviderDialog(
        providerWith([
          { local_model: "local-a", upstream_model: "upstream-a", context_window: 64000 },
        ]),
      );
      expect(screen.queryByLabelText(/(context (window|tokens)|上下文窗口).*1/i)).not.toBeInTheDocument();
    });

    it("edits an unbound mapping's context in expanded details and submits it", async () => {
      const user = userEvent.setup();
      const { onSave } = renderProviderDialog(
        providerWith([
          { local_model: "local-a", upstream_model: "upstream-a", context_window: 64000 },
        ]),
      );

      // 未展开时不显示在列表中
      expect(screen.queryByLabelText(/(context (window|tokens)|上下文窗口).*1/i)).not.toBeInTheDocument();

      // 点击展开模型详情
      fireEvent.click(screen.getByTestId("ai-gateway-mapping-expand-0"));

      // 展开后以纯 tokens 64000 展示
      expect(contextInput(0)).toHaveValue("64000");

      // 支持输入纯 tokens 如 128000
      await user.clear(contextInput(0));
      await user.type(contextInput(0), "128000");
      fireEvent.click(screen.getByRole("button", { name: "Save" }));

      expect(onSave).toHaveBeenCalledTimes(1);
      const draft = onSave.mock.calls[0][0] as GatewayUpstreamProvider;
      expect((draft.mappings[0] as ContextMapping).context_window).toBe(128000);
    });

    it("clears an unbound mapping with a blank input", async () => {
      const user = userEvent.setup();
      const { onSave } = renderProviderDialog(
        providerWith([
          { local_model: "local-a", upstream_model: "upstream-a", context_window: 64000 },
        ]),
      );

      fireEvent.click(screen.getByTestId("ai-gateway-mapping-expand-0"));
      await user.clear(contextInput(0));
      fireEvent.click(screen.getByRole("button", { name: "Save" }));

      expect(onSave).toHaveBeenCalledTimes(1);
      const mapping = (onSave.mock.calls[0][0] as GatewayUpstreamProvider)
        .mappings[0] as ContextMapping;
      expect(mapping.context_window ?? null).toBeNull();
    });

    it("supports quick preset pills in expanded details", async () => {
      const { onSave } = renderProviderDialog(
        providerWith([
          { local_model: "local-a", upstream_model: "upstream-a" },
        ]),
      );

      fireEvent.click(screen.getByTestId("ai-gateway-mapping-expand-0"));
      fireEvent.click(screen.getByRole("button", { name: "256K" }));
      expect(contextInput(0)).toHaveValue("256000");

      fireEvent.click(screen.getByRole("button", { name: "Save" }));
      expect(onSave).toHaveBeenCalledTimes(1);
      const draft = onSave.mock.calls[0][0] as GatewayUpstreamProvider;
      expect((draft.mappings[0] as ContextMapping).context_window).toBe(256000);
    });

    it("allows editing a bound mapping's inherited template value", async () => {
      const user = userEvent.setup();
      const template = templateWith([
        { upstream_model: "upstream-a", local_model: "local-a", enabled: true, context_window: 200000 },
      ]);
      const { onSave } = renderProviderDialog(
        providerWith(
          [
            { local_model: "local-a", upstream_model: "upstream-a", context_window: 200000 },
            { local_model: "local-b", upstream_model: "upstream-b" },
          ],
          { template_id: template.id },
        ),
        { templates: [templateView(template)] },
      );

      fireEvent.click(screen.getByTestId("ai-gateway-mapping-expand-0"));
      expect(contextInput(0)).toHaveValue("200000");
      const inherited = contextInput(0) as HTMLInputElement;
      expect(inherited.readOnly || inherited.disabled).toBe(false);
      expect(
        screen.getByText(/inherited from.*Context Template/i),
      ).toBeInTheDocument();

      // 用户可以自由修改并保存
      await user.clear(contextInput(0));
      await user.type(contextInput(0), "512000");
      fireEvent.click(screen.getByRole("button", { name: "Save" }));

      expect(onSave).toHaveBeenCalledTimes(1);
      const draft = onSave.mock.calls[0][0] as GatewayUpstreamProvider;
      expect((draft.mappings[0] as ContextMapping).context_window).toBe(512000);
    });
  });

  describe("model list", () => {
    it("shows each source value and the conservative aggregate with formatted units", () => {
      const providers = [
        providerWith(
          [{ local_model: "shared", upstream_model: "a-128k", context_window: 128000 }],
          { id: "p1", name: "Provider One" },
        ),
        providerWith(
          [{ local_model: "shared", upstream_model: "b-256k", context_window: 256000 }],
          { id: "p2", name: "Provider Two" },
        ),
      ];
      renderWithProviders(<ModelListPanel providers={providers} />);

      const row = within(screen.getByTestId("ai-gateway-model-list"))
        .getAllByTestId("ai-gateway-model-list-row")
        .find((entry) => entry.getAttribute("data-model") === "shared");
      expect(row).toBeDefined();
      const upstreams = within(row!).getAllByTestId("ai-gateway-model-list-upstream");
      expect(within(upstreams[0]).getByText(/128K/)).toBeInTheDocument();
      expect(within(upstreams[1]).getByText(/256K/)).toBeInTheDocument();
      expect(within(row!).getByText(/conservative|effective context/i)).toBeInTheDocument();
      expect(within(row!).getAllByText(/128K/).length).toBeGreaterThan(0);
    });

    it("formats context windows below 10,000 as comma-separated and above 1,000,000 as M", () => {
      const providers = [
        providerWith(
          [{ local_model: "small-ctx", upstream_model: "m-4k", context_window: 4096 }],
          { id: "p1", name: "Provider One" },
        ),
        providerWith(
          [{ local_model: "mega-ctx", upstream_model: "m-1m", context_window: 1000000 }],
          { id: "p2", name: "Provider Two" },
        ),
      ];
      renderWithProviders(<ModelListPanel providers={providers} />);

      const smallRow = within(screen.getByTestId("ai-gateway-model-list"))
        .getAllByTestId("ai-gateway-model-list-row")
        .find((entry) => entry.getAttribute("data-model") === "small-ctx");
      expect(within(smallRow!).getAllByText(/4,096 tokens/).length).toBeGreaterThan(0);

      const megaRow = within(screen.getByTestId("ai-gateway-model-list"))
        .getAllByTestId("ai-gateway-model-list-row")
        .find((entry) => entry.getAttribute("data-model") === "mega-ctx");
      expect(within(megaRow!).getAllByText(/1M tokens/).length).toBeGreaterThan(0);
    });

    it("marks an ordinary model incomplete when any contributing source is unknown", () => {
      const providers = [
        providerWith(
          [{ local_model: "shared", upstream_model: "known", context_window: 256000 }],
          { id: "p1", name: "Provider One" },
        ),
        providerWith(
          [{ local_model: "shared", upstream_model: "unknown" }],
          { id: "p2", name: "Provider Two" },
        ),
      ];
      renderWithProviders(<ModelListPanel providers={providers} />);

      const row = screen
        .getAllByTestId("ai-gateway-model-list-row")
        .find((entry) => entry.getAttribute("data-model") === "shared");
      expect(within(row!).getByText(/incomplete|unknown context/i)).toBeInTheDocument();
    });

    it("shows a pool's conservative context without an editable override", () => {
      const providers = [
        providerWith(
          [{ local_model: "model-a", upstream_model: "upstream-a", context_window: 256000 }],
          { id: "p1", name: "Provider One" },
        ),
        providerWith(
          [{ local_model: "model-b", upstream_model: "upstream-b", context_window: 128000 }],
          { id: "p2", name: "Provider Two" },
        ),
      ];
      const pool: GatewayModelPool = {
        id: "pool-1",
        name: "Context Pool",
        model: "context-pool",
        enabled: true,
        candidates: [
          {
            provider_id: "p1",
            local_model: "model-a",
            upstream_model: "upstream-a",
            protocol: "chat_completions",
            weight: 1,
          },
          {
            provider_id: "p2",
            local_model: "model-b",
            upstream_model: "upstream-b",
            protocol: "chat_completions",
            weight: 1,
          },
        ],
      };
      renderWithProviders(<ModelListPanel providers={providers} modelPools={[pool]} />);

      const row = screen
        .getAllByTestId("ai-gateway-model-list-row")
        .find((entry) => entry.getAttribute("data-model") === "context-pool");
      expect(within(row!).getAllByText(/128K/).length).toBeGreaterThan(0);
      expect(within(row!).queryByRole("textbox")).not.toBeInTheDocument();
    });

    it("shows a pool's unified context window limit when explicitly configured", () => {
      const providers = [
        providerWith(
          [{ local_model: "model-a", upstream_model: "upstream-a", context_window: 256000 }],
          { id: "p1", name: "Provider One" },
        ),
      ];
      const pool: GatewayModelPool = {
        id: "pool-unified",
        name: "Unified Pool",
        model: "unified-pool",
        enabled: true,
        context_window: 1050000, // 1050K -> 格式化为 1.1M tokens
        candidates: [
          {
            provider_id: "p1",
            local_model: "model-a",
            upstream_model: "upstream-a",
            protocol: "chat_completions",
            weight: 1,
          },
        ],
      };
      renderWithProviders(<ModelListPanel providers={providers} modelPools={[pool]} />);

      const row = screen
        .getAllByTestId("ai-gateway-model-list-row")
        .find((entry) => entry.getAttribute("data-model") === "unified-pool");
      expect(within(row!).getAllByText(/1\.1M tokens/).length).toBeGreaterThan(0);
    });

    it("keeps the hidden panel free of context rows", () => {
      const providers = [
        providerWith(
          [{ local_model: "shared", upstream_model: "known", context_window: 256000 }],
          { id: "p1", name: "Provider One" },
        ),
      ];
      renderWithProviders(<ModelListPanel providers={providers} isActive={false} />);

      expect(screen.queryAllByTestId("ai-gateway-model-list-row")).toHaveLength(0);
    });
  });

  describe("terminal sync panel", () => {
    it("does not render context summary boxes on tool cards to keep the UI clean", () => {
      renderTerminalPanel([
        target("opencode", {
          mode: "per_model",
          scope: "generated_models",
          coverage: { total: 3, known: 2, unknown: 1 },
          reasons: ["unknown_context"],
        }),
        target("codex", {
          mode: "selected_model",
          scope: "activation",
          coverage: { total: 1, known: 1, unknown: 0 },
          selected_model: "gpt-5",
          raw_context: 400000,
          applied_context: 400000,
          reasons: [],
        }),
        target("claude", {
          mode: "shared_compaction",
          scope: "global_and_isolated_settings",
          coverage: { total: 3, known: 3, unknown: 0 },
          raw_context: 200000,
          applied_context: 200000,
          reasons: [],
        }),
      ]);

      expect(screen.queryByTestId("ai-gateway-target-context-opencode")).not.toBeInTheDocument();
      expect(screen.queryByTestId("ai-gateway-target-context-codex")).not.toBeInTheDocument();
      expect(screen.queryByTestId("ai-gateway-target-context-claude")).not.toBeInTheDocument();
    });
  });

  describe("model pool dialog and card", () => {
    it("displays context limit badges in candidate sources and selected candidate list", () => {
      const providers = [
        providerWith(
          [
            {
              local_model: "claude-3-5",
              upstream_model: "claude-3-5-sonnet",
              protocol: "anthropic_messages",
              context_window: 200000,
            },
            {
              local_model: "gpt-4o",
              upstream_model: "gpt-4o",
              protocol: "chat_completions",
              context_window: 128000,
            },
          ],
          { id: "p1", name: "Provider One" },
        ),
      ];
      const pool: GatewayModelPool = {
        id: "pool-ctx",
        name: "Test Pool",
        model: "test-pool",
        enabled: true,
        candidates: [
          {
            provider_id: "p1",
            local_model: "claude-3-5",
            upstream_model: "claude-3-5-sonnet",
            protocol: "anthropic_messages",
            weight: 1,
          },
        ],
      };

      const claudeKey = modelPoolCandidateKey({
        provider_id: "p1",
        local_model: "claude-3-5",
        upstream_model: "claude-3-5-sonnet",
        protocol: "anthropic_messages",
      });
      const gptKey = modelPoolCandidateKey({
        provider_id: "p1",
        local_model: "gpt-4o",
        upstream_model: "gpt-4o",
        protocol: "chat_completions",
      });

      renderWithProviders(
        <ModelPoolDialog
          open
          onOpenChange={vi.fn()}
          providers={providers}
          pool={pool}
          onSave={vi.fn()}
        />,
      );

      // 备选模型列表中展示上下文限制徽标（200K、128K）
      expect(
        screen.getByTestId(`model-pool-source-context-${claudeKey}`),
      ).toHaveTextContent("200K");
      expect(
        screen.getByTestId(`model-pool-source-context-${gptKey}`),
      ).toHaveTextContent("128K");

      // 已选 candidate 卡片中展示上下文限制徽标
      expect(
        screen.getByTestId(`model-pool-selected-context-${claudeKey}`),
      ).toHaveTextContent("200K");
    });

    it("supports selecting a unified public context limit via dropdown from deduplicated candidate limits, defaulting to max", async () => {
      const onSave = vi.fn().mockResolvedValue(undefined);
      const providers = [
        providerWith(
          [
            {
              local_model: "claude-small",
              upstream_model: "claude-small-v1",
              protocol: "anthropic_messages",
              context_window: 128000,
            },
            {
              local_model: "claude-3-5",
              upstream_model: "claude-3-5-sonnet",
              protocol: "anthropic_messages",
              context_window: 200000,
            },
            {
              local_model: "claude-backup",
              upstream_model: "claude-backup-sonnet",
              protocol: "anthropic_messages",
              context_window: 200000, // duplicate context
            },
          ],
          { id: "p1", name: "Provider One" },
        ),
      ];
      const pool: GatewayModelPool = {
        id: "pool-ctx",
        name: "Test Pool",
        model: "test-pool",
        enabled: true,
        // pool 未指定 context_window，应默认使用已选模型的最大上下文长度 (200000)
        candidates: [
          {
            provider_id: "p1",
            local_model: "claude-small",
            upstream_model: "claude-small-v1",
            protocol: "anthropic_messages",
            weight: 1,
          },
          {
            provider_id: "p1",
            local_model: "claude-3-5",
            upstream_model: "claude-3-5-sonnet",
            protocol: "anthropic_messages",
            weight: 1,
          },
          {
            provider_id: "p1",
            local_model: "claude-backup",
            upstream_model: "claude-backup-sonnet",
            protocol: "anthropic_messages",
            weight: 1,
          },
        ],
      };

      renderWithProviders(
        <ModelPoolDialog
          open
          onOpenChange={vi.fn()}
          providers={providers}
          pool={pool}
          onSave={onSave}
        />,
      );

      const select = screen.getByTestId("model-pool-context-select") as HTMLSelectElement;
      // 选项只能从已选模型的上下文长度中去重选择：128000, 200000（200000 虽出现两次但已去重）
      const options = Array.from(select.options).map((opt) => opt.value);
      expect(options).toEqual(["128000", "200000"]);

      // 默认使用已选模型的最大上下文长度
      expect(select.value).toBe("200000");

      // 用户切换选择为 128000
      fireEvent.change(select, { target: { value: "128000" } });
      expect(select.value).toBe("128000");

      // 点击保存
      fireEvent.click(screen.getByRole("button", { name: /save|保存/i }));
      expect(onSave).toHaveBeenCalledTimes(1);
      const savedPool = onSave.mock.calls[0][0] as GatewayModelPool;
      expect(savedPool.context_window).toBe(128000);
    });

    it("disables context limit dropdown when no selected candidate has a known limit and saves null", async () => {
      const onSave = vi.fn().mockResolvedValue(undefined);
      const providers = [
        providerWith(
          [
            {
              local_model: "custom-no-limit",
              upstream_model: "custom-upstream",
              protocol: "anthropic_messages",
              context_window: null,
            },
          ],
          { id: "p1", name: "Provider One" },
        ),
      ];
      const pool: GatewayModelPool = {
        id: "pool-no-ctx",
        name: "No Ctx Pool",
        model: "no-ctx-pool",
        enabled: true,
        candidates: [
          {
            provider_id: "p1",
            local_model: "custom-no-limit",
            upstream_model: "custom-upstream",
            protocol: "anthropic_messages",
            weight: 1,
          },
        ],
      };

      renderWithProviders(
        <ModelPoolDialog
          open
          onOpenChange={vi.fn()}
          providers={providers}
          pool={pool}
          onSave={onSave}
        />,
      );

      const select = screen.getByTestId("model-pool-context-select") as HTMLSelectElement;
      expect(select).toBeDisabled();
      expect(select.value).toBe("");

      fireEvent.click(screen.getByRole("button", { name: /save|保存/i }));
      expect(onSave).toHaveBeenCalledTimes(1);
      const savedPool = onSave.mock.calls[0][0] as GatewayModelPool;
      expect(savedPool.context_window).toBeNull();
    });

    it("does not display context limit badges on model pool cards", () => {
      const providers = [
        providerWith(
          [{ local_model: "claude-3-5", upstream_model: "claude-3-5-sonnet", context_window: 200000 }],
          { id: "p1", name: "Provider One" },
        ),
      ];
      const pool: GatewayModelPool = {
        id: "pool-ctx",
        name: "Test Pool",
        model: "test-pool",
        enabled: true,
        candidates: [
          {
            provider_id: "p1",
            local_model: "claude-3-5",
            upstream_model: "claude-3-5-sonnet",
            protocol: "anthropic_messages",
            weight: 1,
          },
        ],
      };

      renderWithProviders(
        <ModelPoolCard
          pool={pool}
          providers={providers}
          onToggleEnabled={vi.fn()}
          onEdit={vi.fn()}
          onOpenLogs={vi.fn()}
        />,
      );

      // 模型池卡片上不显示上下文限制信息
      expect(screen.queryByTestId(/model-pool-.*-context/)).not.toBeInTheDocument();
      expect(screen.queryByText("200K")).not.toBeInTheDocument();
    });
  });

  describe("Claude selection dialog", () => {
    function renderClaudeDialog(contextByModel: Record<string, number | null>, models = ["cc-a"]) {
      const onSubmit = vi.fn();
      const onCancel = vi.fn();
      const view = render(
        <ClaudeModelSelectionDialog
          open
          models={models}
          contextByModel={contextByModel}
          onSubmit={onSubmit}
          onCancel={onCancel}
        />,
      );
      return { ...view, onSubmit, onCancel };
    }

    function selectEverySlot(models: string[], selected: string) {
      for (const testId of [
        "claude-model-select-default",
        "claude-model-select-haiku",
        "claude-model-select-sonnet",
        "claude-model-select-opus",
      ]) {
        fireEvent.change(screen.getByTestId(testId), { target: { value: selected } });
      }
      expect(models).toContain(selected);
    }

    it("shows 99999 as unavailable and 100000/1000000 as accepted", () => {
      const first = renderClaudeDialog({ "cc-a": 99999, "cc-b": 100000, "cc-c": 1000000 });
      selectEverySlot(["cc-a", "cc-b", "cc-c"], "cc-a");
      expect(screen.getByText(/unavailable/i)).toBeInTheDocument();
      expect(screen.getByText(/below 100K|minimum|低于 100K/i)).toBeInTheDocument();
      first.unmount();

      const second = renderClaudeDialog({ "cc-a": 99999, "cc-b": 100000, "cc-c": 1000000 });
      selectEverySlot(["cc-a", "cc-b", "cc-c"], "cc-b");
      expect(screen.getByText(/shared/i)).toBeInTheDocument();
      expect(screen.getAllByText(/100K/).length).toBeGreaterThan(0);
      second.unmount();

      const third = renderClaudeDialog({ "cc-a": 99999, "cc-b": 100000, "cc-c": 1000000 });
      selectEverySlot(["cc-a", "cc-b", "cc-c"], "cc-c");
      expect(screen.getAllByText(/1M/).length).toBeGreaterThan(0);
      expect(screen.queryByText(/capped/i)).not.toBeInTheDocument();
      third.unmount();
    });

    it("discloses the 1000000 cap and an unknown minimum", () => {
      const capped = renderClaudeDialog({
        "cc-a": 1000000,
        "cc-b": 1500000,
        "cc-c": null,
      });
      selectEverySlot(["cc-a", "cc-b", "cc-c"], "cc-b");
      expect(screen.getByText(/capped/i)).toBeInTheDocument();
      expect(screen.getAllByText(/1M/).length).toBeGreaterThan(0);
      expect(screen.getAllByText(/1\.5M/).length).toBeGreaterThan(0);
      capped.unmount();

      renderClaudeDialog({ "cc-a": null, "cc-b": 100000 });
      selectEverySlot(["cc-a", "cc-b"], "cc-a");
      expect(screen.getByText(/unknown context/i)).toBeInTheDocument();
    });
  });

  describe("locale and accessibility", () => {
    it("renders Chinese labels and source-priority help", async () => {
      await i18n.changeLanguage("zh");
      renderTemplateDialog(
        templateWith([
          { upstream_model: "model-a", local_model: "model-a", enabled: true, context_window: 128000 },
        ]),
      );

      fireEvent.click(screen.getByTestId("template-edit-model-expand-0"));
      expect(contextInput(0)).toHaveValue("128000");
      expect(screen.getByText(/有效.*(覆盖|替换)/)).toBeInTheDocument();
      expect(screen.getByText(/缺失.*保留/)).toBeInTheDocument();
      expect(screen.getByText(/null.*清除/)).toBeInTheDocument();
    });

    it("renders Chinese context details in expanded provider dialog", async () => {
      await i18n.changeLanguage("zh");
      renderProviderDialog(
        providerWith([
          { local_model: "model-a", upstream_model: "model-a", enabled: true, context_window: 128000 },
        ]),
      );

      fireEvent.click(screen.getByTestId("ai-gateway-mapping-expand-0"));
      expect(contextInput(0)).toHaveValue("128000");
      expect(screen.getByText(/128,000 tokens/)).toBeInTheDocument();
      expect(screen.getByText(/常用预设/)).toBeInTheDocument();
    });

    it("exposes the context input through its accessible label and keyboard entry", async () => {
      const user = userEvent.setup();
      renderTemplateDialog(
        templateWith([
          { upstream_model: "model-a", local_model: "model-a", enabled: true },
        ]),
      );

      fireEvent.click(screen.getByTestId("template-edit-model-expand-0"));
      const input = contextInput(0);
      expect(input).toHaveAccessibleName(/context (window|tokens).*1|上下文窗口.*1/i);
      await user.click(input);
      await user.keyboard("4096");
      expect(input).toHaveValue("4096");
    });
  });
});
