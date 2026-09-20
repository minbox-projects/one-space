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

    expect(screen.getByTestId("template-edit-model-upstream-0")).toHaveValue("test-model-1");
    expect(screen.getByTestId("template-edit-model-upstream-1")).toHaveValue("gpt-4o");

    const searchInput = screen.getByTestId("template-edit-models-search");
    fireEvent.change(searchInput, { target: { value: "gpt" } });

    expect(screen.queryByTestId("template-edit-model-upstream-0")).not.toBeInTheDocument();
    expect(screen.getByTestId("template-edit-model-upstream-1")).toHaveValue("gpt-4o");
  });

  it("expands mapping row to edit pricing and reasoning efforts and saves them", async () => {
    const { onSave } = renderDialog();

    // 默认未展开，价格与推理档位不显示
    expect(screen.queryByTestId("api-gateway-mapping-price-0")).not.toBeInTheDocument();
    expect(screen.queryByTestId("api-gateway-mapping-efforts-0")).not.toBeInTheDocument();

    // 点击展开第 1 个模型
    fireEvent.click(screen.getByTestId("template-edit-model-expand-0"));

    expect(screen.getByTestId("api-gateway-mapping-price-0")).toBeInTheDocument();
    expect(screen.getByTestId("api-gateway-mapping-efforts-0")).toBeInTheDocument();

    // 编辑本地模型 ID
    const local0 = screen.getByTestId("template-edit-model-local-0");
    fireEvent.change(local0, { target: { value: "custom-local-1" } });

    // 编辑协议
    const protocol0 = screen.getByTestId("template-edit-model-protocol-0");
    fireEvent.change(protocol0, { target: { value: "responses" } });

    // 编辑价格 (通过 MappingPriceEditor 的输入框)
    const inputPrice = screen.getByTestId("api-gateway-price-0-input");
    const outputPrice = screen.getByTestId("api-gateway-price-0-output");
    fireEvent.change(inputPrice, { target: { value: "1.5" } });
    fireEvent.change(outputPrice, { target: { value: "3.5" } });

    // 添加推理档位
    const effortInput = screen.getByTestId("api-gateway-mapping-effort-input-0");
    fireEvent.change(effortInput, { target: { value: "medium" } });
    fireEvent.click(screen.getByTestId("api-gateway-mapping-effort-add-0"));

    expect(screen.getByTestId("api-gateway-mapping-effort-0-medium")).toBeInTheDocument();

    // 保存
    await act(async () => {
      fireEvent.click(screen.getByTestId("template-edit-save-btn"));
    });

    expect(onSave).toHaveBeenCalledTimes(1);
    const saved = onSave.mock.calls[0][0] as GatewayProviderTemplate;
    expect(saved.models[0]).toMatchObject({
      upstream_model: "test-model-1",
      local_model: "custom-local-1",
      protocol: "responses",
      input: 1.5,
      output: 3.5,
      reasoning_efforts: ["medium"],
      enabled: true,
    });
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

  it("creates a new template with model mappings, prices and protocols", async () => {
    const onSave = vi.fn().mockResolvedValue(true);
    render(
      <ProviderTemplateEditDialog
        open={true}
        onOpenChange={vi.fn()}
        template={null}
        providers={[]}
        busy={false}
        onSave={onSave}
        onDelete={vi.fn()}
      />,
    );

    expect(screen.getByTestId("template-edit-name")).toHaveValue("");
    expect(screen.getByTestId("template-edit-models-count")).toHaveTextContent("0");

    // 填写基本信息
    fireEvent.change(screen.getByTestId("template-edit-name"), {
      target: { value: "New Custom Template" },
    });
    fireEvent.change(screen.getByTestId("template-edit-base-url"), {
      target: { value: "https://new.example.com/v1" },
    });

    // 添加模型映射行
    fireEvent.click(screen.getByTestId("template-edit-add-model"));
    expect(screen.getByTestId("template-edit-model-row-0")).toBeInTheDocument();

    // 输入本地模型 ID、上游模型 ID、显示名称、协议
    fireEvent.change(screen.getByTestId("template-edit-model-local-0"), {
      target: { value: "claude-3-7-sonnet" },
    });
    fireEvent.change(screen.getByTestId("template-edit-model-upstream-0"), {
      target: { value: "claude-3-7-sonnet-20250219" },
    });
    fireEvent.change(screen.getByTestId("template-edit-model-display-0"), {
      target: { value: "Claude 3.7 Sonnet" },
    });
    fireEvent.change(screen.getByTestId("template-edit-model-protocol-0"), {
      target: { value: "chat_completions" },
    });

    // 展开并配置价格与推理档位
    fireEvent.click(screen.getByTestId("template-edit-model-expand-0"));
    fireEvent.change(screen.getByTestId("api-gateway-price-0-input"), {
      target: { value: "3" },
    });
    fireEvent.change(screen.getByTestId("api-gateway-price-0-output"), {
      target: { value: "15" },
    });

    const effortInput = screen.getByTestId("api-gateway-mapping-effort-input-0");
    fireEvent.change(effortInput, { target: { value: "high" } });
    fireEvent.click(screen.getByTestId("api-gateway-mapping-effort-add-0"));

    // 保存
    await act(async () => {
      fireEvent.click(screen.getByTestId("template-edit-save-btn"));
    });

    expect(onSave).toHaveBeenCalledTimes(1);
    const saved = onSave.mock.calls[0][0] as GatewayProviderTemplate;
    expect(saved.name).toBe("New Custom Template");
    expect(saved.base_url).toBe("https://new.example.com/v1");
    expect(saved.models).toHaveLength(1);
    expect(saved.models[0]).toMatchObject({
      local_model: "claude-3-7-sonnet",
      upstream_model: "claude-3-7-sonnet-20250219",
      display_name: "Claude 3.7 Sonnet",
      protocol: "chat_completions",
      input: 3,
      output: 15,
      reasoning_efforts: ["high"],
      enabled: true,
    });
  });

  it("initializes icon selector with template icon and updates on change", async () => {
    const templateWithIcon: GatewayProviderTemplate = {
      ...baseTemplate,
      icon: "commandcode",
    };
    const { onSave } = renderDialog({ template: templateWithIcon });

    const iconSelect = screen.getByTestId("template-edit-icon") as HTMLSelectElement;
    expect(iconSelect.value).toBe("commandcode");

    fireEvent.change(iconSelect, { target: { value: "opencode" } });
    expect(iconSelect.value).toBe("opencode");

    await act(async () => {
      fireEvent.click(screen.getByTestId("template-edit-save-btn"));
    });

    expect(onSave).toHaveBeenCalledTimes(1);
    const saved = onSave.mock.calls[0][0] as GatewayProviderTemplate;
    expect(saved.icon).toBe("opencode");
  });

  it("saves null icon when auto/empty is selected", async () => {
    const templateWithIcon: GatewayProviderTemplate = {
      ...baseTemplate,
      icon: "commandcode",
    };
    const { onSave } = renderDialog({ template: templateWithIcon });

    const iconSelect = screen.getByTestId("template-edit-icon") as HTMLSelectElement;
    fireEvent.change(iconSelect, { target: { value: "" } });

    await act(async () => {
      fireEvent.click(screen.getByTestId("template-edit-save-btn"));
    });

    expect(onSave).toHaveBeenCalledTimes(1);
    const saved = onSave.mock.calls[0][0] as GatewayProviderTemplate;
    expect(saved.icon).toBeNull();
  });
});
