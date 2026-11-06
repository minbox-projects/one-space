import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import { TelegramIcon } from "./TelegramIcon";

describe("TelegramIcon", () => {
  it("应成功渲染 Telegram 官方矢量图标", () => {
    const { container } = render(<TelegramIcon data-testid="tg-icon" />);
    const svg = container.querySelector("svg");
    expect(svg).toBeInTheDocument();
    expect(svg).toHaveAttribute("viewBox", "0 0 24 24");
    expect(svg).toHaveAttribute("fill", "currentColor");
  });

  it("应支持自定义 className 与尺寸", () => {
    const { container } = render(
      <TelegramIcon className="w-6 h-6 text-sky-500" size={24} />
    );
    const svg = container.querySelector("svg");
    expect(svg).toHaveClass("w-6");
    expect(svg).toHaveClass("h-6");
    expect(svg).toHaveClass("text-sky-500");
    expect(svg).toHaveAttribute("width", "24");
    expect(svg).toHaveAttribute("height", "24");
  });
});
