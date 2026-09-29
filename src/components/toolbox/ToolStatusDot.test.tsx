import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ToolStatusDot, type ToolStatusDotTone } from "./ToolStatusDot";

describe("ToolStatusDot", () => {
  it("默认渲染 success 绿色的微光圆点和实体圆点", () => {
    render(<ToolStatusDot testId="status-dot" />);

    const root = screen.getByTestId("status-dot");
    expect(root).toBeInTheDocument();
    expect(root).toHaveClass("absolute", "right-1", "top-1", "flex", "h-2", "w-2");

    const ping = screen.getByTestId("status-dot-ping");
    expect(ping).toHaveClass("animate-ping", "bg-emerald-400");

    const core = screen.getByTestId("status-dot-core");
    expect(core).toHaveClass("bg-emerald-500");
  });

  it.each([
    ["success", "bg-emerald-400", "bg-emerald-500"],
    ["warning", "bg-amber-400", "bg-amber-500"],
    ["error", "bg-destructive", "bg-destructive"],
    ["info", "bg-blue-400", "bg-blue-500"],
  ] as const)("正确应用 %s 色调样式", (tone: ToolStatusDotTone, expectedPing, expectedDot) => {
    render(<ToolStatusDot tone={tone} testId={`status-dot-${tone}`} />);

    const ping = screen.getByTestId(`status-dot-${tone}-ping`);
    expect(ping).toHaveClass(expectedPing);

    const core = screen.getByTestId(`status-dot-${tone}-core`);
    expect(core).toHaveClass(expectedDot);
  });

  it("当 ping=false 时不渲染呼吸扩散层", () => {
    render(<ToolStatusDot ping={false} testId="no-ping-dot" />);

    expect(screen.queryByTestId("no-ping-dot-ping")).not.toBeInTheDocument();
    expect(screen.getByTestId("no-ping-dot-core")).toBeInTheDocument();
  });

  it("支持附加额外 className", () => {
    render(<ToolStatusDot className="custom-class" testId="custom-dot" />);

    const root = screen.getByTestId("custom-dot");
    expect(root).toHaveClass("custom-class");
  });
});
