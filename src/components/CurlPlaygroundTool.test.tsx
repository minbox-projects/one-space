import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CurlPlaygroundTool } from "./CurlPlaygroundTool";
import { renderWithProviders } from "@/test/mocks/render";

describe("CurlPlaygroundTool", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it("renders the tool shell, method selector, and URL input without sample button", () => {
    renderWithProviders(<CurlPlaygroundTool />);
    expect(
      screen.getByRole("heading", { name: /cURL Playground|cURL 请求/i }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Send|发送/i })).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /Sample|演示样例/i }),
    ).not.toBeInTheDocument();
  });

  it("parses input curl command and updates endpoint", async () => {
    const user = userEvent.setup();
    renderWithProviders(<CurlPlaygroundTool />);

    const curlInput = screen.getByPlaceholderText(/curl -X POST/i);
    await user.type(
      curlInput,
      'curl "https://api.github.com/users/octocat"',
    );

    const urlInput = screen.getByPlaceholderText(/endpoint/i);
    expect(urlInput).toHaveValue("https://api.github.com/users/octocat");
  });

  it("switches to Code Export tab and generates typescript fetch code", async () => {
    const user = userEvent.setup();
    renderWithProviders(<CurlPlaygroundTool />);

    const codeTabBtn = screen.getByRole("button", { name: /Export Code|导出代码/i });
    await user.click(codeTabBtn);

    expect(screen.getByText(/TypeScript fetch/i)).toBeInTheDocument();
  });
});
