import { describe, it, expect, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import {
  ProviderTemplateIcon,
  ProviderTemplateAvatar,
  ProviderTemplateIconPicker,
  resolveProviderTemplateIcon,
  resolveEffectiveProviderIcon,
} from "./ProviderTemplateIcon";

describe("resolveProviderTemplateIcon", () => {
  it("resolves explicit icons", () => {
    expect(resolveProviderTemplateIcon("opencode")).toBe("opencode");
    expect(resolveProviderTemplateIcon("builtin:opencode")).toBe("opencode");
    expect(resolveProviderTemplateIcon("commandcode")).toBe("commandcode");
    expect(resolveProviderTemplateIcon("builtin:commandcode")).toBe("commandcode");
    expect(resolveProviderTemplateIcon("openai")).toBe("openai");
    expect(resolveProviderTemplateIcon("chatgpt")).toBe("openai");
    expect(resolveProviderTemplateIcon("builtin:chatgpt")).toBe("openai");
  });

  it("infers icon from templateId or templateName when icon is absent", () => {
    expect(resolveProviderTemplateIcon(null, "opencode-zen", "OpenCode Zen")).toBe("opencode");
    expect(resolveProviderTemplateIcon(undefined, "commandcode", "CommandCode")).toBe("commandcode");
    expect(resolveProviderTemplateIcon("", "openai-official", "OpenAI Official")).toBe("openai");
  });

  it("returns null when neither icon nor text matches", () => {
    expect(resolveProviderTemplateIcon(null, "custom-id", "Custom Service")).toBeNull();
  });
});

describe("ProviderTemplateIcon component", () => {
  it("renders OpenCode icon when specified", () => {
    render(<ProviderTemplateIcon icon="opencode" />);
    expect(screen.getByTestId("provider-icon-opencode")).toBeInTheDocument();
  });

  it("renders CommandCode icon when specified", () => {
    render(<ProviderTemplateIcon icon="commandcode" />);
    expect(screen.getByTestId("provider-icon-commandcode")).toBeInTheDocument();
  });

  it("renders OpenAI icon when specified", () => {
    render(<ProviderTemplateIcon icon="openai" />);
    expect(screen.getByTestId("provider-icon-openai")).toBeInTheDocument();
  });

  it("renders inferred icon for opencode-zen template id", () => {
    render(<ProviderTemplateIcon templateId="opencode-zen" templateName="OpenCode Zen" />);
    expect(screen.getByTestId("provider-icon-opencode")).toBeInTheDocument();
  });

  it("renders default Sparkles icon when unknown without fallback", () => {
    render(<ProviderTemplateIcon templateId="custom" templateName="Custom Provider" />);
    expect(screen.getByTestId("provider-icon-default")).toBeInTheDocument();
  });

  it("renders custom fallback when provided", () => {
    render(
      <ProviderTemplateIcon
        templateId="custom"
        templateName="Custom Provider"
        fallback={<span data-testid="custom-fallback">Custom</span>}
      />
    );
    expect(screen.getByTestId("custom-fallback")).toBeInTheDocument();
    expect(screen.queryByTestId("provider-icon-default")).not.toBeInTheDocument();
  });
});

describe("ProviderTemplateAvatar component", () => {
  it("renders OneSpace style avatar container with icon", () => {
    const { container } = render(
      <ProviderTemplateAvatar icon="opencode" templateName="OpenCode Zen" size={42} />
    );
    expect(screen.getByTestId("provider-icon-opencode")).toBeInTheDocument();
    const avatar = container.firstChild as HTMLElement;
    expect(avatar).toHaveStyle({ width: "42px", height: "42px" });
  });

  it("renders fallback text character when icon cannot be resolved", () => {
    render(
      <ProviderTemplateAvatar templateId="custom-svc" templateName="My Service" size={36} />
    );
    expect(screen.getByText("M")).toBeInTheDocument();
  });
});

describe("ProviderTemplateIconPicker component", () => {
  it("renders trigger and opens menu with options on click", () => {
    const onChange = vi.fn();
    render(
      <ProviderTemplateIconPicker
        value="opencode"
        onChange={onChange}
        templateId="tpl-test"
        templateName="Test Template"
      />
    );

    const trigger = screen.getByTestId("template-edit-icon-trigger");
    expect(trigger).toBeInTheDocument();
    expect(screen.queryByTestId("template-edit-icon-menu")).not.toBeInTheDocument();

    fireEvent.click(trigger);
    expect(screen.getByTestId("template-edit-icon-menu")).toBeInTheDocument();
    expect(screen.getByTestId("template-icon-option-auto")).toBeInTheDocument();
    expect(screen.getByTestId("template-icon-option-opencode")).toBeInTheDocument();
    expect(screen.getByTestId("template-icon-option-commandcode")).toBeInTheDocument();
    expect(screen.getByTestId("template-icon-option-openai")).toBeInTheDocument();

    fireEvent.click(screen.getByTestId("template-icon-option-commandcode"));
    expect(onChange).toHaveBeenCalledWith("commandcode");
    expect(screen.queryByTestId("template-edit-icon-menu")).not.toBeInTheDocument();
  });

  it("supports custom options, autoLabel, and inheritedIcon", () => {
    const onChange = vi.fn();
    render(
      <ProviderTemplateIconPicker
        value=""
        onChange={onChange}
        autoLabel="Inherit from template (Default)"
        inheritedIcon="opencode"
        triggerTestId="provider-icon-trigger"
        selectTestId="provider-icon-select"
        menuTestId="provider-icon-menu"
      />
    );

    const trigger = screen.getByTestId("provider-icon-trigger");
    expect(trigger).toHaveTextContent("Inherit from template (Default)");
    expect(screen.getByTestId("provider-icon-opencode")).toBeInTheDocument();

    fireEvent.click(trigger);
    expect(screen.getByTestId("provider-icon-menu")).toBeInTheDocument();
  });
});

describe("resolveEffectiveProviderIcon", () => {
  it("prioritizes provider custom icon over template icon", () => {
    expect(
      resolveEffectiveProviderIcon(
        { icon: "builtin:deepseek", template_id: "opencode-zen" },
        { icon: "opencode" },
      ),
    ).toBe("builtin:deepseek");
  });

  it("falls back to template icon when provider has no custom icon", () => {
    expect(
      resolveEffectiveProviderIcon(
        { icon: "", template_id: "opencode-zen" },
        { icon: "opencode" },
      ),
    ).toBe("opencode");
    expect(
      resolveEffectiveProviderIcon(
        { icon: null, template_id: "opencode-zen" },
        { icon: "commandcode" },
      ),
    ).toBe("commandcode");
  });

  it("returns null when neither provider nor template has an icon", () => {
    expect(resolveEffectiveProviderIcon(null, null)).toBeNull();
    expect(
      resolveEffectiveProviderIcon({ icon: "" }, { icon: "" }),
    ).toBeNull();
  });
});
