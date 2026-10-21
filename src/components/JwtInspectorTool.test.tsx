import { fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { JwtInspectorTool } from "./JwtInspectorTool";
import { renderWithProviders } from "@/test/mocks/render";

describe("JwtInspectorTool", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it("renders the tool shell and input textarea", () => {
    renderWithProviders(<JwtInspectorTool />);
    expect(
      screen.getByRole("heading", { name: /JWT Inspector|JWT 调试器/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByPlaceholderText(
        /Paste JWT token string here|在此粘贴 JWT 字符串/i,
      ),
    ).toBeInTheDocument();
  });

  it("loads sample JWT and displays subject context and claims", async () => {
    const user = userEvent.setup();
    renderWithProviders(<JwtInspectorTool />);

    // 点击“加载样例”按钮
    const sampleBtn = screen.getByRole("button", {
      name: /Sample|演示样例/i,
    });
    await user.click(sampleBtn);

    // 应该展示主体数据卡片
    expect(
      screen.getByText(/Subject Identity|主体身份/i),
    ).toBeInTheDocument();
    // 应该展示角色胶囊
    expect(screen.getByText("admin")).toBeInTheDocument();
    expect(screen.getByText("developer")).toBeInTheDocument();
  });

  it("shows parsing error for invalid token", () => {
    renderWithProviders(<JwtInspectorTool />);
    const textarea = screen.getByPlaceholderText(
      /Paste JWT token string here|在此粘贴 JWT 字符串/i,
    );

    fireEvent.change(textarea, { target: { value: "invalid-token" } });
    expect(
      screen.getByText(/Invalid JWT format|JWT 格式错误/i),
    ).toBeInTheDocument();
  });
});
