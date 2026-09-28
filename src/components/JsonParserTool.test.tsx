import { act, fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { JsonParserTool } from "@/components/JsonParserTool";
import { renderWithProviders } from "@/test/mocks/render";

describe("JsonParserTool", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it("formats valid JSON into the same editable textarea using the selected indent", async () => {
    const user = userEvent.setup();
    renderWithProviders(<JsonParserTool />);

    const editor = screen.getByLabelText(/JSON input|JSON 输入/);
    fireEvent.change(editor, { target: { value: '{"name":"OneSpace","tools":[1,2]}' } });
    await user.selectOptions(screen.getByLabelText(/Indentation|缩进/), "4");
    await user.click(screen.getByRole("button", { name: /Format JSON|美化 JSON/ }));

    expect(editor).toHaveValue('{\n    "name": "OneSpace",\n    "tools": [\n        1,\n        2\n    ]\n}');
  });

  it("keeps invalid input and displays the parser error", async () => {
    const user = userEvent.setup();
    renderWithProviders(<JsonParserTool />);

    const editor = screen.getByLabelText(/JSON input|JSON 输入/);
    fireEvent.change(editor, { target: { value: "{broken" } });
    await user.click(screen.getByRole("button", { name: /Format JSON|美化 JSON/ }));

    expect(editor).toHaveValue("{broken");
    expect(screen.getByRole("alert")).toHaveTextContent(/JSON parse error|JSON 解析错误/);
  });

  it("copies the current textarea value", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    renderWithProviders(<JsonParserTool />);

    fireEvent.change(screen.getByLabelText(/JSON input|JSON 输入/), {
      target: { value: '{"ok":true}' },
    });
    await user.click(screen.getByRole("button", { name: /Copy JSON|复制 JSON/ }));

    expect(writeText).toHaveBeenCalledWith('{"ok":true}');
  });

  it("复制成功后在 1600 毫秒后重置复制状态", async () => {
    vi.useFakeTimers();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    renderWithProviders(<JsonParserTool />);

    fireEvent.change(screen.getByLabelText(/JSON input|JSON 输入/), {
      target: { value: '{"ok":true}' },
    });
    fireEvent.click(screen.getByRole("button", { name: /Copy JSON|复制 JSON/ }));

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    const copyButton = screen.getByRole("button", {
      name: /Copy JSON|复制 JSON/,
    });
    expect(copyButton.querySelector("svg")).toHaveClass("lucide-check");

    act(() => {
      vi.advanceTimersByTime(1700);
    });

    expect(copyButton.querySelector("svg")).toHaveClass("lucide-copy");
  });
});
