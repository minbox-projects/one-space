import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ToolErrorBanner } from "@/components/toolbox/ToolErrorBanner";
import { ToolShell } from "@/components/toolbox/ToolShell";

describe("ToolShell", () => {
  it("renders the heading with the provided id", () => {
    render(
      <ToolShell titleId="demo-tool-title" title="Demo Tool">
        <p>tool body</p>
      </ToolShell>,
    );

    const heading = screen.getByRole("heading", { level: 2, name: "Demo Tool" });
    expect(heading).toHaveAttribute("id", "demo-tool-title");
  });

  it("renders the optional description and actions slot", () => {
    render(
      <ToolShell
        title="Demo Tool"
        description="A short description."
        actions={<button type="button">Refresh</button>}
      >
        <p>tool body</p>
      </ToolShell>,
    );

    expect(screen.getByText("A short description.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Refresh" })).toBeInTheDocument();
  });

  it("renders children content", () => {
    render(
      <ToolShell title="Demo Tool">
        <p data-testid="tool-body">tool body</p>
      </ToolShell>,
    );

    expect(screen.getByTestId("tool-body")).toHaveTextContent("tool body");
  });

  it("shows the error message through the alert role and clears it", () => {
    const { rerender } = render(
      <ToolShell title="Demo Tool" error="Something failed.">
        <p>tool body</p>
      </ToolShell>,
    );

    expect(screen.getByRole("alert")).toHaveTextContent("Something failed.");

    rerender(
      <ToolShell title="Demo Tool" error={null}>
        <p>tool body</p>
      </ToolShell>,
    );

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("renders no alert when no error is provided", () => {
    render(
      <ToolShell title="Demo Tool">
        <p>tool body</p>
      </ToolShell>,
    );

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});

describe("ToolErrorBanner", () => {
  it("renders the message through the alert role", () => {
    render(<ToolErrorBanner message="Copy failed." />);

    expect(screen.getByRole("alert")).toHaveTextContent("Copy failed.");
  });

  it.each([null, undefined, ""])("renders nothing for %j", (message) => {
    const { container } = render(<ToolErrorBanner message={message} />);

    expect(container).toBeEmptyDOMElement();
  });
});
