import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ProviderTemplateEditDialog } from "./ProviderTemplateEditDialog";
import {
  apiGatewayFetchModels,
  type GatewayProviderTemplate,
  type GatewayUpstreamProvider,
} from "@/lib/apiGateway";

vi.mock("@/lib/apiGateway", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/apiGateway")>();
  return {
    ...actual,
    apiGatewayFetchModels: vi.fn(),
  };
});

const baseTemplate: GatewayProviderTemplate = {
  id: "tpl-1",
  name: "Test Template",
  description: "Description of test template",
  base_url: "https://test.example.com/v1",
  protocol: "chat_completions",
  source: "https://source.com",
  snapshot_version: "1",
  models_url: "https://test.example.com/v1/models",
  models: [
    {
      upstream_model: "test-model-1",
      display_name: "Test Model 1",
      input: 0.5,
      cache_read: 0,
      cache_write: 0,
      output: 1.5,
    },
    {
      upstream_model: "gpt-4o",
      display_name: "GPT-4o",
      input: 2.5,
      cache_read: 0,
      cache_write: 0,
      output: 10,
    },
  ],
};

function makeProvider(overrides: Partial<GatewayUpstreamProvider> = {}): GatewayUpstreamProvider {
  return {
    id: "p1",
    name: "My Provider",
    base_url: "https://api.com",
    api_key: "sk-test",
    default_model: null,
    protocol: "chat_completions",
    mappings: [],
    enabled: true,
    auto_disabled: false,
    disabled_reason: null,
    disabled_at: null,
    consecutive_failures: 0,
    last_error_at: null,
    ...overrides,
  };
}

describe("ProviderTemplateEditDialog", () => {
  it("renders template fields and models_url correctly", () => {
    render(
      <ProviderTemplateEditDialog
        open={true}
        onOpenChange={vi.fn()}
        template={baseTemplate}
        providers={[]}
        busy={false}
        onSave={vi.fn()}
        onDelete={vi.fn()}
      />,
    );

    expect(screen.getByTestId("template-edit-name")).toHaveValue("Test Template");
    expect(screen.getByTestId("template-edit-base-url")).toHaveValue(
      "https://test.example.com/v1",
    );
    expect(screen.getByTestId("template-edit-models-url")).toHaveValue(
      "https://test.example.com/v1/models",
    );
    expect(screen.getByTestId("template-edit-description")).toHaveValue(
      "Description of test template",
    );
    expect(screen.getByTestId("template-edit-models-count")).toHaveTextContent("2");
  });

  it("filters models list by search keyword", () => {
    render(
      <ProviderTemplateEditDialog
        open={true}
        onOpenChange={vi.fn()}
        template={baseTemplate}
        providers={[]}
        busy={false}
        onSave={vi.fn()}
        onDelete={vi.fn()}
      />,
    );

    expect(screen.getByDisplayValue("test-model-1")).toBeInTheDocument();
    expect(screen.getByDisplayValue("gpt-4o")).toBeInTheDocument();

    const searchInput = screen.getByTestId("template-edit-models-search");
    fireEvent.change(searchInput, { target: { value: "gpt" } });

    expect(screen.queryByDisplayValue("test-model-1")).not.toBeInTheDocument();
    expect(screen.getByDisplayValue("gpt-4o")).toBeInTheDocument();
  });

  it("toggles fetch models panel, fetches and imports new models", async () => {
    vi.mocked(apiGatewayFetchModels).mockResolvedValue([
      "test-model-1", // already present
      "new-remote-model-2", // new
    ]);

    render(
      <ProviderTemplateEditDialog
        open={true}
        onOpenChange={vi.fn()}
        template={baseTemplate}
        providers={[]}
        busy={false}
        onSave={vi.fn()}
        onDelete={vi.fn()}
      />,
    );

    const toggleBtn = screen.getByTestId("template-edit-fetch-models-btn");
    fireEvent.click(toggleBtn);

    expect(screen.getByTestId("template-edit-fetch-panel")).toBeInTheDocument();

    const startBtn = screen.getByTestId("template-edit-fetch-start-btn");
    await act(async () => {
      fireEvent.click(startBtn);
    });

    expect(apiGatewayFetchModels).toHaveBeenCalledWith(
      "https://test.example.com/v1/models",
      "",
    );

    const importBtn = screen.getByTestId("template-edit-import-fetched-btn");
    await act(async () => {
      fireEvent.click(importBtn);
    });

    expect(screen.getByTestId("template-edit-models-count")).toHaveTextContent("3");
    expect(screen.getByDisplayValue("new-remote-model-2")).toBeInTheDocument();
  });

  it("disables delete button and shows warning if template is in use by a provider", () => {
    const providers = [
      makeProvider({ id: "p-used", name: "In Use Provider", template_id: "tpl-1" }),
    ];

    render(
      <ProviderTemplateEditDialog
        open={true}
        onOpenChange={vi.fn()}
        template={baseTemplate}
        providers={providers}
        busy={false}
        onSave={vi.fn()}
        onDelete={vi.fn()}
      />,
    );

    const deleteBtn = screen.getByTestId("template-edit-delete-btn");
    expect(deleteBtn).toBeDisabled();

    const warning = screen.getByTestId("template-edit-in-use-warning");
    expect(warning).toBeInTheDocument();
    expect(warning.textContent).toContain("In Use Provider");
  });

  it("enables delete and invokes onDelete after confirmation when template is NOT in use", async () => {
    const onDelete = vi.fn().mockResolvedValue(true);
    const onOpenChange = vi.fn();

    render(
      <ProviderTemplateEditDialog
        open={true}
        onOpenChange={onOpenChange}
        template={baseTemplate}
        providers={[]}
        busy={false}
        onSave={vi.fn()}
        onDelete={onDelete}
      />,
    );

    const deleteBtn = screen.getByTestId("template-edit-delete-btn");
    expect(deleteBtn).toBeEnabled();

    // First click prompts for confirmation
    await act(async () => {
      fireEvent.click(deleteBtn);
    });
    expect(onDelete).not.toHaveBeenCalled();

    // Second click confirms
    await act(async () => {
      fireEvent.click(deleteBtn);
    });
    expect(onDelete).toHaveBeenCalledWith("tpl-1");
  });

  it("submits updated template data with models_url on save", async () => {
    const onSave = vi.fn().mockResolvedValue(true);
    const onOpenChange = vi.fn();

    render(
      <ProviderTemplateEditDialog
        open={true}
        onOpenChange={onOpenChange}
        template={baseTemplate}
        providers={[]}
        busy={false}
        onSave={onSave}
        onDelete={vi.fn()}
      />,
    );

    fireEvent.change(screen.getByTestId("template-edit-name"), {
      target: { value: "Updated Template Name" },
    });
    fireEvent.change(screen.getByTestId("template-edit-models-url"), {
      target: { value: "https://custom.com/v1/models" },
    });

    await act(async () => {
      fireEvent.click(screen.getByTestId("template-edit-save-btn"));
    });

    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        id: "tpl-1",
        name: "Updated Template Name",
        models_url: "https://custom.com/v1/models",
      }),
    );
  });

  it("validates required fields before calling onSave", () => {
    const onSave = vi.fn();

    render(
      <ProviderTemplateEditDialog
        open={true}
        onOpenChange={vi.fn()}
        template={baseTemplate}
        providers={[]}
        busy={false}
        onSave={onSave}
        onDelete={vi.fn()}
      />,
    );

    fireEvent.change(screen.getByTestId("template-edit-name"), {
      target: { value: "   " },
    });

    fireEvent.click(screen.getByTestId("template-edit-save-btn"));

    expect(onSave).not.toHaveBeenCalled();
  });
});
