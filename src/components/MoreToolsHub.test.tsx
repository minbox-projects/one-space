import { act, fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MoreToolsHub } from "@/components/MoreToolsHub";
import {
  MORE_TOOLS_ORDER_KEY,
  writeSavedOrder,
} from "@/lib/launcherToolOrder";
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
vi.mock("./Md5EncryptionTool", () => ({
  Md5EncryptionTool: () => <div>MD5 Encryption detail</div>,
}));
vi.mock("./ShortLinkTool", () => ({
  ShortLinkTool: () => <div>Short Link detail</div>,
}));
vi.mock("./FileSharingTool", () => ({
  FileSharingTool: () => <div>File Sharing detail</div>,
}));
vi.mock("./JttDataParserTool", () => ({
  JttDataParserTool: ({ initialTab }: { initialTab?: string }) => (
    <div>
      JT/T Data Parser detail subtab={initialTab ?? "none"}
    </div>
  ),
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

  it("显示 MD5 卡片并分发同一详情组件", async () => {
    const user = userEvent.setup();
    const onSelectTool = vi.fn();
    const { rerender } = renderWithProviders(
      <MoreToolsHub
        activeTool={null}
        onSelectTool={onSelectTool}
        onBack={vi.fn()}
      />,
    );

    await user.click(
      screen.getByRole("button", { name: /MD5 Encryption|MD5 加密/ }),
    );
    expect(onSelectTool).toHaveBeenCalledWith("md5-encryption");

    rerender(
      <MoreToolsHub
        activeTool="md5-encryption"
        onSelectTool={onSelectTool}
        onBack={vi.fn()}
      />,
    );
    expect(screen.getByText("MD5 Encryption detail")).toBeInTheDocument();
  });

  it("显示短链接卡片、分发详情并返回工具列表", async () => {
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

    await user.click(
      screen.getByRole("button", { name: /Short Link|生成短链接/ }),
    );
    expect(onSelectTool).toHaveBeenCalledWith("short-link");

    rerender(
      <MoreToolsHub
        activeTool="short-link"
        onSelectTool={onSelectTool}
        onBack={onBack}
      />,
    );
    expect(screen.getByText("Short Link detail")).toBeInTheDocument();

    await user.click(
      screen.getByRole("button", { name: /Back to tools|返回工具列表/ }),
    );
    expect(onBack).toHaveBeenCalledOnce();
  });

  it("按 md5Encryption 可见性隐藏 MD5 卡片但保留直接详情入口", () => {
    localStorage.setItem(
      LAUNCHER_TOOL_VISIBILITY_KEY,
      JSON.stringify({ md5Encryption: false }),
    );
    const { rerender } = renderWithProviders(
      <MoreToolsHub activeTool={null} onSelectTool={vi.fn()} onBack={vi.fn()} />,
    );

    expect(
      screen.queryByRole("button", { name: /MD5 Encryption|MD5 加密/ }),
    ).not.toBeInTheDocument();

    rerender(
      <MoreToolsHub
        activeTool="md5-encryption"
        onSelectTool={vi.fn()}
        onBack={vi.fn()}
      />,
    );
    expect(screen.getByText("MD5 Encryption detail")).toBeInTheDocument();
    expect(
      screen.getByRole("switch", {
        name: /Show in Launcher|在启动台展示/,
      }),
    ).toHaveAttribute("aria-checked", "false");
  });

  it.each([
    "bookmarks",
    "cloud",
    "ssh",
    "ssh-tunnels",
    "protocol-router",
    "random-password",
    "json-parser",
    "short-link",
    "file-sharing",
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

  it("在 MD5 详情中持久化唯一可见性字段", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <MoreToolsHub
        activeTool="md5-encryption"
        onSelectTool={vi.fn()}
        onBack={vi.fn()}
      />,
    );

    await user.click(
      screen.getByRole("switch", { name: /Show in Launcher|在启动台展示/ }),
    );
    expect(
      JSON.parse(localStorage.getItem(LAUNCHER_TOOL_VISIBILITY_KEY) || "{}"),
    ).toMatchObject({ md5Encryption: false });
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
    "md5-encryption",
    "short-link",
    "file-sharing",
  ] as const)("为 %s 渲染共享图标容器", (toolId) => {
    renderWithProviders(
      <MoreToolsHub activeTool={null} onSelectTool={vi.fn()} onBack={vi.fn()} />,
    );

    expect(screen.getByTestId(`more-tool-icon-${toolId}`)).toBeInTheDocument();
  });

  it.each([
    ["random-password", "text-emerald-600"],
    ["json-parser", "text-sky-600"],
    ["md5-encryption", "text-teal-600"],
    ["short-link", "text-teal-600"],
    ["file-sharing", "text-rose-600"],
  ] as const)("为 %s 保留详情页图标色彩", (toolId, className) => {
    renderWithProviders(
      <MoreToolsHub activeTool={null} onSelectTool={vi.fn()} onBack={vi.fn()} />,
    );

    expect(screen.getByTestId(`more-tool-icon-${toolId}`)).toHaveClass(className);
  });

  it("使用 Hash 图标展示 MD5 工具", () => {
    renderWithProviders(
      <MoreToolsHub activeTool={null} onSelectTool={vi.fn()} onBack={vi.fn()} />,
    );

    expect(
      screen.getByTestId("more-tool-icon-md5-encryption").querySelector("svg"),
    ).toHaveClass("lucide-hash");
  });

  it("使用 Link 图标展示短链接工具", () => {
    renderWithProviders(
      <MoreToolsHub activeTool={null} onSelectTool={vi.fn()} onBack={vi.fn()} />,
    );

    expect(
      screen.getByTestId("more-tool-icon-short-link").querySelector("svg"),
    ).toHaveClass("lucide-link");
  });

  it("展示 JT/T 数据解析卡片并分发同一详情组件", async () => {
    const user = userEvent.setup();
    const onSelectTool = vi.fn();
    const { rerender } = renderWithProviders(
      <MoreToolsHub
        activeTool={null}
        onSelectTool={onSelectTool}
        onBack={vi.fn()}
      />,
    );

    await user.click(
      screen.getByRole("button", { name: /JT\/T 数据解析|JT\/T Data Parser/ }),
    );
    expect(onSelectTool).toHaveBeenCalledWith("jtt-data-parser");

    rerender(
      <MoreToolsHub
        activeTool="jtt-data-parser"
        onSelectTool={onSelectTool}
        onBack={vi.fn()}
      />,
    );
    expect(screen.getByText(/JT\/T Data Parser detail/)).toBeInTheDocument();
  });

  it("将可选 JT/T 子标签页传达到解析器组件", () => {
    renderWithProviders(
      <MoreToolsHub
        activeTool="jtt-data-parser"
        onSelectTool={vi.fn()}
        onBack={vi.fn()}
        jttParserTab="jt809"
      />,
    );

    expect(screen.getByText(/subtab=jt809/)).toBeInTheDocument();
  });

  it("隐藏启动台展示后 JT/T 目录卡片仍然可见并可通过详情开关恢复", async () => {
    const user = userEvent.setup();
    localStorage.setItem(
      LAUNCHER_TOOL_VISIBILITY_KEY,
      JSON.stringify({ "jtt-data-parser": false }),
    );
    const { rerender } = renderWithProviders(
      <MoreToolsHub activeTool={null} onSelectTool={vi.fn()} onBack={vi.fn()} />,
    );

    expect(
      screen.getByRole("button", { name: /JT\/T 数据解析|JT\/T Data Parser/ }),
    ).toBeInTheDocument();

    rerender(
      <MoreToolsHub
        activeTool="jtt-data-parser"
        onSelectTool={vi.fn()}
        onBack={vi.fn()}
      />,
    );
    expect(
      screen.getByRole("switch", { name: /Show in Launcher|在启动台展示/ }),
    ).toHaveAttribute("aria-checked", "false");

    await user.click(
      screen.getByRole("switch", { name: /Show in Launcher|在启动台展示/ }),
    );
    expect(
      JSON.parse(localStorage.getItem(LAUNCHER_TOOL_VISIBILITY_KEY) || "{}"),
    ).toMatchObject({ "jtt-data-parser": true });
  });

  it("为 jtt-data-parser 渲染共享图标容器", () => {
    renderWithProviders(
      <MoreToolsHub activeTool={null} onSelectTool={vi.fn()} onBack={vi.fn()} />,
    );

    expect(
      screen.getByTestId("more-tool-icon-jtt-data-parser"),
    ).toBeInTheDocument();
  });

  describe("卡片拖拽整理", () => {
    afterEach(() => {
      vi.useRealTimers();
    });

    const cardOrder = () =>
      screen
        .getAllByTestId(/more-tool-card-/)
        .map((card) =>
          card.getAttribute("data-testid")!.replace("more-tool-card-", ""),
        );

    const pressHandle = (toolId: string) =>
      fireEvent.pointerDown(
        screen.getByTestId(`more-tool-drag-handle-${toolId}`),
        { pointerId: 1, clientX: 20, clientY: 20 },
      );

    it("长按拖拽把手拖拽卡片实时移动位置并持久化，且不显示提示框或完成按钮", () => {
      vi.useFakeTimers();
      renderWithProviders(
        <MoreToolsHub
          activeTool={null}
          onSelectTool={vi.fn()}
          onBack={vi.fn()}
        />,
      );

      pressHandle("bookmarks");
      act(() => vi.advanceTimersByTime(300));

      expect(
        screen.queryByText(/可拖拽调整顺序|Drag cards to reorder/),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByRole("button", { name: /完成|Done/ }),
      ).not.toBeInTheDocument();

      fireEvent.pointerOver(screen.getByTestId("more-tool-card-ssh"), {
        pointerId: 1,
      });

      expect(cardOrder().slice(0, 3)).toEqual(["cloud", "ssh", "bookmarks"]);
      expect(
        JSON.parse(localStorage.getItem(MORE_TOOLS_ORDER_KEY) || "[]"),
      ).toEqual([
        "cloud",
        "ssh",
        "bookmarks",
        "ssh-tunnels",
        "protocol-router",
        "random-password",
        "json-parser",
        "md5-encryption",
        "short-link",
        "file-sharing",
        "jtt-data-parser",
      ]);

      fireEvent.pointerUp(window, { pointerId: 1 });
      vi.useRealTimers();
    });

    it("渲染时应用已保存的卡片顺序", () => {
      writeSavedOrder(MORE_TOOLS_ORDER_KEY, ["ssh", "bookmarks", "cloud"]);
      renderWithProviders(
        <MoreToolsHub
          activeTool={null}
          onSelectTool={vi.fn()}
          onBack={vi.fn()}
        />,
      );

      expect(cardOrder().slice(0, 3)).toEqual(["ssh", "bookmarks", "cloud"]);
    });

    it("短按拖拽把手不触发拖拽，点击卡片直接打开工具", () => {
      vi.useFakeTimers();
      const onSelectTool = vi.fn();
      renderWithProviders(
        <MoreToolsHub
          activeTool={null}
          onSelectTool={onSelectTool}
          onBack={vi.fn()}
        />,
      );

      pressHandle("bookmarks");
      fireEvent.pointerUp(
        screen.getByTestId("more-tool-drag-handle-bookmarks"),
        { pointerId: 1 },
      );
      act(() => vi.advanceTimersByTime(300));
      expect(
        screen.queryByText(/可拖拽调整顺序|Drag cards to reorder/),
      ).not.toBeInTheDocument();

      fireEvent.click(screen.getByTestId("more-tool-card-ssh"));
      expect(onSelectTool).toHaveBeenCalledWith("ssh");
      vi.useRealTimers();
    });

    it("拖拽完成后点击卡片恢复打开工具", () => {
      vi.useFakeTimers();
      const onSelectTool = vi.fn();
      renderWithProviders(
        <MoreToolsHub
          activeTool={null}
          onSelectTool={onSelectTool}
          onBack={vi.fn()}
        />,
      );

      pressHandle("bookmarks");
      act(() => vi.advanceTimersByTime(300));
      fireEvent.pointerUp(window, { pointerId: 1 });

      fireEvent.click(screen.getByTestId("more-tool-card-ssh"));
      expect(onSelectTool).toHaveBeenCalledWith("ssh");
      vi.useRealTimers();
    });
  });
});
