import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { QuickSftpTool } from "./QuickSftpTool";
import { renderWithProviders } from "@/test/mocks/render";

describe("QuickSftpTool", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it("renders the tool shell, host selector and directory navigation", () => {
    renderWithProviders(<QuickSftpTool />);
    expect(
      screen.getByRole("heading", { name: /SFTP|文件与日志追踪/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByPlaceholderText(/~ or \/var\/log/i),
    ).toBeInTheDocument();
  });

  it("shows quick bookmark paths and switches directory", async () => {
    const user = userEvent.setup();
    renderWithProviders(<QuickSftpTool />);

    const varLogBtn = screen.getByRole("button", { name: "/var/log" });
    await user.click(varLogBtn);

    const pathInput = screen.getByPlaceholderText(/~ or \/var\/log/i);
    expect(pathInput).toHaveValue("/var/log");
  });
});
