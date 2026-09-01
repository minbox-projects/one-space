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
            .setActiveTab?.("file-sharing")
        }
      >
        从启动台打开文件共享
      </button>
      <button
        type="button"
        onClick={() =>
          (window as typeof window & { setActiveTab?: (tab: string) => void })
            .setActiveTab?.("md5-encryption")
        }
      >
        从启动台打开 MD5
      </button>
      <button
        type="button"
        onClick={() =>
          (window as typeof window & { setActiveTab?: (tab: string) => void })
            .setActiveTab?.("short-link")
        }
      >
        从启动台打开短链接
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
    onSelectTool: (
      tool:
        | "ssh"
        | "file-sharing"
        | "md5-encryption"
        | "short-link",
    ) => void;
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
      <div>
        <button type="button" onClick={() => onSelectTool("ssh")}>
          从更多工具打开 SSH
        </button>
        <button type="button" onClick={() => onSelectTool("md5-encryption")}>
          从更多工具打开 MD5
        </button>
        <button type="button" onClick={() => onSelectTool("short-link")}>
          从更多工具打开短链接
        </button>
      </div>
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

  it("从启动台进入文件共享后返回启动台", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );

    await user.click(
      await screen.findByRole("button", { name: "从启动台打开文件共享" }),
    );
    expect(screen.getByTestId("active-tool")).toHaveTextContent("file-sharing");

    await user.click(screen.getByRole("button", { name: "返回" }));
    expect(screen.getByTestId("launcher")).toHaveAttribute(
      "data-visible",
      "true",
    );
  });

  it("通过统一 MD5 别名从启动台进入详情并返回启动台", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );

    await user.click(
      await screen.findByRole("button", { name: "从启动台打开 MD5" }),
    );
    expect(screen.getByTestId("active-tool")).toHaveTextContent("md5-encryption");

    await user.click(screen.getByRole("button", { name: "返回" }));
    expect(screen.getByTestId("launcher")).toHaveAttribute(
      "data-visible",
      "true",
    );
  });

  it("从 More Tools 选择 MD5 后返回既有工具列表上下文", async () => {
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
      screen.getByRole("button", { name: "从更多工具打开 MD5" }),
    );
    expect(screen.getByTestId("active-tool")).toHaveTextContent("md5-encryption");

    await user.click(screen.getByRole("button", { name: "返回" }));
    expect(
      screen.getByRole("button", { name: "从更多工具打开 MD5" }),
    ).toBeInTheDocument();
  });

  it("从启动台按稳定 ID 进入短链接详情，显示标题面包屑并返回启动台", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <ThemeProvider>
        <App />
      </ThemeProvider>,
    );

    await user.click(
      await screen.findByRole("button", { name: "从启动台打开短链接" }),
    );
    expect(screen.getByTestId("active-tool")).toHaveTextContent("short-link");
    expect(
      screen.getByRole("heading", { name: /Short Link|生成短链接/ }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("navigation", { name: /Breadcrumb|面包屑/ }),
    ).toHaveTextContent(/More Tools|更多工具/);

    await user.click(screen.getByRole("button", { name: "返回" }));
    expect(screen.getByTestId("launcher")).toHaveAttribute(
      "data-visible",
      "true",
    );
  });

  it("从 More Tools 进入短链接详情并返回工具列表上下文", async () => {
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
      screen.getByRole("button", { name: "从更多工具打开短链接" }),
    );
    expect(screen.getByTestId("active-tool")).toHaveTextContent("short-link");
    expect(
      screen.getByRole("navigation", { name: /Breadcrumb|面包屑/ }),
    ).toHaveTextContent(/Short Link|生成短链接/);

    await user.click(screen.getByRole("button", { name: "返回" }));
    expect(
      screen.getByRole("button", { name: "从更多工具打开短链接" }),
    ).toBeInTheDocument();
  });

});
