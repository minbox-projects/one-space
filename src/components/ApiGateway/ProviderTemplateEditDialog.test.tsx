import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ProviderTemplateEditDialog } from "./ProviderTemplateEditDialog";
import type {
  GatewayProviderTemplate,
  GatewayUpstreamProvider,
} from "@/lib/apiGateway";

const baseTemplate: GatewayProviderTemplate = {
  id: "tpl-1",
  name: "Test Template",
  description: "Description of test template",
  base_url: "https://test.example.com/v1",
  protocol: "chat_completions",
  source: "https://source.com",
  models_url: "https://test.example.com/v1/models",
  models: [
    {
      upstream_model: "test-model-1",
      display_name: "Test Model 1",
      protocol: "chat_completions",
      enabled: true,
    },
    {
      upstream_model: "gpt-4o",
      display_name: "GPT-4o",
      protocol: "chat_completions",
      enabled: true,
    },
  ],
};

const REMOVED_FETCH_TESTIDS = [
  "template-edit-fetch-models-btn",
  "template-edit-fetch-panel",
  "template-edit-fetch-api-key",
  "template-edit-fetch-start-btn",
  "template-edit-import-fetched-btn",
] as const;

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

function renderDialog(overrides: {
  template?: GatewayProviderTemplate | null;
  providers?: GatewayUpstreamProvider[];
  onSave?: ReturnType<typeof vi.fn>;
  onDelete?: ReturnType<typeof vi.fn>;
} = {}) {
  const onSave = overrides.onSave ?? vi.fn().mockResolvedValue(true);
  const onDelete = overrides.onDelete ?? vi.fn().mockResolvedValue(true);
  render(
    <ProviderTemplateEditDialog
      open={true}
      onOpenChange={vi.fn()}
      template={overrides.template === undefined ? baseTemplate : overrides.template}
      providers={overrides.providers ?? []}
      busy={false}
      onSave={onSave}
      onDelete={onDelete}
    />,
  );
  return { onSave, onDelete };
}

describe("ProviderTemplateEditDialog", () => {
  it("renders base fields and a manual price-free model list", () => {
    renderDialog();

    expect(screen.getByTestId("template-edit-name")).toHaveValue("Test Template");
    expect(screen.getByTestId("template-edit-protocol")).toHaveValue("chat_completions");
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
    expect(screen.getByTestId("template-edit-models-list")).toBeInTheDocument();
    expect(screen.getByTestId("template-edit-add-model")).toBeInTheDocument();

    // 无任何价格输入
    expect(document.querySelectorAll('input[type="number"]')).toHaveLength(0);
    for (const label of ["$/1M tokens", "Input", "Cache read", "Cache write", "Output"]) {
      expect(
        screen.queryAllByText(label),
        `${label} 价签不应出现`,
      ).toHaveLength(0);
    }

    // 无获取远端模型面板及其入口
    for (const testId of REMOVED_FETCH_TESTIDS) {
      expect(screen.queryByTestId(testId), `${testId} 不应存在`).not.toBeInTheDocument();
    }
  });

  it("edits model names and toggles the enabled state per row", () => {
    renderDialog();

    const upstream0 = screen.getByTestId("template-edit-model-upstream-0");
    const display0 = screen.getByTestId("template-edit-model-display-0");
    expect(upstream0).toHaveValue("test-model-1");
    expect(display0).toHaveValue("Test Model 1");

    fireEvent.change(upstream0, { target: { value: "renamed-model" } });
    fireEvent.change(display0, { target: { value: "Renamed" } });
    expect(upstream0).toHaveValue("renamed-model");
    expect(display0).toHaveValue("Renamed");

    const enabled0 = screen.getByTestId("template-edit-model-enabled-0");
    expect(enabled0).toBeChecked();
    fireEvent.click(enabled0);
    expect(enabled0).not.toBeChecked();
    expect(
      screen.getByTestId("template-edit-model-row-0").getAttribute("data-disabled"),
      "禁用的编辑行应带 data-disabled=true",
    ).toBe("true");
  });

  it("adds a default-enabled row and removes rows", () => {
    renderDialog();

    expect(screen.queryByTestId("template-edit-model-row-2")).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("template-edit-add-model"));

    const added = screen.getByTestId("template-edit-model-enabled-2");
    expect(added, "新增模型行应默认启用").toBeChecked();
    expect(screen.getByTestId("template-edit-models-count")).toHaveTextContent("3");

    fireEvent.click(screen.getByTestId("template-edit-remove-model-1"));

    expect(screen.getByTestId("template-edit-models-count")).toHaveTextContent("2");
    expect(screen.getByTestId("template-edit-model-row-1")).toBeInTheDocument();
    expect(screen.queryByTestId("template-edit-model-row-2")).not.toBeInTheDocument();
  });

  it("filters models list by search keyword", () => {
    renderDialog();

    expect(screen.getByDisplayValue("test-model-1")).toBeInTheDocument();
    expect(screen.getByDisplayValue("gpt-4o")).toBeInTheDocument();

    const searchInput = screen.getByTestId("template-edit-models-search");
    fireEvent.change(searchInput, { target: { value: "gpt" } });

    expect(screen.queryByDisplayValue("test-model-1")).not.toBeInTheDocument();
    expect(screen.getByDisplayValue("gpt-4o")).toBeInTheDocument();
  });

  it("saves the complete model list with enabled flags and no snapshot_version", async () => {
    const { onSave } = renderDialog();

    fireEvent.change(screen.getByTestId("template-edit-model-display-0"), {
      target: { value: "Renamed" },
    });
    // 新增一行保持 upstream 空白，保存时应被过滤
    fireEvent.click(screen.getByTestId("template-edit-add-model"));
    // 关闭第 2 行的启用开关
    fireEvent.click(screen.getByTestId("template-edit-model-enabled-1"));

    await act(async () => {
      fireEvent.click(screen.getByTestId("template-edit-save-btn"));
    });

    expect(onSave).toHaveBeenCalledTimes(1);
    const saved = onSave.mock.calls[0][0] as GatewayProviderTemplate;
    expect(saved).not.toHaveProperty("snapshot_version");
    expect(saved.models).toHaveLength(2);
    expect(saved.models.map((m) => m.upstream_model)).toEqual([
      "test-model-1",
      "gpt-4o",
    ]);
    expect(saved.models[0]).toMatchObject({
      upstream_model: "test-model-1",
      display_name: "Renamed",
      enabled: true,
    });
    expect(saved.models[1]).toMatchObject({
      upstream_model: "gpt-4o",
      enabled: false,
    });
    for (const model of saved.models) {
      expect(model).not.toHaveProperty("snapshot_version");
      expect(typeof model.enabled).toBe("boolean");
    }
  });

  it("disables delete button and shows warning if template is in use by a provider", () => {
    const providers = [
      makeProvider({ id: "p-used", name: "In Use Provider", template_id: "tpl-1" }),
    ];

    renderDialog({ providers });

    const deleteBtn = screen.getByTestId("template-edit-delete-btn");
    expect(deleteBtn).toBeDisabled();

    const warning = screen.getByTestId("template-edit-in-use-warning");
    expect(warning).toBeInTheDocument();
    expect(warning.textContent).toContain("In Use Provider");
  });

  it("enables delete and invokes onDelete after confirmation when template is NOT in use", async () => {
    const onDelete = vi.fn().mockResolvedValue(true);
    const { onDelete: deleteFn } = renderDialog({ onDelete });

    const deleteBtn = screen.getByTestId("template-edit-delete-btn");
    expect(deleteBtn).toBeEnabled();

    // First click prompts for confirmation
    await act(async () => {
      fireEvent.click(deleteBtn);
    });
    expect(deleteFn).not.toHaveBeenCalled();

    // Second click confirms
    await act(async () => {
      fireEvent.click(deleteBtn);
    });
    expect(deleteFn).toHaveBeenCalledWith("tpl-1");
  });

  it("submits updated template data with models_url on save", async () => {
    const { onSave } = renderDialog();

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
    const saved = onSave.mock.calls[0][0] as GatewayProviderTemplate;
    expect(saved).not.toHaveProperty("snapshot_version");
  });

  it("validates required fields before calling onSave", () => {
    const { onSave } = renderDialog();

    fireEvent.change(screen.getByTestId("template-edit-name"), {
      target: { value: "   " },
    });

    fireEvent.click(screen.getByTestId("template-edit-save-btn"));

    expect(onSave).not.toHaveBeenCalled();
  });
});
