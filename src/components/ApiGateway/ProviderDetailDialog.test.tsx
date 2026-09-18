import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { ProviderDetailDialog } from "@/components/ApiGateway/ProviderDetailDialog";
import {
  API_GATEWAY_KEY_MASK,
  type GatewayUpstreamProvider,
} from "@/lib/apiGateway";
import { renderWithProviders } from "@/test/mocks/render";

function makeProvider(
  overrides: Partial<GatewayUpstreamProvider> = {},
): GatewayUpstreamProvider {
  return {
    id: "p1",
    name: "Upstream A",
    base_url: "https://api.a.example",
    api_key: API_GATEWAY_KEY_MASK,
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

describe("ProviderDetailDialog 模型映射", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("映射行可编辑本地模型名称并随保存提交", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const onOpenChange = vi.fn();
    const provider = makeProvider({
      mappings: [
        {
          local_model: "gpt-4o",
          upstream_model: "gpt-4o-2024",
          display_name: "GPT-4o",
          protocol: null,
        },
      ],
    });

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={onSave}
        onDelete={vi.fn()}
        onOpenChange={onOpenChange}
      />,
    );

    const displayNameInput = screen.getByRole("textbox", {
      name: "Local model name 1",
    });
    expect(displayNameInput).toHaveValue("GPT-4o");

    await user.clear(displayNameInput);
    await user.type(displayNameInput, "GPT-4o Custom");
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledTimes(1);
    const saved = onSave.mock.calls[0][0] as GatewayUpstreamProvider;
    expect(saved.mappings[0]).toMatchObject({
      local_model: "gpt-4o",
      upstream_model: "gpt-4o-2024",
      display_name: "GPT-4o Custom",
    });
  });

  it("清空本地模型名称时保存为 undefined 且不改动其他映射字段", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const provider = makeProvider({
      mappings: [
        {
          local_model: "gpt-4o",
          upstream_model: "gpt-4o-2024",
          display_name: "GPT-4o",
          protocol: null,
        },
      ],
    });

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={onSave}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );

    const displayNameInput = screen.getByRole("textbox", {
      name: "Local model name 1",
    });
    await user.clear(displayNameInput);
    expect(displayNameInput).toHaveValue("");

    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledTimes(1);
    const saved = onSave.mock.calls[0][0] as GatewayUpstreamProvider;
    expect(saved.mappings[0].local_model).toBe("gpt-4o");
    expect(saved.mappings[0].upstream_model).toBe("gpt-4o-2024");
    expect(saved.mappings[0].display_name).toBeUndefined();
  });

  it("新增映射行包含空的本地模型名称输入", async () => {
    const user = userEvent.setup();
    const provider = makeProvider({ mappings: [] });

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("button", { name: /Add mapping/ }));

    const displayNameInput = screen.getByRole("textbox", {
      name: "Local model name 1",
    });
    expect(displayNameInput).toHaveValue("");
  });

  it("上游服务商表单不提供模型价格录入（价格由用量页独立弹窗负责）", () => {
    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={makeProvider({
          mappings: [
            {
              local_model: "gpt-4o",
              upstream_model: "gpt-4o-2024",
              display_name: "GPT-4o",
              protocol: null,
            },
          ],
        })}
        busy={false}
        onSave={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );

    expect(screen.queryByText(/model prices?/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/模型价格/)).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /price/i }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("textbox", { name: /price/i }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("spinbutton", { name: /price/i }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("combobox", { name: /price/i }),
    ).not.toBeInTheDocument();
  });

  it("映射行开关反映存储状态、关闭后保存并重开仍为禁用", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const provider = makeProvider({
      mappings: [
        { local_model: "local-a", upstream_model: "remote-a", enabled: true },
        { local_model: "local-b", upstream_model: "remote-b", enabled: false },
      ],
    });

    const { unmount } = renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={onSave}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );

    const enabledSwitch = screen.getByRole("switch", {
      name: "Enable mapping 1",
    });
    const disabledSwitch = screen.getByRole("switch", {
      name: "Enable mapping 2",
    });
    expect(enabledSwitch, "第 1 行启用映射开关应为开启").toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(disabledSwitch, "第 2 行禁用映射开关应为关闭").toHaveAttribute(
      "aria-checked",
      "false",
    );

    await user.click(enabledSwitch);
    expect(enabledSwitch, "点击后第 1 行开关应变为关闭").toHaveAttribute(
      "aria-checked",
      "false",
    );

    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledTimes(1);
    const saved = onSave.mock.calls[0][0] as GatewayUpstreamProvider;
    expect(saved.mappings[0].enabled, "保存载荷第 1 条映射应为禁用").toBe(false);
    expect(saved.mappings[1].enabled, "保存载荷第 2 条映射应为禁用").toBe(false);

    unmount();
    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={saved}
        busy={false}
        onSave={vi.fn()}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );
    expect(
      screen.getByRole("switch", { name: "Enable mapping 1" }),
      "重开后第 1 行开关应保持关闭",
    ).toHaveAttribute("aria-checked", "false");
    expect(
      screen.getByRole("switch", { name: "Enable mapping 2" }),
      "重开后第 2 行开关应保持关闭",
    ).toHaveAttribute("aria-checked", "false");
  });

  it("禁用映射行带 data-disabled 与弱化样式", () => {
    const provider = makeProvider({
      mappings: [
        { local_model: "local-a", upstream_model: "remote-a", enabled: true },
        { local_model: "local-b", upstream_model: "remote-b", enabled: false },
      ],
    });

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={vi.fn()}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );

    const disabledSwitch = screen.getByRole("switch", {
      name: "Enable mapping 2",
    });
    const disabledRow = disabledSwitch.closest("li");
    expect(disabledRow).not.toBeNull();
    expect(
      disabledRow!.getAttribute("data-disabled"),
      "禁用映射行应带 data-disabled=true",
    ).toBe("true");
    expect(
      disabledRow!.className,
      "禁用映射行应带弱化样式 opacity-60",
    ).toContain("opacity-60");

    const enabledSwitch = screen.getByRole("switch", {
      name: "Enable mapping 1",
    });
    const enabledRow = enabledSwitch.closest("li");
    expect(enabledRow).not.toBeNull();
    expect(
      enabledRow!.getAttribute("data-disabled"),
      "启用映射行不应带 data-disabled",
    ).toBeNull();
  });

  it("新增映射行默认启用", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const provider = makeProvider({ mappings: [] });

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={onSave}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("button", { name: /Add mapping/ }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(onSave).toHaveBeenCalledTimes(1);
    const saved = onSave.mock.calls[0][0] as GatewayUpstreamProvider;
    expect(saved.mappings).toHaveLength(1);
    expect(saved.mappings[0].enabled, "新增映射行应默认启用").toBe(true);
  });

  it("缺省 enabled 的既有映射行开关为启用", () => {
    const provider = makeProvider({
      mappings: [{ local_model: "local-a", upstream_model: "remote-a" }],
    });

    renderWithProviders(
      <ProviderDetailDialog
        open
        provider={provider}
        busy={false}
        onSave={vi.fn()}
        onDelete={vi.fn()}
        onOpenChange={vi.fn()}
      />,
    );

    expect(
      screen.getByRole("switch", { name: "Enable mapping 1" }),
      "缺省 enabled 的既有映射行开关应视为启用",
    ).toHaveAttribute("aria-checked", "true");
  });
});
