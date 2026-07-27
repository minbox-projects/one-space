import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { MoreToolsHub } from "@/components/MoreToolsHub";
import { LAUNCHER_TOOL_VISIBILITY_KEY } from "@/lib/launcherToolVisibility";
import { renderWithProviders } from "@/test/mocks/render";

vi.mock("./Bookmarks", () => ({
  Bookmarks: () => <div>Bookmarks detail</div>,
}));
vi.mock("./CloudDrive", () => ({
  CloudDrive: () => <div>Cloud Drive detail</div>,
}));
vi.mock("./SshServers", () => ({
  SshServers: () => <div>SSH Servers detail</div>,
}));
vi.mock("./SshTunnels", () => ({
  SshTunnels: () => <div>SSH Tunnels detail</div>,
}));
vi.mock("./ProtocolRouterTool", () => ({
  ProtocolRouterTool: () => <div>Protocol Router detail</div>,
}));
vi.mock("./RandomPasswordTool", () => ({
  RandomPasswordTool: () => <div>Random Password detail</div>,
}));
vi.mock("./JsonParserTool", () => ({
  JsonParserTool: () => <div>JSON Parser detail</div>,
}));
vi.mock("./AiRequestCaptureTool", () => ({
  AiRequestCaptureTool: () => <div>AI Request Capture detail</div>,
}));

describe("MoreToolsHub", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("在工具详情页提供返回工具列表的导航", async () => {
    const user = userEvent.setup();
    const onSelectTool = vi.fn();
    const onBack = vi.fn();
    const { rerender } = renderWithProviders(
      <MoreToolsHub
        activeTool={null}
        onSelectTool={onSelectTool}
        onBack={onBack}
      />,
    );

    await user.click(screen.getByRole("button", { name: /Bookmarks|书签/ }));
    expect(onSelectTool).toHaveBeenCalledWith("bookmarks");

    rerender(
      <MoreToolsHub
        activeTool="bookmarks"
        onSelectTool={onSelectTool}
        onBack={onBack}
      />,
    );

    expect(screen.getByText("Bookmarks detail")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Cloud Drive|云盘/ })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /Back to tools|返回工具列表/ }));
    expect(onBack).toHaveBeenCalledOnce();
  });

  it("从启动台进入详情时显示返回启动台", () => {
    renderWithProviders(
      <MoreToolsHub
        activeTool="bookmarks"
        onSelectTool={vi.fn()}
        onBack={vi.fn()}
        backToLauncher
      />,
    );

    expect(
      screen.getByRole("button", { name: /Back to Launcher|返回启动台/ }),
    ).toBeInTheDocument();
  });

  it.each([
    "bookmarks",
    "cloud",
    "ssh",
    "ssh-tunnels",
    "protocol-router",
    "random-password",
    "json-parser",
    "ai-request-capture",
  ] as const)("在 %s 详情中持久化启动台可见性开关", async (tool) => {
    const user = userEvent.setup();
    renderWithProviders(
      <MoreToolsHub
        activeTool={tool}
        onSelectTool={vi.fn()}
        onBack={vi.fn()}
      />,
    );

    const visibilitySwitch = screen.getByRole("switch", {
      name: /Show in Launcher|在启动台展示/,
    });
    expect(visibilitySwitch).toHaveAttribute("aria-checked", "true");

    await user.click(visibilitySwitch);

    expect(
      JSON.parse(localStorage.getItem(LAUNCHER_TOOL_VISIBILITY_KEY) || "{}"),
    ).toMatchObject({ [tool]: false });
  });

  it("目录卡片不再渲染辅助工具或启动台标签", () => {
    renderWithProviders(
      <MoreToolsHub
        activeTool={null}
        onSelectTool={vi.fn()}
        onBack={vi.fn()}
      />,
    );

    expect(screen.queryAllByText(/^(Utility|辅助工具)$/)).toHaveLength(0);
    expect(screen.queryByText(/^Launcher$|^启动台$/)).not.toBeInTheDocument();
  });

  it.each([
    "bookmarks",
    "cloud",
    "ssh",
    "ssh-tunnels",
    "protocol-router",
    "random-password",
    "json-parser",
    "ai-request-capture",
  ] as const)("为 %s 渲染共享图标容器", (toolId) => {
    renderWithProviders(
      <MoreToolsHub activeTool={null} onSelectTool={vi.fn()} onBack={vi.fn()} />,
    );

    expect(screen.getByTestId(`more-tool-icon-${toolId}`)).toBeInTheDocument();
  });

  it.each([
    ["random-password", "text-emerald-600"],
    ["json-parser", "text-sky-600"],
    ["ai-request-capture", "text-cyan-600"],
  ] as const)("为 %s 保留详情页图标色彩", (toolId, className) => {
    renderWithProviders(
      <MoreToolsHub activeTool={null} onSelectTool={vi.fn()} onBack={vi.fn()} />,
    );

    expect(screen.getByTestId(`more-tool-icon-${toolId}`)).toHaveClass(className);
  });

  it("使用 ScanSearch 图标打开 AI 请求抓包详情", async () => {
    const user = userEvent.setup();
    const onSelectTool = vi.fn();
    const { rerender } = renderWithProviders(
      <MoreToolsHub activeTool={null} onSelectTool={onSelectTool} onBack={vi.fn()} />,
    );

    await user.click(screen.getByRole("button", { name: /AI Request Capture|AI 请求抓包/ }));
    expect(onSelectTool).toHaveBeenCalledWith("ai-request-capture");
    expect(screen.getByTestId("more-tool-icon-ai-request-capture").querySelector("svg")).toHaveClass("lucide-scan-search");

    rerender(
      <MoreToolsHub activeTool="ai-request-capture" onSelectTool={onSelectTool} onBack={vi.fn()} />,
    );
    expect(screen.getByText("AI Request Capture detail")).toBeInTheDocument();
  });
});
