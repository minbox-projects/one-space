import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "@/App";
import { ThemeProvider } from "@/components/ThemeProvider";
import { renderWithProviders } from "@/test/mocks/render";

vi.mock("@/components/Launcher", () => ({
  Launcher: ({ isVisible }: { isVisible?: boolean }) => (
    <div data-testid="launcher" data-visible={String(isVisible)}>
      <button
        type="button"
        onClick={() =>
          (window as typeof window & { setActiveTab?: (tab: string) => void })
            .setActiveTab?.("ssh")
        }
      >
        从启动台打开 SSH
      </button>
      <button
        type="button"
        onClick={() =>
          (window as typeof window & { setActiveTab?: (tab: string) => void })
            .setActiveTab?.("ai-request-capture")
        }
      >
        从启动台打开 AI 请求抓包
      </button>
    </div>
  ),
}));

vi.mock("@/components/MoreToolsHub", () => ({
  MoreToolsHub: ({
    activeTool,
    onSelectTool,
    onBack,
  }: {
    activeTool: string | null;
    onSelectTool: (tool: "ssh" | "ai-request-capture") => void;
    onBack: () => void;
  }) =>
    activeTool ? (
      <div>
        <div data-testid="active-tool">{activeTool}</div>
        <button type="button" onClick={onBack}>
          返回
        </button>
      </div>
    ) : (
      <button type="button" onClick={() => onSelectTool("ssh")}>
        从更多工具打开 SSH
      </button>
    ),
}));

describe("App 更多工具详情导航", () => {
  beforeEach(() => {
    delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    window.matchMedia = vi.fn().mockReturnValue({ matches: false });
  });

  it("从启动台进入工具详情后返回启动台", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );

    await user.click(
      await screen.findByRole("button", { name: "从启动台打开 SSH" }),
    );
    expect(screen.getByTestId("active-tool")).toHaveTextContent("ssh");

    await user.click(screen.getByRole("button", { name: "返回" }));
    expect(screen.getByTestId("launcher")).toHaveAttribute(
      "data-visible",
      "true",
    );
  });

  it("从更多工具进入详情后返回工具列表", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );

    await user.click(
      await screen.findByRole("button", { name: /More Tools|更多工具/ }),
    );
    await user.click(
      screen.getByRole("button", { name: "从更多工具打开 SSH" }),
    );
    expect(screen.getByTestId("active-tool")).toHaveTextContent("ssh");

    await user.click(screen.getByRole("button", { name: "返回" }));
    expect(
      screen.getByRole("button", { name: "从更多工具打开 SSH" }),
    ).toBeInTheDocument();
  });

  it("从启动台进入 AI 请求抓包后返回启动台", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );

    await user.click(
      await screen.findByRole("button", { name: "从启动台打开 AI 请求抓包" }),
    );
    expect(screen.getByTestId("active-tool")).toHaveTextContent("ai-request-capture");

    await user.click(screen.getByRole("button", { name: "返回" }));
    expect(screen.getByTestId("launcher")).toHaveAttribute(
      "data-visible",
      "true",
    );
  });
});
