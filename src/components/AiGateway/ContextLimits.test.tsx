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
} from "@/lib/aiGateway";
import { ProviderTemplateEditDialog } from "./ProviderTemplateEditDialog";
import { ProviderDetailDialog } from "./ProviderDetailDialog";
import { ModelListPanel } from "./ModelListPanel";
import { TerminalSyncPanel } from "./TerminalSyncPanel";
import { ClaudeModelSelectionDialog } from "./ClaudeModelSelectionDialog";
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

function digitPattern(value: number): RegExp {
  const digits = String(value);
  const grouped = digits.replace(/\B(?=(\d{3})+(?!\d))/g, "[,\\s]?");
  return new RegExp(`${grouped}(?!\\d)`);
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
    it("labels each template model's token input and shows its stored value", () => {
      renderTemplateDialog(
        templateWith([
          { upstream_model: "model-a", local_model: "model-a", enabled: true, context_window: 128000 },
          { upstream_model: "model-b", local_model: "model-b", enabled: true },
        ]),
      );

      expect(contextInput(0)).toHaveValue("128000");
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

      for (const invalid of ["0", "-5", "1.5", "4294967296", "abc"]) {
        await user.clear(contextInput(0));
        await user.type(contextInput(0), invalid);
        fireEvent.click(screen.getByTestId("template-edit-save-btn"));
        expect(onSave).not.toHaveBeenCalled();
        expect(contextInput(0)).toHaveValue(invalid);
        expect(screen.getByText(/context window|context tokens/i)).toBeInTheDocument();
      }
    });

    it("writes nothing when the operator cancels", async () => {
      const user = userEvent.setup();
      const { onSave, onOpenChange } = renderTemplateDialog(
        templateWith([
          { upstream_model: "model-a", local_model: "model-a", enabled: true, context_window: 1000 },
        ]),
      );

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
    it("edits an unbound mapping's context and submits it", async () => {
      const user = userEvent.setup();
      const { onSave } = renderProviderDialog(
        providerWith([
          { local_model: "local-a", upstream_model: "upstream-a", context_window: 64000 },
        ]),
      );

      expect(contextInput(0)).toHaveValue("64000");
      await user.clear(contextInput(0));
      await user.type(contextInput(0), "131072");
      fireEvent.click(screen.getByRole("button", { name: "Save" }));

      expect(onSave).toHaveBeenCalledTimes(1);
      const draft = onSave.mock.calls[0][0] as GatewayUpstreamProvider;
      expect((draft.mappings[0] as ContextMapping).context_window).toBe(131072);
    });

    it("clears an unbound mapping with a blank input", async () => {
      const user = userEvent.setup();
      const { onSave } = renderProviderDialog(
        providerWith([
          { local_model: "local-a", upstream_model: "upstream-a", context_window: 64000 },
        ]),
      );

      await user.clear(contextInput(0));
      fireEvent.click(screen.getByRole("button", { name: "Save" }));

      expect(onSave).toHaveBeenCalledTimes(1);
      const mapping = (onSave.mock.calls[0][0] as GatewayUpstreamProvider)
        .mappings[0] as ContextMapping;
      expect(mapping.context_window ?? null).toBeNull();
    });

    it("shows a bound mapping's inherited template value read-only", () => {
      const template = templateWith([
        { upstream_model: "upstream-a", local_model: "local-a", enabled: true, context_window: 200000 },
      ]);
      renderProviderDialog(
        providerWith(
          [
            { local_model: "local-a", upstream_model: "upstream-a", context_window: 1 },
            { local_model: "local-b", upstream_model: "upstream-b" },
          ],
          { template_id: template.id },
        ),
        { templates: [templateView(template)] },
      );

      expect(contextInput(0)).toHaveValue("200000");
      const inherited = contextInput(0) as HTMLInputElement;
      expect(inherited.readOnly || inherited.disabled).toBe(true);
      expect(
        screen.getByText(/inherited from.*Context Template/i),
      ).toBeInTheDocument();
      expect(contextInput(1)).toHaveValue("");
      expect(
        screen.getByText(/template model.*(missing|not found)|unspecified/i),
      ).toBeInTheDocument();
    });
  });

  describe("model list", () => {
    it("shows each source value and the conservative aggregate", () => {
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
      expect(within(upstreams[0]).getByText(digitPattern(128000))).toBeInTheDocument();
      expect(within(upstreams[1]).getByText(digitPattern(256000))).toBeInTheDocument();
      expect(within(row!).getByText(/conservative|effective context/i)).toBeInTheDocument();
      expect(within(row!).getAllByText(digitPattern(128000)).length).toBeGreaterThan(0);
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
      expect(within(row!).getAllByText(digitPattern(128000)).length).toBeGreaterThan(0);
      expect(within(row!).queryByRole("textbox")).not.toBeInTheDocument();
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
    it("describes OpenCode per-model coverage and unknown counts", async () => {
      renderTerminalPanel([
        target("opencode", {
          mode: "per_model",
          scope: "generated_models",
          coverage: { total: 3, known: 2, unknown: 1 },
          reasons: ["unknown_context"],
        }),
      ]);

      const row = screen.getByTestId("ai-gateway-target-opencode");
      expect(within(row).getByText(/generated models?/i)).toBeInTheDocument();
      expect(within(row).getByText(/2\s*(of|\/)\s*3/i)).toBeInTheDocument();
      expect(within(row).getByText(/unknown context|1 unknown/i)).toBeInTheDocument();
    });

    it("shows Codex's selected model, applied value and manual activation scope", () => {
      renderTerminalPanel([
        target("codex", {
          mode: "selected_model",
          scope: "activation",
          coverage: { total: 1, known: 1, unknown: 0 },
          selected_model: "gpt-5",
          raw_context: 400000,
          applied_context: 400000,
          reasons: [],
        }),
      ]);

      const row = screen.getByTestId("ai-gateway-target-codex");
      expect(within(row).getByText(/selected model/i)).toBeInTheDocument();
      expect(within(row).getByText("gpt-5")).toBeInTheDocument();
      expect(within(row).getByText(digitPattern(400000))).toBeInTheDocument();
      expect(within(row).getByText(/manual|activation/i)).toBeInTheDocument();
      expect(
        within(row).getByText(/does not activate|not automatically activate/i),
      ).toBeInTheDocument();
    });

    it("marks an unknown Codex context without claiming an applied value", () => {
      renderTerminalPanel([
        target("codex", {
          mode: "selected_model",
          scope: "activation",
          coverage: { total: 1, known: 0, unknown: 1 },
          selected_model: "gpt-5-unknown",
          raw_context: null,
          applied_context: null,
          reasons: ["unknown_context"],
        }),
      ]);

      const row = screen.getByTestId("ai-gateway-target-codex");
      expect(within(row).getByText("gpt-5-unknown")).toBeInTheDocument();
      expect(within(row).getByText(/unknown context/i)).toBeInTheDocument();
      expect(within(row).getByText(/manual|activation/i)).toBeInTheDocument();
      expect(within(row).queryByText(/applied/i)).not.toBeInTheDocument();
    });

    it("shows Claude's shared threshold and new-session scope", () => {
      renderTerminalPanel([
        target("claude", {
          mode: "shared_compaction",
          scope: "global_and_isolated_settings",
          coverage: { total: 3, known: 3, unknown: 0 },
          raw_context: 200000,
          applied_context: 200000,
          reasons: [],
        }),
      ]);

      const row = screen.getByTestId("ai-gateway-target-claude");
      expect(within(row).getByText(/shared/i)).toBeInTheDocument();
      expect(within(row).getAllByText(digitPattern(200000)).length).toBeGreaterThan(0);
      expect(within(row).getByText(/new sessions?/i)).toBeInTheDocument();
      expect(within(row).getByText(/running (CLI|session)/i)).toBeInTheDocument();
      expect(within(row).getByText(/does not.*(consum|apply)/i)).toBeInTheDocument();
    });

    it("discloses the Claude 1000000 cap", () => {
      renderTerminalPanel([
        target("claude", {
          mode: "shared_compaction",
          scope: "global_and_isolated_settings",
          coverage: { total: 3, known: 3, unknown: 0 },
          raw_context: 1500000,
          applied_context: 1000000,
          reasons: ["capped_at_1000000"],
        }),
      ]);

      const row = screen.getByTestId("ai-gateway-target-claude");
      expect(within(row).getByText(/capped/i)).toBeInTheDocument();
      expect(within(row).getAllByText(digitPattern(1000000)).length).toBeGreaterThan(0);
    });

    it("explains Claude's unavailable below-minimum and unknown cases", () => {
      renderTerminalPanel([
        target("claude", {
          mode: "shared_compaction",
          scope: "global_and_isolated_settings",
          coverage: { total: 3, known: 2, unknown: 1 },
          raw_context: 99999,
          applied_context: null,
          reasons: ["below_minimum", "unknown_context"],
        }),
      ]);

      const row = screen.getByTestId("ai-gateway-target-claude");
      expect(within(row).getByText(/unavailable/i)).toBeInTheDocument();
      expect(within(row).getByText(/below 100000|minimum/i)).toBeInTheDocument();
      expect(within(row).getByText(/unknown/i)).toBeInTheDocument();
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
      expect(screen.getByText(/below 100000|minimum/i)).toBeInTheDocument();
      first.unmount();

      const second = renderClaudeDialog({ "cc-a": 99999, "cc-b": 100000, "cc-c": 1000000 });
      selectEverySlot(["cc-a", "cc-b", "cc-c"], "cc-b");
      expect(screen.getByText(/shared/i)).toBeInTheDocument();
      expect(screen.getAllByText(digitPattern(100000)).length).toBeGreaterThan(0);
      second.unmount();

      const third = renderClaudeDialog({ "cc-a": 99999, "cc-b": 100000, "cc-c": 1000000 });
      selectEverySlot(["cc-a", "cc-b", "cc-c"], "cc-c");
      expect(screen.getAllByText(digitPattern(1000000)).length).toBeGreaterThan(0);
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
      expect(screen.getAllByText(digitPattern(1000000)).length).toBeGreaterThan(0);
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

      expect(contextInput(0)).toHaveValue("128000");
      expect(screen.getByText(/有效.*(覆盖|替换)/)).toBeInTheDocument();
      expect(screen.getByText(/缺失.*保留/)).toBeInTheDocument();
      expect(screen.getByText(/null.*清除/)).toBeInTheDocument();
    });

    it("renders Chinese terminal scope without claiming a running CLI consumed it", async () => {
      await i18n.changeLanguage("zh");
      renderTerminalPanel([
        target("claude", {
          mode: "shared_compaction",
          scope: "global_and_isolated_settings",
          coverage: { total: 3, known: 3, unknown: 0 },
          raw_context: 128000,
          applied_context: 128000,
          reasons: [],
        }),
      ]);

      const row = screen.getByTestId("ai-gateway-target-claude");
      expect(within(row).getByText(/共享/)).toBeInTheDocument();
      expect(within(row).getByText(/新会话/)).toBeInTheDocument();
      expect(within(row).getByText(/运行中/)).toBeInTheDocument();
    });

    it("exposes the context input through its accessible label and keyboard entry", async () => {
      const user = userEvent.setup();
      renderTemplateDialog(
        templateWith([
          { upstream_model: "model-a", local_model: "model-a", enabled: true },
        ]),
      );

      const input = contextInput(0);
      expect(input).toHaveAccessibleName(/context (window|tokens).*1|上下文窗口.*1/i);
      await user.click(input);
      await user.keyboard("4096");
      expect(input).toHaveValue("4096");
    });
  });
});
