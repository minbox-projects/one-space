import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { ProviderTemplatePickerDialog } from "./ProviderTemplatePickerDialog";
import type { GatewayProviderTemplateView } from "@/lib/apiGateway";

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
    expect(screen.getByTestId("template-picker-edit-tpl-zen")).toBeInTheDocument();
  });

  it("renders model count and source without the offline snapshot badge", () => {
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
    expect(screen.queryByText(i18n.t("apiGatewayTemplateSnapshot"))).not.toBeInTheDocument();
    expect(screen.getByText(/1 models/)).toBeInTheDocument();
    expect(screen.getByText(/opencode\.ai\/zen\/v1\/models/)).toBeInTheDocument();
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

  it("triggers onSelectTemplate when clicking template card body", () => {
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

    fireEvent.click(screen.getByTestId("template-picker-select-tpl-zen"));
    expect(onSelectTemplate).toHaveBeenCalledWith(mockTemplates[0].template);
  });

  it("triggers onEditTemplate when clicking pencil edit icon", () => {
    const onEditTemplate = vi.fn();
    render(
      <ProviderTemplatePickerDialog
        open={true}
        onOpenChange={vi.fn()}
        templates={mockTemplates}
        providers={[]}
        busy={false}
        onSelectBlank={vi.fn()}
        onSelectTemplate={vi.fn()}
        onEditTemplate={onEditTemplate}
      />,
    );

    fireEvent.click(screen.getByTestId("template-picker-edit-tpl-zen"));
    expect(onEditTemplate).toHaveBeenCalledWith(mockTemplates[0].template);
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
});
