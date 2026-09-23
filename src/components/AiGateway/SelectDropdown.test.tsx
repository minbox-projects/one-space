import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { SelectDropdown } from "./SelectDropdown";

describe("SelectDropdown", () => {
  const options = [
    { value: "a", label: "Option A" },
    { value: "b", label: "Option B" },
    { value: "c", label: "Option C" },
  ];

  it("默认展示当前选中项的文本", () => {
    render(
      <SelectDropdown
        value="a"
        options={options}
        onChange={() => {}}
        testId="test-select"
      />,
    );

    const trigger = screen.getByTestId("test-select-trigger");
    expect(trigger).toHaveTextContent("Option A");
    expect(screen.queryByTestId("test-select-menu")).not.toBeInTheDocument();
  });

  it("点击按钮展开菜单，选择选项后触发 onChange 并关闭菜单", async () => {
    const user = userEvent.setup();
    const handleChange = vi.fn();

    render(
      <SelectDropdown
        value="a"
        options={options}
        onChange={handleChange}
        testId="test-select"
      />,
    );

    await user.click(screen.getByTestId("test-select-trigger"));
    expect(screen.getByTestId("test-select-menu")).toBeInTheDocument();

    await user.click(screen.getByRole("option", { name: "Option B" }));
    expect(handleChange).toHaveBeenCalledWith("b");
    expect(screen.queryByTestId("test-select-menu")).not.toBeInTheDocument();
  });

  it("按 Escape 键关闭已展开的菜单", async () => {
    const user = userEvent.setup();

    render(
      <SelectDropdown
        value="a"
        options={options}
        onChange={() => {}}
        testId="test-select"
      />,
    );

    await user.click(screen.getByTestId("test-select-trigger"));
    expect(screen.getByTestId("test-select-menu")).toBeInTheDocument();

    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("test-select-menu")).not.toBeInTheDocument();
  });

  it("点击外部区域关闭菜单", async () => {
    const user = userEvent.setup();

    render(
      <div>
        <div data-testid="outside">Outside area</div>
        <SelectDropdown
          value="a"
          options={options}
          onChange={() => {}}
          testId="test-select"
        />
      </div>,
    );

    await user.click(screen.getByTestId("test-select-trigger"));
    expect(screen.getByTestId("test-select-menu")).toBeInTheDocument();

    await user.click(screen.getByTestId("outside"));
    expect(screen.queryByTestId("test-select-menu")).not.toBeInTheDocument();
  });

  it("触发按钮与下拉菜单选项均包含 whitespace-nowrap 防止换行", async () => {
    const user = userEvent.setup();

    render(
      <SelectDropdown
        value="a"
        options={options}
        onChange={() => {}}
        testId="test-select"
      />,
    );

    const trigger = screen.getByTestId("test-select-trigger");
    expect(trigger.className).toContain("whitespace-nowrap");

    await user.click(trigger);
    const menu = screen.getByTestId("test-select-menu");
    expect(menu.className).toContain("w-max");

    const optionA = screen.getByRole("option", { name: "Option A" });
    expect(optionA.className).toContain("whitespace-nowrap");
  });
});
