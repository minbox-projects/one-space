import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { LogViewer } from "./LogViewer";

describe("LogViewer", () => {
  it("renders logs and allows scrolling without forcing scroll to bottom when scrolled up", async () => {
    const onClear = vi.fn();
    const { rerender } = render(
      <LogViewer logs={["line 1", "line 2"]} onClear={onClear} />,
    );

    expect(screen.getByText("line 1")).toBeInTheDocument();
    expect(screen.getByText("line 2")).toBeInTheDocument();

    const scrollContainer = screen.getByText("line 1").closest(".overflow-y-auto") as HTMLDivElement;
    expect(scrollContainer).toBeInTheDocument();

    // 模拟容器尺寸与滚动位置（用户向上滚动，离底部很远）
    Object.defineProperty(scrollContainer, "scrollHeight", { configurable: true, value: 500 });
    Object.defineProperty(scrollContainer, "clientHeight", { configurable: true, value: 100 });
    Object.defineProperty(scrollContainer, "scrollTop", { configurable: true, writable: true, value: 10 });

    // 触发滚动事件
    act(() => {
      fireEvent.scroll(scrollContainer);
    });

    // 出现“回到最新”悬浮按钮
    const toLatestBtn = await screen.findByTestId("port-tunnel-scroll-latest");
    expect(toLatestBtn).toBeInTheDocument();

    // 当有新日志到来时，由于处于非底部，scrollTop 不会被强制重设为底部
    rerender(
      <LogViewer logs={["line 1", "line 2", "line 3"]} onClear={onClear} />,
    );
    expect(screen.getByText("line 3")).toBeInTheDocument();
    expect(scrollContainer.scrollTop).toBe(10);

    // 点击“回到最新”按钮，重置到底部
    act(() => {
      toLatestBtn.click();
    });
    expect(scrollContainer.scrollTop).toBe(500);
    expect(screen.queryByTestId("port-tunnel-scroll-latest")).not.toBeInTheDocument();
  });
});
