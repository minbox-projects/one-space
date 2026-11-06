import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import { TelegramIcon } from "./TelegramIcon";

describe("TelegramIcon", () => {
  it("应成功渲染 Lucide 风格的 Telegram Outline 线性矢量图标", () => {
    const { container } = render(<TelegramIcon data-testid="tg-icon" />);
    const svg = container.querySelector("svg");
    expect(svg).toBeInTheDocument();
    expect(svg).toHaveAttribute("viewBox", "0 0 24 24");
    expect(svg).toHaveAttribute("fill", "none");
    expect(svg).toHaveAttribute("stroke", "currentColor");
    expect(svg).toHaveAttribute("stroke-width", "2");
    expect(svg).toHaveAttribute("stroke-linecap", "round");
    expect(svg).toHaveAttribute("stroke-linejoin", "round");
  });

  it("应支持自定义 className、尺寸与 strokeWidth", () => {
    const { container } = render(
      <TelegramIcon
        className="w-6 h-6 text-emerald-600"
        size={24}
        strokeWidth={1.5}
      />
    );
    const svg = container.querySelector("svg");
    expect(svg).toHaveClass("w-6");
    expect(svg).toHaveClass("h-6");
    expect(svg).toHaveClass("text-emerald-600");
    expect(svg).toHaveAttribute("width", "24");
    expect(svg).toHaveAttribute("height", "24");
    expect(svg).toHaveAttribute("stroke-width", "1.5");
  });
});
