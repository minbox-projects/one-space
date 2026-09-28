import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ToolStatusBadge } from "@/components/toolbox/ToolStatusBadge";
import { ToolEmptyState } from "@/components/toolbox/ToolEmptyState";

const SHARED_BADGE_CLASSES = [
  "rounded-full",
  "border",
  "px-2",
  "py-0.5",
  "text-[11px]",
  "font-medium",
];

describe("ToolStatusBadge", () => {
  it("渲染标签文本为 span", () => {
    render(<ToolStatusBadge tone="success" label="Connected" />);

    const badge = screen.getByText("Connected");
    expect(badge.tagName).toBe("SPAN");
  });

  it.each([
    ["success", /emerald/],
    ["warning", /amber/],
    ["error", /destructive|rose/],
    ["neutral", /muted/],
  ] as const)("为 %s 色调应用对应颜色类", (tone, pattern) => {
    render(<ToolStatusBadge tone={tone} label={tone} />);

    expect(screen.getByText(tone).className).toMatch(pattern);
  });

  it("始终携带共享徽章基础样式", () => {
    render(<ToolStatusBadge tone="neutral" label="Idle" />);

    expect(screen.getByText("Idle")).toHaveClass(...SHARED_BADGE_CLASSES);
  });

  it("将 testId 透传到元素", () => {
    render(
      <ToolStatusBadge tone="success" label="Connected" testId="status-badge" />,
    );

    expect(screen.getByTestId("status-badge")).toHaveTextContent("Connected");
  });

  it("追加自定义 className", () => {
    render(
      <ToolStatusBadge
        tone="error"
        label="Failed"
        className="mt-3 shrink-0"
      />,
    );

    expect(screen.getByText("Failed")).toHaveClass("mt-3", "shrink-0");
  });
});

describe("ToolEmptyState", () => {
  it("渲染标题与可选描述", () => {
    render(
      <ToolEmptyState
        title="No requests yet"
        description="Requests appear here once the router runs."
      />,
    );

    expect(screen.getByText("No requests yet")).toBeInTheDocument();
    expect(
      screen.getByText("Requests appear here once the router runs."),
    ).toBeInTheDocument();
  });

  it("未提供描述时不渲染描述", () => {
    const { container } = render(<ToolEmptyState title="No data" />);

    expect(screen.getByText("No data")).toBeInTheDocument();
    expect(container.textContent?.trim()).toBe("No data");
  });

  it("将 testId 透传到元素并追加 className", () => {
    render(
      <ToolEmptyState
        title="Nothing here"
        testId="empty-state"
        className="px-4"
      />,
    );

    expect(screen.getByTestId("empty-state")).toHaveTextContent("Nothing here");
    expect(screen.getByTestId("empty-state")).toHaveClass("px-4");
  });
});
