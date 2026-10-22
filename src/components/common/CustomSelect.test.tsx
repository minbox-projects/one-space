import { render, screen, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi } from "vitest";
import { CustomSelect } from "./CustomSelect";

describe("CustomSelect component", () => {
  const options = [
    { value: "opt1", label: "Option 1" },
    { value: "opt2", label: "Option 2" },
  ];

  it("renders trigger with label and supports selecting an option", () => {
    const onChange = vi.fn();
    render(
      <CustomSelect
        value="opt1"
        onChange={onChange}
        options={options}
        triggerTestId="custom-trigger"
        menuTestId="custom-menu"
        optionTestIdPrefix="opt"
      />
    );

    const trigger = screen.getByTestId("custom-trigger");
    expect(trigger).toHaveTextContent("Option 1");

    fireEvent.click(trigger);
    expect(screen.getByTestId("custom-menu")).toBeInTheDocument();

    fireEvent.click(screen.getByTestId("opt-opt2"));
    expect(onChange).toHaveBeenCalledWith("opt2");
    expect(screen.queryByTestId("custom-menu")).not.toBeInTheDocument();
  });

  it("renders small size with h-8 class", () => {
    render(
      <CustomSelect
        value="opt1"
        onChange={vi.fn()}
        options={options}
        size="sm"
        triggerTestId="sm-trigger"
      />
    );

    const trigger = screen.getByTestId("sm-trigger");
    expect(trigger.className).toContain("h-8");
  });

  it("supports userEvent.selectOptions on hidden native select", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(
      <CustomSelect
        value="opt1"
        onChange={onChange}
        options={options}
        ariaLabel="Algorithm"
      />
    );

    const select = screen.getByRole("combobox", { name: "Algorithm" });
    await user.selectOptions(select, "opt2");
    expect(onChange).toHaveBeenCalledWith("opt2");
  });
});
