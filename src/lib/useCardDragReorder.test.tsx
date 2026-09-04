import { useState } from "react";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useCardDragReorder } from "@/lib/useCardDragReorder";

function Harness({ longPressMs }: { longPressMs?: number }) {
  const [items, setItems] = useState([{ id: "a" }, { id: "b" }, { id: "c" }]);
  const drag = useCardDragReorder({
    ids: items.map((item) => item.id),
    onReorder: (ids) => setItems(ids.map((id) => ({ id }))),
    longPressMs,
  });
  return (
    <div>
      {drag.draggingId ? (
        <div data-testid="dragging-id">{drag.draggingId}</div>
      ) : null}
      {items.map((item) => (
        <div
          key={item.id}
          data-testid={`card-${item.id}`}
          onPointerOver={() => drag.handleCardPointerOver(item.id)}
        >
          <button
            type="button"
            data-testid={`handle-${item.id}`}
            onPointerDown={(e) => drag.handlePointerDown(e, item.id)}
            onPointerUp={drag.handlePointerUp}
          >
            grip
          </button>
        </div>
      ))}
    </div>
  );
}

const cardOrder = () =>
  screen
    .getAllByTestId(/^card-/)
    .map((card) => card.getAttribute("data-testid")!.replace("card-", ""));

const press = (testId: string) =>
  fireEvent.pointerDown(screen.getByTestId(testId), {
    pointerId: 1,
    clientX: 20,
    clientY: 20,
  });

const moveTo = (testId: string) =>
  fireEvent.pointerOver(screen.getByTestId(testId), { pointerId: 1 });

describe("useCardDragReorder", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("长按拖拽把手后进入拖拽状态并标记正在拖拽的卡片", () => {
    vi.useFakeTimers();
    render(<Harness longPressMs={300} />);

    press("handle-a");
    expect(screen.queryByTestId("dragging-id")).not.toBeInTheDocument();

    act(() => vi.advanceTimersByTime(300));

    expect(screen.getByTestId("dragging-id")).toHaveTextContent("a");

    fireEvent.pointerUp(window, { pointerId: 1 });
    expect(screen.queryByTestId("dragging-id")).not.toBeInTheDocument();
  });

  it("短按即松开时不进入拖拽状态", () => {
    vi.useFakeTimers();
    render(<Harness longPressMs={300} />);

    press("handle-a");
    fireEvent.pointerUp(screen.getByTestId("handle-a"), { pointerId: 1 });
    act(() => vi.advanceTimersByTime(300));
    expect(screen.queryByTestId("dragging-id")).not.toBeInTheDocument();
  });

  it("拖拽划过另一张卡片时实时移动位置", () => {
    vi.useFakeTimers();
    render(<Harness longPressMs={300} />);

    press("handle-a");
    act(() => vi.advanceTimersByTime(300));
    moveTo("card-c");

    expect(cardOrder()).toEqual(["b", "c", "a"]);

    fireEvent.pointerUp(window, { pointerId: 1 });
  });

  it("松开后结束拖拽并保留移动结果", () => {
    vi.useFakeTimers();
    render(<Harness longPressMs={300} />);

    press("handle-a");
    act(() => vi.advanceTimersByTime(300));
    moveTo("card-b");
    expect(cardOrder()).toEqual(["b", "a", "c"]);

    fireEvent.pointerUp(window, { pointerId: 1 });
    expect(screen.queryByTestId("dragging-id")).not.toBeInTheDocument();
    expect(cardOrder()).toEqual(["b", "a", "c"]);
  });
});