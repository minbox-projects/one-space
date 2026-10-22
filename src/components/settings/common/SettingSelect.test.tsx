import { render, screen, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi } from "vitest";
import { SettingSelect } from "./SettingSelect";

describe("SettingSelect component", () => {
  const options = [
    { value: "local", label: "Local Only" },
    { value: "all", label: "Allow All" },
  ];

  it("renders trigger with selected option label", () => {
    render(
      <SettingSelect
        value="local"
        onChange={vi.fn()}
        options={options}
        triggerTestId="select-trigger"
        selectTestId="native-select"
        menuTestId="select-menu"
      />
    );

    const trigger = screen.getByTestId("select-trigger");
    expect(trigger).toHaveTextContent("Local Only");
    expect(screen.queryByTestId("select-menu")).not.toBeInTheDocument();
  });

  it("opens menu when trigger is clicked and allows selecting an option", async () => {
    const onChange = vi.fn();
    render(
      <SettingSelect
        value="local"
        onChange={onChange}
        options={options}
        triggerTestId="select-trigger"
        menuTestId="select-menu"
        optionTestIdPrefix="opt"
      />
    );

    const trigger = screen.getByTestId("select-trigger");
    fireEvent.click(trigger);

    expect(screen.getByTestId("select-menu")).toBeInTheDocument();
    const optAll = screen.getByTestId("opt-all");
    expect(optAll).toBeInTheDocument();
    expect(optAll).toHaveTextContent("Allow All");

    fireEvent.click(optAll);
    expect(onChange).toHaveBeenCalledWith("all");
    expect(screen.queryByTestId("select-menu")).not.toBeInTheDocument();
  });

  it("closes menu when Escape is pressed", () => {
    render(
      <SettingSelect
        value="local"
        onChange={vi.fn()}
        options={options}
        triggerTestId="select-trigger"
        menuTestId="select-menu"
      />
    );

    fireEvent.click(screen.getByTestId("select-trigger"));
    expect(screen.getByTestId("select-menu")).toBeInTheDocument();

    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByTestId("select-menu")).not.toBeInTheDocument();
  });

  it("closes menu on click outside", () => {
    render(
      <div>
        <span data-testid="outside">Outside</span>
        <SettingSelect
          value="local"
          onChange={vi.fn()}
          options={options}
          triggerTestId="select-trigger"
          menuTestId="select-menu"
        />
      </div>
    );

    fireEvent.click(screen.getByTestId("select-trigger"));
    expect(screen.getByTestId("select-menu")).toBeInTheDocument();

    fireEvent.mouseDown(screen.getByTestId("outside"));
    expect(screen.queryByTestId("select-menu")).not.toBeInTheDocument();
  });

  it("supports userEvent.selectOptions on hidden native select for automated tests", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(
      <SettingSelect
        value="local"
        onChange={onChange}
        options={options}
        ariaLabel="Permission"
      />
    );

    const select = screen.getByRole("combobox", { name: "Permission" });
    await user.selectOptions(select, "all");
    expect(onChange).toHaveBeenCalledWith("all");
  });
});
