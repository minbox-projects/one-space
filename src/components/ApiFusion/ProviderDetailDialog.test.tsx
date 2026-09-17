import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { ProviderDetailDialog } from "@/components/ApiFusion/ProviderDetailDialog";
import {
  API_FUSION_KEY_MASK,
  type FusionUpstreamProvider,
} from "@/lib/apiFusion";
import { renderWithProviders } from "@/test/mocks/render";

function makeProvider(
  overrides: Partial<FusionUpstreamProvider> = {},
): FusionUpstreamProvider {
  return {
    id: "p1",
    name: "Upstream A",
    base_url: "https://api.a.example",
    api_key: API_FUSION_KEY_MASK,
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
    const saved = onSave.mock.calls[0][0] as FusionUpstreamProvider;
    expect(saved.mappings[0]).toMatchObject({
      local_model: "gpt-4o",
      upstream_model: "gpt-4o-2024",
      display_name: "GPT-4o Custom",
    });
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
});
