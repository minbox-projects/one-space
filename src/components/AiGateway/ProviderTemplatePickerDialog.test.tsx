import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { ProviderTemplatePickerDialog } from "./ProviderTemplatePickerDialog";
import type { GatewayProviderTemplateView } from "@/lib/aiGateway";

const mockTemplates: GatewayProviderTemplateView[] = [
  {
    template: {
      id: "tpl-zen",
      name: "OpenCode Zen",
      description: "Official OpenCode Zen models",
      base_url: "https://opencode.ai/zen/v1",
      protocol: "chat_completions",
      source: "https://opencode.ai/zen/v1/models",
      models_url: "https://opencode.ai/zen/v1/models",
      models: [
        {
          upstream_model: "zen-model-1",
          display_name: "Zen Model 1",
          protocol: "chat_completions",
          enabled: true,
        },
      ],
    },
    synced_at: 1_700_000_000,
    from_snapshot: true,
    source: "https://opencode.ai/zen/v1/models",
  },
];

describe("ProviderTemplatePickerDialog", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("renders manual creation and templates list", () => {
    render(
      <ProviderTemplatePickerDialog
        open={true}
        onOpenChange={vi.fn()}
        templates={mockTemplates}
        providers={[]}
        busy={false}
        onSelectBlank={vi.fn()}
        onSelectTemplate={vi.fn()}
        onEditTemplate={vi.fn()}
        onNewTemplate={vi.fn()}
      />,
    );

    expect(screen.getByTestId("template-picker-blank-btn")).toBeInTheDocument();
    expect(screen.getByText("OpenCode Zen")).toBeInTheDocument();
    expect(screen.getByTestId("template-picker-item-tpl-zen")).toBeInTheDocument();
    expect(screen.queryByTestId("template-picker-edit-tpl-zen")).not.toBeInTheDocument();
  });

  it("renders model count and source without duplicate numbers and without truncation", () => {
    render(
      <ProviderTemplatePickerDialog
        open={true}
        onOpenChange={vi.fn()}
        templates={mockTemplates}
        providers={[]}
        busy={false}
        onSelectBlank={vi.fn()}
        onSelectTemplate={vi.fn()}
        onEditTemplate={vi.fn()}
      />,
    );

    expect(screen.queryByText("Offline snapshot")).not.toBeInTheDocument();
    expect(screen.queryByText(i18n.t("aiGatewayTemplateSnapshot"))).not.toBeInTheDocument();
    expect(screen.getByText("Provider Templates")).toBeInTheDocument();
    expect(screen.getByText("1 models")).toBeInTheDocument();
    const sourceElement = screen.getByText(/opencode\.ai\/zen\/v1\/models/);
    expect(sourceElement).toBeInTheDocument();
    expect(sourceElement.className).toContain("break-all");
    expect(sourceElement.className).not.toContain("truncate");
  });

  it("renders non-duplicate model count and full source in Chinese locale", async () => {
    await i18n.changeLanguage("zh");
    render(
      <ProviderTemplatePickerDialog
        open={true}
        onOpenChange={vi.fn()}
        templates={mockTemplates}
        providers={[]}
        busy={false}
        onSelectBlank={vi.fn()}
        onSelectTemplate={vi.fn()}
        onEditTemplate={vi.fn()}
      />,
    );

    expect(screen.getByText("服务商模板")).toBeInTheDocument();
    expect(screen.getByText("1 个模型")).toBeInTheDocument();
    const sourceElement = screen.getByText(/opencode\.ai\/zen\/v1\/models/);
    expect(sourceElement).toBeInTheDocument();
    expect(sourceElement.className).toContain("break-all");
    expect(sourceElement.className).not.toContain("truncate");
  });

  it("triggers onSelectBlank when clicking manual creation card", () => {
    const onSelectBlank = vi.fn();
    render(
      <ProviderTemplatePickerDialog
        open={true}
        onOpenChange={vi.fn()}
        templates={mockTemplates}
        providers={[]}
        busy={false}
        onSelectBlank={onSelectBlank}
        onSelectTemplate={vi.fn()}
        onEditTemplate={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByTestId("template-picker-blank-btn"));
    expect(onSelectBlank).toHaveBeenCalledTimes(1);
  });

  it("triggers onSelectTemplate when clicking template card", () => {
    const onSelectTemplate = vi.fn();
    render(
      <ProviderTemplatePickerDialog
        open={true}
        onOpenChange={vi.fn()}
        templates={mockTemplates}
        providers={[]}
        busy={false}
        onSelectBlank={vi.fn()}
        onSelectTemplate={onSelectTemplate}
        onEditTemplate={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByTestId("template-picker-item-tpl-zen"));
    expect(onSelectTemplate).toHaveBeenCalledWith(mockTemplates[0].template);
  });

  it("does not render pencil edit button on template cards and has matching action cue", () => {
    render(
      <ProviderTemplatePickerDialog
        open={true}
        onOpenChange={vi.fn()}
        templates={mockTemplates}
        providers={[]}
        busy={false}
        onSelectBlank={vi.fn()}
        onSelectTemplate={vi.fn()}
        onEditTemplate={vi.fn()}
      />,
    );

    expect(screen.queryByTestId("template-picker-edit-tpl-zen")).not.toBeInTheDocument();
    const templateCard = screen.getByTestId("template-picker-item-tpl-zen");
    expect(within(templateCard).getByText("Use template")).toBeInTheDocument();
  });

  it("triggers onNewTemplate when clicking new template button", () => {
    const onNewTemplate = vi.fn();
    render(
      <ProviderTemplatePickerDialog
        open={true}
        onOpenChange={vi.fn()}
        templates={mockTemplates}
        providers={[]}
        busy={false}
        onSelectBlank={vi.fn()}
        onSelectTemplate={vi.fn()}
        onEditTemplate={vi.fn()}
        onNewTemplate={onNewTemplate}
      />,
    );

    fireEvent.click(screen.getByTestId("template-picker-new-btn"));
    expect(onNewTemplate).toHaveBeenCalledTimes(1);
  });

  it("renders corresponding provider icon for template items in list", () => {
    const multiTemplates: GatewayProviderTemplateView[] = [
      {
        template: {
          id: "tpl-zen",
          name: "OpenCode Zen",
          description: "OpenCode Zen",
          base_url: "https://opencode.ai/zen/v1",
          protocol: "chat_completions",
          source: "",
          models: [],
          icon: "opencode",
        },
        synced_at: null,
        from_snapshot: true,
        source: "",
      },
      {
        template: {
          id: "tpl-cmd",
          name: "CommandCode",
          description: "CommandCode",
          base_url: "https://api.commandcode.ai/provider/v1",
          protocol: "chat_completions",
          source: "",
          models: [],
          icon: "commandcode",
        },
        synced_at: null,
        from_snapshot: true,
        source: "",
      },
    ];

    render(
      <ProviderTemplatePickerDialog
        open={true}
        onOpenChange={vi.fn()}
        templates={multiTemplates}
        providers={[]}
        busy={false}
        onSelectBlank={vi.fn()}
        onSelectTemplate={vi.fn()}
      />,
    );

    const zenItem = screen.getByTestId("template-picker-item-tpl-zen");
    expect(within(zenItem).getByTestId("provider-icon-opencode")).toBeInTheDocument();

    const cmdItem = screen.getByTestId("template-picker-item-tpl-cmd");
    expect(within(cmdItem).getByTestId("provider-icon-commandcode")).toBeInTheDocument();
  });
});
