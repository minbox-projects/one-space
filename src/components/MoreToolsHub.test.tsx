import { act, fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MoreToolsHub } from "@/components/MoreToolsHub";
import {
  MORE_TOOLS_ORDER_KEY,
  writeSavedOrder,
} from "@/lib/launcherToolOrder";
import {
  LAUNCHER_TOOL_VISIBILITY_KEY,
  LAUNCHER_TOOL_VISIBILITY_UPDATED_EVENT,
} from "@/lib/launcherToolVisibility";
import { renderWithProviders } from "@/test/mocks/render";

vi.mock("./Bookmarks", () => ({
  Bookmarks: () => <div>Bookmarks detail</div>,
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
vi.mock("./AiGateway", () => ({
  AiGateway: () => <div>AI Gateway detail</div>,
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
  FileSharingTool: ({ isVisible }: { isVisible?: boolean }) => (
    <div>
      File Sharing detail
      <span data-testid="file-sharing-is-visible">{String(isVisible)}</span>
    </div>
  ),
}));
vi.mock("./JttDataParserTool", () => ({
  JttDataParserTool: ({ initialTab }: { initialTab?: string }) => (
    <div>
      JT/T Data Parser detail subtab={initialTab ?? "none"}
    </div>
  ),
}));
vi.mock("./AiWorkflowModelSwitcher", () => ({
  AiWorkflowModelSwitcher: () => <div>AI Workflow Model Switcher detail</div>,
}));


type RegistryToolDescriptor = {
  id: string;
  surfaces: readonly string[];
};

type RegistryModule = {
  TOOLBOX_TOOLS: readonly RegistryToolDescriptor[];
  listToolboxTools: (surface: string) => RegistryToolDescriptor[];
  getToolboxTool: (id: string) => RegistryToolDescriptor | undefined;
};

/**
 * Loaded lazily so the task-001 guards in this suite still run while the
 * registry module is unavailable.
 */
async function loadRegistry(): Promise<RegistryModule> {
  const specifier = ["@/toolbox", "registry"].join("/");
  return (await import(/* @vite-ignore */ specifier)) as RegistryModule;
}

/** Hub ids straight from the registry so no parallel hand-maintained list exists. */
async function registryHubIds(): Promise<string[]> {
  const registry = await loadRegistry();
  return registry.listToolboxTools("hub").map((tool) => tool.id);
}

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

  it("显示 AI Workflow 模型切换卡片并分发详情组件", async () => {
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
      screen.getByRole("button", {
        name: /AI Workflow 模型切换|AI Workflow Model Switcher/,
      }),
    );
    expect(onSelectTool).toHaveBeenCalledWith("ai-workflow-model-switcher");

    rerender(
      <MoreToolsHub
        activeTool={"ai-workflow-model-switcher" as any}
        onSelectTool={onSelectTool}
        onBack={onBack}
      />,
    );
    expect(
      screen.getByText("AI Workflow Model Switcher detail"),
    ).toBeInTheDocument();
  });


  it("忽略遗留 md5Encryption 记录并在网格中保留 MD5 卡片", () => {
    localStorage.setItem(
      LAUNCHER_TOOL_VISIBILITY_KEY,
      JSON.stringify({ md5Encryption: false }),
    );
    const { rerender } = renderWithProviders(
      <MoreToolsHub activeTool={null} onSelectTool={vi.fn()} onBack={vi.fn()} />,
    );

    expect(
      screen.getByTestId("more-tool-card-md5-encryption"),
    ).toBeInTheDocument();

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
    ).toHaveAttribute("aria-checked", "true");
  });

  it("在可见性更新事件后无需重挂载即刷新 MD5 卡片", () => {
    renderWithProviders(
      <MoreToolsHub activeTool={null} onSelectTool={vi.fn()} onBack={vi.fn()} />,
    );
    expect(
      screen.getByTestId("more-tool-card-md5-encryption"),
    ).toBeInTheDocument();

    localStorage.setItem(
      LAUNCHER_TOOL_VISIBILITY_KEY,
      JSON.stringify({ "md5-encryption": false }),
    );
    act(() => {
      window.dispatchEvent(new Event(LAUNCHER_TOOL_VISIBILITY_UPDATED_EVENT));
    });

    expect(
      screen.queryByTestId("more-tool-card-md5-encryption"),
    ).not.toBeInTheDocument();
  });

  it("Hub 隐藏时将 isVisible=false 传给当前活动工具", () => {
    renderWithProviders(
      <MoreToolsHub
        activeTool="file-sharing"
        onSelectTool={vi.fn()}
        onBack={vi.fn()}
        {...({ isVisible: false } as Record<string, unknown>)}
      />,
    );

    expect(screen.getByTestId("file-sharing-is-visible")).toHaveTextContent(
      "false",
    );
  });

  it.each([
    "bookmarks",
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
    const registry = await loadRegistry();
    const md5Tool = registry.getToolboxTool("md5-encryption");
    expect(md5Tool).toBeDefined();
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
    ).toMatchObject({ [md5Tool!.id]: false });
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

  it("不再把 AI 网关作为更多工具卡片展示", () => {
    renderWithProviders(
      <MoreToolsHub activeTool={null} onSelectTool={vi.fn()} onBack={vi.fn()} />,
    );

    expect(
      screen.queryByTestId("more-tool-card-ai-gateway"),
    ).not.toBeInTheDocument();
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

    it("拖拽时突出边框作用在被选中卡片而非落点卡片", () => {
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

      expect(screen.getByTestId("more-tool-card-bookmarks")).toHaveClass(
        "ring-primary",
      );

      fireEvent.pointerOver(screen.getByTestId("more-tool-card-ssh"), {
        pointerId: 1,
      });
      expect(screen.getByTestId("more-tool-card-ssh")).not.toHaveClass(
        "ring-primary",
      );
      expect(screen.getByTestId("more-tool-card-bookmarks")).toHaveClass(
        "ring-primary",
      );

      fireEvent.pointerUp(window, { pointerId: 1 });
      vi.useRealTimers();
    });

    it("长按拖拽把手拖拽卡片实时移动位置并持久化，且不显示提示框或完成按钮", async () => {
      const hubIds = await registryHubIds();
      const fromIndex = hubIds.indexOf("bookmarks");
      const toIndex = hubIds.indexOf("ssh");
      const expectedOrder = [...hubIds];
      const [moved] = expectedOrder.splice(fromIndex, 1);
      expectedOrder.splice(toIndex, 0, moved);

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

      expect(cardOrder()).toEqual(expectedOrder);
      expect(
        JSON.parse(localStorage.getItem(MORE_TOOLS_ORDER_KEY) || "[]"),
      ).toEqual(expectedOrder);

      fireEvent.pointerUp(window, { pointerId: 1 });
      vi.useRealTimers();
    });

    it("渲染时应用已保存的卡片顺序", async () => {
      const hubIds = await registryHubIds();
      const third = hubIds.find((id) => id !== "ssh" && id !== "bookmarks");
      expect(third).toBeDefined();
      writeSavedOrder(MORE_TOOLS_ORDER_KEY, ["ssh", "bookmarks", third!]);
      renderWithProviders(
        <MoreToolsHub
          activeTool={null}
          onSelectTool={vi.fn()}
          onBack={vi.fn()}
        />,
      );

      expect(cardOrder().slice(0, 3)).toEqual(["ssh", "bookmarks", third]);
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

  describe("注册表驱动的 hub 面板", () => {
    it("为注册表 hub 面板的每个工具渲染卡片", async () => {
      const registry = await loadRegistry();
      const hubTools = registry.listToolboxTools("hub");
      expect(hubTools.length).toBeGreaterThan(0);

      renderWithProviders(
        <MoreToolsHub
          activeTool={null}
          onSelectTool={vi.fn()}
          onBack={vi.fn()}
        />,
      );

      for (const tool of hubTools) {
        expect(
          screen.getByTestId(`more-tool-card-${tool.id}`),
        ).toBeInTheDocument();
      }
    });

    it("不把非 hub 面板的注册表工具渲染为卡片", async () => {
      const registry = await loadRegistry();
      const nonHubTools = registry.TOOLBOX_TOOLS.filter(
        (tool) => !tool.surfaces.includes("hub"),
      );
      expect(nonHubTools.length).toBeGreaterThan(0);

      renderWithProviders(
        <MoreToolsHub
          activeTool={null}
          onSelectTool={vi.fn()}
          onBack={vi.fn()}
        />,
      );

      for (const tool of nonHubTools) {
        expect(
          screen.queryByTestId(`more-tool-card-${tool.id}`),
        ).not.toBeInTheDocument();
      }
    });

    it("选择任一 hub 卡片时通知其注册表稳定 id", async () => {
      const user = userEvent.setup();
      const registry = await loadRegistry();
      const onSelectTool = vi.fn();

      renderWithProviders(
        <MoreToolsHub
          activeTool={null}
          onSelectTool={onSelectTool}
          onBack={vi.fn()}
        />,
      );

      for (const tool of registry.listToolboxTools("hub")) {
        onSelectTool.mockClear();
        await user.click(screen.getByTestId(`more-tool-card-${tool.id}`));
        expect(onSelectTool).toHaveBeenCalledWith(tool.id);
      }
    });
  });
});
